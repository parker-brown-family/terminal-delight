//! The tool the agent is holding, worn as the pane's logo.
//!
//! A pane's logo used to be a still portrait: an uploaded image, or the one its
//! cwd inherits from `dir-logos.toml`. It never moved, while the thing it was
//! labelling — an agent, working — changed what it was doing every few seconds.
//! The terminal already knew: [`crate::mcp_tail`] reads the structured tool
//! calls out of the pane's own transcript, and until now only did so when an
//! outside MCP client asked. This module turns that stream into a face.
//!
//! **The vocabulary is not ours.** Which tool is which drawing, in which
//! colour, was decided in agent-playhouse by counting 9,320 real tool calls
//! across 84 transcripts — `ctx_shell` alone is 52.5% of them, twelve tools
//! cover 98%. A hand-kept second copy of a fifty-row table is a fork waiting to
//! drift, so `scripts/sync-tool-props.mjs` generates both the manifest and the
//! plates from the playhouse's own files, and `--check` fails when they part
//! ways. Edit the playhouse, re-run the script; never edit the assets.
//!
//! Plates are baked PNGs rather than SVG because gpui renders images from
//! paths, and compiled in rather than installed because a bare deployed
//! `td-<sha>` has to carry its own art — the same bargain [`crate::art`] makes
//! for the mascot, down to sharing its runtime-materialisation.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The generated tool table. Regenerate with `scripts/sync-tool-props.mjs`.
const MANIFEST: &str = include_str!("../assets/tool-props.json");

/// Every baked plate, compiled in. The unit test below asserts this table
/// covers every `art` the manifest names — a new drawing in the playhouse
/// fails HERE, at `cargo test`, not as a blank square on somebody's wall.
const ART: &[(&str, &[u8])] = &[
    ("board", include_bytes!("../assets/img/props/board.png")),
    ("book", include_bytes!("../assets/img/props/book.png")),
    ("chisel", include_bytes!("../assets/img/props/chisel.png")),
    ("compass", include_bytes!("../assets/img/props/compass.png")),
    ("console", include_bytes!("../assets/img/props/console.png")),
    ("dish", include_bytes!("../assets/img/props/dish.png")),
    ("frame", include_bytes!("../assets/img/props/frame.png")),
    ("horn", include_bytes!("../assets/img/props/horn.png")),
    ("ledger", include_bytes!("../assets/img/props/ledger.png")),
    ("lens", include_bytes!("../assets/img/props/lens.png")),
    ("map", include_bytes!("../assets/img/props/map.png")),
    ("net", include_bytes!("../assets/img/props/net.png")),
    ("parcel", include_bytes!("../assets/img/props/parcel.png")),
    (
        "question",
        include_bytes!("../assets/img/props/question.png"),
    ),
    ("quill", include_bytes!("../assets/img/props/quill.png")),
    ("radar", include_bytes!("../assets/img/props/radar.png")),
    ("scroll", include_bytes!("../assets/img/props/scroll.png")),
    ("tile", include_bytes!("../assets/img/props/tile.png")),
    ("wrench", include_bytes!("../assets/img/props/wrench.png")),
];

/// The plate a tool nobody wrote a row for reaches — the playhouse's lettered
/// block, kept on purpose as the honest fallback.
const FALLBACK_ART: &str = "tile";

/// One tool's row: its drawing, its two-letter mark, the gerund a human reads,
/// and the colour it is lit in.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Row {
    pub art: Option<String>,
    pub glyph: Option<String>,
    pub verb: Option<String>,
    pub tint: Option<String>,
}

#[derive(Deserialize)]
struct Manifest {
    props: HashMap<String, Row>,
}

/// The user's layer over the generated table: `~/.config/terminal-delight/
/// tool-props.toml`. Adds rows for tools the playhouse has never seen and
/// overrides ones it has, without a rebuild — and carries the one switch.
///
/// ```toml
/// follows_tool = true
///
/// [props.deploy_the_thing]
/// art = "parcel"
/// glyph = "DT"
/// verb = "shipping it"
/// ```
#[derive(Deserialize, Default)]
struct UserLayer {
    #[serde(default = "yes")]
    follows_tool: bool,
    #[serde(default)]
    props: HashMap<String, Row>,
}

fn yes() -> bool {
    true
}

fn user_layer() -> &'static UserLayer {
    static L: OnceLock<UserLayer> = OnceLock::new();
    L.get_or_init(|| {
        let p = crate::instance::config_dir().join("tool-props.toml");
        std::fs::read_to_string(p)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    })
}

