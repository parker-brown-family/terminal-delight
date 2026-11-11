------------------------------- MODULE Launch -------------------------------
(* Launching an agent from the composer: write a sentence, press Enter (the *)
(* agent's tab comes forward and the page goes) or Ctrl+Enter (the tab waits *)
(* behind and the page stays, empty, saying it started).                    *)
(* Code: app/src/entry_view.rs composer_send (its refusals: nothing written,*)
(* a far project with no machine, a model the catalog does not offer, an    *)
(* effort the model does not take, a launch that fails), open_composer,     *)
(* composer_put_away; Workspace::adopt_pane, open_tab.                      *)
(* Abstracted: which refusal (one `problem`), pictures, far machines.       *)
VARIABLES page, sentence, problem, note, last, lost

vars == <<page, sentence, problem, note, last, lost>>

Init == page = "away" /\ sentence = "empty" /\ problem = FALSE /\ note = "none"
        /\ last = "none" /\ lost = FALSE

YouOpenIt == page = "away" /\ page' = "up" /\ note' = "none" /\ last' = "none"
             /\ UNCHANGED <<sentence, problem, lost>>
YouWrite == page = "up" /\ sentence' = "written" /\ note' = "none" /\ last' = "none"
            /\ UNCHANGED <<page, problem, lost>>
\* A chip set to something the launch will refuse, or set right again.
YouChooseSomething == page = "up" /\ problem' \in BOOLEAN /\ last' = "none"
                      /\ UNCHANGED <<page, sentence, note, lost>>
YouPutItAway == page = "up" /\ page' = "away" /\ last' = "none"
                /\ UNCHANGED <<sentence, problem, note, lost>>

\* Enter (bg = FALSE) or Ctrl+Enter (bg = TRUE).
YouSend(bg) ==
    /\ page = "up"
    /\ IF sentence = "empty" \/ problem
       THEN /\ note' = "why" /\ last' = "refused"
            /\ UNCHANGED <<page, sentence, problem, lost>>
       ELSE /\ sentence' = "empty"
            /\ last' = IF bg THEN "behind" ELSE "front"
            /\ page' = IF bg THEN "up" ELSE "away"
            /\ note' = IF bg THEN "started" ELSE "none"
            /\ UNCHANGED <<problem, lost>>

Next == YouOpenIt \/ YouWrite \/ YouChooseSomething \/ YouPutItAway
        \/ YouSend(FALSE) \/ YouSend(TRUE)

\* Nothing starts from an empty sentence, and a refusal says why and keeps
\* what was written.
ARefusalSaysWhyAndKeepsTheDraft == last = "refused" => (page = "up" /\ note = "why")
\* Enter takes you to the agent; Ctrl+Enter leaves you on an empty page that
\* says it started.
EnterTakesYouThere == last = "front" => page = "away"
BackgroundKeepsYouHere == last = "behind" => (page = "up" /\ sentence = "empty" /\ note = "started")
=============================================================================
