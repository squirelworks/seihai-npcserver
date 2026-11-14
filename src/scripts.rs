//! The script host: runs server-side GS1 NPCs.
//!
//! One [`ScriptWorld`] from `seihai-core` per level that has server-side
//! NPCs, plus one with an empty name for the level-less NPCs (an empty
//! `STARTLEVEL`), which get `playerlogin` / `playerlogout` from
//! [`Scripts::on_login`]. The tick loop in [`crate::session`] calls [`Scripts::tick`] once per
//! tick. Effects become link messages in [`crate::effects`].
//!
//! Bare `#n` is the acting player's nickname: `seihai-core` expands it from
//! [`PlayerView::nick`], which [`Scripts::on_player`] fills before
//! `playerenters` and `playerleaves`. `#n(id)` is expanded here from the
//! id-to-nickname table [`Scripts::set_nicks`] builds out of
//! [`crate::world::World::players`]. An id that is not online stays written
//! as `#n(id)`. The engine itself does not expand that form.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use seihai_core::protocol::latin1;
use seihai_core::scripting::engine::world::{ObjKind, ObjSpec, PlayerView, ScriptWorld};
use seihai_core::scripting::engine::{Effect, Era, Val};

use crate::effects::{self, Ctx, Outbound};
use crate::link::{show, Message};
use crate::npcdb::NpcDef;
use crate::world::Player;

/// The world key of the level-less NPCs: an empty level name.
const LEVELLESS: &[u8] = b"";

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
    /// Latest whole `server.` flag set. Applied to every level world.
    server_flags: BTreeMap<String, String>,
    /// Script log lines taken while collecting effects, still to be logged.
    pending_logs: Vec<(Vec<u8>, String)>,
    /// Inbound notes already logged (`PM` on a level with no NPCs, and so on).
    noted: BTreeSet<String>,
}

