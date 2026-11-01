//! One link session: connect, authenticate, keep the gserver's world state,
//! and (from milestone 2) run server-side NPC scripts on a 0.1 s tick.

use std::collections::BTreeMap;
use std::io;
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use seihai_core::protocol::parse_player_props;

use crate::link::{self, show, Message, LINK_VERSION};
use crate::scripts::{Produced, Scripts};
pub use crate::world::{Player, World};

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
///
/// `scripts`, when set, is started on the first `LEVEL_LIST` (the NPC
/// database is read then, if the host was built with [`Scripts::with_world`])
/// and ticked once per loop. Pass `None` to only log what the gserver sends.
pub fn run(
    addr: &str,
    token: &[u8],
    timing: Timing,
    scripts: Option<&mut Scripts>,
    log: Log,
) -> (End, World) {
    let mut world = World::default();
    let stream = match TcpStream::connect(addr) {
        Ok(s) => s,
        Err(e) => return (End::Closed(format!("connect to {addr}: {e}")), world),
    };
    let _ = stream.set_nodelay(true);
    log(format!("connected to {addr}"));
    let end = match session(stream, token, timing, &mut world, scripts, log) {
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
    mut scripts: Option<&mut Scripts>,
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
                if let Some(end) = handle(
                    m,
                    world,
                    &mut out,
                    &mut seen_unknown,
                    scripts.as_deref_mut(),
                    log,
                ) {
                    let _ = stream.shutdown(Shutdown::Both);
                    return Ok(end);
                }
            }
            if Instant::now() >= tick_end {
                break;
            }
        }
        // 2. Advance scripts by one configured tick and send what they asked for.
        if let Some(s) = scripts.as_deref_mut().filter(|s| s.is_started()) {
            let ms = u64::try_from(timing.tick.as_millis()).unwrap_or(u64::MAX);
            let produced = s.tick(ms);
            log_produced(&produced, log);
            log_script_lines(s, log);
            out.extend(produced.messages);
        }
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
    mut scripts: Option<&mut Scripts>,
    log: Log,
) -> Option<End> {
    match m {
        Message::LevelList { names } => {
            log(format!("level list: {} levels", names.len()));
            for n in &names {
                log(format!("  level {}", show(n)));
            }
            world.levels = names;
            // Once per session. A later LEVEL_LIST replaces the names above
            // but does not reload scripts; `created` has already run.
            if let Some(s) = scripts.filter(|s| !s.is_started()) {
                out.extend(start_scripts(s, world, log));
            }
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
            if let Some(player) = world.players.get(&player_id).cloned() {
                player_event(&mut scripts, player_id, &player, "playerenters", out, log);
            }
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
                player_event(&mut scripts, player_id, &p, "playerleaves", out, log);
            }
        }
        Message::PlayerLevel { player_id, level } => {
            log(format!("player {player_id} entered \"{}\"", show(&level)));
            if let Some(mut arrived) = world.players.get(&player_id).cloned() {
                let old = std::mem::replace(&mut arrived.level, level.clone());
                if let Some(slot) = world.players.get_mut(&player_id) {
                    slot.level = level;
                }
                if old != arrived.level {
                    let mut leaving = arrived.clone();
                    leaving.level = old;
                    player_event(&mut scripts, player_id, &leaving, "playerleaves", out, log);
                    player_event(&mut scripts, player_id, &arrived, "playerenters", out, log);
                }
            }
        }
        Message::PmToNpc { .. } => note_once(
            seen_unknown,
            link::id::PM_TO_NPC,
            log,
            "PM_TO_NPC is not run yet",
        ),
        Message::ServerFlags { .. } => note_once(
            seen_unknown,
            link::id::SERVER_FLAGS,
            log,
            "SERVER_FLAGS is not applied yet",
        ),
        Message::TriggerAction { .. } => note_once(
            seen_unknown,
            link::id::TRIGGER_ACTION,
            log,
            "TRIGGER_ACTION is not run yet",
        ),
        Message::NpcAdded { .. } => note_once(
            seen_unknown,
            link::id::NPC_ADDED,
            log,
            "NPC_ADDED from the gserver is ignored",
        ),
        Message::NpcProps { .. } => note_once(
            seen_unknown,
            link::id::NPC_PROPS,
            log,
            "NPC_PROPS from the gserver is ignored",
        ),
        Message::NpcDeleted { .. } => note_once(
            seen_unknown,
            link::id::NPC_DELETED,
            log,
            "NPC_DELETED from the gserver is ignored",
        ),
        Message::SetPlayerProps { .. } => note_once(
            seen_unknown,
            link::id::SET_PLAYER_PROPS,
            log,
            "SET_PLAYER_PROPS from the gserver is ignored",
        ),
        Message::WarpPlayer { .. } => note_once(
            seen_unknown,
            link::id::WARP_PLAYER,
            log,
            "WARP_PLAYER from the gserver is ignored",
        ),
        Message::AddWeapon { .. } => note_once(
            seen_unknown,
            link::id::ADD_WEAPON,
            log,
            "ADD_WEAPON from the gserver is ignored",
        ),
        Message::ServerFlag { .. } => note_once(
            seen_unknown,
            link::id::SERVER_FLAG,
            log,
            "SERVER_FLAG from the gserver is ignored",
        ),
        Message::Pm { .. } => note_once(
            seen_unknown,
            link::id::PM,
            log,
            "PM from the gserver is ignored",
        ),
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

/// Load the NPC database (if this host has a world folder), run `created`,
/// and log script lines. Called once, from the first `LEVEL_LIST`.
/// Returns `NPC_ADDED` and whatever `created` asked to send.
fn start_scripts(scripts: &mut Scripts, world: &World, log: Log) -> Vec<Message> {
    let (count, problems) = match scripts.open_database() {
        Ok(v) => v,
        Err(e) => {
            log(format!("NPC database: {e}"));
            return Vec::new();
        }
    };
    log(format!("NPC database: {count} server-side NPCs"));
    for p in problems {
        log(format!("  skipped {p}"));
    }
    let missing: Vec<Vec<u8>> = scripts
        .levels()
        .filter(|level| !world.levels.iter().any(|name| name == *level))
        .map(|level| (*level).to_vec())
        .collect();
    for level in missing {
        log(format!(
            "NPC level {} is not in the gserver level list",
            show(&level)
        ));
    }
    log_script_lines(scripts, log);
    let produced = scripts.start();
    log_produced(&produced, log);
    log_script_lines(scripts, log);
    produced.messages
}

fn player_event(
    scripts: &mut Option<&mut Scripts>,
    player_id: u16,
    player: &Player,
    event: &str,
    out: &mut Vec<Message>,
    log: Log,
) {
    let Some(scripts) = scripts.as_mut() else {
        return;
    };
    if !scripts.is_started() {
        return;
    }
    let produced = scripts.on_player(player_id, player, event);
    log_produced(&produced, log);
    log_script_lines(scripts, log);
    out.extend(produced.messages);
}

fn log_produced(produced: &Produced, log: Log) {
    for line in &produced.notes {
        log(line.clone());
    }
}

fn note_once(seen: &mut [bool; 256], id: u8, log: Log, line: &str) {
    if !seen[id as usize] {
        seen[id as usize] = true;
        log(line.to_string());
    }
}

fn log_script_lines(scripts: &mut Scripts, log: Log) {
    for (level, line) in scripts.take_log() {
        log(format!("script {}: {line}", show(&level)));
    }
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
