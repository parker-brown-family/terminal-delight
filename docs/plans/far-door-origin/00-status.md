# The far door: telling another machine from a process on this one

Status: **approved by Parker, 2026-11-25 (option A), and shipped in 0.5.5.** Issue #1467. Difficulty 5/10: one rule and one switch, but the rule decides who may type into an agent, and a wrong one fails silently in either direction.

## The door, and what is wrong with it

Another machine's Threshing Floor reaches this one by running `terminal-delight ctl --all mcp rpc` here over ssh (`farfloor::rpc_script`). The window answers three methods for it: `far/census`, `far/prompt` and `far/prompt_status` (`mcp::far_door`). What the caller may then do is the ceiling a person set for the machine it names, in Settings › Machines (`sharing.toml`).

The door turned away a caller in a pane: one that named a pane, or whose process descended from a pane's shell. Everything else it took for another machine. "Not in a pane" is not "another machine":

- `setsid -f` from a pane's shell gives a process whose parent is the user's `systemd`, outside the pane (measured on 2026-11-17: `320749 → 1146 (systemd)`).
- An agent supervised by a user service (a background session) never was in a pane.

Either one can call `far/prompt` with `from` set to any machine at **prompt** in `sharing.toml`, and its words are typed into an agent here with no line on the Floor. `ctl.rs` learned the same lesson on 2026-10-23 for the person-acting verbs: a caller's ancestry outside a pane is not an identity.

## What a real call looks like

Walked on the far box over ssh, 2026-11-17:

| pid | uid | comm |
|---|---|---|
| 335715 | 1000 | bash (the command ssh ran) |
| 335714 | 1000 | `sshd-session` |
| 335711 | **0** | `sshd-session` |
| 993 | **0** | `sshd` |

and from a pane on the Legion:

| pid | uid | comm |
|---|---|---|
| 2443233 | 1000 | bash |
| 102158 | 1000 | claude |
| 101613 | 1000 | bash (the pane's shell) |
| 3958327 | 1000 | `td-a47eabf8-wav` (the window) |
| 1146 | 1000 | systemd (the user's manager) |

A process owned by **root** and named `sshd` / `sshd-session` sits above every ssh-delivered command, and above nothing else. A user cannot make one: renaming a process changes its name, not its owner.

## The options

| | A. Root `sshd` ancestor | B. Pairing secret | C. Session of an sshd login | D. Say it in Settings |
|---|---|---|---|---|
| **Rule** | the peer's parents include a uid-0 process named `sshd` or `sshd-session` | the calling machine presents a token issued here when the person shared with it | the peer's cgroup is a logind session whose service is `sshd` (`/proc/<pid>/cgroup`, `/run/systemd/sessions/<n>`) | no rule; a line under the prompt chip says a program on this machine can use the door |
| **Blocks `setsid -f`** | yes | no: the agent reads the token from the same home | yes: a user process cannot move into a root-owned session scope | no |
| **Blocks a service-supervised agent** | yes | no | yes | no |
| **Blocks an agent that runs `ssh localhost`** | no, and none of these can: that is "ssh as the same person", the risk `farfloor.rs` already states | no | no | no |
| **Real ssh from the far machine** | works (measured chain above) | works, after a person copies the token across | works where logind runs (Omarchy, most distros); not in a container or on a system without logind | works |
| **The two-home rig** (`~/Work/td-multibox/rig`, stand-in ssh) | needs the window launched with `TD_FAR_DOOR_LOCAL=1` | works if the rig copies tokens | needs the same switch | works |
| **Cost** | about 60 lines in `ctl.rs` beside `descends_from`, one snapshot field, one switch, tests; no UI | a pairing flow in Settings, nine languages, a secret on disk, rotation | depends on systemd's file layout; parsing a private format | one string in nine languages |

**Why not "refuse a peer outside any session's process tree"?** Read literally (refuse a peer that descends from no Terminal Delight session), it refuses every real call too: a command ssh runs on this machine descends from `sshd`, not from a session host. Read as "refuse a peer outside an sshd login session", it is option C.

## Recommendation: A

A blocks both bypasses the issue names and keeps real ssh working with no setup. It depends on nothing but `/proc`, which the window already reads for `descends_from`. It leaves exactly the risk the module's header already states: ssh as the same person can do anything that person can. C blocks the same things but trades `/proc` for logind's private files. B blocks nothing against an agent in the same home. D is worth adding beside A, not instead of it.

What A deliberately does not do:

- **It does not stop `ssh localhost`.** On the Legion that fails today on strict host-key checking, but an agent can turn that off, and whether it then logs in is up to `authorized_keys`.
- **An agent running inside someone's ssh login on this machine**, not in a pane, passes. It arrived over ssh as the person.
- **Unknown stays shut.** If a parent cannot be read (it exited mid-walk, or `/proc` refused), the origin is `None` and the door refuses with its own sentence.

## What changes (draft pull request)

- `ctl.rs`: `Hop`, `chain_arrived_over_ssh` (the pure rule) and `arrived_over_ssh(pid) -> Option<bool>` (the `/proc` walk, up to 64 parents).
- `mcp.rs`: `Snapshot.peer_over_ssh: Option<bool>` and `Snapshot.far_door_local: bool`. `far_door` refuses with -32003 unless the call came over ssh or the window was launched for the rig. A pane is still refused with -32002 either way.
- `mcp_transport.rs`: the walk runs only for far-door methods. `TD_FAR_DOOR_LOCAL=1` is read once at launch, like `TD_MCP_SCRIPTED`, so an agent cannot set it on a running window.
- **The rig**: `mb.sh up b TD_FAR_DOOR_LOCAL=1`. The rig lives outside this repository and is not changed here.

## Proof

- The rule on the two real chains, a forged `sshd-session` owned by the user, and pid 1 alone.
- A process detached with `setsid` reads `Some(false)` from the real `/proc`.
- The test process itself reads `Some(true)` when, and only when, it runs inside an ssh session. That test was run on the far box over ssh to show the real path still opens.
- The door refuses `Some(false)` and `None`, opens for `Some(true)` and for the rig's switch, and still refuses a pane under the switch.
- Mutation runs are listed in the pull request.

## Needs Parker

- Nothing. Parker approved A on 2026-11-25.
- Whether to add D's sentence in Settings › Machines next to the prompt chip (nine languages).
