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
    let (end, world) = session::run(&addr, token, timing, None, &mut |l| lines.push(l));
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
    let (end, _) = session::run(&addr, TOKEN, timing, None, &mut |l| lines.push(l));
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
            id: 0x48,
            data: vec![9; 10],
        },
    );
    script.updates.insert(
        1,
        Message::Unknown {
            id: 0x48,
            data: vec![],
        },
    );
    let (end, world, lines, _) = run_against(script, TOKEN, fast());
    assert_eq!(end, End::Closed("bye: lab script done".into()));
    assert_eq!(world.players.len(), 1);
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.contains("unknown message 0x48"))
            .count(),
        1
    );
}

#[test]
fn scripts_start_after_the_level_list() {
    let dir = std::env::temp_dir().join(format!("seihai-npcserver-scripts-{}", std::process::id()));
    let npcs = dir.join("npcs");
    std::fs::create_dir_all(&npcs).unwrap();
    std::fs::write(
        npcs.join("a.txt"),
        "\
LEVEL onlinestartlocal.nw
X 30
Y 30
IMAGE block.png
SCRIPT
if (created) {
  madeupthing();
  timeout = 0.05;
}
if (timeout) {
  timeouthook();
}
SCRIPTEND
",
    )
    .unwrap();
    std::fs::write(
        npcs.join("b.txt"),
        "\
LEVEL not-on-server.nw
X 1
Y 1
IMAGE a.png
SCRIPT
if (created) { otherhook(); }
SCRIPTEND
",
    )
    .unwrap();
    // One bad file is reported and skipped; it must not stop the others.
    std::fs::write(npcs.join("c.txt"), "COLOR red\n").unwrap();

    let listener = mock::bind(0).unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let server = thread::spawn(move || mock::serve_one(&listener, &Script::lab(TOKEN)).unwrap());
    let mut lines = Vec::new();
    let mut scripts = seihai_npcserver::scripts::Scripts::with_world(
        seihai_core::scripting::engine::Era::V21,
        &dir,
    );
    let timing = Timing {
        // 0.05 s is one engine tick (50 ms), so the first loop tick runs timeout.
        tick: Duration::from_millis(50),
        handshake: Duration::from_secs(5),
        ping_after: Duration::from_secs(30),
        dead_after: Duration::from_secs(30),
    };
    let (end, _) = session::run(&addr, TOKEN, timing, Some(&mut scripts), &mut |l| {
        lines.push(l)
    });
    let _ = server.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        end,
        End::Closed("bye: lab script done".into()),
        "{lines:#?}"
    );

    let text = lines.join("\n");
    let list_at = text.find("level list: 2 levels").expect(&text);
    let db_at = text.find("NPC database: 2 server-side NPCs").expect(&text);
    assert!(
        list_at < db_at,
        "database must load after the level list:\n{text}"
    );
    assert!(
        text.contains("skipped") && text.contains("unknown key"),
        "{text}"
    );
    assert!(
        text.contains("NPC level not-on-server.nw is not in the gserver level list"),
        "{text}"
    );
    let created = text
        .find("script onlinestartlocal.nw: object 0: call to unknown function `madeupthing`")
        .expect(&text);
    let timed = text
        .find("script onlinestartlocal.nw: object 0: call to unknown function `timeouthook`")
        .expect(&text);
    assert!(created < timed, "timeout runs on a later tick:\n{text}");
    assert!(
        text.contains("script not-on-server.nw: object 0: call to unknown function `otherhook`"),
        "{text}"
    );
    // `madeupthing` / `timeouthook` are not link messages. NPC_ADDED still
    // goes out for the two NPCs; the lab script's BYE ends the session.
}

