//! Adding a project from the composer: a machine, then a folder on it, or a
//! repository cloned or started there.
//!
//! Parker, 2026-11-08, pasting MiaAI_lab's fork of T3 Code (*"adding a project,
//! simplified … Pick a machine: this computer, or any SSH host from your config
//! … Click through to a folder and press Add project … Clone or start empty
//! from the same dialog"*): *"Get that functionality for new project into our
//! build of TD"*. Scuttled rather than copied (their fork is MIT, from
//! `MiaAI-Lab/t3code`): their dialog talks to a T3 server it installs on the far
//! machine, and this window has only ssh, the way the composer's far machine
//! does (see `entry.rs`). So a far folder is listed by a short POSIX script run
//! over one ssh hop, and a far clone is `git clone` run over the same hop.
//!
//! The composer's PROJECT list is a depth-one scan of five roots
//! ([`launcher::scan`]); it cannot see a folder deeper down or on another
//! machine. What this adds that the scan cannot find is remembered in
//! `projects.toml`, beside `machines.toml`.
//!
//! Pure, apart from reading a folder or a config file: the dialog's state,
//! keys and drawing are in `addproject_view.rs`.

use crate::entry::{self, Machine};
use crate::launcher;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// The machines: ~/.ssh/config
// ---------------------------------------------------------------------------

/// A `Host` block from an ssh config that could be a machine to work on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshHost {
    /// Every alias on the `Host` line, in its order; the first is what the
    /// dialog says and what `ssh` is given.
    pub aliases: Vec<String>,
    /// `HostName`, when the block names one.
    pub hostname: Option<String>,
    /// `User`, when the block names one.
    pub user: Option<String>,
}

impl SshHost {
    pub fn alias(&self) -> &str {
        &self.aliases[0]
    }

    /// A git remote rather than a machine: a forge's own host, or any block
    /// that logs in as `git`. `github-bfs` in Parker's config is the example —
    /// a key for pushing, with no shell behind it.
    pub fn is_git_host(&self) -> bool {
        const FORGES: &[&str] = &[
            "github.com",
            "ssh.github.com",
            "gitlab.com",
            "altssh.gitlab.com",
            "bitbucket.org",
            "altssh.bitbucket.org",
            "codeberg.org",
            "git.sr.ht",
            "gitea.com",
            "ssh.dev.azure.com",
            "vs-ssh.visualstudio.com",
        ];
        let host = self
            .hostname
            .as_deref()
            .unwrap_or(self.alias())
            .to_ascii_lowercase();
        FORGES.contains(&host.as_str()) || self.user.as_deref() == Some("git")
    }

    /// Whether a machine in `machines.toml` already is this host: its ssh
    /// destination is one of the aliases, with or without a user in front.
    pub fn is_machine(&self, machine: &Machine) -> bool {
        let Some(dest) = machine.ssh.as_deref() else {
            return false;
        };
        let host = dest.rsplit('@').next().unwrap_or(dest);
        self.aliases.iter().any(|a| a == host)
    }
}

/// The hosts an ssh config offers as machines, and how many it names that
/// are git remotes, which are counted rather than silently dropped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SshHosts {
    pub hosts: Vec<SshHost>,
    pub git_hosts: usize,
}

/// One meaningful line of an ssh config.
#[derive(Debug, PartialEq, Eq)]
enum ConfigLine {
    Host(Vec<String>),
    /// A `Match` block: its keys belong to no alias this dialog could list.
    Match,
    Include(Vec<String>),
    Key(String, String),
}

/// Split an ssh config line into its keyword and arguments. ssh accepts
/// `Key value`, `Key=value` and `Key = value`, and double-quoted arguments.
fn config_line(line: &str) -> Option<ConfigLine> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let split = line
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(line.len());
    let key = line[..split].to_ascii_lowercase();
    let rest = line[split..]
        .trim_start_matches(|c: char| c.is_whitespace())
        .trim_start_matches('=')
        .trim();
    let mut args = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    for c in rest.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !word.is_empty() {
                    args.push(std::mem::take(&mut word));
                }
            }
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        args.push(word);
    }
    Some(match key.as_str() {
        "host" => ConfigLine::Host(args),
        "match" => ConfigLine::Match,
        "include" => ConfigLine::Include(args),
        _ => ConfigLine::Key(key, args.into_iter().next().unwrap_or_default()),
    })
}

