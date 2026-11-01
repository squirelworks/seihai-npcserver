//! seihai-npcserver: a Graal NPC-server, a separate program that logs into
//! seihai-gserver and runs server-side NPC scripts.
//!
//! Module map (details in `docs/ARCHITECTURE.md`):
//!
//! - [`link`]: the wire format of the gserver link (`PROTOCOL.md`). No
//!   sockets, so the gserver can share it.
//! - [`session`]: one link session: connect, authenticate, the 0.1 s tick
//!   loop.
//! - [`world`]: levels and players as the gserver described them.
//! - [`npcdb`]: server-side NPC files in the world's `npcs/` folder
//!   (milestone 2).
//! - [`scripts`]: runs those NPCs with `seihai-core`'s GS1 engine.
//! - [`effects`]: turns script effects into link messages.
//! - [`mock`]: a stand-in gserver for tests and loopback lab runs.

pub mod effects;
pub mod link;
pub mod mock;
pub mod npcdb;
pub mod scripts;
pub mod session;
pub mod world;
