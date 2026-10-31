//! One link session: connect, authenticate, then keep the gserver's world
//! state and log it, on a 0.1 s tick (milestone 1).

use std::collections::BTreeMap;
use std::io;
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use seihai_core::protocol::parse_player_props;

use crate::link::{self, show, Message, LINK_VERSION};

/// Timings. The defaults are the ones in `PROTOCOL.md`; tests shorten them.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// Main loop tick.
    pub tick: Duration,
    /// How long to wait for `WELCOME` or `REJECT`.
    pub handshake: Duration,
    /// Send `PING` after sending nothing for this long.
    pub ping_after: Duration,
    /// Close after hearing nothing for this long.
    pub dead_after: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            tick: Duration::from_millis(100),
            handshake: Duration::from_secs(10),
            ping_after: Duration::from_secs(30),
            dead_after: Duration::from_secs(90),
        }
    }
}

/// What the NPC-server knows about one player.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Player {
    pub account: Vec<u8>,
    pub level: Vec<u8>,
    /// Latest raw value of each prop id.
    pub props: BTreeMap<u8, Vec<u8>>,
}

/// The gserver's world as the link has described it so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct World {
    pub server_name: Vec<u8>,
    pub levels: Vec<Vec<u8>>,
    pub players: BTreeMap<u16, Player>,
}

/// How a session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum End {
    /// The gserver refused us. Retrying with the same token will not help.
    Rejected(String),
    /// The link closed or failed after `WELCOME`; a reconnect may help.
    Closed(String),
}

/// A log sink. `main` prints lines with a time stamp; tests collect them.
pub type Log<'a> = &'a mut dyn FnMut(String);

/// Connect to `addr`, authenticate with `token`, then run until the link
/// closes. Returns how it ended and the last known world.
pub fn run(addr: &str, token: &[u8], timing: Timing, log: Log) -> (End, World) {
    let mut world = World::default();
    let stream = match TcpStream::connect(addr) {
        Ok(s) => s,
        Err(e) => return (End::Closed(format!("connect to {addr}: {e}")), world),
    };
    let _ = stream.set_nodelay(true);
    log(format!("connected to {addr}"));
    let end = match session(stream, token, timing, &mut world, log) {
        Ok(end) => end,
        Err(e) => End::Closed(e.to_string()),
    };
    (end, world)
}

fn session(
    mut stream: TcpStream,
    token: &[u8],
    timing: Timing,
    world: &mut World,
    log: Log,
) -> io::Result<End> {
    let program = format!("seihai-npcserver {}", env!("CARGO_PKG_VERSION"));
    link::write_frame(
        &mut stream,
        &[Message::Hello {
            link_version: LINK_VERSION,
            token: token.to_vec(),
            program: program.into_bytes(),
        }],
    )?;

    // Handshake: the first message must be WELCOME or REJECT.
    stream.set_read_timeout(Some(timing.handshake))?;
    let first = link::read_frame(&mut stream).map_err(|e| {
        if matches!(
            e.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        ) {
            io::Error::new(e.kind(), "no WELCOME from the gserver in time")
        } else {
            e
        }
    })?;
    let mut msgs = first.into_iter();
    match msgs.next() {
        Some(Message::Welcome {
            link_version,
            server_name,
            program,
        }) => {
            if link_version > LINK_VERSION || link_version == 0 {
                return Ok(End::Rejected(format!(
                    "gserver offered link version {link_version}, we speak {LINK_VERSION}"
                )));
            }
            log(format!(
                "authenticated: server \"{}\" ({}), link version {link_version}",
                show(&server_name),
                show(&program)
            ));
            world.server_name = server_name;
        }
        Some(Message::Reject { reason }) => {
            let reason = String::from_utf8_lossy(&reason).into_owned();
            log(format!("gserver rejected us: {reason}"));
            return Ok(End::Rejected(reason));
        }
        Some(other) => {
            return Ok(End::Rejected(format!(
                "expected WELCOME, got message 0x{:02x}",
                other.id()
            )))
        }
        None => return Ok(End::Rejected("empty first frame".into())),
    }
    stream.set_read_timeout(None)?;

    // A reader thread hands frames to the tick loop.
    let (tx, rx) = mpsc::channel::<io::Result<Vec<Message>>>();
    let mut reader = stream.try_clone()?;
    std::thread::spawn(move || loop {
        let r = link::read_frame(&mut reader);
        let failed = r.is_err();
        if tx.send(r).is_err() || failed {
            return;
        }
    });

    let rest: Vec<Message> = msgs.collect();
    let mut seen_unknown = [false; 256];
    let mut last_recv = Instant::now();
    let mut last_sent = Instant::now();
    let mut nonce: u32 = 0;
    let mut out: Vec<Message> = Vec::new();

    let mut pending = Some(rest);
    loop {
        let tick_end = Instant::now() + timing.tick;
        // 1. Read everything that has arrived this tick.
        loop {
            let batch = match pending.take() {
                Some(b) => Ok(b),
                None => match rx.recv_timeout(tick_end.saturating_duration_since(Instant::now())) {
                    Ok(r) => {
                        last_recv = Instant::now();
                        r
                    }
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => {
                        return Ok(End::Closed("reader stopped".into()))
                    }
                },
            };
            let batch = match batch {
                Ok(b) => b,
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    return Ok(End::Closed("gserver closed the link".into()))
                }
                Err(e) => return Ok(End::Closed(format!("link error: {e}"))),
            };
            for m in batch {
                if let Some(end) = handle(m, world, &mut out, &mut seen_unknown, log) {
                    let _ = stream.shutdown(Shutdown::Both);
                    return Ok(end);
                }
            }
            if Instant::now() >= tick_end {
                break;
            }
        }
        // 2. Scripts run here from milestone 2.
        // 3. Keep-alive.
        if last_recv.elapsed() >= timing.dead_after {
            let _ = stream.shutdown(Shutdown::Both);
            return Ok(End::Closed(format!(
                "nothing from the gserver for {:?}",
                timing.dead_after
            )));
        }
        if out.is_empty() && last_sent.elapsed() >= timing.ping_after {
            nonce = nonce.wrapping_add(1);
            out.push(Message::Ping { nonce });
        }
        // 4. Send what this tick produced.
        if !out.is_empty() {
            link::write_frame(&mut stream, &out)?;
            out.clear();
            last_sent = Instant::now();
        }
    }
}

