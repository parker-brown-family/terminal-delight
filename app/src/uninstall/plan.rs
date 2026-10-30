//! What a removal will do, decided before anything is done. The plan is an
//! ordered list of steps, each naming the steps it waits on, and a list of
//! reports: things found that this removal will not touch, each with who owns
//! it and what a person can do about it. The same plan is the preview, the
//! page the person reads before saying yes, and the worker's script, and the
//! worker re-plans and refuses if the machine no longer matches what was read.
use super::blocks::{self, KeyBlock};
use super::guard::{protection, Machine, Protection};
use super::known::{self, Line, Ownership, Receipt};
use super::mcp;
use super::roots::Roots;
use super::settings;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub(crate) const SCHEMA: &str = "td-uninstall-plan/1";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Retention {
    /// Remove the application, keep settings, sessions and records. The default.
    Keep,
    /// Export settings and records, verify the export, then remove them too.
    Export,
    /// Remove settings and records without an export.
    Delete,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub(crate) enum Op {
    Export {
        dest: PathBuf,
    },
    DefaultTerminalOff {
        list: PathBuf,
        journal: PathBuf,
    },
    /// Fails while TD is still listed as a default terminal: the program and
    /// its desktop entry wait on it, so the desktop is never left with none.
    CheckDefaultTerminal {
        list: PathBuf,
    },
    EditSettings {
        file: PathBuf,
        sha256: String,
        commands: Vec<String>,
    },
    McpRemove {
        client: String,
        name: String,
        argv: Vec<String>,
        cwd: Option<PathBuf>,
        /// The client's configuration file, copied before its CLI edits it.
        file: PathBuf,
        /// The entry as reviewed. It is read again just before removal, and a
        /// replacement under the same name is left alone.
        scope: String,
        project: Option<PathBuf>,
        command: String,
        args: Vec<String>,
    },
    StripKeyBlock {
        file: PathBuf,
        sha256: String,
        dest: String,
    },
    CloseHosts {
        dir: PathBuf,
    },
    StopProcesses {
        exes: Vec<PathBuf>,
        /// TD's builds directory: a process running a `td-*` build from it is
        /// TD's even when that build has since been replaced or removed.
        builds: PathBuf,
    },
    ThawScopes {
        sessions: Vec<String>,
    },
    RemoveFile {
        path: PathBuf,
        sha256: Option<String>,
    },
    RemoveLink {
        path: PathBuf,
        target: PathBuf,
    },
    RemoveTree {
        path: PathBuf,
    },
    /// The person's settings or records, removed only when they chose so.
    RemoveRecords {
        path: PathBuf,
    },
    RemoveLooseAssets {
        dir: PathBuf,
        /// Only the demo layouts TD writes to the temp directory whatever the
        /// runtime directory, not the pictures and sounds.
        demos_only: bool,
    },
    RemoveDirIfEmpty {
        path: PathBuf,
    },
    RefreshCaches {
        applications: PathBuf,
        icons: PathBuf,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct Step {
    pub(crate) op: Op,
    pub(crate) why: String,
    /// Steps that must have finished first. A failed one blocks this one.
    pub(crate) after: Vec<usize>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct Report {
    pub(crate) what: String,
    pub(crate) path: Option<PathBuf>,
    /// `td`, `shared`, `sibling`, `yours` or `unknown`.
    pub(crate) owner: String,
    pub(crate) why: String,
    pub(crate) manual: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct Size {
    pub(crate) what: String,
    pub(crate) path: PathBuf,
    /// Absent when the directory is not there: unknown is not zero.
    pub(crate) bytes: Option<u64>,
    pub(crate) files: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Plan {
    pub(crate) schema: String,
    pub(crate) stamp: String,
    pub(crate) machine_id: Option<String>,
    pub(crate) hostname: Option<String>,
    pub(crate) home: PathBuf,
    pub(crate) retention: Retention,
    /// Why this machine refuses removal, when it does.
    pub(crate) protected: Option<String>,
    /// Why this plan cannot run anywhere: TD's directories overlap, or a run
    /// on a home that is not the account's reaches outside it.
    pub(crate) blocked: Option<String>,
    pub(crate) appimage: Option<PathBuf>,
    pub(crate) steps: Vec<Step>,
    pub(crate) reports: Vec<Report>,
    pub(crate) sizes: Vec<Size>,
    pub(crate) warnings: Vec<String>,
}

pub(crate) struct Inputs<'a> {
    pub(crate) roots: &'a Roots,
    pub(crate) env: &'a dyn Fn(&str) -> Option<OsString>,
    pub(crate) machine: &'a Machine,
    pub(crate) retention: Retention,
    pub(crate) stamp: &'a str,
}

/// Helpers the source installers put on PATH, with the component that owns each.
const HELPERS: [(&str, &str); 6] = [
    ("td-agent-hooks", "agent-hooks"),
    ("td-agent-ledger", "recovery-hook"),
    ("td-keybindings", "keys-hotkey"),
    ("td-send", "send-hotkey"),
    ("leanctx-mcp", "plugins"),
    ("td-send-mcp", "plugins"),
];

/// Tools from the Omarchy companion repositories that share TD's `td-` names.
const SIBLING_BINS: [&str; 5] = [
    "td-tint",
    "td-monitor",
    "td-mcp",
    "td-tubes",
    "td-mobile-gateway",
];

/// Children of TD's data directory that TD itself writes.
const DATA_OWN: [&str; 3] = ["skills", "keys.html", "demo-logos"];

struct Builder {
    steps: Vec<Step>,
    reports: Vec<Report>,
}

impl Builder {
    fn step(&mut self, op: Op, why: impl Into<String>, after: Vec<usize>) -> usize {
        self.steps.push(Step {
            op,
            why: why.into(),
            after,
        });
        self.steps.len() - 1
    }
    fn report(
        &mut self,
        what: impl Into<String>,
        path: Option<PathBuf>,
        owner: &str,
        why: impl Into<String>,
        manual: Option<String>,
    ) {
        self.reports.push(Report {
            what: what.into(),
            path,
            owner: owner.into(),
            why: why.into(),
            manual,
        });
    }
}

pub(crate) fn plan(i: &Inputs) -> Plan {
    let r = i.roots;
    let get = |k: &str| {
        (i.env)(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let receipts = known::receipts(&r.state);
    let table = known::known();
    let mut b = Builder {
        steps: Vec::new(),
        reports: Vec::new(),
    };
    let mut integrations = Vec::new();

    let protected = match protection(r, i.machine) {
        p @ (Protection::ThisMachine { .. } | Protection::Unreadable { .. }) => Some(describe(&p)),
        Protection::OtherMachine { marker, names } => {
            b.report(
                "A protection marker for another machine",
                Some(marker),
                "yours",
                format!(
                    "it names {names}, not this machine, so it was copied here and is not obeyed"
                ),
                None,
            );
            None
        }
        Protection::None => None,
    };

    // Records are exported after TD has stopped, so the export holds the
    // sessions the hosts save on their way out, and are removed only after the
    // export is verified. A failed export keeps the records; the application
    // is still removed, which is what was asked.
    let first: Vec<usize> = Vec::new();

    // 1. Undo what TD changed in shared settings, while the binary still exists.
    let list = r.config_base.join("xdg-terminals.list");
    let journal = r.state.join("default-terminal.journal");
    let mut dt_step = None;
    if journal.is_file() {
        let n = b.step(
            Op::DefaultTerminalOff {
                list: list.clone(),
                journal: journal.clone(),
            },
            "put back the default terminal you had before TD, unless you changed it since",
            first.clone(),
        );
        integrations.push(n);
        dt_step = Some(n);
    } else if std::fs::read_to_string(&list)
        .is_ok_and(|t| t.lines().any(|l| l.trim() == "terminal-delight.desktop"))
    {
        b.report(
            "TD is listed as a default terminal",
            Some(list.clone()),
            "shared",
            "there is no record of what was there before, so nothing is guessed",
            Some(format!(
                "remove the line terminal-delight.desktop from {}",
                list.display()
            )),
        );
    }

    let mut blocked_helpers: Vec<String> = Vec::new();
    let mut settings_steps: Vec<usize> = Vec::new();
    let mut candidates = Vec::new();
    if let Some(p) = get("CLAUDE_SETTINGS") {
        candidates.push(p);
    }
    if let Some(d) = get("CLAUDE_CONFIG_DIR") {
        candidates.push(d.join("settings.json"));
    }
    candidates.push(r.home.join(".claude/settings.json"));
    for rc in &receipts {
        for l in &rc.lines {
            if let Line::Hook { settings, .. } = l {
                candidates.push(settings.clone());
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    for file in candidates {
        let bytes = match std::fs::read(&file) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                blocked_helpers.extend(HELPERS[..2].iter().map(|(n, _)| n.to_string()));
                b.report(
                    "Claude settings TD could not read",
                    Some(file.clone()),
                    "shared",
                    format!("{e}; TD's hooks in it, if any, stay, and so do the helpers they run"),
                    Some("make the file readable, then run the uninstall again".into()),
                );
                continue;
            }
        };
        let mut commands: Vec<String> = receipts
            .iter()
            .flat_map(|rc| rc.lines.iter())
            .filter_map(|l| match l {
                Line::Hook { settings, command } if *settings == file => Some(command.clone()),
                _ => None,
            })
            .collect();
        for (name, _) in &HELPERS[..2] {
            commands.push(r.bin.join(name).to_string_lossy().into_owned());
        }
        commands.sort();
        commands.dedup();
        let text = String::from_utf8_lossy(&bytes);
        match settings::strip(&text, &commands, true) {
            Err(e) => {
                blocked_helpers.extend(HELPERS[..2].iter().map(|(n, _)| n.to_string()));
                b.report(
                    "Claude settings TD could not read",
                    Some(file.clone()),
                    "shared",
                    format!("{e}; TD's hooks in it, if any, stay, and so do the helpers they run"),
                    Some("fix the file, then run the uninstall again".into()),
                );
            }
            Ok(s) => {
                for f in &s.foreign_refs {
                    for (n, _) in &HELPERS[..2] {
                        if f.contains(n) {
                            blocked_helpers.push(n.to_string());
                        }
                    }
                    b.report(
                        "A hook that is not TD's mentions a TD helper",
                        Some(file.clone()),
                        "yours",
                        format!("{f} — it stays, and so does the helper it runs"),
                        Some("edit or remove that hook yourself".into()),
                    );
                }
                if s.text.is_some() {
                    let n = b.step(
                        Op::EditSettings {
                            file: file.clone(),
                            sha256: known::sha256_bytes(&bytes),
                            commands: commands.clone(),
                        },
                        format!(
                            "take out TD's entries ({} hook{}{}) and nothing else",
                            s.removed_hooks.len(),
                            if s.removed_hooks.len() == 1 { "" } else { "s" },
                            if s.removed_allow.is_empty() {
                                ""
                            } else {
                                ", the MCP allow rule"
                            }
                        ),
                        first.clone(),
                    );
                    integrations.push(n);
                    settings_steps.push(n);
                }
                report_backups(&mut b, &file);
            }
        }
    }

    // MCP registrations, through each client's own CLI.
    let appimage = proven_appimage(&get);
    let mut td_exes = td_executables(r, appimage.as_ref().map(|a| a.0.as_path()));
    // The program inside a mounted AppImage is what its processes run.
    if let Some((_, payload)) = &appimage {
        td_exes.push(payload.clone());
        td_exes.sort();
        td_exes.dedup();
    }
    let appimage = appimage.map(|a| a.0);
    let (entries, problems) = mcp::discover(r, i.env, &td_exes);
    for e in entries {
        if e.td {
            let (argv, cwd) = e.remove_argv();
            integrations.push(b.step(
                Op::McpRemove {
                    client: e.client.cli().into(),
                    name: e.name.clone(),
                    argv,
                    cwd,
                    file: e.file.clone(),
                    scope: e.scope.into(),
                    project: e.project.clone(),
                    command: e.command.clone(),
                    args: e.args.clone(),
                },
                format!(
                    "TD's MCP registration in {} ({} scope)",
                    e.client.cli(),
                    e.scope
                ),
                first.clone(),
            ));
        } else {
            b.report(
                format!(
                    "An MCP entry \"{}\" in {} that runs {}",
                    e.name,
                    e.client.cli(),
                    e.command
                ),
                Some(e.file.clone()),
                "yours",
                "it points at a wrapper, not at TD itself, so it is yours to keep or remove",
                Some(e.manual()),
            );
        }
    }
    for p in problems {
        b.report(
            "Agent configuration TD could not read",
            None,
            "shared",
            p,
            None,
        );
    }
    b.report(
        "Project .mcp.json files",
        None,
        "yours",
        "a project-scope TD registration lives inside that project's repository, which this does not search",
        Some("grep -rl --include=.mcp.json terminal-delight <your projects directory>".into()),
    );

    // The key-sheet block.
    let block_line = receipts
        .iter()
        .flat_map(|rc| rc.lines.iter())
        .find_map(|l| match l {
            Line::Block { file, open } if open == blocks::OPEN => Some(file.clone()),
            _ => None,
        });
    let bindings = block_line.unwrap_or_else(|| r.config_base.join("hypr/bindings.lua"));
    let key_dest = receipt_file(&receipts, "td-keybindings")
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| r.bin.clone());
    let mut key_step = None;
    if let Ok(text) = std::fs::read_to_string(&bindings) {
        match blocks::strip_key_block(&text, &key_dest.to_string_lossy()) {
            KeyBlock::Absent => {}
            KeyBlock::Owned(_) => {
                let n = b.step(
                    Op::StripKeyBlock {
                        file: bindings.clone(),
                        sha256: known::sha256_bytes(text.as_bytes()),
                        dest: key_dest.to_string_lossy().into_owned(),
                    },
                    "remove the SUPER+ALT+T key-sheet binding TD added",
                    first.clone(),
                );
                integrations.push(n);
                key_step = Some(n);
            }
            KeyBlock::Refused(why) => {
                blocked_helpers.push("td-keybindings".into());
                b.report(
                    "The key-sheet block",
                    Some(bindings.clone()),
                    "shared",
                    why,
                    Some("delete the block between the td-keybindings markers by hand".into()),
                );
            }
        }
    }
    report_hyprland(&mut b, r);

    // 2. Stop TD. Hosts first, so each saves its session; then windows and
    //    relays by executable path; then thaw what rest froze.
    //
    // From here on, a step that removes a directory or reads its sockets is
    // planned whether or not the directory exists now: each handles absence
    // as "already gone". A plan that depended on it would change when TD
    // writes its own runtime files, or when the plan itself is written into
    // TD's cache for review, and the worker would refuse a plan nobody changed.
    let mut stop = Vec::new();
    for dir in host_dirs(r) {
        stop.push(b.step(
            Op::CloseHosts { dir },
            "ask each session host to save and close",
            integrations.clone(),
        ));
    }
    let mut after_hosts = integrations.clone();
    after_hosts.extend(&stop);
    let kill = b.step(
        Op::StopProcesses {
            exes: td_exes.clone(),
            builds: r.builds.clone(),
        },
        "close TD's windows and the MCP relays agents started, matched by executable",
        after_hosts,
    );
    let sessions = session_keys(&r.config);
    let thaw = (!sessions.is_empty()).then(|| {
        b.step(
            Op::ThawScopes { sessions },
            "wake panes that rest had frozen, so they can close",
            vec![kill],
        )
    });
    let mut gone: Vec<usize> = vec![kill];
    gone.extend(thaw);

    // The program and its desktop entry wait on TD no longer being a default
    // terminal, whatever the restore above managed.
    let check = b.step(
        Op::CheckDefaultTerminal { list: list.clone() },
        "make sure the desktop still has a terminal once TD is gone",
        dt_step.into_iter().chain(gone.iter().copied()).collect(),
    );
    let mut last = gone.clone();
    last.push(check);
    let export = (i.retention == Retention::Export).then(|| {
        b.step(
            Op::Export {
                dest: r.home.join(format!("terminal-delight-export-{}", i.stamp)),
            },
            "your settings and records, copied and verified after TD stopped and saved them",
            gone.clone(),
        )
    });

    // 3. Remove TD's files.
    let mut removed_for: Vec<(String, usize)> = Vec::new();
    for (name, _component) in HELPERS {
        let mut paths = vec![r.bin.join(name)];
        if let Some(p) = receipt_file(&receipts, name) {
            if !paths.contains(&p) {
                paths.push(p);
            }
        }
        for path in paths {
            if std::fs::symlink_metadata(&path).is_err() {
                continue;
            }
            let (own, hash) = known::ownership(&path, &receipts, &table);
            if blocked_helpers.iter().any(|h| h.as_str() == name) {
                b.report(
                    format!("{name}, kept"),
                    Some(path),
                    "td",
                    "something still uses it (see above)",
                    None,
                );
            } else if own.owned() {
                let mut after = gone.clone();
                if matches!(name, "td-agent-hooks" | "td-agent-ledger") {
                    after.extend(&settings_steps);
                }
                if name == "td-keybindings" {
                    after.extend(key_step);
                }
                let n = b.step(
                    Op::RemoveFile { path, sha256: hash },
                    format!("{name}, installed by TD ({})", own_label(&own)),
                    after,
                );
                removed_for.push((name.to_string(), n));
            } else {
                b.report(
                    format!("A file named {name}"),
                    Some(path),
                    "unknown",
                    format!(
                        "it does not match any version TD shipped ({})",
                        own_label(&own)
                    ),
                    Some("if it is TD's, delete it by hand".into()),
                );
            }
        }
    }
    let keys_html =
        receipt_file(&receipts, "keys.html").unwrap_or_else(|| r.data.join("keys.html"));
    if keys_html.exists() && !blocked_helpers.iter().any(|h| h == "td-keybindings") {
        let (own, hash) = known::ownership(&keys_html, &receipts, &table);
        if own.owned() {
            b.step(
                Op::RemoveFile {
                    path: keys_html,
                    sha256: hash,
                },
                "the key sheet's page",
                gone.clone(),
            );
        }
    }

    desktop(&mut b, r, &receipts, &table, &last);

    // TD writes these into the temp directory only when there is no runtime
    // directory; with one, a file of that name in a shared /tmp is somebody
    // else's.
    let loose_dir = r.runtime_base.clone().unwrap_or_else(|| r.temp.clone());
    if r.runtime_base.is_some() {
        b.step(
            Op::RemoveLooseAssets {
                dir: r.temp.clone(),
                demos_only: true,
            },
            "demo layouts TD left in the temp directory",
            gone.clone(),
        );
    }
    b.step(
        Op::RemoveLooseAssets {
            dir: loose_dir,
            demos_only: false,
        },
        "pictures and sounds TD unpacked beside its runtime directory",
        gone.clone(),
    );
    for dir in host_dirs(r) {
        b.step(
            Op::RemoveTree { path: dir },
            "TD's runtime directory: sockets and live state",
            gone.clone(),
        );
    }
    b.step(
        Op::RemoveTree {
            path: r.cache.clone(),
        },
        "TD's cache",
        gone.clone(),
    );
    if let Ok(rd) = std::fs::read_dir(&r.data) {
        let mut data_steps = Vec::new();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name == "keys.html" {
                continue;
            }
            if DATA_OWN.contains(&name.as_str()) {
                data_steps.push(b.step(
                    Op::RemoveTree { path: e.path() },
                    format!("TD's {name}"),
                    gone.clone(),
                ));
            } else {
                let who = if name == "palettes" {
                    "the Omarchy tools' td-tint"
                } else {
                    "another program"
                };
                b.report(
                    format!("{name} in TD's data directory"),
                    Some(e.path()),
                    "sibling",
                    format!("written by {who}, so it stays"),
                    None,
                );
            }
        }
        b.step(
            Op::RemoveDirIfEmpty {
                path: r.data.clone(),
            },
            "TD's data directory, if nothing else is in it",
            data_steps,
        );
    }

    // Builds, launcher, AppImage: last. They wait on TD having stopped and on
    // the default terminal being put back: removing the program while it is
    // still the terminal Omarchy opens would leave the person with none.
    builds_and_launcher(&mut b, r, appimage.as_deref(), &last);

    // Receipts of components that are now gone, when records are kept.
    if i.retention == Retention::Keep {
        for rc in &receipts {
            let files: Vec<usize> = rc
                .lines
                .iter()
                .filter_map(|l| match l {
                    Line::File { path, .. } => path.file_name().and_then(|n| n.to_str()),
                    _ => None,
                })
                .filter_map(|n| removed_for.iter().find(|(m, _)| m == n).map(|(_, s)| *s))
                .collect();
            if !files.is_empty() {
                b.step(
                    Op::RemoveFile {
                        path: rc.path.clone(),
                        sha256: None,
                    },
                    format!("the {} install receipt", rc.component),
                    files,
                );
            }
        }
    }

    // 4. Records, only when chosen, and only after everything that reads them.
    let mut sizes = Vec::new();
    for (what, p) in [
        ("settings and sessions", r.config.clone()),
        ("records", r.state.clone()),
        ("data", r.data.clone()),
        ("cache", r.cache.clone()),
        ("builds", r.builds.clone()),
    ] {
        let (bytes, files) = dir_size(&p);
        sizes.push(Size {
            what: what.into(),
            path: p,
            bytes,
            files,
        });
    }
    if i.retention == Retention::Keep {
        b.report(
            "Your settings, sessions and records are kept",
            Some(r.config.clone()),
            "yours",
            format!(
                "{} and {} stay where they are",
                r.config.display(),
                r.state.display()
            ),
            None,
        );
    } else {
        let mut all: Vec<usize> = (0..b.steps.len()).collect();
        all.extend(export);
        for p in [r.config.clone(), r.state.clone()] {
            b.step(
                Op::RemoveRecords { path: p },
                "your TD settings and records, as you chose",
                all.clone(),
            );
        }
        for legacy in [
            r.home.join(".config/terminal-delight"),
            r.home.join(".local/state/terminal-delight"),
        ] {
            if !legacy.exists() || legacy == r.config || legacy == r.state {
                continue;
            }
            if i.retention == Retention::Delete {
                b.step(
                    Op::RemoveRecords { path: legacy },
                    "an older copy of TD's settings or records under HOME",
                    all.clone(),
                );
            } else {
                // The export copies the current directories only; an older
                // copy it did not hold is kept rather than lost.
                b.report(
                    "An older copy of TD's settings or records",
                    Some(legacy),
                    "yours",
                    "the export holds the current ones only, so this copy is kept",
                    None,
                );
            }
        }
    }

    report_siblings(&mut b, r);
    if r.config.join("mobile/token").exists() {
        b.report(
            "A phone was paired with this TD",
            Some(r.config.join("mobile")),
            "td",
            "the phone keeps its copy of the pairing",
            Some("uninstall the Terminal Delight app on the phone".into()),
        );
    }

    Plan {
        schema: SCHEMA.into(),
        stamp: i.stamp.into(),
        machine_id: i.machine.id.clone(),
        hostname: i.machine.hostname.clone(),
        home: r.home.clone(),
        retention: i.retention,
        protected,
        blocked: blocked(r),
        appimage,
        steps: b.steps,
        reports: b.reports,
        sizes,
        warnings: r.warnings.clone(),
    }
}

fn describe(p: &Protection) -> String {
    match p {
        Protection::ThisMachine {
            marker,
            hostname,
            reason,
        } => format!(
            "this machine{} is protected by {}{}",
            hostname
                .as_deref()
                .map(|h| format!(" ({h})"))
                .unwrap_or_default(),
            marker.display(),
            reason
                .as_deref()
                .map(|r| format!(": {r}"))
                .unwrap_or_default()
        ),
        Protection::Unreadable { marker, why } => {
            format!(
                "{} exists and cannot be read as a protection marker ({why}); treated as protected",
                marker.display()
            )
        }
        _ => String::new(),
    }
}

fn own_label(o: &Ownership) -> String {
    match o {
        Ownership::Receipt(c) => format!("its {c} receipt"),
        Ownership::Shipped(p) => format!("a shipped {p}"),
        Ownership::Unrecognised => "content unrecognised".into(),
        Ownership::NotAFile(w) => w.clone(),
    }
}

fn receipt_file(receipts: &[Receipt], name: &str) -> Option<PathBuf> {
    receipts
        .iter()
        .flat_map(|rc| rc.lines.iter())
        .find_map(|l| match l {
            Line::File { path, .. } if path.file_name().and_then(|n| n.to_str()) == Some(name) => {
                Some(path.clone())
            }
            _ => None,
        })
}

/// Where session hosts and window sockets live: the runtime dir, and the two
/// different `/tmp` fallbacks TD uses when there is none.
pub(crate) fn host_dirs(r: &Roots) -> Vec<PathBuf> {
    let uid = unsafe { libc::getuid() };
    let mut v: Vec<PathBuf> = r.runtime().into_iter().collect();
    v.push(r.temp.join(format!("terminal-delight-{uid}")));
    v
}

/// The executables a running TD process could have: the launcher's target,
/// every build, the launcher itself if it is a binary, the AppImage.
/// The AppImage this TD came from, and the program inside it, only when the
/// AppImage runtime's own variables agree: `APPIMAGE` names a file with the
/// AppImage magic, and `APPDIR`, its mount, holds a terminal-delight program.
/// An `APPIMAGE` inherited from some other AppImage that started TD names
/// another application and is never taken as TD's.
fn proven_appimage(get: &dyn Fn(&str) -> Option<PathBuf>) -> Option<(PathBuf, PathBuf)> {
    use std::io::Read;
    let image = get("APPIMAGE")?;
    let payload = get("APPDIR")?.join("usr/bin/terminal-delight");
    let mut head = [0u8; 11];
    let magic = std::fs::File::open(&image)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && head[..4] == *b"\x7fELF"
        && head[8..11] == *b"AI\x02";
    if !(magic && is_elf(&payload)) {
        return None;
    }
    // The payload has to be this very program: the running TD itself, or the
    // worker's staged copy of it, byte for byte. An APPDIR and an APPIMAGE
    // that merely both exist prove nothing about each other or about TD.
    let me = std::env::current_exe().ok()?;
    let payload = std::fs::canonicalize(&payload).unwrap_or(payload);
    // And the image has to be the one mounted at APPDIR: the AppImage runtime
    // names its FUSE filesystem after the image it serves.
    let image_name = image.file_name()?.to_string_lossy().into_owned();
    let appdir = payload.parent()?.parent()?.parent()?.to_path_buf();
    let mounted = std::fs::read_to_string("/proc/self/mountinfo").is_ok_and(|info| {
        info.lines().any(|l| {
            let mut halves = l.splitn(2, " - ");
            let (pre, post) = (halves.next().unwrap_or(""), halves.next().unwrap_or(""));
            let point = pre.split(' ').nth(4).map(PathBuf::from);
            let mut fields = post.split(' ');
            let (fstype, source) = (fields.next().unwrap_or(""), fields.next().unwrap_or(""));
            point.as_deref() == Some(appdir.as_path())
                && fstype.starts_with("fuse")
                && (fstype.contains(&image_name) || source.contains(&image_name))
        })
    });
    if !mounted {
        return None;
    }
    let same = std::fs::canonicalize(&me).ok().as_deref() == Some(payload.as_path())
        || matches!(
            (known::sha256_file(&me), known::sha256_file(&payload)),
            (Ok(a), Ok(b)) if a == b
        );
    same.then_some((image, payload))
}

/// Why a plan for these roots must not run: two of TD's directories are one
/// directory (a cache root that is also the settings root would take the
/// settings with it), or, on a home that is not the account's, one of them
/// resolves outside that home.
fn blocked(r: &Roots) -> Option<String> {
    use super::roots::resolve_existing_prefix;
    let runtime = r.runtime();
    let fallback = host_dirs(r).pop().unwrap_or_default();
    let mut named: Vec<(&str, &PathBuf)> = vec![
        ("settings", &r.config),
        ("records", &r.state),
        ("data", &r.data),
        ("cache", &r.cache),
        ("builds", &r.builds),
        ("runtime fallback", &fallback),
    ];
    if let Some(rt) = &runtime {
        named.push(("runtime", rt));
    }
    // Older copies under HOME are kept whenever records are kept or exported,
    // so nothing else may land on them either.
    let legacy = [
        r.home.join(".config/terminal-delight"),
        r.home.join(".local/state/terminal-delight"),
    ];
    for (what, l) in [
        ("older settings", &legacy[0]),
        ("older records", &legacy[1]),
    ] {
        if l.exists() && *l != r.config && *l != r.state {
            named.push((what, l));
        }
    }
    for (i, (a, pa)) in named.iter().enumerate() {
        for (b, pb) in &named[i + 1..] {
            let (x, y) = (resolve_existing_prefix(pa), resolve_existing_prefix(pb));
            if x.starts_with(&y) || y.starts_with(&x) {
                return Some(format!(
                    "TD's {a} directory ({}) and its {b} directory ({}) overlap, so removing one would take the other",
                    pa.display(),
                    pb.display()
                ));
            }
        }
    }
    if super::guard::is_account_home(&r.home) {
        return None;
    }
    let home = std::fs::canonicalize(&r.home).unwrap_or_else(|_| r.home.clone());
    // A "fixture" home that holds the account's real home would reach the real
    // install, and the real protection marker would not be read.
    if let Some(real) = super::guard::passwd_home() {
        let real = std::fs::canonicalize(&real).unwrap_or(real);
        if real.starts_with(&home) {
            return Some(format!(
                "this home ({}) contains the account's own home, so a removal here could reach the real install",
                r.home.display()
            ));
        }
    }
    let mut all: Vec<&PathBuf> = vec![
        &r.config,
        &r.state,
        &r.data,
        &r.cache,
        &r.builds,
        &r.bin,
        &r.config_base,
        &r.data_base,
        &r.cache_base,
        &r.temp,
    ];
    all.extend(r.runtime_base.as_ref());
    all.into_iter()
        .find(|p| !resolve_existing_prefix(p).starts_with(&home))
        .map(|p| {
            format!(
                "this home is not the account's, and {} resolves outside it, so a removal here could reach real files",
                p.display()
            )
        })
}

pub(crate) fn td_executables(r: &Roots, appimage: Option<&Path>) -> Vec<PathBuf> {
    // Only executables proven to be TD's: an ELF build inside TD's own builds
    // directory (resolved), the launcher when it is itself such a program, and
    // the AppImage the running TD says it came from. A launcher that points
    // anywhere else names nothing: a link into another program, or into a
    // live install from a fixture, must never make that program a target.
    let mut v = Vec::new();
    let builds = std::fs::canonicalize(&r.builds).ok();
    let in_builds = |p: &Path| {
        builds
            .as_deref()
            .is_some_and(|b| std::fs::canonicalize(p).is_ok_and(|c| c.parent() == Some(b)))
    };
    if let Ok(rd) = std::fs::read_dir(&r.builds) {
        for e in rd.flatten() {
            let p = e.path();
            if e.file_name().to_string_lossy().starts_with("td-") && is_elf(&p) && in_builds(&p) {
                v.push(p);
            }
        }
    }
    let launcher = r.bin.join("terminal-delight");
    if let Ok(m) = std::fs::symlink_metadata(&launcher) {
        if m.file_type().is_symlink() {
            if let Ok(t) = std::fs::read_link(&launcher) {
                let t = if t.is_absolute() { t } else { r.bin.join(t) };
                let is_appimage = appimage.is_some_and(|a| {
                    std::fs::canonicalize(a).ok() == std::fs::canonicalize(&t).ok()
                });
                if (in_builds(&t) && is_elf(&std::fs::canonicalize(&t).unwrap_or(t.clone())))
                    || is_appimage
                {
                    v.push(t);
                }
            }
        } else if is_elf(&launcher) {
            v.push(launcher.clone());
        }
    }
    v.extend(appimage.filter(|a| is_elf(a)).map(Path::to_path_buf));
    v.sort();
    v.dedup();
    v
}

pub(crate) fn is_elf(p: &Path) -> bool {
    use std::io::Read;
    let Ok(m) = std::fs::symlink_metadata(p) else {
        return false;
    };
    if !m.is_file() {
        return false;
    }
    let mut head = [0u8; 4];
    std::fs::File::open(p)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && head == *b"\x7fELF"
}

fn session_keys(config: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(config.join("sessions"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(".toml")
                .map(String::from)
        })
        .collect();
    v.sort();
    v
}

pub(crate) fn dir_size(p: &Path) -> (Option<u64>, Option<u64>) {
    let Ok(m) = std::fs::symlink_metadata(p) else {
        return (None, None);
    };
    if !m.is_dir() {
        return (Some(m.len()), Some(1));
    }
    let (mut bytes, mut files) = (0u64, 0u64);
    let mut stack = vec![p.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() {
                stack.push(e.path());
            } else {
                bytes += m.len();
                files += 1;
            }
        }
    }
    (Some(bytes), Some(files))
}

fn report_backups(b: &mut Builder, settings_file: &Path) {
    let (Some(dir), Some(name)) = (settings_file.parent(), settings_file.file_name()) else {
        return;
    };
    let prefix = format!("{}.td-backup.", name.to_string_lossy());
    let n = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .count();
    if n > 0 {
        b.report(
            format!("{n} backup copies of your Claude settings that TD's scripts made"),
            Some(dir.to_path_buf()),
            "yours",
            "each is a whole copy of your settings, env included; they are yours to keep or delete",
            Some(format!("ls {}/{prefix}*", dir.display())),
        );
    }
}

fn desktop(
    b: &mut Builder,
    r: &Roots,
    receipts: &[Receipt],
    table: &std::collections::BTreeMap<String, Vec<String>>,
    gone: &[usize],
) {
    let mut bases = vec![r.home.join(".local/share"), r.data_base.clone()];
    bases.dedup();
    for base in bases {
        let entry = base.join("applications/terminal-delight.desktop");
        let svg = base.join("icons/hicolor/scalable/apps/terminal-delight.svg");
        let mut touched = Vec::new();
        for p in [&entry, &svg] {
            if !p.exists() {
                continue;
            }
            let (own, hash) = known::ownership(p, receipts, table);
            if own.owned() {
                touched.push(b.step(
                    Op::RemoveFile {
                        path: p.clone(),
                        sha256: hash,
                    },
                    "TD's desktop entry or icon",
                    gone.to_vec(),
                ));
            } else {
                b.report(
                    "A Terminal Delight desktop file TD did not ship",
                    Some(p.clone()),
                    "unknown",
                    own_label(&own),
                    None,
                );
            }
        }
        if !touched.is_empty() {
            for s in [48, 64, 128, 256] {
                let png = base.join(format!("icons/hicolor/{s}x{s}/apps/terminal-delight.png"));
                if !png_file(&png) {
                    continue;
                }
                // The PNGs are rendered on the person's machine, so no shipped
                // hash can name them; only the installer's receipt can.
                let (own, hash) = known::ownership(&png, receipts, table);
                if own.owned() {
                    touched.push(b.step(
                        Op::RemoveFile {
                            path: png,
                            sha256: hash,
                        },
                        "an icon TD's installer rendered",
                        gone.to_vec(),
                    ));
                } else {
                    b.report(
                        "A Terminal Delight icon with no install receipt",
                        Some(png.clone()),
                        "unknown",
                        "probably rendered from TD's own by an older installer, but nothing proves it",
                        Some(format!("rm {}", png.display())),
                    );
                }
            }
            b.step(
                Op::RefreshCaches {
                    applications: base.join("applications"),
                    icons: base.join("icons/hicolor"),
                },
                "refresh the desktop and icon caches TD's installer updated",
                touched,
            );
        }
    }
}

fn png_file(p: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 8];
    std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file())
        && std::fs::File::open(p)
            .and_then(|mut f| f.read_exact(&mut head))
            .is_ok()
        && head == *b"\x89PNG\r\n\x1a\n"
}

fn builds_and_launcher(b: &mut Builder, r: &Roots, appimage: Option<&Path>, after: &[usize]) {
    let mut gone = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&r.builds) {
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("td-") && is_elf(&e.path()) {
                gone.push(b.step(
                    Op::RemoveFile {
                        path: e.path(),
                        sha256: None,
                    },
                    format!("build {name}"),
                    after.to_vec(),
                ));
            } else {
                b.report(
                    format!("{name} in TD's builds directory"),
                    Some(e.path()),
                    "unknown",
                    "not a TD build, so it stays",
                    None,
                );
            }
        }
        let n = b.step(
            Op::RemoveDirIfEmpty {
                path: r.builds.clone(),
            },
            "the builds directory, if empty",
            gone.clone(),
        );
        gone.push(n);
    }
    let launcher = r.bin.join("terminal-delight");
    if let Ok(m) = std::fs::symlink_metadata(&launcher) {
        if m.file_type().is_symlink() {
            let t = std::fs::read_link(&launcher).unwrap_or_default();
            let abs = if t.is_absolute() {
                t.clone()
            } else {
                r.bin.join(&t)
            };
            if abs.starts_with(&r.builds) || Some(abs.as_path()) == appimage {
                b.step(
                    Op::RemoveLink {
                        path: launcher.clone(),
                        target: t,
                    },
                    "the terminal-delight launcher",
                    after.to_vec(),
                );
            } else {
                b.report(
                    "The terminal-delight launcher points somewhere unexpected",
                    Some(launcher.clone()),
                    "unknown",
                    format!("it points at {}", abs.display()),
                    Some(format!("rm {}", launcher.display())),
                );
            }
        } else if is_elf(&launcher) {
            b.step(
                Op::RemoveFile {
                    path: launcher.clone(),
                    sha256: None,
                },
                "the terminal-delight program",
                after.to_vec(),
            );
        }
    }
    if let Some(a) = appimage {
        if a != launcher && is_elf(a) {
            b.step(
                Op::RemoveFile {
                    path: a.to_path_buf(),
                    sha256: None,
                },
                "the AppImage you downloaded",
                after.to_vec(),
            );
        }
    }
    if let Ok(rd) = std::fs::read_dir(&r.bin) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("terminal-delight") && name != "terminal-delight" {
                b.report(
                    format!("{name} beside the launcher"),
                    Some(e.path()),
                    "unknown",
                    "named like TD but not something TD installs",
                    Some(format!("rm {}", e.path().display())),
                );
            } else if SIBLING_BINS.contains(&name.as_str()) {
                b.report(name.clone(), Some(e.path()), "sibling", if name == "td-mobile-gateway" { "the phone gateway from TD's mobile branch; stop its service and remove it by hand" } else { "a tool from the Omarchy companion repository, removed by its own uninstall" }, None);
            }
        }
    }
}

