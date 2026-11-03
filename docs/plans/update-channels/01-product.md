# Product: Update Terminal Delight from inside TD

## Problem

Parker, 2026-11-02, by email: *"Add an Update Terminal Delight action that supports two explicit update channels"*. On the stable channel TD finds published releases, shows the current and available versions with release notes, and installs a verified artifact. On the dev channel a person opts in to development builds and sees the exact commit and date. The acceptance bar from that email:

- Stable install can discover a newer tagged release and update safely.
- An opted-in dev install can track the dev branch with a visible commit SHA.
- User can switch channels and roll back without losing live terminal sessions.
- Failures leave the old TD executable and persistent sessions intact.

Today an update means going to the releases page, downloading an AppImage, marking it executable and running it. That is three steps outside TD, with no record of what you ran before and no indication of whether your open terminals will survive. On Parker's own machine a different tool does the job: a swap script that is not in the repository. Nobody else has it.

## What the survey of the code changes about the request

These facts come from the existing code, and each one shapes a screen below. The file references are in Gate 2.

1. **Your terminals live in a separate process that does not update.** The session host is started from whichever build opened the first window, and it keeps running that build. Handing a running host over to a new build has been planned (seamless restart) but has not been built. An update can therefore always replace the **window** while every terminal stays alive. It can replace the **host** only by ending those terminals.
2. **Most updates only need the window.** The window and the host share a versioned wire format. While it is unchanged, a new window attaches to the old host and nothing is lost. When it changes, the new window cannot talk to the old host, and today it quietly stops the host, which ends every terminal. The updater must never take that path without asking.
3. **A build does not know its own commit.** `--version` prints `0.4.0-rc.3` whether you run rc.3 or a build three days of dev commits later. A visible commit SHA, the dev channel's whole promise, needs the build to carry its identity.
4. **"Verified" can only mean "matches the publisher's checksum".** Releases ship a `.sha256` beside the AppImage (rc.1 onward; v0.3.0 has none). Nothing is signed. A checksum from the same release page catches a corrupt or truncated download. It does not catch a compromised release. The screen will say exactly that and never "signed".
5. **Dev builds are compiled but not published.** CI builds an AppImage for every dev push, but GitHub keeps those behind a login. If dev updates should work for anyone, a public dev build has to be posted somewhere. Until it is, the dev channel offers the source-build command for the exact commit and does not pretend a download exists.
6. **Stable is older than dev.** The latest stable release is 0.3.0, and 0.4.0 is still in candidates, while dev runs 0.5.0 work. Taken literally, "Switch back to Stable" is a **downgrade**, and an older build may misread settings and saved sessions written by a newer one.

## Decisions this gate makes (the agent's calls, open to overturning)