/// Apply one message. Returns `Some` when the session must end.
fn handle(
    m: Message,
    world: &mut World,
    out: &mut Vec<Message>,
    seen_unknown: &mut [bool; 256],
    log: Log,
) -> Option<End> {
    match m {
        Message::LevelList { names } => {
            log(format!("level list: {} levels", names.len()));
            for n in &names {
                log(format!("  level {}", show(n)));
            }
            world.levels = names;
        }
        Message::LevelReloaded { name } => log(format!("level reloaded: {}", show(&name))),
        Message::PlayerJoined {
            player_id,
            account,
            level,
            props,
        } => {
            let mut p = Player {
                account,
                level,
                props: BTreeMap::new(),
            };
            let n = merge_props(&mut p, &props);
            log(format!(
                "player {player_id} joined: account \"{}\" in \"{}\", {n} props: {}",
                show(&p.account),
                show(&p.level),
                prop_summary(&props)
            ));
            world.players.insert(player_id, p);
        }
        Message::PlayerProps { player_id, props } => match world.players.get_mut(&player_id) {
            Some(p) => {
                merge_props(p, &props);
                log(format!(
                    "player {player_id} props: {}",
                    prop_summary(&props)
                ));
            }
            None => log(format!("props for unknown player {player_id}, ignored")),
        },
        Message::PlayerLeft { player_id } => {
            if let Some(p) = world.players.remove(&player_id) {
                log(format!("player {player_id} left: \"{}\"", show(&p.account)));
            }
        }
        Message::PlayerLevel { player_id, level } => {
            log(format!("player {player_id} entered \"{}\"", show(&level)));
            if let Some(p) = world.players.get_mut(&player_id) {
                p.level = level;
            }
        }
        Message::Ping { nonce } => out.push(Message::Pong { nonce }),
        Message::Pong { .. } => {}
        Message::Bye { reason } => {
            let reason = String::from_utf8_lossy(&reason).into_owned();
            log(format!("gserver said bye: {reason}"));
            return Some(End::Closed(format!("bye: {reason}")));
        }
        Message::Reject { reason } => {
            return Some(End::Rejected(String::from_utf8_lossy(&reason).into_owned()))
        }
        Message::Hello { .. } | Message::Welcome { .. } => {
            log("unexpected handshake message after WELCOME, ignored".into());
        }
        Message::Unknown { id, data } => {
            if !seen_unknown[id as usize] {
                seen_unknown[id as usize] = true;
                log(format!(
                    "unknown message 0x{id:02x} ({} bytes), skipping this id from now on",
                    data.len()
                ));
            }
        }
    }
    None
}

/// Store each prop; returns how many were read.
fn merge_props(p: &mut Player, stream: &[u8]) -> usize {
    let props = parse_player_props(stream);
    let n = props.len();
    for pr in props {
        p.props.insert(pr.id, pr.raw);
    }
    n
}

/// `id=value` pairs for the log; values as escaped bytes.
fn prop_summary(stream: &[u8]) -> String {
    let props = parse_player_props(stream);
    let used: usize = props.iter().map(|p| 1 + p.raw.len()).sum();
    let mut s: Vec<String> = props
        .iter()
        .map(|p| format!("{}={}", p.id, show(&p.raw)))
        .collect();
    if used < stream.len() {
        s.push(format!("(+{} bytes not read)", stream.len() - used));
    }
    if s.is_empty() {
        "none".into()
    } else {
        s.join(" ")
    }
}
