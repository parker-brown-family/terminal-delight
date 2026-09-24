//! Who owns a keystroke on a pane, when several surfaces are up at once.
//!
//! # Why this is a module and not a run of `if` statements
//!
//! A pane stacks surfaces: the paint overlay is drawn across every pane in the
//! window, the header carries a ⋯ menu and an inline rename box, a right-click
//! raises a copy tray, the FOCUS modal mirrors the pane, a sticky note can hold
//! the pen, and underneath all of it the pane is showing either its terminal or
//! its workbench. Every one of them wants keys.
//!
//! That precedence used to exist only as the order of branches in
//! `TerminalView::on_key`, and an order of branches has no runtime signal at
//! all: swapping two correct blocks compiles, and every behavioural test in the
//! suite stays green. It went wrong that way five times.
//!
//! Escape was lifted out first — this module's ancestor decided that one key and
//! nothing else — and the sentence left behind in `on_key` said exactly what was
//! still wrong with it: *"Only ESCAPE goes through here. Every other key keeps
//! the path it had."* The path it had ran `bench_key` in the middle of the
//! handler, and every exit from that function stops the event, so on the
//! workbench face the hundred lines below it were unreachable. Twenty-one of
//! twenty-seven chords did nothing there: the inline rename box took no letters,
//! `alt+s` stuck no note, `ctrl+shift+b` opened no left bar, and `ctrl+c` could
//! not interrupt the agent whose conversation was on the screen.
//!
//! So the whole decision moved here. [`route`] is the only thing that decides,
//! for every key, the way every control in gpui-kit routes through one
//! `resolve_style` so the layering cannot drift apart between controls. A new
//! surface is a new variant, which the compiler makes every caller handle, and a
//! new row in a table somebody can argue with.
//!
//! # One ladder, and each rung says what it claims
//!
//! The obvious mistake here is two ladders — one for escape, one for typing —
//! because escape and a letter genuinely land on different layers. The FOCUS
//! reader takes escape and the paging keys and nothing else; a letter typed
//! while it is up belongs to whatever has the caret. Two ladders drift, silently
//! and in exactly the way this module exists to prevent.
//!
//! There is one ladder. Each rung carries **what that layer claims**, so the
//! reader taking escape but not a letter is one row rather than two tables, and
//! the same walk answers both questions. [`Layer`]'s declaration order **is** the
//! precedence, topmost first.
//!
//! # Two layers answer for a stack of their own
//!
//! [`Layer::Paint`] and [`Layer::Bench`] are each a surface with an internal
//! order of its own — the overlay's shelves and chords; the bench's gallery,
//! shelf chords, note buffer, composer and rail. That order belongs to them, is
//! tested in their own modules, and is deliberately not restated here.
//!
//! Both may hand a key back: `paint_key` answers `Pass` and `bench_key` answers
//! `false`, and the caller then gives it to [`Layer::Terminal`], which is the
//! floor for both. **The bench handing a key back is the change this table was
//! rewritten for.** It used to end every path in `stop_propagation`, so a key it
//! had no use for died there instead of reaching the agent whose conversation
//! was on the screen.
//!
//! # Why this module imports nothing
//!
//! Zero `use` statements, like `tree.rs`. It compiles standalone, so
//! `rustc --test app/src/keylayer.rs` runs this file's tests in about a second
//! against `cargo test`'s several minutes — which is what makes mutation-testing
//! the table affordable enough that it actually gets done.
//!
//! The consequence is that a claim needing pane state arrives as an input:
//! [`Up::note`] is what makes `alt+backspace` a peel rather than readline's
//! kill-word. A claim that belongs to another module stays in that module,
//! tested there, and arrives here as an answer.

