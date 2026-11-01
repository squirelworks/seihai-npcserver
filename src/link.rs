//! The gserver to NPC-server link: frames and messages (see `PROTOCOL.md`).
//!
//! This module is the whole wire format. It has no sockets in it, so the
//! gserver can use the same code for its side of the link.

use std::io::{self, Read, Write};

use seihai_core::protocol::{
    bz2_compress, bz2_decompress, zlib_compress, zlib_decompress, FrameLen,
};

/// The link version this code speaks.
pub const LINK_VERSION: u16 = 1;

/// The default link address: the gserver listens here.
pub const DEFAULT_ADDR: &str = "127.0.0.1:14810";

/// The environment variable that carries the token from the gserver.
pub const TOKEN_ENV: &str = "SEIHAI_NPCSERVER_TOKEN";

/// The largest frame, or decompressed body, either side accepts.
pub const MAX_FRAME: usize = 16 << 20;

/// Bodies shorter than this go out uncompressed.
const COMPRESS_FROM: usize = 48;

/// Message ids.
pub mod id {
    pub const HELLO: u8 = 0x01;
    pub const WELCOME: u8 = 0x02;
    pub const REJECT: u8 = 0x03;
    pub const PING: u8 = 0x04;
    pub const PONG: u8 = 0x05;
    pub const BYE: u8 = 0x06;
    pub const LEVEL_LIST: u8 = 0x10;
    pub const LEVEL_RELOADED: u8 = 0x11;
    pub const PLAYER_JOINED: u8 = 0x20;
    pub const PLAYER_PROPS: u8 = 0x21;
    pub const PLAYER_LEFT: u8 = 0x22;
    pub const PLAYER_LEVEL: u8 = 0x23;
    pub const PM_TO_NPC: u8 = 0x24;
    pub const SERVER_FLAGS: u8 = 0x25;
    pub const TRIGGER_ACTION: u8 = 0x26;
    pub const NPC_ADDED: u8 = 0x40;
    pub const NPC_PROPS: u8 = 0x41;
    pub const NPC_DELETED: u8 = 0x42;
    pub const SET_PLAYER_PROPS: u8 = 0x43;
    pub const WARP_PLAYER: u8 = 0x44;
    pub const ADD_WEAPON: u8 = 0x45;
    pub const SERVER_FLAG: u8 = 0x46;
    pub const PM: u8 = 0x47;
}

/// One link message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Hello {
        link_version: u16,
        token: Vec<u8>,
        program: Vec<u8>,
    },
    Welcome {
        link_version: u16,
        server_name: Vec<u8>,
        program: Vec<u8>,
    },
    Reject {
        reason: Vec<u8>,
    },
    Ping {
        nonce: u32,
    },
    Pong {
        nonce: u32,
    },
    Bye {
        reason: Vec<u8>,
    },
    LevelList {
        names: Vec<Vec<u8>>,
    },
    LevelReloaded {
        name: Vec<u8>,
    },
    PlayerJoined {
        player_id: u16,
        account: Vec<u8>,
        level: Vec<u8>,
        props: Vec<u8>,
    },
    PlayerProps {
        player_id: u16,
        props: Vec<u8>,
    },
    PlayerLeft {
        player_id: u16,
    },
    PlayerLevel {
        player_id: u16,
        level: Vec<u8>,
    },
    PmToNpc {
        player_id: u16,
        text: Vec<u8>,
    },
    /// Whole `server.` flag set, `name=value` entries.
    ServerFlags {
        entries: Vec<Vec<u8>>,
    },
    TriggerAction {
        player_id: u16,
        level: Vec<u8>,
        x: u16,
        y: u16,
        action: Vec<u8>,
    },
    NpcAdded {
        npc_id: u32,
        level: Vec<u8>,
        props: Vec<u8>,
    },
    NpcProps {
        npc_id: u32,
        props: Vec<u8>,
    },
    NpcDeleted {
        npc_id: u32,
    },
    SetPlayerProps {
        player_id: u16,
        props: Vec<u8>,
    },
    WarpPlayer {
        player_id: u16,
        level: Vec<u8>,
        x: u16,
        y: u16,
    },
    AddWeapon {
        player_id: u16,
        weapon: Vec<u8>,
    },
    ServerFlag {
        name: Vec<u8>,
        set: bool,
        value: Vec<u8>,
    },
    Pm {
        player_id: u16,
        text: Vec<u8>,
    },
    /// A message this code does not know. Kept so it can be logged and skipped.
    Unknown {
        id: u8,
        data: Vec<u8>,
    },
}

