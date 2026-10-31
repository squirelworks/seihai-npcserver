# seihai-npcserver

A Graal NPC-server: a separate program that logs into seihai-gserver and runs server-side NPC scripts.

Part of the Seihai server stack:
[seihai-gserver](https://github.com/squirelworks/seihai-gserver) ·
[seihai-npcserver](https://github.com/squirelworks/seihai-npcserver) ·
[seihai-listserver](https://github.com/squirelworks/seihai-listserver) ·
[seihai-rc](https://github.com/squirelworks/seihai-rc) ·
[seihai-gserver-v2](https://github.com/squirelworks/seihai-gserver-v2) (GPL fork of GServer-v2, ships first) ·
client: [seihai](https://github.com/squirelworks/seihai).

**Status: milestone 1.** It connects to the gserver over the NPC-server link,
authenticates, and logs the level list and player props it is sent. It runs no
scripts yet. The link is specified in [PROTOCOL.md](PROTOCOL.md), the module map and next
steps are in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), the plan is in
[docs/PLAN.md](docs/PLAN.md), and the rules every contributor and coding agent
follows are in [AGENTS.md](AGENTS.md).

## Run

Most people never start it by hand: with `npcserver = auto` in its config,
`seihai-gserver` starts it and passes it a fresh token (see
[PROTOCOL.md](PROTOCOL.md#starting-the-npc-server-from-the-gserver-npcserver--auto)).
To run it yourself:

```
seihai-npcserver --gserver 127.0.0.1:14810 --token-file npcserver-token.txt --world path/to/world
```

The token file holds the same token as `npcserver_token` in the gserver's
config. The token is never taken on the command line.

## Try it without a gserver

Until `seihai-gserver` has its side of the link, a stand-in gserver ships as an
example. It listens on `127.0.0.1` only:

```
export SEIHAI_NPCSERVER_TOKEN=lab-token-0123456789
cargo run --example mock_gserver &
cargo run -- --once
```

## Build

```
cargo build --release
cargo test        # includes loopback tests against the stand-in gserver
```

Releases for Linux, Windows and macOS are built by GitHub Actions on each `v*` tag.

## Licence

`MIT OR Apache-2.0`, at your option (see `LICENSE-MIT` and `LICENSE-APACHE`).
This repo contains no code from GServer-v2, gs2lib or gs2-parser (GPL) and must not.
