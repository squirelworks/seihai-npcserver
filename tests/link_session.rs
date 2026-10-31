//! Loopback tests: the session against the mock gserver on 127.0.0.1.

use std::thread;
use std::time::Duration;

use seihai_npcserver::link::{self, Message};
use seihai_npcserver::mock::{self, Script};
use seihai_npcserver::session::{self, End, Timing};

const TOKEN: &[u8] = b"lab-token-0123456789";

fn fast() -> Timing {
    Timing {
        tick: Duration::from_millis(20),
        handshake: Duration::from_secs(5),
        ping_after: Duration::from_secs(30),
        dead_after: Duration::from_secs(30),
    }
}

fn run_against(
    script: Script,
    token: &[u8],
    timing: Timing,
) -> (End, session::World, Vec<String>, mock::Seen) {
    let listener = mock::bind(0).unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let server = thread::spawn(move || mock::serve_one(&listener, &script).unwrap());
    let mut lines = Vec::new();
    let (end, world) = session::run(&addr, token, timing, &mut |l| lines.push(l));
    (end, world, lines, server.join().unwrap())
}

#[test]
fn logs_levels_and_players() {
    let (end, world, lines, seen) = run_against(Script::lab(TOKEN), TOKEN, fast());
    assert_eq!(
        end,
        End::Closed("bye: lab script done".into()),
        "{lines:#?}"
    );

    match seen.hello {
        Some(Message::Hello {
            link_version,
            token,
            program,
        }) => {
            assert_eq!(link_version, 1);
            assert_eq!(token, TOKEN);
            assert!(program.starts_with(b"seihai-npcserver "));
        }
        other => panic!("no HELLO: {other:?}"),
    }
    // One PONG per PING, with the PING's nonce.
    let pongs: Vec<u32> = seen
        .other
        .iter()
        .filter_map(|m| {
            if let Message::Pong { nonce } = m {
                Some(*nonce)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(pongs, vec![0, 1, 2, 3, 4]);

    assert_eq!(world.server_name, b"Lab Mock");
    assert_eq!(
        world.levels,
        vec![b"onlinestartlocal.nw".to_vec(), b"gmaps/house1.nw".to_vec()]
    );
    // Player 3 joined and left; player 2 moved and changed level.
    assert_eq!(world.players.len(), 1);
    let p = &world.players[&2];
    assert_eq!(p.account, b"test205");
    assert_eq!(p.level, b"gmaps/house1.nw");
    assert_eq!(p.props[&0], b"\x27test205");
    assert_eq!(p.props[&15], vec![0x20 + 62]);

    let text = lines.join("\n");
    for want in [
        "authenticated: server \"Lab Mock\"",
        "level list: 2 levels",
        "player 2 joined: account \"test205\" in \"onlinestartlocal.nw\", 3 props",
        "player 2 props: 15=",
        "player 3 joined",
        "player 2 entered \"gmaps/house1.nw\"",
        "level reloaded: onlinestartlocal.nw",
        "player 3 left: \"test222\"",
        "gserver said bye: lab script done",
    ] {
        assert!(text.contains(want), "missing {want:?} in:\n{text}");
    }
    assert!(!text.contains("lab-token"), "the token must not be logged");
}

#[test]
fn bad_token_is_rejected() {
    let (end, world, lines, _) = run_against(Script::lab(TOKEN), b"wrong-token-0123456789", fast());
    assert_eq!(end, End::Rejected("bad token".into()), "{lines:#?}");
    assert!(world.levels.is_empty());
}

#[test]
fn sends_ping_when_idle_and_closes_when_dead() {
    // A gserver that welcomes and then goes quiet.
    let listener = mock::bind(0).unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let _hello = link::read_frame(&mut s).unwrap();
        link::write_frame(
            &mut s,
            &[Message::Welcome {
                link_version: 1,
                server_name: b"Quiet".to_vec(),
                program: b"test".to_vec(),
            }],
        )
        .unwrap();
        // Stay silent: the NPC-server PINGs every ping_after, then gives up
        // and closes after dead_after.
        let mut msgs = Vec::new();
        while let Ok(m) = link::read_frame(&mut s) {
            msgs.extend(m);
        }
        (msgs, true)
    });
    let timing = Timing {
        tick: Duration::from_millis(10),
        handshake: Duration::from_secs(5),
        ping_after: Duration::from_millis(100),
        dead_after: Duration::from_millis(400),
    };
    let mut lines = Vec::new();
    let (end, _) = session::run(&addr, TOKEN, timing, &mut |l| lines.push(l));
    let (msgs, closed) = server.join().unwrap();
    assert!(matches!(msgs[0], Message::Ping { .. }), "{msgs:?}");
    assert!(
        matches!(end, End::Closed(ref r) if r.starts_with("nothing from the gserver")),
        "{end:?}"
    );
    assert!(closed);
}

#[test]
fn unknown_messages_are_skipped() {
    let mut script = Script::lab(TOKEN);
    script.updates.insert(
        0,
        Message::Unknown {
            id: 0x45,
            data: vec![9; 10],
        },
    );
    script.updates.insert(
        1,
        Message::Unknown {
            id: 0x45,
            data: vec![],
        },
    );
    let (end, world, lines, _) = run_against(script, TOKEN, fast());
    assert_eq!(end, End::Closed("bye: lab script done".into()));
    assert_eq!(world.players.len(), 1);
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.contains("unknown message 0x45"))
            .count(),
        1
    );
}

#[test]
fn nobody_listening() {
    // Bind and drop to find a port nobody is on.
    let port = mock::bind(0).unwrap().local_addr().unwrap().port();
    let (end, _) = session::run(&format!("127.0.0.1:{port}"), TOKEN, fast(), &mut |_| {});
    assert!(
        matches!(end, End::Closed(ref r) if r.starts_with("connect to")),
        "{end:?}"
    );
}