impl Scripts {
    /// `era` picks the GS1 vocabulary. Server-side scripts in 2001 were
    /// 2.x scripts, so the default is [`Era::V217`].
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
            server_flags: BTreeMap::new(),
            pending_logs: Vec::new(),
            noted: BTreeSet::new(),
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
        self.apply_server_flags();
    }

    /// Online player id to nickname. `#n(id)` in script text uses this.
    pub fn set_nicks(&mut self, nicks: BTreeMap<u16, String>) {
        self.outbound.set_nicks(nicks);
    }

    /// Online player id to current sword and shield image.
    /// `playerswordpower` and `playershieldpower` keep the image.
    pub fn set_gear(&mut self, gear: BTreeMap<u16, crate::effects::Gear>) {
        self.outbound.set_gear(gear);
    }

    /// Replace the `server.` flag set on every level. `entries` is the whole
    /// set from `SERVER_FLAGS` (`name` to value).
    pub fn set_server_flags(&mut self, flags: BTreeMap<String, String>) {
        self.server_flags = flags;
        self.apply_server_flags();
    }

    fn apply_server_flags(&mut self) {
        for w in self.levels.values_mut() {
            w.core.server_vars.clear();
            for (name, value) in &self.server_flags {
                // A flag set with no value reads as 1, as the engine's own
                // flags do; `if (server.gateopen)` is then true.
                let v = if value.is_empty() {
                    Val::Num(1.0)
                } else {
                    Val::Str(value.clone())
                };
                w.core.server_vars.insert(name.clone(), v);
            }
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
        if level.as_slice() == LEVELLESS {
            return Produced::default();
        }
        let Some(w) = self.levels.get_mut(&level) else {
            return Produced::default();
        };
        w.core.player = view_of(player);
        w.core.level = latin1(&level);
        w.broadcast(event);
        self.collect(&level, Some(player_id))
    }

    /// `playerlogin` or `playerlogout` for `player`, on the level-less NPCs
    /// (a `GRNPC001` file with an empty `STARTLEVEL`, such as the fork's
    /// Control-NPC). Those NPCs run in their own world with no level.
    pub fn on_login(&mut self, player_id: u16, player: &Player, event: &str) -> Produced {
        let Some(w) = self.levels.get_mut(LEVELLESS) else {
            return Produced::default();
        };
        w.core.player = view_of(player);
        w.core.level = String::new();
        w.broadcast(event);
        self.collect(LEVELLESS, Some(player_id))
    }

    /// A PM from `player` to the NPC-server. The link names the sender and
    /// the text, not an NPC, so every NPC on that player's level gets
    /// `playerchats` with `#c` set to the text. The engine has no PM event.
    pub fn on_pm(&mut self, player_id: u16, player: &Player, text: &str) -> Produced {
        let level = player.level.clone();
        if level.as_slice() == LEVELLESS || !self.levels.contains_key(&level) {
            let mut produced = Produced::default();
            self.note_once(
                "pm-no-npcs",
                &format!("PM_TO_NPC on {} has no server NPCs, not run", show(&level)),
                &mut produced,
            );
            return produced;
        }
        self.chats(player_id, player, text)
    }

    /// `player` said `text` (their chat prop, 12, changed in
    /// `PLAYER_PROPS`): every NPC on their level gets `playerchats` with
    /// `#c` set to the text, as the 2001 NPC-server's level NPCs heard
    /// players talk. A level with no server NPCs is skipped quietly, since
    /// most chat is not meant for an NPC.
    pub fn on_chat(&mut self, player_id: u16, player: &Player, text: &str) -> Produced {
        if player.level.as_slice() == LEVELLESS || !self.levels.contains_key(&player.level) {
            return Produced::default();
        }
        self.chats(player_id, player, text)
    }

    fn chats(&mut self, player_id: u16, player: &Player, text: &str) -> Produced {
        let level = player.level.clone();
        {
            let Some(w) = self.levels.get_mut(&level) else {
                return Produced::default();
            };
            w.core.player = view_of(player);
            w.core.level = latin1(&level);
            w.chat(text);
        }
        self.collect(&level, Some(player_id))
    }

    /// `TRIGGER_ACTION`. `x` and `y` are tiles. Fires `action<name>` on each
    /// NPC whose touch rectangle holds that point, the same hit test the
    /// engine uses for a script's own `triggeraction`.
    pub fn on_trigger(
        &mut self,
        player_id: u16,
        player: Option<&Player>,
        level: &[u8],
        x: f64,
        y: f64,
        action: &str,
    ) -> Produced {
        let action = action.trim().to_ascii_lowercase();
        if action.is_empty() {
            let mut produced = Produced::default();
            self.note_once(
                "trigger-empty",
                "TRIGGER_ACTION has an empty action, not run",
                &mut produced,
            );
            return produced;
        }
        if level == LEVELLESS || !self.levels.contains_key(level) {
            let mut produced = Produced::default();
            self.note_once(
                "trigger-no-npcs",
                &format!(
                    "TRIGGER_ACTION on {} has no server NPCs, not run",
                    show(level)
                ),
                &mut produced,
            );
            return produced;
        }
        {
            let w = self.levels.get_mut(level).unwrap();
            if let Some(player) = player {
                w.core.player = view_of(player);
                w.core.level = latin1(level);
            }
        }
        let event = format!("action{action}");
        let hits: Vec<usize> = {
            let w = self.levels.get(level).unwrap();
            w.ids()
                .into_iter()
                .filter(|&id| {
                    w.obj(id).is_some_and(|o| {
                        if o.kind != ObjKind::Npc || o.destroyed {
                            return false;
                        }
                        let (ox, oy, wdt, h) = o.rect();
                        x >= ox && x < ox + wdt && y >= oy && y < oy + h
                    })
                })
                .collect()
        };
        if hits.is_empty() {
            let mut produced = Produced::default();
            self.note_once(
                "trigger-miss",
                &format!(
                    "TRIGGER_ACTION {event} at {x},{y} on {} hit no NPC",
                    show(level)
                ),
                &mut produced,
            );
            return produced;
        }
        {
            let w = self.levels.get_mut(level).unwrap();
            for id in hits {
                w.fire(id, &event, &[]);
            }
        }
        self.collect(level, Some(player_id))
    }

    /// Script log lines (parse problems, unknown calls) since the last call,
    /// by level. Lines consumed while turning `addweapon` into `ADD_WEAPON`
    /// are not returned.
    pub fn take_log(&mut self) -> Vec<(Vec<u8>, String)> {
        let mut out = std::mem::take(&mut self.pending_logs);
        for (level, w) in self.levels.iter_mut() {
            out.extend(w.take_log().into_iter().map(|l| (level.clone(), l)));
        }
        out
    }

    /// A level-less NPC has no level and no players watching it: its own
    /// prop changes, sounds and trigger actions have nowhere to go. Player
    /// changes, warps, weapons, PMs, flags and staff lines (`sendtonc`,
    /// `sendtorc`) still go out.
    fn drop_level_bound(&mut self, produced: &mut Produced) {
        let before = produced.messages.len();
        produced.messages.retain(|m| {
            !matches!(
                m,
                Message::NpcAdded { .. }
                    | Message::NpcProps { .. }
                    | Message::NpcDeleted { .. }
                    | Message::PlaySound { .. }
                    | Message::StopSound { .. }
                    | Message::StopMidi { .. }
                    | Message::NpcTriggerAction { .. }
            )
        });
        if produced.messages.len() != before {
            self.note_once(
                "levelless-dropped",
                "a level-less NPC changed itself, played a sound or sent a trigger action; \
                 with no level these are not sent",
                produced,
            );
        }
    }

    fn note_once(&mut self, key: &str, line: &str, produced: &mut Produced) {
        if self.noted.insert(key.to_string()) {
            produced.notes.push(line.to_string());
        }
    }

    /// Levels that have server-side NPCs.
    pub fn levels(&self) -> impl Iterator<Item = &[u8]> {
        self.levels
            .keys()
            .map(|k| k.as_slice())
            .filter(|k| *k != LEVELLESS)
    }

    pub fn is_started(&self) -> bool {
        self.started
    }

    fn announce(&mut self) -> Produced {
        if self.announced {
            return Produced::default();
        }
        self.announced = true;
        let messages: Vec<Message> = self
            .npcs
            .iter()
            // A level-less NPC is not in any level, so no player sees it.
            .filter(|n| !n.level.is_empty())
            .map(added)
            .collect();
        Produced {
            messages,
            notes: Vec::new(),
        }
    }

    /// NPC Control: the NPC list, an `NC_NPC` for every NPC in the
    /// database. The session sends it once the scripts start.
    pub fn nc_list(&self) -> Vec<Message> {
        self.npcs.iter().map(nc_npc).collect()
    }

    /// NPC Control: an NPC's script or attributes (`NC_GET`).
    pub fn nc_get(&self, request: u32, npc_id: u32, what: u8) -> Message {
        let Some(n) = self.npcs.iter().find(|n| n.id == npc_id) else {
            return Message::NcText {
                request,
                found: false,
                text: Vec::new(),
            };
        };
        let text = if what == crate::link::nc_get::SCRIPT {
            latin1_bytes(&n.script)
        } else {
            let mut t = format!(
                "name={}\nlevel={}\nx={}\ny={}\nimage={}\n",
                n.name,
                show(&n.level),
                n.x,
                n.y,
                n.image
            );
            for (k, v) in &n.props {
                t.push_str(&format!("{}={v}\n", k.to_ascii_lowercase()));
            }
            latin1_bytes(&t)
        };
        Message::NcText {
            request,
            found: true,
            text,
        }
    }

    /// NPC Control: a new script for an NPC (`NC_SCRIPT_SET`). The NPC's
    /// file gets it, and the NPCs of its level start again (their
    /// `created` runs once more), so the new script runs at once.
    pub fn nc_set_script(&mut self, npc_id: u32, script: &str) -> Result<Produced, String> {
        let n = self
            .npcs
            .iter_mut()
            .find(|n| n.id == npc_id)
            .ok_or_else(|| format!("no NPC {npc_id}"))?;
        if let Some(path) = &n.path {
            let text = String::from_utf8_lossy(
                &std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?,
            )
            .into_owned();
            let new = crate::npcdb::with_script(&text, n.format, script);
            write_file(path, &new).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        n.script = script.to_string();
        let level = n.level.clone();
        Ok(self.restart_level(&level))
    }

    /// NPC Control: take an NPC out (`NC_NPC_DELETE`). Its file is kept as
    /// `.txt.deleted`, which the database does not read, so it can be put
    /// back by hand. An existing recovery file is never replaced.
    pub fn nc_delete(&mut self, npc_id: u32) -> Result<Produced, String> {
        let i = self
            .npcs
            .iter()
            .position(|n| n.id == npc_id)
            .ok_or_else(|| format!("no NPC {npc_id}"))?;
        if let Some(path) = &self.npcs[i].path {
            let gone = path.with_extension("txt.deleted");
            // A rename may replace an earlier recovery. A hard link creates
            // the recovery name without replacement, including when another
            // request creates it concurrently. Only then remove the live name.
            std::fs::hard_link(path, &gone)
                .map_err(|e| format!("{}: recovery {}: {e}", path.display(), gone.display()))?;
            std::fs::remove_file(path).map_err(|e| {
                format!(
                    "{}: could not remove original; recovery kept at {}: {e}",
                    path.display(),
                    gone.display()
                )
            })?;
        }
        let n = self.npcs.remove(i);
        self.npc_count = self.npc_count.saturating_sub(1);
        let mut produced = Produced::default();
        if !n.level.is_empty() {
            produced.messages.push(Message::NpcDeleted { npc_id });
        }
        produced.append(self.restart_level(&n.level));
        // Last, so NPC Control drops it from its list only once it is
        // gone. `NPC_DELETED` alone does not say so: a restart sends it too.
        produced.messages.push(Message::NcNpcDelete { npc_id });
        Ok(produced)
    }

    /// Build `level`'s script world again from the database and, once the
    /// scripts run, start it. The players see the level's NPCs go and come
    /// back with their new state.
    fn restart_level(&mut self, level: &[u8]) -> Produced {
        let mut produced = Produced::default();
        let mine: Vec<NpcDef> = self
            .npcs
            .iter()
            .filter(|n| n.level == level)
            .cloned()
            .collect();
        if !level.is_empty() && self.announced {
            for n in &mine {
                produced.messages.push(Message::NpcDeleted { npc_id: n.id });
            }
        }
        if mine.is_empty() {
            self.levels.remove(level);
            return produced;
        }
        let mut w = ScriptWorld::new(self.era);
        w.set_online(true, true);
        for n in &mine {
            w.add(ObjSpec::npc(&n.image, n.x, n.y, &n.script).with_tag(n.id as usize));
        }
        self.levels.insert(level.to_vec(), w);
        self.apply_server_flags();
        if self.announced {
            for n in &mine {
                if !level.is_empty() {
                    produced.messages.push(added(n));
                }
                produced.messages.push(nc_npc(n));
            }
        }
        if self.started {
            if let Some(w) = self.levels.get_mut(level) {
                w.start_all();
            }
            produced.append(self.collect(level, None));
        }
        produced
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
                    level: level.to_vec(),
                },
            );
            for note in mapped.notes {
                produced
                    .notes
                    .push(format!("script {}: {note}", show(level)));
            }
            produced.messages.extend(mapped.messages);
        }
        if level == LEVELLESS {
            self.drop_level_bound(&mut produced);
        }
        // `addweapon` is not a command in seihai-core, so the engine logs it
        // as an unknown command and keeps the argument text. That text is
        // the weapon name `ADD_WEAPON` carries.
        let logs = self
            .levels
            .get_mut(level)
            .map(|w| w.take_log())
            .unwrap_or_default();
        for line in logs {
            if let Some(weapon) = unknown_addweapon(&line) {
                let mapped = self.outbound.map(
                    &Effect::Command {
                        obj: 0,
                        name: "addweapon".into(),
                        args: vec![Val::from(weapon)],
                    },
                    &Ctx {
                        npc: None,
                        npc_xy: None,
                        player,
                        hearts,
                        player_xy: Some(player_xy),
                        level: level.to_vec(),
                    },
                );
                for note in mapped.notes {
                    produced
                        .notes
                        .push(format!("script {}: {note}", show(level)));
                }
                produced.messages.extend(mapped.messages);
            } else {
                self.pending_logs.push((level.to_vec(), line));
            }
        }
        self.own_server_flags(&produced.messages);
        produced
    }

    /// A `server.` flag a script set or unset is seen by every level's
    /// scripts at once, not only after the gserver sends `SERVER_FLAGS`
    /// back: a gate on another level (or later in this tick) reads it
    /// straight away. The gserver's next `SERVER_FLAGS` still replaces the
    /// whole set.
    fn own_server_flags(&mut self, messages: &[Message]) {
        let mut changed = false;
        for m in messages {
            if let Message::ServerFlag { name, set, value } = m {
                let name = latin1(name).trim().to_ascii_lowercase();
                if *set {
                    self.server_flags.insert(name, latin1(value));
                } else {
                    self.server_flags.remove(&name);
                }
                changed = true;
            }
        }
        if changed {
            self.apply_server_flags();
        }
    }
}

