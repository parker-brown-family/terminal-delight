//! The background half of one rest-and-sleep tick, for one pane.
//!
//! The window gathers what only it knows — focus, the person's last touch, the
//! screen — into a [`Job`] on the main thread; this runs on the background
//! executor, where reading `/proc`, talking to systemd and waiting for a
//! process to stop cannot hold up a frame; the [`Outcome`] goes back to the
//! main thread to be applied. Everything this touches comes from the pane's own
//! shell pid or its own group: nothing is found by name.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::agentrest::{self, Agent, Asleep, Look, Refusal, Thresholds};
use crate::panegroup::{self, Gate, PaneGroup, Usage};

/// Where a pane stands as the tick starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Awake,
    Resting {
        rested: Duration,
    },
    Asleep,
    /// A person asked for sleep now: the clocks are skipped, the refusals not.
    SleepNow,
}

/// One pane, as the main thread saw it.
pub struct Job {
    pub pane: u64,
    pub shell: i32,
    pub session: String,
    pub gate: Arc<Gate>,
    pub group: Option<PaneGroup>,
    pub agent: Option<Agent>,
    pub stage: Stage,
    /// Everything [`Look`] holds that the window knows; the rest is filled here.
    pub look: Look,
    pub cpu_prev: Option<(Instant, u64)>,
    pub thresholds: Thresholds,
    pub rest_on: bool,
    pub sleep_on: bool,
    /// The gate's write count when the window looked.
    pub epoch: u64,
    /// An agent the sleep signalled earlier that had not gone yet: its pid,
    /// its start time, and the sleep it becomes.
    pub stopping: Option<(i32, Option<u64>, Asleep)>,
}

/// What the tick did to one pane.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    Nothing,
    /// Frozen. `before` is what its processes held in RAM just before, so the
    /// window can say how much has gone out since — at once, and as the
    /// kernel goes on draining a group that no longer touches its pages.
    Rested {
        before: Option<u64>,
    },
    /// It was resting and is not any more (something thawed it).
    Woke,
    /// A previous window left it frozen; this window's gate holds it now.
    FoundResting,
    Slept(Asleep),
    /// Signalled by the sleep and not gone by its deadline: the pane keeps
    /// the sleep it will become, and the next look asks again.
    Stopping {
        pid: i32,
        start: Option<u64>,
        asleep: Asleep,
    },
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub pane: u64,
    pub group: Result<PaneGroup, String>,
    pub usage: Usage,
    pub cpu: Option<(Instant, u64)>,
    pub act: Act,
    pub refusal: Option<Refusal>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Run one pane's tick.
