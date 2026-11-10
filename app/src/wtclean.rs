//! Worktree cleanup: which of this machine's git worktrees can go, and why.
//!
//! T3 Code cleans the worktrees its threads leave behind (Settings › Worktrees
//! › Cleanup: delete merged worktrees, delete inactive worktrees after N days,
//! delete unchanged worktrees, keep worktrees with local changes). Parker,
//! 2026-11-09: *"oh and the worktree cleanup -- yeah we should go for that
//! 100%"*. Agents here make worktrees by the dozen and nothing tidies them.
//!
//! Deleting a worktree is the one door here that does not swing back, so this
//! file keeps four promises, whatever the switches say:
//!
//! - **A worktree with local changes is never a candidate.** Modified, staged
//!   or untracked files keep it. So does a status git could not give: unknown
//!   is not clean.
//! - **A worktree anything is running in is never a candidate**: a pane, an
//!   agent in another window, a Codex session, a build. Every process of this
//!   user whose working directory is inside it keeps it, read from `/proc`
//!   at the look and again just before removal. Nor the main checkout, nor
//!   one git has locked.
//! - **Nothing goes without a person's second press** (the view arms, then
//!   removes), and removal is plain `git worktree remove`, without `--force`,
//!   so git refuses a dirty tree a second time on its own.
//! - **Branches are kept.** Only the checkout on disk goes.
//!
//! The rules are T3's, in git's terms: *merged* is a merged pull request for
//! the worktree's branch (asked of `gh`, because a squash merge leaves no
//! ancestry for git to see); *unchanged* is no commit beyond the default
//! branch; *inactive* is nothing committed, staged or checked out for N days.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The switches, as saved in `worktrees.toml`. Every rule is off until a
/// person switches it on; an absent switch is unset, which reads as off.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inactive: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inactive_days: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unchanged: Option<bool>,
}

/// T3's default for inactive worktrees.
pub const DEFAULT_DAYS: u32 = 5;

impl Prefs {
    pub fn merged_on(&self) -> bool {
        self.merged.unwrap_or(false)
    }
    pub fn inactive_on(&self) -> bool {
        self.inactive.unwrap_or(false)
    }
    pub fn unchanged_on(&self) -> bool {
        self.unchanged.unwrap_or(false)
    }
    pub fn days(&self) -> u32 {
        self.inactive_days.unwrap_or(DEFAULT_DAYS).clamp(1, 365)
    }
    pub fn any_on(&self) -> bool {
        self.merged_on() || self.inactive_on() || self.unchanged_on()
    }
}

pub fn prefs_path(config: &Path) -> PathBuf {
    config.join("worktrees.toml")
}

pub fn load(config: &Path) -> Prefs {
    std::fs::read_to_string(prefs_path(config))
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(config: &Path, p: &Prefs) -> Result<(), String> {
    let text = toml::to_string(p).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(config).map_err(|e| e.to_string())?;
    let path = prefs_path(config);
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// One entry of `git worktree list --porcelain`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Listed {
    pub path: PathBuf,
    /// The branch's short name; `None` when detached.
    pub branch: Option<String>,
    pub bare: bool,
    pub locked: bool,
    pub prunable: bool,
}

/// `git worktree list --porcelain`, entry by entry. The first is the main
/// worktree, as git always lists it.
pub fn parse_list(text: &str) -> Vec<Listed> {
    let mut out = Vec::new();
    let mut cur: Option<Listed> = None;
    for line in text.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            out.extend(cur.take());
            cur = Some(Listed {
                path: PathBuf::from(p),
                ..Default::default()
            });
            continue;
        }
        let Some(c) = cur.as_mut() else { continue };
        if let Some(b) = line.strip_prefix("branch ") {
            c.branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
        } else if line == "bare" {
            c.bare = true;
        } else if line == "locked" || line.starts_with("locked ") {
            c.locked = true;
        } else if line == "prunable" || line.starts_with("prunable ") {
            c.prunable = true;
        }
    }
    out.extend(cur);
    out
}

