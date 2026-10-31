//! The script host: runs server-side GS1 NPCs (milestone 2).
//!
//! One [`ScriptWorld`] from `seihai-core` per level that has server-side
//! NPCs. The tick loop in [`crate::session`] calls [`Scripts::tick`] once per
//! 0.1 s tick and turns the returned [`Effect`]s into link messages for the
//! gserver (see `docs/ARCHITECTURE.md`, "Effects to link messages").
//!
//! What works now: loading NPCs, `created`, `timeout` and collecting effects.
//! What is still TODO is marked `TODO(milestone 2)` below.

use std::collections::BTreeMap;

use seihai_core::scripting::engine::world::{ObjSpec, ScriptWorld};
use seihai_core::scripting::engine::{Effect, Era};

use crate::npcdb::NpcDef;

/// An effect and the level whose script asked for it.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelEffect {
    pub level: Vec<u8>,
    /// The NPC's id in the NPC database ([`NpcDef::id`]), when the effect
    /// came from an NPC.
    pub npc: Option<u32>,
    pub effect: Effect,
}

/// Server-side scripts for every level.
pub struct Scripts {
    era: Era,
    levels: BTreeMap<Vec<u8>, ScriptWorld>,
    started: bool,
}

impl Scripts {
    /// `era` picks the GS1 vocabulary. Server-side scripts in 2001 were
    /// 2.x scripts, so the default is [`Era::V21`].
    pub fn new(era: Era) -> Self {
        Self {
            era,
            levels: BTreeMap::new(),
            started: false,
        }
    }

    /// Add NPCs from the database. Call before [`Self::start`]; NPCs added
    /// later get `created` on the next [`Self::start`].
    pub fn load(&mut self, npcs: &[NpcDef]) {
        for npc in npcs {
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
    pub fn start(&mut self) {
        for w in self.levels.values_mut() {
            w.start_all();
        }
        self.started = true;
    }

    /// Advance every level by `ms` and collect what the scripts asked for.
    ///
    /// TODO(milestone 2): before each event, copy the triggering player's
    /// props into the world's player view (`PlayerView`), and fire
    /// `playerenters` / `playerleaves` from `PLAYER_LEVEL` and `PLAYER_LEFT`.
    /// The engine has one player view per world; a server-side event runs
    /// for one player at a time, which matches the 2001 NPC-server.
    pub fn tick(&mut self, ms: u64) -> Vec<LevelEffect> {
        let mut out = Vec::new();
        for (level, w) in self.levels.iter_mut() {
            w.tick(ms);
            for effect in w.take_effects() {
                out.push(LevelEffect {
                    level: level.clone(),
                    npc: npc_of(w, &effect),
                    effect,
                });
            }
        }
        out
    }

    /// Script log lines (parse problems, `message` debug output) since the
    /// last call, by level.
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
}

/// The database id of the NPC an effect belongs to.
fn npc_of(w: &ScriptWorld, e: &Effect) -> Option<u32> {
    let obj = match e {
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
    };
    w.obj(obj).map(|o| o.tag as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn created_runs_on_start() {
        let mut s = Scripts::new(Era::V21);
        s.load(&[npc("if (created) { message hello; }")]);
        s.start();
        let fx = s.tick(0);
        assert!(
            fx.iter().any(|e| e.npc == Some(7)
                && matches!(&e.effect, Effect::Message { text, .. } if text == "hello")),
            "{fx:?}"
        );
    }

    #[test]
    fn timeout_fires_on_the_tick() {
        let mut s = Scripts::new(Era::V21);
        s.load(&[npc(
            "if (created) { timeout = 0.1; } if (timeout) { message tock; }",
        )]);
        s.start();
        let _ = s.tick(0);
        let fx = s.tick(100);
        assert!(
            fx.iter()
                .any(|e| matches!(&e.effect, Effect::Message { text, .. } if text == "tock")),
            "{fx:?}"
        );
    }
}
