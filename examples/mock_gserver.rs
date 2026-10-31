//! A stand-in gserver for loopback lab runs of seihai-npcserver.
//!
//! ```text
//! export SEIHAI_NPCSERVER_TOKEN=lab-token-0123456789
//! cargo run --example mock_gserver &          # listens on 127.0.0.1:14810
//! cargo run -- --once
//! ```
//!
//! It serves one connection with the lab script from `seihai_npcserver::mock`
//! and exits. It binds to 127.0.0.1 only.

use seihai_npcserver::link::TOKEN_ENV;
use seihai_npcserver::mock;

fn main() -> std::io::Result<()> {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(14810);
    let token = std::env::var(TOKEN_ENV).unwrap_or_else(|_| {
        eprintln!("set {TOKEN_ENV} to the token the NPC-server will send");
        std::process::exit(2);
    });
    let listener = mock::bind(port)?;
    eprintln!("mock gserver on 127.0.0.1:{port}");
    let seen = mock::serve_one(&listener, &mock::Script::lab(token.as_bytes()))?;
    eprintln!("mock gserver: HELLO {:?}", seen.hello.map(|m| m.id()));
    eprintln!(
        "mock gserver: {} messages back: {:?}",
        seen.other.len(),
        seen.other
    );
    Ok(())
}
