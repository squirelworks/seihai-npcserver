//! Milestone 3: the sample world in `examples/m3-world`. Its NPCs only do
//! anything when this server runs them: a greeter, a shop that takes
//! gralats, a gate that opens on a server flag and a trainer that gives a
//! weapon by script. These tests run the world's own `npcs/` folder.

use std::thread;
use std::time::Duration;

use seihai_core::scripting::engine::Era;
use seihai_npcserver::link::{self, Message};
use seihai_npcserver::mock;
use seihai_npcserver::scripts::{Produced, Scripts};
use seihai_npcserver::session::{self, End, Player, Timing};

const WORLD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/m3-world");
const SQUARE: &[u8] = b"m3_square.nw";
const HALL: &[u8] = b"m3_hall.nw";
const TOKEN: &[u8] = b"lab-token-0123456789";

fn gint3(n: u32) -> Vec<u8> {
    vec![
        ((n >> 14) & 0x7F) as u8 + 0x20,
        ((n >> 7) & 0x7F) as u8 + 0x20,
        (n & 0x7F) as u8 + 0x20,
    ]
}

/// Nick, position and gralats (prop 3, a GINT3).
fn props(nick: &[u8], rupees: u32) -> Vec<u8> {
    let mut p = mock::sample_props(nick, 30, 32);
    p.push(0x20 + 3);
    p.extend(gint3(rupees));
    p
}

fn player(level: &[u8], rupees: u32) -> Player {
    let mut p = Player {
        account: b"lab205".to_vec(),
        level: level.to_vec(),
        ..Player::default()
    };
    for pr in seihai_core::protocol::parse_player_props(&props(b"lab205", rupees)) {
        p.props.insert(pr.id, pr.raw);
    }
    p
}

fn started() -> Scripts {
    let mut s = Scripts::with_world(Era::V217, WORLD);
    let (count, problems) = s.open_database().unwrap();
    assert_eq!(count, 5, "{problems:?}");
    assert!(problems.is_empty(), "{problems:?}");
    s.start();
    s
}

fn npc_message(m: &Message) -> Option<String> {
    let Message::NpcProps { npc_id, props } = m else {
        return None;
    };
    let mut raw = gint3(*npc_id);
    raw.extend_from_slice(props);
    seihai_core::protocol::npcprops::parse_npc_props(&raw, false).and_then(|(_, p)| p.message)
}

fn messages(p: &Produced) -> Vec<String> {
    p.messages.iter().filter_map(npc_message).collect()
}

/// The props in every `SET_PLAYER_PROPS` for `id`, as `(prop, raw)`.
fn player_props(p: &Produced, id: u16) -> Vec<(u8, Vec<u8>)> {
    p.messages
        .iter()
        .filter_map(|m| match m {
            Message::SetPlayerProps { player_id, props } if *player_id == id => Some(props),
            _ => None,
        })
        .flat_map(|raw| seihai_core::protocol::parse_player_props(raw))
        .map(|q| (q.id, q.raw))
        .collect()
}

#[test]
fn the_world_loads_and_everyone_shows_up() {
    let mut s = Scripts::with_world(Era::V217, WORLD);
    s.open_database().unwrap();
    let started = s.start();
    let added = started
        .messages
        .iter()
        .filter(|m| matches!(m, Message::NpcAdded { .. }))
        .count();
    assert_eq!(added, 5, "{:?}", started.messages);
    let said = messages(&started);
    for line in [
        "Say \"hi\" to me!",
        "Say \"buy\": 10 bombs for 20 gralats.",
        "Say \"open\" and I open the gate.",
        "Welcome to the hall!",
    ] {
        assert!(said.iter().any(|m| m == line), "{line:?} not in {said:?}");
    }
}

#[test]
fn the_greeter_welcomes_and_tops_up_a_poor_player() {
    let mut s = started();
    let poor = s.on_player(2, &player(SQUARE, 5), "playerenters");
    assert!(
        messages(&poor)
            .iter()
            .any(|m| m == "Welcome, lab205! Here are 30 gralats for the shop."),
        "{:?}",
        poor.messages
    );
    assert!(
        player_props(&poor, 2).contains(&(3, gint3(35))),
        "{:?}",
        poor.messages
    );

    let rich = s.on_player(2, &player(SQUARE, 50), "playerenters");
    assert!(messages(&rich)
        .iter()
        .any(|m| m == "Welcome to the sample world, lab205!"));
    assert!(player_props(&rich, 2).is_empty(), "{:?}", rich.messages);

    let hi = s.on_chat(2, &player(SQUARE, 50), "hi");
    assert!(messages(&hi)
        .iter()
        .any(|m| m == "Hi lab205! The shop is west, the gate is east."));
}

