//! The Threshing Floor across machines, as the window runs it.
//!
//! The rules and the messages are in `farfloor`. This file is the window's two
//! halves of them. Receiving: another machine asks who is here and hands over
//! prompts, and each answer is cut to the ceiling a person here set for that
//! machine. Sending: an agent here names an agent on another machine, the
//! line a person drew is checked, and the call goes out over ssh from a
//! background thread while the window carries on.
//!
//! Nothing here types into a far terminal. A prompt for a far agent is handed
//! to that machine's own window, which types it into its own pane by its own
//! rules (`prompt`), at a recognised rest, once.

use super::*;
use crate::farfloor::{self, FarAgent, FarStage, MachineCensus};

/// What this window knows of one other machine's agents.
#[derive(Clone, Debug)]
pub(crate) enum MachineView {
    /// Asked, not yet answered, and never answered before.
    Reading,
    Read {
        at: Instant,
        census: MachineCensus,
    },
    /// It could not be asked. `last` is the last good answer, kept so that a
    /// line can still be read while the machine is away.
    Trouble {
        at: Instant,
        why: String,
        last: Option<MachineCensus>,
    },
}

impl MachineView {
    pub(crate) fn census(&self) -> Option<&MachineCensus> {
        match self {
            MachineView::Read { census, .. } => Some(census),
            MachineView::Trouble { last, .. } => last.as_ref(),
            MachineView::Reading => None,
        }
    }

    /// When a good answer last arrived. `None`: never.
    fn heard(&self) -> Option<Instant> {
        match self {
            MachineView::Read { at, .. } => Some(*at),
            _ => None,
        }
    }

    fn asked(&self) -> Option<Instant> {
        match self {
            MachineView::Read { at, .. } | MachineView::Trouble { at, .. } => Some(*at),
            MachineView::Reading => None,
        }
    }
}

/// The window's state for other machines.
#[derive(Default)]
pub(crate) struct FarFloor {
    pub(crate) lines: farfloor::FarLines,
    /// The machines in `machines.toml`, as last read.
    pub(crate) machines: Vec<entry::Machine>,
    pub(crate) machines_trouble: Option<String>,
    pub(crate) census: std::collections::HashMap<String, MachineView>,
    /// Machines being asked right now, so one is never asked twice at once.
    pub(crate) asking: std::collections::HashSet<String>,
    pub(crate) book: farfloor::FarBook,
    /// Which machine each far sender's prompts are filed under: the name
    /// behind [`far_sender_pane`]'s number. Lost when the window closes, and
    /// a prompt whose sender can no longer be named is not typed.
    pub(crate) senders: std::collections::HashMap<u64, String>,
    /// Machines that asked who is here and were shown nothing, since this
    /// window opened: what Settings offers a person to decide about.
    pub(crate) knocked: Vec<String>,
    /// The ceilings in `sharing.toml`, as last read: what Settings draws.
    pub(crate) sharing: farfloor::Sharing,
    pub(crate) sharing_trouble: Option<String>,
    /// Each machine's own account of itself, for Settings.
    pub(crate) probes: std::collections::HashMap<String, ProbeView>,
    pub(crate) probing: std::collections::HashSet<String>,
    /// The machine a window was just asked to open on, and when: the page
    /// says so for a few seconds, since the window itself takes a moment.
    pub(crate) opening: Option<(String, Instant)>,
    /// The local seat whose far lines the section edits.
    pub(crate) from: Option<threshing_floor::SeatId>,
}

/// How long a machine's answer is good for while the Floor is open on it.
const CENSUS_OPEN_SECS: u64 = 8;
/// And while only a line or an open prompt needs it.
const CENSUS_IDLE_SECS: u64 = 30;
/// How often an open far prompt is asked after.
const STATUS_SECS: u64 = 3;

/// The pane number a far machine's prompts are filed under in the prompt
/// book, which keys every sender by a pane. Far above any pane a session host
/// mints, and one per machine, so "one prompt at a time from this sender to
/// that worker" holds per machine as it holds per pane.
pub(crate) fn far_sender_pane(host: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    host.to_ascii_lowercase().hash(&mut h);
    FAR_SENDER_BASE | (h.finish() & 0x0000_ffff_ffff_ffff)
}

pub(crate) const FAR_SENDER_BASE: u64 = 1 << 62;

/// A moment old enough that any answer stamped with it is due again. Never
/// before the clock's own start, which subtracting from `now` could ask for.
fn long_ago() -> Instant {
    let now = Instant::now();
    now.checked_sub(Duration::from_secs(CENSUS_IDLE_SECS + 1))
        .unwrap_or(now)
}

/// Whether a prompt book's sender is another machine rather than a pane here.
pub(crate) fn is_far_sender(pane: u64) -> bool {
    pane & FAR_SENDER_BASE != 0
}

fn far_ok(text: String, structured: serde_json::Value) -> mcp::AskReply {
    mcp::AskReply::Said(mcp::tool_ok(text, structured))
}

fn far_no(text: &str) -> mcp::AskReply {
    mcp::AskReply::Said(mcp::tool_err(text))
}

/// A far stage in one sentence, for the agent that sent the prompt.
fn far_stage_words(id: u64, machine: &str, stage: &FarStage) -> String {
    let later = format!("Read it with prompt_status {{\"prompt_id\": {id}, \"wait_seconds\": 6}}.");
    match stage {
        FarStage::Sending => format!("prompt {id}: on its way to {machine}. {later}"),
        FarStage::Held { far } => {
            format!("prompt {id}: {machine} has it; its window says {far}. {later}")
        }
        FarStage::Finished {
            reply: Some(reply), ..
        } => format!("prompt {id}: the agent on {machine} finished. Its last message:\n\n{reply}"),
        FarStage::Finished {
            reply: None,
            unknown,
        } => format!(
            "prompt {id}: the agent on {machine} finished; its last message is not known ({}).",
            unknown.as_deref().unwrap_or("not read")
        ),
        FarStage::Failed { why } => {
            format!("prompt {id}: it did not go through on {machine}: {why}")
        }
    }
}

impl Workspace {
    // ---- receiving: another machine asks ----

    /// A call that crossed machines, answered. See [`mcp::FarCall`].
    pub(crate) fn far_bridge(
        &mut self,
        call: &mcp::FarCall,
        cx: &mut Context<Self>,
    ) -> mcp::AskReply {
        match call {
            mcp::FarCall::Census { from } => self.far_census_in(from, cx),
            mcp::FarCall::PromptIn { target, prompt } => self.far_prompt_in(*target, prompt, cx),
            mcp::FarCall::PromptInStatus { from, prompt_id } => {
                match self.prompts.status_of_sender(
                    threshing_floor::AskId(*prompt_id),
                    far_sender_pane(from),
                    crate::prompt::now_ms(),
                ) {
                    Some(status) => mcp::AskReply::Said(mcp::prompt_said(&status)),
                    None => far_no("no prompt by that number came from your machine."),
                }
            }
            mcp::FarCall::AskOut {
                caller_pane,
                machine,
                to,
            } => self.far_ask_out(*caller_pane, machine, to, cx),
            mcp::FarCall::PromptOut {
                caller_pane,
                machine,
                to,
                brief,
                key,
            } => self.far_prompt_out(*caller_pane, machine, to, brief, key.clone(), cx),
        }
    }

    /// The ceiling a person here set for `from`, read from the file each time
    /// so that a change in Settings, in this window or another, holds from the
    /// next call. A file that cannot be read shares with nobody.
    fn far_ceiling(&self, from: &str) -> Option<threshing_floor::Rung> {
        farfloor::Sharing::load(&instance::config_dir())
            .ok()
            .and_then(|s| s.ceiling(from))
    }

