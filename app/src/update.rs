//! Update Terminal Delight from inside TD: find a newer build on the channel a
//! person chose, check it, install it beside the old one, and swap only when
//! every check has passed. The plan, the product decisions (D1–D11) and the
//! mockups are in `docs/plans/update-channels/`.
//!
//! Five facts shape everything here.
//!
//! 1. **Terminals live in the session host, and an update never replaces it.**
//!    The host runs whichever build started it and keeps running it. An update
//!    therefore restarts only the window, and every terminal stays — as long as
//!    the new window speaks the old host's wire ([`Compat`]).
//! 2. **When the wire differs, nothing is forced.** The candidate is staged and
//!    installed at the next cold start ([`apply_pending_at_cold_start`]), or
//!    right away if the person presses the button that says how many terminals
//!    it ends.
//! 3. **Verified means "matches the publisher's checksum", nothing more.** A
//!    release's `.sha256` sits on the same page as the build. That catches a
//!    corrupt or truncated download, not a compromised release, and the panel
//!    never says "signed". A release without a checksum is shown, never
//!    installed.
//! 4. **Only installs TD can update safely are updated** ([`Install`]). An
//!    AppImage is replaced in place with the old one kept beside it; a
//!    launcher build (`~/.local/bin/terminal-delight` → a build in
//!    `~/.local/lib/terminal-delight`) gets a new build and a repointed
//!    launcher; a source checkout and a package-owned install are told what to
//!    run instead.
//! 5. **Every install backs the settings up first** ([`backup`]), using the
//!    verified export that CONFIG › ADMIN already has, so a rollback can put
//!    back what an older build understands.
//!
//! The network is reached through `curl`, which every desktop Linux has, so
//! the binary links no HTTP stack. The only requests are to GitHub's public
//! release and commit listings and to the release files themselves, and they
//! carry nothing about the person: no identifier, no version, no terminal
//! content.
use crate::uninstall::{export, roots::Roots};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The repository releases are published from.
pub(crate) const REPO: &str = "parker-brown-family/terminal-delight";
/// The rolling prerelease CI replaces on every push to a `dev/*` branch.
pub(crate) const DEV_TAG: &str = "dev-build";
/// The one asset every channel installs, and its checksum beside it.
pub(crate) const ASSET: &str = "terminal-delight-x86_64.AppImage";
const CHECKSUM: &str = "terminal-delight-x86_64.AppImage.sha256";
/// What a dev build says about itself, written by CI beside the AppImage.
const BUILD_JSON: &str = "build.json";
/// The line a release note carries when its fixes reach terminals only once
/// the session host restarts — a parsing guard, a host-side security fix.
const HOST_RESTART_MARK: &str = "TD-Host-Restart: recommended";
/// How many settings backups are kept; the oldest beyond this are removed.
const KEEP_BACKUPS: usize = 5;
const SMOKE_BUDGET: Duration = Duration::from_secs(25);

// ---------------------------------------------------------------- identity

/// What one build is, as `--build-info` prints it.
///
/// Every field but the version and core can be unknown, and is kept unknown
/// rather than filled: a build from a tarball has no commit, and "no commit"
/// is not the same answer as any commit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct BuildInfo {
    pub(crate) schema: String,
    pub(crate) version: String,
    pub(crate) sha: Option<String>,
    pub(crate) date: Option<String>,
    pub(crate) dirty: Option<bool>,
    /// The window↔host wire fingerprint (`build.rs`). Two builds with the
    /// same one can share a session host; anything else is a full restart.
    pub(crate) wire: Option<String>,
    pub(crate) core: String,
}

pub(crate) const BUILD_SCHEMA: &str = "td-build/1";

impl BuildInfo {
    pub(crate) fn this() -> BuildInfo {
        BuildInfo {
            schema: BUILD_SCHEMA.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            sha: option_env!("TD_BUILD_SHA").map(String::from),
            date: option_env!("TD_BUILD_DATE").map(String::from),
            dirty: option_env!("TD_BUILD_DIRTY").map(|d| d == "1"),
            wire: option_env!("TD_BUILD_WIRE").map(String::from),
            core: crate::vt::CORE_NAME.into(),
        }
    }

    pub(crate) fn short_sha(&self) -> Option<&str> {
        self.sha.as_deref().map(|s| &s[..s.len().min(8)])
    }

    /// `0.5.0 · 3e2f69bb · 2026-11-02`, with `unknown` where a fact is.
    pub(crate) fn describe(&self) -> String {
        let mut s = self.version.clone();
        s.push_str(" · ");
        s.push_str(self.short_sha().unwrap_or("commit unknown"));
        if self.dirty == Some(true) {
            s.push_str("+dirty");
        }
        if let Some(d) = &self.date {
            s.push_str(" · ");
            s.push_str(d.get(..10).unwrap_or(d));
        }
        s
    }
}

/// The reply to `terminal-delight --build-info`.
pub(crate) fn build_info_json() -> String {
    serde_json::to_string(&BuildInfo::this()).unwrap_or_default()
}

/// Whether a candidate can attach to the session host this window uses.
///
/// Judged against this window, not the host: the window is attached and
/// working, so a candidate with the same wire works with that host exactly as
/// well. An unknown on either side is never taken as a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Compat {
    /// Same wire: restart the window and every terminal stays.
    Same,
    /// A different wire: installing it ends the session's terminals.
    Different,
    /// One side does not say (an older build has no `--build-info`). Treated
    /// as `Different`, because guessing wrong would end terminals silently.
    Unknown,
}

pub(crate) fn compat(this: &BuildInfo, other: Option<&BuildInfo>) -> Compat {
    match (this.wire.as_deref(), other.and_then(|o| o.wire.as_deref())) {
        (Some(a), Some(b)) if a == b && this.core == other.map_or("", |o| &o.core) => Compat::Same,
        (Some(_), Some(_)) => Compat::Different,
        _ => Compat::Unknown,
    }
}

// ---------------------------------------------------------------- versions

/// `major.minor.patch[-pre]`, compared the way semver orders them: a
/// prerelease sorts before its release, and prerelease parts compare
/// numerically where both are numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Version {
    core: (u64, u64, u64),
    pre: Vec<String>,
}

impl Version {
    pub(crate) fn parse(s: &str) -> Option<Version> {
        let s = s.trim().trim_start_matches('v');
        let s = s.split('+').next()?;
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (s, None),
        };
        let mut n = core.split('.').map(|p| p.parse::<u64>());
        let v = (n.next()?.ok()?, n.next()?.ok()?, n.next()?.ok()?);
        if n.next().is_some() {
            return None;
        }
        let pre = match pre {
            Some(p) if !p.is_empty() => p.split('.').map(String::from).collect(),
            Some(_) => return None,
            None => vec![],
        };
        Some(Version { core: v, pre })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        match self.core.cmp(&other.core) {
            Equal => {}
            o => return o,
        }
        match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, true) => return Equal,
            (true, false) => return Greater,
            (false, true) => return Less,
            _ => {}
        }
        for (a, b) in self.pre.iter().zip(&other.pre) {
            let o = match (a.parse::<u64>(), b.parse::<u64>()) {
                (Ok(x), Ok(y)) => x.cmp(&y),
                (Ok(_), Err(_)) => Less,
                (Err(_), Ok(_)) => Greater,
                _ => a.cmp(b),
            };
            if o != Equal {
                return o;
            }
        }
        self.pre.len().cmp(&other.pre.len())
    }
}

// ---------------------------------------------------------------- settings

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Channel {
    /// Published releases. Absent reads as this: a person who never chose is
    /// on the channel everyone was on before there was a choice.
    #[default]
    Stable,
    /// The rolling build of the development branch. Experimental.
    Dev,
}

impl Channel {
    pub(crate) fn word(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Dev => "dev",
        }
    }
}

/// The person's choices, in `~/.config/terminal-delight/updates.toml`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Choices {
    #[serde(default)]
    pub(crate) channel: Channel,
    /// Absent is off: a daily check is something a person turns on.
    #[serde(default)]
    pub(crate) check_daily: Option<bool>,
}

/// What this machine has done, in `~/.local/state/terminal-delight/updates.toml`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Record {
    /// Unix seconds of the last completed check; absent before the first.
    pub(crate) last_check: Option<u64>,
    /// The offer a daily check last announced, so it is announced once.
    pub(crate) last_announced: Option<String>,
    /// The build the last install replaced: what Roll back goes to.
    pub(crate) previous: Option<Kept>,
    /// A checked build waiting for the next cold start (an update whose wire
    /// differs from the running session host's).
    pub(crate) pending: Option<Pending>,
    /// The newest settings backup, for the panel and for rollback.
    pub(crate) last_backup: Option<PathBuf>,
}

/// A build kept for rollback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Kept {
    /// The file Roll back puts back: a launcher target, or `<image>.previous`.
    pub(crate) path: PathBuf,
    pub(crate) describe: String,
    pub(crate) wire: Option<String>,
    /// The settings backup taken just before it was replaced.
    pub(crate) backup: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Pending {
    /// The verified, smoke-checked file waiting to be installed.
    pub(crate) staged: PathBuf,
    pub(crate) describe: String,
    pub(crate) channel: Channel,
    pub(crate) sha256: String,
    /// What a new window started from when this was staged. A different
    /// answer at the cold start means the install was moved on another way.
    #[serde(default)]
    pub(crate) over: Option<String>,
}

fn choices_path(roots: &Roots) -> PathBuf {
    roots.config.join("updates.toml")
}

fn record_path(roots: &Roots) -> PathBuf {
    roots.state.join("updates.toml")
}

pub(crate) fn load_choices(roots: &Roots) -> Choices {
    fs::read_to_string(choices_path(roots))
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn save_choices(roots: &Roots, c: &Choices) -> Result<(), String> {
    write_toml(&choices_path(roots), c)
}

pub(crate) fn load_record(roots: &Roots) -> Record {
    fs::read_to_string(record_path(roots))
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn save_record(roots: &Roots, r: &Record) -> Result<(), String> {
    write_toml(&record_path(roots), r)
}

fn write_toml<T: Serialize>(path: &Path, v: &T) -> Result<(), String> {
    let text = toml::to_string_pretty(v).map_err(|e| e.to_string())?;
    let dir = path.parent().ok_or("no parent directory")?;
    fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    fs::write(&tmp, text).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("cannot replace {}: {e}", path.display()))
}

// ---------------------------------------------------------------- installs

/// How this copy of TD was installed, which decides what an update may do.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum Install {
    /// A single AppImage file, replaced in place.
    AppImage { image: PathBuf },
    /// `~/.local/bin/terminal-delight` linking to a build in the library.
    Launcher {
        launcher: PathBuf,
        library: PathBuf,
        /// What the launcher points at now; absent when it is not a link.
        target: Option<PathBuf>,
    },
    /// Built from a git checkout; TD never pulls or rebuilds one.
    Source {
        checkout: PathBuf,
        changed: Option<usize>,
    },
    /// Owned by a system package; the package manager updates it.
    Package { exe: PathBuf },
    /// Anything else, with the reason it is not updated.
    Unknown { exe: PathBuf, why: String },
}

