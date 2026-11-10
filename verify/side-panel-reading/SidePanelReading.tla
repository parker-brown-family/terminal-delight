-------------------------- MODULE SidePanelReading --------------------------
(***************************************************************************)
(* One side-panel tab's reading: the changed-file list, a file's text, a  *)
(* pull request, its diff, one file's patch, or the + page's suggestions. *)
(* Each is read off the UI thread and lands back on it; the person sees   *)
(* the answer, a red note, "reading…", or the refresh mark turning.       *)
(*                                                                         *)
(* Code (each action names the code it stands for):                       *)
(*   app/src/flyout_view.rs  Reading (due, land), changes_read,           *)
(*                           file_read, pull_read, patch_read,            *)
(*                           suggest_read, flyout_kick, read_file         *)
(*                                                                         *)
(* OneState = FALSE is the code before the change: five fields (got,      *)
(* error, busy, at, task), and file_read's "nothing moved" arm set only   *)
(* busy and at. OneState = TRUE is the code after it: what is shown is    *)
(* one enum (never read, an answer, or a failure that keeps the answer    *)
(* before it), and every landing, "nothing moved" too, goes through it.   *)
(*                                                                         *)
(* Abstracted: time (a read is due or it is not), what an answer holds,   *)
(* several readings at once (each is its own), the + page's cache.        *)
(* Every action is allowed on every kind; the comment on each says which  *)
(* kinds' code can take it, so a path names the tab it happens in.        *)
(***************************************************************************)

CONSTANT OneState

VARIABLES
    got,      \* an answer is held, and drawn
    error,    \* a failure is held, and drawn as a red note
    out,      \* a read is out: busy, and the task that carries it
    forced,   \* the read that is out was forced (opening, the r key, a save)
    source,   \* the thing read, as it is now: "there" | "gone"
              \*   (a file moved away, git or gh failing)
    moved,    \* it changed since the answer held was read
    latest,   \* ghost: what the latest read to land found: "none" | "ok" | "failed"
    had       \* ghost: an answer has been shown since the tab was last cleared

vars == <<got, error, out, forced, source, moved, latest, had>>

Init ==
    /\ got = FALSE
    /\ error = FALSE
    /\ out = FALSE
    /\ forced = FALSE
    /\ source \in {"there", "gone"}
    /\ moved = FALSE
    /\ latest = "none"
    /\ had = FALSE

(* --- the window ---------------------------------------------------------- *)

\* flyout_kick: the poll every second, when the reading is due, or forced by
\* opening, switching, the r key or a save. Both paths refuse while busy
\* (Reading::due; `force && !busy`). busy and the task are set in one call
\* on the UI thread, so they are one step here.
YouOrThePollRead(f) ==
    /\ ~out
    /\ out' = TRUE
    /\ forced' = f
    /\ UNCHANGED <<got, error, source, moved, latest, had>>

\* Every kind: the read found an answer. Reading::land(Ok).
ReadLands ==
    /\ out
    /\ source = "there"
    \* file_read reads the whole file only when forced, when it holds no
    \* text, or when the file's time or size moved (read_file's stamp).
    /\ forced \/ ~got \/ moved
    /\ got' = TRUE
    /\ error' = FALSE
    /\ out' = FALSE
    /\ moved' = FALSE
    /\ latest' = "ok"
    /\ had' = TRUE
    /\ UNCHANGED <<forced, source>>

\* File only: read_file returns Ok(None) when the time and size are the ones
\* last read. Before: the arm set busy = false and at = now, and nothing else.
\* After: it lands as "the answer held is current", which a failure gives up.
NothingMoved ==
    /\ out
    /\ source = "there"
    /\ ~forced /\ got /\ ~moved
    /\ out' = FALSE
    /\ latest' = "ok"
    /\ error' = IF OneState THEN FALSE ELSE error
    /\ UNCHANGED <<got, forced, source, moved, had>>

\* Every kind: the read failed. Reading::land(Err) keeps the answer before it.
ReadFails ==
    /\ out
    /\ source = "gone"
    /\ error' = TRUE
    /\ out' = FALSE
    /\ latest' = "failed"
    /\ UNCHANGED <<got, forced, source, moved, had>>

\* Pull request's diff only: the pull request moved, so pull_read drops its
\* diff (`p.diff = Reading::default()`). Dropping the task cancels the read.
\* The patches of a changed-file list go the same way (changes_landed's
\* retain), and so does every reading when its tab closes (flyout_kick).
TheDiffIsDropped ==
    /\ got' = FALSE
    /\ error' = FALSE
    /\ out' = FALSE
    /\ latest' = "none"
    /\ had' = FALSE
    /\ UNCHANGED <<forced, source, moved>>

(* --- the world ------------------------------------------------------------ *)

ItGoesAway ==
    /\ source = "there"
    /\ source' = "gone"
    /\ UNCHANGED <<got, error, out, forced, moved, latest, had>>

\* Back as it was (a rename keeps the time) or changed.
ItComesBack ==
    /\ source = "gone"
    /\ source' = "there"
    /\ moved' \in BOOLEAN
    /\ UNCHANGED <<got, error, out, forced, latest, had>>

SomeoneEditsIt ==
    /\ source = "there"
    /\ ~moved
    /\ moved' = TRUE
    /\ UNCHANGED <<got, error, out, forced, source, latest, had>>

Next ==
    \/ YouOrThePollRead(FALSE)
    \/ YouOrThePollRead(TRUE)
    \/ ReadLands
    \/ NothingMoved
    \/ ReadFails
    \/ TheDiffIsDropped
    \/ ItGoesAway
    \/ ItComesBack
    \/ SomeoneEditsIt

(* --- the promises --------------------------------------------------------- *)

\* A red note is about the latest read. Once a read has succeeded, the note
\* about the one before it goes.
ErrorIsTheLatestRead == error => latest = "failed"

\* A failure is shown: when the latest read failed, the red note is there,
\* even beside an answer a person was reading.
TroubleIsShown == latest = "failed" => error

\* A moment's trouble never blanks what a person was reading: an answer,
\* once shown, stays until the tab is cleared on purpose.
NeverBlankedByTrouble == had => got

=============================================================================