#[test]
fn the_shop_takes_twenty_gralats_for_ten_bombs() {
    let mut s = started();
    let bought = s.on_chat(2, &player(SQUARE, 25), "buy");
    let sent = player_props(&bought, 2);
    assert!(sent.contains(&(3, gint3(5))), "{:?}", bought.messages);
    assert!(
        sent.contains(&(5, vec![0x20 + 10])),
        "{:?}",
        bought.messages
    );
    assert!(messages(&bought)
        .iter()
        .any(|m| m == "Thank you, lab205! 10 bombs for 20 gralats."));

    let short = s.on_chat(2, &player(SQUARE, 5), "buy");
    assert!(player_props(&short, 2).is_empty(), "{:?}", short.messages);
    assert!(messages(&short)
        .iter()
        .any(|m| m == "Sorry lab205, that is 20 gralats. You have 5."));

    // Other chat leaves the shop alone.
    let chat = s.on_chat(2, &player(SQUARE, 25), "nice shop");
    assert!(player_props(&chat, 2).is_empty(), "{:?}", chat.messages);
}

#[test]
fn the_gate_opens_on_the_server_flag_and_shuts_again() {
    let mut s = started();
    let gate_id = 1; // gate.txt is first in name order.
    let hidden = |p: &Produced| {
        p.messages.iter().any(|m| {
            let Message::NpcProps { npc_id, props } = m else {
                return false;
            };
            let mut raw = gint3(*npc_id);
            raw.extend_from_slice(props);
            *npc_id == gate_id
                && seihai_core::protocol::npcprops::parse_npc_props(&raw, false)
                    .and_then(|(_, p)| p.vis_flags)
                    .map(|v| v & 1 != 0)
                    == Some(false)
        })
    };
    assert!(!hidden(&s.tick(600)));

    let open = s.on_chat(2, &player(SQUARE, 0), "open");
    assert!(
        open.messages.iter().any(|m| matches!(
            m,
            Message::ServerFlag { name, set: true, .. } if name == b"server.gateopen"
        )),
        "{:?}",
        open.messages
    );
    assert!(hidden(&s.tick(600)), "the gate should hide on the flag");

    // Thirty seconds on, the gatekeeper takes the flag down and the gate
    // comes back.
    let mut later = Produced::default();
    for _ in 0..62 {
        let p = s.tick(500);
        later.messages.extend(p.messages);
    }
    assert!(
        later.messages.iter().any(|m| matches!(
            m,
            Message::ServerFlag { name, set: false, .. } if name == b"server.gateopen"
        )),
        "{:?}",
        later.messages
    );
    assert!(
        later.messages.iter().any(|m| {
            let Message::NpcProps { npc_id, props } = m else {
                return false;
            };
            let mut raw = gint3(*npc_id);
            raw.extend_from_slice(props);
            *npc_id == gate_id
                && seihai_core::protocol::npcprops::parse_npc_props(&raw, false)
                    .and_then(|(_, p)| p.vis_flags)
                    .map(|v| v & 1 != 0)
                    == Some(true)
        }),
        "{:?}",
        later.messages
    );
}

#[test]
fn the_gate_also_opens_when_rc_sets_the_flag() {
    let mut s = started();
    let mut flags = std::collections::BTreeMap::new();
    flags.insert("server.gateopen".to_string(), String::new());
    s.set_server_flags(flags);
    let p = s.tick(600);
    assert!(
        p.messages
            .iter()
            .any(|m| matches!(m, Message::NpcProps { npc_id: 1, .. })),
        "{:?}",
        p.messages
    );
}

#[test]
fn the_trainer_gives_the_lamp() {
    let mut s = started();
    let p = s.on_player(2, &player(HALL, 0), "playerenters");
    assert!(
        p.messages.iter().any(|m| matches!(
            m,
            Message::AddWeapon { player_id: 2, weapon } if weapon == b"Lamp"
        )),
        "{:?}",
        p.messages
    );
}

#[test]
fn the_lamp_weapon_file_is_there() {
    let lamp = std::fs::read_to_string(format!("{WORLD}/weapons/Lamp.txt"))
        .unwrap()
        .replace("\r\n", "\n");
    assert!(lamp.starts_with("GRAWP001\nREALNAME Lamp\n"), "{lamp}");
}

