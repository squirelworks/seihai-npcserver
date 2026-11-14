# Milestone 3 sample world

A small `seihai-gserver` world whose people only do anything while
`seihai-npcserver` runs them. Without the NPC-server the levels are empty
and the doorway to the hall is open; with it:

| NPC | Where | What it does | How |
|---|---|---|---|
| Greeter (`npcs/greeter.txt`) | `m3_square.nw` 32,29 | Welcomes each player by name. Anyone with under 20 gralats gets 30. Answers "hi". | `playerenters`, `playerrupees`, `playerchats` |
| Shopkeeper (`npcs/shopkeeper.txt`) | `m3_square.nw` 20,29 | Say "buy": 20 gralats for 10 bombs, or "Sorry" with your count. | `playerchats`, `playerrupees`, `playerbombs` |
| Gatekeeper (`npcs/gatekeeper.txt`) | `m3_square.nw` 45,22 | Say "open": sets the server flag `server.gateopen` for 30 seconds. | `set`/`unset server.`, `timeout` |
| Gate (`npcs/gate.txt`) | `m3_square.nw` 41,20 | A block in the doorway to the hall. Hidden while `server.gateopen` is set, whoever set it (the gatekeeper or RC). | `server.` flag, `hide`/`show` |
| Trainer (`npcs/trainer.txt`) | `m3_hall.nw` 31,41 | Gives every visitor the Lamp (`weapons/Lamp.txt`). | `playerenters`, `addweapon` |

Players talk to NPCs by chatting next to them: the gserver passes each
player's chat (prop 12) to the NPC-server, which runs `playerchats` on
that player's level.

## Run it

Build `seihai-gserver` and `seihai-npcserver` and put both binaries in one
folder (or set `npcserver_path`). Copy this folder somewhere writable and:

```
seihai-gserver --world path/to/m3-world --verbose
```

`config/server.txt` has `npcserver = auto`, so the gserver starts the
NPC-server itself. It listens on `127.0.0.1:14802` and makes an account
on first login (`create_accounts = true`): lab use only.

## Where the levels come from

`levels/m3_square.nw` and `levels/m3_hall.nw` are Seihai's own starter
levels `new1.nw` and `new2.nw` (`seihai-core`, `assets/default/levels`,
same licence), with their links and signs replaced. In the square the
doorway to the house is narrowed to two tiles so one 32 by 32 block
(`block.png`) closes it. The tiles follow the standard Graal tile layout,
so real clients draw them with their own `pics1.png`.
