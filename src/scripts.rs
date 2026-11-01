//! The script host: runs server-side GS1 NPCs.
//!
//! One [`ScriptWorld`] from `seihai-core` per level that has server-side
//! NPCs. The tick loop in [`crate::session`] calls [`Scripts::tick`] once per
//! tick. Effects become link messages in [`crate::effects`].
//!
//! Bare `#n` is the acting player's nickname: `seihai-core` expands it from
//! [`PlayerView::nick`], which [`Scripts::on_player`] fills before
//! `playerenters` and `playerleaves`. `#n(id)` is left as written. The engine
//! does not expand it, and this server has no separate id-to-nickname table
//! to consult (players are stored by id on [`crate::world::World`], which is
//! not a nickname lookup).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use seihai_core::protocol::latin1;
use seihai_core::scripting::engine::world::{ObjSpec, PlayerView, ScriptWorld};
use seihai_core::scripting::engine::{Effect, Era};

use crate::effects::{self, Ctx, Outbound};
use crate::link::{show, Message};
use crate::npcdb::NpcDef;
use crate::world::Player;

/// Messages to send and lines to log from one script step.
#[derive(Debug, Default)]
pub struct Produced {
    pub messages: Vec<Message>,
    pub notes: Vec<String>,
}

impl Produced {
    fn append(&mut self, other: Produced) {
        self.messages.extend(other.messages);
        self.notes.extend(other.notes);
    }
}

/// Server-side scripts for every level.
pub struct Scripts {
    era: Era,
    /// `<world>` folder. When set, [`Self::open_database`] reads `<world>/npcs`.
    world_dir: Option<PathBuf>,
    /// [`Self::open_database`] has run, or [`Self::load`] was called directly.
    database_opened: bool,
    npc_count: usize,
    /// NPCs in load order, for `NPC_ADDED`.
    npcs: Vec<NpcDef>,
    levels: BTreeMap<Vec<u8>, ScriptWorld>,
    started: bool,
    announced: bool,
    outbound: Outbound,
}

impl Scripts {
    /// `era` picks the GS1 vocabulary. Server-side scripts in 2001 were
    /// 2.x scripts, so the default is [`Era::V21`].
    pub fn new(era: Era) -> Self {
        Self {
            era,
            world_dir: None,
            database_opened: false,
            npc_count: 0,
            npcs: Vec::new(),
            levels: BTreeMap::new(),
            started: false,
            announced: false,
            outbound: Outbound::default(),
        }
    }

    /// A host that reads `<world>/npcs` the first time [`Self::open_database`]
    /// runs. The session calls that after `LEVEL_LIST`, which is when the
    /// 2001 NPC-server loaded its database.
    pub fn with_world(era: Era, world: impl AsRef<Path>) -> Self {
        let mut s = Self::new(era);
        s.world_dir = Some(world.as_ref().to_path_buf());
        s
    }

    /// Read the NPC folder once. A second call returns the same count and
    /// no new problems. With no world folder this is a no-op, so a host
    /// filled through [`Self::load`] is left as it is.
    pub fn open_database(&mut self) -> std::io::Result<(usize, Vec<String>)> {
        if self.database_opened {
            return Ok((self.npc_count, Vec::new()));
        }
        let Some(dir) = self.world_dir.clone() else {
            self.database_opened = true;
            return Ok((self.npc_count, Vec::new()));
        };
        let (npcs, problems) = crate::npcdb::load(&dir)?;
        let n = npcs.len();
        self.load(&npcs);
        Ok((n, problems))
    }

    /// Add NPCs from the database. Call before [`Self::start`]; NPCs added
    /// later get `created` on the next [`Self::start`].
    pub fn load(&mut self, npcs: &[NpcDef]) {
        self.database_opened = true;
        self.npc_count += npcs.len();
        for npc in npcs {
            self.npcs.push(npc.clone());
            let era = self.era;
            let world = self.levels.entry(npc.level.clone()).or_insert_with(|| {
                let mut w = ScriptWorld::new(era);
                // Scripts check `isonline`/server-side state through these.
                w.set_online(true, true);
                w
            });
            world
                .add(ObjSpec::npc(&npc.image, npc.x, npc.y, &npc.script).with_tag(npc.id as usize));
        }
    }