/// What a keystroke is for, on a pane, right now.
///
/// **Declaration order is precedence, topmost first.** Moving a variant in this
/// list changes behaviour; that is the point, and it is why the list is short
/// enough to read in one go.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Layer {
    /// `F1` — the help modal, from anywhere, on either face.
    Help,
    /// `alt+k` — flip this pane between its two faces, from either side.
    ///
    /// Above [`Layer::Window`] because [`window_chord`] claims the letter `k` as
    /// well: the window refuses to encode it so it cannot reach a shell as
    /// `ESC k`, and the pane is the thing that acts on it.
    Face,
    /// The window's own chords — pane focus, the splits, close, the FOCUS
    /// reader. Never a pane's content, on either of its faces, so the pane
    /// consumes nothing and lets the event bubble.
    ///
    /// Above everything a pane can stack, which is the answer to the alt-chord
    /// complaint: a handler that runs early and keeps a key it cannot use is the
    /// whole bug class this module is about.
    Window,
    /// The paint overlay, drawn across every pane in the window at once. It
    /// outranks anything one pane owns because it is not one pane's surface.
    Paint,
    /// The right-click copy tray, floating at the cursor. Escape only — a menu
    /// is driven with the mouse.
    CtxMenu,
    /// The ⋯ overflow menu in the header, which is drawn on BOTH faces. Escape
    /// only, for the same reason.
    HeaderMenu,
    /// This pane mirrored large in the FOCUS modal. Escape closes it and the
    /// paging keys drive the reader's view; every other keystroke flows past, so
    /// you keep directing the agent while you read it big.
    Reader,
    /// The pane's own chords — the tab, find, cut, the panels, the note. True on
    /// both faces, which is the half of the alt-chord fix that never shipped:
    /// the *window's* chords were handed back and the *pane's* were not.
    ///
    /// **Above the two text fields below it**, and that is not a detail. `alt+s`
    /// is how you put the pen down on the note you are writing; a composer that
    /// swallowed it would leave Enter as the only way out, which is a bug this
    /// pane has already had once. The same argument the window makes one rung up
    /// — a surface may keep the keys it uses, not the ones it happens to
    /// receive — and a chord is never a character, so nothing a person types
    /// into a box is at stake.
    PaneChord,
    /// A sticky note holding the pen. A mode with a caret blinking in it, so it
    /// claims every key that is not one of the pane's own chords, until it is
    /// posted or reverted.
    Sticky,
    /// The inline rename box in the header, also drawn on both faces, also a
    /// caret you can see. Same terms as [`Layer::Sticky`].
    Rename,
    /// A document floating over the terminal face. Escape closes it, and that
    /// is all it takes: every other key still reaches the shell underneath,
    /// which is the whole point of reading beside it rather than instead of it.
    ///
    /// Below [`Layer::Sticky`] and [`Layer::Rename`], so a caret blinking
    /// somewhere else on the pane keeps its own Escape.
    Float,
    /// The workbench face and everything stacked on it. Claimed whenever that
    /// face is showing; what the bench then does with the key is its own ladder,
    /// and it may decline.
    Bench,
    /// The terminal: keyboard selection, the scrollback keys, and the
    /// pseudoterminal itself. The floor — everything not claimed above lands
    /// here, and so does everything [`Layer::Paint`] and [`Layer::Bench`] hand
    /// back, which is what makes `ctrl+c` reach a running agent from either face.
    Terminal,
}

/// One keystroke, in the terms this module decides with.
///
/// Built from a `gpui::Keystroke` at the call site rather than imported, so this
/// file keeps its second-long test run. `types_char` is the caller's answer to
/// *does this keystroke put a character in front of a person* — a question with a
/// subtlety worth keeping in one place, since gpui fills `key_char` for `alt+r`
/// with `"r"`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Key<'a> {
    pub key: &'a str,
    pub alt: bool,
    pub control: bool,
    pub shift: bool,
    pub platform: bool,
    pub function: bool,
    pub types_char: bool,
}

/// Which of a pane's surfaces are currently up.
///
/// A struct of named fields rather than a tuple, so that a call site cannot
/// silently swap two of them — they are all the same type, and the compiler has
/// nothing to say about `route(k, a, b, c, d, e, f, g)`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Up {
    pub paint: bool,
    pub ctx_menu: bool,
    pub header_menu: bool,
    /// This pane is mirrored in the FOCUS modal.
    pub reader: bool,
    /// A sticky note is holding the pen. Not merely stuck: a posted note claims
    /// nothing, because a note you stopped seeing this morning must not decide
    /// whether escape reaches your agent.
    pub sticky: bool,
    /// The inline rename box is open.
    pub rename: bool,
    /// A note is stuck on this pane — stuck, not being written. `alt+backspace`
    /// peels it, and with no note the chord belongs to readline.
    pub note: bool,
    /// The pane is showing its WORKBENCH face. Not "the bench wants the key" —
    /// what the bench does with it is its own ladder's call, and it may well be
    /// nothing.
    pub bench: bool,
    /// A document is floating over the terminal face.
    pub float: bool,
}

