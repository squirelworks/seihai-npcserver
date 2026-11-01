//! Script [`Effect`]s as NPC-server link messages.
//!
//! The table is `docs/ARCHITECTURE.md`, "Effects to link messages". An effect
//! with no message is logged once and skipped. NPC prop bytes are the
//! `PLO_NPCPROPS` stream after the NPC id (`seihai-core` `protocol::npcprops`).
//! Player prop bytes are a `(GCHAR id, value)*` stream, the same shape as
//! `PLAYER_PROPS`.

use std::collections::{BTreeMap, BTreeSet};

use seihai_core::looks::{self, DEFAULT_COLORS};
use seihai_core::protocol::npcprops::{
    half_tiles, npc_props_message, Head, PropValue, DEFAULT_NPC_COLORS, NPCPROP_BODY,
    NPCPROP_COLORS, NPCPROP_GANI, NPCPROP_GATTRIB1, NPCPROP_GATTRIB10, NPCPROP_GATTRIB6,
    NPCPROP_HEAD, NPCPROP_IMAGE, NPCPROP_MESSAGE, NPCPROP_NICKNAME, NPCPROP_VISFLAGS, NPCPROP_X,
    NPCPROP_Y, VIS_VISIBLE,
};
use seihai_core::scripting::engine::Effect;

use crate::link::Message;

/// What the session knows about the effect besides the effect itself.
pub struct Ctx {
    /// Database id of the NPC the effect came from.
    pub npc: Option<u32>,
    /// That NPC's position after the effect, in tiles.
    pub npc_xy: Option<(f64, f64)>,
    /// The player the event ran for. `None` for `created` and `timeout`.
    pub player: Option<u16>,
    /// Hearts after the event (`hurt` already changed the view).
    pub hearts: f64,
    /// Where a `setlevel` with no coordinates should put the player.
    pub player_xy: Option<(f64, f64)>,
}

/// Link messages for one effect, plus lines to log (each skip once).
#[derive(Debug, Default)]
pub struct Mapped {
    pub messages: Vec<Message>,
    pub notes: Vec<String>,
}

/// Remembers which unmapped effects have already been logged.
#[derive(Debug, Default)]
pub struct Outbound {
    skipped: BTreeSet<String>,
    npc_colors: BTreeMap<u32, [u8; 5]>,
    player_colors: BTreeMap<u16, [u8; 5]>,
}

impl Outbound {
    pub fn map(&mut self, effect: &Effect, ctx: &Ctx) -> Mapped {
        match effect {
            Effect::SetPos { x, y, .. } => self.npc_move(ctx, *x, *y, "setpos"),
            Effect::Move { .. } => match ctx.npc_xy {
                Some((x, y)) => self.npc_move(ctx, x, y, "move"),
                None => self.skip("move-no-pos", "move has no NPC position, not sent"),
            },
            Effect::SetImg { file, part, .. } => {
                let mut mapped = self.npc_props(
                    ctx,
                    vec![(NPCPROP_IMAGE, PropValue::Text(file.clone()))],
                    "setimg",
                );
                if part.is_some() {
                    mapped.notes.extend(self.once(
                        "setimg-part",
                        "setimg part is not an NPC prop; only the image is sent",
                    ));
                }
                mapped
            }
            Effect::SetVisible { visible, local, .. } => {
                let mut mapped = self.npc_props(
                    ctx,
                    vec![(
                        NPCPROP_VISFLAGS,
                        PropValue::Byte(if *visible { VIS_VISIBLE } else { 0 }),
                    )],
                    "setvisible",
                );
                if *local {
                    mapped.notes.extend(self.once(
                        "setvisible-local",
                        "showlocal/hidelocal is sent as a level-wide NPC_PROPS vis flag",
                    ));
                }
                mapped
            }
            Effect::SetCharAni { ani, params, .. } => {
                let mut mapped = self.npc_props(
                    ctx,
                    vec![(NPCPROP_GANI, PropValue::Text(ani.trim().to_string()))],
                    "setcharani",
                );
                if !params.is_empty() {
                    mapped.notes.extend(self.once(
                        "setcharani-params",
                        "setcharani parameters are not an NPC prop; only the gani name is sent",
                    ));
                }
                mapped
            }
            Effect::SetCharProp { code, value, .. } => self.char_prop(ctx, code, value),
            Effect::Message { text, .. } => self.npc_props(
                ctx,
                vec![(NPCPROP_MESSAGE, PropValue::Text(text.clone()))],
                "message",
            ),
            Effect::Destroy { .. } => match ctx.npc {
                Some(npc_id) => Mapped::one(Message::NpcDeleted { npc_id }),
                None => self.skip("destroy-no-npc", "destroy has no NPC id, not sent"),
            },
            Effect::SetPlayerProp { code, value } => self.player_prop(ctx, code, value),
            Effect::Hurt { .. } => match ctx.player {
                Some(player_id) => {
                    let cur = (ctx.hearts * 2.0).round().clamp(0.0, 223.0) as u8;
                    Mapped::one(Message::SetPlayerProps {
                        player_id,
                        props: prop_byte(2, cur),
                    })
                }
                None => self.skip("hurt-no-player", "hurt has no triggering player, not sent"),
            },
            Effect::SetLevel { level, x, y } => self.warp(ctx, level, *x, *y),
            Effect::SetFlag { name, value } if is_server_flag(name) => {
                self.flag(name, true, value.clone().unwrap_or_default())
            }
            Effect::UnsetFlag { name } if is_server_flag(name) => {
                self.flag(name, false, String::new())
            }
            other => {
                let key = skip_key(other);
                let line = format!("effect {key} has no link message, skipped");
                self.skip(&key, &line)
            }
        }
    }