impl Message {
    /// The message id.
    pub fn id(&self) -> u8 {
        match self {
            Message::Hello { .. } => id::HELLO,
            Message::Welcome { .. } => id::WELCOME,
            Message::Reject { .. } => id::REJECT,
            Message::Ping { .. } => id::PING,
            Message::Pong { .. } => id::PONG,
            Message::Bye { .. } => id::BYE,
            Message::LevelList { .. } => id::LEVEL_LIST,
            Message::LevelReloaded { .. } => id::LEVEL_RELOADED,
            Message::PlayerJoined { .. } => id::PLAYER_JOINED,
            Message::PlayerProps { .. } => id::PLAYER_PROPS,
            Message::PlayerLeft { .. } => id::PLAYER_LEFT,
            Message::PlayerLevel { .. } => id::PLAYER_LEVEL,
            Message::PmToNpc { .. } => id::PM_TO_NPC,
            Message::ServerFlags { .. } => id::SERVER_FLAGS,
            Message::TriggerAction { .. } => id::TRIGGER_ACTION,
            Message::NpcAdded { .. } => id::NPC_ADDED,
            Message::NpcProps { .. } => id::NPC_PROPS,
            Message::NpcDeleted { .. } => id::NPC_DELETED,
            Message::SetPlayerProps { .. } => id::SET_PLAYER_PROPS,
            Message::WarpPlayer { .. } => id::WARP_PLAYER,
            Message::AddWeapon { .. } => id::ADD_WEAPON,
            Message::ServerFlag { .. } => id::SERVER_FLAG,
            Message::Pm { .. } => id::PM,
            Message::Unknown { id, .. } => *id,
        }
    }