pub fn run(mut job: Job) -> Outcome {
    let group = match job.group.clone() {
        Some(g) if g.dir.exists() => Ok(g),
        _ => panegroup::adopt(&job.session, job.pane, job.shell).map_err(|e| format!("{e:?}")),
    };
    let mut out = Outcome {
        pane: job.pane,
        group: group.clone(),
        usage: Usage::default(),
        cpu: None,
        act: Act::Nothing,
        refusal: None,
    };
    // An agent the sleep already signalled: asleep once it has gone, and
    // nothing else is decided for this pane until then. Read by the process's
    // own identity, so a pane whose group cannot be read right now still
    // reaches its answer and its give-up.
    if let Some((pid, start, asleep)) = job.stopping.take() {
        let ended = pid_ended(pid, start);
        // A new agent under the shell, started by a person in the moment
        // between; the old one, unreaped, is not it.
        let replaced =
            ended && crate::vitals::agent_under(job.shell as u32).is_some_and(|p| p as i32 != pid);
        out.act = match agentrest::stop_look(ended, replaced, Duration::ZERO) {
            agentrest::StopLook::Asleep => Act::Slept(asleep),
            agentrest::StopLook::Replaced => Act::Woke,
            agentrest::StopLook::Stopping | agentrest::StopLook::GaveUp => {
                Act::Stopping { pid, start, asleep }
            }
        };
        if let Ok(g) = &group {
            out.usage = panegroup::usage(g);
        }
        return out;
    }
    let Ok(group) = group else {
        out.refusal = Some(Refusal::Unknown("memory group"));
        return out;
    };
    out.usage = panegroup::usage(&group);
    let now = Instant::now();
    out.cpu = out.usage.cpu_usec.map(|u| (now, u));
    job.look.cpu_share = match (job.cpu_prev, out.cpu) {
        (Some(prev), Some(cur)) => agentrest::cpu_share(prev, cur),
        _ => None,
    };

    // A previous window's rest, found frozen: take it over rather than fight it.
    let frozen = panegroup::frozen(&group);
    match job.stage {
        Stage::Awake if frozen == Some(true) => {
            job.gate.adopt_frozen(&group);
            out.act = Act::FoundResting;
            return out;
        }
        Stage::Resting { .. } if frozen != Some(true) || !job.gate.holds() => {
            out.act = Act::Woke;
            return out;
        }
        Stage::Asleep => return out,
        _ => {}
    }
    if job.stage == Stage::SleepNow {
        job.look.focused = false;
        job.look.since_touch = Some(Duration::MAX);
        job.look.since_output = job
            .look
            .since_output
            .map(|d| d.max(job.thresholds.rest_after));
        job.look.enabled = true;
    }

    let agent_pid = crate::vitals::agent_under(job.shell as u32).map(|p| p as i32);
    let report = match (job.agent, agent_pid) {
        (Some(Agent::Claude), Some(pid)) => agentrest::self_report(pid),
        _ => None,
    };
    if let Some(r) = &report {
        job.look.said_idle_for = r.idle_for(now_ms());
    }
    let census = agent_pid.map(|agent| {
        let listening = panegroup::listening_inodes();
        let procs: Vec<_> = panegroup::procs(&group)
            .into_iter()
            .filter_map(|pid| panegroup::proc_info(pid, &listening))
            .collect();
        (
            agentrest::foreign(&procs, job.shell, agent),
            agentrest::helpers(&procs, job.shell, agent) > 0,
        )
    });
    job.look.owns_helpers = census.as_ref().map(|(_, owns)| *owns);
    job.look.foreign = census.map(|(foreign, _)| foreign);
    job.look.enabled = job.rest_on;

    match job.stage {
        Stage::Awake => match agentrest::may_rest(&job.look, &job.thresholds) {
            Ok(()) => {
                if job.gate.freeze_if_quiet(&group, job.epoch) {
                    let before = out.usage.resident;
                    let _ = panegroup::reclaim_paced(&group);
                    out.act = Act::Rested { before };
                    out.usage = panegroup::usage(&group);
                } else {
                    out.refusal = Some(Refusal::Touched);
                }
            }
            Err(r) => out.refusal = Some(r),
        },
        Stage::Resting { .. } | Stage::SleepNow => {
            let rested = match job.stage {
                Stage::Resting { rested } => rested,
                _ => Duration::MAX,
            };
            if !job.sleep_on && job.stage != Stage::SleepNow {
                out.refusal = Some(Refusal::Off);
                return out;
            }
            let home = crate::session::home_dir();
            // The conversation the agent is in, by its own word: Claude's
            // registry, or the rollout a Codex holds open.
            let session = match job.agent {
                Some(Agent::Codex) => {
                    agent_pid.and_then(|pid| crate::session::codex_held_session(pid as u32, &home))
                }
                _ => report.as_ref().and_then(|r| r.session_id.clone()),
            };
            let (Some(pid), Some(session)) = (agent_pid, session) else {
                out.refusal = Some(Refusal::NoConversation);
                return out;
            };
            let argv = read_argv(pid);
            let wake = match job.agent {
                Some(Agent::Codex) => agentrest::codex_wake_argv(&argv, &session),
                _ => agentrest::wake_argv(&argv, &session),
            };
            if let Err(r) = agentrest::may_sleep(&job.look, rested, &job.thresholds, &wake) {
                out.refusal = Some(r);
                return out;
            }
            // A Codex's held rollout is its saved conversation; a Claude may
            // report an id it has not written down yet.
            let cwd = std::fs::read_link(format!("/proc/{}/cwd", job.shell));
            if job.agent != Some(Agent::Codex)
                && !cwd.is_ok_and(|cwd| conversation_saved(&home, &cwd, &session))
            {
                out.refusal = Some(Refusal::NoConversation);
                return out;
            }
            match sleep(
                &job.gate,
                &group,
                pid,
                &session,
                &wake.unwrap_or_default(),
                &argv,
                job.epoch,
            ) {
                Ok(Ended::Gone(asleep)) => {
                    out.act = Act::Slept(asleep);
                    // The state now describes the surviving shell; the usage
                    // sampled before termination described the agent as well.
                    out.usage = panegroup::usage(&group);
                }
                Ok(Ended::Stopping(asleep, start)) => {
                    out.act = Act::Stopping { pid, start, asleep }
                }
                Err(r) => out.refusal = Some(r),
            }
        }
        Stage::Asleep => {}
    }
    out
}

