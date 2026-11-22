//! The NPC database: server-side NPCs and where they live (milestone 2).
//!
//! The 2001 NPC-server loaded "the NPC database" after the level list. Ours
//! is a folder of plain text files, one NPC per file, in the world's `npcs/`
//! folder. Two formats are read there.
//!
//! Our own (see `docs/ARCHITECTURE.md`, "NPC files"):
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
//! GServer-v2 world folders (`npcs/npc<NAME>.txt`), so existing worlds move
//! over. These start with the line `GRNPC001`, then `KEY value` lines and
//! the script between `NPCSCRIPT` and `NPCSCRIPTEND`:
//!
//! ```text
//! GRNPC001
//! NAME Control-NPC
//! ID 10000
//! TYPE CONTROL
//! IMAGE
//! STARTLEVEL
//! STARTX 30.00
//! STARTY 30.00
//! COLORS 2,0,10,4,18
//! SHAPE 32 48
//! NPCSCRIPT
//! if (playerlogin) { sendtorc #a has logged in!; }
//! NPCSCRIPTEND
//! ```
//!
//! That reader is written from the format as seen in sample files (the
//! fork's lab worlds), not from GServer-v2's code (`AGENTS.md`).

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

/// First line of a GServer-v2 style NPC file.
pub const GRNPC_MAGIC: &str = "GRNPC001";

/// Which file format an NPC was read from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NpcFormat {
    /// Our own `LEVEL`/`X`/`Y`/`IMAGE`/`SCRIPT` format.
    #[default]
    Seihai,
    /// A GServer-v2 style `GRNPC001` file.
    Grnpc,
}

/// One server-side NPC.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NpcDef {
    /// Stable id, the file's place in name order (from 1). Goes into link
    /// messages as the NPC id.
    pub id: u32,
    /// `NAME` in a `GRNPC001` file; the file name without `.txt` in ours.
    pub name: String,
    /// Start level. Empty for a level-less NPC (a `GRNPC001` file with an
    /// empty `STARTLEVEL`, such as a `TYPE CONTROL` NPC).
    pub level: Vec<u8>,
    /// Tiles.
    pub x: f64,
    pub y: f64,
    pub image: String,
    pub script: String,
    /// Every other `KEY value` line of a `GRNPC001` file, as written
    /// (`ID`, `TYPE`, `NICK`, `ANI`, the stats, `COLORS`, `SHAPE`, ...,
    /// and keys this reader does not know). Empty for our own format.
    pub props: BTreeMap<String, String>,
    pub format: NpcFormat,
    /// The file it was read from, if any. NPC Control writes a new script
    /// back to it.
    pub path: Option<std::path::PathBuf>,
}

impl NpcDef {
    /// `TYPE` from a `GRNPC001` file (`CONTROL`, ...), if given.
    pub fn kind(&self) -> Option<&str> {
        self.props
            .get("TYPE")
            .map(String::as_str)
            .filter(|t| !t.is_empty())
    }
}

/// Parse one NPC file in either format. `id` is assigned by the caller;
/// `name` is the file name without `.txt` (a `GRNPC001` file's `NAME`
/// replaces it).
pub fn parse_file(id: u32, name: &str, text: &str) -> Result<NpcDef, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let first = text.lines().next().unwrap_or("").trim_end_matches('\r');
    if first == GRNPC_MAGIC {
        parse_grnpc(id, name, text)
    } else {
        let mut npc = parse(id, text)?;
        npc.name = name.to_string();
        Ok(npc)
    }
}