    /// Append this message (id, length, data) to `out`.
    pub fn encode_into(&self, out: &mut Vec<u8>) -> io::Result<()> {
        let mut d = Vec::new();
        match self {
            Message::Hello {
                link_version,
                token,
                program,
            } => {
                d.extend_from_slice(&link_version.to_be_bytes());
                put_str(&mut d, token)?;
                put_str(&mut d, program)?;
            }
            Message::Welcome {
                link_version,
                server_name,
                program,
            } => {
                d.extend_from_slice(&link_version.to_be_bytes());
                put_str(&mut d, server_name)?;
                put_str(&mut d, program)?;
            }
            Message::Reject { reason } | Message::Bye { reason } => put_str(&mut d, reason)?,
            Message::Ping { nonce } | Message::Pong { nonce } => {
                d.extend_from_slice(&nonce.to_be_bytes())
            }
            Message::LevelList { names } => {
                let n = u16::try_from(names.len())
                    .map_err(|_| invalid("more than 65535 levels in one LEVEL_LIST"))?;
                d.extend_from_slice(&n.to_be_bytes());
                for name in names {
                    put_str(&mut d, name)?;
                }
            }
            Message::LevelReloaded { name } => put_str(&mut d, name)?,
            Message::PlayerJoined {
                player_id,
                account,
                level,
                props,
            } => {
                d.extend_from_slice(&player_id.to_be_bytes());
                put_str(&mut d, account)?;
                put_str(&mut d, level)?;
                put_blob(&mut d, props)?;
            }
            Message::PlayerProps { player_id, props } => {
                d.extend_from_slice(&player_id.to_be_bytes());
                put_blob(&mut d, props)?;
            }
            Message::PlayerLeft { player_id } => d.extend_from_slice(&player_id.to_be_bytes()),
            Message::PlayerLevel { player_id, level } => {
                d.extend_from_slice(&player_id.to_be_bytes());
                put_str(&mut d, level)?;
            }
            Message::PmToNpc { player_id, text } | Message::Pm { player_id, text } => {
                d.extend_from_slice(&player_id.to_be_bytes());
                put_str(&mut d, text)?;
            }
            Message::ServerFlags { entries } => {
                let n = u16::try_from(entries.len())
                    .map_err(|_| invalid("more than 65535 server flags"))?;
                d.extend_from_slice(&n.to_be_bytes());
                for e in entries {
                    put_str(&mut d, e)?;
                }
            }
            Message::TriggerAction {
                player_id,
                level,
                x,
                y,
                action,
            } => {
                d.extend_from_slice(&player_id.to_be_bytes());
                put_str(&mut d, level)?;
                d.extend_from_slice(&x.to_be_bytes());
                d.extend_from_slice(&y.to_be_bytes());
                put_str(&mut d, action)?;
            }
            Message::NpcAdded {
                npc_id,
                level,
                props,
            } => {
                d.extend_from_slice(&npc_id.to_be_bytes());
                put_str(&mut d, level)?;
                put_blob(&mut d, props)?;
            }
            Message::NpcProps { npc_id, props } => {
                d.extend_from_slice(&npc_id.to_be_bytes());
                put_blob(&mut d, props)?;
            }
            Message::NpcDeleted { npc_id } => d.extend_from_slice(&npc_id.to_be_bytes()),
            Message::SetPlayerProps { player_id, props } => {
                d.extend_from_slice(&player_id.to_be_bytes());
                put_blob(&mut d, props)?;
            }
            Message::WarpPlayer {
                player_id,
                level,
                x,
                y,
            } => {
                d.extend_from_slice(&player_id.to_be_bytes());
                put_str(&mut d, level)?;
                d.extend_from_slice(&x.to_be_bytes());
                d.extend_from_slice(&y.to_be_bytes());
            }
            Message::AddWeapon { player_id, weapon } => {
                d.extend_from_slice(&player_id.to_be_bytes());
                put_str(&mut d, weapon)?;
            }
            Message::ServerFlag { name, set, value } => {
                put_str(&mut d, name)?;
                d.push(u8::from(*set));
                put_str(&mut d, value)?;
            }
            Message::Unknown { data, .. } => d.extend_from_slice(data),
        }
        let len = u32::try_from(d.len()).map_err(|_| invalid("message over 4 GiB"))?;
        out.push(self.id());
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&d);
        Ok(())
    }

    /// Decode one message's data. Unknown ids become [`Message::Unknown`].
    pub fn decode(id: u8, data: &[u8]) -> io::Result<Message> {
        let mut r = Cursor { buf: data, pos: 0 };
        let m = match id {
            id::HELLO => Message::Hello {
                link_version: r.u16()?,
                token: r.str()?,
                program: r.str()?,
            },
            id::WELCOME => Message::Welcome {
                link_version: r.u16()?,
                server_name: r.str()?,
                program: r.str()?,
            },
            id::REJECT => Message::Reject { reason: r.str()? },
            id::PING => Message::Ping { nonce: r.u32()? },
            id::PONG => Message::Pong { nonce: r.u32()? },
            id::BYE => Message::Bye { reason: r.str()? },
            id::LEVEL_LIST => {
                let n = r.u16()?;
                let mut names = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    names.push(r.str()?);
                }
                Message::LevelList { names }
            }
            id::LEVEL_RELOADED => Message::LevelReloaded { name: r.str()? },
            id::PLAYER_JOINED => Message::PlayerJoined {
                player_id: r.u16()?,
                account: r.str()?,
                level: r.str()?,
                props: r.blob()?,
            },
            id::PLAYER_PROPS => Message::PlayerProps {
                player_id: r.u16()?,
                props: r.blob()?,
            },
            id::PLAYER_LEFT => Message::PlayerLeft {
                player_id: r.u16()?,
            },
            id::PLAYER_LEVEL => Message::PlayerLevel {
                player_id: r.u16()?,
                level: r.str()?,
            },
            id::PM_TO_NPC => Message::PmToNpc {
                player_id: r.u16()?,
                text: r.str()?,
            },
            id::SERVER_FLAGS => {
                let n = r.u16()?;
                let mut entries = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    entries.push(r.str()?);
                }
                Message::ServerFlags { entries }
            }
            id::TRIGGER_ACTION => Message::TriggerAction {
                player_id: r.u16()?,
                level: r.str()?,
                x: r.u16()?,
                y: r.u16()?,
                action: r.str()?,
            },
            id::NPC_ADDED => Message::NpcAdded {
                npc_id: r.u32()?,
                level: r.str()?,
                props: r.blob()?,
            },
            id::NPC_PROPS => Message::NpcProps {
                npc_id: r.u32()?,
                props: r.blob()?,
            },
            id::NPC_DELETED => Message::NpcDeleted { npc_id: r.u32()? },
            id::SET_PLAYER_PROPS => Message::SetPlayerProps {
                player_id: r.u16()?,
                props: r.blob()?,
            },
            id::WARP_PLAYER => Message::WarpPlayer {
                player_id: r.u16()?,
                level: r.str()?,
                x: r.u16()?,
                y: r.u16()?,
            },
            id::ADD_WEAPON => Message::AddWeapon {
                player_id: r.u16()?,
                weapon: r.str()?,
            },
            id::SERVER_FLAG => Message::ServerFlag {
                name: r.str()?,
                set: r.byte()? != 0,
                value: r.str()?,
            },
            id::PM => Message::Pm {
                player_id: r.u16()?,
                text: r.str()?,
            },
            other => Message::Unknown {
                id: other,
                data: data.to_vec(),
            },
        };
        // Bytes after the known fields are allowed: later versions may append.
        Ok(m)
    }
}

