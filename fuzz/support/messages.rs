use seihai_npcserver::link::{nc_get, Message};
pub fn all() -> Vec<Message> {
    vec![
        Message::Hello {
            link_version: 1,
            token: b"secret".to_vec(),
            program: b"seihai-npcserver 0.1.0".to_vec(),
        },
        Message::Welcome {
            link_version: 1,
            server_name: b"Lab".to_vec(),
            program: b"seihai-gserver".to_vec(),
        },
        Message::Reject {
            reason: b"bad token".to_vec(),
        },
        Message::Ping { nonce: 7 },
        Message::Pong { nonce: 7 },
        Message::Bye {
            reason: b"stopping".to_vec(),
        },
        Message::LevelList {
            names: vec![b"a.nw".to_vec(), b"gmaps/b.nw".to_vec()],
        },
        Message::LevelReloaded {
            name: b"a.nw".to_vec(),
        },
        Message::PlayerJoined {
            player_id: 3,
            account: b"test1".to_vec(),
            level: b"a.nw".to_vec(),
            props: vec![0x2f, 0x40],
        },
        Message::PlayerProps {
            player_id: 3,
            props: vec![0x30, 0x41],
        },
        Message::PlayerLeft { player_id: 3 },
        Message::PlayerLevel {
            player_id: 3,
            level: b"gmaps/b.nw".to_vec(),
        },
        Message::PmToNpc {
            player_id: 3,
            text: b"hello npc".to_vec(),
        },
        Message::ServerFlags {
            entries: vec![b"server.door=1".to_vec(), b"server.open".to_vec()],
        },
        Message::TriggerAction {
            player_id: 3,
            level: b"a.nw".to_vec(),
            x: 60,
            y: 62,
            action: b"push".to_vec(),
        },
        Message::NpcAdded {
            npc_id: 7,
            level: b"a.nw".to_vec(),
            props: vec![0x20, 0x21, b'b'],
        },
        Message::NpcProps {
            npc_id: 7,
            props: vec![0x2f, 0x22, b'h', b'i'],
        },
        Message::NpcDeleted { npc_id: 7 },
        Message::SetPlayerProps {
            player_id: 3,
            props: vec![0x22, 0x25],
        },
        Message::WarpPlayer {
            player_id: 3,
            level: b"house.nw".to_vec(),
            x: 61,
            y: 62,
        },
        Message::AddWeapon {
            player_id: 3,
            weapon: b"sword".to_vec(),
        },
        Message::ServerFlag {
            name: b"server.door".to_vec(),
            set: true,
            value: b"1".to_vec(),
        },
        Message::Pm {
            player_id: 3,
            text: b"hi".to_vec(),
        },
        Message::PlaySound {
            level: b"a.nw".to_vec(),
            file: b"chest.wav".to_vec(),
            x: 60,
            y: 62,
            looped: true,
        },
        Message::StopSound {
            level: b"a.nw".to_vec(),
            file: b"chest.wav".to_vec(),
        },
        Message::StopMidi {
            level: b"a.nw".to_vec(),
        },
        Message::NpcShape {
            npc_id: 7,
            kind: 1,
            w: 32,
            h: 48,
        },
        Message::NpcTriggerAction {
            level: b"a.nw".to_vec(),
            x: 21,
            y: 41,
            action: b"open".to_vec(),
            params: b"red,door".to_vec(),
        },
        Message::SendToNc {
            text: b"shop open".to_vec(),
        },
        Message::SendToRc {
            text: b"acct has logged in".to_vec(),
        },
        Message::NcNpc {
            npc_id: 3,
            name: b"bell".to_vec(),
            kind: b"CONTROL".to_vec(),
            level: b"a.nw".to_vec(),
        },
        Message::NcGet {
            request: 9,
            npc_id: 3,
            what: nc_get::ATTRIBUTES,
        },
        Message::NcText {
            request: 9,
            found: true,
            text: b"if (created) {\n  message hi;\n}".to_vec(),
        },
        Message::NcScriptSet {
            npc_id: 3,
            script: b"if (playerenters) { message hi; }".to_vec(),
        },
        Message::NcNpcDelete { npc_id: 3 },
        Message::NcNpcReset { npc_id: 3 },
        Message::NcNpcWarp {
            npc_id: 3,
            level: b"b.nw".to_vec(),
            x: 61,
            y: 20,
        },
        Message::NcNpcAdd {
            name: b"Guard".to_vec(),
            kind: b"OBJECT".to_vec(),
            scripter: b"boss".to_vec(),
            level: b"start.nw".to_vec(),
            x: 60,
            y: 61,
        },
        Message::Unknown {
            id: 0x99,
            data: vec![1, 2, 3],
        },
    ]
}