fn table() -> &'static HashMap<String, Row> {
    static T: OnceLock<HashMap<String, Row>> = OnceLock::new();
    T.get_or_init(|| {
        let mut m: HashMap<String, Row> = serde_json::from_str::<Manifest>(MANIFEST)
            .map(|man| man.props)
            .unwrap_or_default();
        for (k, v) in &user_layer().props {
            m.insert(k.clone(), v.clone());
        }
        m
    })
}

/// Whether panes wear the tool at all. On unless the user's layer says no.
pub fn follows_tool() -> bool {
    user_layer().follows_tool
}

/// `mcp__lean-ctx__ctx_read` → `ctx_read`. An MCP tool arrives in a transcript
/// namespaced by its server, but the vocabulary is written in bare tool names,
/// so a row is looked up under the full name first and the bare one after —
/// which lets a user's layer disambiguate two servers' same-named tools.
fn bare(tool: &str) -> &str {
    tool.strip_prefix("mcp__")
        .and_then(|rest| rest.split_once("__"))
        .map(|(_, t)| t)
        .unwrap_or(tool)
}

/// The row for a tool, if the vocabulary has one.
pub fn row(tool: &str) -> Option<&'static Row> {
    let t = table();
    t.get(tool).or_else(|| t.get(bare(tool)))
}

/// Two letters for a tool nobody wrote a row for: `WebFetch` → `WF`,
/// `read_session` → `RS`, `grep` → `GR`. Mirrors the playhouse's own fallback
/// so the same unknown tool gets the same mark in both.
pub fn initials(tool: &str) -> String {
    let tool = bare(tool);
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prev_lower_or_digit = false;
    for ch in tool.chars() {
        if !ch.is_ascii_alphanumeric() {
            if !cur.is_empty() {
                parts.push(std::mem::take(&mut cur));
            }
            prev_lower_or_digit = false;
            continue;
        }
        // camelCase is a word boundary too, so `WebFetch` is two parts.
        if ch.is_ascii_uppercase() && prev_lower_or_digit && !cur.is_empty() {
            parts.push(std::mem::take(&mut cur));
        }
        prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        cur.push(ch);
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    let mark: String = if parts.len() >= 2 {
        parts[..2]
            .iter()
            .filter_map(|p| p.chars().next())
            .collect::<String>()
    } else {
        tool.chars().take(2).collect()
    };
    mark.to_uppercase()
}

/// `#c98500` → `0x00c98500`, for `gpui::rgb`. Tolerates a missing `#`.
pub fn tint_rgb(hex: &str) -> Option<u32> {
    let h = hex.trim().trim_start_matches('#');
    (h.len() == 6).then(|| u32::from_str_radix(h, 16).ok())?
}

/// What a pane wears while an agent holds `tool`: the plate to draw, the mark
/// to letter it with when the plate is the anonymous fallback, and the colour
/// and gerund that go with it.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    /// The baked plate on disk, ready for `gpui::img`.
    pub plate: PathBuf,
    /// Two letters, set only when `plate` is the anonymous fallback — a known
    /// tool's drawing IS its identity and wants no lettering over it.
    pub mark: Option<String>,
    /// `0xRRGGBB`, for the border and the glow the plate is lit with.
    pub tint: Option<u32>,
    /// "at the console" — what a human calls what the agent is doing.
    pub verb: String,
    /// The tool name as the transcript spelled it, for the tooltip.
    pub tool: String,
}

impl Face {
    /// What a human reads under the plate. The verb, because the verb is the
    /// specification the drawing was made from — `web/art/CONVENTIONS.md` is
    /// blunt that the tool name is only WHICH drawing, never what he is doing.
    /// For a tool nobody wrote a row for there is no verb worth showing, and
    /// its own name beats a generic "working".
    pub fn caption(&self) -> &str {
        if self.mark.is_some() {
            bare(&self.tool)
        } else {
            &self.verb
        }
    }
}

/// Resolve a tool name to the face a pane should wear. `None` only when the
/// plate cannot be materialised (a read-only runtime dir), never for an
/// unrecognised tool — that is what the lettered fallback is for.
pub fn face(tool: &str) -> Option<Face> {
    let row = row(tool);
    let art = row
        .and_then(|r| r.art.as_deref())
        .filter(|a| ART.iter().any(|(n, _)| n == a))
        .unwrap_or(FALLBACK_ART);
    let plate = plate_path(art)?;
    Some(Face {
        plate,
        mark: (art == FALLBACK_ART).then(|| {
            row.and_then(|r| r.glyph.clone())
                .unwrap_or_else(|| initials(tool))
        }),
        tint: row.and_then(|r| r.tint.as_deref()).and_then(tint_rgb),
        verb: row
            .and_then(|r| r.verb.clone())
            .unwrap_or_else(|| "working".into()),
        tool: tool.to_string(),
    })
}