fn report_hyprland(b: &mut Builder, r: &Roots) {
    let dir = r.config_base.join("hypr");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<_> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| matches!(p.extension().and_then(|x| x.to_str()), Some("lua" | "conf")))
        .collect();
    files.sort();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        let mut in_key_block = false;
        let mut hits = Vec::new();
        for (n, line) in text.lines().enumerate() {
            if line == blocks::OPEN {
                in_key_block = true;
            }
            if !in_key_block
                && (line.contains("terminal-delight") || line.contains("td-terminal-tag"))
                && !line.trim_start().starts_with("--")
            {
                hits.push(format!("line {}: {}", n + 1, line.trim()));
            }
            if line == blocks::SHUT {
                in_key_block = false;
            }
        }
        if !hits.is_empty() {
            b.report(
                format!(
                    "{} line(s) in {} that use TD",
                    hits.len(),
                    f.file_name().unwrap_or_default().to_string_lossy()
                ),
                Some(f.clone()),
                "yours",
                format!(
                    "not written by an installer TD can undo: {}",
                    hits.join("; ")
                ),
                Some("remove them by hand; remove a require line before the file it loads".into()),
            );
        }
    }
}

fn report_siblings(b: &mut Builder, r: &Roots) {
    let units = r.config_base.join("systemd/user");
    if let Ok(rd) = std::fs::read_dir(&units) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("td-") && name.ends_with(".service") {
                b.report(format!("systemd user unit {name}"), Some(e.path()), "sibling", "a service from a TD companion project; stop and disable it with its own uninstall", Some(format!("systemctl --user disable --now {name}")));
            }
        }
    }
    let themes = r.config_base.join("omarchy/themes");
    let current = r
        .state
        .parent()
        .map(|s| s.join("omarchy/current/theme.name"));
    let current = current
        .and_then(|c| std::fs::read_to_string(c).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if let Ok(rd) = std::fs::read_dir(&themes) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("terminal-delight") {
                let cur = if name == current {
                    "; it is your current Omarchy theme"
                } else {
                    ""
                };
                b.report(
                    format!("Omarchy theme {name}"),
                    Some(e.path()),
                    "sibling",
                    format!("the TD Omarchy theme is its own product{cur}"),
                    Some("switch theme first, then remove it with Omarchy".into()),
                );
            }
        }
    }
    if let Ok(rd) = std::fs::read_dir(r.config_base.join("omarchy/hooks")) {
        for d in rd.flatten() {
            for e in std::fs::read_dir(d.path()).into_iter().flatten().flatten() {
                if e.file_name().to_string_lossy().starts_with("td-") {
                    b.report(
                        "An Omarchy hook from a TD companion project",
                        Some(e.path()),
                        "sibling",
                        "it does nothing once its tool is gone",
                        None,
                    );
                }
            }
        }
    }
    let shared = r.cache_base.join("omarchy/agent-usage");
    if shared.exists() {
        b.report(
            "Omarchy's agent-usage cache",
            Some(shared),
            "shared",
            "TD's usage collectors write here as Omarchy's do, and the files cannot be told apart",
            None,
        );
    }
    let cdx = r.data_base.join("context-delight");
    if cdx.exists() {
        b.report(
            "Context Delight packages",
            Some(cdx),
            "sibling",
            "TD pointed the context-delight plugin here; the packages are that plugin's",
            None,
        );
    }
    if r.config.join("hotkey-terminal-key.owned").exists() {
        b.report(
            "A GNOME Ctrl+Alt+T shortcut",
            Some(r.config.join("hotkey-terminal-key.owned")),
            "td",
            "GNOME removal is not qualified in this release",
            Some("scripts/install-hotkey.sh --uninstall from a terminal-delight checkout".into()),
        );
    }
}
