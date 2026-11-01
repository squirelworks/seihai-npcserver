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
| `link` | done (m2) | Frames and messages, including the NPC-to-gserver rows. No sockets, so `seihai-gserver` can depend on this crate (by git tag) and share the codec. |
| `session` | done (m2) | Connect, `HELLO`, wait for `WELCOME`, then the tick loop: read, run scripts, send their messages, keep-alive. Reader thread + `mpsc`, no async runtime. |
| `world` | done (m2) | Levels and players as the gserver described them. Raw prop bytes per prop id. Nick, position, direction, hearts and chat are decoded for the player view. |
| `npcdb` | started (m2) | Reads `<world>/npcs/*.txt` (format below). |
| `scripts` | done (m2) | One `seihai_core::scripting::engine::world::ScriptWorld` per level. `created`, `timeout`, `playerenters`, `playerleaves`, `playerchats` (from `PM_TO_NPC`) and `action*` (from `TRIGGER_ACTION`) run. |
| `effects` | done (m2) | Maps `Effect` values onto link messages. Anything without a message is logged once. |
| `mock` | done (m2) | Stand-in gserver for tests and `examples/mock_gserver.rs`. Binds to 127.0.0.1 only. Records `NPC_PROPS` and the other NPC-to-gserver messages. |
| `main` | done (m1) | Arguments, token, logging, reconnect with backoff, `--once`. |

## The tick (session loop)

Every 0.1 s (the smallest useful `timeout`):

1. Drain frames from the reader thread; apply each message to `World`
   (`session::handle`).
2. `Scripts::tick` once per loop, by the configured tick (100 ms by
   default). Script log lines are printed. Effects are mapped to link
   messages (`src/effects.rs`) and written in step 4.
3. Keep-alive: `PING` after 30 s of not sending, close after 90 s of silence.
4. Write everything produced this tick as one frame.

## Milestone 2 plan (one PR, in this order)

1. **Done.** `session::run` takes an optional `Scripts`. On the first
   `LEVEL_LIST` it reads `<world>/npcs` (the 2001 order), calls
   `Scripts::start`, then `Scripts::tick` each tick, and logs script lines.
   A fresh host is used on each connection, so `created` runs again after
   a reconnect.
2. **Done.** The milestone 2 messages are specified in `PROTOCOL.md` (link
   version 1 stays; the ids were reserved) and encoded in `link`, with
   round-trip tests. The copy in `seihai-gserver` is still the draft.
3. **Done.** `src/effects.rs`:

   | `Effect` | Link message |
   |---|---|
   | `SetPos`, `SetImg`, `SetVisible`, `SetCharAni`, `SetCharProp`, `Move` | `NPC_PROPS` for that NPC (NPC prop stream, see `seihai-core` `protocol::npcprops`) |
   | `Destroy` | `NPC_DELETED` |
   | `SetPlayerProp`, `Hurt` | `SET_PLAYER_PROPS` for the triggering player |
   | `SetLevel` (`setlevel2`) | `WARP_PLAYER` |
   | `SetFlag`, `UnsetFlag` on `server.` names | `SERVER_FLAG` |
   | `Message` | `NPC_PROPS` chat prop (`message` shows over the NPC) |
   | `Say2` | `PM` to the triggering player |
   | `Command` `toweapons` or `addweapon` | `ADD_WEAPON` for that player. `addweapon` is not a command in `seihai-core` v0.0.3, so the statement is read from the engine's unknown-command log line |
   | `Command` `showcharacter` | `NPC_PROPS`: image `#c#`, gani `idle` |
   | `SetPlayerVar` `playerx`, `playery`, `playerdir`, `playerhearts` | `SET_PLAYER_PROPS` (props 15, 16, 17, 2, the GCHAR forms this server already reads) |
   | `say`, `play`, `playlooped`, `play2`, `stopsound`, `stopmidi`, `setshape`, `triggeraction`, `sendrpgmessage`, `sendtoserver`, other commands, other player variables, flags that are not `server.` | log once, unsupported. No link message for them |

4. **Done.** Before `playerenters` / `playerleaves`, the level's `PlayerView`
   is filled from `world::Player` (nick, account, position, direction, hearts,
   chat). `playerenters` fires on `PLAYER_LEVEL` / `PLAYER_JOINED` into a level
   with NPCs, and `playerleaves` on the way out. The engine has one player view
   per `ScriptWorld`; a server-side event runs for one player at a time.
   Bare `#n` is that player's nickname; `seihai-core` expands it from
   `PlayerView.nick`. `#n(id)` is expanded from `World.players`: the id is
   the player's link id, and the nickname is prop 0. An id that is not
   online stays written as `#n(id)`. The engine does not expand that form;
   this server does, on the text it sends.
5. **Done.** The mock records the new messages. Loopback test: an NPC with
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
