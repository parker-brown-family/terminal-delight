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
    job.look.foreign = agent_pid.map(|agent| {
        let listening = panegroup::listening_inodes();
        let procs: Vec<_> = panegroup::procs(&group)
            .into_iter()
            .filter_map(|pid| panegroup::proc_info(pid, &listening))
            .collect();
        agentrest::foreign(&procs, job.shell, agent)
    });
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
            let (Some(pid), Some(session)) = (
                agent_pid,
                report.as_ref().and_then(|r| r.session_id.clone()),
            ) else {
                out.refusal = Some(Refusal::NoConversation);
                return out;
            };
            let argv = read_argv(pid);
            let wake = agentrest::wake_argv(&argv, &session);
            if let Err(r) = agentrest::may_sleep(&job.look, rested, &job.thresholds, &wake) {
                out.refusal = Some(r);
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
                Ok(asleep) => {
                    out.act = Act::Slept(asleep);
                    // The state now describes the surviving shell; the usage
                    // sampled before termination described the agent as well.
                    out.usage = panegroup::usage(&group);
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
         # Its own launch line, with --resume naming its conversation.\n\
         #\n\
         # First, up to three seconds for the window to give this pane its own\n\
         # memory group, so the agent is born in it and every page it touches is\n\
         # charged where rest can reach it.\n\
         for _ in $(seq 60); do read -r cg < /proc/self/cgroup; case $cg in *td-pane-*) break ;; esac; sleep 0.05; done\n\
         exec {}\n",
        line.join(" ")
    )
}

/// The model word in a launch line, for the sleep card.
fn model_of(argv: &[String]) -> Option<String> {
    argv.iter()
        .enumerate()
        .find_map(|(i, w)| match w.split_once('=') {
            Some(("--model", v)) => Some(v.to_owned()),
            _ if w == "--model" => argv.get(i + 1).cloned(),
            _ => None,
        })
}

/// End the agent and keep its conversation: write the wake script, thaw, send
/// SIGTERM to the agent alone, and wait for it and its self-report to go. An
/// agent that does not stop within five seconds is left as it is — awake — and
/// never killed harder: SIGKILL could cut a transcript write.
fn sleep(
    gate: &Gate,
    group: &PaneGroup,
    pid: i32,
    session: &str,
    wake: &[String],
    argv: &[String],
    epoch: u64,
) -> Result<Asleep, Refusal> {
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
    // SAFETY: plain kill(2) on a pid from this pane's own group.
    if unsafe { libc::kill(pid, libc::SIGTERM) } != 0 {
        return Err(Refusal::Unknown("agent process"));
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let gone = |pid: i32| !std::path::Path::new(&format!("/proc/{pid}")).exists();
    while !gone(pid) {
        if Instant::now() > deadline {
            return Err(Refusal::AgentBusy(Some("did not stop".into())));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Ok(Asleep {
        since_ms: now_ms(),
        session: session.to_owned(),
        wake_file,
        model: model_of(argv),
    })
}

/// A live Claude Code process that has `session` open, by its own registry.
/// Waking a sleeper while one exists would put two writers on one
/// conversation — what a probe did to pane 111 on 2026-10-26.
pub fn conversation_holder(session: &str) -> Option<i32> {
    let home = std::env::var_os("HOME")?;
    let dir = std::path::Path::new(&home).join(".claude/sessions");
    std::fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        let name = entry.file_name();
        let pid: i32 = name.to_str()?.strip_suffix(".json")?.parse().ok()?;
        let text = std::fs::read_to_string(entry.path()).ok()?;
        let report = agentrest::parse_self_report(&text, pid)?;
        (report.session_id.as_deref() == Some(session)
            && std::path::Path::new(&format!("/proc/{pid}")).exists())
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
