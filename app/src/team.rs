//! High trust agentic: the agent a person talks to starts its own team.
//!
//! Until now every agent in a window was started by a person, and every line
//! between two agents was drawn by a person. That is the Threshing Floor's
//! founding rule, and it stays the default. This module is the one deliberate
//! exception, and a person has to switch it on.
//!
//! Parker, 2026-11-15: *"Better to have an agent that the human talks to and
//! to tell that agent: SPIN UP THIS TEAM CONFIGURATION"*, and *"maybe that is
//! how we secure the wall around this behaviour: a SETTING that says HIGH
//! TRUST AGENTIC"*. The reason is cost. An agent program's own subagents
//! re-read their whole context every turn and nobody can see them work. A
//! team of real panes is slower to start, and each member is a terminal a
//! person can watch, stop, and talk to.
//!
//! With the setting on, one verb (`start_team`) lets an agent in a pane open
//! a tab, start up to [`MAX_MEMBERS`] agents in it with the program, model,
//! effort and first prompt it names, and hold a line to each. Four rules keep
//! that from being more than it says:
//!
//! - **Off by default, per window.** With the switch off the verb refuses and
//!   names the switch. Nothing else about the window changes when it is on.
//! - **Only a promoted orchestrator.** The switch alone lets no agent start
//!   anything. A person also promotes the particular pane, on the Threshing
//!   Floor, and only a promoted pane may call the verb. Parker, the same day,
//!   on reading the first version, which let any agent a person had started
//!   do it: *"this requires FLAGGING or PROMOTING orchestrators... otherwise
//!   chaos and token burn compounding is possible. so we are still doing
//!   ENGINEERING work"*. An agent a team started is not promoted, so teams
//!   are one level deep unless a person promotes a member on purpose.
//! - **A count, held.** At most [`MAX_LIVE`] agents started this way may be
//!   running in a window at once, however many coordinators ask.
//! - **Lines only downward, only to its own.** The coordinator gets a line to
//!   each agent it started and nothing else: no line back, none between
//!   members, none to any pane that was already there. Those remain a
//!   person's to draw.
//!
//! Pure: what a request may say and whether it may go ahead. The window
//! starts the panes (`team_view.rs`).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::launcher::{Harness, Reach as Permissions};
use crate::threshing_floor::{Rung, SeatId};

/// The most agents one `start_team` may start: with one tab holding four
/// panes, a team is one tab.
pub const MAX_MEMBERS: usize = 4;

/// The most agents started by agents that may be running in one window at
/// once. Two full teams.
pub const MAX_LIVE: usize = 8;

/// The longest a team's or a member's name may be.
pub const NAME_MAX: usize = 40;

/// One agent a coordinator asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    /// What the pane is called: `reviewer`, `research`.
    pub role: String,
    pub harness: Harness,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// What the agent is told first. `None`: it starts and waits.
    pub prompt: Option<String>,
    /// How far the coordinator's line to it reaches.
    pub line: Rung,
    /// How far the agent itself may act without asking, as the launcher's
    /// own dial words it.
    pub permissions: Permissions,
}

/// A whole request, checked for shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spec {
    /// The tab's name, and the org's.
    pub name: String,
    /// Where every member starts.
    pub project: PathBuf,
    pub members: Vec<Member>,
}

fn name_ok(text: &str) -> Option<String> {
    let name: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!name.is_empty() && name.chars().count() <= NAME_MAX && !name.chars().any(|c| c.is_control()))
        .then_some(name)
}

/// A model or effort word as it will sit on a command line: what the agent
/// programs themselves use for these, and nothing a shell reads as more.
fn word_ok(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-:/[]".contains(c))
        && !text.starts_with('-')
}