#[test]
fn scripts_do_not_load_when_rejected() {
    let dir =
        std::env::temp_dir().join(format!("seihai-npcserver-rejected-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("npcs")).unwrap();
    std::fs::write(
        dir.join("npcs/a.txt"),
        "LEVEL onlinestartlocal.nw\nX 1\nY 1\nIMAGE a.png\nSCRIPT\nif (created) { madeupthing(); }\nSCRIPTEND\n",
    )
    .unwrap();
    let mut scripts = seihai_npcserver::scripts::Scripts::with_world(
        seihai_core::scripting::engine::Era::V21,
        &dir,
    );
    let (end, _, lines, _) = {
        let listener = mock::bind(0).unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let server =
            thread::spawn(move || mock::serve_one(&listener, &Script::lab(TOKEN)).unwrap());
        let mut lines = Vec::new();
        let (end, world) = session::run(
            &addr,
            b"wrong-token-0123456789",
            fast(),
            Some(&mut scripts),
            &mut |l| lines.push(l),
        );
        (end, world, lines, server.join().unwrap())
    };
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(end, End::Rejected("bad token".into()), "{lines:#?}");
    let text = lines.join("\n");
    assert!(!text.contains("NPC database"), "{text}");
    assert!(!text.contains("madeupthing"), "{text}");
    assert!(!scripts.is_started());
}

#[test]
fn timeout_setcharprop_arrives_as_npc_props() {
    let dir = std::env::temp_dir().join(format!("seihai-npcserver-props-{}", std::process::id()));
    let npcs = dir.join("npcs");
    std::fs::create_dir_all(&npcs).unwrap();
    std::fs::write(
        npcs.join("greet.txt"),
        "\
LEVEL onlinestartlocal.nw
X 30
Y 30
IMAGE block.png
SCRIPT
if (created) { timeout = 0.1; }
if (timeout) { setcharprop #c,hi; timeout = 1; }
SCRIPTEND
",
    )
    .unwrap();

    let listener = mock::bind(0).unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let hello = link::read_frame(&mut s).unwrap();
        link::write_frame(
            &mut s,
            &[
                Message::Welcome {
                    link_version: 1,
                    server_name: b"Lab Mock".to_vec(),
                    program: b"test".to_vec(),
                },
                Message::LevelList {
                    names: vec![b"onlinestartlocal.nw".to_vec()],
                },
            ],
        )
        .unwrap();
        s.set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_millis(800);
        let mut got = Vec::new();
        while std::time::Instant::now() < deadline {
            match link::read_frame(&mut s) {
                Ok(m) => got.extend(m),
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    continue;
                }
                Err(_) => break,
            }
        }
        let _ = link::write_frame(
            &mut s,
            &[Message::Bye {
                reason: b"props seen".to_vec(),
            }],
        );
        (hello, got)
    });

    let mut scripts = seihai_npcserver::scripts::Scripts::with_world(
        seihai_core::scripting::engine::Era::V21,
        &dir,
    );
    let timing = Timing {
        tick: Duration::from_millis(100),
        handshake: Duration::from_secs(5),
        ping_after: Duration::from_secs(30),
        dead_after: Duration::from_secs(30),
    };
    let mut lines = Vec::new();
    let (end, _) = session::run(&addr, TOKEN, timing, Some(&mut scripts), &mut |l| {
        lines.push(l)
    });
    let (hello, got) = server.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(matches!(hello[0], Message::Hello { .. }), "{hello:?}");
    assert!(
        matches!(end, End::Closed(ref r) if r.starts_with("bye:")),
        "{end:?} {lines:#?}"
    );

    assert!(
        got.iter().any(|m| matches!(
            m,
            Message::NpcAdded { npc_id: 1, level, .. } if level == b"onlinestartlocal.nw"
        )),
        "NPC_ADDED missing in {got:?}\n{lines:#?}"
    );
    let props = got.iter().find_map(|m| match m {
        Message::NpcProps { npc_id: 1, props } => Some(props.clone()),
        _ => None,
    });
    let Some(props) = props else {
        panic!("NPC_PROPS missing in {got:?}\n{lines:#?}");
    };
    let mut raw = vec![0x20, 0x20, 0x21];
    raw.extend_from_slice(&props);
    let (id, parsed) = seihai_core::protocol::npcprops::parse_npc_props(&raw, false).unwrap();
    assert_eq!(id, 1);
    assert_eq!(parsed.message.as_deref(), Some("hi"));
}

#[test]
fn nobody_listening() {
    // Bind and drop to find a port nobody is on.
    let port = mock::bind(0).unwrap().local_addr().unwrap().port();
    let (end, _) = session::run(
        &format!("127.0.0.1:{port}"),
        TOKEN,
        fast(),
        None,
        &mut |_| {},
    );
    assert!(
        matches!(end, End::Closed(ref r) if r.starts_with("connect to")),
        "{end:?}"
    );
}
