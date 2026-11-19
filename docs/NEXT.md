# Roadmap status (issue #22)

Working mirror of [issue #22](https://github.com/squirelworks/seihai-npcserver/issues/22)
against `main` at `f090491` (after #45, seihai-core `v0.4.0`). Tick boxes
when the matching PR merges. Rules: `AGENTS.md`.

## Where it stands

- **Milestones 1–3 are done.** The NPC-server logs into seihai-gserver over
  the link in `PROTOCOL.md`, loads `<world>/npcs`, runs the server-side GS1
  events and sends changes back. A mock gserver covers the link. `v0.1.0` /
  `v0.1.1` are released.
- **Milestone 3 lab:** sample world `#35` (`examples/m3-world`) and the
  2026-11-14 real-client run (`#36`,
  `docs/lab/2026-11-14-m3-sample-world.md`) passed on 2.05 rev 2, 2.17.1,
  2.22, 2.31 and Seihai. Seihai still draws no `setcharprop #n` names on
  NPCs and a helmet head where real clients draw the default head (client
  note for the seihai repo, not an NPC-server defect).
- **`sendtonc` / `sendtorc`:** produced here as `0x4D` / `0x4E` (`#42`);
  gserver delivers them as `PLO_RC_CHAT` to logged-in NC/RC sessions
  (squirelworks/seihai-gserver#139); protocol note `#44`. A finer
  NC-versus-RC rights rule is still open, and no real client has been run
  for those lines.
- **Follow-through done:** NPC recovery filenames `#37`/`#39`; inbound
  link backpressure `#38`/`#41`; core pin `v0.4.0` (`#45`).

## Open checkboxes

### Code / product (non-lab)

- [ ] **NPC Control completeness:** `#31` already provides the NPC list,
      attributes read, script read/write and delete over link ids
      `0x80`–`0x84` (gserver has a matching NC integration test). Reset
      and warp are on the link as `0x85`/`0x86` (NPC-server side; the
      gserver still has to send them). Classes and weapons are served by
      the gserver from its own `scripts/` and `weapons/` folders
      (squirelworks/seihai-gserver#157). NPC add is on the link as `0x87`
      (NPC-server side; the gserver still has to send it). Still open: NPC
      flags (NC 108/110).
- [ ] **servertools database-NPC editor:** the LAT level-NPC editor does
      not edit database NPCs (seihai-servertools#50).
- [ ] **Server-side GS2:** define and implement the NPC-server
      host/event/effect integration and acceptance scope. The clean-room
      compiler and GS1 lowering (squirelworks/seihai-core#6) are related
      foundations; that issue explicitly excludes server-side GS2, so
      compiler progress alone does not close this item.
- [ ] **Finer NC-versus-RC rights** on `SEND_TO_NC` / `SEND_TO_RC`
      (today every logged-in session of that kind gets the line).

### Lab

- [ ] Real-client confirmation of `sendtonc` / `sendtorc` delivery to NC
      and RC (loopback only).

## Done record (milestones 1–3)

Kept short; the old per-item ticks lived here before this refresh.

- [x] Milestone 1 link (auth, levels, players) —
      `docs/lab/2026-10-31-milestone1.md`.
- [x] Milestone 2 scripts, effects, sounds, shape, triggers, player
      props, mock gserver round-trips (`PROTOCOL.md`, `src/effects.rs`).
- [x] `playerleaves` (`#20`, `v0.1.1`).
- [x] `GRNPC001` NPC files, level-less NPCs, `playerlogin` /
      `playerlogout` (`#21`, `#24`, `#25`).
- [x] Sample world + Milestone 3 lab (`#35`, `#36`).
- [x] `sendtonc` / `sendtorc` produce + gserver staff delivery (`#42`,
      gserver `#139`, `#44`).
- [x] Recovery-file preserve (`#39`); link read-ahead bound (`#41`).
- [x] seihai-core pins through `v0.4.0` (`#43`, `#45`).

## Not in this repo

Graal3D hosting (G3D-9) stays next to `seihai-g3d`. This binary stays on
the 2D / gen-5 gserver link.