- **D1: Updates replace the window, and the host keeps running.** When the new build is wire-compatible, Update installs it and offers **Restart window**. Every terminal and agent stays. The old host keeps running its old build until the next time TD starts cold, and the panel shows that it is doing so.
- **D2: An incompatible update is staged, never forced.** The panel says *"This update changes how the window talks to your terminals. Installing it ends your N open terminals."* It then offers **Restart everything now** or **Install at next cold start** (the default). Nothing ends without that press.
- **D3: Switching back to Stable does not downgrade.** It means "follow stable from here", and the build you run is kept until a stable release at least as new arrives. A separate, explicit **Install stable X now (older)** carries a warning about settings written by the newer build.
- **D4: Rollback goes to the previous build only.** One previous build is kept per install. **Roll back** goes to it, with the same window-or-everything rule as D1 and D2. It is not a version picker.
- **D5: Nothing happens on its own.** Checking is manual by default. An opt-in daily check raises a notification and never downloads or installs anything. The channel changes only from this panel.
- **D6: TD updates only installs it can update safely.**
  - *AppImage*: replaced in place, with the previous image kept beside it.
  - *Hand-installed launcher build* (Parker's daily layout): installed into the build library, and the launcher is repointed.
  - *Source checkout*: shows the commit and any uncommitted changes, and gives the commands to run. It never pulls or builds by itself.
  - *Anything owned by a system package*: shows the version and tells you to update with your package manager.
- **D7: A release without a checksum is shown but not installed.** It appears with "no checksum published — download it yourself" and a link. v0.3.0 is the case today.
- **D8: The first network request TD has ever made goes to GitHub's public release list and nothing else.** It carries no identifier and no terminal content. It is made only when someone presses Check now or has opted in to the daily check.

## The cutover: what survives an update, and what is backed up

Parker, on the Gate 1 page, 2026-11-02: *"are we 100% sure folks will not lose config and agent state current process etc.? … 2ndly if a backup of their config etc. is a good move"*.

**Not 100% today, and here is what we know.** Every swap of Parker's live window so far has been checked from the outside. The check reads the host process and its start time, every shell's process and start time, the saved layout, the MCP policy and the window geometry, before and after. Recorded results include 36 terminal identities kept on 2026-10-24, 33 kept on 2026-10-22, and 22 open panes kept on 2026-10-30. **Running terminals and agents survive a window-only update.** That is measured, not hoped.

Three things are known to be lost on a window restart, and the updater has to name each one, not lose it quietly:

1. **Closed tabs still waiting to be undone.** A tab you closed is held by the window so that undo can bring it back, and restarting the window ends it. The panel will say *"2 closed tabs can still be reopened — restarting the window lets them go"*.
2. **A floating document's view.** The file comes back but its scroll position does not, because it is not saved.
3. **Anything a window holds only in memory.** This one is not yet enumerated. Gate 2 lists it, and the qualification run checks it.

Settings are not at risk from an ordinary update, because a newer build reads older settings forward. The risk is going *backwards* (rollback, or an older stable). An older build may misread settings a newer one wrote, and that is what a backup is for.

- **D9: Every install backs up your settings first.** Before an update or rollback touches anything, TD copies its settings and layout (`~/.config/terminal-delight`, 19 MB on Parker's machine) and its small state records into one timestamped backup, kept beside the previous build. Small state records means the Threshing Floor, agent records, rest and wake records, notes and handovers. **Roll back** restores the build and offers to restore that backup with it. The bulky history is not copied: conversations, surfaces and brief backups, about 1.2 GB of the 1.3 GB state on Parker's machine. An update only ever appends to it. Gate 2 has to prove that claim (that no migration rewrites those files) before relying on it. The settings export that uninstall already has is the first thing to reuse.
- **D10: Losses are named before the press, not discovered after.** Restart window lists anything from the three cases above that is present right now. With nothing to lose it says *"nothing is lost"*. It never says "safe" when it has not checked.
- **D11: A full restart resumes agents where it can.** In the rare host-change case (D2), TD already knows how to start an agent again with `--resume` on its own launch line. That is the path used to wake a sleeping agent. So **Restart everything now** would bring agents back into their conversations, and only plain shells would start fresh. Whether every agent harness supports that is a Gate 2 question. Until it is answered, the panel promises only what it has checked.

## Success metric

TD counts no installs and this feature adds no telemetry, so adoption cannot be measured and nothing here pretends to measure it. The measurable number is **zero terminals lost across the qualification run**. That run is at least ten real updates: stable → stable, stable → dev, dev → dev, dev → stable, a rollback, and a forced failure at each of download, checksum and smoke check. For each update we read the host's process and start time and every terminal's shell process before and after, and they must match. Every update must also leave a backup that restores byte-for-byte, and must leave the settings files unchanged except where the new build's own migration is recorded as changing them. The secondary number is the person's effort: an update with no host change takes **two presses** (Update, then Restart window).

## Announcement — the blog post before the feature

> Terminal Delight now updates itself. Open ⚙ CONFIG → UPDATES to see what you are running (version, commit and date), whether something newer is out, and what changed. One press downloads the release and checks it against its published checksum, and a second restarts the window. Your terminals and agents keep running through it, because they never lived in the window. If an update ever needs your terminals to restart, TD says so and waits for you. Want the newest work before it ships? Switch to the Dev channel, which is clearly experimental and shows the exact commit you are on, and switch back whenever you like. The build you had before is always one press away.

## Screens

The mockups live in a single page with every state drawn at panel width, using the real strings and real version numbers: `mockups/settings-updates.html`.

1. Stable, update available: current versus available, release notes, Update.
2. Downloaded and checked, window-only: Restart window, with *your 23 terminals keep running*.
3. Downloaded and checked, host change: the two-button choice from D2.
4. Dev channel: experimental band, commit and date, plus CI build or source-build command.
5. Can't update here: source checkout with uncommitted changes, or package-managed.
6. Switch back to Stable when stable is older (D3), and Roll back (D4).
7. Failure: checksum mismatch, old build untouched.

## Answered

**Q1: Ship in 0.5.0? Yes.** Parker, 2026-11-02, on the Gate 1 page: *"100% yes."* It is built on `dev/0.5.0`, and the qualification run (Success metric) has to pass inside the November 9–12 window ahead of the November 13 candidate.