/// What is known about one worktree. Every reading git could not give is
/// `None`, and the judge treats it as the reason to keep.
#[derive(Clone, Debug, PartialEq)]
pub struct Facts {
    /// The main checkout of its repository.
    pub repo: PathBuf,
    pub path: PathBuf,
    pub branch: Option<String>,
    /// This IS the main checkout.
    pub primary: bool,
    pub locked: bool,
    /// Modified, staged or untracked files.
    pub dirty: Option<bool>,
    /// Commits on HEAD that the default branch does not have.
    pub ahead: Option<u32>,
    /// The default branch it was measured against (`origin/main`).
    pub base: Option<String>,
    /// Its branch has a merged pull request.
    pub pr_merged: Option<bool>,
    /// The latest of its last commit, its index and its HEAD log.
    pub touched: Option<SystemTime>,
    /// A pane's working directory is inside it.
    pub occupied: bool,
    /// Ignored entries removal would also delete (`target/`, `.env`), at most
    /// a handful, top level only.
    pub ignored: Vec<String>,
}

/// Why a worktree stays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Keep {
    Primary,
    Occupied,
    Locked,
    Changes,
    Unread,
    NoRule,
}

/// Why a worktree can go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Why {
    Merged,
    Inactive { days: u32 },
    Unchanged,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Keep(Keep),
    Remove(Vec<Why>),
}

impl Verdict {
    pub fn removes(&self) -> bool {
        matches!(self, Verdict::Remove(_))
    }
}

/// The verdict on one worktree under `p` at `now`. The guards come first and
/// no switch can reach past them.
pub fn judge(f: &Facts, p: &Prefs, now: SystemTime) -> Verdict {
    if f.primary {
        return Verdict::Keep(Keep::Primary);
    }
    if f.occupied {
        return Verdict::Keep(Keep::Occupied);
    }
    if f.locked {
        return Verdict::Keep(Keep::Locked);
    }
    match f.dirty {
        Some(true) => return Verdict::Keep(Keep::Changes),
        None => return Verdict::Keep(Keep::Unread),
        Some(false) => {}
    }
    let mut why = Vec::new();
    if p.merged_on() && f.pr_merged == Some(true) {
        why.push(Why::Merged);
    }
    if p.inactive_on() {
        if let Some(age) = f.touched.and_then(|t| now.duration_since(t).ok()) {
            let days = (age.as_secs() / 86_400) as u32;
            if days >= p.days() {
                why.push(Why::Inactive { days });
            }
        }
    }
    if p.unchanged_on() && f.ahead == Some(0) {
        why.push(Why::Unchanged);
    }
    if why.is_empty() {
        Verdict::Keep(Keep::NoRule)
    } else {
        Verdict::Remove(why)
    }
}

// ---- reading the disk ------------------------------------------------------

const GIT_BUDGET: Duration = Duration::from_secs(8);

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let (code, out) =
        crate::engstate::run_git_bytes(dir, args, GIT_BUDGET, 4 * 1024 * 1024).ok()?;
    (code == 0).then(|| String::from_utf8_lossy(&out).into_owned())
}