/// An alias a person could type after `ssh` to reach one machine: no
/// wildcard, no negation, and nothing [`entry::valid_destination`] refuses.
fn plain_alias(pattern: &str) -> bool {
    !pattern.contains(['*', '?', '!', ',']) && entry::valid_destination(pattern)
}

/// The hosts in an ssh config's text, following `Include` through `read`,
/// which is handed each pattern as written and returns the texts it names.
///
/// Only a block whose `Host` line names a plain alias is a machine; a block of
/// patterns (`Host *`, `Host *.lan`) configures many hosts and names none. A
/// `HostName` or `User` is read from the alias's own block, which is the one
/// the person wrote for it. The first block naming an alias wins, as it does
/// for ssh.
pub fn parse_ssh_config(text: &str, read: &mut dyn FnMut(&str) -> Vec<String>) -> SshHosts {
    let mut hosts: Vec<SshHost> = Vec::new();
    collect_hosts(text, read, &mut hosts, 0);
    let mut out = SshHosts::default();
    for host in hosts {
        if host.is_git_host() {
            out.git_hosts += 1;
        } else {
            out.hosts.push(host);
        }
    }
    out
}

fn collect_hosts(
    text: &str,
    read: &mut dyn FnMut(&str) -> Vec<String>,
    hosts: &mut Vec<SshHost>,
    depth: usize,
) {
    // ssh stops at sixteen; a loop of includes should not hang a dialog.
    if depth > 8 {
        return;
    }
    // The block being read: an index into `hosts`, or none for a block that
    // names no alias of its own.
    let mut current: Option<usize> = None;
    for line in text.lines() {
        match config_line(line) {
            None => {}
            Some(ConfigLine::Host(patterns)) => {
                let aliases: Vec<String> = patterns
                    .iter()
                    .filter(|p| plain_alias(p))
                    .filter(|p| !hosts.iter().any(|h| h.aliases.contains(p)))
                    .cloned()
                    .collect();
                current = if aliases.is_empty() {
                    None
                } else {
                    hosts.push(SshHost {
                        aliases,
                        hostname: None,
                        user: None,
                    });
                    Some(hosts.len() - 1)
                };
            }
            Some(ConfigLine::Match) => current = None,
            Some(ConfigLine::Include(paths)) => {
                for path in paths {
                    for included in read(&path) {
                        collect_hosts(&included, read, hosts, depth + 1);
                    }
                }
            }
            Some(ConfigLine::Key(key, value)) => {
                let Some(host) = current.and_then(|i| hosts.get_mut(i)) else {
                    continue;
                };
                match key.as_str() {
                    "hostname" if host.hostname.is_none() => host.hostname = Some(value),
                    "user" if host.user.is_none() => host.user = Some(value),
                    _ => {}
                }
            }
        }
    }
}