/// Materialise one plate to the runtime dir and hand back its path.
fn plate_path(art: &str) -> Option<PathBuf> {
    let bytes = ART.iter().find(|(n, _)| *n == art).map(|(_, b)| *b)?;
    Some(crate::art::runtime_asset(
        &format!("terminal-delight-prop-{art}.png"),
        bytes,
    ))
}

// ------------------------------------------------------------------ the sweep

/// What one pane's transcript looked like at the last sweep.
///
/// Carried across sweeps rather than recomputed, so an unchanged file costs one
/// `stat` and nothing else. That is most panes most of the time: only an agent
/// mid-turn appends, and a wall of six panes usually has one or two of those.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolProbe {
    path: Option<PathBuf>,
    len: u64,
    mtime: Option<std::time::SystemTime>,
    /// The tool named by the last structured event in the transcript.
    pub tool: Option<String>,
}

/// One pane's place in a sweep, snapshotted on the main thread so that the I/O
/// can happen off it.
pub struct ToolProbeReq {
    pub id: gpui::EntityId,
    /// The pane's mode label — `CLAUDE` / `CODEX`, as `transcript_for` spells it.
    pub mode: String,
    pub cwd: Option<String>,
    /// The resume command, which carries the fd-accurate session id. Without it
    /// two panes in one directory both resolve to the newest transcript, and
    /// one of them wears the other's tool.
    pub session: Option<String>,
    /// Whether the agent is actually doing something. A resting pane is probed
    /// anyway — it keeps the cache warm for nothing — but wears no face.
    pub working: bool,
    pub prev: ToolProbe,
}

/// The background half of the sweep: resolve each pane's transcript and read
/// the tool it ended on. Pure I/O — no gpui, no main thread, and never a write.
pub fn resolve_probes(reqs: Vec<ToolProbeReq>) -> Vec<(gpui::EntityId, ToolProbe, bool)> {
    let home = crate::session::home_dir();
    reqs.into_iter()
        .map(|r| {
            let probe = probe_one(&r, &home);
            (r.id, probe, r.working)
        })
        .collect()
}

fn probe_one(r: &ToolProbeReq, home: &std::path::Path) -> ToolProbe {
    let path =
        crate::mcp_tail::transcript_for(&r.mode, r.cwd.as_deref(), r.session.as_deref(), home);
    probe_transcript(path, &r.prev)
}