/// Read a `start_team` call's arguments.
///
/// `caller_cwd` is where the calling agent's own pane is: a team with no
/// `project` starts there, since a coordinator's team works where it works.
/// Every refusal is a sentence the calling agent can act on.
pub fn parse(args: &serde_json::Value, caller_cwd: Option<&Path>) -> Result<Spec, String> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .and_then(name_ok)
        .ok_or_else(|| {
            format!("start_team needs `name`: what to call the team, up to {NAME_MAX} characters.")
        })?;
    let project = match args.get("project").and_then(|v| v.as_str()) {
        Some(p) if Path::new(p).is_absolute() => PathBuf::from(p),
        Some(p) => {
            return Err(format!(
                "`project` {p:?} is not an absolute path. Give the folder the team works in, in full."
            ))
        }
        None => caller_cwd.map(Path::to_path_buf).ok_or(
            "start_team needs `project`: the folder the team works in. Your own pane's folder is not known, so it cannot stand in.",
        )?,
    };
    let list = args
        .get("agents")
        .and_then(|v| v.as_array())
        .filter(|a| !a.is_empty())
        .ok_or("start_team needs `agents`: at least one agent to start.")?;
    if list.len() > MAX_MEMBERS {
        return Err(format!(
            "a team is at most {MAX_MEMBERS} agents, one tab's worth; this asks for {}.",
            list.len()
        ));
    }
    let mut members: Vec<Member> = Vec::with_capacity(list.len());
    for (i, a) in list.iter().enumerate() {
        let n = i + 1;
        let role = a
            .get("role")
            .and_then(|v| v.as_str())
            .and_then(name_ok)
            .ok_or_else(|| format!("agent {n} needs `role`: what to call it, such as reviewer."))?;
        if members.iter().any(|m| m.role.eq_ignore_ascii_case(&role)) {
            return Err(format!(
                "two agents are both called {role:?}. Give each its own role, or they cannot be told apart."
            ));
        }
        let harness_word = a.get("harness").and_then(|v| v.as_str()).unwrap_or("");
        let harness = Harness::from_label(&harness_word.to_ascii_lowercase()).ok_or_else(|| {
            let known: Vec<&str> = Harness::ALL.iter().map(|h| h.label()).collect();
            format!(
                "agent {n} ({role}): `harness` {harness_word:?} is not a program this Terminal Delight starts. It starts: {}.",
                known.join(", ")
            )
        })?;
        let word = |key: &str| -> Result<Option<String>, String> {
            match a.get(key).and_then(|v| v.as_str()).map(str::trim) {
                None | Some("") => Ok(None),
                Some(w) if word_ok(w) => Ok(Some(w.to_string())),
                Some(w) => Err(format!(
                    "agent {n} ({role}): `{key}` {w:?} is not a word the program takes."
                )),
            }
        };
        let (model, effort) = (word("model")?, word("effort")?);
        let prompt = match a.get("prompt").and_then(|v| v.as_str()) {
            None => None,
            Some(p) => {
                let clean = crate::prompt::clean(p);
                let chars = clean.chars().count();
                if chars > crate::threshing_floor::PROMPT_MAX_CHARS {
                    return Err(format!(
                        "agent {n} ({role}): its first prompt is {chars} characters; the most is 32,000."
                    ));
                }
                (!clean.trim().is_empty()).then_some(clean)
            }
        };
        let line = match a.get("line").and_then(|v| v.as_str()) {
            None | Some("prompt") => Rung::Prompt,
            Some("ask") => Rung::Ask,
            Some("read") => Rung::Read,
            Some("see") => Rung::See,
            Some(w) => {
                return Err(format!(
                    "agent {n} ({role}): `line` {w:?} is not see, read, ask or prompt."
                ))
            }
        };
        let permissions = match a.get("permissions").and_then(|v| v.as_str()) {
            None | Some("anywhere") => Permissions::Anywhere,
            Some("machine") => Permissions::Machine,
            Some("repo") => Permissions::Repo,
            Some(w) => {
                return Err(format!(
                    "agent {n} ({role}): `permissions` {w:?} is not repo, machine or anywhere."
                ))
            }
        };
        members.push(Member {
            role,
            harness,
            model,
            effort,
            prompt,
            line,
            permissions,
        });
    }
    Ok(Spec {
        name,
        project,
        members,
    })
}

/// What the window knows about whoever is asking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Asker {
    /// The window's MCP switch.
    pub mcp: bool,
    /// The High trust agentic switch.
    pub trusted: bool,
    /// Whether the caller's pane is running an agent.
    pub is_agent: bool,
    /// Whether a person promoted the caller's pane to orchestrator.
    pub promoted: bool,
    /// Whether the caller was itself started by `start_team`. It changes
    /// only what a refusal says: a member is told to ask its coordinator.
    pub is_member: bool,
    /// Agents started by agents and still running in this window.
    pub live: usize,
}

