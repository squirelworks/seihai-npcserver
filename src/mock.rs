//! A stand-in gserver for tests and loopback lab runs.
//!
//! It speaks the gserver side of `PROTOCOL.md` with canned world state. It is
//! not the real gserver and only ever binds to `127.0.0.1`. Frames the
//! NPC-server sends back, including `NPC_PROPS`, are kept on [`Seen`].

use std::io;
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use crate::link::{self, tokens_match, Message, LINK_VERSION};

/// A player online when the NPC-server joins: id, account, level, props.
pub type OnlinePlayer = (u16, Vec<u8>, Vec<u8>, Vec<u8>);

/// The canned world a mock gserver describes.
#[derive(Debug, Clone)]
pub struct Script {
    pub token: Vec<u8>,
    pub server_name: Vec<u8>,
    pub levels: Vec<Vec<u8>>,
    /// `(player_id, account, level, props)` online when the NPC-server joins.
    pub players: Vec<OnlinePlayer>,
    /// Sent after the initial state, one frame each, `gap` apart.
    pub updates: Vec<Message>,
    pub gap: Duration,
    /// Close after the updates (with `BYE` when `Some`).
    pub bye: Option<Vec<u8>>,
}

impl Script {
    /// A small lab world: two levels, one player, a few updates.
    pub fn lab(token: &[u8]) -> Self {
        Self {
            token: token.to_vec(),
            server_name: b"Lab Mock".to_vec(),
            levels: vec![b"onlinestartlocal.nw".to_vec(), b"gmaps/house1.nw".to_vec()],
            players: vec![(
                2,
                b"test205".to_vec(),
                b"onlinestartlocal.nw".to_vec(),
                sample_props(b"test205", 30, 30),
            )],
            updates: vec![
                Message::PlayerProps {
                    player_id: 2,
                    props: pos_props(31, 30),
                },
                Message::PlayerJoined {
                    player_id: 3,
                    account: b"test222".to_vec(),
                    level: b"onlinestartlocal.nw".to_vec(),
                    props: sample_props(b"test222", 20, 40),
                },
                Message::PlayerLevel {
                    player_id: 2,
                    level: b"gmaps/house1.nw".to_vec(),
                },
                Message::LevelReloaded {
                    name: b"onlinestartlocal.nw".to_vec(),
                },
                Message::PlayerLeft { player_id: 3 },
            ],
            gap: Duration::from_millis(50),
            bye: Some(b"lab script done".to_vec()),
        }
    }
}

/// Player props: nickname (0), X (15), Y (16).
pub fn sample_props(nick: &[u8], x: u8, y: u8) -> Vec<u8> {
    let mut p = vec![0x20, 0x20 + nick.len() as u8];
    p.extend_from_slice(nick);
    p.extend(pos_props(x, y));
    p
}

/// X (15) and Y (16) in the classic half-tile form.
pub fn pos_props(x: u8, y: u8) -> Vec<u8> {
    vec![0x20 + 15, 0x20 + x * 2, 0x20 + 16, 0x20 + y * 2]
}

/// What the mock saw from the NPC-server.
#[derive(Debug, Clone, Default)]
pub struct Seen {
    pub hello: Option<Message>,
    pub other: Vec<Message>,
}

/// Bind a mock gserver on `127.0.0.1` (port 0 picks a free one).
pub fn bind(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", port))
}

/// Serve one NPC-server connection with `script`.
pub fn serve_one(listener: &TcpListener, script: &Script) -> io::Result<Seen> {
    let (stream, _) = listener.accept()?;
    serve(stream, script)
}

/// Serve an accepted connection.
pub fn serve(mut s: TcpStream, script: &Script) -> io::Result<Seen> {
    let mut seen = Seen::default();
    s.set_read_timeout(Some(Duration::from_secs(10)))?;
    let hello = link::read_frame(&mut s)?.into_iter().next();
    let ok = match &hello {
        Some(Message::Hello {
            link_version,
            token,
            ..
        }) => {
            if *link_version < 1 {
                Err(format!("link version {link_version} not supported"))
            } else if !tokens_match(token, &script.token) {
                Err("bad token".to_string())
            } else {
                Ok(())
            }
        }
        _ => Err("expected HELLO".to_string()),
    };
    seen.hello = hello;
    if let Err(reason) = ok {
        link::write_frame(
            &mut s,
            &[Message::Reject {
                reason: reason.into_bytes(),
            }],
        )?;
        return Ok(seen);
    }
    let mut first = vec![
        Message::Welcome {
            link_version: LINK_VERSION,
            server_name: script.server_name.clone(),
            program: b"seihai-npcserver mock gserver".to_vec(),
        },
        Message::LevelList {
            names: script.levels.clone(),
        },
    ];
    for (player_id, account, level, props) in &script.players {
        first.push(Message::PlayerJoined {
            player_id: *player_id,
            account: account.clone(),
            level: level.clone(),
            props: props.clone(),
        });
    }
    link::write_frame(&mut s, &first)?;
    for (i, u) in script.updates.iter().enumerate() {
        std::thread::sleep(script.gap);
        link::write_frame(&mut s, &[u.clone(), Message::Ping { nonce: i as u32 }])?;
    }
    // Collect the PONGs (and anything else) for a moment.
    s.set_read_timeout(Some(script.gap.max(Duration::from_millis(300))))?;
    while let Ok(msgs) = link::read_frame(&mut s) {
        seen.other.extend(msgs);
        if seen
            .other
            .iter()
            .filter(|m| matches!(m, Message::Pong { .. }))
            .count()
            >= script.updates.len()
        {
            break;
        }
    }
    if let Some(reason) = &script.bye {
        link::write_frame(
            &mut s,
            &[Message::Bye {
                reason: reason.clone(),
            }],
        )?;
    }
    Ok(seen)
}