/// The main checkout of the repository `dir` is in.
pub fn repo_of(dir: &Path) -> Option<PathBuf> {
    let common = git(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let common = PathBuf::from(common.trim());
    if common.file_name().is_some_and(|n| n == ".git") {
        common.parent().map(Path::to_path_buf)
    } else {
        None
    }
}

/// The default branch to measure against: origin's HEAD, else a local
/// `main` or `master`.
fn default_ref(repo: &Path) -> Option<String> {
    if let Some(r) = git(
        repo,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    ) {
        let r = r.trim();
        if !r.is_empty() {
            return Some(r.to_string());
        }
    }
    ["main", "master"]
        .into_iter()
        .find(|b| git(repo, &["rev-parse", "--verify", "--quiet", b]).is_some())
        .map(str::to_string)
}

/// Branches with a merged pull request, asked of `gh` once per repository.
/// `None` when gh cannot say: no remote on GitHub, no login, no network.
fn merged_branches(repo: &Path) -> Option<HashSet<String>> {
    let ran = crate::engstate::run_bounded(
        "gh",
        repo,
        &[
            "pr",
            "list",
            "--state",
            "merged",
            "--json",
            "headRefName",
            "--limit",
            "400",
        ],
        &[("GH_PROMPT_DISABLED", "1")],
        Duration::from_secs(20),
        2 * 1024 * 1024,
        0,
    )
    .ok()?;
    if ran.code != 0 {
        return None;
    }
    #[derive(Deserialize)]
    struct Row {
        #[serde(rename = "headRefName")]
        head: String,
    }
    let rows: Vec<Row> = serde_json::from_slice(&ran.out).ok()?;
    Some(rows.into_iter().map(|r| r.head).collect())
}

/// The working directory of every process this user can see, from `/proc`.
/// Another user's processes are unreadable and skipped; they cannot be in a
/// worktree this user can remove anyway.
pub fn process_cwds() -> Vec<PathBuf> {
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    dir.flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .bytes()
                .all(|b| b.is_ascii_digit())
        })
        .filter_map(|e| std::fs::read_link(e.path().join("cwd")).ok())
        .collect()
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).ok()?.modified().ok()
}