/// The files an `Include` names: relative to `~/.ssh`, `~` expanded, and a
/// `*` or `?` in the file name matched against its folder.
fn include_files(pattern: &str, home: &Path) -> Vec<PathBuf> {
    let path = if let Some(rest) = pattern.strip_prefix("~/") {
        home.join(rest)
    } else if pattern.starts_with('/') {
        PathBuf::from(pattern)
    } else {
        home.join(".ssh").join(pattern)
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if !name.contains(['*', '?']) {
        return vec![path];
    }
    let Some(dir) = path.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| glob_match(&name, &e.file_name().to_string_lossy()))
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

/// `*` and `?`, the two wildcards an ssh `Include` uses in practice.
fn glob_match(pattern: &str, name: &str) -> bool {
    fn go(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], n) || (!n.is_empty() && go(p, &n[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &n[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &n[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    go(&p, &n)
}

/// The machines `~/.ssh/config` offers. No file is no hosts, which is not an
/// error; a file that cannot be read is.
pub fn read_ssh_hosts(home: &Path) -> Result<SshHosts, String> {
    let main = home.join(".ssh").join("config");
    let text = match std::fs::read_to_string(&main) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(SshHosts::default()),
        Err(e) => return Err(e.to_string()),
    };
    let mut read = |pattern: &str| -> Vec<String> {
        include_files(pattern, home)
            .iter()
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .collect()
    };
    Ok(parse_ssh_config(&text, &mut read))
}

/// The hosts not yet in `machines.toml`, in the config's order.
pub fn hosts_not_added(hosts: &[SshHost], machines: &[Machine]) -> Vec<SshHost> {
    hosts
        .iter()
        .filter(|h| !machines.iter().any(|m| h.is_machine(m)))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Talking to a far machine
// ---------------------------------------------------------------------------

/// The ssh options every call this dialog makes carries.
///
/// `BatchMode` so nothing ever waits on a password or a host-key question in
/// a terminal nobody can see: a host that needs one is refused with ssh's own
/// reason, and the person is told how to answer it once themselves. A short
/// connect timeout so an unreachable machine says so in seconds. And, when
/// there is a private runtime folder, a shared connection: browsing a far
/// folder is one ssh call per folder, and without it every call pays a whole
/// handshake.
pub fn ssh_args(dest: &str, control_dir: Option<&Path>) -> Vec<String> {
    let mut args: Vec<String> = [
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=8",
        "-o",
        "ServerAliveInterval=10",
        "-o",
        "ServerAliveCountMax=3",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    if let Some(dir) = control_dir {
        args.extend([
            "-o".into(),
            "ControlMaster=auto".into(),
            "-o".into(),
            format!("ControlPath={}/td-ssh-%C", dir.display()),
            "-o".into(),
            "ControlPersist=60".into(),
        ]);
    }
    args.extend(["-T".into(), "--".into(), dest.into()]);
    args
}

/// The folder the shared connection's socket lives in: this user's runtime
/// folder, which nobody else can open. None when there is none, and then each
/// call connects on its own.
pub fn control_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|d| d.is_dir())
}

/// The command string a far shell is handed: `sh -c` running `script` with
/// `args` as `$1`, `$2`…, every word single-quoted. The far account's login
/// shell parses this line, so it is assumed to read POSIX quoting, as the
/// composer's `ssh -t` line already does.
pub fn remote_command(script: &str, args: &[&str]) -> String {
    let mut out = format!("sh -c {} td", launcher::shell_quote(script));
    for a in args {
        out.push(' ');
        out.push_str(&launcher::shell_quote(a));
    }
    out
}

/// The prelude every far script shares: `~` and `~/…` in `$1`-style paths are
/// the far account's home, which only the far side knows.
const HOME_OF: &str = r#"home_of() { case $1 in "~") printf '%s' "$HOME" ;; "~/"*) printf '%s' "$HOME/${1#"~/"}" ;; *) printf '%s' "$1" ;; esac; }
"#;

/// Lists the folders in `$1`: `at <absolute path>` first, then one line per
/// folder, `g` for a git checkout and `d` for the rest; or `missing`.
pub fn list_script() -> String {
    format!(
        r#"{HOME_OF}d=$(home_of "$1")
if [ ! -e "$d" ]; then echo missing; exit 0; fi
cd -- "$d" || exit 3
printf 'at %s\n' "$(pwd)"
for e in * .*; do
  case $e in .|..) continue ;; esac
  [ -d "$e" ] || continue
  if [ -e "$e/.git" ]; then printf 'g %s\n' "$e"; else printf 'd %s\n' "$e"; fi
done
"#
    )
}

/// Makes `$1` (and any folder above it that is missing), then says where it
/// is and whether it is a git checkout.
pub fn make_script() -> String {
    format!(
        r#"{HOME_OF}d=$(home_of "$1")
mkdir -p -- "$d" && cd -- "$d" || exit 3
printf 'at %s\n' "$(pwd)"
if [ -e .git ]; then echo git; fi
"#
    )
}

/// Clones `$1` into `$2`, git's progress on stderr, then says where it is.
pub fn clone_script() -> String {
    format!(
        r#"{HOME_OF}d=$(home_of "$2")
GIT_TERMINAL_PROMPT=0 git clone --progress -- "$1" "$d" || exit 4
cd -- "$d" || exit 3
printf 'at %s\n' "$(pwd)"
echo git
"#
    )
}

/// Starts an empty git repository at `$1`. A folder already holding anything
/// is refused rather than turned into a repository around what is there.
pub fn init_script() -> String {
    format!(
        r#"{HOME_OF}d=$(home_of "$1")
if [ -e "$d" ] && [ -n "$(ls -A -- "$d" 2>/dev/null)" ]; then echo "not empty: $d" >&2; exit 5; fi
mkdir -p -- "$d" && cd -- "$d" || exit 3
git init -q || exit 4
printf 'at %s\n' "$(pwd)"
echo git
"#
    )
}

/// What ssh's failure means for the person, from what it wrote on stderr.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SshTrouble {
    /// The host's key is not in `known_hosts` yet; `BatchMode` will not ask.
    KeyUnknown,
    /// The key ssh has on file no longer matches: something to look into
    /// before going anywhere near the host.
    KeyChanged(String),
    /// Anything else, as ssh said it.
    Other(String),
}

/// Read ssh's stderr. The last meaningful line is the reason; banners and
/// warnings about adding a host to known hosts are not.
pub fn ssh_trouble(stderr: &str) -> SshTrouble {
    if stderr.contains("IDENTIFICATION HAS CHANGED") {
        return SshTrouble::KeyChanged(last_line(stderr));
    }
    if stderr.contains("Host key verification failed")
        || stderr.contains("No ED25519 host key is known")
        || stderr.contains("host key is known for")
    {
        return SshTrouble::KeyUnknown;
    }
    SshTrouble::Other(last_line(stderr))
}

/// The last line of a program's complaint worth showing, cut to fit a line.
pub fn last_line(text: &str) -> String {
    let line = text
        .split(['\n', '\r'])
        .map(str::trim)
        .rfind(|l| !l.is_empty() && !l.starts_with("Warning: Permanently added"))
        .unwrap_or("")
        .to_string();
    if line.chars().count() > 180 {
        let cut: String = line.chars().take(179).collect();
        format!("{cut}…")
    } else {
        line
    }
}

// ---------------------------------------------------------------------------
// Folders
// ---------------------------------------------------------------------------

/// A folder inside the one being browsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folder {
    pub name: String,
    /// It holds a `.git`: a checkout, which is what a project usually is.
    pub git: bool,
}

/// What a machine said about a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Listing {
    /// It is there: its absolute path as the machine spells it, and the
    /// folders inside, plain names first, then the ones starting with a dot.
    At { path: String, folders: Vec<Folder> },
    /// Nothing is there yet. Adding it will make it.
    Missing,
}