/// Width and height from a PNG's IHDR chunk.
fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n") && &bytes[12..16] == b"IHDR");
    let be = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
    (be(&bytes[16..20]), be(&bytes[20..24]))
}

/// Lab 2026-11-14: with `IMAGE bomb1.png` (a 128 by 320 sprite sheet) real
/// 2.05 to 2.31 drew the whole sheet as the Lamp's icon, in the weapons
/// list and over the gralat counter. The image must ship with the world
/// and be icon-sized.
#[test]
fn the_lamp_icon_ships_with_the_world_and_is_icon_sized() {
    let lamp = std::fs::read_to_string(format!("{WORLD}/weapons/Lamp.txt")).unwrap();
    let image = lamp
        .lines()
        .find_map(|l| l.trim_end().strip_prefix("IMAGE "))
        .expect("Lamp.txt has an IMAGE line");
    let bytes = std::fs::read(format!("{WORLD}/files/{image}"))
        .unwrap_or_else(|e| panic!("files/{image}: {e}"));
    let (w, h) = png_size(&bytes);
    assert!(w <= 32 && h <= 32, "{image} is {w}x{h}");
}

/// Lab 2026-11-14: the greeter and the shopkeeper stood on the same row
/// (y 29), and every real client drew their long lines over each other.
/// NPCs that talk on one level keep at least three rows apart.
#[test]
fn talking_npcs_on_a_level_do_not_share_a_row() {
    let mut spots: Vec<(String, String, f32)> = Vec::new();
    for entry in std::fs::read_dir(format!("{WORLD}/npcs")).unwrap() {
        let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        if !text.contains("message ") {
            continue;
        }
        let field = |k: &str| {
            text.lines()
                .find_map(|l| l.trim_end().strip_prefix(k).map(str::to_owned))
                .unwrap()
        };
        spots.push((field("LEVEL "), field("X "), field("Y ").parse().unwrap()));
    }
    assert!(spots.len() >= 4, "{spots:?}");
    for (i, a) in spots.iter().enumerate() {
        for b in &spots[i + 1..] {
            if a.0 == b.0 {
                assert!((a.2 - b.2).abs() >= 3.0, "{a:?} and {b:?} share a row");
            }
        }
    }
}

/// Over the link: a player saying "buy" (chat prop 12 in `PLAYER_PROPS`)
/// reaches the shop, and the gserver echoing a chat line this server set
/// does not count as the player talking.
#[test]
fn chat_in_player_props_reaches_the_shop_over_the_link() {
    let listener = mock::bind(0).unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let _hello = link::read_frame(&mut s).unwrap();
        link::write_frame(
            &mut s,
            &[
                Message::Welcome {
                    link_version: 1,
                    server_name: b"Lab Mock".to_vec(),
                    program: b"test".to_vec(),
                },
                Message::LevelList {
                    names: vec![SQUARE.to_vec(), HALL.to_vec()],
                },
                Message::PlayerJoined {
                    player_id: 2,
                    account: b"lab205".to_vec(),
                    level: SQUARE.to_vec(),
                    props: props(b"lab205", 25),
                },
            ],
        )
        .unwrap();
        let mut got = read_for(&mut s, Duration::from_millis(400));
        let mut chat = vec![0x20 + 12, 0x20 + 3];
        chat.extend_from_slice(b"buy");
        link::write_frame(
            &mut s,
            &[Message::PlayerProps {
                player_id: 2,
                props: chat,
            }],
        )
        .unwrap();
        let bought = read_for(&mut s, Duration::from_millis(600));
        // The gserver applies what the shop sent and echoes it, as
        // seihai-gserver does: 5 gralats left. Then an echo of a chat line
        // the server set (`setplayerprop #c`) must not run `playerchats`.
        let mut echo = vec![0x20 + 3];
        echo.extend(gint3(5));
        link::write_frame(
            &mut s,
            &[Message::PlayerProps {
                player_id: 2,
                props: echo,
            }],
        )
        .unwrap();
        let mut again = vec![0x20 + 12, 0x20 + 3];
        again.extend_from_slice(b"buy");
        link::write_frame(
            &mut s,
            &[Message::PlayerProps {
                player_id: 2,
                props: again,
            }],
        )
        .unwrap();
        let short = read_for(&mut s, Duration::from_millis(600));
        let _ = link::write_frame(
            &mut s,
            &[Message::Bye {
                reason: b"done".to_vec(),
            }],
        );
        got.extend(bought.iter().cloned());
        (bought, short)
    });

    let mut scripts = Scripts::with_world(Era::V217, WORLD);
    let timing = Timing {
        tick: Duration::from_millis(50),
        handshake: Duration::from_secs(5),
        ping_after: Duration::from_secs(30),
        dead_after: Duration::from_secs(30),
    };
    let mut lines = Vec::new();
    let (end, _) = session::run(&addr, TOKEN, timing, Some(&mut scripts), &mut |l| {
        lines.push(l)
    });
    let (bought, short) = server.join().unwrap();
    assert!(matches!(end, End::Closed(_)), "{end:?}");
    assert!(
        bought.iter().any(|m| matches!(
            m,
            Message::SetPlayerProps { player_id: 2, props } if props.starts_with(&[0x20 + 3])
                && props[1..4] == gint3(5)[..]
        )),
        "{bought:?}\n{lines:#?}"
    );
    assert!(
        short
            .iter()
            .filter_map(npc_message)
            .any(|m| m == "Sorry lab205, that is 20 gralats. You have 5."),
        "{short:?}\n{lines:#?}"
    );
}