/// Parse a `GRNPC001` file (the magic line included). A missing key keeps
/// its default; an unknown key is kept in [`NpcDef::props`].
pub fn parse_grnpc(id: u32, name: &str, text: &str) -> Result<NpcDef, String> {
    let mut npc = NpcDef {
        id,
        name: name.to_string(),
        format: NpcFormat::Grnpc,
        ..NpcDef::default()
    };
    let mut lines = text.lines().map(|l| l.trim_end_matches('\r'));
    if lines.next() != Some(GRNPC_MAGIC) {
        return Err(format!("does not start with {GRNPC_MAGIC}"));
    }
    let tiles = |key: &str, value: &str| -> Result<f64, String> {
        let v = value.trim();
        if v.is_empty() {
            return Ok(0.0);
        }
        v.parse().map_err(|_| format!("bad {key} {value:?}"))
    };
    while let Some(line) = lines.next() {
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "" => {}
            "NAME" => {
                if !value.trim().is_empty() {
                    npc.name = value.trim().to_string();
                }
            }
            "STARTLEVEL" => npc.level = value.trim().as_bytes().to_vec(),
            "STARTX" => npc.x = tiles(key, value)?,
            "STARTY" => npc.y = tiles(key, value)?,
            "IMAGE" => npc.image = value.trim().to_string(),
            "NPCSCRIPT" => {
                let mut body = Vec::new();
                let mut closed = false;
                for l in lines.by_ref() {
                    if l == "NPCSCRIPTEND" {
                        closed = true;
                        break;
                    }
                    body.push(l);
                }
                if !closed {
                    return Err("NPCSCRIPT without NPCSCRIPTEND".into());
                }
                npc.script = body.join("\n");
            }
            other => {
                npc.props.insert(other.to_string(), value.to_string());
            }
        }
    }
    if let Some(v) = npc.props.get("ID") {
        if !v.trim().is_empty() && v.trim().parse::<u32>().is_err() {
            return Err(format!("bad ID {v:?}"));
        }
    }
    Ok(npc)
}

