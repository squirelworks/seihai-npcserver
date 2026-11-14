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
| `npcdb` | done (m2) | Reads `<world>/npcs/*.txt`: our format and GServer-v2's `GRNPC001` files (below). |
| `scripts` | done (m2) | One `seihai_core::scripting::engine::world::ScriptWorld` per level. `created`, `timeout`, `playerenters`, `playerchats` (from `PM_TO_NPC`, and from a player's chat prop in `PLAYER_PROPS` since milestone 3) and `action*` (from `TRIGGER_ACTION`) run. `playerleaves` runs since `seihai-core` v0.0.6. |
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

The reader has one decoded frame of queued read-ahead. When it fills, the
reader waits instead of accumulating more frames, so TCP applies backpressure.
The reader may hold one further frame while sending, and the tick loop handles
one batch at a time. Each frame is limited to 16 MiB of decoded body by `link`;
decoded messages also have allocation overhead, so this is not a fixed RSS cap.
This bounds the inbound handoff, not persistent world state or script effects.
Every session exit drops the inbox, shuts down the shared socket and joins the
reader, releasing either a blocked send or an idle/partial-frame read. This
does not interrupt scripts, file operations, socket writes or decoding already
in progress; it runs when the session exits.

## Milestone 2 (done)

1. **Done.** `session::run` takes an optional `Scripts`. On the first
   `LEVEL_LIST` it reads `<world>/npcs` (the 2001 order), calls
   `Scripts::start`, then `Scripts::tick` each tick, and logs script lines.
   A fresh host is used on each connection, so `created` runs again after
   a reconnect.
2. **Done.** The milestone 2 messages are specified in `PROTOCOL.md` (link
   version 1 stays; the ids were reserved) and encoded in `link`, with
   round-trip tests. `seihai-gserver` carries the same file.
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
   | `Command` `toweapons` or `addweapon` | `ADD_WEAPON` for that player. `addweapon` is not a command in `seihai-core` (checked again at v0.0.5), so the statement is read from the engine's unknown-command log line |
   | `Command` `showcharacter` | `NPC_PROPS`: image `#c#`, gani `idle` |
   | `SetPlayerVar` `playerx`, `playery`, `playerdir`, `playerhearts`, `playerhp` | `SET_PLAYER_PROPS` (props 15, 16, 17, 2; `playerhp` is the same half-hearts prop as `playerhearts`, the GCHAR forms this server already reads) |
   | `SetPlayerVar` `playerrupees`, `playerdarts`, `playerbombs` | `SET_PLAYER_PROPS`: prop 3 as a GINT3, props 4 and 5 as GCHARs (counts clamped to 0 to 223) |
   | `SetPlayerVar` `playerswordpower`, `playershieldpower` | `SET_PLAYER_PROPS` prop 8 or 9 with the player's current image from the gserver's props (`c(power + 30)` or `c(power + 10)` + image). With no image known, the bare power (sword 0 to 4, shield 0 to 3) |
   | `Play` (`play`, `playlooped`, `play2`) | `PLAY_SOUND` for the script's level, at the NPC or at `play2`'s point. `play2`'s volume is dropped |
   | `StopSound`, `StopMidi` | `STOP_SOUND`, `STOP_MIDI` for the script's level |
   | `SetShape` | `NPC_SHAPE` (kind, width and height in pixels) |
   | `TriggerAction` (a script's own `triggeraction`) | `NPC_TRIGGER_ACTION` for the script's level. The engine itself fires it on the NPCs in that level first (`action<name>`, params split on commas) |
   | `say`, `sendrpgmessage`, `sendtoserver`, other commands, other player variables (the selected weapon among them), flags that are not `server.` | log once, unsupported. No link message for them. `sendtoserver` and setting the selected weapon stay this way on purpose: the reference server's scripts have neither |

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
and Seihai once `seihai-gserver` applies the NPC-server's messages (it
already has the handshake, level list and player side).

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
level), `CLASS`. Until then an NPC's name is its file name without `.txt`.

### GServer-v2 world folders (`GRNPC001`)

A file in the same folder whose first line is `GRNPC001` is read as a
GServer-v2 style NPC (`npcs/npc<NAME>.txt`). The reader was written from the
sample files in the fork's lab worlds, not from GServer-v2's code.

```
GRNPC001
NAME Control-NPC
ID 10000
TYPE CONTROL
IMAGE
STARTLEVEL
STARTX 30.00
STARTY 30.00
COLORS 2,0,10,4,18
SHAPE 32 48
NPCSCRIPT
if (playerlogin) { sendtorc #a has logged in!; }
NPCSCRIPTEND
```

- `NAME`, `STARTLEVEL`, `STARTX`, `STARTY`, `IMAGE` and the script fill the
  same `NpcDef` as our format. Every other `KEY value` line (`ID`, `TYPE`,
  `NICK`, `ANI`, the stats, `COLORS`, `SHAPE`, `SAVEARR`, and keys we do not
  know) is kept as written in `NpcDef::props`, not refused.
- A missing key keeps its default (empty, or 0 for `STARTX`/`STARTY`).
  CRLF line ends and a UTF-8 byte order mark are fine.
- NPC ids stay the file's place in name order; the `ID` line is kept in
  `props` only.
- When one of our files and a `GRNPC001` file have the same name, ours is
  used and the other is logged as skipped.
- An empty `STARTLEVEL` (the Control-NPC) is a level-less NPC. It runs in its
  own script world with no level: `created`, `timeout`, and `playerlogin` /
  `playerlogout` when a player joins or leaves the server (`PLAYER_JOINED`,
  `PLAYER_LEFT`; login before `playerenters`, logout after `playerleaves`).
  It gets no `NPC_ADDED`, and what only makes sense in a level (its own
  props, sounds, `NPC_TRIGGER_ACTION`) is dropped with one log line. Player
  props, warps, weapons, PMs and flags go out as usual. `playerlogin` and
  `playerlogout` are events from `seihai-core` `v0.0.7` (seihai #416).

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