/// Whether a team of `asking` agents may start, or the sentence that says why
/// not. Checked in this order: the window's MCP switch, the High trust
/// agentic switch, a caller that is an agent, a caller a person promoted to
/// orchestrator, and room under [`MAX_LIVE`].
pub fn decide(asker: &Asker, asking: usize) -> Result<(), String> {
    if !asker.mcp {
        return Err("MCP exposure is disabled.".into());
    }
    if !asker.trusted {
        return Err(
            "High trust agentic is off in this window, so an agent cannot start other agents here. A person switches it on in Settings, under Sharing & access. Until then, ask your person to start the agents you need."
                .into(),
        );
    }
    if !asker.is_agent {
        return Err("start_team is for an agent in a pane; this pane is not running one.".into());
    }
    if !asker.promoted {
        return Err(if asker.is_member {
            "you were started by another agent's team, and an agent started that way cannot start a team of its own unless a person promotes it. Ask your coordinator."
                .into()
        } else {
            "your pane has not been promoted to orchestrator, and only an orchestrator may start agents. A person promotes a pane on the Threshing Floor: right-click its card, then Orchestrator. Ask your person."
                .into()
        });
    }
    if asker.live + asking > MAX_LIVE {
        return Err(format!(
            "this window already has {} agents that agents started; the most at once is {MAX_LIVE}, and this asks for {asking} more. Close some first, or ask your person.",
            asker.live
        ));
    }
    Ok(())
}

/// One team, as the window remembers it: saved with the session, because who
/// started whom is what stops a member starting a team after a restart.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub id: u64,
    pub name: String,
    /// The coordinator's seat.
    pub by: SeatId,
    /// The seats of the agents it started.
    #[serde(default)]
    pub members: Vec<SeatId>,
    pub started_ms: u64,
}

/// Every team this window has started.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Teams {
    teams: Vec<Record>,
}

impl Teams {
    pub fn from_saved(teams: Vec<Record>) -> Self {
        Self { teams }
    }

    pub fn saved(&self) -> &[Record] {
        &self.teams
    }

    pub fn add(&mut self, record: Record) {
        self.teams.push(record);
    }

    /// Whether `seat` belongs to an agent some team started.
    pub fn is_member(&self, seat: SeatId) -> bool {
        self.teams.iter().any(|t| t.members.contains(&seat))
    }