/// Parse one file in our own format. `id` is assigned by the caller.
pub fn parse(id: u32, text: &str) -> Result<NpcDef, String> {
    let mut npc = NpcDef {
        id,
        ..NpcDef::default()
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

/// Load every `*.txt` in `<world>/npcs/`, in name order: our own format
/// and GServer-v2's `GRNPC001` files. The n-th file gets NPC id n. A file
/// that does not parse is reported in the second list and skipped. When a
/// `GRNPC001` file and one of ours have the same name, ours wins and the
/// other is reported as skipped.
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
    let mut parsed = Vec::new();
    let mut problems = Vec::new();
    for (i, path) in names.iter().enumerate() {
        let text = String::from_utf8_lossy(&std::fs::read(path)?).into_owned();
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        match parse_file(i as u32 + 1, &stem, &text) {
            Ok(mut n) => {
                match crate::attributes::load(path) {
                    Ok(attrs) => n.props.extend(attrs),
                    Err(e) => {
                        problems.push(format!("{}: attributes: {e}", path.display()));
                        continue;
                    }
                }
                n.path = Some(path.clone());
                parsed.push((path, n))
            }
            Err(e) => problems.push(format!("{}: {e}", path.display())),
        }
    }
    let ours: std::collections::BTreeSet<String> = parsed
        .iter()
        .filter(|(_, n)| n.format == NpcFormat::Seihai)
        .map(|(_, n)| n.name.clone())
        .collect();
    let mut npcs = Vec::new();
    for (path, n) in parsed {
        if n.format == NpcFormat::Grnpc && ours.contains(&n.name) {
            problems.push(format!(
                "{}: GRNPC001 NPC {:?} has a file in our own format too; ours is used",
                path.display(),
                n.name
            ));
            continue;
        }
        npcs.push(n);
    }
    Ok((npcs, problems))
}

/// `text`, an NPC file in `format`, with its script replaced by
/// `script`. The parsers use the last script block, so that is the block
/// replaced; earlier blocks and other lines stay as they were. A file with
/// no script block gets one at the end.
pub fn with_script(text: &str, format: NpcFormat, script: &str) -> String {
    let (open, close) = match format {
        NpcFormat::Seihai => ("SCRIPT", "SCRIPTEND"),
        NpcFormat::Grnpc => ("NPCSCRIPT", "NPCSCRIPTEND"),
    };
    let mut source = text.lines().enumerate();
    let mut effective = None;
    while let Some((index, line)) = source.next() {
        if line.trim_end_matches('\r') == open {
            effective = Some(index);
            // An opening marker inside a script is body text, not a new block.
            for (_, body) in source.by_ref() {
                if body.trim_end_matches('\r') == close {
                    break;
                }
            }
        }
    }
    let mut out = String::new();
    let mut lines = text.lines().enumerate();
    while let Some((index, line)) = lines.next() {
        if Some(index) == effective {
            for (_, l) in lines.by_ref() {
                if l.trim_end_matches('\r') == close {
                    break;
                }
            }
            push_block(&mut out, open, close, script);
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if effective.is_none() {
        push_block(&mut out, open, close, script);
    }
    out
}

/// `text`, an NPC file in `format`, with its level and position set:
/// `LEVEL`/`X`/`Y` in ours, `STARTLEVEL`/`STARTX`/`STARTY` in a
/// `GRNPC001` file. Lines before the script block that name those keys
/// are replaced; missing ones are added before the script (after the
/// magic line of a `GRNPC001` file). Everything else stays as it was.
pub fn with_position(text: &str, format: NpcFormat, level: &str, x: f64, y: f64) -> String {
    let (kl, kx, ky, open) = match format {
        NpcFormat::Seihai => ("LEVEL", "X", "Y", "SCRIPT"),
        NpcFormat::Grnpc => ("STARTLEVEL", "STARTX", "STARTY", "NPCSCRIPT"),
    };
    let num = |v: f64| {
        if format == NpcFormat::Grnpc {
            format!("{v:.2}")
        } else if v.fract() == 0.0 {
            format!("{v}")
        } else {
            format!("{v:.1}")
        }
    };
    let wanted = [(kl, level.to_string()), (kx, num(x)), (ky, num(y))];
    let mut seen = [false; 3];
    let mut out = String::new();
    let mut in_script = false;
    let mut header_done = format == NpcFormat::Seihai;
    let mut missing_written = false;
    let write_missing = |out: &mut String, seen: &[bool; 3]| {
        for (i, (k, v)) in wanted.iter().enumerate() {
            if !seen[i] {
                out.push_str(&format!("{k} {v}\n"));
            }
        }
    };
    let close = if format == NpcFormat::Seihai {
        "SCRIPTEND"
    } else {
        "NPCSCRIPTEND"
    };
    for line in text.lines() {
        let bare = line.trim_end_matches('\r');
        if in_script {
            if bare == close {
                in_script = false;
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if !header_done {
            // The `GRNPC001` magic line stays first.
            header_done = true;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if bare == open {
            if !missing_written {
                write_missing(&mut out, &seen);
                missing_written = true;
            }
            in_script = true;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let key = bare.split_whitespace().next().unwrap_or("");
        match wanted.iter().position(|(k, _)| *k == key) {
            Some(i) if !missing_written => {
                seen[i] = true;
                out.push_str(&format!("{} {}\n", wanted[i].0, wanted[i].1));
            }
            _ => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    if !missing_written {
        write_missing(&mut out, &seen);
    }
    out
}

fn push_block(out: &mut String, open: &str, close: &str, script: &str) {
    out.push_str(open);
    out.push('\n');
    for l in script.lines() {
        // A line that would end the block early is kept with a space.
        if l.trim_end() == close {
            out.push(' ');
        }
        out.push_str(l);
        out.push('\n');
    }
    out.push_str(close);
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "LEVEL onlinestartlocal.nw\nX 30\nY 30.5\nIMAGE block.png\nSCRIPT\nif (created) {\n  message hello;\n}\nSCRIPTEND\n";

    #[test]
    fn a_new_position_keeps_the_rest_of_the_file() {
        let text = with_position(SAMPLE, NpcFormat::Seihai, "b.nw", 12.5, 7.0);
        assert_eq!(
            text,
            SAMPLE
                .replace("LEVEL onlinestartlocal.nw", "LEVEL b.nw")
                .replace("X 30\n", "X 12.5\n")
                .replace("Y 30.5\n", "Y 7\n")
        );
        // A script line that looks like a key is not touched.
        let tricky = "LEVEL a.nw\nSCRIPT\nX 1\nSCRIPTEND\n";
        assert_eq!(
            with_position(tricky, NpcFormat::Seihai, "b.nw", 2.0, 3.0),
            "LEVEL b.nw\nX 2\nY 3\nSCRIPT\nX 1\nSCRIPTEND\n"
        );
        let grnpc = "GRNPC001\nNAME Bell\nSTARTLEVEL a.nw\nSTARTX 1.00\nNPCSCRIPT\nSTARTY 9\nNPCSCRIPTEND\n";
        assert_eq!(
            with_position(grnpc, NpcFormat::Grnpc, "b.nw", 30.5, 20.0),
            "GRNPC001\nNAME Bell\nSTARTLEVEL b.nw\nSTARTX 30.50\nSTARTY 20.00\nNPCSCRIPT\nSTARTY 9\nNPCSCRIPTEND\n"
        );
        // The new place reads back.
        let back = parse_file(
            1,
            "bell",
            &with_position(SAMPLE, NpcFormat::Seihai, "b.nw", 12.5, 7.0),
        )
        .unwrap();
        assert_eq!(
            (back.level.as_slice(), back.x, back.y),
            (b"b.nw".as_slice(), 12.5, 7.0)
        );
    }

    #[test]
    fn a_new_script_keeps_the_rest_of_the_file() {
        let text = with_script(
            SAMPLE,
            NpcFormat::Seihai,
            "if (playerenters) {\n  message hi;\n}",
        );
        let n = parse(1, &text).unwrap();
        assert_eq!(n.script, "if (playerenters) {\n  message hi;\n}");
        assert_eq!(
            (n.level.as_slice(), n.x, n.image.as_str()),
            (&b"onlinestartlocal.nw"[..], 30.0, "block.png")
        );
        let g = "GRNPC001\nNAME Bell\nTYPE CONTROL\nNPCSCRIPT\nold;\nNPCSCRIPTEND\nSHAPE 32 48\n";
        let text = with_script(g, NpcFormat::Grnpc, "new;\nNPCSCRIPTEND");
        let n = parse_grnpc(1, "x", &text).unwrap();
        assert_eq!(n.script, "new;\n NPCSCRIPTEND");
        assert_eq!(n.props.get("SHAPE").map(String::as_str), Some("32 48"));
        assert_eq!(n.kind(), Some("CONTROL"));
    }

    #[test]
    fn script_markers_inside_bodies_do_not_select_a_block() {
        for (format, header, open, close) in [
            (NpcFormat::Seihai, "LEVEL a.nw\n", "SCRIPT", "SCRIPTEND"),
            (
                NpcFormat::Grnpc,
                "GRNPC001\nSTARTLEVEL a.nw\n",
                "NPCSCRIPT",
                "NPCSCRIPTEND",
            ),
        ] {
            let earlier = format!("{open}\nearlier;\n{open}\n{close}\n");
            let text = format!("{header}{earlier}{open}\nold;\n{open}\n{close}\n");
            let updated = with_script(&text, format, "new;");
            assert!(updated.starts_with(&format!("{header}{earlier}")));
            assert_eq!(parse_file(1, "test", &updated).unwrap().script, "new;");
        }
    }

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

    /// Hand-made, in the shape of the fork's lab-world files.
    const GR: &str = "GRNPC001\nNAME Door-Keeper\nID 10001\nTYPE LEVELNPC\nSCRIPTER \nIMAGE door.png\nSTARTLEVEL onlinestartlocal.nw\nSTARTX 12.50\nSTARTY 40.00\nNICK Keeper\nANI idle\nHP 3\nGRALATS 5\nCOLORS 2,0,10,4,18\nSHAPE 32 48\nSAVEARR 0,0,0,0,0,0,0,0,0,0\nNPCSCRIPT\nif (created) {\n  message hi;\n}\nNPCSCRIPTEND\n";

    #[test]
    fn parses_a_grnpc_file() {
        let n = parse_file(3, "npcDoor-Keeper", GR).unwrap();
        assert_eq!(n.format, NpcFormat::Grnpc);
        assert_eq!(n.id, 3);
        assert_eq!(n.name, "Door-Keeper");
        assert_eq!(n.level, b"onlinestartlocal.nw");
        assert_eq!((n.x, n.y), (12.5, 40.0));
        assert_eq!(n.image, "door.png");
        assert_eq!(n.script, "if (created) {\n  message hi;\n}");
        assert_eq!(n.kind(), Some("LEVELNPC"));
        for (k, v) in [
            ("ID", "10001"),
            ("NICK", "Keeper"),
            ("ANI", "idle"),
            ("HP", "3"),
            ("GRALATS", "5"),
            ("COLORS", "2,0,10,4,18"),
            ("SHAPE", "32 48"),
            ("SCRIPTER", ""),
        ] {
            assert_eq!(n.props.get(k).map(String::as_str), Some(v), "{k}");
        }
    }

    #[test]
    fn grnpc_crlf_reads_the_same() {
        let crlf = GR.replace('\n', "\r\n");
        let a = parse_file(3, "x", GR).unwrap();
        let b = parse_file(3, "x", &crlf).unwrap();
        assert_eq!(a, b);
        assert!(!b.script.contains('\r'));
    }

    #[test]
    fn grnpc_missing_fields_keep_defaults() {
        let n = parse_file(
            1,
            "npcBare",
            "GRNPC001\nNAME Bare\nNPCSCRIPT\nNPCSCRIPTEND\n",
        )
        .unwrap();
        assert_eq!(n.name, "Bare");
        assert!(n.level.is_empty(), "no STARTLEVEL: level-less");
        assert_eq!((n.x, n.y), (0.0, 0.0));
        assert_eq!(n.image, "");
        assert_eq!(n.script, "");
        assert_eq!(n.kind(), None);
        // No NAME: the file name stands in.
        let n = parse_file(1, "npcNoName", "GRNPC001\nSTARTLEVEL a.nw\n").unwrap();
        assert_eq!(n.name, "npcNoName");
        assert_eq!(n.level, b"a.nw");
    }

    #[test]
    fn grnpc_unknown_keys_are_kept_not_refused() {
        let n = parse_file(1, "x", "GRNPC001\nNAME A\nFUTUREKEY some value\nBARE\n").unwrap();
        assert_eq!(
            n.props.get("FUTUREKEY").map(String::as_str),
            Some("some value")
        );
        assert_eq!(n.props.get("BARE").map(String::as_str), Some(""));
    }

    #[test]
    fn grnpc_errors() {
        assert!(parse_file(1, "x", "GRNPC001\nSTARTX east\n").is_err());
        assert!(parse_file(1, "x", "GRNPC001\nID ten\n").is_err());
        assert!(parse_file(1, "x", "GRNPC001\nNPCSCRIPT\nfoo;\n").is_err());
        // Without the magic line it is read as ours, where NAME is unknown.
        assert!(parse_file(1, "x", "NAME A\nLEVEL a.nw\n").is_err());
    }

    #[test]
    fn grnpc_with_a_byte_order_mark() {
        let n = parse_file(1, "x", &format!("\u{feff}{GR}")).unwrap();
        assert_eq!(n.format, NpcFormat::Grnpc);
    }

    #[test]
    fn loads_both_formats_and_ours_wins_a_name_clash() {
        let dir = tempfile_dir("both");
        std::fs::write(dir.join("npcs/Door-Keeper.txt"), SAMPLE).unwrap();
        std::fs::write(dir.join("npcs/npcDoor-Keeper.txt"), GR).unwrap();
        std::fs::write(
            dir.join("npcs/npcSign.txt"),
            GR.replace("Door-Keeper", "Sign").replace("\n", "\r\n"),
        )
        .unwrap();
        let (npcs, problems) = load(&dir).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let got: Vec<_> = npcs
            .iter()
            .map(|n| (n.id, n.name.as_str(), n.format))
            .collect();
        assert_eq!(
            got,
            vec![
                (1, "Door-Keeper", NpcFormat::Seihai),
                (3, "Sign", NpcFormat::Grnpc)
            ]
        );
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("npcDoor-Keeper.txt"), "{problems:?}");
        assert!(problems[0].contains("ours is used"), "{problems:?}");
    }

    fn tempfile_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("seihai-npcdb-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("npcs")).unwrap();
        dir
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