    fn npc_move(&mut self, ctx: &Ctx, x: f64, y: f64, kind: &str) -> Mapped {
        self.npc_props(
            ctx,
            vec![
                (NPCPROP_X, PropValue::Byte(half_tiles(x as f32))),
                (NPCPROP_Y, PropValue::Byte(half_tiles(y as f32))),
            ],
            kind,
        )
    }

    fn npc_props(&mut self, ctx: &Ctx, props: Vec<(u8, PropValue)>, kind: &str) -> Mapped {
        match ctx.npc {
            Some(npc_id) => Mapped::one(npc_props_msg(npc_id, props)),
            None => self.skip(
                &format!("{kind}-no-npc"),
                &format!("{kind} has no NPC id, not sent"),
            ),
        }
    }

    fn char_prop(&mut self, ctx: &Ctx, code: &str, value: &str) -> Mapped {
        let Some(npc) = ctx.npc else {
            return self.skip("setcharprop-no-npc", "setcharprop has no NPC id, not sent");
        };
        let code = code.trim();
        let props = match code {
            "#c" => vec![(NPCPROP_MESSAGE, PropValue::Text(value.to_string()))],
            "#n" => vec![(NPCPROP_NICKNAME, PropValue::Text(value.to_string()))],
            "#3" => vec![(NPCPROP_HEAD, head_value(value))],
            "#8" => vec![(NPCPROP_BODY, PropValue::Text(value.trim().to_string()))],
            "#1" | "#2" => {
                return self.skip(
                    &format!("setcharprop {code}"),
                    &format!("setcharprop {code} is not in the NPC prop stream, skipped"),
                );
            }
            c => {
                if let Some(slot) = color_slot(c) {
                    let Some(color) = looks::color_value(value) else {
                        return self.skip(
                            &format!("setcharprop {c} bad"),
                            &format!("setcharprop {c} colour {value:?} is not a colour, skipped"),
                        );
                    };
                    let entry = self.npc_colors.entry(npc).or_insert(DEFAULT_NPC_COLORS);
                    entry[slot] = color;
                    vec![(NPCPROP_COLORS, PropValue::Bytes(entry.to_vec()))]
                } else if let Some(n) = pattr(c) {
                    let Some(prop) = gattrib_prop(n) else {
                        return self.skip(
                            &format!("setcharprop {c}"),
                            &format!("setcharprop {c} has no NPC prop, skipped"),
                        );
                    };
                    vec![(prop, PropValue::Text(value.to_string()))]
                } else {
                    return self.skip(
                        &format!("setcharprop {c}"),
                        &format!("setcharprop {c} has no NPC prop, skipped"),
                    );
                }
            }
        };
        Mapped::one(npc_props_msg(npc, props))
    }

