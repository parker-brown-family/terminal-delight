//! The protected-machine marker: a file in TD's config root naming this
//! machine, which the removal worker, the ADMIN panel and the agent handoff all
//! refuse on. Parker, 2026-10-30, about the Legion: "DO NOT KILL TD ON THIS BOX".
//! Prose in a plan cannot stop a click, so the refusal lives here, in code.
//!
//! The marker names `/etc/machine-id`, so a config directory copied to another
//! machine does not carry the protection with it. It fails closed: a marker that
//! cannot be read, or a machine whose id cannot be read, counts as protected.
//! Lifting it is deliberately manual: delete the file.
use super::roots::Roots;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub(crate) const MARKER: &str = "protected";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct Machine {
    /// `/etc/machine-id`, trimmed; absent when it cannot be read.
    pub(crate) id: Option<String>,
    pub(crate) hostname: Option<String>,
}

impl Machine {
    pub(crate) fn read() -> Machine {
        let id = std::fs::read_to_string("/etc/machine-id")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname")
            .or_else(|_| std::fs::read_to_string("/etc/hostname"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Machine { id, hostname }
    }

    /// The first eight characters of the id, which is all a person reads.
    pub(crate) fn short_id(&self) -> String {
        self.id
            .as_deref()
            .map(|i| i.chars().take(8).collect())
            .unwrap_or_else(|| "unknown".into())
    }
}

#[derive(Debug, Deserialize)]
struct MarkerFile {
    #[serde(rename = "machine-id")]
    machine_id: Option<String>,
    hostname: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum Protection {
    /// No marker in any place this run checks.
    None,
    /// A marker names this machine.
    ThisMachine {
        marker: PathBuf,
        hostname: Option<String>,
        reason: Option<String>,
    },
    /// A marker names a different machine: a copied config. Reported, not obeyed.
    OtherMachine { marker: PathBuf, names: String },
    /// A marker exists but cannot be read or does not name a machine, or this
    /// machine's own id cannot be read. Treated as protected.
    Unreadable { marker: PathBuf, why: String },
}

impl Protection {
    pub(crate) fn refuses(&self) -> bool {
        matches!(
            self,
            Protection::ThisMachine { .. } | Protection::Unreadable { .. }
        )
    }
}

/// The marker files this run must honour: the one in the resolved config root,
/// and, when HOME is the account's own home, the one in the account's default
/// config root too, so pointing `XDG_CONFIG_HOME` somewhere else cannot route
/// around the real install's protection.
pub(crate) fn marker_paths(roots: &Roots) -> Vec<PathBuf> {
    let mut v = vec![roots.config.join(MARKER)];
    if let Some(pw) = passwd_home() {
        if same_dir(&pw, &roots.home) {
            let real = pw.join(".config/terminal-delight").join(MARKER);
            if !v.contains(&real) {
                v.push(real);
            }
        }
    }
    v
}

pub(crate) fn protection(roots: &Roots, machine: &Machine) -> Protection {
    let mut other = None;
    for marker in marker_paths(roots) {
        match std::fs::symlink_metadata(&marker) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Protection::Unreadable {
                    marker,
                    why: e.to_string(),
                }
            }
            Ok(_) => {}
        }
        let text = match std::fs::read_to_string(&marker) {
            Ok(t) => t,
            Err(e) => {
                return Protection::Unreadable {
                    marker,
                    why: e.to_string(),
                }
            }
        };
        let parsed: MarkerFile = match toml::from_str(&text) {
            Ok(p) => p,
            Err(e) => {
                return Protection::Unreadable {
                    marker,
                    why: format!("not a marker: {e}"),
                }
            }
        };
        let Some(named) = parsed
            .machine_id
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        else {
            return Protection::Unreadable {
                marker,
                why: "names no machine-id".into(),
            };
        };
        let Some(ours) = machine.id.as_deref() else {
            return Protection::Unreadable {
                marker,
                why: "this machine's id cannot be read".into(),
            };
        };
        if named == ours {
            return Protection::ThisMachine {
                marker,
                hostname: parsed.hostname,
                reason: parsed.reason,
            };
        }
        other.get_or_insert(Protection::OtherMachine {
            marker,
            names: parsed
                .hostname
                .unwrap_or_else(|| named.chars().take(8).collect()),
        });
    }
    other.unwrap_or(Protection::None)
}

/// The text of a marker for this machine.
pub(crate) fn marker_text(machine: &Machine, reason: &str, since: &str) -> Result<String, String> {
    let id = machine
        .id
        .as_deref()
        .ok_or("this machine's id cannot be read")?;
    let host = machine.hostname.as_deref().unwrap_or("unknown");
    let q = |s: &str| toml::Value::String(s.to_string()).to_string();
    Ok(format!(
        "# Terminal Delight will not uninstall itself on this machine.\n\
         # The marker names /etc/machine-id, so a copied config does not carry it.\n\
         # To lift the protection, delete this file by hand.\n\
         machine-id = {}\nhostname = {}\nsince = {}\nreason = {}\n",
        q(id),
        q(host),
        q(since),
        q(reason)
    ))
}

/// Write the marker for this machine into `roots.config`, refusing to replace
/// an existing one. Used by `terminal-delight admin protect`.
pub(crate) fn protect(
    roots: &Roots,
    machine: &Machine,
    reason: &str,
    since: &str,
) -> Result<PathBuf, String> {
    let text = marker_text(machine, reason, since)?;
    std::fs::create_dir_all(&roots.config).map_err(|e| e.to_string())?;
    let path = roots.config.join(MARKER);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    use std::io::Write;
    f.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    Ok(path)
}

fn passwd_home() -> Option<PathBuf> {
    // SAFETY: getpwuid_r writes into the buffers we own and returns a pointer
    // into them; nothing is retained past this call.
    unsafe {
        let mut pwd: libc::passwd = std::mem::zeroed();
        let mut buf = vec![0 as libc::c_char; 4096];
        let mut out: *mut libc::passwd = std::ptr::null_mut();
        let rc = libc::getpwuid_r(
            libc::getuid(),
            &mut pwd,
            buf.as_mut_ptr(),
            buf.len(),
            &mut out,
        );
        if rc != 0 || out.is_null() || pwd.pw_dir.is_null() {
            return None;
        }
        let dir = std::ffi::CStr::from_ptr(pwd.pw_dir)
            .to_string_lossy()
            .into_owned();
        (!dir.is_empty()).then(|| PathBuf::from(dir))
    }
}

/// Whether HOME is the account's real home, the only case in which global
/// effects (the systemd user bus, other programs' CLIs) may be touched.
pub(crate) fn is_account_home(home: &Path) -> bool {
    passwd_home().is_some_and(|pw| same_dir(&pw, home))
}

fn same_dir(a: &Path, b: &Path) -> bool {
    a == b
        || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn fixture(tag: &str) -> (PathBuf, Roots) {
        let root = std::env::temp_dir().join(format!("td-guard-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let home = root.clone();
        let roots =
            Roots::from_env(&move |k| (k == "HOME").then(|| OsString::from(&home))).unwrap();
        (root, roots)
    }

    fn me() -> Machine {
        Machine {
            id: Some("0123456789abcdef".into()),
            hostname: Some("box".into()),
        }
    }

    #[test]
    fn no_marker_is_unprotected() {
        let (root, r) = fixture("none");
        assert_eq!(protection(&r, &me()), Protection::None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn this_machine_refuses_and_other_machine_does_not() {
        let (root, r) = fixture("this");
        let p = protect(&r, &me(), "test", "2026-10-30").unwrap();
        assert!(protection(&r, &me()).refuses());
        let other = Machine {
            id: Some("ffff".into()),
            hostname: None,
        };
        assert!(matches!(
            protection(&r, &other),
            Protection::OtherMachine { .. }
        ));
        assert!(!protection(&r, &other).refuses());
        // A second protect never replaces the first.
        assert!(protect(&r, &me(), "again", "x").is_err());
        assert!(std::fs::read_to_string(p)
            .unwrap()
            .contains("0123456789abcdef"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unreadable_or_unnamed_marker_fails_closed() {
        let (root, r) = fixture("bad");
        std::fs::create_dir_all(&r.config).unwrap();
        for text in ["not = [toml", "hostname = \"x\"\n", "machine-id = \"\"\n"] {
            std::fs::write(r.config.join(MARKER), text).unwrap();
            assert!(protection(&r, &me()).refuses(), "{text}");
        }
        std::fs::write(r.config.join(MARKER), "machine-id = \"0123\"\n").unwrap();
        let blind = Machine {
            id: None,
            hostname: None,
        };
        assert!(protection(&r, &blind).refuses());
        std::fs::remove_dir_all(root).unwrap();
    }
}