    /// Who another machine may see here.
    fn far_census_in(&mut self, from: &str, cx: &mut Context<Self>) -> mcp::AskReply {
        let ceiling = self.far_ceiling(from);
        if ceiling.is_none() && !self.far_floor.knocked.iter().any(|k| k == from) {
            // Remembered so Settings can ask a person about it, and capped so
            // a caller inventing names cannot grow the list without end.
            if self.far_floor.knocked.len() < 16 {
                self.far_floor.knocked.push(from.to_string());
                cx.notify();
            }
        }
        let mut agents = Vec::new();
        for tab in &self.tabs {
            let mut leaves = vec![];
            tab.root.leaves(&mut leaves);
            for leaf in leaves {
                let t = leaf.read(cx);
                let party = crate::threshing_floor_view::prompt_party(t);
                if !party.is_agent {
                    continue;
                }
                let reach = farfloor::inbound_reach(&self.mcp, ceiling, &party);
                if reach < threshing_floor::Reach::Seen {
                    continue;
                }
                let Some(pid) = t.shell_pid() else {
                    continue;
                };
                let vit = self.agent_vitals.get(&pid);
                let may_read = reach >= threshing_floor::Reach::Ask;
                let response = t
                    .bench
                    .latest_response()
                    .filter(|_| may_read && t.bench_is_its_own());
                agents.push(FarAgent {
                    // Traded for a conversation or a seat by the MCP layer,
                    // which is where a conversation is proven.
                    addr: format!("pid:{pid}"),
                    also: None,
                    name: self.pane_name(leaf, cx).full(),
                    harness: t.mode.label().to_string(),
                    // English: this is a message between machines, not a card.
                    state: t.agent_status().state.label_in(lang::Lang::En).to_string(),
                    model: vit.and_then(|v| v.model.clone()),
                    size: vit.and_then(|v| v.tokens),
                    reach,
                    note: t.saved_note().filter(|_| may_read).map(|n| match n.title {
                        Some(title) if !n.text.is_empty() => format!("{title} — {}", n.text),
                        Some(title) => title,
                        None => n.text,
                    }),
                    brief: response.and_then(|r| r.brief.clone()),
                    layman: response.map(|r| r.layman.clone()),
                });
            }
        }
        let census = farfloor::Census {
            machine: farfloor::machine_name(),
            session: instance::key().to_string(),
            window: std::process::id(),
            mcp: self.mcp.enabled,
            ceiling,
            agents,
            unnamed: 0,
        };
        let text = format!(
            "{} agent(s) on {} that {from} may see",
            census.agents.len(),
            census.machine
        );
        far_ok(text, serde_json::to_value(&census).unwrap_or_default())
    }

