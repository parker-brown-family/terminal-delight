//! Who owns a keystroke on a pane, when several surfaces are up at once.
//!
//! # Why this is a module and not a run of `if` statements
//!
//! A pane stacks surfaces: the paint overlay is drawn across every pane in the
//! window, the header carries a ⋯ menu and an inline rename box, a right-click
//! raises a copy tray, the FOCUS modal mirrors the pane, a sticky note can hold
//! the pen, and underneath all of it the pane is showing either its terminal or
//! its workbench. Escape means *give me less* to every one of them.
//!
//! That precedence used to exist only as the order of branches in
//! `TerminalView::on_key`, and an order of branches has no runtime signal at
//! all: swapping two correct blocks compiles, and every behavioural test in the
//! suite stays green. It went wrong exactly that way. `bench_key` consumes every
//! escape on the workbench face and ends in `stop_propagation`, and it sat above
//! the paint block — so with the paint cards up over a pane showing its bench,
//! one press flipped the face and left the overlay standing (PR #532 — Escape
//! stops at the bench). Reordering
//! those two blocks fixed that one case and left three more of the same shape:
//! the right-click tray, the header's ⋯ menu and the rename box are all drawn on
//! the workbench face and all sat *below* `bench_key`, so escape could not reach
//! any of them either.
//!
//! The guard written for the first case read `on_key`'s source text and asserted
//! that one call appeared before another. That is a test of the file, not of the
//! program: it cannot say whether the order is *right*, only that it has not
//! changed, and it breaks on a comment that mentions the wrong word. So the
//! ordering moved here and became a value.
//!
//! [`EscTarget`]'s declaration order **is** the precedence, topmost first, and
//! [`escape_target`] is the only thing that reads it. One function decides, the
//! way every control in gpui-kit routes through one `resolve_style` so the
//! layering cannot drift apart between controls again. A new surface is a new
//! variant, which the compiler makes every caller handle, and a new row in a
//! table somebody can argue with.
//!
//! # Why this module imports nothing
//!
//! Zero `use` statements, like `tree.rs`. It compiles standalone, so
//! `rustc --test app/src/keylayer.rs` runs this file's tests in about a second
//! against `cargo test`'s several minutes — which is what makes mutation-testing
//! the table affordable enough that it actually gets done.

/// What escape is for, on a pane, right now.
///
/// **Declaration order is precedence, topmost surface first.** Moving a variant
/// in this list changes behaviour; that is the point, and it is why the list is
/// short enough to read in one go.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum EscTarget {
    /// The paint overlay, drawn across every pane in the window at once. It
    /// outranks anything one pane owns because it is not one pane's surface.
    Paint,
    /// The right-click copy tray, floating at the cursor.
    CtxMenu,
    /// The ⋯ overflow menu in the header. The header is drawn on BOTH faces,
    /// which is why this has to outrank the bench.
    HeaderMenu,
    /// This pane mirrored large in the FOCUS modal.
    Reader,
    /// A sticky note holding the pen.
    Sticky,
    /// The inline rename box, also in the header, also on both faces.
    Rename,
    /// The workbench face's own ladder — see [`crate::workbench::peel`], which
    /// decides what escape takes off *within* the bench and refuses to take the
    /// bench itself.
    Bench,
    /// Nothing above it wants the key, so it goes to the pseudoterminal as
    /// `0x1b`. On a working agent that ends its turn, which is why every layer
    /// above gets asked first.
    Terminal,
}

/// Which of a pane's surfaces are currently up.
///
/// A struct of named flags rather than a tuple, so that a call site cannot
/// silently swap two of them — all seven are the same type, and the compiler
/// has nothing to say about `escape_target(a, b, c, d, e, f, g)`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Up {
    pub paint: bool,
    pub ctx_menu: bool,
    pub header_menu: bool,
    pub reader: bool,
    pub sticky: bool,
    pub rename: bool,
    /// The pane is showing its WORKBENCH face. Not "the bench wants the key" —
    /// what the bench then does with it is [`crate::workbench::peel`]'s call,
    /// and it may well be nothing.
    pub bench: bool,
}

