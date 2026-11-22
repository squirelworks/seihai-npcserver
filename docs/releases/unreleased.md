Notes for the next seihai-npcserver release. One line per user-visible change, added by the PR that makes it; the release PR moves them into `vX.Y.Z.md` (VERSIONING.md).

- Add ordinary NC script-set and warp write-failure, retry and reload tests for both NPC file formats, including preservation of the running script timer after refusal.

- NC warps now update position fields after script blocks as well as before them, so accepted NPC files retain the live location on reload. Exact and valued script openers keep their bodies unchanged.

- Script `setcharprop #P1` through `#P30` changes now save GANI attributes in a typed `.txt.attrs` sidecar and restore them into the runtime, NC readback and `NPC_ADDED` replay. NPC source files stay unchanged. This is partial #49; native NC attribute editing remains open.

- Built on seihai-core v0.7.2 (was v0.6.3): NewWorld beta 1/2 as real versions (with their 8-byte COLORS, which v0.7.0 had as 7), the version-token/profile audit, GS2 reading non-numeric text as -1, and the PLO_FILE mod time for 2.05 and the NewWorld betas (v0.7.2).

- NC script edits now replace the last effective script block in accepted NPC files, matching both parsers. Earlier blocks and unrelated fields are preserved, so the live edit survives a fresh load.