    /// A prompt another machine handed over, decided against that machine's
    /// ceiling and the worker's own position, then given to the prompt book,
    /// which types it only at a recognised rest.
    fn far_prompt_in(
        &mut self,
        target_pid: u32,
        p: &farfloor::PromptIn,
        cx: &mut Context<Self>,
    ) -> mcp::AskReply {
        let now = crate::prompt::now_ms();
        let sender_pane = far_sender_pane(&p.from);
        if let Some(key) = p.key.as_deref() {
            if let Some(status) = self.prompts.status_for_key(sender_pane, key, now) {
                return mcp::AskReply::Said(mcp::prompt_said(&status));
            }
        }
        let brief = crate::prompt::clean(&p.prompt);
        let brief_len = brief.chars().count();
        let Some(target) = self.leaf_by_shell_pid(target_pid, cx) else {
            return far_no("that agent is no longer in this window.");
        };
        let (party, to_seat, program, returns) = {
            let tv = target.read(cx);
            (
                crate::threshing_floor_view::prompt_party(tv),
                tv.seat,
                tv.mode.label().to_string(),
                askcopy::Program::of_mode(tv.mode.label()).is_some(),
            )
        };
        self.ask_seq = (self.ask_seq + 1).max(now);
        let id = threshing_floor::AskId(self.ask_seq);
        let mut entry = askrecord::Entry {
            id,
            at_ms: now,
            from: None,
            to: to_seat,
            via: format!("far:{}", p.from),
            stage: "refused".into(),
            refusal: None,
            question_len: brief_len,
            question_sha256: askrecord::fingerprint(&brief),
            program: Some(program),
            model: None,
            reads_tokens: None,
            tokens_read: None,
            cost_usd: None,
        };
        let ceiling = self.far_ceiling(&p.from);
        let reach = farfloor::inbound_reach(&self.mcp, ceiling, &party);
        let refusal = if brief_len == 0 || brief_len > threshing_floor::PROMPT_MAX_CHARS {
            Some((
                "bad_brief",
                "the prompt is empty or longer than 32,000 characters.",
            ))
        } else if !self.mcp.enabled {
            Some(("mcp_off", "this window's MCP switch is off."))
        } else if !party.is_agent {
            Some(("not_an_agent", "that pane is not an agent."))
        } else if reach < threshing_floor::Reach::Prompt {
            Some((
                "no_far_prompt",
                "this machine does not let yours prompt that agent. A person here sets how far \
                 your machine reaches in Settings, under Machines; a read-only or off-limits \
                 pane is never prompted.",
            ))
        } else {
            None
        };
        if let Some((code, why)) = refusal {
            entry.refusal = Some(code.into());
            self.record(&entry);
            return far_no(why);
        }
        self.far_floor.senders.insert(sender_pane, p.from.clone());
        let sender = format!("{} (on {})", crate::prompt::clean(&p.sender), p.from);
        let header = crate::prompt::header_far(id, &sender, returns);
        let taken = self.prompts.send_durable(
            crate::prompt::NewPrompt {
                id,
                sender_pane,
                from: None,
                to: to_seat,
                target_pid,
                key: p.key.clone(),
                text: crate::prompt::compose(&header, &brief),
                brief_len,
                brief_sha256: entry.question_sha256.clone(),
                now_ms: now,
            },
            &self.prompt_key_path,
        );
        match taken {
            Ok(crate::prompt::Taken::New(status)) => {
                entry.stage = "waiting".into();
                self.record(&entry);
                eprintln!(
                    "terminal-delight: took prompt {} from {} for pane pid {target_pid}",
                    id.0, p.from
                );
                cx.notify();
                mcp::AskReply::Said(mcp::prompt_said(&status))
            }
            Ok(crate::prompt::Taken::Again(status)) => {
                mcp::AskReply::Said(mcp::prompt_said(&status))
            }
            Err(r) => {
                entry.refusal = serde_json::to_value(&r)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string));
                self.record(&entry);
                far_no(r.why())
            }
        }
    }

    /// Whether a prompt another machine handed over may still be typed: that
    /// machine's ceiling and the worker's position, as they stand now.
    pub(crate) fn far_inbound_allowed(
        &self,
        sender_pane: u64,
        worker: &threshing_floor::Party,
    ) -> Result<(), threshing_floor::Refusal> {
        let Some(from) = self.far_floor.senders.get(&sender_pane) else {
            return Err(threshing_floor::Refusal::Gone);
        };
        if !self.mcp.enabled {
            return Err(threshing_floor::Refusal::McpOff);
        }
        if !worker.is_agent {
            return Err(threshing_floor::Refusal::NotAnAgent);
        }
        if farfloor::inbound_reach(&self.mcp, self.far_ceiling(from), worker)
            < threshing_floor::Reach::Prompt
        {
            return Err(threshing_floor::Refusal::NoPromptLine);
        }
        Ok(())
    }

    // ---- sending: an agent here names a far agent ----

    /// The seat of the pane a call came from.
    fn far_caller_seat(&self, caller_pane: u64, cx: &App) -> Option<threshing_floor::SeatId> {
        self.leaf_by_pane_id(caller_pane, cx)
            .and_then(|l| l.read(cx).seat)
    }

    /// How far `seat` reaches a far agent, and that agent as last reported.
    fn far_reach_out(
        &self,
        seat: Option<threshing_floor::SeatId>,
        machine: &str,
        to: &str,
    ) -> (
        threshing_floor::Reach,
        Option<(&farfloor::Census, &FarAgent)>,
    ) {
        let found = self
            .far_floor
            .census
            .get(machine)
            .and_then(MachineView::census)
            .and_then(|c| c.find(to));
        // The line may have been drawn to either of the agent's names: its
        // seat, saved before its conversation was known, or its conversation.
        let names = [
            Some(to),
            found.map(|(_, a)| a.addr.as_str()),
            found.and_then(|(_, a)| a.also.as_deref()),
        ];
        let line = seat.and_then(|s| {
            names
                .into_iter()
                .flatten()
                .find_map(|n| self.far_floor.lines.rung(s, machine, n))
        });
        (
            farfloor::outbound_reach(line, found.map(|(_, a)| a.reach)),
            found,
        )
    }

    /// Why a seat here does not reach a far agent far enough, in a sentence
    /// that names which side to change.
    fn far_refusal_out(
        &self,
        seat: Option<threshing_floor::SeatId>,
        machine: &str,
        to: &str,
        want: threshing_floor::Rung,
    ) -> String {
        let word = format!("{want:?}").to_lowercase();
        if !self.far_floor.machines.iter().any(|m| m.name == machine) {
            return format!("no machine named {machine:?} is in this window's machines.toml.");
        }
        let Some(seat) = seat else {
            return "your pane is not seated on the Threshing Floor, so no line leaves it.".into();
        };
        let line = self.far_floor.lines.rung(seat, machine, to);
        if line.is_none_or(|r| r < want) {
            return format!(
                "no line from your seat reaches that agent on {machine} as far as {word}. A person \
                 draws it in the Threshing Floor here, under Other machines."
            );
        }
        match self.far_floor.census.get(machine) {
            None | Some(MachineView::Reading) => {
                format!("{machine} has not answered yet. Try again in a few seconds.")
            }
            Some(MachineView::Trouble { why, .. }) => format!("{machine} cannot be reached: {why}"),
            Some(MachineView::Read { census, .. }) => match census.find(to) {
                None => format!(
                    "{machine} did not list that agent when it last answered. It may have \
                     ended, or {machine} may have stopped sharing it. {machine} is being asked \
                     again now; try once more in a few seconds."
                ),
                Some(_) => format!(
                    "{machine} does not let this machine {word} that agent. A person there sets \
                     how far this machine reaches, in its Settings under Machines."
                ),
            },
        }
    }

    /// `ask_agent` for an agent on another machine: answered from what that
    /// machine last reported, with how old the report is.
    fn far_ask_out(
        &mut self,
        caller_pane: u64,
        machine: &str,
        to: &str,
        cx: &mut Context<Self>,
    ) -> mcp::AskReply {
        let seat = self.far_caller_seat(caller_pane, cx);
        let (reach, found) = self.far_reach_out(seat, machine, to);
        let Some((_, agent)) = found.filter(|_| reach >= threshing_floor::Reach::Ask) else {
            let why = self.far_refusal_out(seat, machine, to, threshing_floor::Rung::Ask);
            self.far_mark_stale(machine);
            return far_no(&why);
        };
        let heard = self
            .far_floor
            .census
            .get(machine)
            .and_then(MachineView::heard)
            .map(|at| at.elapsed().as_secs());
        let mut text = format!(
            "{} on {machine}: {} · {}",
            agent.name, agent.harness, agent.state
        );
        if let Some(model) = &agent.model {
            text.push_str(&format!(" · {model}"));
        }
        if let Some(size) = agent.size {
            text.push_str(&format!(" · {size} tokens"));
        }
        if let Some(note) = &agent.note {
            text.push_str(&format!("\nIts sticky note: {note}"));
        }
        match (&agent.brief, &agent.layman) {
            (Some(brief), _) => text.push_str(&format!("\nIts own latest summary: {brief}")),
            (None, Some(layman)) => text.push_str(&format!("\nIts own latest summary: {layman}")),
            (None, None) => {
                text.push_str("\nIt has written no summary that this machine may read.")
            }
        }
        text.push_str(&match heard {
            Some(s) => {
                format!("\n(As {machine} reported it {s}s ago. Nothing was sent to the agent.)")
            }
            None => format!("\n(From {machine}'s last answer; it is not answering now.)"),
        });
        far_ok(
            text,
            serde_json::json!({ "machine": machine, "agent": agent, "heard_s": heard }),
        )
    }

    /// `prompt_agent` for an agent on another machine. Decided here against
    /// the line a person drew, numbered here, and sent from a background
    /// thread: nothing about a network is waited on in this call.
    fn far_prompt_out(
        &mut self,
        caller_pane: u64,
        machine: &str,
        to: &str,
        brief: &str,
        key: Option<String>,
        cx: &mut Context<Self>,
    ) -> mcp::AskReply {
        let seat = self.far_caller_seat(caller_pane, cx);
        let brief = crate::prompt::clean(brief);
        let chars = brief.chars().count();
        if chars == 0 || chars > threshing_floor::PROMPT_MAX_CHARS {
            return far_no("the prompt is empty or longer than 32,000 characters.");
        }
        let (reach, found) = self.far_reach_out(seat, machine, to);
        let Some((census, _)) = found.filter(|_| reach >= threshing_floor::Reach::Prompt) else {
            let why = self.far_refusal_out(seat, machine, to, threshing_floor::Rung::Prompt);
            self.far_mark_stale(machine);
            return far_no(&why);
        };
        let far_window = census.window;
        // A retry with a key this agent already used is answered with the
        // first attempt, before anything else: the answer that carried its
        // number may be what was lost, and `busy` would only point back at it.
        let key = key.map(|k| farfloor::scoped_key(seat, caller_pane, &k));
        if let Some(first) = key
            .as_deref()
            .and_then(|k| self.far_floor.book.by_key(machine, k))
        {
            let mut data = serde_json::to_value(&first.stage).unwrap_or_default();
            data["prompt_id"] = first.id.into();
            data["machine"] = first.machine.clone().into();
            return far_ok(
                far_stage_words(first.id, &first.machine, &first.stage),
                data,
            );
        }
        if self.far_floor.book.busy(caller_pane, machine, to) {
            return far_no(
                "an earlier prompt from you to that agent has not finished. Read it with \
                 prompt_status first.",
            );
        }
        let Some(dest) = self
            .far_floor
            .machines
            .iter()
            .find(|m| m.name == machine)
            .and_then(|m| m.ssh.clone())
        else {
            return far_no(&format!(
                "no machine named {machine:?} is in machines.toml."
            ));
        };
        let now = crate::prompt::now_ms();
        self.ask_seq = (self.ask_seq + 1).max(now);
        let id = self.ask_seq;
        let sender = self
            .leaf_by_pane_id(caller_pane, cx)
            .map(|l| self.pane_name(&l, cx).full())
            .unwrap_or_default();
        self.far_floor.book.open(farfloor::FarSent {
            id,
            sender_pane: caller_pane,
            machine: machine.to_string(),
            to: to.to_string(),
            far: None,
            sent_ms: now,
            stage: FarStage::Sending,
            misses: 0,
            key: key.clone(),
        });
        // The far window keeps one key book per sending machine, so a key is
        // made this agent's own before it leaves: two agents here that both
        // say "review" are two assignments there, not one.
        let line = farfloor::request(
            farfloor::M_PROMPT,
            serde_json::json!({
                "from": farfloor::machine_name(),
                "sender": sender,
                "to": to,
                "prompt": brief,
                "key": key,
            }),
        );
        let window_arg = far_window.to_string();
        let machine_name = machine.to_string();
        cx.spawn(async move |this, cx| {
            let sent = cx
                .background_executor()
                .spawn(async move {
                    far::ask_with(
                        &dest,
                        &farfloor::rpc_script(),
                        &[&window_arg],
                        Some(line.as_bytes()),
                        far::ASK_BUDGET,
                    )
                })
                .await;
            let _ = this.update(cx, |ws: &mut Workspace, cx| {
                let stage = match sent {
                    Err(why) => (
                        None,
                        FarStage::Failed {
                            why: far::trouble_words(&machine_name, &machine_name, &why),
                        },
                    ),
                    Ok(out) => match out.lines().find_map(farfloor::parse_reply_line) {
                        Some((_, Ok(result))) => farfloor::stage_of(&result),
                        Some((_, Err(why))) => (None, FarStage::Failed { why }),
                        None => (
                            None,
                            FarStage::Failed {
                                why: "the far machine gave no answer this build can read".into(),
                            },
                        ),
                    },
                };
                if let Some(s) = ws.far_floor.book.get_mut(id) {
                    s.far = stage.0.map(|far_id| (far_window, far_id));
                    s.stage = stage.1;
                }
                cx.notify();
            });
        })
        .detach();
        far_ok(
            far_stage_words(id, machine, &FarStage::Sending),
            serde_json::json!({ "prompt_id": id, "machine": machine, "stage": "sending" }),
        )
    }

    /// A far prompt's progress, for the pane that sent it. `None`: this pane
    /// sent no far prompt by that number.
    pub(crate) fn far_prompt_status(&self, id: u64, caller_pane: u64) -> Option<mcp::AskReply> {
        let sent = self.far_floor.book.of_sender(id, caller_pane)?;
        let mut data = serde_json::to_value(&sent.stage).unwrap_or_default();
        data["prompt_id"] = id.into();
        data["machine"] = sent.machine.clone().into();
        Some(far_ok(
            far_stage_words(id, &sent.machine, &sent.stage),
            data,
        ))
    }

    /// Agents on other machines that seats here have lines to, for
    /// `list_panes`.
    pub(crate) fn far_seen(&self, _cx: &App) -> Vec<mcp::FarSeen> {
        self.far_floor
            .lines
            .saved()
            .iter()
            .filter_map(|line| {
                let rung = line.rung.known()?;
                let view = self.far_floor.census.get(&line.machine);
                let found = view
                    .and_then(MachineView::census)
                    .and_then(|c| c.find(&line.to))
                    .map(|(_, a)| a);
                Some(mcp::FarSeen {
                    from: line.from,
                    machine: line.machine.clone(),
                    agent: line.to.clone(),
                    name: found
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| line.name.clone()),
                    state: found.map(|a| a.state.clone()),
                    harness: found.map(|a| a.harness.clone()),
                    // Two different silences. A machine that has not been
                    // heard from says nothing about its agent, so the line is
                    // shown at its own rung with the state unknown, and the
                    // far machine has the last word when a call is made. A
                    // machine that has just answered and does not list the
                    // agent has said something: it is not shared now, and
                    // showing "you reach it" would be this window's guess
                    // against that machine's word.
                    reach: match (found, view) {
                        (Some(a), _) => farfloor::outbound_reach(Some(rung), Some(a.reach)),
                        (None, Some(MachineView::Read { .. })) => threshing_floor::Reach::Hidden,
                        (None, _) => threshing_floor::Reach::from(rung),
                    },
                    heard_s: view
                        .and_then(MachineView::heard)
                        .map(|at| at.elapsed().as_secs()),
                })
            })
            .collect()
    }

    // ---- the poller ----

    /// Keep what this window knows of other machines current, while anything
    /// needs it: the Floor is open, a far line is drawn, or a far prompt is
    /// open. A window none of those is true of asks nobody anything.
    pub(crate) fn watch_far_floor(window: &Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_secs(STATUS_SECS))
                .await;
            let Ok((asks, statuses)) = this.update(cx, |ws: &mut Workspace, cx| {
                (ws.far_due(cx), ws.far_open_prompts())
            }) else {
                break;
            };
            // Every machine is asked at once, and each answer is applied when
            // it lands: an unreachable machine costs its own wait, never the
            // others'. Prompt statuses start with the censuses rather than
            // behind them. The pass still ends when the last one is in, so a
            // machine is never asked twice at once.
            let mut statuses_in = Vec::new();
            for (id, dest, far_window, far_id) in statuses {
                let line = farfloor::request(
                    farfloor::M_STATUS,
                    serde_json::json!({ "from": farfloor::machine_name(), "prompt_id": far_id }),
                );
                let work = cx.background_executor().spawn(async move {
                    far::ask_with(
                        &dest,
                        &farfloor::rpc_script(),
                        &[&far_window.to_string()],
                        Some(line.as_bytes()),
                        far::ASK_BUDGET,
                    )
                });
                let this = this.clone();
                statuses_in.push(cx.spawn(async move |cx| {
                    let answer = work.await;
                    let _ = this.update(cx, |ws: &mut Workspace, cx| {
                        // The machine could not be reached at all: nothing is
                        // known, so nothing changes, and the next pass asks again.
                        let Ok(out) = answer else { return };
                        let changed = match out.lines().find_map(farfloor::parse_reply_line) {
                            Some((_, Ok(result))) => {
                                let (_, stage) = farfloor::stage_of(&result);
                                ws.far_floor.book.heard(id, stage)
                            }
                            // The machine answered and that window did not.
                            Some((_, Err(why))) => ws.far_floor.book.window_missed(id, &why),
                            None => ws.far_floor.book.window_missed(id, "no answer"),
                        };
                        if changed {
                            cx.notify();
                        }
                    });
                }));
            }
            let mut censuses = Vec::new();
            for (name, dest) in asks {
                let line = farfloor::request(
                    farfloor::M_CENSUS,
                    serde_json::json!({ "from": farfloor::machine_name() }),
                );
                let work = cx.background_executor().spawn({
                    let dest = dest.clone();
                    async move {
                        far::ask_with(
                            &dest,
                            &farfloor::rpc_script(),
                            &["all"],
                            Some(line.as_bytes()),
                            far::ASK_BUDGET,
                        )
                    }
                });
                let this = this.clone();
                censuses.push(cx.spawn(async move |cx| {
                    let answer = work.await;
                    let _ = this.update(cx, |ws: &mut Workspace, cx| {
                        ws.far_census_arrived(&name, &dest, answer);
                        cx.notify();
                    });
                }));
            }
            // Which machines' Settings to probe is read from what the
            // censuses said, so the probes wait for them, and only them.
            for c in censuses {
                c.await;
            }
            let Ok(probes) = this.update(cx, |ws: &mut Workspace, _| ws.far_probes_due()) else {
                break;
            };
            let mut probed = Vec::new();
            for machine in probes {
                let asked = machine.clone();
                let work = cx
                    .background_executor()
                    .spawn(async move { far::probe(&asked) });
                let this = this.clone();
                probed.push(cx.spawn(async move |cx| {
                    let result = work.await;
                    let _ = this.update(cx, |ws: &mut Workspace, cx| {
                        ws.far_probe_arrived(&machine.name, result);
                        cx.notify();
                    });
                }));
            }
            for p in probed {
                p.await;
            }
            for s in statuses_in {
                s.await;
            }
        })
        .detach();
    }

    /// Drop far lines whose seat nobody holds any more: not worn by a pane
    /// in this window and not in any org. A seat whose pane is merely gone
    /// keeps its lines, as it keeps its local ones.
    fn far_prune_lines(&mut self, cx: &App) {
        if self.far_floor.lines.is_empty() {
            return;
        }
        let mut worn = std::collections::HashSet::new();
        for tab in &self.tabs {
            let mut leaves = vec![];
            tab.root.leaves(&mut leaves);
            worn.extend(leaves.into_iter().filter_map(|l| l.read(cx).seat));
        }
        let gone: Vec<threshing_floor::SeatId> = self
            .far_floor
            .lines
            .saved()
            .iter()
            .map(|l| l.from)
            .filter(|s| !worn.contains(s) && !self.floor_orgs.has_membership(*s))
            .collect();
        for seat in gone {
            self.far_floor.lines.forget_seat(seat);
        }
    }

    /// Read `machines.toml` again. Cheap, and done on every pass that asks a
    /// machine anything, so a machine added in Settings is asked on the next.
    pub(crate) fn far_reload_machines(&mut self) {
        match entry::load_machines(&instance::config_dir()) {
            Ok(machines) => {
                self.far_floor.machines = machines;
                self.far_floor.machines_trouble = None;
            }
            Err(why) => {
                self.far_floor.machines.clear();
                self.far_floor.machines_trouble = Some(why);
            }
        }
        match farfloor::Sharing::load(&instance::config_dir()) {
            Ok(sharing) => {
                self.far_floor.sharing = sharing;
                self.far_floor.sharing_trouble = None;
            }
            Err(why) => {
                self.far_floor.sharing = Default::default();
                self.far_floor.sharing_trouble = Some(why);
            }
        }
    }

    /// The machines whose answer has gone stale and is wanted.
    fn far_due(&mut self, cx: &App) -> Vec<(String, String)> {
        let open = self.threshing_floor || self.far_machines_page_open();
        self.far_prune_lines(cx);
        let lined: Vec<String> = self
            .far_floor
            .lines
            .machines()
            .into_iter()
            .map(str::to_string)
            .chain(
                self.far_floor
                    .book
                    .open_ones()
                    .into_iter()
                    .map(|s| s.machine),
            )
            .collect();
        if !open && lined.is_empty() {
            return vec![];
        }
        self.far_reload_machines();
        let mut due = vec![];
        for m in self.far_floor.machines.clone() {
            let Some(dest) = m.ssh.clone() else { continue };
            let wanted_open = open;
            let wanted_idle = lined.contains(&m.name);
            if !wanted_open && !wanted_idle {
                continue;
            }
            let fresh_for = if wanted_open {
                CENSUS_OPEN_SECS
            } else {
                CENSUS_IDLE_SECS
            };
            let stale = self
                .far_floor
                .census
                .get(&m.name)
                .and_then(MachineView::asked)
                .is_none_or(|at| at.elapsed().as_secs() >= fresh_for);
            if stale && self.far_floor.asking.insert(m.name.clone()) {
                self.far_floor
                    .census
                    .entry(m.name.clone())
                    .or_insert(MachineView::Reading);
                due.push((m.name, dest));
            }
        }
        due
    }

    fn far_census_arrived(&mut self, name: &str, dest: &str, answer: Result<String, String>) {
        self.far_floor.asking.remove(name);
        let last = self
            .far_floor
            .census
            .get(name)
            .and_then(MachineView::census)
            .cloned();
        let parsed = answer
            .map_err(|e| far::trouble_words(name, dest, &e))
            .and_then(|out| farfloor::parse_census(&out).map_err(|e| format!("{name}: {e}")));
        let view = match parsed {
            Ok(census) => MachineView::Read {
                at: Instant::now(),
                census,
            },
            Err(why) => MachineView::Trouble {
                at: Instant::now(),
                why,
                last,
            },
        };
        self.far_floor.census.insert(name.to_string(), view);
    }

    /// The far prompts still open that the far machine has taken: what to ask
    /// after, and where.
    fn far_open_prompts(&self) -> Vec<(u64, String, u32, u64)> {
        self.far_floor
            .book
            .open_ones()
            .into_iter()
            .filter_map(|s| {
                let (far_window, far_id) = s.far?;
                let dest = self
                    .far_floor
                    .machines
                    .iter()
                    .find(|m| m.name == s.machine)
                    .and_then(|m| m.ssh.clone())?;
                Some((s.id, dest, far_window, far_id))
            })
            .collect()
    }

    /// A call just found one machine's answer wanting: ask it again on the
    /// next pass rather than when its answer would have aged out, so the
    /// agent that was refused for a stale reason is right a few seconds later.
    fn far_mark_stale(&mut self, machine: &str) {
        if let Some(MachineView::Read { at, .. } | MachineView::Trouble { at, .. }) =
            self.far_floor.census.get_mut(machine)
        {
            *at = long_ago();
        }
    }

    /// Ask a machine again now, whatever its answer's age: the refresh press.
    pub(crate) fn far_refresh(&mut self, cx: &mut Context<Self>) {
        for view in self.far_floor.census.values_mut() {
            if let MachineView::Read { at, .. } | MachineView::Trouble { at, .. } = view {
                *at = long_ago();
            }
        }
        cx.notify();
    }
}