#[test]
fn an_echo_of_our_own_chat_line_is_not_the_player_talking() {
    // A shopkeeper that would answer every chat line, and sets the player's
    // chat itself. The echo of that line must not set off another round.
    let dir = std::env::temp_dir().join(format!("seihai-npcserver-echo-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("npcs")).unwrap();
    std::fs::write(
        dir.join("npcs/parrot.txt"),
        "LEVEL m3_square.nw\nX 30\nY 30\nIMAGE block.png\nSCRIPT\nif (playerchats) { setplayerprop #c,polly; }\nSCRIPTEND\n",
    )
    .unwrap();
    let listener = mock::bind(0).unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let _hello = link::read_frame(&mut s).unwrap();
        let mut hello = vec![0x20 + 12, 0x20 + 5];
        hello.extend_from_slice(b"hello");
        link::write_frame(
            &mut s,
            &[
                Message::Welcome {
                    link_version: 1,
                    server_name: b"Lab Mock".to_vec(),
                    program: b"test".to_vec(),
                },
                Message::LevelList {
                    names: vec![SQUARE.to_vec()],
                },
                Message::PlayerJoined {
                    player_id: 2,
                    account: b"lab205".to_vec(),
                    level: SQUARE.to_vec(),
                    props: props(b"lab205", 0),
                },
                Message::PlayerProps {
                    player_id: 2,
                    props: hello,
                },
            ],
        )
        .unwrap();
        let first = read_for(&mut s, Duration::from_millis(400));
        // Echo what the parrot set, as the gserver does.
        let mut echo = vec![0x20 + 12, 0x20 + 5];
        echo.extend_from_slice(b"polly");
        link::write_frame(
            &mut s,
            &[Message::PlayerProps {
                player_id: 2,
                props: echo,
            }],
        )
        .unwrap();
        let after_echo = read_for(&mut s, Duration::from_millis(400));
        let _ = link::write_frame(
            &mut s,
            &[Message::Bye {
                reason: b"done".to_vec(),
            }],
        );
        (first, after_echo)
    });
    let mut scripts = Scripts::with_world(Era::V217, &dir);
    let timing = Timing {
        tick: Duration::from_millis(50),
        handshake: Duration::from_secs(5),
        ping_after: Duration::from_secs(30),
        dead_after: Duration::from_secs(30),
    };
    let (_end, _) = session::run(&addr, TOKEN, timing, Some(&mut scripts), &mut |_| {});
    let (first, after_echo) = server.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let sets = |ms: &[Message]| {
        ms.iter()
            .filter(|m| matches!(m, Message::SetPlayerProps { player_id: 2, .. }))
            .count()
    };
    assert_eq!(sets(&first), 1, "{first:?}");
    assert_eq!(sets(&after_echo), 0, "{after_echo:?}");
}

fn read_for(s: &mut std::net::TcpStream, budget: Duration) -> Vec<Message> {
    s.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
    let deadline = std::time::Instant::now() + budget;
    let mut got = Vec::new();
    while std::time::Instant::now() < deadline {
        match link::read_frame(s) {
            Ok(m) => got.extend(m),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => break,
        }
    }
    got
}
