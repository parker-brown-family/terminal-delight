------------------------------ MODULE Composer ------------------------------
(***************************************************************************)
(* The composer's faces: the sentence, a chip's list (PROJECT, or one of   *)
(* HARNESS, MODEL, EFFORT, REACH, MACHINE), and the add-a-project dialog   *)
(* over it; and what a chip's list shows for what is typed on it.          *)
(*                                                                         *)
(* Code (each action names the code it stands for):                       *)
(*   app/src/entry_view.rs      open_composer, composer_key (escape, enter, *)
(*                              tab, typing, backspace), composer_escape,  *)
(*                              the chip, draft and row presses,           *)
(*                              Composer::refilter, project_rows           *)
(*   app/src/addproject_view.rs add_project_open, add_project_escape       *)
(*                              (Back), the folder taken (its done path)   *)
(*                                                                         *)
(* Derived = FALSE is the code before 2026-11-10: PROJECT's rows were a    *)
(* stored list (`order`), recomputed only by typing, Backspace and a new   *)
(* project. Derived = TRUE is the code after it: the rows are worked out   *)
(* from what is typed whenever they are read, so there is no copy to go    *)
(* stale.                                                                  *)
(*                                                                         *)
(* Abstracted: which project rows match (all, or narrowed), the other      *)
(* chips' lists (they keep no copy: Composer::choices is worked out each   *)
(* time), the sentence's text, pictures, the send, far machines.           *)
(***************************************************************************)

CONSTANT Derived

VARIABLES
    up,       \* the composer is up
    face,     \* "sentence" | "project" | "other" (any other chip's list) | "adding"
    under,    \* the face under the dialog while it is up
    query,    \* what is typed on the chip: "" | "typed"
    order     \* PROJECT's rows as stored: "all" | "narrowed"

vars == <<up, face, under, query, order>>

\* What PROJECT's list draws: the stored rows, or (Derived) the rows for what
\* is typed, worked out as it is drawn.
Rows == IF Derived THEN (IF query = "" THEN "all" ELSE "narrowed") ELSE order

Chips == {"project", "other"}

Init ==
    /\ up = FALSE
    /\ face = "sentence"
    /\ under = "sentence"
    /\ query = ""
    /\ order = "all"

\* open_composer: on the sentence, the list whole (launcher::filter(_, "")).
YouOpenIt ==
    /\ ~up
    /\ up' = TRUE
    /\ face' = "sentence"
    /\ query' = ""
    /\ order' = "all"
    /\ UNCHANGED under

\* Tab or Shift+Tab walks the ring; a press on a chip opens its list or, on
\* the open one, puts it away; Enter or Esc on a chip, or a press on one of
\* its rows, goes back to the sentence. Every one clears what was typed, and
\* none recomputes the stored rows.
YouMoveBetweenFaces ==
    /\ up /\ face \in {"sentence"} \cup Chips
    /\ face' \in ({"sentence"} \cup Chips) \ {face}
    /\ query' = ""
    /\ UNCHANGED <<up, under, order>>

\* Typing on a chip narrows its list (Composer::refilter): PROJECT's stored
\* rows are recomputed.
YouTypeOnAChip ==
    /\ up /\ face \in Chips
    /\ query' = "typed"
    /\ order' = IF face = "project" THEN "narrowed" ELSE order
    /\ UNCHANGED <<up, face, under>>

\* Backspace to nothing typed: refilter again, so PROJECT's rows are whole.
YouBackspaceItAway ==
    /\ up /\ face \in Chips /\ query = "typed"
    /\ query' = ""
    /\ order' = IF face = "project" THEN "all" ELSE order
    /\ UNCHANGED <<up, face, under>>

\* A press on the sentence goes back to it, leaving what was typed.
YouPressTheSentence ==
    /\ up /\ face \in Chips
    /\ face' = "sentence"
    /\ UNCHANGED <<up, under, query, order>>

\* Ctrl+O: the dialog comes up over the face you were on.
YouAddAProject ==
    /\ up /\ face \in {"sentence"} \cup Chips
    /\ face' = "adding"
    /\ under' = face
    /\ UNCHANGED <<up, query, order>>

\* Back from the dialog's first step: the face under it, as it was.
YouGoBackFromTheDialog ==
    /\ up /\ face = "adding"
    /\ face' = under
    /\ UNCHANGED <<up, under, query, order>>

\* A folder is taken: it becomes the project, the list is whole again, and
\* the sentence has the keys.
TheFolderIsTaken ==
    /\ up /\ face = "adding"
    /\ face' = "sentence"
    /\ query' = ""
    /\ order' = "all"
    /\ UNCHANGED <<up, under>>

\* Esc on the sentence, Ctrl+Shift+E, or a row under LOOK AT ME: put away.
YouPutItAway ==
    /\ up /\ face # "adding"
    /\ up' = FALSE
    /\ UNCHANGED <<face, under, query, order>>

Next ==
    \/ YouOpenIt
    \/ YouMoveBetweenFaces
    \/ YouTypeOnAChip
    \/ YouBackspaceItAway
    \/ YouPressTheSentence
    \/ YouAddAProject
    \/ YouGoBackFromTheDialog
    \/ TheFolderIsTaken
    \/ YouPutItAway

(* --- the promises --------------------------------------------------------- *)

\* PROJECT's list shows what its field says: every project when nothing is
\* typed, only the matches when something is.
ListMatchesField ==
    (up /\ face = "project") => (Rows = IF query = "" THEN "all" ELSE "narrowed")

\* Keys go to what is drawn: while the dialog is up, it is the only face.
TheDialogIsTheFace == (up /\ face = "adding") => under # "adding"

=============================================================================