// ---- the Floor's own section ----

/// A rung's word in the reader's language, as the Floor already says it.
fn rung_word(ui: &lang::Chrome, rung: threshing_floor::Rung) -> &'static str {
    match rung {
        threshing_floor::Rung::See => ui.floor_rung_see,
        threshing_floor::Rung::Read => ui.floor_rung_read,
        threshing_floor::Rung::Ask => ui.floor_rung_ask,
        threshing_floor::Rung::Prompt => ui.floor_rung_prompt,
    }
}

/// An age a person reads at a glance: seconds under two minutes, then minutes.
pub(crate) fn short_age(secs: u64) -> String {
    if secs < 120 {
        format!("{secs}s")
    } else {
        format!("{}m", secs / 60)
    }
}

/// What a machine's last answer comes to, for the line under its name.
enum MachineLine {
    Asking,
    Unreachable(String),
    OldBuild,
    McpOff,
    SharesNothing,
    Agents,
}

fn machine_line(view: Option<&MachineView>) -> MachineLine {
    match view {
        None | Some(MachineView::Reading) => MachineLine::Asking,
        Some(MachineView::Trouble { why, .. }) => MachineLine::Unreachable(why.clone()),
        Some(MachineView::Read { census, .. }) => {
            if census.shares() {
                MachineLine::Agents
            } else if census.windows.is_empty()
                && census
                    .troubles
                    .iter()
                    .any(|t| t.contains("method not found") || t.contains("unknown command"))
            {
                // Its windows answered, in the words of a build that does not
                // know the question.
                MachineLine::OldBuild
            } else if !census.windows.is_empty() && census.windows.iter().all(|w| !w.mcp) {
                MachineLine::McpOff
            } else {
                MachineLine::SharesNothing
            }
        }
    }
}

