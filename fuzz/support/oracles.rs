use seihai_npcserver::link::{decode_body, encode_frame, read_frame, Message};
use std::io::{self, Read};

pub const MAX_INPUT: usize = 131_072;

struct Chunks<'a> {
    bytes: &'a [u8],
    position: usize,
    chunk: usize,
}

impl Read for Chunks<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = output
            .len()
            .min(self.chunk)
            .min(self.bytes.len() - self.position);
        output[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

fn canonical(messages: &[Message]) {
    let mut body = Vec::new();
    for message in messages {
        if message.encode_into(&mut body).is_err() {
            return;
        }
        // One message may already allocate substantially during encode_into.
        // Bound subsequent canonical decoding/compression, not that allocation.
        if body.len() > MAX_INPUT + 5 {
            return;
        }
    }
    assert_eq!(decode_body(&body).unwrap(), messages);
    if let Ok(frame) = encode_frame(messages) {
        assert_eq!(read_frame(&mut &frame[..]).unwrap(), messages);
    }
}

pub fn body(data: &[u8]) {
    if data.len() <= MAX_INPUT {
        if let Ok(messages) = decode_body(data) {
            canonical(&messages);
        }
    }
}

pub fn frames(data: &[u8]) {
    if data.len() > MAX_INPUT {
        return;
    }
    let mut whole = Chunks {
        bytes: data,
        position: 0,
        chunk: usize::MAX,
    };
    let mut fragmented = Chunks {
        bytes: data,
        position: 0,
        chunk: 1 + data.first().copied().unwrap_or(0) as usize % 32,
    };
    for _ in 0..4 {
        let before = whole.position;
        let left = read_frame(&mut whole);
        let right = read_frame(&mut fragmented);
        assert_eq!(whole.position, fragmented.position);
        match (left, right) {
            (Ok(left), Ok(right)) => {
                assert_eq!(left, right);
                assert!(whole.position > before);
                canonical(&left);
            }
            (Err(left), Err(right)) => {
                assert_eq!(left.kind(), right.kind());
                break;
            }
            _ => panic!("fragmentation changed frame decoding"),
        }
        if whole.position == data.len() {
            break;
        }
    }
}
