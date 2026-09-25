//! Markdown drawn inside the pane: comrak's tree, parsed once, built into gpui
//! elements every frame.
//!
//! # Where this came from
//!
//! Lifted from markdown-delight's `app/src/render.rs` at commit
//! `425041bd36d31cd44d25984ee9cd71d071ead948` (MIT, © 2026 Parker Brown /
//! brown-family-sports; the notice is carried in `THIRD-PARTY-LICENSES.md`),
//! together with `BlockMeta`, `normalize` and `fingerprint` from its
//! `comments.rs`. Both apps build against the same Zed pin
//! (`abbe85a3321bf6cb7f5b241e623d9c2e16c29187`) and comrak 0.52, so the copied
//! parsing and layout code compiles as it was written. `paragraph_text` was
//! left behind, and so were `BlockMeta`'s plain text and source range: their
//! only readers were markdown-delight's comment threads.
//!
//! # What changed, and why
//!
//! 1. **The palette is a parameter.** The six hacker-green constants are gone;
//!    every colour reads a field of [`MdPalette`], which is built from the
//!    pane's resolved theme. Headings, links, list markers and the quote bar
//!    take the accent; code and quote grounds and the table header take the
//!    surface; rules and borders take `faint`; inline code takes the
//!    complement, so it is never mistaken for a link. `human` is left out on
//!    purpose: it is the colour of Parker's own typing, and nothing in a
//!    document is his typing.
//! 2. **Links keep their targets.** A link's byte range and URL are recorded as
//!    the inline text is collected, and the laid-out text of every paragraph
//!    that holds one is pushed to a [`LinkSink`] as it is built. A press finds
//!    the link with [`link_at`] after the pane has un-bent the pointer. Nothing
//!    here is an `InteractiveText`: its click handler is a gpui hitbox, and a
//!    hitbox fires beside what it draws on a bent pane.
//! 3. **Images are drawn.** A paragraph holding only images becomes one
//!    [`Block::Image`] each, drawn from pixels the view decoded and owns (see
//!    [`super::DocumentView`]), so every texture can be given back. A remote
//!    image is never fetched.
//!
//! And one the plan did not list: a run records what the text IS — bold, a
//! link, code — rather than the colour it is painted. The original baked the
//! colour in at parse time, which was fine for one fixed palette and is wrong
//! for a pane that can be re-themed while a document is open, or for the
//! bench's memo of parsed surfaces, which serves every theme at once.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use comrak::nodes::{AstNode, ListType, NodeValue};
use comrak::{parse_document, Anchorizer, Arena, Options};
use gpui::{
    canvas, div, img, prelude::*, px, AnyElement, Div, FontStyle, FontWeight, HighlightStyle, Hsla,
    ImageSource, ObjectFit, Pixels, Point, RenderImage, ScrollDelta, SharedString,
    StrikethroughStyle, TextLayout, UnderlineStyle,
};

use crate::docopen::DocScroll;
use crate::skin::Skin;
use crate::theme::Theme;

/* ================= owned document model (parse once) ================= */

/// What a run of inline text is. The colour it gets is decided when the
/// element is built, from the palette in hand then (see the module notes).
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Emphasis {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: bool,
}

impl Emphasis {
    fn any(&self) -> bool {
        self.bold || self.italic || self.strike || self.code || self.link
    }
}

/// A run of styled inline text: a byte range and what it is.
pub type Runs = Vec<(Range<usize>, Emphasis)>;

pub struct Inline {
    text: SharedString,
    runs: Runs,
    /// The byte range of each link's text and where it points, outermost
    /// first, so a linked image goes where the link goes.
    links: Vec<(Range<usize>, String)>,
}

pub enum Block {
    Heading {
        level: u8,
        inline: Inline,
    },
    Paragraph(Inline),
    Code(Vec<SharedString>),
    Quote(Vec<Block>),
    List(Vec<(SharedString, Vec<Block>)>),
    Table(Vec<(bool, Vec<Inline>)>),
    Rule,
    Html(Vec<SharedString>),
    /// A picture on a line of its own. `src` is as written in the file.
    Image {
        src: String,
        alt: SharedString,
    },
}

/// Anchor-relevant view of a top-level block. markdown-delight's also carried
/// the block's plain text and source byte range, for its comment threads;
/// nothing here reads either, so they stayed behind. The line the block starts
/// on is new: the bench's compact card cuts at it.
#[derive(Clone, Debug)]
pub struct BlockMeta {
    pub fp: u64,
    /// The zero-based source line the block starts on.
    pub line: usize,
}

/// Collapse all runs of whitespace to single spaces and trim — the basis for a
/// fingerprint that survives reflow/indent churn but changes on real edits.
fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Stable content fingerprint of a block's text.
pub fn fingerprint(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    normalize(text).hash(&mut h);
    h.finish()
}

/// A parsed document: its blocks, where each came from, and the folder its
/// relative links and images are resolved against (`None` on the bench, where
/// a surface has no folder).
pub struct MdDoc {
    pub blocks: Vec<Block>,
    pub meta: Vec<BlockMeta>,
    pub dir: Option<PathBuf>,
    /// Each top-level heading's slug, the way GitHub makes it, and its block.
    anchors: Vec<(String, usize)>,
}

fn md_options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.autolink = true;
    options
}

/// Parse, and alongside the render tree emit per-block anchor metadata
/// (a fingerprint and the line the block starts on). Was markdown-delight's
/// `parse_with_meta(text)`; it now also takes the document's folder and
/// records every heading's slug for `#fragment` links.
pub fn parse(text: &str, dir: Option<&Path>) -> MdDoc {
    let arena = Arena::new();
    let root = parse_document(&arena, text, &md_options());

    let mut blocks = Vec::new();
    let mut meta = Vec::new();
    let mut anchors = Vec::new();
    let mut anchorizer = Anchorizer::new();
    for node in root.children() {
        let line = node.data.borrow().sourcepos.start.line.saturating_sub(1);
        let mut made = Vec::new();
        to_blocks(node, &mut made);
        for b in made {
            let plain = block_plain(&b);
            if matches!(b, Block::Heading { .. }) {
                anchors.push((anchorizer.anchorize(&plain), blocks.len()));
            }
            meta.push(BlockMeta {
                fp: fingerprint(&plain),
                line,
            });
            blocks.push(b);
        }
    }
    MdDoc {
        blocks,
        meta,
        dir: dir.map(Path::to_path_buf),
        anchors,
    }
}