    fn player_prop(&mut self, ctx: &Ctx, code: &str, value: &str) -> Mapped {
        let Some(player_id) = ctx.player else {
            return self.skip(
                "setplayerprop-no-player",
                "setplayerprop has no triggering player, not sent",
            );
        };
        let code = code.trim();
        let props = match code {
            "#c" => prop_text(12, value),
            "#n" => prop_text(0, value),
            "#3" => prop_head(value),
            "#8" => prop_text(35, value.trim()),
            c => {
                if let Some(slot) = color_slot(c) {
                    let Some(color) = looks::color_value(value) else {
                        return self.skip(
                            &format!("setplayerprop {c} bad"),
                            &format!("setplayerprop {c} colour {value:?} is not a colour, skipped"),
                        );
                    };
                    let entry = self
                        .player_colors
                        .entry(player_id)
                        .or_insert(DEFAULT_COLORS);
                    entry[slot] = color;
                    prop_colors(entry)
                } else if let Some(n) = pattr(c) {
                    // Player attributes are props 37 to 41. NPC attributes
                    // start at 36; this arm is only the player.
                    if !(1..=5).contains(&n) {
                        return self.skip(
                            &format!("setplayerprop {c}"),
                            &format!("setplayerprop {c} has no player prop, skipped"),
                        );
                    }
                    prop_text(36 + n, value)
                } else {
                    return self.skip(
                        &format!("setplayerprop {c}"),
                        &format!("setplayerprop {c} has no player prop, skipped"),
                    );
                }
            }
        };
        Mapped::one(Message::SetPlayerProps { player_id, props })
    }

    fn warp(&mut self, ctx: &Ctx, level: &str, x: Option<f64>, y: Option<f64>) -> Mapped {
        let Some(player_id) = ctx.player else {
            return self.skip(
                "warp-no-player",
                "setlevel has no triggering player, not sent",
            );
        };
        let fallback = ctx.player_xy.unwrap_or((0.0, 0.0));
        let x = x.unwrap_or(fallback.0);
        let y = y.unwrap_or(fallback.1);
        Mapped::one(Message::WarpPlayer {
            player_id,
            level: latin1_bytes(level),
            x: half_u16(x),
            y: half_u16(y),
        })
    }

    fn flag(&mut self, name: &str, set: bool, value: String) -> Mapped {
        Mapped::one(Message::ServerFlag {
            name: latin1_bytes(name),
            set,
            value: latin1_bytes(&value),
        })
    }

    fn skip(&mut self, key: &str, line: &str) -> Mapped {
        Mapped {
            messages: Vec::new(),
            notes: self.once(key, line),
        }
    }

    fn once(&mut self, key: &str, line: &str) -> Vec<String> {
        if self.skipped.insert(key.to_string()) {
            vec![line.to_string()]
        } else {
            Vec::new()
        }
    }
}

impl Mapped {
    fn one(message: Message) -> Self {
        Self {
            messages: vec![message],
            notes: Vec::new(),
        }
    }
}

/// `NPC_ADDED` props for an NPC just loaded: image and position.
pub fn place_props(image: &str, x: f64, y: f64) -> Vec<u8> {
    npc_prop_stream(&[
        (NPCPROP_IMAGE, PropValue::Text(image.to_string())),
        (NPCPROP_X, PropValue::Byte(half_tiles(x as f32))),
        (NPCPROP_Y, PropValue::Byte(half_tiles(y as f32))),
    ])
}

fn npc_props_msg(npc_id: u32, mut props: Vec<(u8, PropValue)>) -> Message {
    props.sort_by_key(|(id, _)| *id);
    Message::NpcProps {
        npc_id,
        props: npc_prop_stream(&props),
    }
}

/// Prop bytes after the NPC id. `npc_props_message` builds a whole client
/// packet (type, id, props, newline); the link carries only the props.
fn npc_prop_stream(props: &[(u8, PropValue)]) -> Vec<u8> {
    let pkt = npc_props_message(0, props);
    if pkt.len() >= 5 && pkt[0] == 3 + 0x20 && pkt[pkt.len() - 1] == b'\n' {
        pkt[4..pkt.len() - 1].to_vec()
    } else {
        Vec::new()
    }
}

fn is_server_flag(name: &str) -> bool {
    name.starts_with("server.")
}

/// `#C0` … `#C4`.
fn color_slot(code: &str) -> Option<usize> {
    let b = code.as_bytes();
    if b.len() == 3 && b[0] == b'#' && b[1] == b'C' && b[2].is_ascii_digit() {
        let slot = (b[2] - b'0') as usize;
        (slot < 5).then_some(slot)
    } else {
        None
    }
}

/// `#P1` … `#P30` as the attribute number.
fn pattr(code: &str) -> Option<u8> {
    let rest = code.strip_prefix("#P")?;
    let n: u8 = rest.parse().ok()?;
    (1..=30).contains(&n).then_some(n)
}