    /// Every seat an agent started. The window counts live members by pane
    /// (`team_view::team_live`); this is what a saved record says.
    #[cfg(test)]
    pub fn member_seats(&self) -> impl Iterator<Item = SeatId> + '_ {
        self.teams.iter().flat_map(|t| t.members.iter().copied())
    }

    /// Forget seats nobody holds any more, and teams with nobody left in
    /// them, so a closed team frees its room under [`MAX_LIVE`].
    pub fn retain_seats(&mut self, held: &dyn Fn(SeatId) -> bool) {
        for t in &mut self.teams {
            t.members.retain(|s| held(*s));
        }
        self.teams.retain(|t| !t.members.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn asker() -> Asker {
        Asker {
            mcp: true,
            trusted: true,
            is_agent: true,
            promoted: true,
            is_member: false,
            live: 0,
        }
    }

    /// The limits hold at their edges. The first cargo-mutants run
    /// (2026-11-17) found `>` turned `>=` survived on the member count and
    /// the first prompt's length, and `+` turned `*` on the live count.
    #[test]
    fn a_team_at_each_limit_is_allowed_and_one_past_it_is_not() {
        let member = |n: usize| json!({ "role": format!("r{n}"), "harness": "claude" });
        let team = |n: usize| json!({ "name": "t", "project": "/tmp", "agents": (0..n).map(member).collect::<Vec<_>>() });
        assert_eq!(
            parse(&team(MAX_MEMBERS), None).unwrap().members.len(),
            MAX_MEMBERS
        );
        assert!(parse(&team(MAX_MEMBERS + 1), None).is_err());
        let with_prompt = |chars: usize| {
            json!({ "name": "t", "project": "/tmp", "agents": [
                { "role": "r", "harness": "claude", "prompt": "x".repeat(chars) } ] })
        };
        let longest = crate::threshing_floor::PROMPT_MAX_CHARS;
        assert!(parse(&with_prompt(longest), None).is_ok());
        assert!(parse(&with_prompt(longest + 1), None).is_err());
        let live = |live| Asker { live, ..asker() };
        assert!(
            decide(&live(MAX_LIVE - 2), 2).is_ok(),
            "exactly full is allowed"
        );
        assert!(decide(&live(MAX_LIVE - 1), 2).is_err());
        assert!(decide(&live(3), 3).is_ok(), "3 live and 3 more is 6, not 9");
    }

    #[test]
    fn a_team_is_read_with_the_defaults_a_coordinator_would_expect() {
        let spec = parse(
            &json!({
                "name": "  vendor   migration ",
                "agents": [
                    { "role": "review", "harness": "Codex", "model": "sol-6.1", "effort": "high",
                      "prompt": "Review the migration plan." },
                    { "role": "ideas", "harness": "claude", "line": "ask", "permissions": "repo" },
                ]
            }),
            Some(Path::new("/work/vendor")),
        )
        .unwrap();
        assert_eq!(spec.name, "vendor migration", "spaces folded");
        assert_eq!(
            spec.project,
            Path::new("/work/vendor"),
            "where the caller works"
        );
        assert_eq!(spec.members.len(), 2);
        let review = &spec.members[0];
        assert_eq!(
            review.harness,
            Harness::Codex,
            "the program's name in any case"
        );
        assert_eq!(review.model.as_deref(), Some("sol-6.1"));
        assert_eq!(
            review.line,
            Rung::Prompt,
            "the line a coordinator needs by default"
        );
        assert_eq!(
            review.permissions,
            Permissions::Anywhere,
            "the launcher's own default"
        );
        assert_eq!(review.prompt.as_deref(), Some("Review the migration plan."));
        let ideas = &spec.members[1];
        assert_eq!(
            (ideas.line, ideas.permissions),
            (Rung::Ask, Permissions::Repo)
        );
        assert_eq!(ideas.prompt, None, "no first prompt: it starts and waits");
        assert_eq!(
            ideas.model, None,
            "no model named is the program's own default"
        );
    }

    #[test]
    fn every_refusal_names_the_agent_and_the_word_that_is_wrong() {
        let cwd = Some(Path::new("/w"));
        let bad = |args: serde_json::Value| parse(&args, cwd).unwrap_err();
        let one = |agent: serde_json::Value| json!({ "name": "t", "agents": [agent] });
        assert!(bad(json!({ "agents": [] })).contains("`name`"));
        assert!(bad(json!({ "name": "t" })).contains("at least one agent"));
        assert!(bad(json!({ "name": "t", "agents": [] })).contains("at least one agent"));
        assert!(bad(one(json!({ "harness": "claude" }))).contains("agent 1 needs `role`"));
        let unknown = bad(one(json!({ "role": "r", "harness": "astra" })));
        assert!(
            unknown.contains("\"astra\"") && unknown.contains("claude, codex, local"),
            "{unknown}"
        );
        assert!(bad(one(
            json!({ "role": "r", "harness": "claude", "model": "x; rm -rf ~" })
        ))
        .contains("`model`"));
        assert!(bad(one(
            json!({ "role": "r", "harness": "claude", "effort": "--dangerously" })
        ))
        .contains("`effort`"));
        assert!(bad(one(
            json!({ "role": "r", "harness": "claude", "line": "command" })
        ))
        .contains("see, read, ask or prompt"));
        assert!(bad(one(
            json!({ "role": "r", "harness": "claude", "permissions": "root" })
        ))
        .contains("repo, machine or anywhere"));
        assert!(bad(json!({ "name": "t", "project": "relative/path",
            "agents": [{ "role": "r", "harness": "claude" }] }))
        .contains("not an absolute path"));
        let twins = bad(json!({ "name": "t", "agents": [
            { "role": "Review", "harness": "claude" }, { "role": "review", "harness": "codex" } ] }));
        assert!(twins.contains("both called"), "{twins}");
        let five: Vec<_> = (0..5)
            .map(|i| json!({ "role": format!("r{i}"), "harness": "claude" }))
            .collect();
        assert!(bad(json!({ "name": "t", "agents": five })).contains("at most 4"));
        // No folder named and none known: asked for, never guessed.
        assert!(
            parse(&one(json!({ "role": "r", "harness": "claude" })), None)
                .unwrap_err()
                .contains("`project`")
        );
        // A first prompt past the limit is refused whole, never cut.
        let long = "x".repeat(crate::threshing_floor::PROMPT_MAX_CHARS + 1);
        assert!(bad(one(
            json!({ "role": "r", "harness": "claude", "prompt": long })
        ))
        .contains("32,000"));
    }

    #[test]
    fn the_switch_is_off_until_a_person_turns_it_on_and_the_refusal_names_it() {
        assert!(decide(&asker(), 3).is_ok());
        let off = decide(
            &Asker {
                trusted: false,
                ..asker()
            },
            1,
        )
        .unwrap_err();
        assert!(off.contains("High trust agentic is off"), "{off}");
        assert!(
            off.contains("Sharing & access"),
            "it says where the switch is: {off}"
        );
        // The window's own MCP switch comes first.
        assert_eq!(
            decide(
                &Asker {
                    mcp: false,
                    trusted: false,
                    ..asker()
                },
                1
            )
            .unwrap_err(),
            "MCP exposure is disabled."
        );
        assert!(decide(
            &Asker {
                is_agent: false,
                ..asker()
            },
            1
        )
        .unwrap_err()
        .contains("not running one"));
    }

    /// Promotion has one writer, and it is the function a person's click
    /// reaches from the pane's menu and from its card on the Floor. A second
    /// `insert` anywhere is a second way to become an orchestrator.
    #[test]
    fn promotion_has_one_writer_and_no_verb_or_socket_command_reaches_it() {
        let floor = include_str!("threshing_floor_view.rs");
        let code = |src: &'static str| src.split("\n#[cfg(test)]").next().unwrap_or(src);
        let mut inserts = 0;
        for src in [
            include_str!("main.rs"),
            include_str!("mcp.rs"),
            include_str!("ctl.rs"),
            include_str!("team_view.rs"),
            include_str!("pane.rs"),
            floor,
        ] {
            inserts += code(src).matches("orchestrators.insert(").count();
        }
        assert_eq!(inserts, 1, "exactly one place promotes a pane");
        let at = floor.find("fn team_promote(").expect("team_promote");
        let body = &floor[at..at + floor[at..].find("\n    }\n").expect("its end")];
        assert!(
            body.contains("orchestrators.insert("),
            "and it is team_promote"
        );
        // Nothing an agent or a socket can call names it.
        for src in [include_str!("mcp.rs"), include_str!("ctl.rs")] {
            assert!(
                !code(src).contains("team_promote"),
                "a verb or command promotes"
            );
        }
    }

    #[test]
    fn only_a_pane_a_person_promoted_may_start_a_team() {
        // The switch is on and the caller is an agent a person started: still no.
        let plain = decide(
            &Asker {
                promoted: false,
                ..asker()
            },
            1,
        )
        .unwrap_err();
        assert!(
            plain.contains("not been promoted to orchestrator"),
            "{plain}"
        );
        assert!(
            plain.contains("right-click its card"),
            "it says how: {plain}"
        );
        // A promoted team member may: a person put it there on purpose.
        assert!(decide(
            &Asker {
                is_member: true,
                ..asker()
            },
            1
        )
        .is_ok());
    }

    #[test]
    fn an_agent_another_agent_started_cannot_start_a_team() {
        let why = decide(
            &Asker {
                promoted: false,
                is_member: true,
                ..asker()
            },
            1,
        )
        .unwrap_err();
        assert!(why.contains("cannot start a team of its own"), "{why}");
        // And the record is what says so, by seat, so it holds after a restart.
        let mut teams = Teams::default();
        teams.add(Record {
            id: 1,
            name: "t".into(),
            by: SeatId(10),
            members: vec![SeatId(11), SeatId(12)],
            started_ms: 0,
        });
        assert!(teams.is_member(SeatId(11)));
        assert!(
            !teams.is_member(SeatId(10)),
            "the coordinator is not a member"
        );
        let back: Vec<Record> = toml::from_str::<std::collections::HashMap<String, Vec<Record>>>(
            &toml::to_string(&std::collections::HashMap::from([("teams", teams.saved())])).unwrap(),
        )
        .unwrap()
        .remove("teams")
        .unwrap();
        assert_eq!(Teams::from_saved(back), teams);
    }

    #[test]
    fn the_count_of_agent_started_agents_is_held_and_closing_them_frees_room() {
        let full = Asker {
            live: MAX_LIVE - 1,
            ..asker()
        };
        assert!(decide(&full, 1).is_ok(), "the last place");
        let over = decide(&full, 2).unwrap_err();
        assert!(over.contains("the most at once is 8"), "{over}");
        let mut teams = Teams::default();
        teams.add(Record {
            id: 1,
            name: "a".into(),
            by: SeatId(1),
            members: vec![SeatId(2), SeatId(3)],
            started_ms: 0,
        });
        teams.add(Record {
            id: 2,
            name: "b".into(),
            by: SeatId(1),
            members: vec![SeatId(4)],
            started_ms: 0,
        });
        assert_eq!(teams.member_seats().count(), 3);
        // Seat 3's pane and all of team b's are closed.
        teams.retain_seats(&|s| s == SeatId(2));
        assert_eq!(teams.member_seats().collect::<Vec<_>>(), vec![SeatId(2)]);
        assert_eq!(
            teams.saved().len(),
            1,
            "a team with nobody left is forgotten"
        );
    }
}
