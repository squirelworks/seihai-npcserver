# Milestone 3 sample world with real clients, 2026-11-14

Loopback only. Everything ran on the lab host inside a private network
namespace whose only interface is `lo`, with the real clients under Wine
on a headless Xvfb screen. No public server and no real player account was
used; every account was a fresh lab account made on first login.

## Setup

- seihai-gserver `main` (`d6c684d`) and this repo at
  `claude/project-thread-zdb1un` (`c7100da`, 0.1.2), both
  `cargo build --release`, side by side in one folder.
- A copy of `examples/m3-world` with `listserver = 127.0.0.1:14900`,
  `listserver_secure = false` added, and in `files/` the art the clients ask
  for: `block.png`, `bomb1.png`, head/body/sword/shield images and the
  standard `.gani` files. `npcserver = auto` started the NPC-server, which
  found 2 levels and 5 NPCs.
- The Python lab listserver on 127.0.0.1 (14900 for the gserver,
  14921 to 14923 for clients).
- Clients: 2.05 rev 2, 2.17.1, 2.22, 2.31, one at a time, each with a fresh
  account (`m3t205`, `m3t217`, `m3t222`, `m3t231`). Steps were driven with
  key and mouse events and checked by screenshot and the gserver log
  (player props, level changes).

## Results

| Step | 2.05 rev 2 (gen 3) | 2.17.1 (gen 3) | 2.22 (gen 5) | 2.31 (gen 5) |
|---|---|---|---|---|
| a. Greeter: welcome by name, gralats 0 to 30 | pass | pass | pass | pass |
| a. "hi" answered ("Hi nick! The shop is west, the gate is east.") | pass | pass | pass | pass |
| b. "buy": gralats 30 to 10, bombs 5 to 15, "Thank you" | pass | pass | pass | pass |
| b. "buy" again: "Sorry nick, that is 20 gralats. You have 10." | pass | pass | pass | pass |
| c. Gate block drawn in the doorway | pass | pass | pass | pass |
| c. "open": gate gone, "The gate is open for 30 seconds, nick." | pass | pass | pass | pass |
| c. Walk through into `m3_hall.nw` | pass | pass | pass | pass |
| c. Gate back after 30 s, gatekeeper's line reset | pass | pass | pass | pass |
| d. Trainer gives the Lamp; it is in the weapons list | pass | pass | pass | pass |
| d. Fire it: player chat "The lamp shines!" | pass | pass | pass | pass |
| e. NPCs drawn as characters with their names | yes | yes | yes | yes |

Coming back to the square with under 20 gralats, the greeter gave another
30, as its script says.

With `npcserver = off` (2.22, account `m3off`): the square has no NPCs, no
welcome, gralats stay 0, and the doorway is open.

## Problems found in the sample world, fixed on this branch

1. **The Lamp's icon was a whole sprite sheet.** `Lamp.txt` had
   `IMAGE bomb1.png`, the 128 by 320 bomb and explosion sheet. A weapon's
   image is its icon: every client drew the whole sheet in the weapons list
   and in the item slot, where it covered the gralat counter. The world now
   ships its own 32 by 32 icon, `files/m3lamp.png` (drawn for this world,
   transparent colour at palette index 0; with it at the last index 2.22
   drew a black square behind the lamp).
2. **The greeter's and the shopkeeper's lines overlapped.** Both stood on
   row 29, and their long messages were drawn over each other in all four
   clients. The shopkeeper now stands at 20,25.

Rerun with the fixed world on 2.22 (accounts `m3fix`, `m3fix2`): the two
lines are apart, the Lamp shows as a small icon in the list and the item
slot with the counters clear, and firing it still gives "The lamp shines!".
`cargo test --release`: all pass, with two new tests,
`the_lamp_icon_ships_with_the_world_and_is_icon_sized` and
`talking_npcs_on_a_level_do_not_share_a_row` (`tests/sample_world.rs`).

## Other observations

- 2.17.1, 2.22 and 2.31 under Wine on Xvfb smear the level tiles while the
  view scrolls (rows of copies of a tile, stale rocks). NPCs and the player
  stay in the right places, and the board is redrawn after a level change.
  2.05 rev 2 did not do this. This looks like the Wine and Xvfb drawing path,
  not the server.
- 2.17.1, 2.22 and 2.31 each sent client message 38 once, about 20 s after
  login; the gserver logged "message 38 not handled in stage 1". Nothing
  visible followed. 2.05 rev 2 did not send it.
- A client sends a chat line only when it differs from its current chat:
  saying "buy" twice in a row reaches the shop once.
- No warnings, errors or timer problems from the NPC-server in the log.

## Seihai client

Seihai `main` (`4d1a28d`, `seihai-mae` built with `--features dev`),
logging in through the lab listserver as 2.22 (gen 5), account `m3sei`,
against the fixed world:

| Step | Seihai |
|---|---|
| a. Welcome by name, gralats 0 to 30; "hi" answered | pass |
| b. "buy": 30 to 10 gralats, 5 to 15 bombs, "Thank you"; again: "Sorry … You have 10." | pass |
| c. Gate drawn; "open" hides it; walk into the hall; back after 30 s | pass |
| d. Lamp given, in the item list with its icon; firing it: "The lamp shines!" | pass |
| e. NPCs drawn as characters | yes, but see below |

Seihai drew the four NPCs as characters but without their names under
them (the real clients show "Greeter", "Shopkeeper" and so on, set with
`setcharprop #n`), and drew every character, the player included, with a
helmet head where the real clients draw the default head. Both are for the
Seihai client, not this server. Seihai's board scrolled cleanly.

## Screenshots

On the lab host, not in the repo: `shots/<client>-<step>.png` in the lab
folder (`205-…`, `217-…`, `222-…`, `231-…`, `off222-01-square.png`,
`fix222-…`, `fix222b-…`, `seihai-…`).