impl Install {
    pub(crate) fn can_install(&self) -> bool {
        matches!(self, Install::AppImage { .. } | Install::Launcher { .. })
    }

    /// The file a new window is started from after an install.
    pub(crate) fn entry(&self) -> Option<PathBuf> {
        match self {
            Install::AppImage { image } => Some(image.clone()),
            Install::Launcher { launcher, .. } => Some(launcher.clone()),
            _ => None,
        }
    }
}

/// The `APPIMAGE` and `APPDIR` an AppImage's runtime sets, from this process.
pub(crate) fn appimage_env() -> (Option<PathBuf>, Option<PathBuf>) {
    (
        std::env::var_os("APPIMAGE").map(PathBuf::from),
        std::env::var_os("APPDIR").map(PathBuf::from),
    )
}

/// Work out the install from the running executable and the environment.
///
/// `APPIMAGE` alone is not trusted: every terminal an AppImage TD opens
/// inherits it, so a launcher build started from one of those panes would
/// otherwise replace an AppImage it is not running from. It counts only when
/// this program lives inside the image's mount, `APPDIR`.
pub(crate) fn detect(
    roots: &Roots,
    exe: &Path,
    appimage: Option<&Path>,
    appdir: Option<&Path>,
) -> Install {
    let canon = |p: &Path| fs::canonicalize(p).ok();
    let exe_c = canon(exe).unwrap_or_else(|| exe.to_path_buf());
    let inside = appdir.and_then(canon).is_some_and(|d| exe_c.starts_with(d));
    if let Some(image) = appimage.filter(|p| inside && p.is_absolute() && p.is_file()) {
        return Install::AppImage {
            image: image.to_path_buf(),
        };
    }
    let launcher = roots.bin.join("terminal-delight");
    let lib_c = canon(&roots.builds).unwrap_or_else(|| roots.builds.clone());
    if exe_c.starts_with(&lib_c) {
        let target = fs::read_link(&launcher).ok().map(|t| {
            if t.is_absolute() {
                t
            } else {
                roots.bin.join(t)
            }
        });
        return Install::Launcher {
            launcher,
            library: roots.builds.clone(),
            target,
        };
    }
    if let Some(checkout) = source_checkout(&exe_c) {
        let changed = Command::new("git")
            .args(["-C"])
            .arg(&checkout)
            .args(["status", "--porcelain", "--untracked-files=no"])
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().count());
        return Install::Source { checkout, changed };
    }
    let system = ["/usr/", "/opt/", "/nix/", "/snap/", "/gnu/"];
    if system.iter().any(|p| exe_c.starts_with(p)) {
        return Install::Package { exe: exe_c };
    }
    Install::Unknown {
        exe: exe_c,
        why: "this file was not installed as an AppImage or into TD's build library, so TD does not replace it".into(),
    }
}

/// `<checkout>` for an executable at `<checkout>/…/target/<profile>/terminal-delight`.
fn source_checkout(exe: &Path) -> Option<PathBuf> {
    let mut at = exe.parent()?;
    while let Some(parent) = at.parent() {
        if at.file_name().is_some_and(|n| n == "target") {
            let mut root = parent;
            loop {
                if root.join(".git").exists() {
                    return Some(root.to_path_buf());
                }
                root = root.parent()?;
            }
        }
        at = parent;
    }
    None
}

// ---------------------------------------------------------------- network

/// Where the GitHub API answers. `TD_UPDATE_API` points it elsewhere, which is
/// how the tests serve a fixture release over `file://` with no network.
fn api_base() -> String {
    std::env::var("TD_UPDATE_API")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://api.github.com".into())
}

fn curl() -> Command {
    let mut c = Command::new("curl");
    let file_ok = api_base().starts_with("file://");
    c.args(["--fail", "--silent", "--show-error", "--location"])
        .args(["--proto", if file_ok { "=https,file" } else { "=https" }])
        .args(["--proto-redir", "=https"])
        .args(["--connect-timeout", "15", "--retry", "2"])
        // GitHub refuses a request with no agent. This one names the program,
        // not the person or the version.
        .args(["-H", "User-Agent: terminal-delight-updater"])
        .args(["-H", "Accept: application/vnd.github+json"])
        .stdin(Stdio::null());
    c
}

fn fetch(url: &str) -> Result<String, String> {
    let out = curl()
        .args(["--max-time", "30", "--", url])
        .output()
        .map_err(|e| format!("cannot run curl: {e}"))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(format!(
            "{url}: {}",
            if why.is_empty() {
                "request failed".into()
            } else {
                why
            }
        ));
    }
    String::from_utf8(out.stdout).map_err(|_| format!("{url}: the answer is not text"))
}

fn fetch_json(url: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(&fetch(url)?).map_err(|e| format!("{url}: not JSON: {e}"))
}

fn download(url: &str, to: &Path) -> Result<(), String> {
    let out = curl()
        .args(["--max-time", "900", "--output"])
        .arg(to)
        .args(["--", url])
        .output()
        .map_err(|e| format!("cannot run curl: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let _ = fs::remove_file(to);
        Err(format!(
            "download failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

// ---------------------------------------------------------------- offers

/// How an offered build relates to the running one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Relation {
    /// It contains the running build's work and more.
    Newer,
    Same,
    /// The running build is ahead of it.
    Older,
    /// Neither contains the other, as a dev build and a later stable can.
    Diverged,
    /// Nothing could be compared.
    Unknown,
}

/// A build a channel offers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct Offer {
    pub(crate) channel: Channel,
    pub(crate) tag: String,
    pub(crate) version: String,
    pub(crate) sha: Option<String>,
    pub(crate) date: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) notes: String,
    pub(crate) page: String,
    pub(crate) asset: Option<String>,
    pub(crate) checksum: Option<String>,
    pub(crate) size: Option<u64>,
    pub(crate) relation: Relation,
    /// Commits it has that the running build lacks, when GitHub said.
    pub(crate) ahead_by: Option<u64>,
    /// Its notes say its fixes reach terminals only after a host restart.
    pub(crate) host_restart: bool,
}

impl Offer {
    /// Why it cannot be installed in-app, or `None` when it can.
    pub(crate) fn not_installable(&self) -> Option<&'static str> {
        if self.asset.is_none() {
            Some("this release has no Linux AppImage")
        } else if self.checksum.is_none() {
            Some("no checksum was published for it, so TD cannot check the download — download it yourself from its page")
        } else {
            None
        }
    }

    pub(crate) fn short_sha(&self) -> Option<&str> {
        self.sha.as_deref().map(|s| &s[..s.len().min(8)])
    }

    pub(crate) fn describe(&self) -> String {
        match (self.channel, self.short_sha()) {
            (Channel::Dev, Some(s)) => format!("{} · {s}", self.version),
            _ => self.version.clone(),
        }
    }

    /// Worth offering as an update: newer, or (dev) not the same commit.
    pub(crate) fn is_update(&self) -> bool {
        match self.channel {
            Channel::Stable => self.relation == Relation::Newer,
            Channel::Dev => matches!(
                self.relation,
                Relation::Newer | Relation::Diverged | Relation::Unknown
            ),
        }
    }
}

/// What a check found.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Check {
    pub(crate) channel: Channel,
    pub(crate) offer: Option<Offer>,
    /// Why there is nothing to offer, in words for the panel.
    pub(crate) none: Option<String>,
    /// For the dev channel with no published build: the commands that build
    /// the newest dev commit from source.
    pub(crate) source_build: Option<String>,
}

pub(crate) fn check(channel: Channel, this: &BuildInfo) -> Result<Check, String> {
    match channel {
        Channel::Stable => check_stable(this),
        Channel::Dev => check_dev(this),
    }
}

fn asset_urls(release: &serde_json::Value) -> (Option<String>, Option<String>, Option<u64>) {
    let mut image = None;
    let mut sum = None;
    let mut size = None;
    for a in release["assets"].as_array().into_iter().flatten() {
        let url = a["browser_download_url"].as_str().map(String::from);
        match a["name"].as_str() {
            Some(ASSET) => {
                image = url;
                size = a["size"].as_u64();
            }
            Some(CHECKSUM) => sum = url,
            _ => {}
        }
    }
    (image, sum, size)
}

/// The newest published, non-prerelease release, from GitHub's list.
pub(crate) fn newest_stable(releases: &serde_json::Value) -> Option<(Version, serde_json::Value)> {
    releases
        .as_array()?
        .iter()
        .filter(|r| {
            !r["draft"].as_bool().unwrap_or(true) && !r["prerelease"].as_bool().unwrap_or(true)
        })
        .filter_map(|r| Some((Version::parse(r["tag_name"].as_str()?)?, r.clone())))
        .max_by(|a, b| a.0.cmp(&b.0))
}

fn check_stable(this: &BuildInfo) -> Result<Check, String> {
    let list = fetch_json(&format!("{}/repos/{REPO}/releases?per_page=50", api_base()))?;
    let Some((version, r)) = newest_stable(&list) else {
        return Ok(Check {
            channel: Channel::Stable,
            offer: None,
            none: Some("no stable release has been published".into()),
            source_build: None,
        });
    };
    let tag = r["tag_name"].as_str().unwrap_or_default().to_string();
    let (relation, ahead_by) = relate(this, &tag, &version);
    let (asset, checksum, size) = asset_urls(&r);
    let notes = r["body"].as_str().unwrap_or_default().to_string();
    Ok(Check {
        channel: Channel::Stable,
        offer: Some(Offer {
            channel: Channel::Stable,
            version: r["tag_name"]
                .as_str()
                .unwrap_or_default()
                .trim_start_matches('v')
                .to_string(),
            tag,
            sha: None,
            date: r["published_at"].as_str().map(String::from),
            branch: None,
            host_restart: notes.contains(HOST_RESTART_MARK),
            notes,
            page: r["html_url"].as_str().unwrap_or_default().to_string(),
            asset,
            checksum,
            size,
            relation,
            ahead_by,
        }),
        none: None,
        source_build: None,
    })
}

/// Compare the running commit with `target` through GitHub, falling back to
/// version order when the commit is unknown or GitHub does not know it.
fn relate(this: &BuildInfo, target: &str, target_version: &Version) -> (Relation, Option<u64>) {
    if let Some(r) = this.sha.as_deref().and_then(|sha| compare(sha, target)) {
        return r;
    }
    match Version::parse(&this.version) {
        Some(mine) if *target_version > mine => (Relation::Newer, None),
        Some(mine) if *target_version == mine => (Relation::Same, None),
        Some(_) => (Relation::Older, None),
        None => (Relation::Unknown, None),
    }
}

/// How `target` relates to the commit `from`, as GitHub's compare answers;
/// `None` when it could not say.
fn compare(from: &str, target: &str) -> Option<(Relation, Option<u64>)> {
    let url = format!("{}/repos/{REPO}/compare/{from}...{target}", api_base());
    let c = fetch_json(&url).ok()?;
    let r = match c["status"].as_str()? {
        "identical" => Relation::Same,
        "ahead" => Relation::Newer,
        "behind" => Relation::Older,
        "diverged" => Relation::Diverged,
        _ => return None,
    };
    Some((r, c["ahead_by"].as_u64()))
}

