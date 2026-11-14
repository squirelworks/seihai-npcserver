# Next steps

One branch and pull request each. Rules: `AGENTS.md`. Milestone 2 is done
(the list below, kept as a record); what is still open is under **Open**.

## Milestone 2 (done)

- [x] **Wire scripts into the session.** `session::run` takes an optional
      `Scripts`. On the first `LEVEL_LIST` it reads `<world>/npcs`, calls
      `Scripts::start`, then `Scripts::tick` each tick, and logs script
      lines.

- [x] **Link messages NPC to gserver.** The milestone 2 messages are in
      `PROTOCOL.md` (no longer a draft). Link version 1 stays. Encode and
      decode live in `link`, with round-trip tests. `seihai-gserver` now
      carries the same `PROTOCOL.md` and depends on this crate's `link`
      at tag `v0.1.0`.

- [x] **Effects to link messages.** `src/effects.rs` maps `Effect` values
      onto those messages (`docs/ARCHITECTURE.md`, the effects table).
      Anything without a message is logged once and skipped.

- [x] **Player view.** Before `playerenters` / `playerleaves`, the level's
      `PlayerView` is filled from `world::Player` (nick, account, position,
      direction, hearts, chat). Bare `#n` is that player's nickname;
      `seihai-core` expands it from `PlayerView.nick`. `#n(id)` came in
      the next items: this server expands it from `World.players`.

- [x] **Mock gserver learns the new messages.** Loopback test: an NPC with
      `if (created) { timeout = 0.1; } if (timeout) { setcharprop #c,hi; timeout = 1; }`
      makes `NPC_PROPS` arrive at the mock.

- [x] **Chat, weapons, PMs, flags, triggers.** `say2` is `PM`. `toweapons`
      and `addweapon` are `ADD_WEAPON`. `showcharacter` is the character
      NPC props. `PM_TO_NPC` runs as `playerchats` on the sender's level,
      `SERVER_FLAGS` replaces the flag set, and `TRIGGER_ACTION` fires
      `action<name>`. `#n(id)` expands from `World.players` when that id
      is online. The commands this left logged-only were given link
      messages in the items below, apart from `say`, `sendrpgmessage`,
      `sendtoserver`, the selected weapon and unknown commands, which are
      still logged once and skipped.

- [x] **Sounds.** `play`, `playlooped` and `play2` are `PLAY_SOUND`,
      `stopsound` is `STOP_SOUND` and `stopmidi` is `STOP_MIDI`, each for
      the script's level (`PROTOCOL.md`, `0x48` to `0x4A`). The mock
      gserver receives them in the loopback test. `play2`'s volume is not
      carried.

- [x] **`setshape`.** Sent as `NPC_SHAPE` (`0x4B`): kind, width and height
      in pixels. The NPC prop stream has no shape prop.

- [x] **A script's own `triggeraction`.** Sent as `NPC_TRIGGER_ACTION`
      (`0x4C`) for the script's level, with the action and its params.

- [x] **Rupees, darts and bombs.** `playerrupees`, `playerdarts` and
      `playerbombs` are `SET_PLAYER_PROPS`: prop 3 as a GINT3, props 4 and
      5 as GCHARs. No new link message.

- [x] **Server NPCs hearing each other's `triggeraction`.** Already done
      by `seihai-core`'s engine inside each level: the NPCs under the point
      get `action<name>` with the params. A trigger that answers itself is
      cut off after 1000 queued calls in one step, and every round still
      sends its effects (and an `NPC_TRIGGER_ACTION`) to the gserver. A test
      pins both.

- [x] **Sword and shield power.** `playerswordpower` and
      `playershieldpower` are `SET_PLAYER_PROPS` props 8 and 9, keeping the
      image the gserver last sent for that player. With no image known,
      the bare power form.

- [x] **The rest of the logged-only effects.** `playerhp` is
      `SET_PLAYER_PROPS` prop 2, the same half-hearts value as
      `playerhearts` (the reference server treats `hp` that way). `sendtoserver`
      and setting the selected weapon stay logged and skipped on purpose:
      the reference server's scripts have neither. Its `sendtonc` and
      `sendtorc` could be a later item.

## Open

- [ ] **`seihai-gserver` applies the messages.** It reads the milestone 1
      messages but does not yet act on anything the NPC-server sends (NPC
      props, player props, warps, PMs, weapons, flags, triggers, sounds,
      shapes). That work is in `seihai-gserver` (its stage 5); milestone 3
      here waits on it.
- [x] **`playerleaves` runs.** `session` fires it, and `seihai-core`
      `v0.0.6` lists it as a 2.1-era event (seihai #390), so
      `if (playerleaves)` runs. Test: `scripts::tests::playerleaves_runs`.
- [x] **GServer-v2 style NPC files.** `npcdb` reads `GRNPC001` files
      beside ours (issue #21; `docs/ARCHITECTURE.md`, "GServer-v2 world
      folders"). Checked on a copy of the fork's `npcdemo` world:
      `docs/lab/2026-11-12-grnpc-npcdemo.md`.
- [x] **Level-less NPCs.** A `GRNPC001` file with an empty `STARTLEVEL`
      (the fork's Control-NPC, `TYPE CONTROL`) runs in a world of its own
      and gets `playerlogin`/`playerlogout` (`docs/ARCHITECTURE.md`).
- [x] **`playerlogin`/`playerlogout` blocks run.** `seihai-core` `v0.0.7`
      lists them (seihai #416). Test:
      `scripts::tests::level_less_npcs_get_playerlogin_and_playerlogout`.
- [ ] **`sendtonc` and `sendtorc`.** The reference server's scripts have
      them; a later item if wanted.
- [ ] **A v0.1.1 release.** Notes in `docs/releases/v0.1.1.md`; the tag
      waits on the owner. Then `seihai-gserver` moves off `v0.1.0`.

## Milestone 3

`seihai-gserver` applies the NPC-server's messages since
squirelworks/seihai-gserver#62.

- [x] **Sample world.** `examples/m3-world`: a greeter, a shop that takes
      gralats, a gate that opens on a server flag and a trainer that
      gives a weapon by script (`examples/m3-world/README.md`). Tests:
      `tests/sample_world.rs`. What it needed here:
  - Players talk to NPCs by chatting: a chat line (prop 12) in
    `PLAYER_PROPS` runs `playerchats` on the player's level with `#c`
    set. The gserver's echo of a chat line this server set
    (`setplayerprop #c`) is not counted.
  - The player view carries gralats (prop 3), arrows (4) and bombs (5),
    so `playerrupees` and the rest read the player's real counts.
  - A `server.` flag a script sets is seen by every level at once, and a
    flag with no value reads as 1, so `if (server.gateopen)` works.
- [x] **Lab run** on seihai-gserver with real 2.05 rev 2, 2.17.1, 2.22 and
      2.31 under Wine, and Seihai: every step passed on all five
      (`docs/lab/2026-11-14-m3-sample-world.md`).
