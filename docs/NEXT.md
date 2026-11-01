# Next steps

Work in this order, one branch and pull request each. The design is in
`docs/ARCHITECTURE.md` (milestone 2 plan). Rules: `AGENTS.md`.

- [x] **Wire scripts into the session.** `session::run` takes an optional
      `Scripts`. On the first `LEVEL_LIST` it reads `<world>/npcs`, calls
      `Scripts::start`, then `Scripts::tick` each tick, and logs script
      lines.

- [x] **Link messages NPC to gserver.** The milestone 2 messages are in
      `PROTOCOL.md` (no longer a draft). Link version 1 stays. Encode and
      decode live in `link`, with round-trip tests. The `seihai-gserver`
      copy of `PROTOCOL.md` is still the draft; that repo was not part of
      this change.

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
      `stopmidi`, `setshape`, a script's own `triggeraction`,
      `sendrpgmessage`, `sendtoserver`, other commands, and player
      variables that are not a GCHAR prop (`playerrupees` and the rest).

Milestone 3 is next (`docs/PLAN.md`): a sample world whose NPCs only work
when the NPC-server is running, lab-tested with real clients once
`seihai-gserver` has its side of the link.
