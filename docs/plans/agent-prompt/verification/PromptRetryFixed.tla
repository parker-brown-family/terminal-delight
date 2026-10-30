-------------------------- MODULE PromptRetryFixed --------------------------
EXTENDS Naturals

\* The first sender/key names A. B makes the prompt clock run after A ends.
\* The receipt is written before A can be delivered and outlives its detail.
VARIABLES a, b, detail, receipt, late, store, firstAssignments,
          freshAssignments, retry
vars == <<a, b, detail, receipt, late, store, firstAssignments,
          freshAssignments, retry>>

Init ==
    /\ a = "absent"
    /\ b = "absent"
    /\ detail = FALSE
    /\ receipt = FALSE
    /\ late = FALSE
    /\ store = "healthy"
    /\ firstAssignments = 0
    /\ freshAssignments = 0
    /\ retry = "not_asked"

StoreFails ==
    /\ a = "absent"
    /\ store = "healthy"
    /\ store' = "failed"
    /\ UNCHANGED <<a, b, detail, receipt, late, firstAssignments,
                  freshAssignments, retry>>

StoreRecovers ==
    /\ a = "absent"
    /\ store = "failed"
    /\ store' = "healthy"
    /\ UNCHANGED <<a, b, detail, receipt, late, firstAssignments,
                  freshAssignments, retry>>

SendFirst ==
    /\ a = "absent"
    /\ store = "healthy"
    /\ a' = "waiting"
    /\ detail' = TRUE
    /\ receipt' = TRUE
    /\ firstAssignments' = 1
    /\ UNCHANGED <<b, late, store, freshAssignments, retry>>

PasteFirst ==
    /\ a = "waiting"
    /\ a' = "typing"
    /\ UNCHANGED <<b, detail, receipt, late, store, firstAssignments,
                  freshAssignments, retry>>

EnterFirst ==
    /\ a = "typing"
    /\ a' = "delivered"
    /\ UNCHANGED <<b, detail, receipt, late, store, firstAssignments,
                  freshAssignments, retry>>

FinishFirst ==
    /\ a = "delivered"
    /\ a' = "finished"
    /\ UNCHANGED <<b, detail, receipt, late, store, firstAssignments,
                  freshAssignments, retry>>

RetentionPasses ==
    /\ a = "finished"
    /\ ~late
    /\ late' = TRUE
    /\ UNCHANGED <<a, b, detail, receipt, store, firstAssignments,
                  freshAssignments, retry>>

AnotherPromptOpens ==
    /\ a = "finished"
    /\ b = "absent"
    /\ b' = "waiting"
    /\ UNCHANGED <<a, detail, receipt, late, store, firstAssignments,
                  freshAssignments, retry>>

TickPrunesEnded ==
    /\ a = "finished"
    /\ b = "waiting"
    /\ late
    /\ detail
    /\ a' = "pruned"
    /\ detail' = FALSE
    /\ UNCHANGED <<b, receipt, late, store, firstAssignments,
                  freshAssignments, retry>>

WindowReopens ==
    /\ a \in {"waiting", "typing", "delivered", "finished", "pruned"}
    /\ detail
    /\ a' = "reopened"
    /\ detail' = FALSE
    /\ UNCHANGED <<b, receipt, late, store, firstAssignments,
                  freshAssignments, retry>>

RetryWithDetail ==
    /\ receipt
    /\ detail
    /\ retry = "not_asked"
    /\ retry' = "same_assignment"
    /\ UNCHANGED <<a, b, detail, receipt, late, store,
                  firstAssignments, freshAssignments>>

RetryAfterDetail ==
    /\ receipt
    /\ ~detail
    /\ retry = "not_asked"
    /\ retry' = "expired"
    /\ UNCHANGED <<a, b, detail, receipt, late, store,
                  firstAssignments, freshAssignments>>

StartFreshWork ==
    /\ receipt
    /\ a \in {"finished", "pruned", "reopened"}
    /\ retry = "expired"
    /\ store = "healthy"
    /\ freshAssignments = 0
    /\ freshAssignments' = 1
    /\ UNCHANGED <<a, b, detail, receipt, late, store,
                  firstAssignments, retry>>

Done ==
    /\ retry # "not_asked"
    /\ UNCHANGED vars

Next == StoreFails \/ StoreRecovers \/ SendFirst \/ PasteFirst \/ EnterFirst \/
        FinishFirst \/ RetentionPasses \/ AnotherPromptOpens \/
        TickPrunesEnded \/ WindowReopens \/ RetryWithDetail \/
        RetryAfterDetail \/ StartFreshWork \/ Done

Spec == Init /\ [][Next]_vars

NoDuplicateFirstAssignment == firstAssignments <= 1
AcceptedHasReceipt == firstAssignments = 0 \/ receipt
ExpiredMeansUnknownDetail == retry # "expired" \/ (~detail /\ receipt)
FreshWorkRemainsPossible ==
    (receipt /\ a \in {"finished", "pruned", "reopened"}
     /\ retry = "expired" /\ store = "healthy" /\ freshAssignments = 0)
    => ENABLED StartFreshWork
=============================================================================
