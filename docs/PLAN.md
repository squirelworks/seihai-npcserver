# seihai-npcserver

A separate program that logs into `seihai-gserver` as a special client and runs
server-side NPC scripts for the levels it owns. This follows the 2001 Graal
NPC-server design (project file `npcserver-2001-notes.md`).

## Design

- Main loop every 0.1 s: read from the gserver, run scripts, push changed NPC
  props to the gserver, push changed player props, sleep out the tick. 0.1 s is
  the smallest useful `timeout`/`sleep`.
- Startup: load options, connect to the gserver, then load the level list, then
  the NPC database.
- Gserver to NPC-server: player props, level temporary state (bombs, explosions,
  bush removal, dropped items), PMs, game time, level reloads, `server.` flag
  add/remove.
- NPC-server to gserver: NPC props, player props when scripts change a player, PM
  replies, weapon grants (`addweapon`, `toweapons`), warps (`setlevel2`).
- The link protocol is ours to define (the original's ids are not known). Keep it
  simple: the same framing as gen 5, with its own message ids, documented in the
  repo's `PROTOCOL.md`.
- Scripts: GS1 first, reusing `seihai-core`'s GS1 parser and interpreter
  (`scripting/`), with the server-side events and commands added there.
- NPC Control: RC 1.41 talked to the NPC-server for the NPC list, classes and
  weapons. `seihai-rc` should talk to it the same way, through the gserver or
  directly.

## Easy setup

The owner wants one-command setup. `seihai-gserver` starts the NPC-server for you
when `npcserver = auto`, passing it a one-time token, so most people never
configure the link. Running it on another machine is the advanced option.

## First milestones

1. Connect, authenticate, receive level lists and player props; log them.
   **Done** (link version 1 in `PROTOCOL.md`; lab run in
   `docs/lab/2026-10-31-milestone1.md`). The gserver side comes with
   `seihai-gserver` stage 5.
2. **Done.** NPC files, the script host, and link messages back to the
   gserver. `setcharprop`, `setlevel2`, `message`, moves, `hurt`,
   `server.` flags, `say2` (`PM`), `toweapons` / `addweapon`
   (`ADD_WEAPON`) and `showcharacter` are sent. `PM_TO_NPC`,
   `SERVER_FLAGS` and `TRIGGER_ACTION` run. See `docs/NEXT.md` for the
   commands that are still only logged.
3. A sample world whose NPCs only work when the NPC-server runs; lab test with
   real 2.05 to 2.31 and Seihai.
