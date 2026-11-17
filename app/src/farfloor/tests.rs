use super::*;
use crate::mcp::{Expose, McpConfig};

fn cfg(enabled: bool) -> McpConfig {
    McpConfig {
        enabled,
        expose: Expose::All,
        ..Default::default()
    }
}

fn agent(access: AgentAccess, seated: bool) -> Party {
    Party {
        seat: seated.then_some(SeatId(7)),
        is_agent: true,
        access,
    }
}

#[test]
fn a_machine_names_itself_by_its_override_then_its_hostname() {
    assert_eq!(
        machine_name_from(Some("legion-a"), Some("legion\n")),
        "legion-a"
    );
    assert_eq!(machine_name_from(None, Some("far-box\n")), "far-box");
    assert_eq!(machine_name_from(Some("  "), Some("box")), "box");
    // Nothing a shell or a file name would read as something else survives.
    assert_eq!(machine_name_from(Some("a b;rm"), None), "abrm");
    assert_eq!(machine_name_from(None, None), "unnamed");
}

#[test]
fn a_machine_nobody_listed_reaches_nothing_and_an_unread_word_grants_nothing() {
    let s = Sharing::parse(
        "[[machine]]\nhost = \"legion\"\nreach = \"ask\"\n\n[[machine]]\nhost = \"later\"\nreach = \"command\"\n",
    )
    .unwrap();
    assert_eq!(s.ceiling("legion"), Some(Rung::Ask));
    assert_eq!(
        s.ceiling("LEGION"),
        Some(Rung::Ask),
        "a hostname's case is not its identity"
    );
    assert_eq!(s.ceiling("stranger"), None);
    assert_eq!(
        s.ceiling("later"),
        None,
        "a word a later build wrote opens nothing here"
    );
    assert_eq!(Sharing::parse("").unwrap().ceiling("legion"), None);
    assert!(Sharing::parse("[[machine]]\nhost = 3\n").is_err());
}

#[test]
fn sharing_round_trips_and_keeps_a_later_builds_word() {
    let mut s = Sharing::parse("[[machine]]\nhost = \"later\"\nreach = \"command\"\n").unwrap();
    assert!(s.set("legion", Some(Rung::Prompt)));
    assert!(
        !s.set("legion", Some(Rung::Prompt)),
        "the same again changes nothing"
    );
    assert!(s.set("legion", Some(Rung::See)));
    let back = Sharing::parse(&s.to_toml()).unwrap();
    assert_eq!(back, s);
    assert_eq!(back.ceiling("legion"), Some(Rung::See));
    assert!(
        s.to_toml().contains("command"),
        "an unread word is written back as it was"
    );
    assert!(s.set("legion", None));
    assert_eq!(s.ceiling("legion"), None);
    assert!(!s.set("", Some(Rung::See)));
    assert!(!s.set("nobody", None));
}

#[test]
fn a_far_agent_gets_the_least_of_the_ceiling_and_the_panes_position() {
    use Reach::*;
    let on = cfg(true);
    // No ceiling: nothing, seated or not. A machine never told about another
    // shows it no agent, which is the whole default.
    assert_eq!(
        inbound_reach(&on, None, &agent(AgentAccess::Open, false)),
        Hidden
    );
    assert_eq!(
        inbound_reach(&on, None, &agent(AgentAccess::Open, true)),
        Hidden
    );
    // The ceiling, where the pane is open.
    assert_eq!(
        inbound_reach(&on, Some(Rung::See), &agent(AgentAccess::Open, true)),
        Seen
    );
    assert_eq!(
        inbound_reach(&on, Some(Rung::Prompt), &agent(AgentAccess::Open, false)),
        Prompt
    );
    // A read-only pane is asked and never prompted, whoever asks.
    assert_eq!(
        inbound_reach(&on, Some(Rung::Prompt), &agent(AgentAccess::ReadOnly, true)),
        Ask
    );
    assert_eq!(
        inbound_reach(&on, Some(Rung::Read), &agent(AgentAccess::ReadOnly, true)),
        Read
    );
    // Off-limits is off-limits.
    assert_eq!(
        inbound_reach(
            &on,
            Some(Rung::Prompt),
            &agent(AgentAccess::OffLimits, true)
        ),
        Hidden
    );
    // The window's switch off shares nothing whatever the file says.
    assert_eq!(
        inbound_reach(
            &cfg(false),
            Some(Rung::Prompt),
            &agent(AgentAccess::Open, true)
        ),
        Hidden
    );
    // A plain shell is not shown when the window shows agents only.
    let agents_only = McpConfig {
        enabled: true,
        expose: Expose::AgentsOnly,
        ..Default::default()
    };
    let shell = Party {
        seat: None,
        is_agent: false,
        access: AgentAccess::Open,
    };
    assert_eq!(
        inbound_reach(&agents_only, Some(Rung::Prompt), &shell),
        Hidden
    );
}

