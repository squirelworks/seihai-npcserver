//! seihai-npcserver: a Graal NPC-server, a separate program that logs into
//! seihai-gserver and runs server-side NPC scripts.
//!
//! Milestone 1: the gserver link ([`link`], specified in `PROTOCOL.md`) and a
//! session that authenticates, keeps the level list and player props, and
//! logs them ([`session`]). [`mock`] is a small stand-in gserver for tests
//! and loopback lab runs until `seihai-gserver` has its side of the link.

pub mod link;
pub mod mock;
pub mod session;