fn read_argv(pid: i32) -> Vec<String> {
    std::fs::read(format!("/proc/{pid}/cmdline"))
        .map(|b| {
            b.split(|c| *c == 0)
                .filter(|a| !a.is_empty())
                .map(|a| String::from_utf8_lossy(a).into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Where wake scripts are kept.
pub fn wake_dir() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("terminal-delight/wake")
}

/// The script a sleeper wakes through: its launch line, word for word.
pub fn wake_script(argv: &[String]) -> String {
    let line: Vec<String> = argv.iter().map(|w| agentrest::shell_quote(w)).collect();
    format!(
        "# Written by Terminal Delight when this agent went to sleep.\n\
         # Its own launch line, naming the conversation it resumes.\n\
         #\n\
         # First, up to three seconds for the window to give this pane its own\n\
         # memory group, so the agent is born in it and every page it touches is\n\
         # charged where rest can reach it.\n\
         for _ in $(seq 60); do read -r cg < /proc/self/cgroup; case $cg in *td-pane-*) break ;; esac; sleep 0.05; done\n\
         exec {}\n",
        line.join(" ")
    )
}

/// Has Claude Code saved conversation `session` where a wake started in `cwd`
/// will look for it: `~/.claude/projects/<cwd slug>/<session>.jsonl`, not empty.
///
/// Claude reports a session id from the moment it starts, but writes the
/// conversation down only once something is said in it. A Claude started and
/// never spoken to, or one `/clear`ed into a fresh id, reports an id that
/// `--resume` cannot open, and ending it leaves a wake line that answers "No
/// conversation found with session ID" — the job-search pane on 2026-10-27.
/// Read from the pane shell's cwd because that is where the wake runs. The id
/// has already passed `safe_resume_id`, so it cannot climb out of the folder.
/// A slug Claude shortens (a path over 200 characters) is not found, and the
/// pane stays awake: that costs memory, where the other mistake costs the pane.
pub fn conversation_saved(home: &std::path::Path, cwd: &std::path::Path, session: &str) -> bool {
    let Some(cwd) = cwd.to_str() else {
        return false;
    };
    let file = home
        .join(".claude/projects")
        .join(crate::session::claude_slug(cwd))
        .join(format!("{session}.jsonl"));
    std::fs::metadata(file).is_ok_and(|m| m.is_file() && m.len() > 0)
}

/// The model word in a launch line, for the sleep card: Claude's `--model`, or
/// Codex's `--model` / `-m`.
/// The harness a launch line runs, as the Floor names it: the first word
/// whose file name is `claude…` or `codex…`. A node wrapper's own path is
/// passed over, since the script it runs is the program.
fn program_of(argv: &[String]) -> Option<String> {
    argv.iter().find_map(|a| {
        let name = std::path::Path::new(a).file_name()?.to_str()?;
        if name.starts_with("claude") {
            Some("CLAUDE".to_string())
        } else if name.starts_with("codex") {
            Some("CODEX".to_string())
        } else {
            None
        }
    })
}

fn model_of(argv: &[String]) -> Option<String> {
    argv.iter()
        .enumerate()
        .find_map(|(i, w)| match w.split_once('=') {
            Some(("--model", v)) => Some(v.to_owned()),
            _ if w == "--model" || w == "-m" => argv.get(i + 1).cloned(),
            _ => None,
        })
}

/// What the sleep did to an agent it signalled.
#[derive(Debug)]
enum Ended {
    /// Gone within the deadline.
    Gone(Asleep),
    /// Signalled, and still going at the deadline, with its start time. The
    /// window looks again on each sweep (`Job::stopping`) and gives up after
    /// [`agentrest::STOP_GIVE_UP`].
    Stopping(Asleep, Option<u64>),
}

/// How long the sleep waits, in the sweep, for the agent it signalled. Every
/// other pane's look waits with it, so it stays short; an agent slower than
/// this is followed up on later sweeps instead.
const STOP_WAIT: Duration = Duration::from_secs(5);

/// A process's state letter and start time from `/proc/<pid>/stat`. The
/// fields are counted after the parenthesised name, which may itself hold
/// spaces and parentheses: the state is field 3, the start time field 22.
fn stat_of(stat: &str) -> Option<(String, Option<u64>)> {
    let (_, rest) = stat.rsplit_once(')')?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let state = fields.first()?.to_string();
    Some((state, fields.get(19).and_then(|f| f.parse().ok())))
}

fn start_of(pid: i32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat_of(&stat).and_then(|(_, start)| start)
}

/// Whether the process `pid`, started at `start`, has ended: no `/proc`
/// entry, a zombie waiting to be reaped, or that pid now naming a process
/// that started at another time. A `/proc` read that fails for any reason
/// but absence is not an ending: unknown is not gone, and a pane marked
/// asleep over a running agent would put two writers on one conversation.
fn pid_ended(pid: i32, start: Option<u64>) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
        Ok(stat) => match stat_of(&stat) {
            Some((state, now)) => {
                state == "Z" || state == "X" || matches!((start, now), (Some(a), Some(b)) if a != b)
            }
            None => false,
        },
    }
}

/// SIGTERM to `pid`, then up to `wait` for it to end. Whether it did, and
/// the start time it was signalled at.
fn signal_and_wait(pid: i32, wait: Duration) -> Result<(bool, Option<u64>), Refusal> {
    let start = start_of(pid);
    // SAFETY: plain kill(2) on a pid the caller read from this pane's group.
    if unsafe { libc::kill(pid, libc::SIGTERM) } != 0 {
        return Err(Refusal::Unknown("agent process"));
    }
    let deadline = Instant::now() + wait;
    while !pid_ended(pid, start) {
        if Instant::now() > deadline {
            return Ok((false, start));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Ok((true, start))
}

/// End the agent and keep its conversation: write the wake script, thaw, send
/// SIGTERM to the agent alone, and wait a moment for it to go. An agent still
/// going at the deadline is followed up on later sweeps ([`Ended::Stopping`]),
/// and never killed harder: SIGKILL could cut a transcript write.
fn sleep(
    gate: &Gate,
    group: &PaneGroup,
    pid: i32,
    session: &str,
    wake: &[String],
    argv: &[String],
    epoch: u64,
) -> Result<Ended, Refusal> {
    let dir = wake_dir();
    std::fs::create_dir_all(&dir).map_err(|e| Refusal::LaunchLine(e.to_string()))?;
    let wake_file = dir.join(format!("{session}.sh"));
    std::fs::write(&wake_file, wake_script(wake))
        .map_err(|e| Refusal::LaunchLine(e.to_string()))?;
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&wake_file, std::fs::Permissions::from_mode(0o600));
    }
    // Thaw, then make sure nobody typed in the moment between the look and now.
    gate.release();
    if gate.writes() != epoch {
        return Err(Refusal::Touched);
    }
    // The pid is the pane's own agent, read from the pane's own shell a moment
    // ago; check it is still the same program before signalling it.
    if !panegroup::procs(group).contains(&pid) {
        return Err(Refusal::Unknown("agent process"));
    }
    let signalled = Instant::now();
    let (gone, start) = signal_and_wait(pid, STOP_WAIT)?;
    eprintln!(
        "terminal-delight: sleep signalled pane agent pid {pid}; {} after {} ms",
        if gone { "it ended" } else { "still going" },
        signalled.elapsed().as_millis()
    );
    let asleep = Asleep {
        since_ms: now_ms(),
        session: session.to_owned(),
        wake_file,
        model: model_of(argv),
        program: program_of(argv),
    };
    Ok(if gone {
        Ended::Gone(asleep)
    } else {
        Ended::Stopping(asleep, start)
    })
}

/// A live process that has conversation `session` open: a Claude Code by its
/// own registry, or a Codex holding that conversation's rollout. Waking a
/// sleeper while one exists would put two writers on one conversation — what a
/// probe did to pane 111 on 2026-10-26.
pub fn conversation_holder(session: &str) -> Option<i32> {
    let home = std::path::PathBuf::from(std::env::var_os("HOME")?);
    let dir = home.join(".claude/sessions");
    let claude = std::fs::read_dir(dir).ok().and_then(|entries| {
        entries.flatten().find_map(|entry| {
            let name = entry.file_name();
            let pid: i32 = name.to_str()?.strip_suffix(".json")?.parse().ok()?;
            let text = std::fs::read_to_string(entry.path()).ok()?;
            let report = agentrest::parse_self_report(&text, pid)?;
            (report.session_id.as_deref() == Some(session)
                && std::path::Path::new(&format!("/proc/{pid}")).exists())
            .then_some(pid)
        })
    });
    claude.or_else(|| codex_holder(&home, session))
}

/// A live process holding Codex conversation `session`'s rollout open. Read
/// from `/proc`, once, when a key wakes a pane: a scan, but not on any tick.
fn codex_holder(home: &std::path::Path, session: &str) -> Option<i32> {
    std::fs::read_dir("/proc")
        .ok()?
        .flatten()
        .find_map(|entry| {
            let pid: i32 = entry.file_name().to_str()?.parse().ok()?;
            let fds = std::fs::read_dir(entry.path().join("fd")).ok()?;
            fds.flatten()
                .any(|fd| {
                    std::fs::read_link(fd.path()).is_ok_and(|target| {
                        crate::session::is_codex_rollout_of(&target, home, session)
                    })
                })
                .then_some(pid)
        })
}

/// What must outlive a window: which panes are asleep (and how they wake), and
/// which the person pinned awake. Keyed by the host's pane id, because a sleep
/// means something only while that pane's shell is alive, and a window of an
/// older build that does not know this file just shows the shell.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Store {
    #[serde(default)]
    pub asleep: std::collections::BTreeMap<String, Asleep>,
    #[serde(default)]
    pub keep_awake: std::collections::BTreeSet<u64>,
}