/// Plain names first, by name ignoring case, then dotted ones the same way:
/// `.git` and `.cache` are rarely the project being looked for.
fn sort_folders(folders: &mut [Folder]) {
    folders.sort_by_key(|f| (f.name.starts_with('.'), f.name.to_lowercase()));
}

/// Read what [`list_script`] printed.
pub fn parse_listing(out: &str) -> Result<Listing, String> {
    let mut lines = out.lines();
    let first = lines.next().unwrap_or("");
    if first == "missing" {
        return Ok(Listing::Missing);
    }
    let Some(path) = first.strip_prefix("at ") else {
        return Err(format!("unexpected answer: {}", last_line(out)));
    };
    let mut folders = Vec::new();
    for line in lines {
        let (git, name) = match (line.get(..2), line.get(2..)) {
            (Some("g "), Some(name)) => (true, name),
            (Some("d "), Some(name)) => (false, name),
            _ => continue,
        };
        if !name.is_empty() {
            folders.push(Folder {
                name: name.to_string(),
                git,
            });
        }
    }
    sort_folders(&mut folders);
    Ok(Listing::At {
        path: path.to_string(),
        folders,
    })
}

/// The same answer [`list_script`] gives, read from this machine's own disk.
pub fn list_local(dir: &Path) -> Result<Listing, String> {
    if !dir.exists() {
        return Ok(Listing::Missing);
    }
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    let mut folders = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        folders.push(Folder {
            name: entry.file_name().to_string_lossy().to_string(),
            git: path.join(".git").exists(),
        });
    }
    sort_folders(&mut folders);
    // Spelled the way `pwd` spells it on a far machine: no trailing slash,
    // except the root's own.
    let path = dir.to_string_lossy();
    let trimmed = path.trim_end_matches('/');
    Ok(Listing::At {
        path: if trimmed.is_empty() {
            "/".into()
        } else {
            trimmed.to_string()
        },
        folders,
    })
}

/// Read what [`make_script`], [`clone_script`] and [`init_script`] print: the
/// folder's absolute path, and whether it is a git checkout.
pub fn parse_made(out: &str) -> Result<(String, bool), String> {
    let mut lines = out.lines();
    let path = lines
        .next()
        .and_then(|l| l.strip_prefix("at "))
        .filter(|p| p.starts_with('/'))
        .ok_or_else(|| format!("unexpected answer: {}", last_line(out)))?;
    Ok((path.to_string(), lines.any(|l| l == "git")))
}

