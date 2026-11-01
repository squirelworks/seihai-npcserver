# Next steps

Work in this order, one branch and pull request each. The design is in
`docs/ARCHITECTURE.md` (milestone 2 plan). Rules: `AGENTS.md`.

- [x] **Wire scripts into the session.** `session::run` takes an optional
      `Scripts`. On the first `LEVEL_LIST` it reads `<world>/npcs`, calls
      `Scripts::start`, then `Scripts::tick` each tick, and logs script
      lines. Effects are drained and not sent.

- [ ] **Link messages NPC to gserver.** Specify the draft messages in
      `PROTOCOL.md` ("Draft: milestone 2") in both repos. Link version 1
      stays (the ids were reserved). Encode and decode them in `link`,
      with round-trip tests.

- [ ] **Effects to link messages.** Map `Effect` values onto those
      messages (`docs/ARCHITECTURE.md`, the effects table). Anything
      without a message is logged once and skipped.

- [ ] **Player view.** Before a player-triggered event, fill `PlayerView`
      from `world::Player` (nick, account, position). Fire `playerenters`
      and `playerleaves`. Bare `#n` is that player's nickname; `seihai-core`
      already expands it from `PlayerView.nick`. Do not add `#n(id)`. The
      engine leaves that code unchanged, and this server must not invent a
      lookup for it.

- [ ] **Mock gserver learns the new messages.** Loopback test: an NPC with
      `if (created) { timeout = 0.1; } if (timeout) { setcharprop #c,hi; timeout = 1; }`
      makes `NPC_PROPS` arrive at the mock.
