--------------------------- MODULE WorktreeCleanup ---------------------------
(***************************************************************************)
(* One candidate worktree under Settings › Worktrees, and an agent that   *)
(* may walk into it at any moment while the person reviews and removes.   *)
(*                                                                         *)
(* Code:                                                                   *)
(*   app/src/wtclean.rs        judge (the guards), scan, process_cwds,     *)
(*                             remove (git worktree remove, no --force)    *)
(*   app/src/settings_view.rs  wt_look, wt_remove_press (arm, then the     *)
(*                             second press re-reads /proc and removes)    *)
(*                                                                         *)
(* git worktree remove is two steps in git itself: it checks the tree is  *)
(* clean, then deletes it. They are not one indivisible step, so the      *)
(* model does not treat them as one.                                       *)
(***************************************************************************)
EXTENDS Naturals

\* FALSE: the code as it stands. TRUE: the proposed fix, where the second
\* press first moves the tree to a quarantine name nobody else knows
\* (git worktree move, a rename, which cannot be half done), so nothing can
\* walk in by path afterwards; a process already inside is still seen by the
\* re-read of /proc, under the new name, and the tree is moved back.
CONSTANT Quarantine

VARIABLES
    ui,         \* "idle" | "listed" | "armed" | "moved" | "rechecked" | "gitChecked" | "gone"
    inside,     \* a process (a shell, an agent) has its cwd inside the worktree
    dirty,      \* the worktree holds modified, staged or untracked files
    harm        \* ghost: "none" | "deletedUnderAProcess" | "deletedWork"

vars == <<ui, inside, dirty, harm>>

Init ==
    /\ ui = "idle"
    /\ inside = FALSE
    /\ dirty = FALSE
    /\ harm = "none"

(* --- anybody else on the machine, at any moment ---------------------------- *)

\* Walking in needs the path. Once the tree is under its quarantine name, the
\* old path no longer leads to it.
Hidden == Quarantine /\ ui \in {"moved", "rechecked", "gitChecked"}
Enter == ui /= "gone" /\ ~Hidden /\ ~inside /\ inside' = TRUE /\ UNCHANGED <<ui, dirty, harm>>
Leave == inside /\ inside' = FALSE /\ UNCHANGED <<ui, dirty, harm>>
\* Only a process standing in the tree writes into it.
Write == ui /= "gone" /\ inside /\ ~dirty /\ dirty' = TRUE /\ UNCHANGED <<ui, inside, harm>>

(* --- the person and the window ------------------------------------------- *)

\* wt_look → wtclean::scan → judge: a candidate only when clean and nobody
\* stands in it (process_cwds read at the look).
Look ==
    /\ ui = "idle"
    /\ ui' = IF ~dirty /\ ~inside THEN "listed" ELSE "idle"
    /\ UNCHANGED <<inside, dirty, harm>>

\* The first press arms the button.
Arm == ui = "listed" /\ ui' = "armed" /\ UNCHANGED <<inside, dirty, harm>>

\* The second press: process_cwds read again; a tree somebody stands in is
\* skipped (back to idle), otherwise git is asked to remove it.
Recheck ==
    /\ ui = IF Quarantine THEN "moved" ELSE "armed"
    /\ ui' = IF inside THEN "idle" ELSE "rechecked"
    /\ UNCHANGED <<inside, dirty, harm>>

\* The fix: git worktree move to the quarantine name, before the re-read.
Move ==
    /\ Quarantine /\ ui = "armed"
    /\ ui' = "moved"
    /\ UNCHANGED <<inside, dirty, harm>>

\* git worktree remove, first half: it refuses a tree with changes.
GitCheck ==
    /\ ui = "rechecked"
    /\ ui' = IF dirty THEN "idle" ELSE "gitChecked"
    /\ UNCHANGED <<inside, dirty, harm>>

\* git worktree remove, second half: the directory is deleted.
GitDelete ==
    /\ ui = "gitChecked"
    /\ ui' = "gone"
    /\ harm' = CASE dirty -> "deletedWork"
                 [] inside -> "deletedUnderAProcess"
                 [] OTHER -> "none"
    /\ UNCHANGED <<inside, dirty>>

Gone == ui = "gone" /\ UNCHANGED vars

Next == Enter \/ Leave \/ Write \/ Look \/ Arm \/ Move \/ Recheck \/ GitCheck \/ GitDelete \/ Gone

Spec == Init /\ [][Next]_vars

(* --- the promises -------------------------------------------------------- *)

\* Removal never deletes work that was not committed.
NeverDeletesWork == harm /= "deletedWork"

\* Removal never pulls a worktree out from under a running process.
NeverUnderAProcess == harm /= "deletedUnderAProcess"
=============================================================================