/// One rung: the layer, and whether it claims this keystroke.
type Rung = (Layer, fn(&Key, &Up) -> bool);

/// The ladder, in [`Layer`]'s declaration order.
///
/// Written as a list of pairs rather than a chain of early returns so that the
/// order is one readable column and adding a surface cannot accidentally be
/// written in the middle of an unrelated branch.
const LADDER: [Rung; 12] = [
    (Layer::Help, |k, _| k.key == "f1"),
    (Layer::Face, |k, _| k.alt && !k.control && k.key == "k"),
    (Layer::Window, |k, _| window_chord(k.key, k.alt, k.control)),
    (Layer::Paint, |k, u| u.paint && paints(k)),
    (Layer::CtxMenu, |k, u| u.ctx_menu && k.key == "escape"),
    (Layer::HeaderMenu, |k, u| u.header_menu && k.key == "escape"),
    (Layer::Reader, |k, u| {
        u.reader && (k.key == "escape" || paging(k).is_some())
    }),
    (Layer::PaneChord, pane_chord),
    (Layer::Sticky, |_, u| u.sticky),
    (Layer::Rename, |_, u| u.rename),
    (Layer::Float, |k, u| u.float && k.key == "escape"),
    (Layer::Bench, |_, u| u.bench),
];

/// The topmost layer that claims this keystroke, or [`Layer::Terminal`].
pub fn route(k: &Key, up: &Up) -> Layer {
    for (layer, claims) in LADDER {
        if claims(k, up) {
            return layer;
        }
    }
    Layer::Terminal
}

/// Chords the WINDOW owns — never a pane's content, on either of its faces.
///
/// # Why this is one table and not two
///
/// A pane can be showing a terminal or a bench, and both of those are *content
/// inside a window*. The window's own gestures — close this pane, split it, open
/// the FOCUS reader, move the highlight — have to survive whichever one is on
/// top, and the way they survive is that the thing on top declines to take them.
///
/// The terminal face has always done this, in `pane::keystroke_bytes`: a short
/// list of alt chords it refuses to encode, so they bubble up to the workspace
/// instead of arriving at somebody's shell as `ESC w`. The bench never got one,
/// and it ended both of its key paths by stopping propagation — so on the
/// workbench face every chord in that list was dead. `alt+w` did nothing at all,
/// which is worse than the state it replaced, because the face toggle that used
/// to sit on `alt+w` was at least handled upstream.
///
/// Extracting the list rather than copying it is the point. Two lists drift, and
/// the drift is invisible: nothing fails to compile, nothing fails a test, a
/// chord just quietly stops working on one face.
///
/// # What is deliberately NOT here
///
/// Only chords carrying `alt` (or `control`+`alt`) qualify, and that boundary is
/// doing real work in both directions:
///
/// - `ctrl+c` must reach a running agent. A bench that refused it would take away
///   the only way to interrupt a turn.
/// - `alt+b` / `alt+f` are readline's word motion, which the composer mirrors
///   through `line_edit` — so a blanket "the window takes every alt chord" would
///   break typing.
/// - A PLAIN arrow walks the bench's rail and a PLAIN escape peels its overlays.
///   Only the modified forms leave.
pub fn window_chord(key: &str, alt: bool, control: bool) -> bool {
    // ctrl+alt+<anything> walks the left bar's tree. Matched on the modifiers
    // alone, because that pair is not an editing chord anywhere.
    if control && alt {
        return true;
    }
    if !alt {
        return false;
    }
    matches!(
        key,
        "left" | "right" | "up" | "down" | "r" | "v" | "h" | "w" | "k"
    )
}