/// Flatten a block to its plain text — the basis for a comment's fingerprint and
/// the magnifier quote. Matches what the reader sees, not the raw markdown.
pub fn block_plain(block: &Block) -> String {
    match block {
        Block::Heading { inline, .. } => inline.text.to_string(),
        Block::Paragraph(inline) => inline.text.to_string(),
        Block::Code(lines) | Block::Html(lines) => lines
            .iter()
            .map(|l| l.as_ref())
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Quote(blocks) => blocks.iter().map(block_plain).collect::<Vec<_>>().join(" "),
        Block::Rule => "—".to_string(),
        Block::List(items) => items
            .iter()
            .map(|(marker, blocks)| {
                let body = blocks.iter().map(block_plain).collect::<Vec<_>>().join(" ");
                format!("{marker} {body}")
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Table(rows) => rows
            .iter()
            .map(|(_, cells)| {
                cells
                    .iter()
                    .map(|c| c.text.to_string())
                    .collect::<Vec<_>>()
                    .join(" | ")
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Image { alt, .. } => alt.to_string(),
    }
}

/// The top-level block a `#fragment` names, matched the way GitHub matches
/// it: the slug of the heading's text, case-insensitively.
pub fn fragment_block(doc: &MdDoc, fragment: &str) -> Option<usize> {
    let want = fragment.trim_start_matches('#');
    doc.anchors
        .iter()
        .find(|(slug, _)| slug == want)
        .or_else(|| {
            let lower = want.to_lowercase();
            doc.anchors.iter().find(|(slug, _)| *slug == lower)
        })
        .map(|(_, block)| *block)
}

/// After a reload, the block whose fingerprint matches the one that was at the
/// top — the nearest such block when the text appears twice — or the nearest
/// index when that block is gone.
pub fn reanchor(old: &[BlockMeta], top: usize, new: &[BlockMeta]) -> usize {
    let Some(last) = new.len().checked_sub(1) else {
        return 0;
    };
    let nearest = top.min(last);
    let Some(was) = old.get(top) else {
        return nearest;
    };
    new.iter()
        .enumerate()
        .filter(|(_, m)| m.fp == was.fp)
        .min_by_key(|(i, _)| i.abs_diff(top))
        .map_or(nearest, |(i, _)| i)
}

/// The blocks whose source starts in the first `lines` lines, counted from the
/// top, and whether any block follows them: what a compact card shows.
pub fn opening(doc: &MdDoc, lines: usize) -> (usize, bool) {
    let count = doc.meta.iter().take_while(|m| m.line < lines).count();
    (count, count < doc.blocks.len())
}

/* ---------------- inline spans ---------------- */

fn collect_inline<'a>(
    node: &'a AstNode<'a>,
    flags: Emphasis,
    out: &mut String,
    runs: &mut Runs,
    links: &mut Vec<(Range<usize>, String)>,
) {
    let mut push = |s: &str, f: Emphasis| {
        let start = out.len();
        out.push_str(s);
        if f.any() {
            runs.push((start..out.len(), f));
        }
    };
    match &node.data.borrow().value {
        NodeValue::Text(t) => push(t, flags),
        NodeValue::Code(c) => push(
            &c.literal,
            Emphasis {
                code: true,
                ..flags
            },
        ),
        NodeValue::SoftBreak | NodeValue::LineBreak => push(" ", flags),
        NodeValue::HtmlInline(h) => push(h, flags),
        NodeValue::Strong => {
            for c in node.children() {
                collect_inline(
                    c,
                    Emphasis {
                        bold: true,
                        ..flags
                    },
                    out,
                    runs,
                    links,
                );
            }
        }
        NodeValue::Emph => {
            for c in node.children() {
                collect_inline(
                    c,
                    Emphasis {
                        italic: true,
                        ..flags
                    },
                    out,
                    runs,
                    links,
                );
            }
        }
        NodeValue::Strikethrough => {
            for c in node.children() {
                collect_inline(
                    c,
                    Emphasis {
                        strike: true,
                        ..flags
                    },
                    out,
                    runs,
                    links,
                );
            }
        }
        // Change 2. An image inside running text is drawn as its alt text,
        // linked to the image file, which opens it in the square.
        NodeValue::Link(l) | NodeValue::Image(l) => {
            let start = out.len();
            let slot = links.len();
            links.push((start..start, l.url.clone()));
            for c in node.children() {
                collect_inline(
                    c,
                    Emphasis {
                        link: true,
                        ..flags
                    },
                    out,
                    runs,
                    links,
                );
            }
            if out.len() > start {
                links[slot].0.end = out.len();
            } else {
                links.remove(slot);
            }
        }
        _ => {
            for c in node.children() {
                collect_inline(c, flags, out, runs, links);
            }
        }
    }
}

fn inline_of<'a>(node: &'a AstNode<'a>) -> Inline {
    let mut text = String::new();
    let mut runs = Vec::new();
    let mut links = Vec::new();
    for c in node.children() {
        collect_inline(c, Emphasis::default(), &mut text, &mut runs, &mut links);
    }
    if text.is_empty() {
        text.push(' ');
    }
    Inline {
        text: text.into(),
        runs,
        links,
    }
}

fn lines_of(literal: &str) -> Vec<SharedString> {
    literal
        .trim_end_matches('\n')
        .split('\n')
        .map(|l| {
            SharedString::from(if l.is_empty() {
                " ".to_string()
            } else {
                l.to_string()
            })
        })
        .collect()
}

/* ---------------- AST → Block ---------------- */

/// Change 3: a paragraph holding nothing but images (and the breaks and spaces
/// between them) is those images, one block each. `None` for anything else.
fn only_images<'a>(node: &'a AstNode<'a>) -> Option<Vec<Block>> {
    if !matches!(node.data.borrow().value, NodeValue::Paragraph) {
        return None;
    }
    let mut images = Vec::new();
    for c in node.children() {
        match &c.data.borrow().value {
            NodeValue::Image(l) => {
                let alt = inline_of(c);
                images.push(Block::Image {
                    src: l.url.clone(),
                    alt: alt.text.trim().to_string().into(),
                });
            }
            NodeValue::SoftBreak | NodeValue::LineBreak => {}
            NodeValue::Text(t) if t.trim().is_empty() => {}
            _ => return None,
        }
    }
    (!images.is_empty()).then_some(images)
}

fn to_blocks<'a>(node: &'a AstNode<'a>, out: &mut Vec<Block>) {
    match only_images(node) {
        Some(images) => out.extend(images),
        None => out.push(to_block(node)),
    }
}

