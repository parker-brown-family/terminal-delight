----------------------------- MODULE SettingsBook -----------------------------
(***************************************************************************)
(* The Settings book: open, a page, a search, a panel shown in place, and *)
(* the ways back. Code: app/src/settings_view.rs (open_settings,           *)
(* settings_go, settings_key, settings_peel, st_open, unhost, hosting,     *)
(* close_settings). A hosted panel has its own flag in the window and can  *)
(* close itself (a press on the page around it), which `hosting` reads     *)
(* through.                                                                *)
(***************************************************************************)
VARIABLES book, searching, host, panel

vars == <<book, searching, host, panel>>
\* book: "closed" | "open"
\* searching: the search box holds text
\* host: the book says it shows a panel in place
\* panel: that panel's own flag in the window is up

Hosting == host /\ panel

Init == book = "closed" /\ searching = FALSE /\ host = FALSE /\ panel = FALSE

YouOpen == book = "closed" /\ book' = "open" /\ UNCHANGED <<searching, host, panel>>
YouType == book = "open" /\ ~Hosting /\ searching' = TRUE /\ UNCHANGED <<book, host, panel>>
YouPickAResult == book = "open" /\ searching /\ searching' = FALSE /\ UNCHANGED <<book, host, panel>>
\* st_open with a host: unhost, set the host, run the opener (the flag goes up).
YouPressOpen == book = "open" /\ ~searching /\ ~Hosting /\ host' = TRUE /\ panel' = TRUE /\ UNCHANGED <<book, searching>>
\* A press on the page around the panel: the panel's own scrim drops its flag.
YouClickBesideThePanel == Hosting /\ panel' = FALSE /\ UNCHANGED <<book, searching, host>>
\* settings_go: unhost first.
YouPickAnotherPage == book = "open" /\ host' = FALSE /\ panel' = FALSE /\ searching' = FALSE /\ UNCHANGED book
\* Esc: settings_peel. A hosted panel closes first; then a search clears;
\* then the book closes (close_settings unhosts too).
YouPressEsc ==
    /\ book = "open"
    /\ IF Hosting THEN host' = FALSE /\ panel' = FALSE /\ UNCHANGED <<book, searching>>
       ELSE IF searching THEN searching' = FALSE /\ UNCHANGED <<book, host, panel>>
       ELSE book' = "closed" /\ host' = FALSE /\ panel' = FALSE /\ UNCHANGED searching

Next == YouOpen \/ YouType \/ YouPickAResult \/ YouPressOpen \/ YouClickBesideThePanel \/ YouPickAnotherPage \/ YouPressEsc

Spec == Init /\ [][Next]_vars

\* A panel shown in place never outlives the book.
NoPanelWithoutTheBook == book = "closed" => ~panel

\* Esc never skips a level: with a panel up, the book is still open after it.
EscGoesBackOneLevel == Hosting => book = "open"
=============================================================================