impl Workspace {
    /// The seated agents in this window, in tab order: where a far line may
    /// start.
    fn far_local_seats(&self, cx: &App) -> Vec<(threshing_floor::SeatId, String)> {
        let mut out = vec![];
        for tab in &self.tabs {
            let mut leaves = vec![];
            tab.root.leaves(&mut leaves);
            for leaf in leaves {
                let v = leaf.read(cx);
                if let Some(seat) = v.seat.filter(|_| v.party().is_agent) {
                    // The shortest word that tells this seat from its
                    // neighbours: the pane's own name when it has one (a team
                    // member's role), else its tab's. Two members of one team
                    // share a tab, and both read as the team's name otherwise.
                    let name = self.pane_name(leaf, cx);
                    let label = v
                        .name
                        .clone()
                        .filter(|n| !n.trim().is_empty())
                        .unwrap_or(name.tab);
                    out.push((seat, label));
                }
            }
        }
        out
    }

    /// The Threshing Floor's section for agents on other machines: each
    /// machine, who is there, and for the chosen seat here a chip per far
    /// agent that says how far the line reaches and moves it one rung a
    /// press. `None` on a window with no other machine listed.
    pub(crate) fn far_floor_section(
        &self,
        th: &theme::Theme,
        sk: &skin::Skin,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        if self.far_floor.machines.is_empty() && self.far_floor.machines_trouble.is_none() {
            return None;
        }
        let s = self.lang.strings().far;
        let ui = self.lang.strings().ui;
        let acc = th.accent;
        let seats = self.far_local_seats(cx);
        let from = self
            .far_floor
            .from
            .filter(|f| seats.iter().any(|(seat, _)| seat == f))
            .or_else(|| seats.first().map(|(seat, _)| *seat));
        let small = |text: String, ink: Hsla| {
            div()
                .text_size(sk.text_px(8.5))
                .text_color(ink)
                .child(SharedString::from(text))
        };
        let mut body = div().flex().flex_col().gap_2();
        if let Some(why) = &self.far_floor.machines_trouble {
            body = body.child(small(format!("machines.toml: {why}"), sk.ink.danger));
        }
        // Which seat's lines the chips below edit.
        if seats.is_empty() {
            body = body.child(small(s.floor_no_seat.to_string(), th.text.alpha(0.65)));
        } else {
            let mut row = div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_1()
                .child(small(s.floor_from.to_string(), th.text.alpha(0.6)));
            for (i, (seat, name)) in seats.iter().enumerate() {
                let lit = Some(*seat) == from;
                let seat = *seat;
                let label = name.clone();
                row = row.child(
                    div()
                        .id(("far-from", i))
                        .debug_selector(move || format!("far-from-{i}"))
                        .px_1p5()
                        .py_0p5()
                        .rounded(sk.radius())
                        .border(sk.box_border(1.))
                        .border_color(if lit { acc } else { th.text.alpha(0.18) })
                        .when(lit, |d| d.bg(acc.alpha(0.16)))
                        .text_size(sk.text_px(8.5))
                        .text_color(if lit { th.text } else { th.text.alpha(0.65) })
                        .cursor_pointer()
                        .child(SharedString::from(label))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.far_floor.from = Some(seat);
                                cx.notify();
                            }),
                        ),
                );
            }
            body = body.child(row);
        }
        let me = farfloor::machine_name();
        for (mi, m) in self.far_floor.machines.iter().enumerate() {
            let view = self.far_floor.census.get(&m.name);
            let badge = m.name.to_uppercase();
            let heard = view.and_then(|v| match v {
                MachineView::Read { at, .. } => Some(at.elapsed().as_secs()),
                _ => None,
            });
            let line = machine_line(view);
            let status = match &line {
                MachineLine::Asking => s.floor_asking.to_string(),
                MachineLine::Unreachable(_) => s.floor_unreachable.to_string(),
                _ => heard
                    .map(|h| s.floor_heard.replace("{age}", &short_age(h)))
                    .unwrap_or_default(),
            };
            let status_ink = match &line {
                MachineLine::Unreachable(_) => sk.ink.danger,
                _ => th.text.alpha(0.55),
            };
            let mut block = div()
                .debug_selector(move || format!("far-machine-{mi}"))
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .rounded(sk.radius())
                .border(sk.box_border(1.))
                .border_color(th.text.alpha(0.16))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_size(sk.text_px(9.5))
                                .text_color(acc)
                                .child(SharedString::from(format!("\u{21c4} {badge}"))),
                        )
                        .child(small(status, status_ink)),
                );
            let census = view.and_then(MachineView::census);
            match &line {
                MachineLine::Asking => {}
                MachineLine::Unreachable(why) => {
                    block = block.child(small(why.clone(), th.text.alpha(0.7)));
                }
                MachineLine::OldBuild => {
                    block = block.child(small(
                        s.floor_old_build.replace("{machine}", &badge),
                        sk.ink.warn,
                    ));
                }
                MachineLine::McpOff => {
                    block = block.child(small(
                        s.floor_mcp_off.replace("{machine}", &badge),
                        sk.ink.warn,
                    ));
                }
                MachineLine::SharesNothing => {
                    block = block.child(small(
                        s.floor_shares_nothing
                            .replace("{machine}", &badge)
                            .replace("{me}", &me),
                        th.text.alpha(0.75),
                    ));
                }
                MachineLine::Agents => {}
            }
            // The agents, from the last good answer even while the machine is
            // away, so a line already drawn can still be read and erased.
            let agents: Vec<FarAgent> = census
                .map(|c| c.agents().map(|(_, a)| a.clone()).collect())
                .unwrap_or_default();
            if matches!(line, MachineLine::Agents) && agents.is_empty() {
                block = block.child(small(s.floor_no_agents.to_string(), th.text.alpha(0.65)));
            }
            // Lines to agents that machine no longer lists stay on screen
            // too: a line nobody can see is a line nobody can erase.
            let mut rows: Vec<(String, String, Option<FarAgent>)> = agents
                .iter()
                .map(|a| (a.addr.clone(), a.name.clone(), Some(a.clone())))
                .collect();
            if let Some(from) = from {
                for l in self.far_floor.lines.lines_of(from) {
                    if l.machine == m.name && !rows.iter().any(|(addr, _, _)| *addr == l.to) {
                        rows.push((l.to.clone(), l.name.clone(), None));
                    }
                }
            }
            for (ai, (addr, name, agent)) in rows.into_iter().enumerate() {
                let rung = from.and_then(|f| self.far_floor.lines.rung(f, &m.name, &addr));
                let allowed = agent.as_ref().map(|a| a.reach);
                let capped = match (rung, allowed) {
                    (Some(r), Some(allowed)) if threshing_floor::Reach::from(r) > allowed => {
                        Some(allowed)
                    }
                    _ => None,
                };
                let chip_text = match rung {
                    Some(r) => ui.floor_rel_wire.replace("{rung}", rung_word(ui, r)),
                    None => s.floor_no_line.to_string(),
                };
                let lit = rung.is_some();
                let ink = if capped.is_some() { sk.ink.warn } else { acc };
                let (machine, to, label) = (m.name.clone(), addr.clone(), name.clone());
                let facts = match &agent {
                    Some(a) => format!("{} \u{b7} {}", a.harness, a.state),
                    None => s.floor_unreachable.to_string(),
                };
                let mut entry = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w(px(0.))
                            .flex_1()
                            .child(
                                div()
                                    .text_size(sk.text_px(9.5))
                                    .text_color(th.text)
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(SharedString::from(name.clone())),
                            )
                            .child(small(facts, th.text.alpha(0.55))),
                    );
                if let Some(from) = from {
                    entry = entry.child(
                        div()
                            .id(("far-line", mi * 1000 + ai))
                            .debug_selector(move || format!("far-line-{mi}-{ai}"))
                            .flex_none()
                            .px_2()
                            .py_0p5()
                            .rounded(sk.radius())
                            .border(sk.box_border(1.))
                            .border_color(if lit { ink } else { th.text.alpha(0.22) })
                            .when(lit, |d| d.bg(ink.alpha(0.16)))
                            .text_size(sk.text_px(8.5))
                            .text_color(if lit { th.text } else { th.text.alpha(0.6) })
                            .cursor_pointer()
                            .child(SharedString::from(chip_text))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    ws.far_floor.lines.cycle(from, &machine, &to, &label);
                                    ws.save(cx);
                                    cx.notify();
                                }),
                            ),
                    );
                }
                block = block.child(entry);
                if let Some(allowed) = capped {
                    let word = match allowed {
                        threshing_floor::Reach::Seen => ui.floor_rung_see,
                        threshing_floor::Reach::Read => ui.floor_rung_read,
                        threshing_floor::Reach::Ask => ui.floor_rung_ask,
                        threshing_floor::Reach::Prompt => ui.floor_rung_prompt,
                        threshing_floor::Reach::Hidden => s.st_nothing,
                    };
                    block = block.child(small(
                        s.floor_allows
                            .replace("{machine}", &badge)
                            .replace("{rung}", word),
                        sk.ink.warn,
                    ));
                }
            }
            let unnamed: u64 = census
                .map(|c| c.windows.iter().map(|w| w.unnamed).sum())
                .unwrap_or(0);
            if unnamed > 0 {
                block = block.child(small(
                    s.floor_unnamed
                        .replace("{count}", &unnamed.to_string())
                        .replace("{machine}", &badge),
                    th.text.alpha(0.6),
                ));
            }
            body = body.child(block);
        }
        Some(
            div()
                .id("far-floor")
                .debug_selector(|| "far-floor".to_string())
                .flex_none()
                .max_h(px(360.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_2()
                .pt_2()
                .border_t_1()
                .border_color(th.text.alpha(0.14))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(th.text.alpha(0.85))
                                .child(SharedString::from(format!(
                                    "{} \u{b7} {}",
                                    s.floor_title,
                                    self.far_floor.machines.len()
                                ))),
                        )
                        .child(with_tip(
                            div()
                                .id("far-refresh")
                                .debug_selector(|| "far-refresh".to_string())
                                .px_1p5()
                                .rounded(sk.radius())
                                .border(sk.box_border(1.))
                                .border_color(th.text.alpha(0.22))
                                .text_color(th.text.alpha(0.75))
                                .cursor_pointer()
                                .hover(move |st| st.bg(acc.alpha(0.14)))
                                .child("\u{21bb}")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                        cx.stop_propagation();
                                        ws.far_refresh(cx);
                                    }),
                                ),
                            vec![s.floor_refresh.to_string()],
                        )),
                )
                .child(body),
        )
    }
}