/// Chords the PANE owns, on either of its faces.
///
/// The other half of [`window_chord`], and the half that was missing. These live
/// on the pane rather than the workspace for a reason their own call sites
/// explain at length: a focused terminal takes the keystroke first, so a chord
/// bound at the workspace compiles, tests green, and does nothing when pressed.
/// The same argument applies one layer down — a bench that keeps them does the
/// same damage for the same invisible reason.
///
/// `alt+backspace` is conditional, and the condition is about not stealing a key
/// that has a better owner: it peels a note only when one is stuck. With no note
/// the chord is readline's backward-kill-word — and the bench's composer mirrors
/// that, so the note costs the same key on both faces or on neither. The
/// question is a field read, so the table can answer it.
///
/// `ctrl+x` is conditional too and is NOT answered here. It is a cut only with
/// something selected, and the only honest way to ask that is to render the
/// selection — an allocation this function would be doing on every keystroke in
/// the session to decide one chord. So the claim is unconditional and
/// `TerminalView::pane_chord_key` hands the key to the terminal when there is
/// nothing to cut, which is what makes bare `ctrl+x` still readline's prefix key
/// (`C-x C-e` opens your editor).
pub fn pane_chord(k: &Key, up: &Up) -> bool {
    if k.platform || k.function {
        return false;
    }
    if k.alt && !k.control && !k.shift {
        return match k.key {
            "s" => true,
            "backspace" => up.note,
            _ => false,
        };
    }
    if k.control && !k.alt {
        if k.shift {
            // The panels and the clipboard. Shift is what keeps raw ctrl+a / ^D /
            // ^G / ^U reaching the pseudoterminal.
            return matches!(
                k.key,
                "t" | "c" | "v" | "k" | "a" | "b" | "n" | "z" | "u" | "y" | "d" | "g" | "f"
            );
        }
        return matches!(
            k.key,
            // ^W is werase, and the tab is worth more than it here.
            "w" | "f" | "x"
        );
    }
    false
}

/// Does the raised paint overlay claim this keystroke?
///
/// Mirrors what `TerminalView::paint_key` would do with it: escape folds the
/// overlay, a plain arrow walks the wall, and a plain single character is either
/// a chord on the shelf or a deliberate no-op. Everything else passes, so the
/// overlay is never a trap — a modified chord still reaches its owner while the
/// cards are up.
fn paints(k: &Key) -> bool {
    if k.key == "escape" {
        return true;
    }
    if k.control || k.alt || k.platform {
        return false;
    }
    matches!(k.key, "left" | "right" | "up" | "down") || k.key.chars().count() == 1
}

/// A paging gesture, independent of which view answers it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Paging {
    PageUp,
    PageDown,
    Top,
    Bottom,
}