/// What CI writes beside a dev build.
#[derive(Debug, Clone, Deserialize)]
struct DevBuild {
    sha: String,
    date: Option<String>,
    branch: Option<String>,
    version: Option<String>,
}

fn check_dev(this: &BuildInfo) -> Result<Check, String> {
    let url = format!("{}/repos/{REPO}/releases/tags/{DEV_TAG}", api_base());
    let release = match fetch_json(&url) {
        Ok(r) => r,
        Err(e) if e.contains("404") => {
            return Ok(Check {
                channel: Channel::Dev,
                offer: None,
                none: Some("no public dev build has been published yet".into()),
                source_build: Some(source_build_lines(newest_dev_branch().as_deref(), None)),
            })
        }
        Err(e) => return Err(e),
    };
    let build_url = release["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|a| a["name"].as_str() == Some(BUILD_JSON))
        .and_then(|a| a["browser_download_url"].as_str())
        .ok_or("the dev build has no build.json")?;
    let build: DevBuild = serde_json::from_str(&fetch(build_url)?)
        .map_err(|e| format!("the dev build's build.json: {e}"))?;
    if build.sha.len() < 7 || !build.sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("the dev build's build.json names no commit".into());
    }
    let (asset, checksum, size) = asset_urls(&release);
    // Two dev commits are ordered only by their history. When GitHub cannot
    // compare them (a local commit it has never seen, or the hourly limit),
    // the answer is unknown, never "older": that would say "up to date".
    let (relation, ahead_by) = match &this.sha {
        Some(s) if s == &build.sha => (Relation::Same, Some(0)),
        Some(s) => compare(s, &build.sha).unwrap_or((Relation::Unknown, None)),
        None => (Relation::Unknown, None),
    };
    let (notes, host_restart) = commit_notes(this.sha.as_deref(), &build.sha);
    let branch_head = build.branch.as_deref().and_then(branch_head);
    let source_build = match &branch_head {
        Some(head) if head != &build.sha => {
            Some(source_build_lines(build.branch.as_deref(), Some(head)))
        }
        _ => None,
    };
    Ok(Check {
        channel: Channel::Dev,
        offer: Some(Offer {
            channel: Channel::Dev,
            tag: DEV_TAG.into(),
            version: build.version.clone().unwrap_or_else(|| "dev".into()),
            sha: Some(build.sha.clone()),
            date: build.date.clone(),
            branch: build.branch.clone(),
            host_restart,
            notes,
            page: release["html_url"].as_str().unwrap_or_default().to_string(),
            asset,
            checksum,
            size,
            relation,
            ahead_by,
        }),
        none: None,
        source_build,
    })
}

/// The first lines of the commits `to` has that `from` lacks, newest first,
/// and whether any of those commits carries the host-restart mark anywhere in
/// its message (a mark is never a commit's first line).
fn commit_notes(from: Option<&str>, to: &str) -> (String, bool) {
    let Some(from) = from else {
        return (String::new(), false);
    };
    let url = format!("{}/repos/{REPO}/compare/{from}...{to}", api_base());
    let Ok(c) = fetch_json(&url) else {
        return (String::new(), false);
    };
    let commits = c["commits"].as_array().cloned().unwrap_or_default();
    let marked = commits.iter().any(|k| {
        k["commit"]["message"]
            .as_str()
            .is_some_and(|m| m.contains(HOST_RESTART_MARK))
    });
    let mut lines: Vec<String> = commits
        .iter()
        .filter_map(|k| {
            let sha = k["sha"].as_str()?;
            let msg = k["commit"]["message"].as_str()?.lines().next()?;
            Some(format!("{} {msg}", &sha[..sha.len().min(8)]))
        })
        .collect();
    lines.reverse();
    lines.truncate(20);
    (lines.join("\n"), marked)
}

fn branch_head(branch: &str) -> Option<String> {
    let url = format!("{}/repos/{REPO}/commits/{branch}", api_base());
    fetch_json(&url).ok()?["sha"].as_str().map(String::from)
}

fn newest_dev_branch() -> Option<String> {
    let list = fetch_json(&format!(
        "{}/repos/{REPO}/branches?per_page=100",
        api_base()
    ))
    .ok()?;
    list.as_array()?
        .iter()
        .filter_map(|b| b["name"].as_str())
        .filter_map(|n| Some((Version::parse(n.strip_prefix("dev/")?)?, n.to_string())))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, n)| n)
}

pub(crate) fn source_build_lines(branch: Option<&str>, sha: Option<&str>) -> String {
    let branch = branch.unwrap_or("dev/0.5.0");
    let mut s = format!("git clone -b {branch} https://github.com/{REPO}\ncd terminal-delight");
    if let Some(sha) = sha {
        s.push_str(&format!(" && git checkout {sha}"));
    }
    s.push_str(
        "\nbash scripts/prepare-gpui.sh && cargo build --release --manifest-path app/Cargo.toml",
    );
    s
}

// ---------------------------------------------------------------- prepare

/// A downloaded build that has passed every check and is ready to install.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Prepared {
    pub(crate) offer: Offer,
    /// The verified AppImage.
    pub(crate) image: PathBuf,
    /// The program inside it, extracted and smoke-checked.
    pub(crate) binary: PathBuf,
    pub(crate) sha256: String,
    /// What the new build said about itself; absent from a build that
    /// predates `--build-info`.
    pub(crate) info: Option<BuildInfo>,
    pub(crate) compat: Compat,
}

/// Where downloads are kept until they are installed or discarded.
fn staging(roots: &Roots) -> PathBuf {
    roots.cache.join("updates")
}

/// Download, check against the published checksum, extract and smoke-check.
/// Nothing outside the staging directory is touched; any failure removes what
/// this call downloaded.
pub(crate) fn prepare(roots: &Roots, offer: &Offer, this: &BuildInfo) -> Result<Prepared, String> {
    if let Some(why) = offer.not_installable() {
        return Err(why.into());
    }
    let (Some(asset), Some(checksum)) = (&offer.asset, &offer.checksum) else {
        unreachable!("checked by not_installable");
    };
    let dir = staging(roots).join(format!(
        "{}-{}-{}",
        offer.channel.word(),
        offer.short_sha().unwrap_or(&offer.version),
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).ok();
    let result = prepare_in(&dir, offer, asset, checksum, this);
    if result.is_err() {
        let _ = fs::remove_dir_all(&dir);
    }
    result
}

fn prepare_in(
    dir: &Path,
    offer: &Offer,
    asset: &str,
    checksum: &str,
    this: &BuildInfo,
) -> Result<Prepared, String> {
    let expected = parse_checksum(&fetch(checksum)?)
        .ok_or("the published checksum file does not hold a sha256")?;
    let image = dir.join(ASSET);
    download(asset, &image)?;
    let got = sha256_file(&image).map_err(|e| format!("cannot read the download: {e}"))?;
    if got != expected {
        let _ = fs::remove_file(&image);
        return Err(format!(
            "the download did not match its published checksum (expected {}…, got {}…); it was deleted",
            &expected[..12],
            &got[..12]
        ));
    }
    fs::set_permissions(&image, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("cannot mark the download executable: {e}"))?;
    let binary = extract_binary(dir, &image)?;
    let info = smoke(&binary)?;
    if let (Some(info), Some(want)) = (&info, &offer.sha) {
        if info.sha.as_deref().is_some_and(|s| s != want) {
            return Err(format!(
                "the downloaded build says it is commit {}, not {}",
                info.short_sha().unwrap_or("?"),
                &want[..want.len().min(8)]
            ));
        }
    }
    if let (Some(info), Channel::Stable) = (&info, offer.channel) {
        if Version::parse(&info.version) != Version::parse(&offer.version) {
            return Err(format!(
                "the downloaded build says it is {}, not {}",
                info.version, offer.version
            ));
        }
    }
    Ok(Prepared {
        offer: offer.clone(),
        image,
        binary,
        sha256: got,
        compat: compat(this, info.as_ref()),
        info,
    })
}

/// The hex digest from a `sha256sum`-style line.
pub(crate) fn parse_checksum(text: &str) -> Option<String> {
    let first = text.split_whitespace().next()?.to_ascii_lowercase();
    (first.len() == 64 && first.bytes().all(|b| b.is_ascii_hexdigit())).then_some(first)
}

pub(crate) fn sha256_file(path: &Path) -> io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Pull `usr/bin/terminal-delight` out of the AppImage without mounting it,
/// so the check works on a machine without FUSE.
fn extract_binary(dir: &Path, image: &Path) -> Result<PathBuf, String> {
    let out = Command::new(image)
        .args(["--appimage-extract", "usr/bin/terminal-delight"])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("the download would not run: {e}"))?;
    let bin = dir.join("squashfs-root/usr/bin/terminal-delight");
    if out.status.success() && bin.is_file() {
        Ok(bin)
    } else {
        Err(format!(
            "the download is not a Terminal Delight AppImage ({})",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Run the new build far enough to say what it is, and no further.
///
/// `--build-info` first. A build from before it existed refuses the flag, so
/// `--version` is the fallback, which proves it starts but says nothing about
/// its wire — and that unknown is carried, not guessed.
pub(crate) fn smoke(binary: &Path) -> Result<Option<BuildInfo>, String> {
    if let Some(text) = run_bounded(binary, "--build-info")? {
        if let Ok(info) = serde_json::from_str::<BuildInfo>(text.trim()) {
            return Ok(Some(info));
        }
    }
    match run_bounded(binary, "--version")? {
        Some(t) if t.starts_with("terminal-delight ") => Ok(None),
        _ => Err("the new build did not start".into()),
    }
}

/// Stdout of `binary flag` when it exits 0 inside the budget.
fn run_bounded(binary: &Path, flag: &str) -> Result<Option<String>, String> {
    let mut child = Command::new(binary)
        .arg(flag)
        .env_remove("TD_SESSION")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("the new build would not run: {e}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut s = String::new();
                if let Some(mut o) = child.stdout.take() {
                    let _ = o.read_to_string(&mut s);
                }
                return Ok(status.success().then_some(s));
            }
            Ok(None) if started.elapsed() > SMOKE_BUDGET => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "the new build did not answer {flag} within {}s",
                    SMOKE_BUDGET.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("cannot wait for the new build: {e}")),
        }
    }
}

// ---------------------------------------------------------------- backup

/// Copy the settings, layout and small records into a new verified backup.
///
/// Uses the export CONFIG › ADMIN already has. The settings directory goes in
/// whole; from the records only the small ones a newer build might rewrite.
/// The bulky history — conversations, surfaces, brief backups, logs — is
/// left out: an update only ever adds to it.
pub(crate) fn backup(roots: &Roots, label: &str) -> Result<PathBuf, String> {
    let base = backups_dir(roots);
    fs::create_dir_all(&base).map_err(|e| format!("cannot create {}: {e}", base.display()))?;
    let stamp = crate::uninstall::now_stamp();
    let safe: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // Two backups inside one second (an install and an immediate rollback)
    // would share a name, and an export only ever makes a new directory.
    let dest = (1..100)
        .map(|n| match n {
            1 => base.join(format!("update-{stamp}-from-{safe}")),
            n => base.join(format!("update-{stamp}-{n}-from-{safe}")),
        })
        .find(|d| fs::symlink_metadata(d).is_err())
        .ok_or("too many backups this second")?;
    let opts = export::ExportOptions {
        records: true,
        dest: Some(dest),
        stamp,
    };
    let host = fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|s| s.trim().to_string());
    let report = export::export_only_records(
        roots,
        &opts,
        env!("CARGO_PKG_VERSION"),
        host.as_deref(),
        BACKED_UP_RECORDS,
    )?;
    // Never prune the backups the record still points at: Roll back offers
    // to restore the one taken when the previous build was replaced.
    let rec = load_record(roots);
    let mut keep: Vec<PathBuf> = vec![report.dir.clone()];
    keep.extend(rec.previous.and_then(|k| k.backup));
    keep.extend(rec.last_backup);
    prune_backups(&base, &keep);
    Ok(report.dir)
}

