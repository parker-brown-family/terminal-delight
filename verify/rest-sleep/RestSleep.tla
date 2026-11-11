------------------------------ MODULE RestSleep ------------------------------
(***************************************************************************)
(* Rest and sleep for one agent pane: the policy's look, the act that     *)
(* follows it, and everything the agent and the person can do between.    *)
(*                                                                         *)
(* Code (each action names the code it stands for):                       *)
(*   app/src/agentrest.rs     may_rest, may_sleep, Dormancy, Stopping      *)
(*   app/src/agentrest_io.rs  the per-pane look (Stage::Awake / Resting)   *)
(*   app/src/panegroup.rs     Gate::freeze_if_quiet, Gate writes, release  *)
(*                                                                         *)
(* Abstracted: time (thresholds are "long enough" or not, as a step),     *)
(* memory, Codex (this is the Claude path), SleepNow, the stop deadline.  *)
(*                                                                         *)
(* Fixed = FALSE is the code before 2026-11-10. Fixed = TRUE is the code   *)
(* after it: the look reads the end of the conversation and refuses a      *)
(* pending ScheduleWakeup (agentrest::wakeup_pending, Refusal::            *)
(* WakeupPending), and a freeze stands only if the agent says, frozen,     *)
(* what it said at the look (agentrest::said_the_same; otherwise           *)
(* Refusal::MovedSinceTheLook and the gate releases it).                    *)
(***************************************************************************)
EXTENDS Naturals

CONSTANT Fixed

VARIABLES
    agent,      \* "prompt": at its prompt, waiting for a person
                \* "working": mid-turn, printing
                \* "waiting": a /loop ScheduleWakeup is pending; it reports idle
    stage,      \* "awake" | "looked" | "frozen" | "resting" | "asleep"
                \*   "looked": the policy has looked and may_rest said yes; the
                \*   freeze has not happened yet (agentrest_io.rs, Stage::Awake arm)
                \*   "frozen": Fixed only: frozen, and its word not yet read again
    seen,       \* what the agent was at the look (its word, its conversation)
    touched,    \* the person wrote to the pane since the look (Gate writes != epoch)
    saved,      \* Claude has written the conversation down (conversation_saved)
    lost        \* ghost: an agent was ended while work was still pending

vars == <<agent, stage, seen, touched, saved, lost>>

Init ==
    /\ agent = "prompt"
    /\ stage = "awake"
    /\ seen = "prompt"
    /\ touched = FALSE
    /\ saved \in BOOLEAN
    /\ lost = FALSE

Running == stage \in {"awake", "looked"}   \* the process can run: not frozen, not ended

(* --- the person --------------------------------------------------------- *)

\* A key in the pane. Awake: it goes to the agent, which starts a turn.
\* Resting: the gate's write thaws the group (panegroup.rs, Gate). Asleep: the
\* wake script resumes the conversation, which is back at its prompt.
PersonTypes ==
    /\ \/ /\ Running
          /\ agent' = "working"
          /\ stage' = stage
       \/ /\ stage \in {"resting", "frozen"}
          /\ stage' = "awake"
          /\ agent' = agent
       \/ /\ stage = "asleep"
          /\ stage' = "awake"
          /\ agent' = "prompt"
    /\ touched' = TRUE
    /\ UNCHANGED <<seen, saved, lost>>

(* --- the agent, only while its process can run -------------------------- *)

AgentFinishes ==
    /\ Running /\ agent = "working"
    /\ agent' = "prompt"
    /\ saved' = TRUE
    /\ UNCHANGED <<stage, seen, touched, lost>>

\* A /loop turn ends by scheduling its own next turn.
AgentSchedulesWakeup ==
    /\ Running /\ agent = "working"
    /\ agent' = "waiting"
    /\ saved' = TRUE
    /\ UNCHANGED <<stage, seen, touched, lost>>

\* The timer fires. A frozen or ended process cannot.
WakeupFires ==
    /\ Running /\ agent = "waiting"
    /\ agent' = "working"
    /\ UNCHANGED <<stage, seen, touched, saved, lost>>

(* --- the policy ---------------------------------------------------------- *)

\* may_rest: Claude's own word says idle and the screen is quiet. Both "prompt"
\* and "waiting" read that way (issue 1176 measured "waiting" as idle, 119 of
\* 119 samples). The look records the gate's write count as its epoch. Fixed:
\* the end of the conversation shows a ScheduleWakeup still to come, and
\* "waiting" refuses (Refusal::WakeupPending).
Look ==
    /\ stage = "awake"
    /\ agent \in IF Fixed THEN {"prompt"} ELSE {"prompt", "waiting"}
    /\ stage' = "looked"
    /\ seen' = agent
    /\ touched' = FALSE
    /\ UNCHANGED <<agent, saved, lost>>

\* Gate::freeze_if_quiet(group, epoch): refuses when the person wrote since
\* the epoch, and otherwise freezes, whatever the agent did meanwhile. Fixed:
\* the freeze is not yet a rest; its word is read again first.
Freeze ==
    /\ stage = "looked"
    /\ stage' = IF touched THEN "awake" ELSE IF Fixed THEN "frozen" ELSE "resting"
    /\ UNCHANGED <<agent, seen, touched, saved, lost>>

\* Fixed only: frozen, it can write nothing more, so its word now is final.
\* agentrest::said_the_same: the same as at the look, it rests; otherwise the
\* gate releases it (Refusal::MovedSinceTheLook).
Verify ==
    /\ stage = "frozen"
    /\ stage' = IF agent = seen THEN "resting" ELSE "awake"
    /\ UNCHANGED <<agent, seen, touched, saved, lost>>

\* The Stage::Resting arm: may_sleep re-runs may_rest on a fresh look (the
\* agent's word as it was when frozen), then conversation_saved, then sleep().
Sleep ==
    /\ stage = "resting"
    /\ agent \in IF Fixed THEN {"prompt"} ELSE {"prompt", "waiting"}
    /\ saved
    /\ stage' = "asleep"
    /\ lost' = (agent = "waiting")
    /\ UNCHANGED <<agent, seen, touched, saved>>

Next ==
    \/ PersonTypes
    \/ AgentFinishes
    \/ AgentSchedulesWakeup
    \/ WakeupFires
    \/ Look
    \/ Freeze
    \/ Verify
    \/ Sleep

Spec == Init /\ [][Next]_vars

(* --- the promises -------------------------------------------------------- *)

\* Rest never freezes an agent in the middle of a turn.
NeverFrozenMidTurn == stage = "resting" => agent /= "working"

\* Sleep never ends an agent with work still pending (a scheduled wakeup).
SleepLosesNothing == ~lost

\* A sleeper always has a conversation to wake into.
SleepKeepsTheConversation == stage = "asleep" => saved

\* Rest never freezes a pane the person wrote to since the look.
NoFreezeOverATouch == stage = "resting" => ~touched
=============================================================================