    /// Run `created` on every NPC that has not run yet.
    ///
    /// The returned messages start with `NPC_ADDED` for each NPC, then
    /// whatever `created` asked for.
    pub fn start(&mut self) -> Produced {
        for w in self.levels.values_mut() {
            w.start_all();
        }
        self.started = true;
        let mut produced = self.announce();
        produced.append(self.collect_all(None));
        produced
    }

    /// Advance every level by `ms` and turn what the scripts asked for into
    /// link messages. `timeout` has no triggering player.
    pub fn tick(&mut self, ms: u64) -> Produced {
        let levels: Vec<Vec<u8>> = self.levels.keys().cloned().collect();
        for level in &levels {
            if let Some(w) = self.levels.get_mut(level) {
                w.tick(ms);
            }
        }
        self.collect_all(None)
    }

    /// Fill the level's player view from `player` and run `event`
    /// (`playerenters` or `playerleaves`) for that one player.
    pub fn on_player(&mut self, player_id: u16, player: &Player, event: &str) -> Produced {
        let level = player.level.clone();
        let Some(w) = self.levels.get_mut(&level) else {
            return Produced::default();
        };
        w.core.player = view_of(player);
        w.core.level = latin1(&level);
        w.broadcast(event);
        self.collect(&level, Some(player_id))
    }

    /// Script log lines (parse problems, unknown calls) since the last call,
    /// by level.
    pub fn take_log(&mut self) -> Vec<(Vec<u8>, String)> {
        let mut out = Vec::new();
        for (level, w) in self.levels.iter_mut() {
            out.extend(w.take_log().into_iter().map(|l| (level.clone(), l)));
        }
        out
    }

    /// Levels that have server-side NPCs.
    pub fn levels(&self) -> impl Iterator<Item = &[u8]> {
        self.levels.keys().map(|k| k.as_slice())
    }

    pub fn is_started(&self) -> bool {
        self.started
    }

    fn announce(&mut self) -> Produced {
        if self.announced {
            return Produced::default();
        }
        self.announced = true;
        let messages = self
            .npcs
            .iter()
            .map(|n| Message::NpcAdded {
                npc_id: n.id,
                level: n.level.clone(),
                props: effects::place_props(&n.image, n.x, n.y),
            })
            .collect();
        Produced {
            messages,
            notes: Vec::new(),
        }
    }

    fn collect_all(&mut self, player: Option<u16>) -> Produced {
        let levels: Vec<Vec<u8>> = self.levels.keys().cloned().collect();
        let mut produced = Produced::default();
        for level in levels {
            produced.append(self.collect(&level, player));
        }
        produced
    }

    fn collect(&mut self, level: &[u8], player: Option<u16>) -> Produced {
        let (hearts, player_xy, batch) = {
            let Some(w) = self.levels.get_mut(level) else {
                return Produced::default();
            };
            let hearts = w.core.player.hearts;
            let player_xy = (w.core.player.x, w.core.player.y);
            let batch: Vec<Batch> = w
                .take_effects()
                .into_iter()
                .map(|effect| {
                    let npc = npc_of(w, &effect);
                    let npc_xy = obj_of(&effect).and_then(|id| w.obj(id).map(|o| (o.x, o.y)));
                    Batch {
                        effect,
                        npc,
                        npc_xy,
                    }
                })
                .collect();
            (hearts, player_xy, batch)
        };
        let mut produced = Produced::default();
        for item in batch {
            let mapped = self.outbound.map(
                &item.effect,
                &Ctx {
                    npc: item.npc,
                    npc_xy: item.npc_xy,
                    player,
                    hearts,
                    player_xy: Some(player_xy),
                },
            );
            for note in mapped.notes {
                produced
                    .notes
                    .push(format!("script {}: {note}", show(level)));
            }
            produced.messages.extend(mapped.messages);
        }
        produced
    }
}

struct Batch {
    effect: Effect,
    npc: Option<u32>,
    npc_xy: Option<(f64, f64)>,
}

fn view_of(player: &Player) -> PlayerView {
    let mut v = PlayerView::default();
    if let Some(n) = player.nick() {
        v.nick = latin1(n);
    }
    v.account = latin1(&player.account);
    if let Some((x, y)) = player.pos() {
        v.x = x;
        v.y = y;
    }
    if let Some(d) = player.dir() {
        v.dir = d;
    }
    if let Some(h) = player.hearts() {
        v.hearts = h;
        v.hp = h;
    }
    if let Some(m) = player.max_hearts() {
        v.max_hp = m;
    }
    if let Some(c) = player.chat() {
        v.chat = c;
    }
    v
}

