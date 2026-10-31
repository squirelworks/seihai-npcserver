//! The NPC database: server-side NPCs and where they live (milestone 2).
//!
//! The 2001 NPC-server loaded "the NPC database" after the level list. Ours
//! is a folder of plain text files, one NPC per file, in the world's `npcs/`
//! folder. The format is ours (see `docs/ARCHITECTURE.md`, "NPC files"):
//!
//! ```text
//! LEVEL onlinestartlocal.nw
//! X 30
//! Y 30
//! IMAGE block.png
//! SCRIPT
//! if (created) { message hello; }
//! SCRIPTEND
//! ```
//!
//! TODO(milestone 2): also read GServer-v2 style `npcs/npc*.txt` files so
//! existing worlds move over. Write that reader from the format as seen in
//! files, not from GServer-v2's code (`AGENTS.md`).

use std::io;
use std::path::Path;

/// One server-side NPC.
#[derive(Debug, Clone, PartialEq)]
pub struct NpcDef {
    /// Stable id, the file's place in name order (from 1). Goes into link
    /// messages as the NPC id.
    pub id: u32,
    pub level: Vec<u8>,
    /// Tiles.
    pub x: f64,
    pub y: f64,
    pub image: String,
    pub script: String,
}

/// Parse one NPC file. `id` is assigned by the caller.
pub fn parse(id: u32, text: &str) -> Result<NpcDef, String> {
    let mut npc = NpcDef {
        id,
        level: Vec::new(),
        x: 0.0,
        y: 0.0,
        image: String::new(),
        script: String::new(),
    };
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let line = line.trim_end_matches('\r');
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "" => {}
            "LEVEL" => npc.level = value.trim().as_bytes().to_vec(),
            "X" => {
                npc.x = value
                    .trim()
                    .parse()
                    .map_err(|_| format!("bad X {value:?}"))?
            }
            "Y" => {
                npc.y = value
                    .trim()
                    .parse()
                    .map_err(|_| format!("bad Y {value:?}"))?
            }
            "IMAGE" => npc.image = value.trim().to_string(),
            "SCRIPT" => {
                let mut body = Vec::new();
                let mut closed = false;
                for l in lines.by_ref() {
                    if l.trim_end_matches('\r') == "SCRIPTEND" {
                        closed = true;
                        break;
                    }
                    body.push(l.trim_end_matches('\r'));
                }
                if !closed {
                    return Err("SCRIPT without SCRIPTEND".into());
                }
                npc.script = body.join("\n");
            }
            other => return Err(format!("unknown key {other:?}")),
        }
    }
    if npc.level.is_empty() {
        return Err("no LEVEL".into());
    }
    Ok(npc)
}

/// Load every `*.txt` in `<world>/npcs/`, in name order. A file that does
/// not parse is reported in the second list and skipped.
pub fn load(world: &Path) -> io::Result<(Vec<NpcDef>, Vec<String>)> {
    let dir = world.join("npcs");
    let mut names: Vec<_> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "txt"))
            .collect(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((Vec::new(), Vec::new())),
        Err(e) => return Err(e),
    };
    names.sort();
    let mut npcs = Vec::new();
    let mut problems = Vec::new();
    for (i, path) in names.iter().enumerate() {
        let text = String::from_utf8_lossy(&std::fs::read(path)?).into_owned();
        match parse(i as u32 + 1, &text) {
            Ok(n) => npcs.push(n),
            Err(e) => problems.push(format!("{}: {e}", path.display())),
        }
    }
    Ok((npcs, problems))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "LEVEL onlinestartlocal.nw\nX 30\nY 30.5\nIMAGE block.png\nSCRIPT\nif (created) {\n  message hello;\n}\nSCRIPTEND\n";

    #[test]
    fn parses_the_sample() {
        let n = parse(1, SAMPLE).unwrap();
        assert_eq!(n.level, b"onlinestartlocal.nw");
        assert_eq!((n.x, n.y), (30.0, 30.5));
        assert_eq!(n.image, "block.png");
        assert_eq!(n.script, "if (created) {\n  message hello;\n}");
    }

    #[test]
    fn errors() {
        assert!(parse(1, "X 1\n").is_err());
        assert!(parse(1, "LEVEL a.nw\nSCRIPT\nfoo;\n").is_err());
        assert!(parse(1, "LEVEL a.nw\nCOLOR red\n").is_err());
    }

    #[test]
    fn loads_a_folder() {
        let dir = std::env::temp_dir().join(format!("seihai-npcdb-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("npcs")).unwrap();
        std::fs::write(dir.join("npcs/a.txt"), SAMPLE).unwrap();
        std::fs::write(dir.join("npcs/b.txt"), "nonsense here\n").unwrap();
        std::fs::write(dir.join("npcs/readme.md"), "ignored").unwrap();
        let (npcs, problems) = load(&dir).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(npcs.len(), 1);
        assert_eq!(npcs[0].id, 1);
        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn no_folder_is_empty() {
        let (npcs, problems) = load(Path::new("/nonexistent-seihai-world")).unwrap();
        assert!(npcs.is_empty() && problems.is_empty());
    }
}

#[cfg(test)]
mod example_world {
    #[test]
    fn the_example_world_loads() {
        let world = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/world");
        let (npcs, problems) = super::load(&world).unwrap();
        assert_eq!(npcs.len(), 1, "{problems:?}");
        assert!(problems.is_empty());
    }
}