fn children_blocks<'a>(node: &'a AstNode<'a>) -> Vec<Block> {
    let mut out = Vec::new();
    for c in node.children() {
        to_blocks(c, &mut out);
    }
    out
}

fn to_block<'a>(node: &'a AstNode<'a>) -> Block {
    match &node.data.borrow().value {
        NodeValue::Heading(h) => Block::Heading {
            level: h.level,
            inline: inline_of(node),
        },
        NodeValue::Paragraph => Block::Paragraph(inline_of(node)),
        NodeValue::CodeBlock(cb) => Block::Code(lines_of(&cb.literal)),
        NodeValue::BlockQuote => Block::Quote(children_blocks(node)),
        NodeValue::ThematicBreak => Block::Rule,
        NodeValue::HtmlBlock(hb) => Block::Html(lines_of(&hb.literal)),
        NodeValue::Table(_) => Block::Table(
            node.children()
                .map(|row| {
                    let header = matches!(&row.data.borrow().value, NodeValue::TableRow(true));
                    (header, row.children().map(inline_of).collect())
                })
                .collect(),
        ),
        NodeValue::List(l) => {
            let ordered = l.list_type == ListType::Ordered;
            let mut n = l.start;
            Block::List(
                node.children()
                    .map(|item| {
                        let marker = item_marker(item, ordered, &mut n);
                        (SharedString::from(marker), children_blocks(item))
                    })
                    .collect(),
            )
        }
        _ => Block::Paragraph(inline_of(node)),
    }
}

fn item_marker<'a>(item: &'a AstNode<'a>, ordered: bool, n: &mut usize) -> String {
    if let NodeValue::TaskItem(t) = &item.data.borrow().value {
        return if t.symbol.is_some() {
            "☑".into()
        } else {
            "☐".into()
        };
    }
    if ordered {
        let m = format!("{n}.");
        *n += 1;
        m
    } else {
        "•".into()
    }
}

/* ================= links and images ================= */

/// A paragraph's laid-out text and its link ranges, pushed at build time and
/// read after paint.
pub struct LinkSpan {
    pub layout: TextLayout,
    pub links: Vec<(Range<usize>, String)>,
}

/// Every paragraph with a link in it, as the last frame built them. Cleared at
/// the top of each render.
pub type LinkSink = Rc<RefCell<Vec<LinkSpan>>>;

/// The link under a flat window point, as the href was written.
///
/// Only valid after the sink's spans have been painted: `TextLayout` panics
/// when asked about text it has not measured, and has no way to be asked
/// first. The view calls this only once its own last-painted recorder has
/// run, and gpui prepaints every child before that recorder.
pub fn link_at(spans: &[LinkSpan], flat: Point<Pixels>) -> Option<String> {
    spans.iter().find_map(|span| {
        if !span.layout.bounds().contains(&flat) {
            return None;
        }
        let ix = span.layout.index_for_position(flat).ok()?;
        span.links
            .iter()
            .find(|(range, _)| range.contains(&ix))
            .map(|(_, url)| url.clone())
    })
}

/// Pixels the view decoded for a document's local images, keyed by the
/// resolved path. A path absent from the map is still decoding; one mapped to
/// `Err` could not be drawn, and the error is a sentence.
pub type Images = HashMap<PathBuf, Result<Arc<RenderImage>, String>>;

/// Where an image's pixels would come from.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ImageRef {
    /// A file on this machine.
    Local(PathBuf),
    /// A web address. Never fetched: a document on screen makes no requests.
    Remote { host: String },
    /// Relative with no folder to resolve against (the bench), or a scheme
    /// that is neither a file nor the web.
    Unresolved,
}

/// Resolve an image source against the document's folder.
pub fn image_ref(dir: Option<&Path>, src: &str) -> ImageRef {
    let lower = src.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        let rest = src.split_once("://").map_or("", |(_, rest)| rest);
        let host = rest.split(['/', '?', '#']).next().unwrap_or("");
        return ImageRef::Remote {
            host: host.to_string(),
        };
    }
    match super::resolve_link(dir.unwrap_or(Path::new("")), src) {
        super::LinkTarget::File { path, .. } if path.is_absolute() => ImageRef::Local(path),
        _ => ImageRef::Unresolved,
    }
}

