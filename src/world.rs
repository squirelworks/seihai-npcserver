//! The gserver's world as the link has described it: levels and players.
//!
//! Filled by [`crate::session`] from gserver messages. From milestone 2 the
//! script host ([`crate::scripts`]) reads it to build the player view a
//! script sees, and writes player changes back through link messages
//! rather than here (the gserver owns players; see `docs/ARCHITECTURE.md`).

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

    /// Position in tiles from the classic X (15) and Y (16) props.
    ///
    /// TODO(milestone 2): prefer the precise 2.3+ props (78/79) when the
    /// gserver sends them; see `seihai_core::protocol::decode_pos_precise`.
    pub fn pos(&self) -> Option<(f64, f64)> {
        let x = *self.props.get(&15)?.first()?;
        let y = *self.props.get(&16)?.first()?;
        Some((
            f64::from(x.wrapping_sub(0x20)) / 2.0,
            f64::from(y.wrapping_sub(0x20)) / 2.0,
        ))
    }
}

/// The gserver's world as the link has described it so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct World {
    pub server_name: Vec<u8>,
    pub levels: Vec<Vec<u8>>,
    pub players: BTreeMap<u16, Player>,
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
}
