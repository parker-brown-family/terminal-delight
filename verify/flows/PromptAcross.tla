---------------------------- MODULE PromptAcross ----------------------------
(* One agent prompting another along a line a person drew at `prompt` in   *)
(* the Threshing Floor: prompt_agent, then the window's tick types it, and  *)
(* prompt_status reads the stage. Two prompts to one worker, in send order.*)
(* Code: app/src/prompt.rs (PromptStage, the three rules in its module doc),*)
(* app/src/mcp.rs prompt_agent / prompt_status, Workspace::prompt_tick.    *)
(* Abstracted: the text, the reply, the waking of a sleeping worker, expiry.*)
EXTENDS Naturals
CONSTANTS P
VARIABLES line, worker, quiet, stage, shown, enters, bad

vars == <<line, worker, quiet, stage, shown, enters, bad>>
Open == {"waiting", "typing", "delivered", "working"}
Typed == {"typing", "delivered", "working"}

Init == /\ line \in BOOLEAN /\ worker = "rest" /\ quiet = TRUE
        /\ stage = [p \in P |-> "none"] /\ shown = [p \in P |-> FALSE]
        /\ enters = [p \in P |-> 0] /\ bad = "none"

\* Prompts are numbered in the order they were sent.
Send(p) == /\ stage[p] = "none" /\ \A q \in P : q < p => stage[q] # "none"
           /\ stage' = [stage EXCEPT ![p] = IF line THEN "waiting" ELSE "refused"]
           /\ UNCHANGED <<line, worker, quiet, shown, enters, bad>>
\* Typed only at a recognised rest, nothing a person typed lately, and only
\* after every earlier prompt to this worker has ended.
TypeIt(p) == /\ stage[p] = "waiting" /\ line /\ worker = "rest" /\ quiet
             /\ \A q \in P : q < p => stage[q] \notin Open
             /\ stage' = [stage EXCEPT ![p] = "typing"]
             /\ bad' = IF worker # "rest" THEN "typed-not-at-rest" ELSE bad
             /\ UNCHANGED <<line, worker, quiet, shown, enters>>
LineErased(p) == /\ stage[p] = "waiting" /\ ~line
                 /\ stage' = [stage EXCEPT ![p] = "failed"]
                 /\ UNCHANGED <<line, worker, quiet, shown, enters, bad>>
PasteShows(p) == /\ stage[p] = "typing" /\ ~shown[p]
                 /\ shown' = [shown EXCEPT ![p] = TRUE]
                 /\ UNCHANGED <<line, worker, quiet, stage, enters, bad>>
\* Enter only on proof the paste landed, and once.
PressEnter(p) == /\ stage[p] = "typing" /\ shown[p]
                 /\ stage' = [stage EXCEPT ![p] = "delivered"]
                 /\ enters' = [enters EXCEPT ![p] = @ + 1]
                 /\ UNCHANGED <<line, worker, quiet, shown, bad>>
NoProof(p) == /\ stage[p] = "typing" /\ ~shown[p]
              /\ stage' = [stage EXCEPT ![p] = "failed"]
              /\ UNCHANGED <<line, worker, quiet, shown, enters, bad>>
WorkerTakesIt(p) == /\ stage[p] = "delivered" /\ worker' = "busy"
                    /\ stage' = [stage EXCEPT ![p] = "working"]
                    /\ UNCHANGED <<line, quiet, shown, enters, bad>>
WorkerRests(p) == /\ stage[p] = "working" /\ worker' = "rest"
                  /\ stage' = [stage EXCEPT ![p] = "finished"]
                  /\ UNCHANGED <<line, quiet, shown, enters, bad>>
\* The world: a person types, time passes, a dialog appears and goes, the
\* line is drawn or erased. Only while nothing is typed into the worker.
World == /\ \A p \in P : stage[p] \notin Typed
         /\ \/ quiet' \in BOOLEAN /\ UNCHANGED <<line, worker>>
            \/ worker' \in {"rest", "busy", "asking"} /\ UNCHANGED <<line, quiet>>
            \/ line' \in BOOLEAN /\ UNCHANGED <<worker, quiet>>
         /\ UNCHANGED <<stage, shown, enters, bad>>

Next == World \/ \E p \in P : Send(p) \/ TypeIt(p) \/ LineErased(p) \/ PasteShows(p)
        \/ PressEnter(p) \/ NoProof(p) \/ WorkerTakesIt(p) \/ WorkerRests(p)

TypedOnlyAtRest == bad # "typed-not-at-rest"
EnterOnlyOnProofAndOnce == \A p \in P : enters[p] <= 1 /\ (stage[p] \in {"delivered", "working", "finished"} => shown[p])
OneAtATime == \A p, q \in P : (p # q /\ stage[p] \in Typed) => stage[q] \notin Typed
NoLineNoTyping == \A p \in P : stage[p] = "refused" => enters[p] = 0
=============================================================================