/// Every worktree of the repository whose main checkout is `repo`, read.
/// `occupied` is every pane's working directory.
pub fn scan(repo: &Path, occupied: &[PathBuf]) -> Result<Vec<Facts>, String> {
    let list = git(repo, &["worktree", "list", "--porcelain"])
        .ok_or_else(|| format!("{}: git worktree list failed", repo.display()))?;
    let listed = parse_list(&list);
    let base = default_ref(repo);
    let merged = if listed.len() > 1 {
        merged_branches(repo)
    } else {
        None
    };
    let mut out = Vec::new();
    for (i, l) in listed.into_iter().enumerate() {
        if l.bare {
            continue;
        }
        let primary = i == 0;
        let gone = l.prunable || !l.path.is_dir();
        let status = if primary || gone {
            None
        } else {
            git(
                &l.path,
                &[
                    "status",
                    "--porcelain=v1",
                    "--ignored=matching",
                    "--untracked-files=normal",
                ],
            )
        };
        let dirty = status.as_ref().map(|s| {
            s.lines()
                .any(|line| !line.starts_with("!! ") && !line.is_empty())
        });
        let ignored: Vec<String> = status
            .as_ref()
            .map(|s| {
                s.lines()
                    .filter_map(|line| line.strip_prefix("!! "))
                    .take(4)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let ahead = match (&base, primary || gone) {
            (Some(b), false) => git(&l.path, &["rev-list", "--count", &format!("{b}..HEAD")])
                .and_then(|n| n.trim().parse().ok()),
            _ => None,
        };
        let touched = if primary || gone {
            None
        } else {
            let commit = git(&l.path, &["log", "-1", "--format=%ct", "HEAD"])
                .and_then(|s| s.trim().parse::<u64>().ok())
                .map(|t| SystemTime::UNIX_EPOCH + Duration::from_secs(t));
            let gitdir =
                git(&l.path, &["rev-parse", "--absolute-git-dir"]).map(|s| PathBuf::from(s.trim()));
            let index = gitdir.as_ref().and_then(|g| mtime(&g.join("index")));
            let log = gitdir.as_ref().and_then(|g| mtime(&g.join("logs/HEAD")));
            [commit, index, log].into_iter().flatten().max()
        };
        let pr_merged = match (&l.branch, &merged) {
            (Some(b), Some(set)) => Some(set.contains(b)),
            _ => None,
        };
        let occupied = occupied.iter().any(|d| d.starts_with(&l.path));
        out.push(Facts {
            repo: repo.to_path_buf(),
            path: l.path,
            branch: l.branch,
            primary,
            locked: l.locked,
            dirty: if gone { None } else { dirty },
            ahead,
            base: base.clone(),
            pr_merged,
            touched,
            occupied,
            ignored,
        });
    }
    Ok(out)
}

/// Remove one worktree the way a person would: `git worktree remove`, with no
/// `--force`, so git itself refuses a tree with changes. The branch stays.
pub fn remove(repo: &Path, path: &Path) -> Result<(), String> {
    let p = path.to_string_lossy();
    let ran = crate::engstate::run_bounded(
        "git",
        repo,
        &["worktree", "remove", &p],
        &[],
        Duration::from_secs(60),
        64 * 1024,
        4096,
    )?;
    if ran.code == 0 {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&ran.err).trim().to_string();
        Err(if err.is_empty() {
            format!("git worktree remove exited {}", ran.code)
        } else {
            err
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This process's own directory is among the ones read, so a worktree a
    /// shell or an agent is standing in is seen as occupied.
    #[test]
    fn a_running_process_is_found_standing_in_its_directory() {
        let here = std::env::current_dir().unwrap();
        let here = std::fs::canonicalize(&here).unwrap_or(here);
        assert!(process_cwds().iter().any(|c| c == &here), "{here:?}");
    }

    #[test]
    fn the_porcelain_list_is_read_entry_by_entry() {
        let text = "worktree /r\nHEAD aaa\nbranch refs/heads/main\n\n\
                    worktree /r/.wt/a\nHEAD bbb\nbranch refs/heads/feat/a\nlocked on a usb disk\n\n\
                    worktree /r/.wt/b\nHEAD ccc\ndetached\nprunable gitdir file points to non-existent location\n";
        let l = parse_list(text);
        assert_eq!(l.len(), 3);
        assert_eq!(l[0].branch.as_deref(), Some("main"));
        assert_eq!(l[1].branch.as_deref(), Some("feat/a"));
        assert!(l[1].locked && !l[1].prunable);
        assert_eq!(l[2].branch, None, "detached");
        assert!(l[2].prunable);
    }

    fn clean() -> Facts {
        Facts {
            repo: "/r".into(),
            path: "/r/.wt/a".into(),
            branch: Some("feat/a".into()),
            primary: false,
            locked: false,
            dirty: Some(false),
            ahead: Some(0),
            base: Some("origin/main".into()),
            pr_merged: Some(true),
            touched: Some(SystemTime::UNIX_EPOCH),
            occupied: false,
            ignored: vec![],
        }
    }

    fn all_on() -> Prefs {
        Prefs {
            merged: Some(true),
            inactive: Some(true),
            inactive_days: Some(5),
            unchanged: Some(true),
        }
    }

    /// The guards: no switch reaches past the main checkout, a pane in the
    /// tree, a lock, local changes, or a status git could not give.
    ///
    /// Mutation-tested: dropping the `None` arm (unknown read as clean), or
    /// the occupied check, each fails here.
    #[test]
    fn no_switch_reaches_past_the_guards() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(400 * 86_400);
        let p = all_on();
        assert!(
            judge(&clean(), &p, now).removes(),
            "the control: it would go"
        );
        for (f, keep) in [
            (
                Facts {
                    primary: true,
                    ..clean()
                },
                Keep::Primary,
            ),
            (
                Facts {
                    occupied: true,
                    ..clean()
                },
                Keep::Occupied,
            ),
            (
                Facts {
                    locked: true,
                    ..clean()
                },
                Keep::Locked,
            ),
            (
                Facts {
                    dirty: Some(true),
                    ..clean()
                },
                Keep::Changes,
            ),
            (
                Facts {
                    dirty: None,
                    ..clean()
                },
                Keep::Unread,
            ),
        ] {
            assert_eq!(judge(&f, &p, now), Verdict::Keep(keep));
        }
    }

    /// Every rule is off until switched on, each fires on its own fact, and
    /// an unknown fact fires nothing.
    #[test]
    fn each_rule_fires_on_its_own_fact_and_unknown_fires_none() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10 * 86_400);
        let f = clean();
        assert_eq!(
            judge(&f, &Prefs::default(), now),
            Verdict::Keep(Keep::NoRule)
        );
        let only = |merged, inactive, unchanged| Prefs {
            merged: Some(merged),
            inactive: Some(inactive),
            inactive_days: Some(5),
            unchanged: Some(unchanged),
        };
        assert_eq!(
            judge(&f, &only(true, false, false), now),
            Verdict::Remove(vec![Why::Merged])
        );
        assert_eq!(
            judge(&f, &only(false, true, false), now),
            Verdict::Remove(vec![Why::Inactive { days: 10 }])
        );
        assert_eq!(
            judge(&f, &only(false, false, true), now),
            Verdict::Remove(vec![Why::Unchanged])
        );
        let fresh = Facts {
            touched: Some(now - Duration::from_secs(86_400)),
            ..clean()
        };
        assert_eq!(
            judge(&fresh, &only(false, true, false), now),
            Verdict::Keep(Keep::NoRule)
        );
        let unknown = Facts {
            pr_merged: None,
            ahead: None,
            touched: None,
            ..clean()
        };
        assert_eq!(judge(&unknown, &all_on(), now), Verdict::Keep(Keep::NoRule));
        assert_eq!(Prefs::default().days(), DEFAULT_DAYS);
    }

    /// A real repository: two linked worktrees, one with an untracked file,
    /// one clean and unchanged; the scan reads both, the judge keeps the
    /// dirty one, and `remove` takes only the clean one.
    #[test]
    fn a_real_repository_is_read_and_only_the_clean_worktree_goes() {
        let dir = std::env::temp_dir().join(format!("td-wtclean-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let sh = |args: &[&str], at: &Path| {
            let ok = std::process::Command::new("git")
                .args(args)
                .current_dir(at)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        sh(&["init", "-q", "-b", "main"], &repo);
        std::fs::write(repo.join("a"), "a").unwrap();
        sh(&["add", "a"], &repo);
        sh(&["commit", "-q", "-m", "a"], &repo);
        sh(&["worktree", "add", "-q", "-b", "clean", "../clean"], &repo);
        sh(&["worktree", "add", "-q", "-b", "messy", "../messy"], &repo);
        std::fs::write(dir.join("messy/new"), "x").unwrap();
        let repo = std::fs::canonicalize(&repo).unwrap();
        assert_eq!(repo_of(&dir.join("clean")), Some(repo.clone()));
        let facts = scan(&repo, &[]).unwrap();
        assert_eq!(facts.len(), 3);
        assert!(facts[0].primary);
        let by = |name: &str| facts.iter().find(|f| f.path.ends_with(name)).unwrap();
        assert_eq!(by("clean").dirty, Some(false));
        assert_eq!(by("clean").ahead, Some(0), "measured against local main");
        assert_eq!(
            by("messy").dirty,
            Some(true),
            "an untracked file is a change"
        );
        let p = Prefs {
            unchanged: Some(true),
            ..Default::default()
        };
        let now = SystemTime::now();
        assert!(judge(by("clean"), &p, now).removes());
        assert_eq!(judge(by("messy"), &p, now), Verdict::Keep(Keep::Changes));
        assert!(
            remove(&repo, &by("messy").path).is_err(),
            "git itself refuses a tree with changes"
        );
        remove(&repo, &by("clean").path).unwrap();
        assert!(!dir.join("clean").exists());
        assert!(dir.join("messy/new").exists());
        let after = scan(&repo, &[]).unwrap();
        assert_eq!(after.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