// ---- Settings › Machines ----

/// What a machine said about itself when Settings last asked.
#[derive(Clone, Debug)]
pub(crate) enum ProbeView {
    Asking,
    Done {
        at: Instant,
        result: Result<far::Probe, String>,
    },
}

/// How long a machine's own account of itself is good for while Settings is
/// open on it.
const PROBE_SECS: u64 = 15;

impl Workspace {
    /// Whether Settings is open on its Machines page, which wants the same
    /// fresh answers the Floor does.
    pub(crate) fn far_machines_page_open(&self) -> bool {
        self.settings
            .as_ref()
            .is_some_and(|b| b.page == settings::Page::Environments)
    }

    /// The machines whose own account has gone stale while Settings shows
    /// them.
    pub(crate) fn far_probes_due(&mut self) -> Vec<entry::Machine> {
        if !self.far_machines_page_open() {
            return vec![];
        }
        let mut due = vec![];
        for m in self.far_floor.machines.clone() {
            let stale = match self.far_floor.probes.get(&m.name) {
                None => true,
                Some(ProbeView::Asking) => false,
                Some(ProbeView::Done { at, .. }) => at.elapsed().as_secs() >= PROBE_SECS,
            };
            if stale && self.far_floor.probing.insert(m.name.clone()) {
                self.far_floor
                    .probes
                    .entry(m.name.clone())
                    .or_insert(ProbeView::Asking);
                due.push(m);
            }
        }
        due
    }

