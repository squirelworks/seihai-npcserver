//! Project-owned durable GANI attributes, separate from the NPC source file.
//! A sidecar contains one existing link NPC_PROPS frame for id zero, with
//! only canonical attribute properties. No text escaping or new wire id.

use std::collections::BTreeMap;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use seihai_core::protocol::npcprops::{npc_props_message, parse_npc_props, PropValue};

use crate::link::{self, Message};

pub(crate) fn number(key: &str) -> Option<u8> {
    let n: u8 = key.strip_prefix("ATTR")?.parse().ok()?;
    ((1..=30).contains(&n) && key == format!("ATTR{n}")).then_some(n)
}

pub(crate) fn props(values: &BTreeMap<String, String>) -> Vec<u8> {
    let attrs: Vec<_> = values
        .iter()
        .filter_map(|(k, v)| {
            Some((
                crate::effects::gattrib_prop(number(k)?)?,
                PropValue::Text(v.clone()),
            ))
        })
        .collect();
    let encoded = npc_props_message(0, &attrs);
    encoded[4..encoded.len() - 1].to_vec()
}

pub(crate) fn values(props: &[u8]) -> BTreeMap<String, String> {
    let mut bytes = vec![0x20; 3];
    bytes.extend_from_slice(props);
    parse_npc_props(&bytes, false)
        .map(|(_, p)| {
            p.attrs
                .into_iter()
                .enumerate()
                .filter_map(|(i, v)| v.map(|v| (format!("ATTR{}", i + 1), v)))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn path(npc: &Path) -> PathBuf {
    npc.with_extension("txt.attrs")
}

/// Keep the sidecar with a deleted NPC's no-clobber recovery file.
pub(crate) fn archive(npc: &Path) -> io::Result<bool> {
    let live = path(npc);
    let recovery = npc.with_extension("txt.deleted.attrs");
    match std::fs::hard_link(&live, &recovery) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound && !live.exists() => return Ok(false),
        Err(e) => return Err(e),
    }
    std::fs::remove_file(live)?;
    Ok(true)
}

pub(crate) fn restore(npc: &Path) -> io::Result<()> {
    std::fs::hard_link(npc.with_extension("txt.deleted.attrs"), path(npc))
}

pub(crate) fn load(npc: &Path) -> io::Result<BTreeMap<String, String>> {
    let file = match std::fs::File::open(path(npc)) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(e),
    };
    let mut data = Vec::new();
    file.take((link::MAX_FRAME + 6) as u64)
        .read_to_end(&mut data)?;
    let mut input = data.as_slice();
    let messages = link::read_frame(&mut input)?;
    if !input.is_empty() {
        return Err(invalid());
    }
    let [Message::NpcProps {
        npc_id: 0,
        props: raw,
    }] = messages.as_slice()
    else {
        return Err(invalid());
    };
    let attrs = values(raw);
    if props(&attrs) != *raw {
        return Err(invalid());
    }
    Ok(attrs)
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid GANI attribute sidecar")
}

pub(crate) fn save(npc: &Path, attrs: &BTreeMap<String, String>) -> io::Result<()> {
    let destination = path(npc);
    let temporary = npc.with_extension("txt.attrs.new");
    let data = link::encode_frame(&[Message::NpcProps {
        npc_id: 0,
        props: props(attrs),
    }])?;
    std::fs::write(&temporary, data)?;
    std::fs::rename(temporary, destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_roundtrip_and_malformed_input() {
        let dir = std::env::temp_dir().join(format!("seihai-attr-sidecar-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let npc = dir.join("bell name.txt");
        assert!(load(&npc).unwrap().is_empty());
        let attrs = BTreeMap::from([
            ("ATTR1".into(), "a=b\nnext,\"value\"".into()),
            ("ATTR30".into(), String::new()),
        ]);
        save(&npc, &attrs).unwrap();
        assert_eq!(load(&npc).unwrap(), attrs);
        assert_eq!(number("ATTR01"), None);
        assert_eq!(number("ATTR+1"), None);
        let mut legacy = attrs.clone();
        legacy.insert("ATTR01".into(), "unrelated".into());
        save(&npc, &legacy).unwrap();
        assert_eq!(load(&npc).unwrap(), attrs);
        std::fs::write(npc.with_extension("txt.deleted.attrs"), b"keep recovery").unwrap();
        assert!(archive(&npc).is_err());
        assert_eq!(load(&npc).unwrap(), attrs);
        assert_eq!(
            std::fs::read(npc.with_extension("txt.deleted.attrs")).unwrap(),
            b"keep recovery"
        );
        let valid = std::fs::read(path(&npc)).unwrap();
        for len in 0..valid.len() {
            std::fs::write(path(&npc), &valid[..len]).unwrap();
            assert!(load(&npc).is_err());
        }
        let mut trailing = valid.clone();
        trailing.push(0);
        std::fs::write(path(&npc), trailing).unwrap();
        assert!(load(&npc).is_err());
        std::fs::write(
            path(&npc),
            link::encode_frame(&[Message::Ping { nonce: 1 }]).unwrap(),
        )
        .unwrap();
        assert!(load(&npc).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