/// The records a backup carries: small, and the ones a newer build might
/// rewrite in a shape an older one cannot read.
pub(crate) const BACKED_UP_RECORDS: &[&str] = &[
    "updates.toml",
    "threshing-floor",
    "warroom",
    "management",
    "agent-ledger",
    "agents",
    "wake",
    "notes",
    "handovers",
];

/// Beside TD's own directories rather than inside one, so that uninstalling TD
/// does not take the backups with it.
pub(crate) fn backups_dir(roots: &Roots) -> PathBuf {
    roots.data_base.join("terminal-delight-backups")
}

fn prune_backups(base: &Path, keep: &[PathBuf]) {
    let Ok(rd) = fs::read_dir(base) else { return };
    let mut ours: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("update-"))
                && p.join("manifest.json").is_file()
                && !keep.contains(p)
        })
        .collect();
    ours.sort();
    while ours.len() + keep.len() > KEEP_BACKUPS && !ours.is_empty() {
        let old = ours.remove(0);
        let _ = fs::remove_dir_all(old);
    }
}

// ---------------------------------------------------------------- install

/// What an install did.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Installed {
    pub(crate) describe: String,
    /// Where a new window starts from now.
    pub(crate) entry: PathBuf,
    pub(crate) previous: Option<Kept>,
    pub(crate) backup: PathBuf,
}

/// The session hosts on this machine, as a swap has to see them.
///
/// Gathered by the caller and handed in, so the rule that uses it can be
/// tested without the machine's own hosts deciding the answer.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct Sessions {
    /// Every session whose socket answers in any way: live, refusing our
    /// protocol, or not answering. Only a socket nobody listens on is absent.
    pub(crate) alive: Vec<String>,
    /// Sessions whose host runs a build nothing here can vouch for.
    pub(crate) unvouched: Vec<String>,
}

impl Sessions {
    pub(crate) fn now(own: Option<&str>) -> Sessions {
        let exe = std::env::current_exe().unwrap_or_default();
        Sessions {
            alive: hosts_alive(),
            unvouched: unvouched_hosts(&exe, own),
        }
    }

    /// Hosts alive other than `own`'s.
    pub(crate) fn others(&self, own: Option<&str>) -> usize {
        self.alive
            .iter()
            .filter(|k| Some(k.as_str()) != own)
            .count()
    }

    /// Whether a swap to a build of `compat` may happen now, and why not.
    ///
    /// The one rule every install, rollback and staged install obeys: a swap
    /// either keeps every live host working (same wire as this window, and
    /// every other session's host vouched for), or happens when no host is
    /// alive at all. Anything else would put the next window that opens in
    /// front of a host it cannot talk to.
    pub(crate) fn allow(&self, compat: Compat) -> Result<(), String> {
        let why = if compat != Compat::Same {
            "this build changes how the window talks to your terminals".to_string()
        } else if !self.unvouched.is_empty() {
            format!(
                "the session host of {} runs a build TD cannot vouch for",
                self.unvouched.join(", ")
            )
        } else {
            return Ok(());
        };
        if self.alive.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "{why}, and {} session host(s) are running; it installs at the next cold start, or from ⚙ CONFIG › UPDATES with Restart everything",
                self.alive.len()
            ))
        }
    }

    /// True when only a cold start, or Restart everything, may install it.
    pub(crate) fn needs_cold_start(&self, compat: Compat) -> bool {
        compat != Compat::Same || !self.unvouched.is_empty()
    }
}

/// Install a prepared build: back up, then swap the AppImage or repoint the
/// launcher, keeping what it replaced. Refuses whatever [`Sessions::allow`]
/// refuses; such a build is staged instead.
pub(crate) fn install(
    roots: &Roots,
    install: &Install,
    p: &Prepared,
    this: &BuildInfo,
    sessions: &Sessions,
) -> Result<Installed, String> {
    if !install.can_install() {
        return Err("this install is not one TD updates".into());
    }
    sessions.allow(p.compat)?;
    let backup = backup(roots, &this.version)?;
    let describe = p
        .info
        .as_ref()
        .map(BuildInfo::describe)
        .unwrap_or_else(|| p.offer.describe());
    let activated = activate(
        roots,
        install,
        &p.image,
        &p.binary,
        &p.offer,
        p.info.as_ref(),
    )?;
    let mut rec = load_record(roots);
    if let Activated::Replaced { kept, is_this } = activated {
        // The replaced file is described as this build only when it IS this
        // build. Another window may already have moved the install on, and
        // then all that is known is its name: the wire is left unknown, so a
        // rollback to it is never taken for a window-only one.
        rec.previous = kept.map(|path| Kept {
            describe: if is_this {
                this.describe()
            } else {
                path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default()
            },
            wire: if is_this { this.wire.clone() } else { None },
            backup: Some(backup.clone()),
            path,
        });
    }
    // A build staged earlier is superseded by this one: its download goes
    // too, or the cache keeps a 40 MB file nothing will ever read.
    drop_pending(roots, &mut rec, p.image.parent());
    rec.last_backup = Some(backup.clone());
    save_record(roots, &rec)?;
    Ok(Installed {
        describe,
        entry: install.entry().expect("can_install"),
        previous: rec.previous.clone(),
        backup,
    })
}

/// Forget a staged build and remove its download, unless it lives in `keep`.
fn drop_pending(roots: &Roots, rec: &mut Record, keep: Option<&Path>) {
    if let Some(old) = rec.pending.take() {
        if let Some(dir) = old.staged.parent() {
            if dir.starts_with(staging(roots)) && Some(dir) != keep {
                let _ = fs::remove_dir_all(dir);
            }
        }
    }
}

/// What putting a build in place did.
#[derive(Debug)]
enum Activated {
    /// The install now runs the new build. `kept` is where the replaced file
    /// went; `is_this` says whether that file is the build this process runs.
    Replaced {
        kept: Option<PathBuf>,
        is_this: bool,
    },
    /// The install already ran exactly this build; nothing was touched, and
    /// the build kept for rollback stays what it was.
    AlreadyThere,
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Put the new build in place.
fn activate(
    roots: &Roots,
    install: &Install,
    image: &Path,
    binary: &Path,
    offer: &Offer,
    info: Option<&BuildInfo>,
) -> Result<Activated, String> {
    let _lock = DeployLock::take(roots)?;
    match install {
        Install::AppImage { image: dest } => {
            if dest.is_file() && sha256_file(dest).ok() == sha256_file(image).ok() {
                return Ok(Activated::AlreadyThere);
            }
            let dir = dest.parent().ok_or("the AppImage has no directory")?;
            let name = dest
                .file_name()
                .ok_or("the AppImage has no name")?
                .to_string_lossy()
                .to_string();
            let pid = std::process::id();
            let part = dir.join(format!(".{name}.td-update-{pid}"));
            copy_exec(image, &part)?;
            // Order matters. The current image is linked aside first, then the
            // new one renamed over it, and only then is the aside renamed to
            // `.previous`. A failure before the second step changes nothing; a
            // failure after it leaves the old image in the aside, which is
            // what gets recorded.
            let aside = dir.join(format!(".{name}.previous-{pid}"));
            let had = dest.is_file();
            if had {
                fs::hard_link(dest, &aside)
                    .or_else(|_| fs::copy(dest, &aside).map(|_| ()))
                    .map_err(|e| {
                        let _ = fs::remove_file(&part);
                        format!("cannot keep the current AppImage: {e}")
                    })?;
            }
            fs::rename(&part, dest).map_err(|e| {
                let _ = fs::remove_file(&part);
                let _ = fs::remove_file(&aside);
                format!("cannot replace {}: {e}", dest.display())
            })?;
            let kept = had.then(|| {
                let previous = dir.join(format!("{name}.previous"));
                match fs::rename(&aside, &previous) {
                    Ok(()) => previous,
                    Err(_) => aside.clone(),
                }
            });
            Ok(Activated::Replaced {
                kept,
                is_this: true,
            })
        }
        Install::Launcher {
            launcher,
            library,
            target,
        } => {
            // Read inside the lock: another install may have moved it since
            // the panel looked, and repointing over that would undo it.
            let now_target = fs::read_link(launcher).ok();
            if now_target.as_ref().map(|t| absolutise(launcher, t)) != target.clone() {
                return Err(
                    "the launcher changed while this update was being prepared; check again".into(),
                );
            }
            fs::create_dir_all(library)
                .map_err(|e| format!("cannot create {}: {e}", library.display()))?;
            let label = match offer.channel {
                Channel::Stable => format!("stable-{}", offer.version),
                Channel::Dev => "dev".to_string(),
            };
            let sha = info
                .and_then(|i| i.short_sha().map(String::from))
                .or_else(|| offer.short_sha().map(String::from))
                .unwrap_or_else(|| "0000000".into());
            let name = format!("td-{sha}-{label}");
            let dest = library.join(&name);
            if dest.exists() {
                if sha256_file(&dest).ok() != sha256_file(binary).ok() {
                    return Err(format!(
                        "{} already exists and is a different build",
                        dest.display()
                    ));
                }
                if target.as_deref().is_some_and(|t| same_file(t, &dest)) {
                    return Ok(Activated::AlreadyThere);
                }
            } else {
                let part = library.join(format!(".{name}.td-update-{}", std::process::id()));
                copy_exec(binary, &part)?;
                fs::rename(&part, &dest).map_err(|e| {
                    let _ = fs::remove_file(&part);
                    format!("cannot install {}: {e}", dest.display())
                })?;
            }
            repoint(launcher, &dest)?;
            let me = std::env::current_exe().unwrap_or_default();
            Ok(Activated::Replaced {
                is_this: target.as_deref().is_some_and(|t| same_file(t, &me)),
                kept: target.clone(),
            })
        }
        _ => Err("this install is not one TD updates".into()),
    }
}

fn absolutise(link: &Path, target: &Path) -> PathBuf {
    if target.is_absolute() {
        target.to_path_buf()
    } else {
        link.parent().unwrap_or(Path::new("/")).join(target)
    }
}

/// Point the launcher at `dest` in one rename, so no moment exists in which
/// it points nowhere.
fn repoint(launcher: &Path, dest: &Path) -> Result<(), String> {
    let dir = launcher.parent().ok_or("the launcher has no directory")?;
    let tmp = dir.join(format!(
        ".terminal-delight.td-update-{}",
        std::process::id()
    ));
    let _ = fs::remove_file(&tmp);
    std::os::unix::fs::symlink(dest, &tmp)
        .map_err(|e| format!("cannot link {}: {e}", tmp.display()))?;
    fs::rename(&tmp, launcher).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("cannot repoint {}: {e}", launcher.display())
    })
}