pub fn store_path(session: &str) -> PathBuf {
    let key: String = session
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    wake_dir()
        .parent()
        .map(|p| p.join(format!("rest-{key}.toml")))
        .unwrap_or_else(|| PathBuf::from(format!("rest-{key}.toml")))
}

pub fn load_store(session: &str) -> Store {
    std::fs::read_to_string(store_path(session))
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_store(session: &str, store: &Store) {
    if let Ok(body) = toml::to_string(store) {
        let path = store_path(session);
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = crate::session::write_atomic(&path, &body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An agent slower to go than the sleep's wait is not "did not stop": it
    /// is followed up, and once it has gone, even as a zombie its parent has
    /// not reaped yet, it reads as ended, so the pane is recorded asleep. Here
    /// a real process stands in for a slow agent: it takes SIGTERM, lingers a
    /// second and a half, and exits.
    ///
    /// Mutation-tested: `pid_ended` that ignores a zombie, or a sleep that
    /// returns early as gone, fails this.
    #[test]
    fn an_agent_slower_than_the_wait_is_still_seen_to_end() {
        let mut child = std::process::Command::new("sh")
            .args([
                "-c",
                "trap 'sleep 1.5; exit 0' TERM; while :; do sleep 0.05; done",
            ])
            .spawn()
            .unwrap();
        let pid = child.id() as i32;
        // Let the shell install its trap before it is signalled.
        std::thread::sleep(Duration::from_millis(300));
        let (gone, start) = signal_and_wait(pid, Duration::from_millis(200)).unwrap();
        assert!(!gone);
        assert!(start.is_some(), "its start time was read");
        assert!(!pid_ended(pid, start), "still running inside its trap");
        // The same pid with another start time is another process.
        assert!(
            pid_ended(pid, start.map(|s| s + 1)),
            "a reused pid is not the agent"
        );
        let t = Instant::now();
        while !pid_ended(pid, start) {
            assert!(t.elapsed() < Duration::from_secs(10), "never seen to end");
            std::thread::sleep(Duration::from_millis(25));
        }
        // Ended while still unreaped: a zombie is gone for the sleep's purposes.
        assert!(std::path::Path::new(&format!("/proc/{pid}")).exists());
        child.wait().unwrap();
        assert!(pid_ended(pid, start));
    }

    /// The state and start time are read after the name's last parenthesis,
    /// so a program that names itself with spaces and parentheses cannot shift
    /// them; and a line that cannot be read is not an ending.
    #[test]
    fn a_stat_line_is_read_past_a_hostile_name() {
        let line = "4242 (evil) Z 1 2 3) S 1 4242 4242 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 1 0 987654 1000 10";
        assert_eq!(stat_of(line), Some(("S".to_string(), Some(987654))));
        assert_eq!(stat_of("garbage"), None);
    }

    #[test]
    fn a_launch_line_names_its_harness_and_an_old_record_reads_as_unknown() {
        let argv = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            program_of(&argv(&["/usr/bin/node", "/opt/claude/cli.js"])),
            None
        );
        assert_eq!(
            program_of(&argv(&["/home/p/.local/bin/claude", "--model", "opus"])).as_deref(),
            Some("CLAUDE")
        );
        assert_eq!(
            program_of(&argv(&["codex", "resume", "x"])).as_deref(),
            Some("CODEX")
        );
        assert_eq!(program_of(&argv(&["bash"])), None);
        // A sleep written before the harness was kept reads as unknown, not Claude.
        let old: Asleep =
            toml::from_str("since_ms = 1\nsession = \"s\"\nwake_file = \"/w.sh\"\n").unwrap();
        assert_eq!(old.program, None);
    }

    #[test]
    fn the_store_round_trips_and_an_empty_one_reads_as_nothing_asleep() {
        let mut store = Store::default();
        store.asleep.insert(
            "12".into(),
            Asleep {
                since_ms: 1,
                session: "abc".into(),
                wake_file: PathBuf::from("/w/abc.sh"),
                model: Some("opus".into()),
                program: Some("CODEX".into()),
            },
        );
        store.keep_awake.insert(4);
        let back: Store = toml::from_str(&toml::to_string(&store).unwrap()).unwrap();
        assert_eq!(back, store);
        let empty: Store = toml::from_str("").unwrap();
        assert!(empty.asleep.is_empty() && empty.keep_awake.is_empty());
    }

    fn s(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn the_wake_script_execs_the_launch_line_word_for_word() {
        let script = wake_script(&s(&[
            "/home/p/.local/bin/claude",
            "--append-system-prompt",
            "it's $x",
            "--resume",
            "abc-1",
        ]));
        assert!(script.ends_with("exec /home/p/.local/bin/claude --append-system-prompt 'it'\\''s $x' --resume abc-1\n"), "{script}");
    }

    #[test]
    fn only_a_conversation_claude_has_saved_where_the_wake_looks_may_sleep() {
        let home = std::env::temp_dir().join(format!("td-rest-saved-{}", std::process::id()));
        let cwd = std::path::Path::new("/home/p/job-search-aug-2026");
        let projects = home.join(".claude/projects");
        let here = projects.join(crate::session::claude_slug("/home/p/job-search-aug-2026"));
        std::fs::create_dir_all(&here).unwrap();
        let id = "c159b064-501c-4a50-b539-f4a64b4d6601";

        // Started, never spoken to: Claude has an id and no file for it.
        assert!(!conversation_saved(&home, cwd, id));
        std::fs::write(here.join(format!("{id}.jsonl")), "").unwrap();
        assert!(
            !conversation_saved(&home, cwd, id),
            "an empty file holds no conversation"
        );
        std::fs::write(here.join(format!("{id}.jsonl")), "{\"type\":\"user\"}\n").unwrap();
        assert!(conversation_saved(&home, cwd, id));
        // Saved, but under another directory: a wake from this one cannot open it.
        assert!(!conversation_saved(
            &home,
            std::path::Path::new("/home/p/elsewhere"),
            id
        ));

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_model_is_read_from_either_spelling() {
        assert_eq!(
            model_of(&s(&["claude", "--model", "opus"])).as_deref(),
            Some("opus")
        );
        assert_eq!(
            model_of(&s(&["claude", "--model=opus"])).as_deref(),
            Some("opus")
        );
        assert_eq!(model_of(&s(&["claude"])), None);
    }
}
