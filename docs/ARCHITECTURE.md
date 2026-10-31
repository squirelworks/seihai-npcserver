# seihai-npcserver architecture

Written for whoever picks this up next (person or coding agent). Read
`AGENTS.md` first: the licence and "no real servers" rules are binding.

## What it is

A separate program that logs into `seihai-gserver` over the NPC-server link
(`PROTOCOL.md`) and runs server-side NPC scripts for the gserver's levels. It
follows the 2001 Graal NPC-server design: the gserver owns players and
connections; the NPC-server owns server-side NPCs and their scripts, and asks
the gserver to change players.

```
 real clients  <-- Graal protocol -->  seihai-gserver  <-- link (PROTOCOL.md) -->  seihai-npcserver
                                         owns players,                              owns server-side
                                         levels, files                              NPCs and scripts
```

## Module map

| Module | State | Job |
|---|---|---|
| `link` | done (m1) | Frames and messages. No sockets, so `seihai-gserver` can depend on this crate (by git tag) and share the codec. |
| `session` | done (m1) | Connect, `HELLO`, wait for `WELCOME`, then the tick loop: read, (run scripts), keep-alive, send. Reader thread + `mpsc`, no async runtime. |
| `world` | done (m1) | Levels and players as the gserver described them. Raw prop bytes per prop id. |
| `npcdb` | started (m2) | Reads `<world>/npcs/*.txt` (format below). |
| `scripts` | started (m2) | One `seihai_core::scripting::engine::world::ScriptWorld` per level; `created` and `timeout` work and produce `Effect`s. |
| `mock` | done (m1) | Stand-in gserver for tests and `examples/mock_gserver.rs`. Binds to 127.0.0.1 only. |
| `main` | done (m1) | Arguments, token, logging, reconnect with backoff, `--once`. |

## The tick (session loop)

Every 0.1 s (the smallest useful `timeout`):

1. Drain frames from the reader thread; apply each message to `World`
   (`session::handle`).
2. **TODO(m2):** `Scripts::tick(100)`, then turn its effects into link
   messages (table below).
3. Keep-alive: `PING` after 30 s of not sending, close after 90 s of silence.
4. Write everything produced this tick as one frame.

## Milestone 2 plan (one PR, in this order)

1. **Wire scripts into the session.** Give `session::run` an optional
   `Scripts`. Load the NPC database after `LEVEL_LIST` (the 2001 order), call
   `Scripts::start`, then `Scripts::tick` each tick. Log script log lines.
2. **Link messages NPC → GS.** Specify the draft messages in `PROTOCOL.md`
   ("Draft: milestone 2") in both repos, bump nothing (link version 1 stays,
   since the ids were reserved), implement encode/decode in `link` with
   round-trip tests.
3. **Effects to link messages.** In a new `effects` module:

   | `Effect` | Link message |
   |---|---|
   | `SetPos`, `SetImg`, `SetVisible`, `SetCharAni`, `SetCharProp`, `Move` | `NPC_PROPS` for that NPC (NPC prop stream, see `seihai-core` `protocol::npcprops`) |
   | `Destroy` | `NPC_DELETED` |
   | `SetPlayerProp`, `Hurt` | `SET_PLAYER_PROPS` for the triggering player |
   | `SetLevel` (`setlevel2`) | `WARP_PLAYER` |
   | `SetFlag`, `UnsetFlag` on `server.` names | `SERVER_FLAG` |
   | `Message` | `NPC_PROPS` chat prop (`message` shows over the NPC) |
   | anything else | log once, skip |

4. **Player view.** Before a player-triggered event, fill the world's
   `PlayerView` from `world::Player` (nick, account, position from props; add
   decoders as needed). Fire `playerenters` on `PLAYER_LEVEL` / `PLAYER_JOINED`
   into a level with NPCs, and `playerleaves` on the way out. The engine has
   one player view per `ScriptWorld`; a server-side event runs for one player at
   a time, which is what the 2001 NPC-server did.
5. **Mock gserver** learns the new messages; loopback test: an NPC with
   `if (created) { timeout = 0.1; } if (timeout) { setcharprop #c,hi; timeout = 1; }`
   makes `NPC_PROPS` arrive at the mock.

Where `seihai-core`'s engine lacks a server-side command or event
(`setlevel2` versus client `setlevel`, `addweapon`, `toweapons`,
`triggeraction` from the server), add it **in the seihai repo** with tests, tag
a new `seihai-core`, and bump the tag in `Cargo.toml` here. Do not fork the
engine into this repo.

Milestone 3 (`05-npcserver.md` in the handoff folder): a sample world whose
NPCs only work with the NPC-server running, lab-tested with real 2.05 to 2.31
and Seihai once `seihai-gserver` has its side of the link (its stage 5).

## NPC files

`<world>/npcs/<name>.txt`, one NPC per file, read in name order; the n-th file
gets NPC id n. Keys, one per line, then the script:

```
LEVEL onlinestartlocal.nw
X 30
Y 30
IMAGE block.png
SCRIPT
if (created) { message hello; }
SCRIPTEND
```

Unknown keys are an error so typos show up. Planned keys: `NAME` (for
`callnpc` by name and NPC Control), `TYPE` (level NPC or "database NPC" with no
level), `CLASS`. GServer-v2 world folders keep NPCs differently; a reader for
those goes in `npcdb` too, written from the file format, not GServer-v2's code.

## Decisions (ask Squirrel before changing)

- Separate program, started by the gserver with `npcserver = auto` (see
  `PROTOCOL.md`, "Starting the NPC-server from the gserver"). Setup must stay one
  download, one config, one start command.
- No async runtime: one reader thread and a tick loop are enough for one link.
- GS1 first, with `seihai-core`'s engine. GS2 later, after the gserver has its
  own GS2 compiler (gs2-parser is GPL and cannot be used).
- The gserver owns players. The NPC-server never changes a player locally; it
  asks with a link message and sees the result in `PLAYER_PROPS`.

## Testing

- `cargo test`: unit tests per module, loopback session tests in
  `tests/link_session.rs` against `mock`.
- Lab runs go in `docs/lab/` (which client, what happened before and after).
- Only `127.0.0.1`, only servers you start yourself.
