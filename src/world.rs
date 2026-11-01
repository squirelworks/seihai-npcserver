//! The gserver's world as the link has described it: levels and players.
//!
//! Filled by [`crate::session`] from gserver messages. The script host
//! ([`crate::scripts`]) reads it to build the player view a script sees.
//! Player changes go back through link messages, not through this struct
//! (the gserver owns players; see `docs/ARCHITECTURE.md`).

use std::collections::BTreeMap;

/// What the NPC-server knows about one player.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Player {
    pub account: Vec<u8>,
    pub level: Vec<u8>,
    /// Latest raw value of each prop id, as on the wire after the id byte.
    pub props: BTreeMap<u8, Vec<u8>>,
}

impl Player {
    /// The nickname (prop 0), if the gserver sent one.
    pub fn nick(&self) -> Option<&[u8]> {
        self.props.get(&0).and_then(|v| v.get(1..))
    }

    /// Position in tiles. Precise X2/Y2 (78/79) win when both are present;
    /// otherwise the classic X (15) and Y (16) half-tile props.
    pub fn pos(&self) -> Option<(f64, f64)> {
        if let Some(xy) = self.precise(78).zip(self.precise(79)) {
            return Some(xy);
        }
        let x = self.gchar(15)?;
        let y = self.gchar(16)?;
        Some((f64::from(x) / 2.0, f64::from(y) / 2.0))
    }

    /// Facing from SPRITE (17): 0 up, 1 left, 2 down, 3 right.
    pub fn dir(&self) -> Option<f64> {
        self.gchar(17).map(|s| f64::from(s % 4))
    }

    /// Hearts from CURPOWER (2), which counts half hearts.
    pub fn hearts(&self) -> Option<f64> {
        self.gchar(2).map(|h| f64::from(h) / 2.0)
    }

    /// Heart containers from MAXPOWER (1).
    pub fn max_hearts(&self) -> Option<f64> {
        self.gchar(1).map(f64::from)
    }

    /// Chat text from prop 12.
    pub fn chat(&self) -> Option<String> {
        let raw = self.props.get(&12)?;
        Some(seihai_core::protocol::latin1(raw.get(1..).unwrap_or(&[])))
    }

    fn gchar(&self, id: u8) -> Option<u8> {
        self.props.get(&id)?.first().map(|b| b.wrapping_sub(0x20))
    }

    fn precise(&self, id: u8) -> Option<f64> {
        let raw = self.props.get(&id)?;
        let v = seihai_core::protocol::read_gshort(raw)?;
        Some(f64::from(seihai_core::protocol::decode_pos_precise(v)))
    }
}

/// The gserver's world as the link has described it so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct World {
    pub server_name: Vec<u8>,
    pub levels: Vec<Vec<u8>>,
    pub players: BTreeMap<u16, Player>,
    /// The whole `server.` flag set from the latest `SERVER_FLAGS`.
    /// Names are lower case, as scripts read them.
    pub server_flags: BTreeMap<String, String>,
}

/// `SERVER_FLAGS` entries are `name=value`. The name is lower-cased. A
/// missing `=` stores an empty value.
pub fn parse_server_flags(entries: &[Vec<u8>]) -> BTreeMap<String, String> {
    let mut flags = BTreeMap::new();
    for entry in entries {
        let text = seihai_core::protocol::latin1(entry);
        let (name, value) = text.split_once('=').unwrap_or((text.as_str(), ""));
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        flags.insert(name, value.to_string());
    }
    flags
}

impl World {
    /// Players in `level`, by id.
    pub fn players_in<'a>(
        &'a self,
        level: &'a [u8],
    ) -> impl Iterator<Item = (u16, &'a Player)> + 'a {
        self.players
            .iter()
            .filter(move |(_, p)| p.level == level)
            .map(|(id, p)| (*id, p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::sample_props;
    use seihai_core::protocol::parse_player_props;

    #[test]
    fn nick_and_pos_from_props() {
        let mut p = Player::default();
        for pr in parse_player_props(&sample_props(b"test205", 30, 31)) {
            p.props.insert(pr.id, pr.raw);
        }
        assert_eq!(p.nick(), Some(&b"test205"[..]));
        assert_eq!(p.pos(), Some((30.0, 31.0)));
    }

    #[test]
    fn precise_position_wins_over_classic() {
        let mut p = Player::default();
        for pr in parse_player_props(&sample_props(b"test205", 30, 31)) {
            p.props.insert(pr.id, pr.raw);
        }
        // X2/Y2: 10.25 and 11.5 tiles, as a GSHORT on the wire.
        p.props.insert(
            78,
            gshort_wire(seihai_core::protocol::encode_pos_precise(10.25)),
        );
        p.props.insert(
            79,
            gshort_wire(seihai_core::protocol::encode_pos_precise(11.5)),
        );
        assert_eq!(p.pos(), Some((10.25, 11.5)));
        p.props.remove(&79);
        assert_eq!(p.pos(), Some((30.0, 31.0)));
    }

    fn gshort_wire(v: u16) -> Vec<u8> {
        vec![((v >> 7) as u8 & 0x7F) + 0x20, (v as u8 & 0x7F) + 0x20]
    }

    #[test]
    fn players_in_a_level() {
        let mut w = World::default();
        w.players.insert(
            1,
            Player {
                level: b"a.nw".to_vec(),
                ..Default::default()
            },
        );
        w.players.insert(
            2,
            Player {
                level: b"b.nw".to_vec(),
                ..Default::default()
            },
        );
        let ids: Vec<u16> = w.players_in(b"a.nw").map(|(id, _)| id).collect();
        assert_eq!(ids, vec![1]);
    }

    #[test]
    fn server_flag_entries_are_the_whole_set() {
        let flags = parse_server_flags(&[
            b"server.door=1".to_vec(),
            b"Server.Note=hello".to_vec(),
            b"bare".to_vec(),
        ]);
        assert_eq!(flags["server.door"], "1");
        assert_eq!(flags["server.note"], "hello");
        assert_eq!(flags["bare"], "");
        assert_eq!(flags.len(), 3);
    }
}