/// The path field, read: the folder whose contents are listed (everything up
/// to and including the last `/`) and the start of a name being typed after
/// it. A path that starts with neither `/` nor `~` is read from the home
/// folder, so `Work/td` means `~/Work/td`.
pub fn split_field(text: &str) -> (String, String) {
    let text = text.trim();
    let text = if text == "~" {
        "~/".to_string()
    } else if text.starts_with('/') || text.starts_with("~/") {
        text.to_string()
    } else {
        format!("~/{text}")
    };
    let cut = text.rfind('/').map_or(0, |i| i + 1);
    (text[..cut].to_string(), text[cut..].to_string())
}

/// The folder the field names, which is what Add adds: the listed folder when
/// nothing is typed after the last `/`, else that folder's child of that name.
pub fn field_target(text: &str) -> String {
    let (dir, partial) = split_field(text);
    let joined = format!("{dir}{partial}");
    let trimmed = joined.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".into()
    } else if trimmed == "~" {
        "~".into()
    } else {
        trimmed.to_string()
    }
}

/// `~` and `~/…` on this machine.
pub fn expand_home(path: &str, home: &Path) -> PathBuf {
    if path == "~" {
        home.to_path_buf()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(path)
    }
}

/// The folder above an absolute path, written as a field value ending in `/`.
/// None at the root.
pub fn parent_field(abs: &str) -> Option<String> {
    let trimmed = abs.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let cut = trimmed.rfind('/')?;
    Some(format!("{}/", &trimmed[..cut]))
}

/// The folders in a listing that start with what is typed, ignoring case.
pub fn matching<'a>(folders: &'a [Folder], partial: &str) -> Vec<&'a Folder> {
    let want = partial.to_lowercase();
    folders
        .iter()
        .filter(|f| f.name.to_lowercase().starts_with(&want))
        .collect()
}

// ---------------------------------------------------------------------------
// Clone or start new
// ---------------------------------------------------------------------------

/// What the clone-or-start field asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewProject {
    /// A repository to clone, and the folder name it lands in.
    Clone { url: String, name: String },
    /// An empty git repository by this name.
    Empty { name: String },
}

/// Why the field cannot be acted on yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewTrouble {
    Empty,
    /// A name, or the name a URL ends in, that cannot be a folder here.
    BadName,
}

/// A folder name this dialog will make: not empty, no `/`, no control
/// characters, not starting with a dot (hidden, and `..`) or a dash (read as
/// an option by half the tools that will touch it), and not padded with
/// spaces.
pub fn folder_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= 100
        && name.trim() == name
        && !name.starts_with(['.', '-'])
        && !name.contains('/')
        && !name.chars().any(char::is_control)
}

/// The scp-like form git takes: `host:path` or `user@host:path`, with no
/// scheme. The part before the colon has to look like an ssh destination, so
/// a Windows drive or a sentence with a colon in it is not one.
fn scp_like(text: &str) -> Option<&str> {
    if text.contains("://") || text.contains(char::is_whitespace) {
        return None;
    }
    let (host, path) = text.split_once(':')?;
    (entry::valid_destination(host) && !path.trim_matches('/').is_empty()).then_some(path)
}

/// The name a repository's folder takes: the last part of its path, with
/// `.git` dropped.
fn repo_name(path: &str) -> String {
    let last = path
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("");
    last.strip_suffix(".git").unwrap_or(last).to_string()
}