    pub(crate) fn far_probe_arrived(&mut self, name: &str, result: Result<far::Probe, String>) {
        self.far_floor.probing.remove(name);
        self.far_floor.probes.insert(
            name.to_string(),
            ProbeView::Done {
                at: Instant::now(),
                result,
            },
        );
    }

    /// Open a far session in a window of its own, the way `terminal-delight
    /// remote` does from a terminal. The new window is its own process: this
    /// one neither waits for it nor owns it. If the machine does not answer,
    /// that process says so on the desktop.
    fn far_open_here(&mut self, name: &str, session: &str, cx: &mut Context<Self>) {
        use std::os::unix::process::CommandExt;
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let mut cmd = std::process::Command::new(exe);
        cmd.args(["remote", name, session])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        // What names THIS window's panes must not ride into another window.
        for var in [
            "TD_SESSION",
            "TD_PANE_ID",
            "TD_TAG",
            "TD_SCRATCH",
            "TD_SEED_CWD",
            "TD_SEED_RESUME",
            "TD_DEMO_STATE",
            far::ENV_NAME,
            far::ENV_DEST,
            far::ENV_SESSION,
        ] {
            cmd.env_remove(var);
        }
        // Its own session, so closing this window does not take it along.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        match cmd.spawn() {
            Ok(_) => {
                self.far_floor.opening = Some((name.to_uppercase(), Instant::now()));
            }
            Err(err) => eprintln!("terminal-delight: could not open {name}: {err}"),
        }
        cx.notify();
    }

    /// Set how far `host`'s agents may reach into this machine, and write it.
    fn far_set_sharing(
        &mut self,
        host: &str,
        reach: Option<threshing_floor::Rung>,
        cx: &mut Context<Self>,
    ) {
        let config = instance::config_dir();
        // Read, change and save under the one lock every window takes: a
        // window holding an older copy must not write back a permission
        // another window has just revoked.
        match farfloor::Sharing::update(&config, host, reach) {
            Ok(sharing) => {
                self.far_floor.sharing_trouble = None;
                self.far_floor.sharing = sharing;
            }
            Err(why) => self.far_floor.sharing_trouble = Some(why),
        }
        self.far_floor
            .knocked
            .retain(|k| !k.eq_ignore_ascii_case(host) || reach.is_none());
        cx.notify();
    }

    /// The five chips that say how far one machine reaches into this one.
    fn far_sharing_chips(
        &self,
        th: &theme::Theme,
        sk: &skin::Skin,
        key: usize,
        host: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = self.lang.strings().far;
        let ui = self.lang.strings().ui;
        let now = self.far_floor.sharing.ceiling(host);
        let acc = th.accent;
        let choices: [(Option<threshing_floor::Rung>, &str); 5] = [
            (None, s.st_nothing),
            (Some(threshing_floor::Rung::See), ui.floor_rung_see),
            (Some(threshing_floor::Rung::Read), ui.floor_rung_read),
            (Some(threshing_floor::Rung::Ask), ui.floor_rung_ask),
            (Some(threshing_floor::Rung::Prompt), ui.floor_rung_prompt),
        ];
        let mut row = div().flex().flex_row().gap_1();
        for (i, (reach, label)) in choices.into_iter().enumerate() {
            let lit = reach == now;
            let host = host.to_string();
            row = row.child(
                div()
                    .id(("far-share", key * 10 + i))
                    .debug_selector(move || format!("far-share-{key}-{i}"))
                    .px_2()
                    .py_0p5()
                    .rounded(sk.radius())
                    .border(sk.box_border(1.))
                    .border_color(if lit { acc } else { th.text.alpha(0.18) })
                    .when(lit, |d| d.bg(acc.alpha(0.16)))
                    .text_size(sk.text_px(9.))
                    .text_color(if lit { th.text } else { th.text.alpha(0.65) })
                    .cursor_pointer()
                    .child(SharedString::from(label.to_string()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.far_set_sharing(&host, reach, cx);
                        }),
                    ),
            );
        }
        row.into_any_element()
    }

    /// The Machines page's rows for each machine, and the sections under
    /// them: this machine's own name, and machines that asked and were shown
    /// nothing.
    pub(crate) fn far_machines_page(
        &self,
        th: &theme::Theme,
        sk: &skin::Skin,
        cx: &mut Context<Self>,
    ) -> (Vec<gpui::Div>, Vec<AnyElement>) {
        let s = self.lang.strings().far;
        let acc = th.accent;
        let mut rows = vec![];
        if let Some(why) = &self.far_floor.machines_trouble {
            rows.push(Self::st_row(
                th,
                sk,
                "machines.toml",
                Some(why.clone()),
                None,
            ));
        }
        if self.far_floor.machines.is_empty() && self.far_floor.machines_trouble.is_none() {
            rows.push(Self::st_row(
                th,
                sk,
                s.st_none,
                Some(s.st_none_sub.to_string()),
                None,
            ));
        }
        for (mi, m) in self.far_floor.machines.iter().enumerate() {
            let dest = m.ssh.clone().unwrap_or_default();
            let probe = self.far_floor.probes.get(&m.name);
            let (status, host, sessions) = match probe {
                None | Some(ProbeView::Asking) => (s.st_asking.to_string(), None, vec![]),
                Some(ProbeView::Done {
                    result: Err(why), ..
                }) => (why.clone(), None, vec![]),
                Some(ProbeView::Done { result: Ok(p), .. }) => {
                    let mut line = match (&p.td, &p.version) {
                        (None, _) => s.st_no_td.to_string(),
                        (Some(_), Some(v)) => s.st_td.replace("{version}", v),
                        (Some(_), None) => s.st_td.replace("{version}", "?"),
                    };
                    if p.tailscale.as_deref().is_some_and(|ip| !ip.is_empty()) {
                        line.push_str(&format!(" \u{b7} {}", s.st_tailscale));
                    }
                    if p.td.is_some() && p.sessions.is_empty() {
                        line.push_str(&format!("\n{}", s.st_no_session));
                    }
                    (line, p.host.clone(), p.sessions.clone())
                }
            };
            let opening = self
                .far_floor
                .opening
                .as_ref()
                .filter(|(name, at)| *name == m.name.to_uppercase() && at.elapsed().as_secs() < 8)
                .map(|(name, _)| s.st_opening.replace("{machine}", name));
            rows.push(Self::st_row(
                th,
                sk,
                &format!("\u{21c4} {} \u{b7} {dest}", m.name.to_uppercase()),
                Some(match opening {
                    Some(o) => format!("{status}\n{o}"),
                    None => status,
                }),
                None,
            ));
            for (si, session) in sessions.iter().enumerate() {
                let sub = match session.window {
                    Some(true) => s.st_session_window,
                    Some(false) => s.st_session_idle,
                    None => s.st_session_unknown,
                };
                let (name, key) = (m.name.clone(), session.key.clone());
                let open = div()
                    .id(("far-open", mi * 100 + si))
                    .debug_selector(move || format!("far-open-{mi}-{si}"))
                    .flex_none()
                    .px_2()
                    .py_0p5()
                    .rounded(sk.radius())
                    .border(sk.box_border(1.))
                    .border_color(acc)
                    .bg(acc.alpha(0.14))
                    .text_size(sk.text_px(9.))
                    .text_color(th.text)
                    .cursor_pointer()
                    .hover(move |st| st.bg(acc.alpha(0.28)))
                    .child(SharedString::from(format!("{} \u{2197}", s.st_open_here)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.far_open_here(&name, &key, cx);
                        }),
                    )
                    .into_any_element();
                rows.push(Self::st_row(
                    th,
                    sk,
                    &s.st_session.replace("{key}", &session.key),
                    Some(sub.to_string()),
                    Some(open.into()),
                ));
            }
            // Sharing goes by the name the machine gives itself, which is
            // known once it has answered. Its Terminal Delight's own word for
            // that comes first: it is the word it will knock with, and it is
            // not always the hostname.
            let host = self
                .far_floor
                .census
                .get(&m.name)
                .and_then(MachineView::census)
                .and_then(|c| c.windows.first())
                .map(|w| w.machine.clone())
                .or(host);
            let sharing = match &host {
                Some(host) => Self::st_row(
                    th,
                    sk,
                    s.st_sharing,
                    // The chips set a ceiling. With this window's own switch
                    // off nothing reaches its agents whatever the ceiling is,
                    // and a person who has just pressed "prompt" is owed that.
                    Some(if self.mcp.enabled {
                        s.st_sharing_sub.to_string()
                    } else {
                        format!("{}\n{}", s.st_sharing_sub, s.st_mcp_off)
                    }),
                    Some(self.far_sharing_chips(th, sk, mi, host, cx).into()),
                ),
                None => Self::st_row(
                    th,
                    sk,
                    s.st_sharing,
                    Some(s.st_sharing_unknown.to_string()),
                    None,
                ),
            };
            rows.push(sharing);
        }
        let mut sections = vec![];
        // Machines that asked and are not among the ones listed above.
        let listed: Vec<String> = self
            .far_floor
            .probes
            .values()
            .filter_map(|p| match p {
                ProbeView::Done { result: Ok(p), .. } => p.host.clone(),
                _ => None,
            })
            .collect();
        let asked: Vec<&String> = self
            .far_floor
            .knocked
            .iter()
            .chain(
                self.far_floor
                    .sharing
                    .hosts()
                    .map(|h| &h.host)
                    .filter(|h| !self.far_floor.knocked.contains(h)),
            )
            .filter(|h| !listed.iter().any(|l| l.eq_ignore_ascii_case(h)))
            .collect();
        if !asked.is_empty() {
            let asked_rows = asked
                .into_iter()
                .enumerate()
                .map(|(i, host)| {
                    Self::st_row(
                        th,
                        sk,
                        &host.to_uppercase(),
                        Some(s.st_asked_sub.to_string()),
                        Some(self.far_sharing_chips(th, sk, 500 + i, host, cx).into()),
                    )
                })
                .collect();
            sections.push(Self::st_section(
                th,
                sk,
                "st-sec-far-asked",
                s.st_asked,
                asked_rows,
            ));
        }
        let mut this_rows = vec![Self::st_row(
            th,
            sk,
            s.st_name,
            Some(s.st_name_sub.to_string()),
            Some(
                div()
                    .debug_selector(|| "far-this-name".to_string())
                    .text_size(sk.text_px(10.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(acc)
                    .child(SharedString::from(farfloor::machine_name()))
                    .into_any_element()
                    .into(),
            ),
        )];
        if let Some(why) = &self.far_floor.sharing_trouble {
            this_rows.push(Self::st_row(
                th,
                sk,
                "sharing.toml",
                Some(why.clone()),
                None,
            ));
        }
        sections.push(Self::st_section(
            th,
            sk,
            "st-sec-far-this",
            s.st_this,
            this_rows,
        ));
        (rows, sections)
    }
}