/// The topmost surface that is up, or [`EscTarget::Terminal`] if none is.
///
/// Reads the flags in [`EscTarget`]'s declaration order. Written as a list of
/// pairs rather than a chain of early returns so that the order is one readable
/// column and adding a surface cannot accidentally be written in the middle of
/// an unrelated branch.
pub fn escape_target(up: Up) -> EscTarget {
    /// One rung: the surface, and how to ask whether it is up.
    type Rung = (EscTarget, fn(&Up) -> bool);
    const LADDER: [Rung; 7] = [
        (EscTarget::Paint, |u| u.paint),
        (EscTarget::CtxMenu, |u| u.ctx_menu),
        (EscTarget::HeaderMenu, |u| u.header_menu),
        (EscTarget::Reader, |u| u.reader),
        (EscTarget::Sticky, |u| u.sticky),
        (EscTarget::Rename, |u| u.rename),
        (EscTarget::Bench, |u| u.bench),
    ];
    for (target, is_up) in LADDER {
        if is_up(&up) {
            return target;
        }
    }
    EscTarget::Terminal
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every flag, one at a time: a surface that is the only one up gets the key.
    #[test]
    fn a_surface_that_is_alone_gets_the_key() {
        let cases: [(fn(&mut Up), EscTarget); 7] = [
            (|u| u.paint = true, EscTarget::Paint),
            (|u| u.ctx_menu = true, EscTarget::CtxMenu),
            (|u| u.header_menu = true, EscTarget::HeaderMenu),
            (|u| u.reader = true, EscTarget::Reader),
            (|u| u.sticky = true, EscTarget::Sticky),
            (|u| u.rename = true, EscTarget::Rename),
            (|u| u.bench = true, EscTarget::Bench),
        ];
        for (set, want) in cases {
            let mut up = Up::default();
            set(&mut up);
            assert_eq!(escape_target(up), want, "alone: {want:?}");
        }
    }

    #[test]
    fn nothing_up_is_the_terminal() {
        assert_eq!(escape_target(Up::default()), EscTarget::Terminal);
    }

    /// THE FOUR BUGS, as rows.
    ///
    /// Each of these is a surface that was drawn on the workbench face and could
    /// not be closed with escape, because `bench_key` ran first and swallowed it.
    /// All four answered "the bench" before this module existed.
    #[test]
    fn a_surface_over_the_bench_outranks_the_bench() {
        let on_bench = Up {
            bench: true,
            ..Up::default()
        };
        assert_eq!(
            escape_target(Up {
                paint: true,
                ..on_bench
            }),
            EscTarget::Paint,
            "the paint cards are drawn over every pane in the window (PR #532)"
        );
        assert_eq!(
            escape_target(Up {
                ctx_menu: true,
                ..on_bench
            }),
            EscTarget::CtxMenu,
            "a right-click on the bench raises the copy tray, and escape must close it"
        );
        assert_eq!(
            escape_target(Up {
                header_menu: true,
                ..on_bench
            }),
            EscTarget::HeaderMenu,
            "the header is drawn on both faces, so its ⋯ menu is reachable from the bench"
        );
        assert_eq!(
            escape_target(Up {
                rename: true,
                ..on_bench
            }),
            EscTarget::Rename,
            "so is the inline rename box"
        );
    }

    /// The ladder is total, and it is monotone.
    ///
    /// Every one of the 128 combinations resolves, and the answer is always the
    /// FIRST flag set in declaration order — which is the whole contract stated
    /// as a property rather than as rows. A variant moved in the enum fails this
    /// test without anybody having to remember to add a case for it.
    #[test]
    fn the_topmost_surface_that_is_up_always_wins() {
        for bits in 0u8..128 {
            let up = Up {
                paint: bits & 1 != 0,
                ctx_menu: bits & 2 != 0,
                header_menu: bits & 4 != 0,
                reader: bits & 8 != 0,
                sticky: bits & 16 != 0,
                rename: bits & 32 != 0,
                bench: bits & 64 != 0,
            };
            let flags = [
                (EscTarget::Paint, up.paint),
                (EscTarget::CtxMenu, up.ctx_menu),
                (EscTarget::HeaderMenu, up.header_menu),
                (EscTarget::Reader, up.reader),
                (EscTarget::Sticky, up.sticky),
                (EscTarget::Rename, up.rename),
                (EscTarget::Bench, up.bench),
            ];
            let want = flags
                .iter()
                .find(|(_, on)| *on)
                .map(|(t, _)| *t)
                .unwrap_or(EscTarget::Terminal);
            assert_eq!(escape_target(up), want, "bits={bits:07b} up={up:?}");
        }
    }

    /// The table's order is the enum's order, probed PAIRWISE.
    ///
    /// The obvious version of this test raises each flag on its own and checks
    /// the answers come back in ascending order. That version was written first
    /// and it does not work: with one flag up, `escape_target` returns that
    /// flag's variant whatever position its row occupies, so swapping two
    /// adjacent rows in `LADDER` passes it. Caught by mutating the table and
    /// watching this test stay green while a different one went red — the claim
    /// in its doc comment was simply wrong.
    ///
    /// Raising a PAIR is what distinguishes them: with both up, the answer has
    /// to be the one declared earlier. `EscTarget` derives `Ord` so the
    /// assertion can be stated against the declaration itself rather than
    /// against a second list that could drift in the same way.
    #[test]
    fn of_any_two_surfaces_the_earlier_declared_one_wins() {
        let setters: [fn(&mut Up); 7] = [
            |u| u.paint = true,
            |u| u.ctx_menu = true,
            |u| u.header_menu = true,
            |u| u.reader = true,
            |u| u.sticky = true,
            |u| u.rename = true,
            |u| u.bench = true,
        ];
        for (i, set_i) in setters.iter().enumerate() {
            for set_j in setters.iter().skip(i + 1) {
                let mut up = Up::default();
                set_i(&mut up);
                set_j(&mut up);
                let alone_i = {
                    let mut u = Up::default();
                    set_i(&mut u);
                    escape_target(u)
                };
                let alone_j = {
                    let mut u = Up::default();
                    set_j(&mut u);
                    escape_target(u)
                };
                assert!(
                    alone_i < alone_j,
                    "{alone_i:?} is declared before {alone_j:?} but does not order before it"
                );
                assert_eq!(
                    escape_target(up),
                    alone_i,
                    "with {alone_i:?} and {alone_j:?} both up, the earlier one must take the key"
                );
            }
        }
    }
}
