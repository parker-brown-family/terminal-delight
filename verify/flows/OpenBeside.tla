----------------------------- MODULE OpenBeside -----------------------------
(* An agent opening its document beside itself (open_document): `here`    *)
(* puts it in the agent's own pane's floating square, `beside` in a split,  *)
(* `side` in the right-hand side panel. A person opens with a click, the    *)
(* artifact rail or `ctl doc here`.                                         *)
(* Code: app/src/mcp.rs open_document (placement and path checks), main.rs *)
(* the open request (Here and Side go to open_here, Beside splits),         *)
(* app/src/pane/doc.rs open_here (HELD when a square is up), open_float     *)
(* (a person's road, which replaces), art_held (the rail lists it).         *)
VARIABLES square, rail, last, replaced

vars == <<square, rail, last, replaced>>
Init == square = "none" /\ rail = FALSE /\ last = "none" /\ replaced = FALSE

PersonOpens == square' = "person" /\ last' = "person-opened" /\ UNCHANGED <<rail, replaced>>
PersonCloses == square # "none" /\ square' = "none" /\ last' = "closed" /\ UNCHANGED <<rail, replaced>>
\* open_document placement `here`: held when any square is already up.
AgentOpensHere == /\ IF square = "none"
                     THEN square' = "agent" /\ last' = "opened" /\ UNCHANGED rail
                     ELSE square' = square /\ rail' = TRUE /\ last' = "held"
                  /\ replaced' = (replaced \/ (square = "person" /\ square' # "person"))
\* `beside` splits; `side` opens a side-panel tab: neither touches the square.
AgentOpensBesideOrSide == last' = "opened-elsewhere" /\ UNCHANGED <<square, rail, replaced>>
\* A path or placement the checks refuse.
AgentIsRefused == last' = "refused" /\ UNCHANGED <<square, rail, replaced>>
PersonOpensFromTheRail == rail /\ square' = "person" /\ rail' = FALSE /\ last' = "person-opened" /\ UNCHANGED replaced

Next == PersonOpens \/ PersonCloses \/ AgentOpensHere \/ AgentOpensBesideOrSide
        \/ AgentIsRefused \/ PersonOpensFromTheRail

\* An agent never replaces what a person opened.
AnAgentNeverReplacesYourDocument == ~replaced
\* What an agent could not open is offered on the rail.
AHeldOpenIsOffered == last = "held" => rail
=============================================================================