/// NPC gani-attribute prop id for `#P1` … `#P30`.
fn gattrib_prop(n: u8) -> Option<u8> {
    match n {
        1..=5 => Some(NPCPROP_GATTRIB1 + (n - 1)),
        6..=9 => Some(NPCPROP_GATTRIB6 + (n - 6)),
        10..=30 => Some(NPCPROP_GATTRIB10 + (n - 10)),
        _ => None,
    }
}

/// `head<N>` / `head<N>.png` / `head<N>.gif` under 100 is an index. Any
/// other picture is a name.
fn head_value(name: &str) -> PropValue {
    let t = name.trim();
    let stem = t
        .strip_suffix(".png")
        .or_else(|| t.strip_suffix(".gif"))
        .unwrap_or(t);
    if let Some(n) = stem.strip_prefix("head") {
        if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) {
            if let Ok(i) = n.parse::<u8>() {
                if i < 100 {
                    return PropValue::Head(Head::Index(i));
                }
            }
        }
    }
    PropValue::Head(Head::Name(t.to_string()))
}

fn prop_head(name: &str) -> Vec<u8> {
    // Prop 11, then the same head bytes an NPC HEAD value uses.
    let mut props = npc_prop_stream(&[(NPCPROP_HEAD, head_value(name))]);
    if props.first() == Some(&(NPCPROP_HEAD + 0x20)) {
        props[0] = 11 + 0x20;
    }
    props
}

fn prop_byte(id: u8, value: u8) -> Vec<u8> {
    vec![id + 0x20, value.min(223) + 0x20]
}

fn prop_text(id: u8, text: &str) -> Vec<u8> {
    let bytes = latin1_bytes(text);
    let bytes = if bytes.len() > 223 {
        bytes[..223].to_vec()
    } else {
        bytes
    };
    let mut out = vec![id + 0x20, bytes.len() as u8 + 0x20];
    out.extend(bytes);
    out
}

fn prop_colors(colors: &[u8; 5]) -> Vec<u8> {
    let mut out = vec![13 + 0x20];
    out.extend(colors.iter().map(|c| (*c).min(223) + 0x20));
    out
}

fn half_u16(v: f64) -> u16 {
    (v * 2.0).round().clamp(0.0, 65535.0) as u16
}

fn latin1_bytes(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
        .collect()
}

