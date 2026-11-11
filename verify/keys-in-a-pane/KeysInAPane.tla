---------------------------- MODULE KeysInAPane ----------------------------
(***************************************************************************)
(* Who gets a typed letter inside one pane, when more than one surface    *)
(* there can hold a caret: a sticky note being written, and the header's  *)
(* rename box.                                                             *)
(*                                                                         *)
(* Code (each action names the code it stands for):                       *)
(*   app/src/keylayer.rs  LADDER and route: Sticky sits above Rename, and *)
(*                        PaneChord (Alt+S) above both                     *)
(*   app/src/pane.rs      sticky_open, sticky_commit, rename_open (the    *)
(*                        header's double-click and right-click),         *)
(*                        commit_rename, layers_up                        *)
(*                                                                         *)
(* OneCaret = FALSE is the code before 2026-11-11: each surface opened    *)
(* without looking at the other. TRUE is the code after it: opening one   *)
(* puts the other away (a note is posted, a name committed).              *)
(*                                                                         *)
(* Left out: the floating document's and the Document face's note carets *)
(* (float_caret, doc_caret; they sit below both on the ladder), the menus *)
(* (Escape only), and every non-letter key.                               *)
(***************************************************************************)

CONSTANT OneCaret

VARIABLES
    sticky,    \* a sticky note is being written (sticky_composing)
    rename,    \* the header's rename box is open (renaming.is_some())
    last,      \* ghost: the caret the person opened last: "none" | "sticky" | "rename"
    typedTo    \* ghost: where the last letter went: "none" | "sticky" | "rename" | "shell"

vars == <<sticky, rename, last, typedTo>>

Init == sticky = FALSE /\ rename = FALSE /\ last = "none" /\ typedTo = "none"

\* Alt+S: a pane chord, above Rename on the ladder, so it fires while naming.
YouPressAltS ==
    /\ ~sticky
    /\ sticky' = TRUE
    /\ rename' = IF OneCaret THEN FALSE ELSE rename
    /\ last' = "sticky"
    /\ typedTo' = "none"

\* A double-click or right-click on the header's title.
YouOpenTheRenameBox ==
    /\ ~rename
    /\ rename' = TRUE
    /\ sticky' = IF OneCaret THEN FALSE ELSE sticky
    /\ last' = "rename"
    /\ typedTo' = "none"

\* A letter: route() takes the first layer on the ladder that claims it.
YouTypeALetter ==
    /\ typedTo' = CASE sticky -> "sticky"
                   [] rename -> "rename"
                   [] OTHER -> "shell"
    /\ UNCHANGED <<sticky, rename, last>>

\* Enter or Escape on the surface that took it; or a click away.
YouFinishTheNote ==
    /\ sticky
    /\ sticky' = FALSE
    /\ last' = IF rename THEN "rename" ELSE "none"
    /\ typedTo' = "none"
    /\ UNCHANGED rename

YouFinishTheName ==
    /\ rename
    /\ rename' = FALSE
    /\ last' = IF sticky THEN "sticky" ELSE "none"
    /\ typedTo' = "none"
    /\ UNCHANGED sticky

Next ==
    \/ YouPressAltS
    \/ YouOpenTheRenameBox
    \/ YouTypeALetter
    \/ YouFinishTheNote
    \/ YouFinishTheName

(* --- the promises ------------------------------------------------------- *)

\* A pane shows one caret.
OneCaretShown == ~(sticky /\ rename)

\* A letter goes to the caret the person opened last.
TypingGoesToTheCaretOpenedLast == typedTo \in {"sticky", "rename"} => typedTo = last

=============================================================================