/// Read the clone-or-start field.
///
/// A URL with a scheme (`https://`, `ssh://`, `git://`, `file://`), git's
/// scp-like `host:owner/repo`, or `owner/repo`, which means GitHub, is
/// cloned. A page of a GitHub repository (`…/owner/repo/tree/main`) clones the
/// repository. Anything else is the name of an empty repository to start.
pub fn new_project(text: &str) -> Result<NewProject, NewTrouble> {
    let text = text.trim();
    if text.is_empty() {
        return Err(NewTrouble::Empty);
    }
    let clone = |url: String, path: &str| {
        let name = repo_name(path);
        if folder_name_ok(&name) {
            Ok(NewProject::Clone { url, name })
        } else {
            Err(NewTrouble::BadName)
        }
    };
    if let Some((scheme, rest)) = text.split_once("://") {
        if !["https", "http", "ssh", "git", "file"].contains(&scheme) || rest.is_empty() {
            return Err(NewTrouble::BadName);
        }
        let rest = rest.split(['?', '#']).next().unwrap_or(rest);
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        let host_name = host.rsplit('@').next().unwrap_or(host).to_ascii_lowercase();
        if (scheme == "https" || scheme == "http") && host_name == "github.com" {
            // A page inside a repository clones the repository.
            let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
            if parts.len() < 2 {
                return Err(NewTrouble::BadName);
            }
            let repo = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
            let url = format!("https://github.com/{}/{}", parts[0], repo);
            return clone(url, repo);
        }
        return clone(format!("{scheme}://{rest}"), path);
    }
    if let Some(path) = scp_like(text) {
        return clone(text.to_string(), path);
    }
    // owner/repo, GitHub's shorthand, the way `gh repo clone` reads it.
    if let Some((owner, repo)) = text.split_once('/') {
        let word = |s: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        };
        if word(owner) && word(repo) && !owner.starts_with(['.', '-']) {
            let repo = repo.strip_suffix(".git").unwrap_or(repo);
            return clone(format!("https://github.com/{owner}/{repo}"), repo);
        }
        return Err(NewTrouble::BadName);
    }
    if folder_name_ok(text) {
        Ok(NewProject::Empty {
            name: text.to_string(),
        })
    } else {
        Err(NewTrouble::BadName)
    }
}

/// The first repository a sentence names, for the clone field to start with:
/// a URL on a known forge or ending in `.git`, or an `ssh://`, `git@` or
/// scp-like one. A link to anything else (a docs page, an X post) is not a
/// repository and is not offered.
pub fn first_repository(sentence: &str) -> Option<String> {
    const FORGES: &[&str] = &[
        "github.com",
        "gitlab.com",
        "bitbucket.org",
        "codeberg.org",
        "git.sr.ht",
    ];
    for word in sentence.split_whitespace() {
        let word = word
            .trim_start_matches(['(', '<', '[', '"', '\'', '`'])
            .trim_end_matches([')', '>', ']', '"', '\'', '`', ',', '.', ';', ':', '!', '?']);
        let repository = if let Some(rest) = word
            .strip_prefix("https://")
            .or_else(|| word.strip_prefix("http://"))
        {
            let host = rest.split('/').next().unwrap_or("").to_ascii_lowercase();
            let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
            FORGES.contains(&host.as_str()) || word.ends_with(".git")
        } else {
            word.starts_with("ssh://") || (word.starts_with("git@") && scp_like(word).is_some())
        };
        if repository && matches!(new_project(word), Ok(NewProject::Clone { .. })) {
            return Some(word.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// What is remembered: projects.toml, and a machine added to machines.toml
// ---------------------------------------------------------------------------

/// A project somebody added that the scan cannot see.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Added {
    /// The folder, absolute, as its machine spells it.
    pub path: String,
    /// The `machines.toml` name of the machine it is on; absent is this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    /// Whether it was a git checkout when it was added, as seen then. Absent
    /// is not known, which is not the same as not a checkout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<bool>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ProjectsFile {
    #[serde(default)]
    project: Vec<Added>,
}

pub fn projects_path(config: &Path) -> PathBuf {
    config.join("projects.toml")
}

const PROJECTS_HEADER: &str =
    "# Projects added from the composer (Ctrl+O). The PROJECT chip lists\n\
# these beside the folders it finds one level inside ~/Work and its other\n\
# roots. `machine` names an entry in machines.toml; absent is this machine.\n";

const MACHINES_HEADER: &str = "# Machines the composer can start an agent on, over ssh.\n\
# The MACHINE chip appears when this lists at least one.\n";

pub fn parse_projects(text: &str) -> Result<Vec<Added>, String> {
    let file: ProjectsFile = toml::from_str(text).map_err(|e| e.to_string())?;
    for p in &file.project {
        if !(p.path.starts_with('/') || p.path.starts_with('~')) {
            return Err(format!("{:?} is not an absolute path", p.path));
        }
    }
    Ok(file.project)
}

/// The added projects; no file is none, which is not an error.
pub fn load_projects(config: &Path) -> Result<Vec<Added>, String> {
    match std::fs::read_to_string(projects_path(config)) {
        Ok(text) => parse_projects(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.to_string()),
    }
}

/// Append a block to a TOML file, made with `header` if it is not there yet,
/// through a temporary file and a rename so a reader never sees half of it.
/// What is already in the file, comments included, is kept as written.
fn append_block(path: &Path, header: &str, block: &str) -> Result<(), String> {
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => header.to_string(),
        Err(e) => return Err(e.to_string()),
    };
    let mut text = existing;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str(block);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, &text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Remember an added project, once: the same folder on the same machine is
/// not written twice. Returns whether anything was written.
pub fn remember_project(config: &Path, added: &Added) -> Result<bool, String> {
    let known = load_projects(config)?;
    if known
        .iter()
        .any(|p| p.path == added.path && p.machine == added.machine)
    {
        return Ok(false);
    }
    let block = toml::to_string(&ProjectsFile {
        project: vec![added.clone()],
    })
    .map_err(|e| e.to_string())?;
    append_block(&projects_path(config), PROJECTS_HEADER, &block)?;
    Ok(true)
}

#[derive(serde::Serialize)]
struct MachineBlock<'a> {
    machine: Vec<MachineEntry<'a>>,
}

#[derive(serde::Serialize)]
struct MachineEntry<'a> {
    name: &'a str,
    ssh: &'a str,
}

/// Add a machine to `machines.toml`, named after its ssh alias, and return it
/// as the composer will hold it. A machine already there under that
/// destination is returned as it is; a name already taken by another
/// destination is refused rather than doubled, since the chip shows names.
pub fn add_machine(config: &Path, alias: &str) -> Result<Machine, String> {
    if !entry::valid_destination(alias) {
        return Err(format!("{alias:?} is not an ssh destination"));
    }
    let known = entry::load_machines(config)?;
    if let Some(m) = known.iter().find(|m| m.ssh.as_deref() == Some(alias)) {
        return Ok(m.clone());
    }
    if known.iter().any(|m| m.name == alias) {
        return Err(format!(
            "machines.toml already has a machine named {alias} going somewhere else"
        ));
    }
    let block = toml::to_string(&MachineBlock {
        machine: vec![MachineEntry {
            name: alias,
            ssh: alias,
        }],
    })
    .map_err(|e| e.to_string())?;
    append_block(&entry::machines_path(config), MACHINES_HEADER, &block)?;
    Ok(Machine {
        name: alias.to_string(),
        ssh: Some(alias.to_string()),
    })
}

/// Whether the composer's scan already finds a folder on this machine: one
/// level inside one of its roots, and not hidden. Such a folder needs no line
/// in `projects.toml`.
pub fn scan_sees(path: &Path, roots: &[PathBuf]) -> bool {
    let hidden = path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with('.'));
    !hidden && path.parent().is_some_and(|p| roots.iter().any(|r| r == p))
}

