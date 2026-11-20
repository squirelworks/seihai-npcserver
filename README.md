# seihai-npcserver

A Graal NPC-server: a separate program that logs into seihai-gserver and runs server-side NPC scripts.

Part of the Seihai project: the client [seihai](https://github.com/squirelworks/seihai)
and its library [seihai-core](https://github.com/squirelworks/seihai-core); the Rust
server stack [seihai-gserver](https://github.com/squirelworks/seihai-gserver),
[seihai-listserver](https://github.com/squirelworks/seihai-listserver),
[seihai-npcserver](https://github.com/squirelworks/seihai-npcserver),
[seihai-rc](https://github.com/squirelworks/seihai-rc) and
[seihai-servertools](https://github.com/squirelworks/seihai-servertools); the GPL forks
[seihai-gserver-v2](https://github.com/squirelworks/seihai-gserver-v2) and
[seihai-gs2lib](https://github.com/squirelworks/seihai-gs2lib); and the research notes in
[Graal_analysis](https://github.com/squirelworks/Graal_analysis).

## Status

Milestone 2 of the [plan](docs/PLAN.md) is done. The NPC-server connects to
the gserver over the NPC-server link, authenticates, and receives the level
list and every player's props. It then loads `<world>/npcs`, runs the
server-side scripts (`created`, `timeout`, `playerenters`, PMs and
`triggeraction`), and sends NPC changes, player changes, PMs and weapon grants
back over the link.

The link is specified in [PROTOCOL.md](PROTOCOL.md), the module map is in
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and what is left is in
[docs/NEXT.md](docs/NEXT.md). Contributors should read [AGENTS.md](AGENTS.md)
for the project rules.

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

## Sample world

`examples/m3-world` is a `seihai-gserver` world whose people only work
while the NPC-server runs: a greeter, a shop that takes gralats, a gate
that opens on a server flag and a trainer that gives a weapon by script.
Its README says how to run it.

## Try it without a gserver

A stand-in gserver ships as an example, for trying the link without
`seihai-gserver`. It listens on `127.0.0.1` only:

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

Prebuilt Linux, Windows and macOS archives are on the
[Releases page](https://github.com/squirelworks/seihai-npcserver/releases), built
by GitHub Actions for each `v*` tag. [VERSIONING.md](VERSIONING.md) explains when
a release is made, what the version numbers mean and what 1.0.0 needs.

## Licence

`MIT OR Apache-2.0`, at your option (see `LICENSE-MIT` and `LICENSE-APACHE`).
This repo contains no code from GServer-v2, gs2lib or gs2-parser (GPL) and must not.