#[test]
fn a_far_agent_is_named_by_conversation_or_seat_and_never_by_a_pane_number() {
    let c = FarAddr::parse("conv:0199-abcd").unwrap();
    assert_eq!(c, FarAddr::Conversation("0199-abcd".into()));
    assert_eq!(c.to_string(), "conv:0199-abcd");
    assert_eq!(FarAddr::parse("seat:22"), Some(FarAddr::Seat(SeatId(22))));
    assert_eq!(FarAddr::Seat(SeatId(22)).to_string(), "seat:22");
    for bad in [
        "pane:3", "pid:4411", "seat:x", "conv:", "conv:a b", "22", "",
    ] {
        assert_eq!(FarAddr::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn a_line_chip_cycles_through_the_rungs_and_back_to_nothing() {
    let mut lines = FarLines::default();
    let (seat, m, to) = (SeatId(11), "farbox", "seat:22");
    assert_eq!(lines.rung(seat, m, to), None);
    let mut seen = vec![];
    for _ in 0..5 {
        seen.push(lines.cycle(seat, m, to, "fixer"));
    }
    assert_eq!(
        seen,
        vec![
            Some(Rung::See),
            Some(Rung::Read),
            Some(Rung::Ask),
            Some(Rung::Prompt),
            None
        ]
    );
    assert!(lines.is_empty(), "the last press erased it");
    // Lines are per seat, per machine, per agent.
    lines.set(seat, m, to, "fixer", Some(Rung::Prompt));
    lines.set(SeatId(12), m, to, "fixer", Some(Rung::See));
    lines.set(seat, "other", to, "x", Some(Rung::Ask));
    assert_eq!(lines.rung(seat, m, to), Some(Rung::Prompt));
    assert_eq!(lines.rung(SeatId(12), m, to), Some(Rung::See));
    assert_eq!(lines.rung(seat, m, "seat:21"), None);
    assert_eq!(lines.machines(), vec!["farbox", "other"]);
    assert_eq!(lines.lines_of(seat).count(), 2);
    lines.forget_seat(seat);
    assert_eq!(lines.saved().len(), 1);
    assert_eq!(lines.rung(SeatId(12), m, to), Some(Rung::See));
}

#[test]
fn a_far_line_saved_by_a_later_build_is_kept_and_opens_nothing() {
    let saved: Vec<FarLine> = toml::from_str::<std::collections::HashMap<String, Vec<FarLine>>>(
        "[[far_lines]]\nfrom = 11\nmachine = \"farbox\"\nto = \"seat:22\"\nrung = \"command\"\n",
    )
    .unwrap()
    .remove("far_lines")
    .unwrap();
    let lines = FarLines::from_saved(saved);
    assert_eq!(lines.rung(SeatId(11), "farbox", "seat:22"), None);
    assert_eq!(lines.saved()[0].rung, RungWord::Unknown("command".into()));
}

#[test]
fn what_leaves_is_the_least_of_the_line_and_what_the_far_machine_allows() {
    use Reach::*;
    assert_eq!(
        outbound_reach(None, Some(Prompt)),
        Hidden,
        "no line, no reach"
    );
    assert_eq!(
        outbound_reach(Some(Rung::Prompt), None),
        Hidden,
        "an agent it did not list"
    );
    assert_eq!(outbound_reach(Some(Rung::Prompt), Some(Ask)), Ask);
    assert_eq!(outbound_reach(Some(Rung::See), Some(Prompt)), Seen);
    assert_eq!(outbound_reach(Some(Rung::Prompt), Some(Prompt)), Prompt);
}

fn census_line(pid: u32, agents: &str, ceiling: &str) -> String {
    format!(
        "{pid}\t{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"content\":[{{\"type\":\"text\",\"text\":\"x\"}}],\"structuredContent\":{{\"machine\":\"farbox-b\",\"session\":\"1\",\"window\":{pid},\"mcp\":true,\"ceiling\":{ceiling},\"agents\":[{agents}],\"unnamed\":0}}}}}}"
    )
}

#[test]
fn a_census_is_read_window_by_window_and_an_older_window_is_a_trouble_not_a_silence() {
    let agent = "{\"addr\":\"seat:22\",\"name\":\"fixer\",\"harness\":\"CLAUDE\",\"state\":\"idle\",\"model\":null,\"size\":null,\"reach\":\"prompt\"}";
    let out = format!(
        "{}\n{}\n4711\terr unknown command\n4712\t{{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{{\"code\":-32601,\"message\":\"method not found: far/census\"}}}}\n",
        census_line(100, agent, "\"prompt\""),
        census_line(200, "", "null"),
    );
    let c = parse_census(&out).unwrap();
    assert_eq!(c.windows.len(), 2);
    assert_eq!(c.troubles.len(), 2, "{:?}", c.troubles);
    assert!(c.troubles[1].contains("method not found"));
    let (window, a) = c.find("seat:22").expect("the agent");
    assert_eq!((window.window, a.reach), (100, Reach::Prompt));
    assert_eq!(a.size, None, "not measured is not zero");
    assert!(c.shares());
    assert!(c.find("seat:21").is_none());
    // A machine that shares nothing answers, and says so.
    let none = parse_census(&census_line(200, "", "null")).unwrap();
    assert!(!none.shares());
    assert_eq!(
        parse_census("no-terminal-delight\n").unwrap_err(),
        "Terminal Delight is not installed there"
    );
    assert!(parse_census("\n")
        .unwrap_err()
        .contains("no Terminal Delight window"));
}

#[test]
fn a_far_prompts_stage_is_read_from_the_far_windows_own_words() {
    let said = |structured: serde_json::Value| serde_json::json!({ "content": [{"type":"text","text":"x"}], "structuredContent": structured });
    let (id, stage) = stage_of(&said(
        serde_json::json!({"prompt_id": 9, "stage": "waiting", "worker": "working"}),
    ));
    assert_eq!(id, Some(9));
    assert_eq!(
        stage,
        FarStage::Held {
            far: "waiting".into()
        }
    );
    assert!(stage.is_open());
    let (_, done) = stage_of(&said(
        serde_json::json!({"prompt_id": 9, "stage": "finished", "reply": "hello"}),
    ));
    assert_eq!(
        done,
        FarStage::Finished {
            reply: Some("hello".into()),
            unknown: None
        }
    );
    assert!(!done.is_open());
    let (_, unknown) = stage_of(&said(
        serde_json::json!({"prompt_id": 9, "stage": "finished", "reply": null, "reply_unknown": "no conversation"}),
    ));
    assert_eq!(
        unknown,
        FarStage::Finished {
            reply: None,
            unknown: Some("no conversation".into())
        }
    );
    let (_, failed) = stage_of(&said(
        serde_json::json!({"prompt_id": 9, "stage": "failed", "why": "pasted, not sent"}),
    ));
    assert_eq!(
        failed,
        FarStage::Failed {
            why: "pasted, not sent".into()
        }
    );
    // A refusal arrives as a tool error, in the far side's words.
    let refused = serde_json::json!({ "isError": true, "content": [{"type":"text","text":"this machine does not let yours prompt that agent."}] });
    let (id, stage) = stage_of(&refused);
    assert_eq!(id, None);
    assert!(matches!(stage, FarStage::Failed { why } if why.contains("does not let yours")));
    // A shape nobody can read is a failure, never a quiet "held".
    assert!(matches!(
        stage_of(&said(serde_json::json!({}))).1,
        FarStage::Failed { .. }
    ));
}

#[test]
fn a_prompt_whose_far_window_closed_ends_as_not_known_rather_than_holding_its_sender() {
    let mut book = FarBook::default();
    book.open(FarSent {
        id: 1,
        sender_pane: 4,
        machine: "farbox".into(),
        to: "seat:22".into(),
        far: Some((100, 9)),
        sent_ms: 0,
        stage: FarStage::Held {
            far: "working".into(),
        },
        misses: 0,
    });
    assert!(book.busy(4, "farbox", "seat:22"));
    assert!(
        !book.busy(5, "farbox", "seat:22"),
        "another sender is not held up"
    );
    for _ in 1..WINDOW_GONE_MISSES {
        assert!(!book.window_missed(1, "err unreachable"));
    }
    // An answer in between starts the count again: only misses in a row end it.
    assert!(
        !book.heard(
            1,
            FarStage::Held {
                far: "working".into()
            }
        ),
        "same stage, no change"
    );
    for _ in 1..WINDOW_GONE_MISSES {
        assert!(!book.window_missed(1, "err unreachable"));
    }
    assert!(book.window_missed(1, "err unreachable"));
    let sent = book.of_sender(1, 4).unwrap();
    assert!(matches!(&sent.stage, FarStage::Failed { why } if why.contains("not known")));
    assert!(
        !book.busy(4, "farbox", "seat:22"),
        "the sender may try again"
    );
    assert!(
        book.of_sender(1, 5).is_none(),
        "only the pane that sent it reads it"
    );
    assert!(book.open_ones().is_empty());
}

#[test]
fn the_far_script_carries_the_line_on_stdin_and_refuses_a_window_that_is_not_a_number() {
    let run = |arg: &str| {
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg(rpc_script())
            .arg("td")
            .arg(arg)
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", "/nonexistent")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child.stdin.take().unwrap().write_all(b"{}").unwrap();
        String::from_utf8_lossy(&child.wait_with_output().unwrap().stdout).into_owned()
    };
    // No Terminal Delight on this PATH: said in a word the reader knows.
    assert_eq!(run("all").trim(), "no-terminal-delight");
    assert_eq!(run("12; rm -rf x").trim(), "no-terminal-delight");
}

/// Two windows changing the sharing file at once: a change read from a copy
/// taken before another window's save must not write that save away. While
/// one holder has the lock, `update` waits, and when it runs it reads the
/// file as the holder left it, so a revocation made under the lock stands.
///
/// Mutation-tested: dropping the `flock` lets the update read the file
/// before the holder's revocation and restore `far-a`.
#[test]
fn a_change_waits_for_the_lock_and_never_restores_a_revocation() {
    use std::os::fd::AsRawFd;
    let dir = std::env::temp_dir().join(format!("td-sharing-lock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut start = Sharing::default();
    start.set("far-a", Some(Rung::Prompt));
    start.save(&dir).unwrap();
    // Another window takes the lock first.
    let held = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("sharing.toml.lock"))
        .unwrap();
    assert_eq!(unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX) }, 0);
    let d = dir.clone();
    let other = std::thread::spawn(move || Sharing::update(&d, "far-b", Some(Rung::See)));
    std::thread::sleep(std::time::Duration::from_millis(150));
    assert!(!other.is_finished(), "the change waits for the lock");
    // Under the lock, that window revokes far-a and saves.
    let mut revoked = Sharing::load(&dir).unwrap();
    assert!(revoked.set("far-a", None));
    revoked.save(&dir).unwrap();
    drop(held);
    let after = other.join().unwrap().unwrap();
    assert_eq!(after.ceiling("far-a"), None, "the revocation stands");
    assert_eq!(after.ceiling("far-b"), Some(Rung::See));
    let on_disk = Sharing::load(&dir).unwrap();
    assert_eq!(on_disk.ceiling("far-a"), None);
    assert_eq!(on_disk.ceiling("far-b"), Some(Rung::See));
    let strays: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(strays.is_empty(), "no temporary file is left: {strays:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A line saved while a far agent was known only by its seat still reaches it
/// once its conversation is proven and the census names it by that.
#[test]
fn a_far_agent_is_found_by_its_seat_after_its_conversation_is_known() {
    let agent = "{\"addr\":\"conv:0199\",\"also\":\"seat:22\",\"name\":\"fixer\",\"harness\":\"CLAUDE\",\"state\":\"idle\",\"reach\":\"prompt\"}";
    let c = parse_census(&census_line(100, agent, "\"prompt\"")).unwrap();
    assert_eq!(
        c.find("conv:0199").map(|(_, a)| a.addr.as_str()),
        Some("conv:0199")
    );
    assert_eq!(
        c.find("seat:22").map(|(_, a)| a.addr.as_str()),
        Some("conv:0199"),
        "the seat it also answers to"
    );
    assert!(c.find("seat:21").is_none());
}

/// Two agents on one machine that both say `key: "review"` are two
/// assignments on the far machine, which keeps one key book per machine.
#[test]
fn a_far_prompt_key_is_the_sending_agents_own() {
    let a = scoped_key(Some(SeatId(3)), 11, "review");
    let b = scoped_key(Some(SeatId(4)), 12, "review");
    assert_ne!(a, b);
    assert_eq!(
        a,
        scoped_key(Some(SeatId(3)), 99, "review"),
        "a seat survives a wake"
    );
    assert_ne!(
        scoped_key(None, 11, "review"),
        scoped_key(None, 12, "review")
    );
}