/// The composer's projects with the added ones in: each added project this
/// machine still has, and each far one whose machine is still listed, ahead
/// of the scan, skipping any the scan already holds. A far project whose
/// machine is gone is named in the second value, not dropped in silence.
pub fn with_added(
    scanned: Vec<launcher::Project>,
    added: &[Added],
    machines: &[Machine],
    home: &Path,
) -> (Vec<launcher::Project>, Vec<String>) {
    let mut front: Vec<launcher::Project> = Vec::new();
    let mut lost: Vec<String> = Vec::new();
    // Newest added first: the file grows at the end.
    for a in added.iter().rev() {
        let path = expand_home(&a.path, home);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| a.path.clone());
        let project = match &a.machine {
            None => {
                if !path.is_dir()
                    || scanned
                        .iter()
                        .any(|p| p.path == path && p.machine.is_none())
                {
                    continue;
                }
                launcher::Project {
                    repo: path.join(".git").exists(),
                    name,
                    path,
                    machine: None,
                }
            }
            Some(m) => {
                if !machines.iter().any(|x| &x.name == m) {
                    if !lost.contains(m) {
                        lost.push(m.clone());
                    }
                    continue;
                }
                launcher::Project {
                    repo: a.git == Some(true),
                    name,
                    path: PathBuf::from(&a.path),
                    machine: Some(m.clone()),
                }
            }
        };
        if !front
            .iter()
            .any(|p| p.path == project.path && p.machine == project.machine)
        {
            front.push(project);
        }
    }
    front.extend(scanned);
    (front, lost)
}

#[cfg(test)]
mod tests;
