# Lab note: GRNPC001 files from the fork's npcdemo world, 2026-11-12

Issue #21. Where: cloud container, Linux x86-64. No server or client was
started: the check is the NPC database load, which runs before any link.

Input: a copy of `bin/servers/npcdemo/` from `seihai-gserver-v2` (`seihai` at
`06fb887`). Its `npcs/` folder holds one file, `npcControl-NPC.txt`
(`GRNPC001`, `NAME Control-NPC`, `ID 10000`, `TYPE CONTROL`, empty
`STARTLEVEL`, a 12-line script). Only the data files were read, not the
fork's code.

## Before

`npcdb::load` read the file as our format and refused it (`unknown key
"GRNPC001"`), so it was logged as skipped with a parse error.

## After

`npcdb::load` on the copy:

```
1 "Control-NPC" Grnpc level="" (30,30) kind=Some("CONTROL") props=25 script_lines=12
problems []
```

`Scripts::open_database` on the same folder:

```
(0, ["NPC \"Control-NPC\" (id 1, TYPE CONTROL): no start level; level-less NPCs are read but not run yet"])
```

So the record comes up whole (name, position, 25 other keys, the script),
and the server says plainly why it does not run it: the Control-NPC has no
level, and level-less NPCs (and `playerlogin`) are the next item in
`docs/NEXT.md`. A GRNPC001 NPC with a start level loads and runs `created`
(`scripts::tests::grnpc_files_come_up_and_level_less_ones_are_noted`).

Not done here: a run against the fork gserver with real clients; that
waits on level-less NPCs and on seihai-gserver applying NPC-server messages.