/// Split a frame body into messages.
pub fn decode_body(body: &[u8]) -> io::Result<Vec<Message>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < body.len() {
        let head = body
            .get(i..i + 5)
            .ok_or_else(|| invalid("truncated message header"))?;
        let id = head[0];
        let len = u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize;
        let start = i + 5;
        let end = start
            .checked_add(len)
            .filter(|&e| e <= body.len())
            .ok_or_else(|| invalid("message runs past the end of its frame"))?;
        out.push(Message::decode(id, &body[start..end])?);
        i = end;
    }
    Ok(out)
}

/// Encode messages into one frame: length, compression byte, body.
pub fn encode_frame(msgs: &[Message]) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    for m in msgs {
        m.encode_into(&mut body)?;
    }
    if body.len() > MAX_FRAME {
        return Err(invalid("frame body over 16 MiB"));
    }
    let (comp, packed) = if body.len() < COMPRESS_FROM {
        (0x02u8, body)
    } else {
        let z = zlib_compress(&body)?;
        if z.len() < body.len() {
            (0x04, z)
        } else {
            (0x02, body)
        }
    };
    Ok(frame_from(comp, &packed))
}

fn frame_from(comp: u8, packed: &[u8]) -> Vec<u8> {
    let total = 1 + packed.len();
    let mut out = Vec::with_capacity(total + 6);
    if total < 0xFFFF {
        out.extend_from_slice(&(total as u16).to_be_bytes());
    } else {
        out.extend_from_slice(&[0xFF, 0xFF]);
        out.extend_from_slice(&(total as u32).to_be_bytes());
    }
    out.push(comp);
    out.extend_from_slice(packed);
    out
}

/// Write messages as one frame.
pub fn write_frame<W: Write>(w: &mut W, msgs: &[Message]) -> io::Result<()> {
    w.write_all(&encode_frame(msgs)?)?;
    w.flush()
}

/// Read one frame and return its messages.
pub fn read_frame<R: Read>(r: &mut R) -> io::Result<Vec<Message>> {
    let mut head = [0u8; 6];
    r.read_exact(&mut head[..2])?;
    if FrameLen::Long.header(&head[..2]).is_none() {
        r.read_exact(&mut head[2..])?;
    }
    let (_, total) = FrameLen::Long.header(&head).expect("six bytes are enough");
    if total == 0 {
        return Err(invalid("empty frame"));
    }
    if total > MAX_FRAME + 1 {
        return Err(invalid("frame over 16 MiB"));
    }
    let mut raw = vec![0u8; total];
    r.read_exact(&mut raw)?;
    let body = match raw[0] {
        0x02 => raw.split_off(1),
        0x04 => zlib_decompress(&raw[1..])?,
        0x06 => bz2_decompress(&raw[1..])?,
        b => return Err(invalid(&format!("unknown compression byte 0x{b:02x}"))),
    };
    if body.len() > MAX_FRAME {
        return Err(invalid("decompressed body over 16 MiB"));
    }
    decode_body(&body)
}

/// Compress with bz2, for a gserver that wants to send a big body that way.
pub fn encode_frame_bz2(msgs: &[Message]) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    for m in msgs {
        m.encode_into(&mut body)?;
    }
    Ok(frame_from(0x06, &bz2_compress(&body)?))
}

/// Compare two tokens without stopping at the first difference.
pub fn tokens_match(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Bytes as text for logs: printable ASCII as is, everything else as `\xNN`.
pub fn show(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len());
    for &b in bytes {
        if (0x20..0x7f).contains(&b) && b != b'\\' {
            s.push(b as char);
        } else {
            s.push_str(&format!("\\x{b:02x}"));
        }
    }
    s
}

fn put_str(d: &mut Vec<u8>, s: &[u8]) -> io::Result<()> {
    let n = u16::try_from(s.len()).map_err(|_| invalid("string over 65535 bytes"))?;
    d.extend_from_slice(&n.to_be_bytes());
    d.extend_from_slice(s);
    Ok(())
}

fn put_blob(d: &mut Vec<u8>, s: &[u8]) -> io::Result<()> {
    let n = u32::try_from(s.len()).map_err(|_| invalid("blob over 4 GiB"))?;
    d.extend_from_slice(&n.to_be_bytes());
    d.extend_from_slice(s);
    Ok(())
}

fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_string())
}

struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn take(&mut self, n: usize) -> io::Result<&[u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.buf.len())
            .ok_or_else(|| invalid("message data too short"))?;
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn byte(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> io::Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> io::Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn str(&mut self) -> io::Result<Vec<u8>> {
        let n = self.u16()? as usize;
        Ok(self.take(n)?.to_vec())
    }
    fn blob(&mut self) -> io::Result<Vec<u8>> {
        let n = self.u32()? as usize;
        Ok(self.take(n)?.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<Message> {
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
            Message::Unknown {
                id: 0x99,
                data: vec![1, 2, 3],
            },
        ]
    }

    #[test]
    fn every_message_round_trips() {
        let msgs = all();
        let frame = encode_frame(&msgs).unwrap();
        assert_eq!(read_frame(&mut &frame[..]).unwrap(), msgs);
    }

    #[test]
    fn hello_bytes_match_protocol_md() {
        let m = Message::Hello {
            link_version: 1,
            token: b"tk".to_vec(),
            program: b"p".to_vec(),
        };
        let frame = encode_frame(&[m]).unwrap();
        // len 1 + 5 + 2 + 4 + 3 = 15; none; id 1; data len 9; v1; "tk"; "p".
        assert_eq!(
            frame,
            [0, 15, 0x02, 0x01, 0, 0, 0, 9, 0, 1, 0, 2, b't', b'k', 0, 1, b'p']
        );
    }

    #[test]
    fn big_bodies_are_zlib() {
        let m = Message::LevelList {
            names: (0..50)
                .map(|i| format!("level{i}.nw").into_bytes())
                .collect(),
        };
        let frame = encode_frame(std::slice::from_ref(&m)).unwrap();
        assert_eq!(frame[2], 0x04);
        assert_eq!(read_frame(&mut &frame[..]).unwrap(), vec![m]);
    }

    #[test]
    fn bz2_frames_are_read() {
        let m = Message::PlayerProps {
            player_id: 1,
            props: vec![0x20; 5000],
        };
        let frame = encode_frame_bz2(std::slice::from_ref(&m)).unwrap();
        assert_eq!(read_frame(&mut &frame[..]).unwrap(), vec![m]);
    }

    #[test]
    fn long_length_form() {
        // Random-ish props so zlib cannot shrink the frame under 0xFFFF.
        let mut x: u32 = 1;
        let props: Vec<u8> = (0..70_000)
            .map(|_| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (x >> 16) as u8
            })
            .collect();
        let m = Message::PlayerProps {
            player_id: 1,
            props,
        };
        let frame = encode_frame(std::slice::from_ref(&m)).unwrap();
        assert_eq!(&frame[..2], &[0xFF, 0xFF]);
        assert_eq!(read_frame(&mut &frame[..]).unwrap(), vec![m]);
    }

    #[test]
    fn appended_fields_are_ignored() {
        let body = [id::PING, 0, 0, 0, 6, 0, 0, 0, 9, 0xAA, 0xBB];
        assert_eq!(
            decode_body(&body).unwrap(),
            vec![Message::Ping { nonce: 9 }]
        );
    }

    #[test]
    fn short_data_is_an_error() {
        let body = [id::PLAYER_LEFT, 0, 0, 0, 1, 0];
        assert!(decode_body(&body).is_err());
        let body = [id::NPC_DELETED, 0, 0, 0, 3, 0, 0, 1];
        assert!(decode_body(&body).is_err());
        let body = [id::PING, 0, 0, 0, 9, 0];
        assert!(decode_body(&body).is_err());
        let body = [id::PING, 0, 0];
        assert!(decode_body(&body).is_err());
    }

    #[test]
    fn bad_compression_byte_is_an_error() {
        assert!(read_frame(&mut &[0u8, 1, 0x78][..]).is_err());
        assert!(read_frame(&mut &[0u8, 0][..]).is_err());
    }

    #[test]
    fn token_compare() {
        assert!(tokens_match(b"abc", b"abc"));
        assert!(!tokens_match(b"abc", b"abd"));
        assert!(!tokens_match(b"abc", b"abcd"));
    }

    #[test]
    fn npc_props_bytes_match_protocol_md() {
        let m = Message::NpcProps {
            npc_id: 7,
            props: vec![0x2f, 0x22, b'h', b'i'],
        };
        let mut body = Vec::new();
        m.encode_into(&mut body).unwrap();
        assert_eq!(
            body,
            [
                id::NPC_PROPS,
                0,
                0,
                0,
                12,
                0,
                0,
                0,
                7,
                0,
                0,
                0,
                4,
                0x2f,
                0x22,
                b'h',
                b'i',
            ]
        );
    }

    #[test]
    fn show_escapes() {
        assert_eq!(show(b"a\\b\x01"), "a\\x5cb\\x01");
    }
}
