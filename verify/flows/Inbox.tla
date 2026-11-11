-------------------------------- MODULE Inbox --------------------------------
(***************************************************************************)
(* One agent's conversation as the INBOX shelves it, and the person's     *)
(* word (snooze, done) that the shelves honour until something new        *)
(* overrides it.                                                           *)
(*                                                                         *)
(* Code: app/src/inbox.rs, shelve (the order: done, snoozed, a lane, woke, *)
(* working, quiet) and Store::advance (a snooze ends on its hour or early  *)
(* for something new and urgent; done lifts for a new lane or new work).  *)
(* The lanes are the attention rail's: a question is urgent, a finish is  *)
(* not. Visiting the pane clears a finish and a woke mark.                *)
(***************************************************************************)
VARIABLES agent, mark, woke, fresh

vars == <<agent, mark, woke, fresh>>
\* agent: "working" | "asks" | "finished" | "idle"
\* mark:  "none" | "snoozed" | "done"
\* woke:  a snooze ended and the person has not looked since
\* fresh: ghost, the agent asked something after the person's mark

Shelf ==
    CASE mark = "done" -> "Done"
      [] mark = "snoozed" -> "Snoozed"
      [] agent \in {"asks", "finished"} -> "LookAtMe"
      [] woke -> "LookAtMe"
      [] agent = "working" -> "Working"
      [] OTHER -> "Quiet"

Init == agent = "working" /\ mark = "none" /\ woke = FALSE /\ fresh = FALSE

\* Store::advance: something new and urgent ends a snooze early and lifts a
\* done; a new finish lifts a done but not a snooze.
AgentAsks ==
    /\ agent = "working"
    /\ agent' = "asks"
    /\ mark' = "none"
    /\ woke' = ((mark = "snoozed") \/ woke)
    /\ fresh' = (mark /= "none")
AgentFinishes ==
    /\ agent = "working"
    /\ agent' = "finished"
    /\ mark' = IF mark = "done" THEN "none" ELSE mark
    /\ UNCHANGED <<woke, fresh>>

YouAnswer == agent = "asks" /\ agent' = "working" /\ woke' = FALSE /\ fresh' = FALSE /\ UNCHANGED mark
YouLook == agent = "finished" /\ agent' = "idle" /\ woke' = FALSE /\ UNCHANGED <<mark, fresh>>
YouSendWork ==
    /\ agent \in {"idle", "finished"}
    /\ agent' = "working"
    /\ mark' = "none"
    /\ woke' = FALSE
    /\ UNCHANGED fresh
YouSnooze == Shelf = "LookAtMe" /\ mark' = "snoozed" /\ woke' = FALSE /\ fresh' = FALSE /\ UNCHANGED agent
SnoozeEnds == mark = "snoozed" /\ mark' = "none" /\ woke' = TRUE /\ UNCHANGED <<agent, fresh>>
YouMarkDone == mark /= "done" /\ mark' = "done" /\ woke' = FALSE /\ fresh' = FALSE /\ UNCHANGED agent
YouReopen == mark = "done" /\ mark' = "none" /\ UNCHANGED <<agent, woke, fresh>>

Next ==
    \/ AgentAsks \/ AgentFinishes
    \/ YouAnswer \/ YouLook \/ YouSendWork
    \/ YouSnooze \/ SnoozeEnds \/ YouMarkDone \/ YouReopen

Spec == Init /\ [][Next]_vars

\* A question asked after the person put the row away is never put away.
ANewQuestionIsSeen == fresh => Shelf = "LookAtMe"

\* An agent with a question is never on the quiet shelf.
AQuestionIsNeverQuiet == agent = "asks" => Shelf /= "Quiet"
=============================================================================
