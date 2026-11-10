---------------------------- MODULE OverlayStack ----------------------------
(***************************************************************************)
(* Four surfaces a person can raise over the panes, and the three places  *)
(* the code decides about them, each written separately:                 *)
(*                                                                         *)
(*  draw order   app/src/main.rs, render: the composer lies inside the    *)
(*               screen box (entry_view.rs, render_composer); then the     *)
(*               window's children in order: menu_overlay (the surface     *)
(*               config), …, settings_overlay, …, help_overlay. Later is  *)
(*               drawn on top.                                             *)
(*  keys         the capture-phase chain (lang_picker_key, help_capture_  *)
(*               key, settings_key, close_popups on Esc) runs before the   *)
(*               pane; the composer reads its keys in the bubble phase     *)
(*               (on_key, composer_key). help and settings pass Ctrl/Alt   *)
(*               chords and F1 on, so those reach the pane's chords.       *)
(*  Esc order    close_popups: composer, then settings, then the theme     *)
(*               menu, then help.                                          *)
(*                                                                         *)
(* The person can press a chord, type a letter, or press Esc.             *)
(***************************************************************************)
EXTENDS Naturals, FiniteSets

\* The keys sheet (F1) is left out: it is drawn over all three and hears its
\* keys first, so it keeps every promise here and only multiplies the states.
Surfaces == {"composer", "palette", "book"}

\* Draw order, lowest first (render).
Z == [composer |-> 1, palette |-> 2, book |-> 3]

VARIABLES
    open,       \* the surfaces up
    opened,     \* ghost: the surface the last chord raised, or "none"
    escHit,     \* ghost: the surface the last Esc closed, or "none"
    typedTo     \* ghost: where the last letter went: a surface, or "shell"

vars == <<open, opened, escHit, typedTo>>

Top == IF open = {} THEN "none"
       ELSE CHOOSE s \in open : \A t \in open : Z[t] <= Z[s]

Init == open = {} /\ opened = "none" /\ escHit = "none" /\ typedTo = "none"

\* A chord reaches the pane's chord handler unless a capture-phase surface is
\* up that eats it. help and the book pass Ctrl chords and F1; the composer
\* holds the window's focus, so the pane's chords do not fire.
ChordsReachPane == "composer" \notin open

Raise(s) ==
    /\ s \notin open
    /\ ChordsReachPane
    /\ open' = open \cup {s}
    /\ opened' = s
    /\ escHit' = "none" /\ typedTo' = "none"

PressCtrlComma  == "book" \notin open /\ Raise("book")         \* Ctrl+, : pane::ToggleSettings
PressCtrlShiftE == Raise("composer")                            \* Ctrl+Shift+E : OpenComposer
PressCtrlShiftD == Raise("palette")                             \* Ctrl+Shift+D : the pane's DESIGN menu

\* A letter: the capture chain runs first (help, then the book); then, with
\* the window focused for the composer, its bubble handler; else the pane.
TypeLetter ==
    /\ open' = open
    /\ typedTo' = CASE "book" \in open -> "book"
                   [] "composer" \in open -> "composer"
                   [] OTHER -> "shell"
    /\ opened' = "none" /\ escHit' = "none"

\* Esc: the capture chain (help, then the book's own Esc), then close_popups
\* in its own order: composer, settings, theme menu, help.
PressEsc ==
    /\ open /= {}
    /\ LET hit == CASE "book" \in open -> "book"
                    [] "composer" \in open -> "composer"
                    [] OTHER -> "palette"
       IN /\ open' = open \ {hit}
          /\ escHit' = hit
    /\ opened' = "none" /\ typedTo' = "none"

Next == PressCtrlComma \/ PressCtrlShiftE \/ PressCtrlShiftD \/ TypeLetter \/ PressEsc

Spec == Init /\ [][Next]_vars

(* --- the promises ------------------------------------------------------- *)

\* What a chord raised is what the person sees on top.
RaisedIsOnTop == opened /= "none" => opened = Top

\* A letter goes to the surface on top (or the shell when nothing is up).
TypingGoesToTheTop == typedTo \in Surfaces => typedTo = Top

\* Esc closes the surface on top: nothing drawn above it stays up.
EscClosesTheTop == escHit \in Surfaces => \A s \in open : Z[s] < Z[escHit]
=============================================================================
