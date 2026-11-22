# NPC link decoder fuzzing

The two targets call the public production link decoders. `link_frames` compares
blocking whole reads with short reads, semantic messages, consumed bytes and error
kinds, for up to four coalesced frames. This does not model resumable `WouldBlock`.
`link_body` checks successful typed decoding through canonical model roundtrips.
Neither target compares compressed bytes or catches panics. Ordinary decoder
errors are accepted. No network listener is used.

Inputs are capped at 128 KiB, including the extended-header seeds. This is not a
decoder allocation cap: advertised frames can allocate about 16 MiB before EOF,
and decompression and owned message allocations can use substantially more.
The CI smoke therefore also limits input time, total time, runs, malloc size and
RSS. Fuzzer RSS checks are periodic, not an instantaneous memory guarantee.
Canonical reencoding stops once its body exceeds the input cap plus one message
header, avoiding a second full expanded-body decode/compression. Encoding one
expanded message can itself allocate substantially before that size check.

The independent nested Cargo workspace keeps libFuzzer out of normal application
builds. Fuzzing requires nightly Rust, cargo-fuzz and a C++ compiler; upstream
cargo-fuzz supports Linux and macOS. Normal Windows checks run deterministic
oracles, not libFuzzer. No macOS build or fuzz execution is claimed here.

The valid corpus models come from this repository's own existing message fixture.
`cargo run --example fuzz_corpus -- <destination>` regenerates seeds through the
production encoders; review changes before replacing checked-in seeds. Tests
compare decoded models rather than platform-dependent compression bytes.
