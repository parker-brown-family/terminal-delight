------------------------------- MODULE Drags -------------------------------
(***************************************************************************)
(* One pointer and the window's drags: the left bar's edge, an INBOX row,  *)
(* a tab, a group, a pane, a split, the gauges' wheels, lights, tracks and *)
(* sliders, FOCUS's zoom and selection, a card's scale, the Agents page.   *)
(* Each is its own field on the Workspace (seventeen of them).             *)
(*                                                                         *)
(* Code (each action names the code it stands for):                       *)
(*   app/src/main.rs  each drag's on_mouse_down (sets its own field, and   *)
(*                    looks at no other), Workspace::on_mouse_move (with   *)
(*                    the button up: clears the fields it knows),          *)
(*                    Workspace::on_mouse_up (a chain: the first field     *)
(*                    that is set is taken, handled, and it returns)       *)
(*                                                                         *)
(* OneDrag = FALSE is the code as it stands. OneDrag = TRUE is the         *)
(* proposal: one `drag: Option<Drag>` that a press replaces and a release  *)
(* takes, so two at once cannot be written.                                *)
(*                                                                         *)
(* Two kinds stand for all seventeen: Early is a drag the mouse-up chain   *)
(* reaches first, Late one it reaches after. A release the window never    *)
(* receives (the compositor took the pointer mid-drag) is the one event    *)
(* this depends on; with every release delivered, the rules hold.          *)
(***************************************************************************)
EXTENDS FiniteSets

CONSTANT OneDrag

Kinds == {"Early", "Late"}

VARIABLES
    button,   \* "up" | "down"
    active,   \* the drags whose fields are set
    started   \* ghost: the drag the last press started, or "none"

vars == <<button, active, started>>

Init == button = "up" /\ active = {} /\ started = "none"

\* A press on something draggable starts its drag (its on_mouse_down).
YouPressOn(k) ==
    /\ button = "up"
    /\ button' = "down"
    /\ active' = IF OneDrag THEN {k} ELSE active \cup {k}
    /\ started' = k

\* The release reaches the window: on_mouse_up takes the first drag in its
\* chain and returns.
YouRelease ==
    /\ button = "down"
    /\ button' = "up"
    /\ active' = IF OneDrag \/ "Early" \notin active THEN {} ELSE active \ {"Early"}
    /\ started' = "none"

\* The release never reaches the window: the compositor took the pointer.
TheReleaseIsLost ==
    /\ button = "down"
    /\ button' = "up"
    /\ UNCHANGED <<active, started>>

\* The pointer moves with no button held: on_mouse_move clears the drags.
YouMoveWithNoButton ==
    /\ button = "up"
    /\ active # {}
    /\ active' = {}
    /\ UNCHANGED <<button, started>>

Next ==
    \/ \E k \in Kinds : YouPressOn(k)
    \/ YouRelease
    \/ TheReleaseIsLost
    \/ YouMoveWithNoButton

(* --- the promises ------------------------------------------------------- *)

\* A press drags what it pressed on, and nothing else comes along.
APressDragsOnlyWhatItPressed == started # "none" => active \subseteq {started}

\* A release that reaches the window ends every drag.
AReleaseEndsTheDrag == (button = "up" /\ started = "none") => active = {}

=============================================================================
