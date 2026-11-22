use seihai_npcserver::link::{encode_frame, encode_frame_bz2, Message};

#[path = "messages.rs"]
mod messages;

// Constructor models come from this repository's own link unit-test fixture.
// Expected messages are independent of the decoder under test.
pub fn valid() -> Vec<(String, Vec<Message>, Vec<u8>)> {
    let all = messages::all();
    let mut cases = Vec::new();
    for (index, message) in all.iter().enumerate() {
        let model = vec![message.clone()];
        cases.push((
            format!("message-{index:02}"),
            model.clone(),
            encode_frame(&model).unwrap(),
        ));
    }
    for (name, model) in [
        ("empty", vec![]),
        ("all", all),
        (
            "compressed",
            vec![Message::PlayerProps {
                player_id: 7,
                props: vec![b'a'; 4096],
            }],
        ),
        (
            "binary",
            vec![Message::Unknown {
                id: 0xff,
                data: (0..=255).collect(),
            }],
        ),
    ] {
        cases.push((name.into(), model.clone(), encode_frame(&model).unwrap()));
        if name == "compressed" {
            cases.push((
                "bz2".into(),
                model.clone(),
                encode_frame_bz2(&model).unwrap(),
            ));
        }
    }
    for total in [65_534usize, 65_535, 65_536] {
        let model = vec![Message::Unknown {
            id: 0xff,
            data: vec![0xa5; total - 6],
        }];
        let mut frame = Vec::new();
        if total < 65_535 {
            frame.extend_from_slice(&(total as u16).to_be_bytes());
        } else {
            frame.extend_from_slice(&[0xff, 0xff]);
            frame.extend_from_slice(&(total as u32).to_be_bytes());
        }
        frame.push(2);
        model[0].encode_into(&mut frame).unwrap();
        cases.push((format!("boundary-{total}"), model, frame));
    }
    cases
}

pub fn seeds() -> Vec<(&'static str, String, Vec<u8>)> {
    let mut seeds = Vec::new();
    for (name, model, frame) in valid() {
        seeds.push(("link_frames", name.clone(), frame));
        // Extended uncompressed frames already cover the large boundaries;
        // keep duplicate body seeds small to bound the checked-in corpus.
        if !name.starts_with("boundary-") {
            let mut body = Vec::new();
            for message in model {
                message.encode_into(&mut body).unwrap();
            }
            seeds.push(("link_body", name, body));
        }
    }
    let ping = encode_frame(&[Message::Ping { nonce: 7 }]).unwrap();
    let mut coalesced = ping.clone();
    coalesced.extend_from_slice(&ping);
    seeds.push(("link_frames", "coalesced".into(), coalesced));
    for (name, bytes) in [
        ("short-header", vec![0]),
        ("extended-header", vec![255, 255, 0]),
        ("zero-length", vec![0, 0]),
        ("unknown-compression", vec![0, 1, 255]),
        ("max-advertised-eof", vec![255, 255, 1, 0, 0, 1]),
        ("over-max-advertised", vec![255, 255, 1, 0, 0, 2]),
        ("truncated-zlib", vec![0, 3, 4, 0x78, 0x9c]),
        ("truncated-bz2", vec![0, 4, 6, b'B', b'Z', b'h']),
    ] {
        seeds.push(("link_frames", name.into(), bytes));
    }
    for (name, bytes) in [
        ("truncated-message", vec![4, 0, 0, 0, 4, 0]),
        ("oversized-payload", vec![4, 255, 255, 255, 255]),
        ("truncated-list", vec![0x10, 0, 0, 0, 2, 255, 255]),
        ("truncated-string", vec![6, 0, 0, 0, 2, 255, 255]),
        (
            "known-trailing-fields",
            vec![4, 0, 0, 0, 5, 0, 0, 0, 7, 255],
        ),
    ] {
        seeds.push(("link_body", name.into(), bytes));
    }
    seeds
}