/// The database id of the NPC an effect belongs to.
fn npc_of(w: &ScriptWorld, e: &Effect) -> Option<u32> {
    obj_of(e).and_then(|obj| w.obj(obj).map(|o| o.tag as u32))
}

fn obj_of(e: &Effect) -> Option<usize> {
    Some(match e {
        Effect::Message { obj, .. }
        | Effect::SetImg { obj, .. }
        | Effect::SetVisible { obj, .. }
        | Effect::SetShape { obj, .. }
        | Effect::SetPos { obj, .. }
        | Effect::Move { obj, .. }
        | Effect::Destroy { obj }
        | Effect::SetCharAni { obj, .. }
        | Effect::SetCharProp { obj, .. }
        | Effect::Play { obj, .. }
        | Effect::Command { obj, .. } => *obj,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use seihai_core::protocol::npcprops::parse_npc_props;

    fn npc(script: &str) -> NpcDef {
        NpcDef {
            id: 7,
            level: b"onlinestartlocal.nw".to_vec(),
            x: 30.0,
            y: 30.0,
            image: "block.png".into(),
            script: script.into(),
        }
    }

    fn message_of(m: &Message) -> Option<String> {
        let Message::NpcProps { npc_id, props } = m else {
            return None;
        };
        let mut raw = vec![
            ((*npc_id >> 14) & 0x7F) as u8 + 0x20,
            ((*npc_id >> 7) & 0x7F) as u8 + 0x20,
            (*npc_id & 0x7F) as u8 + 0x20,
        ];
        raw.extend_from_slice(props);
        parse_npc_props(&raw, false).and_then(|(_, p)| p.message)
    }

    #[test]
    fn created_runs_on_start() {
        let mut s = Scripts::new(Era::V21);
        s.load(&[npc("if (created) { message hello; }")]);
        let produced = s.start();
        assert!(matches!(
            produced.messages[0],
            Message::NpcAdded { npc_id: 7, .. }
        ));
        assert!(
            produced
                .messages
                .iter()
                .any(|m| message_of(m).as_deref() == Some("hello")),
            "{:?}",
            produced.messages
        );
    }

    #[test]
    fn timeout_fires_on_the_tick() {
        let mut s = Scripts::new(Era::V21);
        s.load(&[npc(
            "if (created) { timeout = 0.1; } if (timeout) { setcharprop #c,hi; }",
        )]);
        let started = s.start();
        assert!(started
            .messages
            .iter()
            .all(|m| matches!(m, Message::NpcAdded { .. })));
        let produced = s.tick(100);
        assert!(
            produced
                .messages
                .iter()
                .any(|m| message_of(m).as_deref() == Some("hi")),
            "{:?}",
            produced.messages
        );
    }

    #[test]
    fn playerenters_fills_nick_account_and_position() {
        let mut s = Scripts::new(Era::V21);
        s.load(&[npc(
            "if (playerenters) { message #n|#a|#v(playerx),#v(playery); }",
        )]);
        let _ = s.start();
        let mut player = Player {
            account: b"acct".to_vec(),
            level: b"onlinestartlocal.nw".to_vec(),
            ..Player::default()
        };
        for pr in seihai_core::protocol::parse_player_props(&crate::mock::sample_props(
            b"test205", 30, 31,
        )) {
            player.props.insert(pr.id, pr.raw);
        }
        let produced = s.on_player(2, &player, "playerenters");
        let text = produced
            .messages
            .iter()
            .find_map(message_of)
            .unwrap_or_default();
        assert_eq!(text, "test205|acct|30,31", "{:?}", produced.messages);
    }

    #[test]
    fn hash_n_with_an_id_is_left_as_written() {
        let mut s = Scripts::new(Era::V21);
        s.load(&[npc("if (playerenters) { message #n(2); }")]);
        let _ = s.start();
        let player = Player {
            account: b"acct".to_vec(),
            level: b"onlinestartlocal.nw".to_vec(),
            ..Player::default()
        };
        let produced = s.on_player(2, &player, "playerenters");
        let text = produced
            .messages
            .iter()
            .find_map(message_of)
            .unwrap_or_default();
        assert_eq!(text, "#n(2)", "{:?}", produced.messages);
    }
}
