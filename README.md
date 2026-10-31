# seihai-npcserver

A Graal NPC-server: a separate program that logs into seihai-gserver and runs server-side NPC scripts.

Part of the Seihai server stack:
[seihai-gserver](https://github.com/squirelworks/seihai-gserver) ·
[seihai-npcserver](https://github.com/squirelworks/seihai-npcserver) ·
[seihai-listserver](https://github.com/squirelworks/seihai-listserver) ·
[seihai-rc](https://github.com/squirelworks/seihai-rc) ·
[seihai-gserver-v2](https://github.com/squirelworks/seihai-gserver-v2) (GPL fork of GServer-v2, ships first) ·
client: [seihai](https://github.com/squirelworks/seihai).

**Status: scaffold.** Nothing works yet. The plan is in [docs/PLAN.md](docs/PLAN.md),
the rules every contributor and coding agent follows are in [AGENTS.md](AGENTS.md).

## Build

```
cargo build --release
cargo test
```

Releases for Linux, Windows and macOS are built by GitHub Actions on each `v*` tag.

## Licence

`MIT OR Apache-2.0`, at your option (see `LICENSE-MIT` and `LICENSE-APACHE`).
This repo contains no code from GServer-v2, gs2lib or gs2-parser (GPL) and must not.
