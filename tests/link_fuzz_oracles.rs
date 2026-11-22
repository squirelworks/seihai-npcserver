#[path = "../fuzz/support/corpus.rs"]
mod corpus;
#[path = "../fuzz/support/oracles.rs"]
mod oracles;
use seihai_npcserver::link::{decode_body, read_frame};

#[test]
fn advertised_length_guard_precedes_payload_read() {
    for (bytes, kind) in [
        ([255, 255, 1, 0, 0, 1], std::io::ErrorKind::UnexpectedEof),
        ([255, 255, 1, 0, 0, 2], std::io::ErrorKind::InvalidData),
    ] {
        assert_eq!(read_frame(&mut &bytes[..]).unwrap_err().kind(), kind);
        oracles::frames(&bytes);
    }
}

#[test]
fn constructor_frames_preserve_models_and_one_frame_cursor() {
    for (name, expected, frame) in corpus::valid() {
        let mut with_suffix = frame.clone();
        with_suffix.push(0xa5);
        let mut reader = &with_suffix[..];
        assert_eq!(read_frame(&mut reader).unwrap(), expected, "{name}");
        assert_eq!(reader, &[0xa5], "{name}");
        oracles::frames(&frame);
        let cuts: Vec<_> = if frame.len() <= 512 {
            (0..frame.len()).collect()
        } else {
            vec![0, 1, 2, 3, 5, 6, frame.len() / 2, frame.len() - 1]
        };
        for cut in cuts {
            assert!(
                read_frame(&mut &frame[..cut]).is_err(),
                "{name} prefix {cut}"
            );
        }
    }
}

#[test]
fn checked_in_seeds_exercise_the_actual_fuzz_oracles() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fuzz/corpus");
    for (target, name, generated) in corpus::seeds() {
        let bytes = std::fs::read(root.join(target).join(&name)).unwrap();
        // Compare decoded models rather than compressor byte output.
        if target == "link_frames" {
            assert_eq!(
                read_frame(&mut &bytes[..]).ok(),
                read_frame(&mut &generated[..]).ok(),
                "{name}"
            );
            oracles::frames(&bytes);
        } else {
            assert_eq!(
                decode_body(&bytes).ok(),
                decode_body(&generated).ok(),
                "{name}"
            );
            oracles::body(&bytes);
        }
    }
}