/// The part worth testing: given a transcript path and what we knew last time,
/// what is this pane holding? Split out from [`probe_one`] because a pane's
/// entity id is not something a test can conjure, and the caching is the half
/// most likely to be got wrong.
fn probe_transcript(path: Option<PathBuf>, prev: &ToolProbe) -> ToolProbe {
    let Some(path) = path else {
        return ToolProbe::default();
    };
    let (len, mtime) = std::fs::metadata(&path)
        .map(|m| (m.len(), m.modified().ok()))
        .unwrap_or((0, None));
    // Same file, same size, same mtime ⇒ the agent has written nothing since we
    // last looked, and the answer cannot have changed.
    if prev.path.as_deref() == Some(path.as_path()) && prev.len == len && prev.mtime == mtime {
        return prev.clone();
    }
    let tool = crate::mcp_tail::tail_tool_events(&path, 1)
        .pop()
        .map(|e| e.tool);
    ToolProbe {
        path: Some(path),
        len,
        mtime,
        tool,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generated manifest parses, and carries the tools that dominate the
    /// count — if this fails the asset was hand-edited or the sync script broke.
    #[test]
    fn the_manifest_carries_the_common_tools() {
        let t = table();
        assert!(t.len() >= 40, "only {} rows in the manifest", t.len());
        for tool in ["Bash", "Read", "Edit", "ctx_shell", "ctx_read", "WebFetch"] {
            assert!(t.contains_key(tool), "{tool} has no row");
        }
        assert_eq!(t["Bash"].art.as_deref(), Some("console"));
    }

    /// Every drawing the manifest names is compiled in. A prop added to the
    /// playhouse and synced without rebuilding this table fails HERE.
    #[test]
    fn every_named_drawing_is_embedded() {
        for (tool, row) in table() {
            let Some(art) = row.art.as_deref() else {
                continue;
            };
            assert!(
                ART.iter().any(|(n, _)| *n == art),
                "{tool} wants plate '{art}', which is not embedded"
            );
        }
        assert!(ART.iter().any(|(n, _)| *n == FALLBACK_ART));
    }

    /// Every embedded plate is a real PNG — a bad path or a stripped commit
    /// fails at build, not as an invisible logo at runtime.
    #[test]
    fn every_embedded_plate_is_a_real_png() {
        for (name, bytes) in ART {
            assert!(bytes.len() > 500, "{name} suspiciously small");
            assert!(
                bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
                "{name} is not a PNG stream"
            );
        }
    }

    /// An MCP tool arrives namespaced; the vocabulary is written bare.
    #[test]
    fn mcp_namespaced_tools_find_their_row() {
        assert_eq!(bare("mcp__lean-ctx__ctx_read"), "ctx_read");
        assert_eq!(bare("mcp__terminal-delight__list_panes"), "list_panes");
        assert_eq!(bare("Bash"), "Bash");
        assert_eq!(
            row("mcp__lean-ctx__ctx_shell").and_then(|r| r.art.clone()),
            Some("console".into())
        );
    }

    /// The lettered fallback mirrors the playhouse's, so the same unknown tool
    /// is marked the same way in the terminal and in the film.
    #[test]
    fn an_unmapped_tool_still_gets_two_letters() {
        assert_eq!(initials("WebFetch"), "WF");
        assert_eq!(initials("read_session"), "RS");
        assert_eq!(initials("grep"), "GR");
        assert_eq!(initials("mcp__garrison__garrison_search"), "GS");
        assert_eq!(initials("X"), "X");
    }

    /// A known tool wears its drawing unlettered; an unknown one wears the
    /// anonymous plate WITH letters. That asymmetry is the whole fallback.
    #[test]
    fn a_known_tool_is_unlettered_and_an_unknown_one_is_not() {
        let known = face("Bash").expect("plate materialises");
        assert!(known.plate.ends_with("terminal-delight-prop-console.png"));
        assert_eq!(known.mark, None);
        assert_eq!(known.tint, Some(0xc98500));
        assert_eq!(known.verb, "at the console");

        let unknown = face("frobnicate_widget").expect("plate materialises");
        assert!(unknown.plate.ends_with("terminal-delight-prop-tile.png"));
        assert_eq!(unknown.mark.as_deref(), Some("FW"));
    }

    /// The end-to-end path a pane actually walks: a real Claude transcript on
    /// disk, through the tailer, into the face a header renders. This is the
    /// scenario on the ticket, minus the window.
    #[test]
    fn a_transcript_resolves_to_the_face_its_last_tool_wears() {
        let dir = std::env::temp_dir().join(format!("td-toolprobe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        let line = |tool: &str, arg: &str| {
            format!(
                r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"{tool}","input":{{"file_path":"{arg}"}}}}]}}}}"#
            ) + "\n"
        };

        std::fs::write(&path, line("Bash", "x") + &line("Read", "src/main.rs")).unwrap();
        let first = probe_transcript(Some(path.clone()), &ToolProbe::default());
        assert_eq!(first.tool.as_deref(), Some("Read"), "the LAST call wins");
        let worn = face(first.tool.as_deref().unwrap()).unwrap();
        assert!(worn.plate.ends_with("terminal-delight-prop-scroll.png"));
        assert_eq!(worn.caption(), "reading");

        // An untouched transcript costs a stat and returns the same answer.
        let again = probe_transcript(Some(path.clone()), &first);
        assert_eq!(again, first, "unchanged file ⇒ cached probe, no re-read");

        // A new call swaps the face, which is the whole feature.
        std::fs::write(
            &path,
            line("Bash", "x") + &line("Read", "src/main.rs") + &line("WebFetch", "http://x"),
        )
        .unwrap();
        let third = probe_transcript(Some(path.clone()), &again);
        assert_eq!(third.tool.as_deref(), Some("WebFetch"));
        assert_eq!(
            face("WebFetch").unwrap().caption(),
            "aerial to the sky",
            "the caption is the VERB, never the tool name"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A pane with no transcript (a plain shell, or an agent that has not
    /// written yet) wears nothing rather than something wrong.
    #[test]
    fn no_transcript_means_no_face() {
        assert_eq!(
            probe_transcript(None, &ToolProbe::default()),
            ToolProbe::default()
        );
    }

    #[test]
    fn tints_parse_and_bad_ones_do_not() {
        assert_eq!(tint_rgb("#c98500"), Some(0xc98500));
        assert_eq!(tint_rgb("2aa9a0"), Some(0x2aa9a0));
        assert_eq!(tint_rgb("#fff"), None);
        assert_eq!(tint_rgb("nonsense"), None);
    }
}