/// Which way a paging keystroke moves a view, or `None` for anything else.
///
/// Plain PageUp/PageDown page; ctrl+Home / ctrl+End jump to the ends. Any other
/// modifier combination is someone else's chord (ctrl+PageUp switches tabs, plain
/// Home/End belong to the shell), so it must NOT match here.
///
/// One table for two readers — the FOCUS modal's claim above, and the pane's own
/// scrollback keys — because they are the same four gestures and a second copy is
/// how one of them quietly stops matching.
pub fn paging(k: &Key) -> Option<Paging> {
    if k.alt || k.shift || k.platform || k.function {
        return None;
    }
    Some(match (k.key, k.control) {
        ("pageup", false) => Paging::PageUp,
        ("pagedown", false) => Paging::PageDown,
        ("home", true) => Paging::Top,
        ("end", true) => Paging::Bottom,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain, unmodified key that types a character.
    fn ch(key: &str) -> Key<'_> {
        Key {
            key,
            types_char: true,
            ..Key::default()
        }
    }

    /// A named key with no character and no modifiers.
    fn named(key: &str) -> Key<'_> {
        Key {
            key,
            ..Key::default()
        }
    }

    fn ctrl(key: &str) -> Key<'_> {
        Key {
            key,
            control: true,
            ..Key::default()
        }
    }

    fn ctrl_shift(key: &str) -> Key<'_> {
        Key {
            key,
            control: true,
            shift: true,
            ..Key::default()
        }
    }

    fn alt(key: &str) -> Key<'_> {
        Key {
            key,
            alt: true,
            ..Key::default()
        }
    }

    /// The pane is showing its workbench.
    fn on_bench() -> Up {
        Up {
            bench: true,
            ..Up::default()
        }
    }

    // ── the ladder, as a whole ──────────────────────────────────────────────

    /// Nothing up, nothing claimed: the key is the terminal's.
    #[test]
    fn an_unclaimed_key_is_the_terminals() {
        assert_eq!(route(&ch("a"), &Up::default()), Layer::Terminal);
        assert_eq!(route(&ctrl("c"), &Up::default()), Layer::Terminal);
        assert_eq!(route(&named("escape"), &Up::default()), Layer::Terminal);
        assert_eq!(route(&named("f5"), &Up::default()), Layer::Terminal);
    }

    /// Every surface, alone, with a key it claims.
    #[test]
    fn a_surface_that_is_alone_gets_what_it_claims() {
        let cases: [(fn(&mut Up), Key, Layer); 8] = [
            (|u| u.paint = true, ch("r"), Layer::Paint),
            (|u| u.ctx_menu = true, named("escape"), Layer::CtxMenu),
            (|u| u.header_menu = true, named("escape"), Layer::HeaderMenu),
            (|u| u.reader = true, named("pageup"), Layer::Reader),
            (|u| u.sticky = true, ch("a"), Layer::Sticky),
            (|u| u.rename = true, ch("a"), Layer::Rename),
            (|u| u.float = true, named("escape"), Layer::Float),
            (|u| u.bench = true, ch("a"), Layer::Bench),
        ];
        for (set, key, want) in cases {
            let mut up = Up::default();
            set(&mut up);
            assert_eq!(route(&key, &up), want, "alone: {want:?}");
        }
    }

    /// The ladder is total and monotone over every combination of state.
    ///
    /// All 512 state combinations against a corpus of twelve keys, and the answer
    /// is always the FIRST rung that claims — the whole contract stated as a
    /// property rather than as rows. A variant moved in the enum fails this
    /// without anybody having to remember to add a case for it.
    #[test]
    fn the_topmost_claiming_layer_always_wins() {
        let keys = [
            named("f1"),
            alt("k"),
            alt("w"),
            ch("a"),
            named("escape"),
            ctrl("c"),
            ctrl("w"),
            ctrl_shift("b"),
            alt("s"),
            named("pageup"),
            named("left"),
            named("enter"),
        ];
        for bits in 0u16..512 {
            let up = Up {
                paint: bits & 1 != 0,
                ctx_menu: bits & 2 != 0,
                header_menu: bits & 4 != 0,
                reader: bits & 8 != 0,
                sticky: bits & 16 != 0,
                rename: bits & 32 != 0,
                note: bits & 64 != 0,
                bench: bits & 128 != 0,
                float: bits & 256 != 0,
            };
            for k in &keys {
                let want = LADDER
                    .iter()
                    .find(|(_, claims)| claims(k, &up))
                    .map(|(l, _)| *l)
                    .unwrap_or(Layer::Terminal);
                assert_eq!(route(k, &up), want, "bits={bits:08b} key={k:?}");
            }
        }
    }

    /// The table's order is the enum's order, probed PAIRWISE.
    ///
    /// The obvious version of this raises each layer on its own and checks the
    /// answers come back in ascending order. That version does not work: with one
    /// layer up, `route` returns that layer's variant whatever position its row
    /// occupies, so swapping two adjacent rows passes it. Raising a PAIR is what
    /// distinguishes them — with both claiming, the answer has to be the one
    /// declared earlier. `Layer` derives `Ord` so the assertion can be stated
    /// against the declaration itself rather than against a second list that
    /// could drift in the same way.
    #[test]
    fn of_any_two_layers_the_earlier_declared_one_wins() {
        // Escape is claimed by every one of these, which is what lets them be
        // compared pairwise at all.
        let setters: [fn(&mut Up); 8] = [
            |u| u.paint = true,
            |u| u.ctx_menu = true,
            |u| u.header_menu = true,
            |u| u.reader = true,
            |u| u.sticky = true,
            |u| u.rename = true,
            |u| u.float = true,
            |u| u.bench = true,
        ];
        let k = named("escape");
        for (i, set_i) in setters.iter().enumerate() {
            for set_j in setters.iter().skip(i + 1) {
                let mut both = Up::default();
                set_i(&mut both);
                set_j(&mut both);
                let alone_i = {
                    let mut u = Up::default();
                    set_i(&mut u);
                    route(&k, &u)
                };
                let alone_j = {
                    let mut u = Up::default();
                    set_j(&mut u);
                    route(&k, &u)
                };
                assert!(
                    alone_i < alone_j,
                    "{alone_i:?} is declared before {alone_j:?} but does not order before it"
                );
                assert_eq!(
                    route(&k, &both),
                    alone_i,
                    "with {alone_i:?} and {alone_j:?} both claiming, the earlier one must take it"
                );
            }
        }
    }

    // ── the floating document ───────────────────────────────────────────────

    fn floating() -> Up {
        Up {
            float: true,
            ..Up::default()
        }
    }

    /// Escape closes the square before the shell underneath ever sees it, and
    /// a letter goes past it to the shell: reading beside a prompt means you
    /// can still type at the prompt.
    #[test]
    fn escape_closes_a_floating_document_before_it_reaches_the_terminal() {
        assert_eq!(route(&named("escape"), &floating()), Layer::Float);
        assert_eq!(route(&ch("a"), &floating()), Layer::Terminal);
        assert_eq!(route(&ctrl("c"), &floating()), Layer::Terminal);
        assert_eq!(route(&named("enter"), &floating()), Layer::Terminal);
    }

    /// A caret somewhere else on the pane keeps its own Escape: a note being
    /// written or a name being typed is reverted first, the square second.
    #[test]
    fn a_caret_elsewhere_on_the_pane_keeps_escape_over_a_float() {
        let mut up = floating();
        up.sticky = true;
        assert_eq!(route(&named("escape"), &up), Layer::Sticky);
        let mut up = floating();
        up.rename = true;
        assert_eq!(route(&named("escape"), &up), Layer::Rename);
        let mut up = floating();
        up.ctx_menu = true;
        assert_eq!(route(&named("escape"), &up), Layer::CtxMenu);
    }

    // ── the bugs, as rows ───────────────────────────────────────────────────

    /// SIX KEYS, each of which reached the wrong layer on the workbench face.
    ///
    /// Five did nothing at all; the first typed into somebody's agent. Each is
    /// here as the case it was reported as, not as a general claim, so a reader
    /// can check them against their own memory of the bug.
    #[test]
    fn a_surface_over_the_bench_outranks_the_bench() {
        let bench = on_bench();

        // Right-click the title, type a name: the letters went to the agent.
        assert_eq!(
            route(
                &ch("m"),
                &Up {
                    rename: true,
                    ..bench
                }
            ),
            Layer::Rename,
            "the inline rename box is drawn on the workbench face and takes its own keys"
        );
        // A sticky note could not be started from the face you were reading.
        assert_eq!(
            route(&alt("s"), &bench),
            Layer::PaneChord,
            "alt+s sticks a note on either face"
        );
        // The panels, the bar and the rail were all dead there.
        assert_eq!(
            route(&ctrl_shift("b"), &bench),
            Layer::PaneChord,
            "ctrl+shift+b opens the left bar on either face"
        );
        assert_eq!(
            route(&ctrl("w"), &bench),
            Layer::PaneChord,
            "ctrl+w closes the tab on either face"
        );
        // The window's chords: the complaint that started this.
        assert_eq!(
            route(&alt("w"), &bench),
            Layer::Window,
            "the window's chords survive whichever face is on top"
        );
        // The paint cards, over a pane showing its bench.
        assert_eq!(
            route(
                &named("escape"),
                &Up {
                    paint: true,
                    ..bench
                }
            ),
            Layer::Paint,
            "the overlay is drawn across every pane, so it outranks one pane's face"
        );
    }

    /// What the bench is asked for, it is still asked for first.
    ///
    /// The other half of the fix, and the one a careless version breaks: routing
    /// every key past the bench would leave the rail unwalkable and the composer
    /// unreachable. `ctrl+c` is the row that says where the floor is — the bench
    /// is asked, declines inside its own ladder, and the agent gets it.
    #[test]
    fn the_bench_is_still_asked_before_the_terminal() {
        let bench = on_bench();
        for k in [
            ch("a"),
            named("up"),
            named("tab"),
            named("enter"),
            named("escape"),
            ctrl("c"),
            alt("m"),
            alt("1"),
        ] {
            assert_eq!(route(&k, &bench), Layer::Bench, "{k:?}");
        }
        assert_eq!(
            route(&ctrl("c"), &Up::default()),
            Layer::Terminal,
            "…and on the terminal face the same key goes straight to the floor"
        );
    }

    // ── the claims ──────────────────────────────────────────────────────────

    #[test]
    fn the_windows_chords_are_never_a_panes_to_take() {
        for key in ["left", "right", "up", "down", "r", "v", "h", "w"] {
            assert!(
                window_chord(key, true, false),
                "alt+{key} is the window's: focus, split, close, FOCUS"
            );
        }
        assert!(window_chord("up", true, true));
        assert!(window_chord("q", true, true), "the pair, not the letter");

        assert!(
            !window_chord("c", false, true),
            "ctrl+c must reach a running agent"
        );
        assert!(
            !window_chord("b", true, false),
            "alt+b is readline's word-back, which the composer mirrors"
        );
        assert!(!window_chord("f", true, false), "alt+f is word-forward");
        assert!(
            !window_chord("up", false, false),
            "a plain arrow walks the bench's rail"
        );
        assert!(
            !window_chord("escape", false, false),
            "a plain escape peels the bench's overlays"
        );
        assert!(!window_chord("a", false, false));
        assert!(!window_chord("enter", false, false));
    }

    /// `alt+k` is the window's letter and the pane's gesture, and the pane wins.
    ///
    /// It is in [`window_chord`] so the encoder refuses it — a `k` that reached a
    /// shell as `ESC k` would be a bug — and it is above [`Layer::Window`] here so
    /// the flip still happens. Delete the `Face` rung and this goes silent: the
    /// chord is declined by the pane, bubbles to the workspace, and nothing there
    /// binds it.
    #[test]
    fn the_face_toggle_outranks_the_letter_it_shares() {
        assert_eq!(route(&alt("k"), &Up::default()), Layer::Face);
        assert!(
            window_chord("k", true, false),
            "…and the encoder refuses it"
        );
        assert_eq!(
            route(
                &Key {
                    key: "k",
                    alt: true,
                    control: true,
                    ..Key::default()
                },
                &Up::default()
            ),
            Layer::Window,
            "ctrl+alt+k walks the left bar's tree and is not the face toggle"
        );
    }

    #[test]
    fn the_panes_chords_are_conditional_where_a_better_owner_exists() {
        let nothing = Up::default();
        let stuck = Up {
            note: true,
            ..Up::default()
        };

        assert!(
            pane_chord(&ctrl("x"), &nothing),
            "^X is claimed unconditionally — whether there is anything to cut costs \
             an allocation to ask, so `pane_chord_key` asks it and hands the key back"
        );

        assert!(
            !pane_chord(&alt("backspace"), &nothing),
            "alt+backspace is backward-kill-word with no note to peel"
        );
        assert!(pane_chord(&alt("backspace"), &stuck));

        assert!(pane_chord(&ctrl("w"), &nothing), "^W closes the tab");
        assert!(pane_chord(&ctrl("f"), &nothing));
        assert!(pane_chord(&ctrl_shift("f"), &nothing), "find across panes");
        assert!(pane_chord(&alt("s"), &nothing));

        assert!(!pane_chord(&ctrl("c"), &nothing), "^C is the agent's");
        assert!(!pane_chord(&ctrl("a"), &nothing), "^A is line-start");
        assert!(!pane_chord(&ctrl("d"), &nothing), "^D is EOF");
        assert!(!pane_chord(&ctrl("u"), &nothing), "^U kills the line");
        assert!(!pane_chord(&ch("w"), &nothing), "a bare letter is a letter");
        assert!(
            !pane_chord(&alt("m"), &nothing),
            "alt+m is the bench's note, not the pane's"
        );
        assert!(
            !pane_chord(&alt("1"), &nothing),
            "alt+<digit> lands on a bench shelf"
        );
    }

    /// The overlay is modal for what it uses and transparent for the rest.
    #[test]
    fn the_paint_overlay_claims_plain_keys_and_escape() {
        let up = Up {
            paint: true,
            ..Up::default()
        };
        assert_eq!(route(&ch("r"), &up), Layer::Paint, "a letter paints");
        assert_eq!(
            route(&named("left"), &up),
            Layer::Paint,
            "arrows walk the wall"
        );
        assert_eq!(
            route(&named("escape"), &up),
            Layer::Paint,
            "escape folds it"
        );
        assert_eq!(
            route(&ctrl_shift("b"), &up),
            Layer::PaneChord,
            "a modified chord passes, so the overlay is never a trap"
        );
        assert_eq!(
            route(&named("enter"), &up),
            Layer::Terminal,
            "and so does a named key the overlay has no chord for"
        );
    }

    /// The reader takes two things and lets everything else past.
    ///
    /// This is the row that makes one ladder possible: escape and a letter land on
    /// different layers with the reader up, and a second table for typing would
    /// have had to encode that twice.
    #[test]
    fn the_reader_claims_escape_and_the_paging_keys_only() {
        let up = Up {
            reader: true,
            ..Up::default()
        };
        assert_eq!(route(&named("escape"), &up), Layer::Reader);
        assert_eq!(route(&named("pageup"), &up), Layer::Reader);
        assert_eq!(
            route(
                &Key {
                    key: "home",
                    control: true,
                    ..Key::default()
                },
                &up
            ),
            Layer::Reader
        );
        assert_eq!(
            route(&ch("a"), &up),
            Layer::Terminal,
            "you keep directing the agent while you read it big"
        );
        assert_eq!(
            route(&ch("a"), &Up { rename: true, ..up }),
            Layer::Rename,
            "…and a box with a caret in it still gets the letter"
        );
    }

    #[test]
    fn paging_is_four_gestures_and_nothing_else() {
        assert_eq!(paging(&named("pageup")), Some(Paging::PageUp));
        assert_eq!(paging(&named("pagedown")), Some(Paging::PageDown));
        assert_eq!(
            paging(&Key {
                key: "home",
                control: true,
                ..Key::default()
            }),
            Some(Paging::Top)
        );
        assert_eq!(
            paging(&Key {
                key: "end",
                control: true,
                ..Key::default()
            }),
            Some(Paging::Bottom)
        );

        assert_eq!(
            paging(&named("home")),
            None,
            "plain Home belongs to the shell"
        );
        assert_eq!(
            paging(&ctrl("pageup")),
            None,
            "ctrl+PageUp switches tabs at the workspace"
        );
        assert_eq!(paging(&named("up")), None);
    }

    /// A chord is not a character, so a box with a caret takes every character
    /// and none of the chords.
    ///
    /// The row that matters is `alt+s`. It is how you put the pen down on the
    /// note you are writing, and the pane has already shipped the version where
    /// the composer ate it: `EditBuffer::apply` drops alt-modified keys, so
    /// pressing it again did nothing at all and Enter was the only way out. The
    /// note's own chords have to outrank the note's own composer.
    #[test]
    fn a_box_with_a_caret_takes_the_characters_and_not_the_chords() {
        let writing = Up {
            sticky: true,
            note: true,
            ..Up::default()
        };
        assert_eq!(
            route(&alt("s"), &writing),
            Layer::PaneChord,
            "alt+s posts the note being written — the composer must not swallow the \
             chord for putting the pen down"
        );
        assert_eq!(route(&alt("backspace"), &writing), Layer::PaneChord);
        assert_eq!(
            route(&ch("s"), &writing),
            Layer::Sticky,
            "…while the bare letter is still a letter, and goes on the paper"
        );

        let renaming = Up {
            rename: true,
            ..Up::default()
        };
        assert_eq!(route(&ch("b"), &renaming), Layer::Rename);
        assert_eq!(route(&ctrl_shift("b"), &renaming), Layer::PaneChord);
        assert_eq!(
            route(&alt("w"), &renaming),
            Layer::Window,
            "…and the window's chords are above every one of a pane's surfaces"
        );
        assert_eq!(
            route(&named("f1"), &renaming),
            Layer::Help,
            "…as is the help modal"
        );
    }

    /// The bench sits with the terminal it is a face of, below the pane's chords.
    ///
    /// Typing at a shell does not cost you `ctrl+shift+b`, and the bench's
    /// composer is a mirror of that shell's own line editor — so it does not cost
    /// you one either. The same argument puts `alt+backspace` on the note when a
    /// note is stuck, on both faces or on neither.
    #[test]
    fn the_bench_is_below_the_panes_chords() {
        let bench = on_bench();
        assert_eq!(route(&ctrl_shift("b"), &bench), Layer::PaneChord);
        assert_eq!(
            route(&alt("backspace"), &bench),
            Layer::Bench,
            "kill-word in the composer, with no note stuck"
        );
        assert_eq!(
            route(
                &alt("backspace"),
                &Up {
                    note: true,
                    ..bench
                }
            ),
            Layer::PaneChord,
            "…and the peel, when there is one — the same trade the terminal makes"
        );
    }

    /// F1 is the one key no surface may keep.
    #[test]
    fn the_help_modal_is_reachable_from_under_everything() {
        let everything = Up {
            paint: true,
            ctx_menu: true,
            header_menu: true,
            reader: true,
            sticky: true,
            rename: true,
            note: true,
            bench: true,
            float: true,
        };
        assert_eq!(route(&named("f1"), &everything), Layer::Help);
    }
}
