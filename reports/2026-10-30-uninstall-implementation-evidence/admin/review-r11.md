# Uninstall Review, Round 11

**1 finding: 0 critical, 0 high, 1 medium.** Reviewed `/home/parker/Work/td-uninstall-lab/terminal-delight` at `1c47b635`. CONFIRMED means traced through source; no scenarios executed. No builds, tests, TD execution, signals or code edits. This report is the only file written.

Both round-10 fixes address their reported cases. Gemini discovery honors its configuration-home override, and the reviewed command pins the client to the inspected configuration root. Planning, pruning and process inspection share the classifier for type 2, marked type 1 and legacy ISO-format AppImages. The remaining finding concerns the ADMIN preview of old builds.

## Medium

### M1. ADMIN Counts a Build with Unknown Use as Safe to Remove

Locations: `app/src/uninstall/builds.rs:103–108`, `builds.rs:167–173`, `builds.rs:186–197`, `builds.rs:409–429`; `app/src/admin_view.rs:52–68`, `admin_view.rs:364–383`; `app/src/lang/chrome.rs:1128–1129`.

Scenario: there are four dated TD builds. The newest is the launcher target. The oldest is also running, but reading that process's executable returns a permission error; its readable process name still identifies it as possible TD. This is the existing supported uncertainty case, such as a process made undumpable.

The scanner preserves the process with an empty executable path, and `list` puts its PID in `listing.uncertain`. It cannot associate that process with the oldest build, so that build's `running` list is empty. `prunable` considers only each build's individual keep reason and returns the oldest build as removable. `count_builds` discards `listing.uncertain` and records one build in use and one removable.

ADMIN therefore says one build is in use and offers to remove the one “that nothing runs,” with its byte count. Its second-click confirmation repeats that removal count. There are actually two builds in use, and the engine already knows its scan cannot establish which old build is running. With unchanged conditions, `prune` correctly refuses the entire operation because `listing.uncertain` is nonempty.

**CONFIRMED (traced).** Deletion remains blocked. The preview loses known uncertainty and presents both the usage count and removable count as established facts before the person confirms.

Suggested fix: carry the process-scan blocker into the ADMIN count result. Show that build use cannot be established, name the blocking PID, and disable pruning while it persists. Keep any count of positively identified running builds explicitly qualified. Add a summary/rendering regression using the existing unknown-process case; assert that the panel never claims a removable count or that nothing runs the candidate while the engine would refuse.