/// Every local image the document draws, each once, in reading order: what
/// the view decodes before building.
pub fn image_paths(doc: &MdDoc) -> Vec<PathBuf> {
    fn walk(blocks: &[Block], dir: Option<&Path>, out: &mut Vec<PathBuf>) {
        for b in blocks {
            match b {
                Block::Image { src, .. } => {
                    if let ImageRef::Local(p) = image_ref(dir, src) {
                        if !out.contains(&p) {
                            out.push(p);
                        }
                    }
                }
                Block::Quote(inner) => walk(inner, dir, out),
                Block::List(items) => {
                    for (_, inner) in items {
                        walk(inner, dir, out);
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&doc.blocks, doc.dir.as_deref(), &mut out);
    out
}

/// The box an image draws in: one image pixel per device pixel at most, so a
/// screenshot stays sharp, and never wider than the column it sits in. `cap`
/// is `None` before the column has been measured.
pub fn image_box(w: i32, h: i32, scale: f32, cap: Option<f32>) -> (f32, f32) {
    let (w, h) = (w.max(1) as f32, h.max(1) as f32);
    let natural = w / scale.max(0.1);
    let drawn = cap.map_or(natural, |cap| natural.min(cap.max(1.0)));
    (drawn, drawn * h / w)
}

/* ================= the palette ================= */

/// Change 1: every colour the renderer paints, from the theme in hand.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MdPalette {
    pub bg: Hsla,
    pub surface: Hsla,
    pub text: Hsla,
    pub accent: Hsla,
    pub faint: Hsla,
    pub complement: Hsla,
}

impl MdPalette {
    pub fn from_theme(th: &Theme) -> MdPalette {
        MdPalette {
            bg: th.bg,
            surface: th.surface,
            text: th.text,
            accent: th.accent,
            faint: th.faint,
            complement: th.complement,
        }
    }

    /// Quotes, strikethrough and raw HTML: the text, quieter.
    fn muted(&self) -> Hsla {
        self.text.alpha(0.62)
    }
}

/// Everything a document is drawn with besides its blocks.
#[derive(Clone, Copy, Debug)]
pub struct MdStyle {
    pub palette: MdPalette,
    /// Body text. Headings are sized from it, so a pane's text dial moves the
    /// whole page and the bench's gauge moves the whole card.
    pub body: Pixels,
    /// Code blocks, tables and placeholders. The skin's own on the bench.
    pub radius: Pixels,
    /// The column's width at the last paint, the widest an image may draw.
    pub image_cap: Option<f32>,
    /// The window's scale factor.
    pub scale: f32,
}

impl MdStyle {
    /// A document in the pane, in the pane's palette and at its text size.
    pub fn document(th: &Theme, zoom: f32) -> MdStyle {
        MdStyle {
            palette: MdPalette::from_theme(th),
            body: px(th.font_size * zoom),
            radius: px(6.),
            image_cap: None,
            scale: 1.0,
        }
    }

    /// A Markdown card on the bench, through the pane's gauge and the skin.
    pub fn bench(sk: &Skin, th: &Theme) -> MdStyle {
        MdStyle {
            palette: MdPalette::from_theme(th),
            body: px(sk.pt(crate::workbench::Step::Body)),
            radius: sk.radius(),
            image_cap: None,
            scale: 1.0,
        }
    }
}

/// A run's paint. A link is underlined only where it can be pressed: on the
/// bench there is nothing to press, so it is the accent and nothing more.
fn run_style(e: Emphasis, p: &MdPalette, pressable: bool) -> HighlightStyle {
    HighlightStyle {
        color: if e.code {
            Some(p.complement)
        } else if e.link {
            Some(p.accent)
        } else {
            None
        },
        font_weight: e.bold.then_some(FontWeight::BOLD),
        font_style: e.italic.then_some(FontStyle::Italic),
        background_color: e.code.then_some(p.surface),
        underline: (e.link && pressable).then_some(UnderlineStyle {
            thickness: px(1.),
            color: Some(p.accent),
            wavy: false,
        }),
        strikethrough: e.strike.then(|| StrikethroughStyle {
            thickness: px(1.),
            color: Some(p.muted()),
        }),
        ..Default::default()
    }
}

/// A heading's size against the body, and the space above it, both as
/// multiples of the body size. markdown-delight's 24/20/17/15 over a 15 body.
fn heading_scale(level: u8) -> (f32, f32) {
    match level {
        1 => (1.6, 0.667),
        2 => (1.333, 0.533),
        3 => (1.133, 0.4),
        _ => (1.0, 0.267),
    }
}

/* ================= Block → GPUI elements (per frame) ================= */

/// A line of text that the bench's drag-selection can collect when it is
/// being built on the bench, and a plain `StyledText` everywhere else.
fn text(s: SharedString) -> gpui::StyledText {
    crate::benchdraw::sel(s)
}

fn styled(inline: &Inline, style: &MdStyle, links: Option<&LinkSink>) -> AnyElement {
    let pressable = links.is_some();
    let styled = text(inline.text.clone()).with_highlights(
        inline
            .runs
            .iter()
            .map(|(range, e)| (range.clone(), run_style(*e, &style.palette, pressable))),
    );
    if let Some(sink) = links {
        if !inline.links.is_empty() {
            sink.borrow_mut().push(LinkSpan {
                layout: styled.layout().clone(),
                links: inline.links.clone(),
            });
        }
    }
    styled.into_any_element()
}

/// Window y of each top-level block at the last paint; `None` until painted.
pub type Tops = Rc<RefCell<Vec<Option<f32>>>>;

/// Every block, top to bottom. `links: None` draws links that cannot be
/// pressed, and `images: None` draws every image as its placeholder: the
/// bench. `tops`, when given, records where each block landed.
pub fn document(
    doc: &MdDoc,
    style: &MdStyle,
    links: Option<&LinkSink>,
    images: Option<&Images>,
    tops: Option<&Tops>,
) -> Div {
    column(doc, doc.blocks.len(), style, links, images, tops)
}

/// The first `count` blocks as one column: [`document`] cut short, for the
/// compact card.
pub fn column(
    doc: &MdDoc,
    count: usize,
    style: &MdStyle,
    links: Option<&LinkSink>,
    images: Option<&Images>,
    tops: Option<&Tops>,
) -> Div {
    if let Some(tops) = tops {
        let mut tops = tops.borrow_mut();
        tops.clear();
        tops.resize(count.min(doc.blocks.len()), None);
    }
    div()
        .flex()
        .flex_col()
        .gap(style.body * 0.75)
        .text_size(style.body)
        .text_color(style.palette.text)
        .children(doc.blocks.iter().take(count).enumerate().map(|(i, b)| {
            let el = block(b, doc, style, links, images);
            match tops {
                Some(tops) => {
                    let tops = tops.clone();
                    div()
                        .relative()
                        .child(el)
                        .child(
                            canvas(
                                move |bounds, _, _| {
                                    if let Some(slot) = tops.borrow_mut().get_mut(i) {
                                        *slot = Some(f32::from(bounds.origin.y));
                                    }
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .inset_0(),
                        )
                        .into_any_element()
                }
                None => el,
            }
        }))
}

/// One block. Was markdown-delight's `block_element(&Block)` and `element(&Block)`.
pub fn block(
    block: &Block,
    doc: &MdDoc,
    style: &MdStyle,
    links: Option<&LinkSink>,
    images: Option<&Images>,
) -> AnyElement {
    let p = &style.palette;
    let inner = |b: &Block| self::block(b, doc, style, links, images);
    match block {
        Block::Heading { level, inline } => {
            let (size, top_pad) = heading_scale(*level);
            let el = div()
                .pt(style.body * top_pad)
                .text_size(style.body * size)
                .font_weight(FontWeight::BOLD)
                .text_color(p.accent)
                .child(styled(inline, style, links));
            if *level <= 2 {
                el.pb_1()
                    .border_b_1()
                    .border_color(p.faint)
                    .into_any_element()
            } else {
                el.into_any_element()
            }
        }
        Block::Paragraph(inline) => div().child(styled(inline, style, links)).into_any_element(),
        Block::Quote(blocks) => div()
            .border_l_2()
            .border_color(p.accent)
            .pl_3()
            .py_1()
            .bg(p.surface)
            .text_color(p.muted())
            .flex()
            .flex_col()
            .gap_2()
            .children(blocks.iter().map(inner))
            .into_any_element(),
        Block::Code(lines) => div()
            .bg(p.surface)
            .border_1()
            .border_color(p.faint)
            .rounded(style.radius)
            .p_3()
            .my_1()
            .flex()
            .flex_col()
            .children(lines.iter().map(|l| div().child(text(l.clone()))))
            .into_any_element(),
        Block::Rule => div().h(px(1.)).my_2().bg(p.faint).into_any_element(),
        Block::Html(lines) => div()
            .text_color(p.muted())
            .flex()
            .flex_col()
            .children(lines.iter().map(|l| div().child(text(l.clone()))))
            .into_any_element(),
        Block::List(items) => div()
            .flex()
            .flex_col()
            .gap_1()
            .pl(px(4.))
            .children(items.iter().map(|(marker, blocks)| {
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_2()
                    .child(
                        div()
                            .flex_none()
                            .min_w(style.body * 1.2)
                            .text_color(p.accent)
                            .child(marker.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .children(blocks.iter().map(inner)),
                    )
            }))
            .into_any_element(),
        Block::Table(rows) => div()
            .my_1()
            .border_1()
            .border_color(p.faint)
            .rounded(style.radius)
            .flex()
            .flex_col()
            .children(rows.iter().map(|(header, cells)| {
                let mut r = div().flex().flex_row().border_b_1().border_color(p.faint);
                if *header {
                    r = r
                        .bg(p.surface)
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.accent);
                }
                r.children(cells.iter().map(|cell| {
                    div()
                        .flex_1()
                        .min_w_0()
                        .px_2()
                        .py_1()
                        .border_r_1()
                        .border_color(p.faint)
                        .child(styled(cell, style, links))
                }))
            }))
            .into_any_element(),
        Block::Image { src, alt } => image(src, alt, doc, style, images),
    }
}

/// An image block: the pixels when the view has them, otherwise a dashed box
/// that says what would be there and why it is not.
fn image(
    src: &str,
    alt: &SharedString,
    doc: &MdDoc,
    style: &MdStyle,
    images: Option<&Images>,
) -> AnyElement {
    let p = &style.palette;
    let name = if alt.is_empty() {
        src.rsplit('/').next().unwrap_or(src).to_string()
    } else {
        alt.to_string()
    };
    let placeholder = |line: String| {
        div()
            .my_1()
            .px_2()
            .py_1()
            .border_1()
            .border_dashed()
            .border_color(p.faint)
            .rounded(style.radius)
            .text_color(p.muted())
            .child(text(line.into()))
            .into_any_element()
    };
    match image_ref(doc.dir.as_deref(), src) {
        ImageRef::Local(path) => match images.and_then(|m| m.get(&path)) {
            Some(Ok(pixels)) => {
                let size = pixels.size(0);
                let (w, h) = image_box(size.width.0, size.height.0, style.scale, style.image_cap);
                img(ImageSource::Render(pixels.clone()))
                    .w(px(w))
                    .h(px(h))
                    .max_w_full()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element()
            }
            Some(Err(why)) => placeholder(format!("image · {name} — {why}")),
            None if images.is_some() => placeholder(format!("image · {name} — decoding…")),
            None => placeholder(format!("image · {name}")),
        },
        ImageRef::Remote { host } => {
            placeholder(format!("image · {name} — on {host}, not fetched"))
        }
        ImageRef::Unresolved => placeholder(format!("image · {name}")),
    }
}

/* ================= the bench's memo ================= */

thread_local! {
    /// Surface bodies parsed once rather than every frame, newest first.
    static PARSED: RefCell<VecDeque<(String, Rc<MdDoc>)>> = const { RefCell::new(VecDeque::new()) };
}

/// How many surface bodies the bench keeps parsed.
const PARSED_KEEP: usize = 32;

/// A bench surface's body, parsed. The bench draws every frame, and a card's
/// body almost never changes between two of them.
pub fn parsed(body: &str) -> Rc<MdDoc> {
    PARSED.with(|memo| {
        let mut memo = memo.borrow_mut();
        if let Some(at) = memo.iter().position(|(seen, _)| seen == body) {
            if let Some(hit) = memo.remove(at) {
                let doc = hit.1.clone();
                memo.push_front(hit);
                return doc;
            }
        }
        let doc = Rc::new(parse(body, None));
        memo.push_front((body.to_string(), doc.clone()));
        memo.truncate(PARSED_KEEP);
        doc
    })
}

/* ================= the view's state ================= */

/// Space around the column inside the view, in logical pixels.
pub const PAD: f32 = 14.;

/// A Markdown document on screen: what was parsed, where it is scrolled, and
/// every image it holds.
pub struct MarkdownDoc {
    /// `None` while the first read is running. The error is a sentence.
    pub doc: Option<Result<Rc<MdDoc>, String>>,
    /// Bumped by every parse, so a measurement of the old layout is never
    /// used to place the new one.
    pub generation: u64,
    /// How far the column is scrolled from its top, in logical pixels. By
    /// hand: nothing under `docview` may scroll by itself (see the module notes
    /// in `docview.rs`).
    pub top: f32,
    /// Window y of each top-level block at the last paint.
    pub tops: Tops,
    /// The column's window y and height at the last paint.
    pub column: Rc<Cell<Option<(f32, f32)>>>,
    /// Which parse the last paint drew.
    pub painted: Rc<Cell<Option<u64>>>,
    /// Where to put the view once the current parse has been laid out.
    pub pending: Option<Pending>,
    /// Decoded, owned, and given back at release.
    pub images: Images,
    /// Decodes in flight, by path. Dropping one cancels it.
    pub decoding: HashMap<PathBuf, gpui::Task<()>>,
    /// The file being read and parsed off the main thread, when it is; a
    /// newer read replaces it. See `markdown_view.rs`.
    pub reading: gpui::Task<()>,
    /// Whether every image had been drawn, for `TD_DOCDEBUG`'s one line.
    pub drew_all: bool,
}

/// A place in the document to go to once it has been laid out.
#[derive(Clone, PartialEq, Debug)]
pub enum Pending {
    /// After a reload: this block, this far into it; else this fraction of
    /// the page, when the block's new position is not known.
    Block {
        index: usize,
        offset: f32,
        fraction: Option<f32>,
    },
    /// A heading, by its `#fragment`.
    Fragment(String),
}

/// Scroll never shows past either end of the column.
pub fn clamp_top(top: f32, content_h: f32, view_h: f32) -> f32 {
    top.min(content_h - view_h).max(0.0)
}

/// A wheel turn in logical pixels, positive toward the top of the document,
/// as gpui signs it. A notch moves three lines, as the terminal's does.
pub fn wheel_px(delta: ScrollDelta, line: f32) -> f32 {
    match delta {
        ScrollDelta::Lines(l) => l.y * line * 3.0,
        ScrollDelta::Pixels(p) => f32::from(p.y),
    }
}

impl Default for MarkdownDoc {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownDoc {
    pub fn new() -> Self {
        MarkdownDoc {
            doc: None,
            generation: 0,
            top: 0.0,
            tops: Rc::default(),
            column: Rc::default(),
            painted: Rc::default(),
            pending: None,
            images: Images::new(),
            decoding: HashMap::new(),
            reading: gpui::Task::ready(()),
            drew_all: false,
        }
    }

    /// The column's height, once the current parse has been painted.
    fn content_h(&self) -> Option<f32> {
        (self.painted.get() == Some(self.generation))
            .then(|| self.column.get().map(|(_, h)| h))
            .flatten()
    }

    /// Where a top-level block sits in the column, from the last paint.
    fn block_y(&self, index: usize) -> Option<f32> {
        let (col_y, _) = self.column.get()?;
        let y = (*self.tops.borrow().get(index)?)?;
        Some(y - col_y)
    }

    /// The top of the view as a fraction of the page. `None` before the
    /// first layout: nothing has been measured, which is not the top.
    pub fn scroll(&self) -> Option<DocScroll> {
        let h = self.content_h()?;
        Some(DocScroll {
            top: if h > 0.0 {
                (self.top / h).clamp(0.0, 1.0)
            } else {
                0.0
            },
        })
    }

    /// The block at the top of the view and how far into it the view is.
    pub fn block_at_top(&self) -> Option<(usize, f32)> {
        if self.painted.get() != Some(self.generation) {
            return None;
        }
        let mut at = None;
        for i in 0..self.tops.borrow().len() {
            match self.block_y(i) {
                Some(y) if y <= self.top => at = Some((i, self.top - y)),
                Some(_) => break,
                None => {}
            }
        }
        at
    }

    /// Put in a new parse, keeping the reader's place: the block that was at
    /// the top stays at the top. Answers the images the new parse no longer
    /// draws, for the view to give back.
    pub fn replace(&mut self, new: Rc<MdDoc>) -> Vec<Arc<RenderImage>> {
        if let Some(Ok(old)) = &self.doc {
            let fraction = self.scroll().map(|s| s.top);
            self.pending = match self.block_at_top() {
                Some((top, offset)) => Some(Pending::Block {
                    index: reanchor(&old.meta, top, &new.meta),
                    offset,
                    fraction,
                }),
                None => fraction.map(|f| Pending::Block {
                    index: usize::MAX,
                    offset: 0.0,
                    fraction: Some(f),
                }),
            };
        }
        let keep = image_paths(&new);
        self.decoding.retain(|p, _| keep.contains(p));
        let gone: Vec<PathBuf> = self
            .images
            .keys()
            .filter(|p| !keep.contains(p))
            .cloned()
            .collect();
        let dropped = gone
            .iter()
            .filter_map(|p| self.images.remove(p))
            .filter_map(Result::ok)
            .collect();
        self.doc = Some(Ok(new));
        self.generation += 1;
        self.drew_all = false;
        dropped
    }

    /// Go to a fraction of the page — a place a saved layout kept — once the
    /// page has been laid out. The same pending place a reload whose block
    /// has gone falls back to, so the two cannot disagree about what a
    /// fraction means.
    pub fn restore_fraction(&mut self, top: f32) {
        self.pending = Some(Pending::Block {
            index: usize::MAX,
            offset: 0.0,
            fraction: Some(top.clamp(0.0, 1.0)),
        });
    }

    /// Go to a heading, now if the page has been laid out, or as soon as it is.
    pub fn go_to_fragment(&mut self, fragment: String, view_h: Option<f32>) {
        self.pending = Some(Pending::Fragment(fragment));
        self.settle(view_h);
    }

    /// Resolve a pending place against the last paint, and keep the scroll
    /// inside the page. Called at the top of every render.
    pub fn settle(&mut self, view_h: Option<f32>) {
        let Some(content_h) = self.content_h() else {
            return;
        };
        match self.pending.take() {
            Some(Pending::Block {
                index,
                offset,
                fraction,
            }) => {
                if let Some(y) = self.block_y(index) {
                    self.top = y + offset;
                } else if let Some(f) = fraction {
                    self.top = f * content_h;
                }
            }
            Some(Pending::Fragment(fragment)) => {
                let block = match &self.doc {
                    Some(Ok(doc)) => fragment_block(doc, &fragment),
                    _ => None,
                };
                if let Some(y) = block.and_then(|b| self.block_y(b)) {
                    self.top = y - PAD;
                }
            }
            None => {}
        }
        if let Some(view_h) = view_h {
            self.top = clamp_top(self.top, content_h, view_h);
        }
    }

    /// Turn the wheel. Answers whether the view moved.
    pub fn wheel(&mut self, delta: ScrollDelta, line: f32, view_h: Option<f32>) -> bool {
        let was = self.top;
        self.pending = None;
        let next = self.top - wheel_px(delta, line);
        self.top = match (self.content_h(), view_h) {
            (Some(content_h), Some(view_h)) => clamp_top(next, content_h, view_h),
            _ => next.max(0.0),
        };
        (self.top - was).abs() > f32::EPSILON
    }

    /// The document's element: the column, offset by the scroll, measured as
    /// it paints. `view` is the view's size at the last paint.
    pub fn element(
        &mut self,
        path: &Path,
        th: &Theme,
        view: Option<gpui::Size<Pixels>>,
        links: &LinkSink,
        window: &mut gpui::Window,
    ) -> AnyElement {
        let note = |s: String| {
            div()
                .p(px(PAD))
                .text_color(th.text.alpha(0.75))
                .child(s)
                .into_any_element()
        };
        let doc = match &self.doc {
            None => return note("reading…".into()),
            Some(Err(why)) => return note(why.clone()),
            Some(Ok(doc)) => doc.clone(),
        };
        self.settle(view.map(|v| f32::from(v.height)));
        // A place still to be found needs this frame's measurements, so ask
        // for the next frame rather than waiting for something to notify.
        if self.pending.is_some() {
            window.request_animation_frame();
        }
        let mut style = MdStyle::document(th, 1.0);
        style.scale = window.scale_factor();
        style.image_cap = view.map(|v| (f32::from(v.width) - 2.0 * PAD).max(1.0));
        let wanted = image_paths(&doc);
        if !self.drew_all && wanted.iter().all(|p| self.images.contains_key(p)) {
            self.drew_all = true;
            if std::env::var_os("TD_DOCDEBUG").is_some() {
                eprintln!("[doc] drew {} with {} images", path.display(), wanted.len());
            }
        }
        let (generation, painted, column) =
            (self.generation, self.painted.clone(), self.column.clone());
        div()
            .absolute()
            .left_0()
            .w_full()
            .top(px(-self.top))
            .p(px(PAD))
            .child(document(
                &doc,
                &style,
                Some(links),
                Some(&self.images),
                Some(&self.tops),
            ))
            .child(
                canvas(
                    move |bounds, _, _| {
                        column.set(Some((
                            f32::from(bounds.origin.y),
                            f32::from(bounds.size.height),
                        )));
                        painted.set(Some(generation));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(doc: &MdDoc) -> &Block {
        assert_eq!(doc.blocks.len(), 1, "one block");
        &doc.blocks[0]
    }

    fn inline(block: &Block) -> &Inline {
        match block {
            Block::Paragraph(i) | Block::Heading { inline: i, .. } => i,
            _ => panic!("not text"),
        }
    }

    /// A link that forgot where it points is text in a different colour, and
    /// markdown-delight's renderer kept only the text. The target has to come
    /// through the parse with the byte range it covers, or a press can find
    /// the words and never the destination.
    /// A place a saved layout kept waits for the page to be laid out — the
    /// file is still being read when a restore asks — and then puts the same
    /// fraction of the page at the top of the view.
    #[test]
    fn a_saved_place_is_restored_once_the_page_is_laid_out() {
        let mut md = MarkdownDoc::new();
        md.restore_fraction(0.37);
        md.settle(Some(100.0));
        assert_eq!(md.top, 0.0, "nothing is laid out yet, so nothing moves");
        assert!(md.pending.is_some(), "and the place is still waiting");
        // The first paint of this parse: a column 1,000 pixels tall.
        md.painted.set(Some(md.generation));
        md.column.set(Some((0.0, 1000.0)));
        md.settle(Some(100.0));
        assert!((md.top - 370.0).abs() < 1e-3, "{}", md.top);
        assert!(md.pending.is_none(), "restored once, then left alone");
        let back = md.scroll().expect("measured now").top;
        assert!((back - 0.37).abs() < 1e-4, "{back}");
        // A place past the end is the end, never beyond it.
        md.restore_fraction(1.7);
        md.settle(Some(100.0));
        assert!((md.top - 900.0).abs() < 1e-3, "{}", md.top);
    }

    #[test]
    fn a_link_keeps_its_target() {
        let doc = parse("[a](b.md#x)", None);
        let i = inline(only(&doc));
        assert_eq!(i.text.as_ref(), "a");
        assert_eq!(i.links, vec![(0..1, "b.md#x".to_string())]);

        let doc = parse("see https://example.com/x now", None);
        let i = inline(only(&doc));
        let (range, url) = i.links.first().expect("the autolink is a link");
        assert_eq!(url, "https://example.com/x");
        assert_eq!(&i.text[range.clone()], "https://example.com/x");

        // A linked picture goes where the link goes, not to the picture.
        let doc = parse("[![logo](l.png)](https://example.com)", None);
        let i = inline(only(&doc));
        assert_eq!(i.links[0].1, "https://example.com");
    }

    #[test]
    fn a_paragraph_that_is_only_an_image_becomes_an_image_block() {
        let doc = parse("![alt](p.png)", None);
        match only(&doc) {
            Block::Image { src, alt } => {
                assert_eq!(src, "p.png");
                assert_eq!(alt.as_ref(), "alt");
            }
            _ => panic!("an image on its own line is an image block"),
        }
        let doc = parse("![a](1.png)\n![b](2.png)", None);
        assert_eq!(doc.blocks.len(), 2, "two pictures, two blocks");
        assert!(doc.blocks.iter().all(|b| matches!(b, Block::Image { .. })));

        // Inside running text it stays text: the alt, linked to the file.
        let doc = parse("see ![alt](p.png) here", None);
        let i = inline(only(&doc));
        assert_eq!(i.text.as_ref(), "see alt here");
        assert_eq!(i.links, vec![(4..7, "p.png".to_string())]);
    }

    /// A document on screen makes no network requests. A web image draws a
    /// box naming its host; it is never handed to a loader, and no image the
    /// renderer draws takes anything but pixels the view already owns.
    #[test]
    fn a_remote_image_is_never_fetched() {
        let dir = Path::new("/docs");
        let doc = parse("![logo](https://example.com/logo.png)", Some(dir));
        assert!(matches!(only(&doc), Block::Image { .. }));
        assert_eq!(
            image_ref(Some(dir), "https://example.com/logo.png"),
            ImageRef::Remote {
                host: "example.com".into()
            }
        );
        assert!(image_paths(&doc).is_empty(), "nothing to decode");
        assert_eq!(
            image_ref(Some(dir), "shots/a b.png"),
            ImageRef::Local("/docs/shots/a b.png".into())
        );
        assert_eq!(image_ref(None, "shots/a.png"), ImageRef::Unresolved);

        let src = include_str!("markdown.rs");
        let live: String = src
            .split("#[cfg(test)]")
            .next()
            .unwrap_or(src)
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let calls: Vec<&str> = live.split("img(").skip(1).collect();
        assert!(!calls.is_empty(), "the renderer draws images");
        for call in calls {
            assert!(
                call.starts_with("ImageSource::Render("),
                "an image drawn from anything but owned pixels: img({}",
                &call[..call.len().min(40)]
            );
        }
    }

    /// Change 1, held: the renderer has no colour of its own to fall back on.
    #[test]
    fn the_lifted_renderer_carries_no_colour_of_its_own() {
        let src = include_str!("markdown.rs");
        let live = src.split("#[cfg(test)]").next().unwrap_or(src);
        for (n, line) in live.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            assert!(!code.contains("rgb(0x"), "line {}: {line}", n + 1);
            let t = code.trim_start();
            let is_colour_const = (t.starts_with("const ") || t.starts_with("pub const "))
                && t.contains(": u32 = 0x");
            assert!(!is_colour_const, "line {}: {line}", n + 1);
        }
    }

    #[test]
    fn headings_and_links_take_the_themes_accent() {
        let mut accents = Vec::new();
        for id in ["hacker", "gamba"] {
            let toml = crate::theme::builtin_toml(id).expect("a builtin theme");
            let th = crate::theme::parse(toml).expect("a builtin theme parses");
            let p = MdPalette::from_theme(&th);
            assert_eq!(p.accent, th.accent, "{id}");
            assert_eq!(p.surface, th.surface, "{id}");
            assert_eq!(p.faint, th.faint, "{id}");
            assert_eq!(p.complement, th.complement, "{id}");
            assert_eq!(p.text, th.text, "{id}");
            assert_eq!(p.bg, th.bg, "{id}");

            let link = Emphasis {
                link: true,
                ..Default::default()
            };
            assert_eq!(run_style(link, &p, true).color, Some(th.accent), "{id}");
            assert!(run_style(link, &p, true).underline.is_some());
            assert!(
                run_style(link, &p, false).underline.is_none(),
                "{id}: a link on the bench cannot be pressed, so it is not underlined"
            );
            let code = Emphasis {
                code: true,
                ..Default::default()
            };
            assert_eq!(
                run_style(code, &p, true).color,
                Some(th.complement),
                "{id}: inline code is not mistaken for a link"
            );
            accents.push(th.accent);
        }
        assert_ne!(accents[0], accents[1], "two themes, two accents");
        // Headings paint in the accent from the palette, not a constant.
        let src = include_str!("markdown.rs");
        let heading = src
            .split("Block::Heading { level, inline } => {")
            .nth(1)
            .expect("the heading arm");
        let heading = heading.split("Block::Paragraph").next().unwrap_or(heading);
        assert!(heading.contains(".text_color(p.accent)"), "{heading}");
    }

    fn meta(text: &str) -> Vec<BlockMeta> {
        parse(text, None).meta
    }

    #[test]
    fn a_reloaded_markdown_file_keeps_the_block_you_were_reading() {
        let old = meta("# A\n\npara one\n\n# B\n\npara two\n");
        // Reading "para two" (block 3) when a section lands above it.
        let grown = meta("# New\n\nintro\n\n# A\n\npara one\n\n# B\n\npara two\n");
        assert_eq!(reanchor(&old, 3, &grown), 5);
        // A section removed above it.
        let shrunk = meta("# B\n\npara two\n");
        assert_eq!(reanchor(&old, 3, &shrunk), 1);
        // The block itself gone: the nearest index that still exists.
        let gone = meta("# A\n\npara one\n");
        assert_eq!(reanchor(&old, 3, &gone), 1);
        // The same text twice: the occurrence nearest where you were.
        let twice = meta("x\n\n---\n\ny\n\n---\n\nz\n");
        assert_eq!(reanchor(&twice, 3, &twice), 3);
        assert_eq!(reanchor(&old, 0, &[]), 0);
    }

    #[test]
    fn comrak_is_built_without_its_defaults() {
        let manifest = include_str!("../../Cargo.toml");
        let line = manifest
            .lines()
            .find(|l| l.trim_start().starts_with("comrak"))
            .expect("comrak is a dependency");
        // A TOML comment is not a setting.
        let setting = line.split('#').next().unwrap_or(line);
        assert!(
            setting.contains("default-features = false"),
            "comrak's defaults bring clap, syntect and the onig C library: {line}"
        );
    }

    #[test]
    fn a_fragment_finds_its_heading_the_way_github_names_it() {
        let doc = parse("# Top\n\ntext\n\n## Two Words & More\n\n## Top\n", None);
        assert_eq!(fragment_block(&doc, "top"), Some(0));
        assert_eq!(fragment_block(&doc, "two-words--more"), Some(2));
        assert_eq!(fragment_block(&doc, "#Two-Words--More"), Some(2));
        assert_eq!(fragment_block(&doc, "top-1"), Some(3), "the second Top");
        assert_eq!(fragment_block(&doc, "nowhere"), None);
    }

    #[test]
    fn an_image_draws_at_most_one_pixel_per_device_pixel_and_never_wider_than_its_column() {
        assert_eq!(image_box(800, 400, 2.0, None), (400.0, 200.0));
        assert_eq!(image_box(800, 400, 1.0, Some(300.0)), (300.0, 150.0));
        assert_eq!(image_box(64, 64, 1.0, Some(300.0)), (64.0, 64.0));
    }

    #[test]
    fn the_wheel_never_scrolls_past_either_end() {
        assert_eq!(clamp_top(-20.0, 1000.0, 400.0), 0.0);
        assert_eq!(clamp_top(900.0, 1000.0, 400.0), 600.0);
        assert_eq!(
            clamp_top(50.0, 300.0, 400.0),
            0.0,
            "a short page does not scroll"
        );
        let up = ScrollDelta::Lines(gpui::point(0.0, 1.0));
        assert_eq!(wheel_px(up, 20.0), 60.0);
    }

    /// Nothing measured is not the top of the page.
    #[test]
    fn a_page_not_yet_laid_out_has_no_scroll_to_save() {
        let mut md = MarkdownDoc::new();
        assert_eq!(md.scroll(), None);
        md.replace(Rc::new(parse("# a\n\nb\n", None)));
        assert_eq!(md.scroll(), None, "parsed, but never painted");
        md.column.set(Some((100.0, 1000.0)));
        md.painted.set(Some(md.generation));
        md.top = 250.0;
        assert_eq!(md.scroll(), Some(DocScroll { top: 0.25 }));
    }
}