/// Copy to `to` (created, never overwritten), executable, flushed to disk.
fn copy_exec(from: &Path, to: &Path) -> Result<(), String> {
    let mut src =
        fs::File::open(from).map_err(|e| format!("cannot read {}: {e}", from.display()))?;
    let mut dst = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o755)
        .open(to)
        .map_err(|e| format!("cannot write {}: {e}", to.display()))?;
    io::copy(&mut src, &mut dst)
        .and_then(|_| dst.flush())
        .and_then(|_| dst.sync_all())
        .map_err(|e| {
            let _ = fs::remove_file(to);
            format!("cannot write {}: {e}", to.display())
        })
}

/// The lock every installer on this machine takes — `land.sh`'s swap tool
/// included — so two installs never repoint one launcher at once.
struct DeployLock(#[allow(dead_code)] fs::File);

impl DeployLock {
    fn take(roots: &Roots) -> Result<DeployLock, String> {
        let dir = roots.runtime().unwrap_or_else(|| roots.cache.clone());
        fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let path = dir.join("deploy.lock");
        let f = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        use std::os::unix::io::AsRawFd;
        let started = Instant::now();
        loop {
            if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                return Ok(DeployLock(f));
            }
            if started.elapsed() > Duration::from_secs(20) {
                return Err("another install is in progress (deploy.lock is held)".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

// ---------------------------------------------------------------- rollback

/// Go back to the build the last install replaced, keeping the current one as
/// the new "previous" so a rollback can itself be undone. Obeys the same rule
/// as an install ([`Sessions::allow`]), judged on the wire recorded for the
/// kept build, and cancels any staged build: a person who went back did not
/// ask for the staged one to come forward at the next login.
pub(crate) fn rollback(
    roots: &Roots,
    install: &Install,
    this: &BuildInfo,
    sessions: &Sessions,
) -> Result<Installed, String> {
    let mut rec = load_record(roots);
    let kept = rec
        .previous
        .clone()
        .ok_or("there is no previous build to go back to")?;
    if !kept.path.is_file() {
        return Err(format!(
            "the previous build {} is gone",
            kept.path.display()
        ));
    }
    sessions.allow(rollback_compat(this, &kept))?;
    let backup = backup(roots, &this.version)?;
    let _lock = DeployLock::take(roots)?;
    let me = std::env::current_exe().unwrap_or_default();
    let (now_kept, is_this) = match install {
        Install::AppImage { image } => {
            let dir = image.parent().ok_or("the AppImage has no directory")?;
            let swap = dir.join(format!(".td-rollback-{}", std::process::id()));
            fs::rename(image, &swap)
                .map_err(|e| format!("cannot move {}: {e}", image.display()))?;
            if let Err(e) = fs::rename(&kept.path, image) {
                let _ = fs::rename(&swap, image);
                return Err(format!("cannot restore {}: {e}", kept.path.display()));
            }
            // The replaced image is now the one to come back to. Where it
            // could not be given the kept name, it stays where it is, and the
            // record says so rather than naming a file that is not there.
            let at = match fs::rename(&swap, &kept.path) {
                Ok(()) => kept.path.clone(),
                Err(_) => swap,
            };
            (at, true)
        }
        Install::Launcher {
            launcher, target, ..
        } => {
            let current = target.clone().ok_or("the launcher is not a link")?;
            let now_target = fs::read_link(launcher)
                .ok()
                .map(|t| absolutise(launcher, &t));
            if now_target.as_ref() != Some(&current) {
                return Err("the launcher changed since this panel looked; check again".into());
            }
            repoint(launcher, &kept.path)?;
            let is_this = same_file(&current, &me);
            (current, is_this)
        }
        _ => return Err("this install is not one TD updates".into()),
    };
    rec.previous = Some(Kept {
        describe: if is_this {
            this.describe()
        } else {
            now_kept
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        },
        wire: if is_this { this.wire.clone() } else { None },
        backup: Some(backup.clone()),
        path: now_kept,
    });
    drop_pending(roots, &mut rec, None);
    rec.last_backup = Some(backup.clone());
    save_record(roots, &rec)?;
    Ok(Installed {
        describe: kept.describe,
        entry: install.entry().expect("can_install"),
        previous: rec.previous,
        backup,
    })
}

/// Whether going back to the kept build keeps the session host: its wire as
/// recorded when it was replaced, against this window's.
pub(crate) fn rollback_compat(this: &BuildInfo, kept: &Kept) -> Compat {
    match (&this.wire, &kept.wire) {
        (Some(a), Some(b)) if a == b => Compat::Same,
        (Some(_), Some(_)) => Compat::Different,
        _ => Compat::Unknown,
    }
}

// ---------------------------------------------------------------- staging

/// The bytes a new window would start from right now, for telling later
/// whether the install was moved on by something else.
fn entry_digest(install: &Install) -> Option<String> {
    match install {
        Install::AppImage { image } => sha256_file(image).ok(),
        Install::Launcher { launcher, .. } => sha256_file(launcher).ok(),
        _ => None,
    }
}

/// Keep a checked build for the next cold start. The file stays in staging;
/// nothing a new window starts from is touched.
pub(crate) fn stage(roots: &Roots, p: &Prepared, install: &Install) -> Result<(), String> {
    let mut rec = load_record(roots);
    let keep = p.image.parent();
    drop_pending(roots, &mut rec, keep);
    rec.pending = Some(Pending {
        staged: p.image.clone(),
        describe: p
            .info
            .as_ref()
            .map(BuildInfo::describe)
            .unwrap_or_else(|| p.offer.describe()),
        channel: p.offer.channel,
        sha256: p.sha256.clone(),
        over: entry_digest(install),
    });
    save_record(roots, &rec)
}

/// Choose a channel. Changing it cancels a build staged from the other one.
pub(crate) fn set_channel(roots: &Roots, channel: Channel) -> Result<Choices, String> {
    let mut c = load_choices(roots);
    if c.channel != channel {
        let mut rec = load_record(roots);
        if rec.pending.is_some() {
            drop_pending(roots, &mut rec, None);
            save_record(roots, &rec)?;
        }
    }
    c.channel = channel;
    save_choices(roots, &c)?;
    Ok(c)
}

/// At a cold start — no session host alive anywhere on this machine — install
/// a staged build and return where the new build now starts from, so the
/// caller can run it instead of this one. `None` when there is nothing to do,
/// or when doing it now could put a new window beside an old host.
///
/// A staged build is dropped, not installed, when its file changed, when the
/// install was moved on by something else since it was staged (another
/// update, `land.sh`, a rollback), or when the channel it came from is no
/// longer the chosen one.
pub(crate) fn apply_pending_at_cold_start(
    roots: &Roots,
    exe: &Path,
    sessions: &Sessions,
) -> Option<PathBuf> {
    let rec = load_record(roots);
    let pending = rec.pending.clone()?;
    if !sessions.alive.is_empty() {
        return None;
    }
    let report = |why: String| {
        eprintln!("terminal-delight: the staged update was not installed: {why}");
        None
    };
    let (appimage, appdir) = appimage_env();
    let install = detect(roots, exe, appimage.as_deref(), appdir.as_deref());
    let stale = if sha256_file(&pending.staged).ok().as_deref() != Some(pending.sha256.as_str()) {
        Some("the staged file changed or is gone")
    } else if pending.over.is_some() && entry_digest(&install) != pending.over {
        Some("Terminal Delight was changed another way since it was staged")
    } else if load_choices(roots).channel != pending.channel {
        Some("it came from a channel that is no longer chosen")
    } else {
        None
    };
    if let Some(why) = stale {
        let mut rec = rec;
        drop_pending(roots, &mut rec, None);
        let _ = save_record(roots, &rec);
        return report(why.into());
    }
    if !install.can_install() {
        return report("this install is not one TD updates".into());
    }
    let dir = pending.staged.parent()?.to_path_buf();
    let binary = match extract_binary(&dir, &pending.staged) {
        Ok(b) => b,
        Err(e) => return report(e),
    };
    let info = match smoke(&binary) {
        Ok(i) => i,
        Err(e) => return report(e),
    };
    let this = BuildInfo::this();
    let offer = Offer {
        channel: pending.channel,
        tag: String::new(),
        version: info.as_ref().map(|i| i.version.clone()).unwrap_or_default(),
        sha: info.as_ref().and_then(|i| i.sha.clone()),
        date: None,
        branch: None,
        notes: String::new(),
        page: String::new(),
        asset: None,
        checksum: None,
        size: None,
        relation: Relation::Unknown,
        ahead_by: None,
        host_restart: false,
    };
    let p = Prepared {
        compat: compat(&this, info.as_ref()),
        offer,
        image: pending.staged.clone(),
        binary,
        sha256: pending.sha256.clone(),
        info,
    };
    match self::install(roots, &install, &p, &this, sessions) {
        Ok(done) => {
            let _ = fs::remove_dir_all(&dir);
            eprintln!(
                "terminal-delight: installed the staged update {}",
                done.describe
            );
            Some(done.entry)
        }
        Err(e) => report(e),
    }
}

/// Every session whose host socket answers in any way. A host that refuses
/// our protocol, or does not answer, is still a host: counting only the ones
/// that answer properly would let a cold-start install run beside it.
pub(crate) fn hosts_alive() -> Vec<String> {
    crate::hostctl::sockets_present()
        .into_iter()
        .filter(|k| {
            !matches!(
                crate::hostctl::probe_host(k),
                crate::hostctl::HostProbe::NoSocket
            )
        })
        .collect()
}

/// Sessions whose host runs a build nothing here can vouch for.
///
/// A window-only update repoints what every NEW window starts from, not just
/// this one's. Another workspace's next window would then meet that
/// workspace's host, so "same wire as this window" is not enough on its own.
/// A host is vouched for when it runs the very bytes of this program, or of
/// the host this session's window is already working with. Bytes are compared
/// rather than asking the host's program, because an older build may not know
/// `--build-info` and must not be started to find out. A host whose program
/// cannot be read, or that does not answer properly, is not vouched for.
pub(crate) fn unvouched_hosts(this_exe: &Path, own_session: Option<&str>) -> Vec<String> {
    use crate::hostctl::{probe_host, sockets_present, HostProbe};
    let exe_of = |pid: u32| -> Option<String> {
        (pid > 0)
            .then(|| sha256_file(Path::new(&format!("/proc/{pid}/exe"))).ok())
            .flatten()
    };
    let mut vouched: Vec<String> = sha256_file(this_exe).ok().into_iter().collect();
    if let Some(HostProbe::Live { host, .. }) = own_session.map(probe_host) {
        vouched.extend(exe_of(host));
    }
    sockets_present()
        .into_iter()
        .filter(|k| Some(k.as_str()) != own_session)
        .filter(|k| match probe_host(k) {
            HostProbe::Live { host, .. } => exe_of(host).is_none_or(|s| !vouched.contains(&s)),
            HostProbe::NoSocket => false,
            HostProbe::Unresponsive | HostProbe::Skewed => true,
        })
        .collect()
}

// ---------------------------------------------------------------- restart

/// Start `entry` as this session's window once this process has exited, then
/// let the caller quit. The waiting is done by a detached shell so it
/// survives this process; it gives up waiting after fifteen seconds and starts
/// the window anyway, which at worst puts two windows on one session — a
/// state TD already handles — rather than none.
pub(crate) fn relaunch_after_exit(entry: &Path, session: &str) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let me = std::process::id().to_string();
    let mut c = Command::new("/bin/sh");
    c.args([
        "-c",
        "pid=$1; shift; i=0; while kill -0 \"$pid\" 2>/dev/null && [ $i -lt 150 ]; do sleep 0.1; i=$((i+1)); done; exec \"$@\"",
        "td-update-relaunch",
        &me,
    ])
    .arg(entry)
    .env("TD_SESSION", session)
    .env_remove("TD_SCRATCH")
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    unsafe {
        c.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    c.spawn()
        .map(|_| ())
        .map_err(|e| format!("cannot start the new window: {e}"))
}

pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Whether a daily check is due.
pub(crate) fn daily_due(c: &Choices, r: &Record, now: u64) -> bool {
    c.check_daily == Some(true)
        && r.last_check
            .is_none_or(|t| now.saturating_sub(t) >= 24 * 3600)
}

// ---------------------------------------------------------------- CLI

const HELP: &str =
    "Usage: terminal-delight update [status|check|install|rollback|channel|backups] [--json]

  status              what is running, how it was installed, and the channel
  check               ask the channel for a newer build (nothing is downloaded)
  install [--now]     download, check and install the newest build on the channel.
                      A build a running session host could not take (a different
                      window↔host wire, or another session's host on a build TD
                      cannot vouch for) is staged for the next cold start. --now
                      installs it at once, which TD allows only while no session
                      host is running: close Terminal Delight first.
  rollback            go back to the build the last install replaced
  channel stable|dev  choose the channel (dev is experimental)
  backups             list the settings backups taken before each install

The new build is used by the next window. Restart the window from
⚙ CONFIG › UPDATES, or close and reopen it; terminals keep running.";

pub(crate) fn run_cli(args: &[String]) -> i32 {
    let json = args.iter().any(|a| a == "--json");
    let now = args.iter().any(|a| a == "--now");
    let words: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|a| !a.starts_with("--"))
        .collect();
    let roots = match Roots::current() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("terminal-delight update: {e}");
            return 1;
        }
    };
    let this = BuildInfo::this();
    let exe = std::env::current_exe().unwrap_or_default();
    let (appimage, appdir) = appimage_env();
    let inst = detect(&roots, &exe, appimage.as_deref(), appdir.as_deref());
    let choices = load_choices(&roots);
    // Inside a TD pane this names the session whose host the caller's window
    // is attached to; from anywhere else there is none.
    let own = std::env::var("TD_SESSION").ok().filter(|s| !s.is_empty());
    let say = |v: serde_json::Value, text: String| {
        if json {
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        } else {
            println!("{text}");
        }
    };
    let fail = |e: String| {
        eprintln!("terminal-delight update: {e}");
        1
    };
    match words.as_slice() {
        [] | ["status"] => {
            let rec = load_record(&roots);
            say(
                serde_json::json!({"build": this, "install": inst, "channel": choices.channel, "check_daily": choices.check_daily, "record": rec}),
                format!(
                    "running   {}\ninstalled {}\nchannel   {}\nprevious  {}\npending   {}",
                    this.describe(),
                    describe_install(&inst),
                    choices.channel.word(),
                    rec.previous
                        .as_ref()
                        .map_or("none".into(), |k| k.describe.clone()),
                    rec.pending
                        .as_ref()
                        .map_or("none".into(), |p| p.describe.clone()),
                ),
            );
            0
        }
        ["check"] => match check(choices.channel, &this) {
            Ok(c) => {
                let mut rec = load_record(&roots);
                rec.last_check = Some(now_unix());
                let _ = save_record(&roots, &rec);
                let text = match &c.offer {
                    Some(o) => format!(
                        "{} channel: {} ({:?}){}",
                        c.channel.word(),
                        o.describe(),
                        o.relation,
                        o.not_installable()
                            .map(|w| format!("\nnot installable here: {w}"))
                            .unwrap_or_default()
                    ),
                    None => format!(
                        "{} channel: {}",
                        c.channel.word(),
                        c.none.clone().unwrap_or_default()
                    ),
                };
                say(serde_json::to_value(&c).unwrap_or_default(), text);
                0
            }
            Err(e) => fail(e),
        },
        ["install"] => {
            if !inst.can_install() {
                return fail(format!("{} — {}", describe_install(&inst), refusal(&inst)));
            }
            let c = match check(choices.channel, &this) {
                Ok(c) => c,
                Err(e) => return fail(e),
            };
            let Some(offer) = c.offer else {
                return fail(c.none.unwrap_or_else(|| "nothing to install".into()));
            };
            if !offer.is_update() {
                say(
                    serde_json::json!({"up_to_date": true, "offer": offer}),
                    format!("already up to date ({})", offer.describe()),
                );
                return 0;
            }
            let p = match prepare(&roots, &offer, &this) {
                Ok(p) => p,
                Err(e) => return fail(e),
            };
            let sessions = Sessions::now(own.as_deref());
            if sessions.needs_cold_start(p.compat) && !now {
                let why = if p.compat != Compat::Same {
                    "it changes how the window talks to your terminals".to_string()
                } else {
                    format!(
                        "the session host of {} runs a build TD cannot vouch for",
                        sessions.unvouched.join(", ")
                    )
                };
                return match stage(&roots, &p, &inst) {
                    Ok(()) => {
                        say(
                            serde_json::json!({"staged": true, "compat": p.compat, "unvouched": sessions.unvouched, "describe": offer.describe()}),
                            format!("{} is checked and staged: {why}, so it installs at the next cold start, when no session host is running", offer.describe()),
                        );
                        0
                    }
                    Err(e) => fail(e),
                };
            }
            match install(&roots, &inst, &p, &this, &sessions) {
                Ok(done) => {
                    let _ =
                        fs::remove_dir_all(p.image.parent().unwrap_or(Path::new("/nonexistent")));
                    say(
                        serde_json::to_value(&done).unwrap_or_default(),
                        format!(
                            "installed {}; settings backed up to {}",
                            done.describe,
                            done.backup.display()
                        ),
                    );
                    0
                }
                Err(e) => fail(e),
            }
        }
        ["rollback"] => {
            if !inst.can_install() {
                return fail(refusal(&inst));
            }
            match rollback(&roots, &inst, &this, &Sessions::now(own.as_deref())) {
                Ok(done) => {
                    say(
                        serde_json::to_value(&done).unwrap_or_default(),
                        format!(
                            "back on {}; settings backed up to {}",
                            done.describe,
                            done.backup.display()
                        ),
                    );
                    0
                }
                Err(e) => fail(e),
            }
        }
        ["channel", which] => {
            let channel = match *which {
                "stable" => Channel::Stable,
                "dev" => Channel::Dev,
                other => return fail(format!("unknown channel `{other}` (stable or dev)")),
            };
            match set_channel(&roots, channel) {
                Ok(_) => {
                    say(
                        serde_json::json!({"channel": channel}),
                        format!("channel: {}", channel.word()),
                    );
                    0
                }
                Err(e) => fail(e),
            }
        }
        ["backups"] => {
            let base = backups_dir(&roots);
            let mut list: Vec<String> = fs::read_dir(&base)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path().display().to_string())
                .filter(|p| p.contains("/update-"))
                .collect();
            list.sort();
            say(
                serde_json::json!({"dir": base, "backups": list}),
                if list.is_empty() {
                    "no backups yet".into()
                } else {
                    list.join("\n")
                },
            );
            0
        }
        ["help"] => {
            println!("{HELP}");
            0
        }
        _ => {
            eprintln!("{HELP}");
            2
        }
    }
}

pub(crate) fn describe_install(i: &Install) -> String {
    match i {
        Install::AppImage { image } => format!("AppImage · {}", image.display()),
        Install::Launcher { launcher, .. } => format!("launcher build · {}", launcher.display()),
        Install::Source { checkout, .. } => format!("source checkout · {}", checkout.display()),
        Install::Package { exe } => format!("system package · {}", exe.display()),
        Install::Unknown { exe, .. } => format!("unrecognised · {}", exe.display()),
    }
}

/// Why this install is not updated in-app, and what to do instead.
pub(crate) fn refusal(i: &Install) -> String {
    match i {
        Install::Source { checkout, .. } => format!(
            "TD never pulls or rebuilds a checkout. To update it yourself:\ngit -C {} pull --ff-only\ncargo build --release --manifest-path {}/app/Cargo.toml",
            checkout.display(),
            checkout.display()
        ),
        Install::Package { .. } => "your package manager owns this install — update it there".into(),
        Install::Unknown { why, .. } => why.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots(home: &Path) -> Roots {
        let h = home.to_path_buf();
        Roots::from_env(&move |k| match k {
            "HOME" => Some(h.clone().into_os_string()),
            _ => None,
        })
        .unwrap()
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("td-update-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn versions_order_the_way_semver_does() {
        let v = |s| Version::parse(s).unwrap();
        assert!(v("0.4.0-rc.3") < v("0.4.0"));
        assert!(v("0.4.0-rc.2") < v("0.4.0-rc.10"));
        assert!(v("v0.3.0") < v("0.4.0-rc.1"));
        assert!(v("0.5.0") > v("0.4.9"));
        assert_eq!(v("v1.2.3"), v("1.2.3+build.7"));
        assert!(Version::parse("dev-build").is_none());
        assert!(Version::parse("1.2").is_none());
        assert!(Version::parse("1.2.3-").is_none());
    }

    /// Drafts and prereleases are never the stable offer, however new.
    #[test]
    fn the_stable_offer_skips_drafts_and_prereleases() {
        let list = serde_json::json!([
            {"tag_name": "v0.4.0-rc.3", "draft": false, "prerelease": true},
            {"tag_name": "v0.5.0", "draft": true, "prerelease": false},
            {"tag_name": "dev-build", "draft": false, "prerelease": true},
            {"tag_name": "v0.3.0", "draft": false, "prerelease": false},
            {"tag_name": "v0.2.1", "draft": false, "prerelease": false},
        ]);
        let (v, r) = newest_stable(&list).unwrap();
        assert_eq!(v, Version::parse("0.3.0").unwrap());
        assert_eq!(r["tag_name"], "v0.3.0");
        // A missing flag is not "false": a release that does not say is skipped,
        // whichever of the two it leaves out.
        for unsaid in [
            serde_json::json!([{"tag_name": "v9.0.0", "prerelease": false}]),
            serde_json::json!([{"tag_name": "v9.0.0", "draft": false}]),
        ] {
            assert!(newest_stable(&unsaid).is_none(), "{unsaid}");
        }
    }

    #[test]
    fn a_checksum_line_yields_its_digest_only_when_it_is_one() {
        let d = "a".repeat(64);
        assert_eq!(
            parse_checksum(&format!("{d}  terminal-delight-x86_64.AppImage\n")),
            Some(d.clone())
        );
        assert_eq!(parse_checksum(&d.to_uppercase()), Some(d));
        assert_eq!(parse_checksum("not-a-digest file"), None);
        assert_eq!(parse_checksum(""), None);
    }

    /// An unknown wire on either side is never read as a match — that would
    /// end terminals with no warning.
    #[test]
    fn compatibility_needs_both_wires_and_the_same_core() {
        let mut a = BuildInfo::this();
        a.wire = Some("abc".into());
        let mut b = a.clone();
        assert_eq!(compat(&a, Some(&b)), Compat::Same);
        b.wire = Some("abd".into());
        assert_eq!(compat(&a, Some(&b)), Compat::Different);
        b.wire = None;
        assert_eq!(compat(&a, Some(&b)), Compat::Unknown);
        assert_eq!(compat(&a, None), Compat::Unknown);
        b.wire = a.wire.clone();
        b.core = "other-core".into();
        assert_eq!(compat(&a, Some(&b)), Compat::Different);
    }

    #[test]
    fn this_build_carries_a_wire_fingerprint() {
        let me = BuildInfo::this();
        assert!(me.wire.as_deref().is_some_and(|w| w.len() == 16), "{me:?}");
        let back: BuildInfo = serde_json::from_str(&build_info_json()).unwrap();
        assert_eq!(back, me);
    }

    #[test]
    fn an_offer_without_a_checksum_is_shown_but_not_installable() {
        let mut o = offer(Channel::Stable, Relation::Newer);
        assert!(o.not_installable().is_none());
        o.checksum = None;
        assert!(o.not_installable().unwrap().contains("no checksum"));
        o.asset = None;
        assert!(o.not_installable().unwrap().contains("no Linux AppImage"));
    }

    /// Switching back to stable never downgrades: an older or unrelated stable
    /// is not an update.
    #[test]
    fn only_a_newer_stable_is_an_update() {
        for (r, want) in [
            (Relation::Newer, true),
            (Relation::Same, false),
            (Relation::Older, false),
            (Relation::Diverged, false),
            (Relation::Unknown, false),
        ] {
            assert_eq!(offer(Channel::Stable, r).is_update(), want, "{r:?}");
        }
        assert!(!offer(Channel::Dev, Relation::Same).is_update());
        assert!(offer(Channel::Dev, Relation::Diverged).is_update());
    }

    fn offer(channel: Channel, relation: Relation) -> Offer {
        Offer {
            channel,
            tag: "v0.5.1".into(),
            version: "0.5.1".into(),
            sha: None,
            date: None,
            branch: None,
            notes: String::new(),
            page: String::new(),
            asset: Some("https://example.invalid/a".into()),
            checksum: Some("https://example.invalid/a.sha256".into()),
            size: None,
            relation,
            ahead_by: None,
            host_restart: false,
        }
    }

    #[test]
    fn installs_are_told_apart_by_where_the_program_lives() {
        let home = tmp("detect");
        let r = roots(&home);
        // An AppImage names itself.
        let image = home.join("Apps/td.AppImage");
        fs::create_dir_all(image.parent().unwrap()).unwrap();
        fs::write(&image, b"x").unwrap();
        // An AppImage names itself, but only to a program running inside it:
        // a launcher build started from one of its panes inherits APPIMAGE.
        let mount = home.join("mount");
        fs::create_dir_all(mount.join("usr/bin")).unwrap();
        let inside = mount.join("usr/bin/terminal-delight");
        fs::write(&inside, b"x").unwrap();
        assert!(matches!(
            detect(&r, &inside, Some(&image), Some(&mount)),
            Install::AppImage { .. }
        ));
        let elsewhere = home.join("elsewhere/terminal-delight");
        fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
        fs::write(&elsewhere, b"x").unwrap();
        assert!(!matches!(
            detect(&r, &elsewhere, Some(&image), Some(&mount)),
            Install::AppImage { .. }
        ));
        assert!(!matches!(
            detect(&r, &inside, Some(&image), None),
            Install::AppImage { .. }
        ));
        // A build in the library, reached through the launcher.
        fs::create_dir_all(&r.builds).unwrap();
        fs::create_dir_all(&r.bin).unwrap();
        let build = r.builds.join("td-abcdef12-x");
        fs::write(&build, b"x").unwrap();
        std::os::unix::fs::symlink(&build, r.bin.join("terminal-delight")).unwrap();
        match detect(&r, &build, None, None) {
            Install::Launcher { target, .. } => assert_eq!(target, Some(build.clone())),
            other => panic!("{other:?}"),
        }
        // A checkout's target directory.
        let co = home.join("src/td");
        fs::create_dir_all(co.join(".git")).unwrap();
        fs::create_dir_all(co.join("app/target/release")).unwrap();
        let bin = co.join("app/target/release/terminal-delight");
        fs::write(&bin, b"x").unwrap();
        assert!(matches!(
            detect(&r, &bin, None, None),
            Install::Source { .. }
        ));
        // A system path, and anything else.
        assert!(matches!(
            detect(&r, Path::new("/usr/bin/terminal-delight"), None, None),
            Install::Package { .. }
        ));
        let loose = home.join("bin/terminal-delight");
        fs::create_dir_all(loose.parent().unwrap()).unwrap();
        fs::write(&loose, b"x").unwrap();
        assert!(matches!(
            detect(&r, &loose, None, None),
            Install::Unknown { .. }
        ));
        let _ = fs::remove_dir_all(&home);
    }

    /// The launcher is repointed in one rename and the old target is kept, so
    /// rollback restores it exactly; a launcher moved by someone else in the
    /// meantime is refused rather than overwritten.
    #[test]
    fn a_launcher_install_keeps_the_old_build_and_rolls_back() {
        let home = tmp("launcher");
        let r = roots(&home);
        fs::create_dir_all(&r.builds).unwrap();
        fs::create_dir_all(&r.bin).unwrap();
        let old = r.builds.join("td-11111111-old");
        fs::write(&old, b"old build").unwrap();
        let launcher = r.bin.join("terminal-delight");
        std::os::unix::fs::symlink(&old, &launcher).unwrap();
        let new_bin = home.join("staged-binary");
        fs::write(&new_bin, b"new build").unwrap();
        let inst = detect(&r, &old, None, None);
        let mut o = offer(Channel::Dev, Relation::Newer);
        o.sha = Some("22222222deadbeef".into());
        let kept = kept_of(activate(&r, &inst, &new_bin, &new_bin, &o, None).unwrap());
        assert_eq!(kept, old);
        let now = fs::canonicalize(&launcher).unwrap();
        assert_eq!(
            now,
            fs::canonicalize(r.builds.join("td-22222222-dev")).unwrap()
        );
        assert_eq!(fs::read(&now).unwrap(), b"new build");
        assert_eq!(fs::read(&old).unwrap(), b"old build");

        // Rollback goes back, and keeps what it left.
        let mut rec = Record::default();
        rec.previous = Some(Kept {
            path: old.clone(),
            describe: "old".into(),
            wire: None,
            backup: None,
        });
        save_record(&r, &rec).unwrap();
        let inst = detect(&r, &now, None, None);
        let this = BuildInfo::this();
        let back = rollback(&r, &inst, &this, &Sessions::default()).unwrap();
        assert_eq!(back.describe, "old");
        assert_eq!(
            fs::canonicalize(&launcher).unwrap(),
            fs::canonicalize(&old).unwrap()
        );
        assert_eq!(load_record(&r).previous.unwrap().path, now);
        assert!(back.backup.join("manifest.json").is_file());

        // A launcher moved since detection is refused.
        let stale = Install::Launcher {
            launcher: launcher.clone(),
            library: r.builds.clone(),
            target: Some(now.clone()),
        };
        let err = activate(&r, &stale, &new_bin, &new_bin, &o, None).unwrap_err();
        assert!(err.contains("changed"), "{err}");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn an_appimage_install_keeps_the_previous_image_beside_it() {
        let home = tmp("appimage");
        let r = roots(&home);
        let image = home.join("Apps/terminal-delight-x86_64.AppImage");
        fs::create_dir_all(image.parent().unwrap()).unwrap();
        fs::write(&image, b"old image").unwrap();
        let staged = home.join("new.AppImage");
        fs::write(&staged, b"new image").unwrap();
        let inst = Install::AppImage {
            image: image.clone(),
        };
        let kept = kept_of(
            activate(
                &r,
                &inst,
                &staged,
                &staged,
                &offer(Channel::Stable, Relation::Newer),
                None,
            )
            .unwrap(),
        );
        assert_eq!(fs::read(&image).unwrap(), b"new image");
        assert_eq!(fs::read(&kept).unwrap(), b"old image");
        // Installing the same build again touches nothing, so the way back
        // still leads to the old image rather than to a copy of the new one.
        assert!(matches!(
            activate(
                &r,
                &inst,
                &staged,
                &staged,
                &offer(Channel::Stable, Relation::Newer),
                None
            )
            .unwrap(),
            Activated::AlreadyThere
        ));
        assert_eq!(fs::read(&kept).unwrap(), b"old image");
        assert!(fs::metadata(&image).unwrap().permissions().mode() & 0o111 != 0);
        let mut rec = Record::default();
        rec.previous = Some(Kept {
            path: kept.clone(),
            describe: "old".into(),
            wire: None,
            backup: None,
        });
        save_record(&r, &rec).unwrap();
        rollback(&r, &inst, &BuildInfo::this(), &Sessions::default()).unwrap();
        assert_eq!(fs::read(&image).unwrap(), b"old image");
        assert_eq!(fs::read(&kept).unwrap(), b"new image");
        let _ = fs::remove_dir_all(&home);
    }

    /// The backup carries settings and the small records, never the bulky
    /// history, and lands outside TD's own directories.
    #[test]
    fn a_backup_copies_settings_and_small_records_only() {
        let home = tmp("backup");
        let r = roots(&home);
        fs::create_dir_all(r.config.join("sessions")).unwrap();
        fs::write(r.config.join("theme.toml"), b"theme").unwrap();
        fs::write(r.config.join("sessions/1.toml"), b"layout").unwrap();
        fs::create_dir_all(r.state.join("threshing-floor")).unwrap();
        fs::write(r.state.join("threshing-floor/orgs.toml"), b"orgs").unwrap();
        fs::create_dir_all(r.state.join("conversations")).unwrap();
        fs::write(r.state.join("conversations/big.jsonl"), b"history").unwrap();
        let dir = backup(&r, "0.4.0-rc.3").unwrap();
        assert!(!r.is_inside_td(&dir), "{}", dir.display());
        assert_eq!(fs::read(dir.join("config/theme.toml")).unwrap(), b"theme");
        assert_eq!(
            fs::read(dir.join("config/sessions/1.toml")).unwrap(),
            b"layout"
        );
        assert_eq!(
            fs::read(dir.join("state/threshing-floor/orgs.toml")).unwrap(),
            b"orgs"
        );
        assert!(!dir.join("state/conversations").exists());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn the_daily_check_is_off_until_turned_on_and_then_daily() {
        let mut c = Choices::default();
        let mut r = Record::default();
        assert!(!daily_due(&c, &r, 1_000_000));
        c.check_daily = Some(false);
        assert!(!daily_due(&c, &r, 1_000_000));
        c.check_daily = Some(true);
        assert!(daily_due(&c, &r, 1_000_000));
        r.last_check = Some(1_000_000 - 3600);
        assert!(!daily_due(&c, &r, 1_000_000));
        r.last_check = Some(1_000_000 - 25 * 3600);
        assert!(daily_due(&c, &r, 1_000_000));
    }

    /// A staged build is installed only when no session host is alive, and a
    /// staged file that changed is dropped, not installed.
    #[test]
    fn a_staged_build_waits_for_a_cold_start_and_must_be_unchanged() {
        let home = tmp("pending");
        let r = roots(&home);
        let staged = home.join("staged.AppImage");
        fs::write(&staged, b"staged").unwrap();
        let mut rec = Record::default();
        rec.pending = Some(Pending {
            staged: staged.clone(),
            describe: "x".into(),
            channel: Channel::Dev,
            sha256: sha256_file(&staged).unwrap(),
            over: None,
        });
        save_record(&r, &rec).unwrap();
        set_channel_quietly(&r, Channel::Dev);
        let exe = home.join("bin/terminal-delight");
        let alive = Sessions {
            alive: vec!["1".into()],
            unvouched: vec![],
        };
        assert_eq!(apply_pending_at_cold_start(&r, &exe, &alive), None);
        assert!(
            load_record(&r).pending.is_some(),
            "a live host leaves it staged"
        );
        fs::write(&staged, b"tampered").unwrap();
        assert_eq!(
            apply_pending_at_cold_start(&r, &exe, &Sessions::default()),
            None
        );
        assert!(
            load_record(&r).pending.is_none(),
            "a changed file is dropped"
        );
        let _ = fs::remove_dir_all(&home);
    }

    /// The cold-start path end to end on a launcher install: the staged image
    /// is extracted, its program answers --build-info, the settings are backed
    /// up, the launcher moves, and the staged download is cleared.
    #[test]
    fn a_staged_build_installs_at_a_cold_start() {
        let home = tmp("coldstart");
        let r = roots(&home);
        fs::create_dir_all(&r.builds).unwrap();
        fs::create_dir_all(&r.bin).unwrap();
        fs::create_dir_all(&r.config).unwrap();
        fs::write(r.config.join("theme.toml"), b"theme").unwrap();
        let old = r.builds.join("td-11111111-old");
        fs::write(&old, b"old").unwrap();
        let launcher = r.bin.join("terminal-delight");
        std::os::unix::fs::symlink(&old, &launcher).unwrap();
        // A stand-in AppImage whose program reports a build with another wire.
        let stage_dir = staging(&r).join("dev-test");
        fs::create_dir_all(&stage_dir).unwrap();
        let program = home.join("program");
        fs::write(
            &program,
            "#!/bin/sh\necho '{\"schema\":\"td-build/1\",\"version\":\"9.9.9\",\"sha\":\"abcdef0123456789\",\"date\":null,\"dirty\":false,\"wire\":\"ffffffffffffffff\",\"core\":\"x\"}'\n",
        )
        .unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        let image = stage_dir.join(ASSET);
        fs::write(
            &image,
            format!(
                "#!/bin/sh\nmkdir -p squashfs-root/usr/bin && cp '{}' squashfs-root/usr/bin/terminal-delight\n",
                program.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&image, fs::Permissions::from_mode(0o755)).unwrap();
        let rec = Record {
            pending: Some(Pending {
                staged: image.clone(),
                describe: "9.9.9".into(),
                channel: Channel::Dev,
                sha256: sha256_file(&image).unwrap(),
                over: None,
            }),
            ..Record::default()
        };
        save_record(&r, &rec).unwrap();

        set_channel_quietly(&r, Channel::Dev);
        let entry = apply_pending_at_cold_start(&r, &old, &Sessions::default()).expect("installed");
        assert_eq!(entry, launcher);
        let now = fs::canonicalize(&launcher).unwrap();
        assert_eq!(
            now,
            fs::canonicalize(r.builds.join("td-abcdef01-dev")).unwrap()
        );
        let rec = load_record(&r);
        assert!(rec.pending.is_none());
        assert_eq!(rec.previous.unwrap().path, old);
        assert!(rec.last_backup.unwrap().join("config/theme.toml").is_file());
        assert!(!stage_dir.exists(), "the staged download is cleared");
        let _ = fs::remove_dir_all(&home);
    }

    fn kept_of(a: Activated) -> PathBuf {
        match a {
            Activated::Replaced { kept: Some(k), .. } => k,
            _ => panic!("nothing was replaced"),
        }
    }

    fn set_channel_quietly(r: &Roots, channel: Channel) {
        save_choices(
            r,
            &Choices {
                channel,
                check_daily: None,
            },
        )
        .unwrap();
    }

    /// The one rule: a swap keeps every live host working, or happens with
    /// no host alive. Unknown is never taken for safe.
    #[test]
    fn a_swap_that_could_strand_a_host_waits_until_none_is_running() {
        let none = Sessions::default();
        let one = Sessions {
            alive: vec!["1".into()],
            unvouched: vec![],
        };
        let foreign = Sessions {
            alive: vec!["1".into(), "2".into()],
            unvouched: vec!["2".into()],
        };
        assert!(one.allow(Compat::Same).is_ok());
        assert!(one.allow(Compat::Different).is_err());
        assert!(one.allow(Compat::Unknown).is_err());
        assert!(none.allow(Compat::Different).is_ok());
        assert!(none.allow(Compat::Unknown).is_ok());
        let err = foreign.allow(Compat::Same).unwrap_err();
        assert!(err.contains("2"), "{err}");
        assert!(foreign.needs_cold_start(Compat::Same));
        assert!(!one.needs_cold_start(Compat::Same));
        assert_eq!(foreign.others(Some("1")), 1);
        assert_eq!(foreign.others(None), 2);
    }

    /// Going back to a build whose wire is unknown is refused while a host
    /// runs, and the launcher is left exactly where it was.
    #[test]
    fn a_rollback_to_an_unknown_wire_waits_for_no_host() {
        let home = tmp("rollback-wire");
        let r = roots(&home);
        fs::create_dir_all(&r.builds).unwrap();
        fs::create_dir_all(&r.bin).unwrap();
        let old = r.builds.join("td-11111111-old");
        let new = r.builds.join("td-22222222-new");
        fs::write(&old, b"old").unwrap();
        fs::write(&new, b"new").unwrap();
        let launcher = r.bin.join("terminal-delight");
        std::os::unix::fs::symlink(&new, &launcher).unwrap();
        let rec = Record {
            previous: Some(Kept {
                path: old.clone(),
                describe: "old".into(),
                wire: None,
                backup: None,
            }),
            ..Record::default()
        };
        save_record(&r, &rec).unwrap();
        let inst = detect(&r, &new, None, None);
        let alive = Sessions {
            alive: vec!["1".into()],
            unvouched: vec![],
        };
        let err = rollback(&r, &inst, &BuildInfo::this(), &alive).unwrap_err();
        assert!(err.contains("changes how the window talks"), "{err}");
        assert_eq!(fs::read_link(&launcher).unwrap(), new);
        rollback(&r, &inst, &BuildInfo::this(), &Sessions::default()).unwrap();
        assert_eq!(fs::read_link(&launcher).unwrap(), old);
        let _ = fs::remove_dir_all(&home);
    }

    /// A staged build is cancelled by everything that says the person moved
    /// on: the install changing another way, a channel switch, a rollback.
    #[test]
    fn a_staged_build_is_dropped_when_the_person_moved_on() {
        let home = tmp("staged-moved");
        let r = roots(&home);
        fs::create_dir_all(&r.builds).unwrap();
        fs::create_dir_all(&r.bin).unwrap();
        let a = r.builds.join("td-aaaaaaaa-a");
        let b = r.builds.join("td-bbbbbbbb-b");
        fs::write(&a, b"a").unwrap();
        fs::write(&b, b"b").unwrap();
        let launcher = r.bin.join("terminal-delight");
        std::os::unix::fs::symlink(&a, &launcher).unwrap();
        let inst = detect(&r, &a, None, None);
        let stage_one = |r: &Roots| {
            let dir = staging(r).join("dev-x");
            fs::create_dir_all(&dir).unwrap();
            let image = dir.join(ASSET);
            fs::write(&image, b"staged").unwrap();
            let rec = Record {
                pending: Some(Pending {
                    staged: image.clone(),
                    describe: "x".into(),
                    channel: Channel::Dev,
                    sha256: sha256_file(&image).unwrap(),
                    over: entry_digest(&inst),
                }),
                ..load_record(r)
            };
            save_record(r, &rec).unwrap();
            dir
        };
        set_channel_quietly(&r, Channel::Dev);

        // Moved on another way: the launcher now starts something else.
        let dir = stage_one(&r);
        repoint(&launcher, &b).unwrap();
        assert_eq!(
            apply_pending_at_cold_start(&r, &b, &Sessions::default()),
            None
        );
        assert!(load_record(&r).pending.is_none());
        assert!(!dir.exists(), "its download is removed");

        // A channel switch.
        let dir = stage_one(&r);
        set_channel(&r, Channel::Stable).unwrap();
        assert!(load_record(&r).pending.is_none());
        assert!(!dir.exists());
        set_channel_quietly(&r, Channel::Dev);

        // A rollback.
        let inst_b = detect(&r, &b, None, None);
        let dir = stage_one(&r);
        let rec = Record {
            previous: Some(Kept {
                path: a.clone(),
                describe: "a".into(),
                wire: BuildInfo::this().wire,
                backup: None,
            }),
            ..load_record(&r)
        };
        save_record(&r, &rec).unwrap();
        rollback(&r, &inst_b, &BuildInfo::this(), &Sessions::default()).unwrap();
        assert!(load_record(&r).pending.is_none());
        assert!(!dir.exists());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn choices_read_absent_as_stable_and_unchosen() {
        let home = tmp("choices");
        let r = roots(&home);
        assert_eq!(
            load_choices(&r),
            Choices {
                channel: Channel::Stable,
                check_daily: None
            }
        );
        save_choices(
            &r,
            &Choices {
                channel: Channel::Dev,
                check_daily: Some(true),
            },
        )
        .unwrap();
        assert_eq!(load_choices(&r).channel, Channel::Dev);
        let _ = fs::remove_dir_all(&home);
    }
}