fn skip_key(effect: &Effect) -> String {
    match effect {
        Effect::SaySign { .. } => "say".into(),
        Effect::Say2 { .. } => "say2".into(),
        Effect::SetShape { .. } => "setshape".into(),
        Effect::Play { .. } => "play".into(),
        Effect::StopSound { .. } => "stopsound".into(),
        Effect::StopMidi => "stopmidi".into(),
        Effect::SetPlayerVar { name, .. } => format!("player var {name}"),
        Effect::SetFlag { name, .. } => format!("flag {name}"),
        Effect::UnsetFlag { name } => format!("unset {name}"),
        Effect::TriggerAction { .. } => "triggeraction".into(),
        Effect::RpgMessage { .. } => "sendrpgmessage".into(),
        Effect::SendToServer { .. } => "sendtoserver".into(),
        Effect::Command { name, .. } => format!("command {name}"),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seihai_core::protocol::npcprops::parse_npc_props;
    use seihai_core::protocol::parse_player_props;
    use seihai_core::scripting::engine::world::ObjId;

    fn ctx_npc(id: u32) -> Ctx {
        Ctx {
            npc: Some(id),
            npc_xy: Some((30.0, 31.0)),
            player: None,
            hearts: 3.0,
            player_xy: None,
        }
    }

    fn ctx_player(id: u16) -> Ctx {
        Ctx {
            npc: None,
            npc_xy: None,
            player: Some(id),
            hearts: 2.5,
            player_xy: Some((10.0, 12.0)),
        }
    }

    fn npc_record(id: u32, props: &[u8]) -> (u32, seihai_core::protocol::npcprops::NpcProps) {
        let mut raw = vec![
            ((id >> 14) & 0x7F) as u8 + 0x20,
            ((id >> 7) & 0x7F) as u8 + 0x20,
            (id & 0x7F) as u8 + 0x20,
        ];
        raw.extend_from_slice(props);
        parse_npc_props(&raw, false).unwrap()
    }

    #[test]
    fn setcharprop_chat_is_the_message_prop() {
        let mut out = Outbound::default();
        let mapped = out.map(
            &Effect::SetCharProp {
                obj: 0,
                code: "#c".into(),
                value: "hi".into(),
            },
            &ctx_npc(7),
        );
        let Message::NpcProps { npc_id, props } = &mapped.messages[0] else {
            panic!("{:?}", mapped.messages);
        };
        assert_eq!(*npc_id, 7);
        let (id, p) = npc_record(7, props);
        assert_eq!(id, 7);
        assert_eq!(p.message.as_deref(), Some("hi"));
    }

    #[test]
    fn message_move_destroy_warp_flag_and_hurt() {
        let mut out = Outbound::default();
        let msg = out.map(
            &Effect::Message {
                obj: 0,
                text: "hello".into(),
            },
            &ctx_npc(1),
        );
        let Message::NpcProps { props, .. } = &msg.messages[0] else {
            panic!("{:?}", msg.messages);
        };
        assert_eq!(npc_record(1, props).1.message.as_deref(), Some("hello"));

        let moved = out.map(
            &Effect::Move {
                obj: 0,
                dx: 1.0,
                dy: 0.0,
                time: 0.0,
                options: 0,
            },
            &ctx_npc(1),
        );
        let Message::NpcProps { props, .. } = &moved.messages[0] else {
            panic!("{:?}", moved.messages);
        };
        let p = npc_record(1, props).1;
        assert_eq!((p.x, p.y), (Some(30.0), Some(31.0)));

        let gone = out.map(&Effect::Destroy { obj: 0 }, &ctx_npc(4));
        assert_eq!(gone.messages, vec![Message::NpcDeleted { npc_id: 4 }]);

        let warp = out.map(
            &Effect::SetLevel {
                level: "house.nw".into(),
                x: Some(30.5),
                y: Some(31.0),
            },
            &ctx_player(2),
        );
        assert_eq!(
            warp.messages,
            vec![Message::WarpPlayer {
                player_id: 2,
                level: b"house.nw".to_vec(),
                x: 61,
                y: 62,
            }]
        );
        let bare = out.map(
            &Effect::SetLevel {
                level: "a.nw".into(),
                x: None,
                y: None,
            },
            &ctx_player(2),
        );
        let Message::WarpPlayer { x, y, .. } = &bare.messages[0] else {
            panic!("{:?}", bare.messages);
        };
        assert_eq!((*x, *y), (20, 24));

        let set = out.map(
            &Effect::SetFlag {
                name: "server.door".into(),
                value: Some("1".into()),
            },
            &ctx_npc(1),
        );
        assert_eq!(
            set.messages,
            vec![Message::ServerFlag {
                name: b"server.door".to_vec(),
                set: true,
                value: b"1".to_vec(),
            }]
        );
        let hurt = out.map(&Effect::Hurt { half_hearts: 1.0 }, &ctx_player(2));
        let Message::SetPlayerProps { player_id, props } = &hurt.messages[0] else {
            panic!("{:?}", hurt.messages);
        };
        assert_eq!(*player_id, 2);
        let parsed = parse_player_props(props);
        assert_eq!(parsed[0].id, 2);
        assert_eq!(parsed[0].gchar(), Some(5));
    }

    #[test]
    fn unmapped_effects_are_logged_once() {
        let mut out = Outbound::default();
        let say = Effect::Say2 { text: "hi".into() };
        let first = out.map(&say, &ctx_npc(1));
        let second = out.map(&say, &ctx_npc(1));
        assert!(first.messages.is_empty());
        assert_eq!(first.notes.len(), 1);
        assert!(second.notes.is_empty(), "{:?}", second.notes);
        let flag = out.map(
            &Effect::SetFlag {
                name: "client.x".into(),
                value: None,
            },
            &ctx_npc(1),
        );
        assert!(flag.messages.is_empty() && flag.notes.len() == 1);
        let cmd = out.map(
            &Effect::Command {
                obj: 0 as ObjId,
                name: "toweapons".into(),
                args: vec![],
            },
            &ctx_player(1),
        );
        assert!(cmd.messages.is_empty());
        assert!(cmd.notes[0].contains("toweapons"));
    }

    #[test]
    fn place_props_round_trip() {
        let props = place_props("block.png", 30.0, 30.5);
        let p = npc_record(1, &props).1;
        assert_eq!(p.image.as_deref(), Some("block.png"));
        assert_eq!((p.x, p.y), (Some(30.0), Some(30.5)));
    }
}
