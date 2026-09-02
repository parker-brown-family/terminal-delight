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

/// Every drawing the terminal ships, named once.
///
/// Each name becomes two compiled-in assets, because the two places a face is
/// worn are different sizes and want different things:
///
/// * **the prop alone**, a still PNG, for the pane header square — it renders
///   about 18px, where a whole robot is a smudge and one object still reads;
/// * **the robot holding it**, an ANIMATED WebP, for the agent wall's card art
///   — 228×116, where there is room for a character and the character is the
///   point. He hovers, his forearm pumps, and he blinks, all sampled off the
///   playhouse's own stylesheet.
///
/// The animation costs the render code nothing: gpui decodes multi-frame WebP
/// and advances it on its own clock, and only while the window is ACTIVE and
/// the element is actually laid out. A closed wall, a background window, or a
/// pane header (which wears the still prop) all cost zero — which is what makes
/// motion affordable here at all.
///
/// One list rather than two tables so a drawing added to the playhouse is added
/// here once; `include_bytes!` then fails the BUILD if either half is missing,
/// rather than leaving a blank square on somebody's wall.
macro_rules! art_tables {
    ($($name:literal),+ $(,)?) => {
        const PROPS: &[(&str, &[u8])] = &[
            $(($name, include_bytes!(concat!("../assets/img/props/", $name, ".png")))),+
        ];
        const SCENES: &[(&str, &[u8])] = &[
            $(($name, include_bytes!(concat!("../assets/img/scenes/", $name, ".webp")))),+
        ];
    };
}

art_tables![
    "board", "book", "chisel", "compass", "console", "dish", "frame", "horn", "ledger", "lens",
    "map", "net", "parcel", "question", "quill", "radar", "scroll", "tile", "wrench",
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
#[derive(Deserialize)]
struct UserLayer {
    #[serde(default = "yes")]
    follows_tool: bool,
    #[serde(default)]
    props: HashMap<String, Row>,
}

fn yes() -> bool {
    true
}

/// Hand-written, and it has to be: `#[derive(Default)]` would hand back
/// `bool::default()` — FALSE — because serde's `default = "yes"` applies only
/// when a field is missing from a document being deserialised, never to
/// `Default::default()`. Since no machine ships a `tool-props.toml`, the derive
/// meant `follows_tool` was false everywhere, the sweep returned an empty list
/// every time, and the whole feature was off by construction while every test
/// passed. Both paths now read the same `yes()`.
impl Default for UserLayer {
    fn default() -> Self {
        Self {
            follows_tool: yes(),
            props: HashMap::new(),
        }
    }
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
    /// The prop alone, for a small square — the pane header. Ready for
    /// `gpui::img`.
    pub plate: PathBuf,
    /// The robot holding it, for a big one — the agent wall's card art.
    pub scene: PathBuf,
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
        .filter(|a| PROPS.iter().any(|(n, _)| n == a))
        .unwrap_or(FALLBACK_ART);
    Some(Face {
        plate: materialise(PROPS, "prop", "png", art)?,
        scene: materialise(SCENES, "scene", "webp", art)?,
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

/// Materialise one embedded asset to the runtime dir and hand back its path.
///
/// gpui sniffs the format from the bytes (`image::guess_format`), not from the
/// name, so the extension is for humans and for anything that filters by one —
/// but it is still written truthfully, because a `.png` holding WebP bytes is a
/// trap for the next person to look in the runtime directory.
fn materialise(table: &[(&str, &[u8])], kind: &str, ext: &str, art: &str) -> Option<PathBuf> {
    let bytes = table.iter().find(|(n, _)| *n == art).map(|(_, b)| *b)?;
    Some(crate::art::runtime_asset(
        &format!("terminal-delight-{kind}-{art}.{ext}"),
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

    /// Every drawing the manifest names is compiled in, in BOTH sizes. A prop
    /// added to the playhouse and synced without adding it to `art_tables!`
    /// fails HERE — and a name in `art_tables!` with only one of its two files
    /// fails earlier still, at `include_bytes!`.
    #[test]
    fn every_named_drawing_is_embedded_in_both_sizes() {
        for (tool, row) in table() {
            let Some(art) = row.art.as_deref() else {
                continue;
            };
            for (kind, t) in [("prop", PROPS), ("scene", SCENES)] {
                assert!(
                    t.iter().any(|(n, _)| *n == art),
                    "{tool} wants {kind} '{art}', which is not embedded"
                );
            }
        }
        assert!(PROPS.iter().any(|(n, _)| *n == FALLBACK_ART));
        assert!(SCENES.iter().any(|(n, _)| *n == FALLBACK_ART));
        assert_eq!(
            PROPS.len(),
            SCENES.len(),
            "the two tables come from one list and cannot differ in length"
        );
    }

    /// Every embedded asset is a real image OF THE RIGHT KIND — props are still
    /// PNGs, scenes are animated WebP.
    ///
    /// The `ANIM` assertion is the one that earns its place. gpui sniffs the
    /// format from the bytes, so a still image in the scenes table would render
    /// perfectly happily and simply never move, and nobody would file that as a
    /// bug — they would just think the animation had been dropped.
    #[test]
    fn every_embedded_asset_is_the_image_kind_it_claims() {
        for (name, bytes) in PROPS {
            assert!(bytes.len() > 500, "prop {name} suspiciously small");
            assert!(
                bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
                "prop {name} is not a PNG stream"
            );
        }
        for (name, bytes) in SCENES {
            assert!(bytes.len() > 2000, "scene {name} suspiciously small");
            assert!(
                bytes.starts_with(b"RIFF") && bytes[8..12] == *b"WEBP",
                "scene {name} is not a WebP stream"
            );
            // `VP8X` is the extended header, and animation lives behind it —
            // a single-frame WebP would still render, just stand perfectly
            // still, which is the failure nobody would notice.
            assert!(
                bytes.windows(4).any(|w| w == b"ANIM"),
                "scene {name} carries no animation chunk"
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
        assert!(
            known.scene.ends_with("terminal-delight-scene-console.webp"),
            "a face carries BOTH sizes: the still prop for the header, the animated robot for the wall"
        );
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

    /// THE regression. No machine ships a `tool-props.toml`, so the absent-file
    /// path is the only path that ever runs, and a derived `Default` silently
    /// made it `false` — the sweep returned nothing, no pane ever wore a tool,
    /// and every other test in this file still passed because none of them went
    /// through the switch. Assert the default the feature actually ships with.
    #[test]
    fn the_feature_is_on_when_there_is_no_config_file() {
        assert!(
            UserLayer::default().follows_tool,
            "a missing tool-props.toml must leave the feature ON"
        );
        assert!(follows_tool(), "and that is what the accessor reports");
    }

    /// A user layer that names only props must not switch the feature off as a
    /// side effect — the same trap one level down, in serde this time.
    #[test]
    fn a_partial_config_keeps_the_feature_on() {
        let l: UserLayer = toml::from_str("[props.frob]\nart = \"wrench\"\n").unwrap();
        assert!(l.follows_tool);
        assert_eq!(l.props["frob"].art.as_deref(), Some("wrench"));

        let off: UserLayer = toml::from_str("follows_tool = false\n").unwrap();
        assert!(!off.follows_tool, "and it can still be switched off");
    }

    #[test]
    fn tints_parse_and_bad_ones_do_not() {
        assert_eq!(tint_rgb("#c98500"), Some(0xc98500));
        assert_eq!(tint_rgb("2aa9a0"), Some(0x2aa9a0));
        assert_eq!(tint_rgb("#fff"), None);
        assert_eq!(tint_rgb("nonsense"), None);
    }
}