/// `NPC_ADDED` for a database NPC.
fn added(n: &NpcDef) -> Message {
    Message::NpcAdded {
        npc_id: n.id,
        level: n.level.clone(),
        props: effects::place_props(&n.image, n.x, n.y),
    }
}

/// `NC_NPC` for a database NPC: its name, `TYPE` (empty when none) and
/// level.
fn nc_npc(n: &NpcDef) -> Message {
    Message::NcNpc {
        npc_id: n.id,
        name: latin1_bytes(&n.name),
        kind: latin1_bytes(n.kind().unwrap_or("")),
        level: n.level.clone(),
    }
}

/// Text as Latin-1 bytes; characters past U+00FF become `?`.
fn latin1_bytes(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
        .collect()
}

/// Write through a temporary file, so a failed write keeps the old one.
fn write_file(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("txt.new");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// `object N: line L: `addweapon Name` ignored (unknown command)`.
fn unknown_addweapon(line: &str) -> Option<String> {
    let marker = "`addweapon ";
    let start = line.find(marker)? + marker.len();
    let rest = line.get(start..)?;
    let end = rest.find('`')?;
    let tail = rest.get(end + 1..)?;
    if !tail.contains("ignored (unknown command)") {
        return None;
    }
    let name = rest[..end].split(',').next().unwrap_or("").trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
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
    if let Some(r) = player.rupees() {
        v.rupees = r;
    }
    if let Some(b) = player.bombs() {
        v.bombs = b;
    }
    if let Some(d) = player.darts() {
        v.darts = d;
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
    use std::collections::BTreeMap;

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
            ..NpcDef::default()
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
        let mut s = Scripts::new(Era::V217);
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
        let mut s = Scripts::new(Era::V217);
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
        let mut s = Scripts::new(Era::V217);
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
    fn playerleaves_runs() {
        // seihai-core v0.0.6 lists `playerleaves` as a 2.1-era event
        // (seihai #390); with v0.0.5 this block never ran.
        let mut s = Scripts::new(Era::V217);
        s.load(&[npc("if (playerleaves) { message bye #n; }")]);
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
        let produced = s.on_player(2, &player, "playerleaves");
        let text = produced
            .messages
            .iter()
            .find_map(message_of)
            .unwrap_or_default();
        assert_eq!(text, "bye test205", "{:?}", produced.messages);
    }

    #[test]
    fn hash_n_with_an_id_expands_when_that_player_is_online() {
        let mut s = Scripts::new(Era::V217);
        s.set_nicks(BTreeMap::from([(2, "test205".into())]));
        s.load(&[npc("if (playerenters) { message #n(2) #n(9); }")]);
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
        assert_eq!(text, "test205 #n(9)", "{:?}", produced.messages);
    }

    #[test]
    fn say2_toweapons_showcharacter_and_addweapon_are_sent() {
        let mut s = Scripts::new(Era::V217);
        s.set_nicks(BTreeMap::from([(2, "test205".into())]));
        s.load(&[npc(
            "if (playerenters) { say2 Hello #n(2); toweapons Bow; showcharacter; play jump.wav; setshape 1,32,32; sendrpgmessage hi; addweapon Sword; }",
        )]);
        let _ = s.start();
        let player = Player {
            account: b"acct".to_vec(),
            level: b"onlinestartlocal.nw".to_vec(),
            ..Player::default()
        };
        let produced = s.on_player(2, &player, "playerenters");
        assert!(
            produced.messages.iter().any(|m| matches!(
                m,
                Message::Pm { player_id: 2, text } if text == b"Hello test205"
            )),
            "{:?}",
            produced.messages
        );
        assert!(
            produced.messages.iter().any(|m| matches!(
                m,
                Message::AddWeapon { player_id: 2, weapon } if weapon == b"Bow"
            )),
            "{:?}",
            produced.messages
        );
        assert!(
            produced.messages.iter().any(|m| matches!(
                m,
                Message::AddWeapon { player_id: 2, weapon } if weapon == b"Sword"
            )),
            "{:?}",
            produced.messages
        );
        assert!(
            produced
                .messages
                .iter()
                .any(|m| { message_image_gani(m) == Some(("#c#".into(), "idle".into())) }),
            "{:?}",
            produced.messages
        );
        assert!(
            produced
                .notes
                .iter()
                .any(|n| n.contains("sendrpgmessage") && n.contains("unsupported")),
            "{:?}",
            produced.notes
        );
        assert!(
            produced.messages.iter().any(|m| matches!(
                m,
                Message::NpcShape {
                    npc_id: 7,
                    kind: 1,
                    w: 32,
                    h: 32
                }
            )),
            "{:?}",
            produced.messages
        );
        assert!(
            produced.messages.iter().any(|m| matches!(
                m,
                Message::PlaySound { file, looped: false, .. } if file == b"jump.wav"
            )),
            "{:?}",
            produced.messages
        );
        let again = s.on_player(2, &player, "playerenters");
        assert!(
            again.notes.iter().all(|n| !n.contains("sendrpgmessage")),
            "sendrpgmessage should be logged once: {:?}",
            again.notes
        );
    }

    #[test]
    fn grnpc_files_come_up_and_level_less_ones_load() {
        let dir = std::env::temp_dir().join(format!("seihai-grnpc-world-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("npcs")).unwrap();
        std::fs::write(
            dir.join("npcs/npcControl-NPC.txt"),
            "GRNPC001\r\nNAME Control-NPC\r\nID 10000\r\nTYPE CONTROL\r\nSTARTLEVEL \r\nNPCSCRIPT\r\nif (playerlogin) { message x; }\r\nNPCSCRIPTEND\r\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("npcs/npcSign.txt"),
            "GRNPC001\nNAME Sign\nIMAGE sign.png\nSTARTLEVEL start.nw\nSTARTX 5\nSTARTY 6\nNPCSCRIPT\nif (created) { message read me; }\nNPCSCRIPTEND\n",
        )
        .unwrap();
        let mut s = Scripts::with_world(Era::V217, &dir);
        let (count, problems) = s.open_database().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(count, 2);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(s.levels().collect::<Vec<_>>(), vec![&b"start.nw"[..]]);
        let started = s.start();
        assert!(
            !started
                .messages
                .iter()
                .any(|m| matches!(m, Message::NpcAdded { level, .. } if level.is_empty())),
            "a level-less NPC is not announced: {:?}",
            started.messages
        );
        assert!(started.messages.iter().any(|m| matches!(
            m,
            Message::NpcAdded { npc_id: 2, level, .. } if level == b"start.nw"
        )));
        assert!(started
            .messages
            .iter()
            .filter_map(message_of)
            .any(|t| t.contains("read me")));
    }

    #[test]
    fn npc_control_reads_sets_and_deletes() {
        let dir = std::env::temp_dir().join(format!("seihai-nc-world-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("npcs")).unwrap();
        std::fs::write(
            dir.join("npcs/bell.txt"),
            "LEVEL start.nw\nX 5\nY 6\nIMAGE bell.png\nSCRIPT\nif (created) { message ding; }\nSCRIPTEND\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("npcs/npcControl.txt"),
            "GRNPC001\nNAME Control\nTYPE CONTROL\nSTARTLEVEL \nNPCSCRIPT\nif (playerlogin) { message x; }\nNPCSCRIPTEND\n",
        )
        .unwrap();
        let mut s = Scripts::with_world(Era::V217, &dir);
        s.open_database().unwrap();
        s.start();
        // The NC list: both NPCs, with name, type and level.
        let list: Vec<_> = s
            .nc_list()
            .iter()
            .filter_map(|m| match m {
                Message::NcNpc {
                    npc_id,
                    name,
                    kind,
                    level,
                } => Some((*npc_id, show(name), show(kind), show(level))),
                _ => None,
            })
            .collect();
        assert_eq!(
            list,
            [
                (1, "bell".into(), "".into(), "start.nw".into()),
                (2, "Control".into(), "CONTROL".into(), "".into())
            ]
        );
        let text = |m: Message| match m {
            Message::NcText { found, text, .. } => {
                (found, String::from_utf8_lossy(&text).into_owned())
            }
            other => panic!("{other:?}"),
        };
        assert_eq!(
            text(s.nc_get(7, 1, crate::link::nc_get::SCRIPT)),
            (true, "if (created) { message ding; }".into())
        );
        let (found, attrs) = text(s.nc_get(8, 2, crate::link::nc_get::ATTRIBUTES));
        assert!(
            found && attrs.contains("name=Control\n") && attrs.contains("type=CONTROL\n"),
            "{attrs}"
        );
        assert!(!text(s.nc_get(9, 99, 0)).0);

        // A new script: in the file, and running at once.
        let set = s
            .nc_set_script(1, "if (created) { message dong; }")
            .unwrap();
        assert!(set
            .messages
            .iter()
            .any(|m| matches!(m, Message::NpcDeleted { npc_id: 1 })));
        assert!(set
            .messages
            .iter()
            .any(|m| matches!(m, Message::NpcAdded { npc_id: 1, .. })));
        assert!(set
            .messages
            .iter()
            .filter_map(message_of)
            .any(|t| t.contains("dong")));
        let file = std::fs::read_to_string(dir.join("npcs/bell.txt")).unwrap();
        assert!(
            file.starts_with("LEVEL start.nw\nX 5\n") && file.contains("message dong;"),
            "{file}"
        );
        assert!(s.nc_set_script(99, "x").is_err());

        // Deleted: gone from the players and the list, the file put aside.
        let del = s.nc_delete(1).unwrap();
        assert!(del
            .messages
            .iter()
            .any(|m| matches!(m, Message::NpcDeleted { npc_id: 1 })));
        assert_eq!(
            del.messages.last(),
            Some(&Message::NcNpcDelete { npc_id: 1 })
        );
        assert!(!dir.join("npcs/bell.txt").exists());
        assert!(dir.join("npcs/bell.txt.deleted").exists());
        assert!(!text(s.nc_get(10, 1, 0)).0);
        assert_eq!(s.levels().count(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn npc_control_delete_preserves_an_existing_recovery_file() {
        let (dir, mut s) = delete_world("recovery-file");
        let path = dir.join("npcs/bell.txt");
        let before = std::fs::read(&path).unwrap();
        let gone = path.with_extension("txt.deleted");
        std::fs::write(&gone, b"earlier recovery").unwrap();
        let list = s.nc_list();
        let script = s.nc_get(1, 1, crate::link::nc_get::SCRIPT);

        assert!(s.nc_delete(1).is_err());
        assert_eq!(std::fs::read(&gone).unwrap(), b"earlier recovery");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(s.nc_list(), list);
        assert_eq!(s.nc_get(1, 1, crate::link::nc_get::SCRIPT), script);
        assert_eq!(s.npc_count, 1);
        assert_eq!(s.levels().count(), 1);

        // Removing the collision lets the same request succeed without a reload.
        std::fs::remove_file(&gone).unwrap();
        let deleted = s.nc_delete(1).unwrap();
        assert_eq!(std::fs::read(&gone).unwrap(), before);
        assert!(!path.exists());
        assert_eq!(
            deleted.messages.last(),
            Some(&Message::NcNpcDelete { npc_id: 1 })
        );
        assert!(s.nc_list().is_empty());
        assert_eq!(s.npc_count, 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn npc_control_delete_storage_errors_keep_the_npc() {
        for missing_source in [false, true] {
            let (dir, mut s) = delete_world(if missing_source {
                "missing-source"
            } else {
                "recovery-dir"
            });
            let path = dir.join("npcs/bell.txt");
            let before = std::fs::read(&path).unwrap();
            let gone = path.with_extension("txt.deleted");
            if missing_source {
                std::fs::remove_file(&path).unwrap();
            } else {
                std::fs::create_dir(&gone).unwrap();
            }
            let list = s.nc_list();
            let script = s.nc_get(1, 1, crate::link::nc_get::SCRIPT);
            assert!(s.nc_delete(1).is_err());
            assert_eq!(s.nc_list(), list);
            assert_eq!(s.nc_get(1, 1, crate::link::nc_get::SCRIPT), script);
            assert_eq!(s.npc_count, 1);
            assert_eq!(s.levels().count(), 1);
            if missing_source {
                assert!(!gone.exists());
            } else {
                assert!(gone.is_dir());
                assert_eq!(std::fs::read(&path).unwrap(), before);
            }
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    fn delete_world(tag: &str) -> (PathBuf, Scripts) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "seihai-nc-delete-{tag}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(dir.join("npcs")).unwrap();
        std::fs::write(dir.join("npcs/bell.txt"), "LEVEL start.nw\nX 5\nY 6\nIMAGE bell.png\nSCRIPT\nif (created) { message ding; }\nSCRIPTEND\n").unwrap();
        let mut s = Scripts::with_world(Era::V217, &dir);
        s.open_database().unwrap();
        s.start();
        (dir, s)
    }

    #[test]
    fn level_less_npcs_run_and_send_only_what_has_somewhere_to_go() {
        let mut s = Scripts::new(Era::V217);
        s.load(&[NpcDef {
            level: Vec::new(),
            ..npc("if (created) { set server.control; message hi; timeout = 0.05; }\nif (timeout) { set server.ticked; }\nif (playerenters) { set server.wrong; }")
        }]);
        let started = s.start();
        assert!(
            started
                .messages
                .iter()
                .all(|m| matches!(m, Message::ServerFlag { .. })),
            "only the flag goes out: {:?}",
            started.messages
        );
        assert_eq!(started.messages.len(), 1, "{:?}", started.messages);
        assert!(
            started.notes.iter().any(|n| n.contains("level-less")),
            "{:?}",
            started.notes
        );
        let ticked = s.tick(100);
        assert_eq!(ticked.messages.len(), 1, "{:?}", ticked.messages);
        // `playerenters` is a level event: a player with no level does not
        // reach the level-less NPCs through it.
        let nobody = Player::default();
        assert!(s.on_player(2, &nobody, "playerenters").messages.is_empty());
    }

    /// seihai-core v0.0.7 lists `playerlogin` and `playerlogout` (seihai
    /// #416), so a level-less NPC's blocks run for each player.
    #[test]
    fn level_less_npcs_get_playerlogin_and_playerlogout() {
        let mut s = Scripts::new(Era::V217);
        s.load(&[NpcDef {
            level: Vec::new(),
            ..npc("if (playerlogin) { set server.in_#a; }\nif (playerlogout) { set server.out_#a; }\nif (playerenters) { set server.wrong; }")
        }]);
        let _ = s.start();
        let player = Player {
            account: b"acct".to_vec(),
            level: b"onlinestartlocal.nw".to_vec(),
            ..Player::default()
        };
        let flag = |p: &Produced| -> Vec<String> {
            p.messages
                .iter()
                .filter_map(|m| match m {
                    Message::ServerFlag { name, .. } => {
                        Some(String::from_utf8_lossy(name).into_owned())
                    }
                    _ => None,
                })
                .collect()
        };
        let login = s.on_login(2, &player, "playerlogin");
        assert_eq!(flag(&login), vec!["server.in_acct"], "{:?}", login.messages);
        // The player's level has no NPCs; the level-less one is not in it.
        assert!(s.on_player(2, &player, "playerenters").messages.is_empty());
        let logout = s.on_login(2, &player, "playerlogout");
        assert_eq!(
            flag(&logout),
            vec!["server.out_acct"],
            "{:?}",
            logout.messages
        );
    }

    /// A level-less Control-NPC's `sendtorc` / `sendtonc` are staff lines,
    /// not level props, so they go out. Its own `message` still does not.
    #[test]
    fn level_less_staff_lines_go_out() {
        let mut s = Scripts::new(Era::V217);
        s.load(&[NpcDef {
            level: Vec::new(),
            ..npc("if (created) { sendtonc started; message hi; }\n\
                 if (playerlogin) { sendtorc #a has logged in!; }")
        }]);
        let started = s.start();
        assert!(
            started.messages.iter().any(|m| matches!(
                m,
                Message::SendToNc { text } if text == b"started"
            )),
            "{:?}",
            started.messages
        );
        assert!(
            started
                .messages
                .iter()
                .all(|m| matches!(m, Message::SendToNc { .. })),
            "the message prop is dropped: {:?}",
            started.messages
        );
        assert!(
            started.notes.iter().any(|n| n.contains("level-less")),
            "{:?}",
            started.notes
        );
        let player = Player {
            account: b"acct".to_vec(),
            level: b"onlinestartlocal.nw".to_vec(),
            ..Player::default()
        };
        let login = s.on_login(2, &player, "playerlogin");
        assert!(
            login.messages.iter().any(|m| matches!(
                m,
                Message::SendToRc { text } if text == b"acct has logged in!"
            )),
            "{:?}",
            login.messages
        );
        assert!(
            !login
                .notes
                .iter()
                .any(|n| n.contains("sendtorc") || n.contains("sendtonc")),
            "{:?}",
            login.notes
        );
    }

    #[test]
    fn a_scripts_triggeraction_reaches_the_npcs_under_the_point() {
        let mut s = Scripts::new(Era::V217);
        let bell = NpcDef {
            id: 8,
            x: 10.0,
            y: 10.0,
            script: "if (actionring) { message rang #p(0)-#p(1); }".into(),
            ..npc("")
        };
        s.load(&[
            npc("if (created) { triggeraction 10.5,10.5,Ring,red,door; }"),
            bell,
        ]);
        let started = s.start();
        assert!(
            started.messages.iter().any(|m| matches!(
                m,
                Message::NpcTriggerAction { action, params, .. }
                    if action == b"ring" && params == b"red,door"
            )),
            "{:?}",
            started.messages
        );
        let texts: Vec<String> = started.messages.iter().filter_map(message_of).collect();
        assert_eq!(texts, ["rang red-door"], "{:?}", started.messages);
    }

    #[test]
    fn a_triggeraction_that_answers_itself_is_cut_off() {
        let mut s = Scripts::new(Era::V217);
        s.load(&[npc(
            "if (created) { triggeraction 30,30,ring,; }\nif (actionring) { triggeraction 30,30,ring,; }",
        )]);
        let started = s.start();
        let sent = started
            .messages
            .iter()
            .filter(|m| matches!(m, Message::NpcTriggerAction { .. }))
            .count();
        // One from `created`, then one per queued call until the cap.
        assert!((2..=1001).contains(&sent), "{sent}");
        assert!(
            s.take_log().iter().any(|(_, l)| l.contains("cut off")),
            "the engine should log the cut-off"
        );
    }

    #[test]
    fn pm_flags_and_trigger_run() {
        let mut s = Scripts::new(Era::V217);
        s.load(&[npc(
            "if (playerchats) { message #c/#v(server.door); }\nif (actionopen) { message triggered; }",
        )]);
        let mut flags = BTreeMap::new();
        flags.insert("server.door".into(), "1".into());
        s.set_server_flags(flags);
        let _ = s.start();
        let player = Player {
            account: b"acct".to_vec(),
            level: b"onlinestartlocal.nw".to_vec(),
            ..Player::default()
        };
        let pm = s.on_pm(2, &player, "hi npc");
        let text = pm.messages.iter().find_map(message_of).unwrap_or_default();
        assert_eq!(text, "hi npc/1", "{:?}", pm.messages);

        let hit = s.on_trigger(2, Some(&player), b"onlinestartlocal.nw", 30.0, 30.0, "open");
        let text = hit.messages.iter().find_map(message_of).unwrap_or_default();
        assert_eq!(text, "triggered", "{:?}", hit.messages);

        let miss = s.on_trigger(2, Some(&player), b"onlinestartlocal.nw", 1.0, 1.0, "open");
        assert!(miss.messages.is_empty(), "{:?}", miss.messages);
        assert!(
            miss.notes.iter().any(|n| n.contains("hit no NPC")),
            "{:?}",
            miss.notes
        );
    }

    fn message_image_gani(m: &Message) -> Option<(String, String)> {
        let Message::NpcProps { npc_id, props } = m else {
            return None;
        };
        let mut raw = vec![
            ((*npc_id >> 14) & 0x7F) as u8 + 0x20,
            ((*npc_id >> 7) & 0x7F) as u8 + 0x20,
            (*npc_id & 0x7F) as u8 + 0x20,
        ];
        raw.extend_from_slice(props);
        let (_, p) = parse_npc_props(&raw, false)?;
        Some((p.image?, p.gani?))
    }
}
