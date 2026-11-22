#![no_main]
#[path = "../support/oracles.rs"]
#[allow(dead_code)]
mod oracles;
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| oracles::body(data));
