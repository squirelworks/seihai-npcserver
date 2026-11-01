//! seihai-npcserver: connects to a seihai-gserver over the NPC-server link
//! (`PROTOCOL.md`) and, for now, logs what the gserver tells it.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use seihai_core::scripting::engine::Era;

use seihai_npcserver::link::{DEFAULT_ADDR, TOKEN_ENV};
use seihai_npcserver::scripts::Scripts;
use seihai_npcserver::session::{self, End, Timing};

const USAGE: &str = "\
usage: seihai-npcserver [--gserver HOST:PORT] [--token-file FILE] [--world DIR] [--once]

  --gserver HOST:PORT  the gserver's NPC-server link (default 127.0.0.1:14810)
  --token-file FILE    read the link token from FILE; otherwise it comes from
                       the SEIHAI_NPCSERVER_TOKEN environment variable, which
                       is how seihai-gserver passes it with `npcserver = auto`
  --world DIR          the world folder; server-side NPCs are read from DIR/npcs
  --once               exit when the link closes instead of reconnecting
  --version            print the version

The token is never accepted on the command line.";

struct Args {
    gserver: String,
    token_file: Option<PathBuf>,
    world: Option<PathBuf>,
    once: bool,
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut a = Args {
        gserver: DEFAULT_ADDR.to_string(),
        token_file: None,
        world: None,
        once: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("seihai-npcserver {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(None);
            }
            "--gserver" => a.gserver = it.next().ok_or("--gserver needs HOST:PORT")?,
            "--token-file" => a.token_file = Some(it.next().ok_or("--token-file needs a path")?.into()),
            "--world" => a.world = Some(it.next().ok_or("--world needs a path")?.into()),
            "--once" => a.once = true,
            "--token" => return Err("the token is not taken on the command line; use --token-file or SEIHAI_NPCSERVER_TOKEN".into()),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(Some(a))
}

fn load_token(a: &Args) -> Result<Vec<u8>, String> {
    let raw = match &a.token_file {
        Some(p) => std::fs::read(p).map_err(|e| format!("read {}: {e}", p.display()))?,
        None => std::env::var_os(TOKEN_ENV)
            .ok_or(format!("no token: set {TOKEN_ENV} or pass --token-file"))?
            .into_encoded_bytes(),
    };
    let token = raw.trim_ascii().to_vec();
    if !(16..=256).contains(&token.len()) {
        return Err(format!(
            "the token must be 16 to 256 bytes, it is {}",
            token.len()
        ));
    }
    Ok(token)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(Some(a)) => a,
        Ok(None) => return ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("seihai-npcserver: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let token = match load_token(&args) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("seihai-npcserver: {e}");
            return ExitCode::from(2);
        }
    };
    let start = Instant::now();
    let mut log = move |line: String| {
        let t = start.elapsed();
        eprintln!("[{:>4}.{:03}] {line}", t.as_secs(), t.subsec_millis());
    };
    log(format!(
        "seihai-npcserver {} starting, link {}",
        env!("CARGO_PKG_VERSION"),
        args.gserver
    ));
    if let Some(w) = &args.world {
        log(format!("world folder {}", w.display()));
    }
    let mut delay = Duration::from_secs(1);
    loop {
        let began = Instant::now();
        // A new host per connection: `created` runs again after the next
        // LEVEL_LIST. Server-side scripts from 2001 are 2.x GS1 (V21).
        // The database is read inside the session, after that list.
        let mut scripts = args
            .world
            .as_ref()
            .map(|w| Scripts::with_world(Era::V21, w));
        let (end, world) = session::run(
            &args.gserver,
            &token,
            Timing::default(),
            scripts.as_mut(),
            &mut log,
        );
        match end {
            End::Rejected(reason) => {
                log(format!("stopping: {reason}"));
                return ExitCode::from(1);
            }
            End::Closed(reason) => {
                log(format!(
                    "link closed: {reason} ({} levels, {} players known)",
                    world.levels.len(),
                    world.players.len()
                ));
                if args.once {
                    return ExitCode::SUCCESS;
                }
            }
        }
        if began.elapsed() > Duration::from_secs(60) {
            delay = Duration::from_secs(1);
        }
        log(format!("reconnecting in {}s", delay.as_secs()));
        std::thread::sleep(delay);
        delay = (delay * 2).min(Duration::from_secs(30));
    }
}
