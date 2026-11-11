# Next steps

Work in this order, one branch and pull request each. The design is in
`docs/ARCHITECTURE.md` (milestone 2 plan). Rules: `AGENTS.md`.

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
      `seihai-core` expands it from `PlayerView.nick`. `#n(id)` is not
      added: the engine leaves that code unchanged, and this server has no
      id-to-nickname table (players are stored by id, which is not that
      lookup).

- [x] **Mock gserver learns the new messages.** Loopback test: an NPC with
      `if (created) { timeout = 0.1; } if (timeout) { setcharprop #c,hi; timeout = 1; }`
      makes `NPC_PROPS` arrive at the mock.

- [x] **Chat, weapons, PMs, flags, triggers.** `say2` is `PM`. `toweapons`
      and `addweapon` are `ADD_WEAPON`. `showcharacter` is the character
      NPC props. `PM_TO_NPC` runs as `playerchats` on the sender's level,
      `SERVER_FLAGS` replaces the flag set, and `TRIGGER_ACTION` fires
      `action<name>`. `#n(id)` expands from `World.players` when that id
      is online. Still unsupported, logged once, because this link has no
      message for them: `say`, `play`, `playlooped`, `play2`, `stopsound`,
      `stopmidi`, `setshape`, a script's own `triggeraction` (these since added, below),
      `sendrpgmessage`, `sendtoserver`, other commands, and player
      variables that are not a GCHAR prop (`playerrupees` and the rest).

- [x] **Sounds.** `play`, `playlooped` and `play2` are `PLAY_SOUND`,
      `stopsound` is `STOP_SOUND` and `stopmidi` is `STOP_MIDI`, each for
      the script's level (`PROTOCOL.md`, `0x48` to `0x4A`). The mock
      gserver receives them in the loopback test. `play2`'s volume is not
      carried. The `seihai-gserver` side still has to apply them.

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

- [ ] **`playerleaves` does not run.** `session` fires it, but
      `seihai-core` v0.0.5 does not list it as an event, so the engine logs
      "`playerleaves` is not an event" and `if (playerleaves)` never runs
      (seen with the mock gserver and `examples/world`). Either add the
      event in the seihai repo and bump the tag, or stop firing it and drop
      it from the docs. Owner's call.
- [ ] **GServer-v2 style NPC files.** `npcdb` reads only our own
      `<world>/npcs/<name>.txt` format. A reader for GServer-v2 world folders,
      written from the file format and not from its code (`AGENTS.md`), is
      still to do.

Milestone 3 is next (`docs/PLAN.md`): a sample world whose NPCs only work
when the NPC-server is running, lab-tested with real clients once
`seihai-gserver` applies the NPC-server's messages.
