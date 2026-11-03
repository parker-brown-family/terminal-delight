//! Where Terminal Delight's files live, resolved once from an environment the
//! caller hands in, so the preview, the export, the build list and the removal
//! worker all agree on the same directories and their tests can point every one
//! of them at a fixture without touching the process environment.
//!
//! A relative XDG value is not used: TD wrote under whatever working directory
//! a window happened to have, and no later process can know which. The default
//! is used instead, and the relative value is reported in `warnings`.
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Roots {
    pub(crate) home: PathBuf,
    /// `<XDG_CONFIG_HOME or ~/.config>/terminal-delight`
    pub(crate) config: PathBuf,
    /// `<XDG_STATE_HOME or ~/.local/state>/terminal-delight`
    pub(crate) state: PathBuf,
    /// `<XDG_DATA_HOME or ~/.local/share>/terminal-delight`
    pub(crate) data: PathBuf,
    /// `<XDG_CACHE_HOME or ~/.cache>/terminal-delight`
    pub(crate) cache: PathBuf,
    /// The XDG config base itself, for shared files such as `xdg-terminals.list`.
    pub(crate) config_base: PathBuf,
    /// The XDG data base, for desktop entries and icons.
    pub(crate) data_base: PathBuf,
    /// The XDG cache base, for shared caches such as `omarchy/agent-usage`.
    pub(crate) cache_base: PathBuf,
    /// `$XDG_RUNTIME_DIR` itself (not its `terminal-delight` child); absent when unset.
    pub(crate) runtime_base: Option<PathBuf>,
    /// `$TMPDIR`, else `/tmp`: where TD falls back when there is no runtime dir.
    pub(crate) temp: PathBuf,
    /// `~/.local/bin`, where the launcher and the helpers are installed.
    pub(crate) bin: PathBuf,
    /// `~/.local/lib/terminal-delight`, where `land.sh` keeps versioned builds.
    pub(crate) builds: PathBuf,
    pub(crate) warnings: Vec<String>,
}

impl Roots {
    /// Resolve from an environment lookup. `HOME` is required and must be an
    /// absolute directory other than `/`; nothing is guessed without it.
    pub(crate) fn from_env(env: &dyn Fn(&str) -> Option<OsString>) -> Result<Roots, String> {
        let get = |k: &str| env(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        let home = get("HOME").ok_or("HOME is unset; no paths guessed")?;
        if !home.is_absolute() || home.parent().is_none() || has_dot_dot(&home) {
            return Err(format!(
                "HOME must be an absolute directory other than /: {}",
                home.display()
            ));
        }
        let mut warnings = Vec::new();
        let mut base = |key: &str, default: PathBuf| match get(key) {
            Some(p) if p.is_absolute() && !has_dot_dot(&p) => p,
            Some(p) => {
                warnings.push(format!(
                    "{key} is {}, which is not an absolute path; using {} instead",
                    p.display(),
                    default.display()
                ));
                default
            }
            None => default,
        };
        let config_base = base("XDG_CONFIG_HOME", home.join(".config"));
        let state_base = base("XDG_STATE_HOME", home.join(".local/state"));
        let data_base = base("XDG_DATA_HOME", home.join(".local/share"));
        let cache_base = base("XDG_CACHE_HOME", home.join(".cache"));
        let runtime_base = get("XDG_RUNTIME_DIR").filter(|p| p.is_absolute() && !has_dot_dot(p));
        let temp = get("TMPDIR")
            .filter(|p| p.is_absolute() && !has_dot_dot(p))
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        Ok(Roots {
            config: config_base.join("terminal-delight"),
            state: state_base.join("terminal-delight"),
            data: data_base.join("terminal-delight"),
            cache: cache_base.join("terminal-delight"),
            bin: home.join(".local/bin"),
            builds: home.join(".local/lib/terminal-delight"),
            config_base,
            data_base,
            cache_base,
            runtime_base,
            temp,
            home,
            warnings,
        })
    }

    /// The roots of the running process.
    pub(crate) fn current() -> Result<Roots, String> {
        Roots::from_env(&|k| std::env::var_os(k))
    }

    /// `<runtime>/terminal-delight`, when there is a runtime dir.
    pub(crate) fn runtime(&self) -> Option<PathBuf> {
        self.runtime_base
            .as_ref()
            .map(|r| r.join("terminal-delight"))
    }

    /// TD's own directories, the ones whose whole contents are TD's to judge.
    pub(crate) fn td_dirs(&self) -> Vec<PathBuf> {
        let mut v = vec![
            self.config.clone(),
            self.state.clone(),
            self.data.clone(),
            self.cache.clone(),
            self.builds.clone(),
        ];
        v.extend(self.runtime());
        // Also what any removal mode may delete beyond the current roots: the
        // runtime fallback in the temp directory, and older copies under HOME.
        let uid = unsafe { libc::getuid() };
        v.push(self.temp.join(format!("terminal-delight-{uid}")));
        v.push(self.home.join(".config/terminal-delight"));
        v.push(self.home.join(".local/state/terminal-delight"));
        v.sort();
        v.dedup();
        v
    }

    /// Whether `p` is one of TD's directories or lies inside one. Compares
    /// lexically and, where both exist, after resolving symlinks, so a path
    /// reached through a link into a TD root still counts as inside it.
    pub(crate) fn is_inside_td(&self, p: &Path) -> bool {
        let resolved = resolve_existing_prefix(p);
        self.td_dirs().iter().any(|root| {
            p.starts_with(root)
                || resolved.starts_with(root)
                || resolved.starts_with(resolve_existing_prefix(root))
        })
    }
}

fn has_dot_dot(p: &Path) -> bool {
    p.components().any(|c| matches!(c, Component::ParentDir))
}

/// Canonicalise the longest prefix of `p` that exists and re-attach the rest,
/// so a destination that does not exist yet is still judged by where its
/// parent really is.
pub(crate) fn resolve_existing_prefix(p: &Path) -> PathBuf {
    let mut tail = Vec::new();
    let mut cur = p.to_path_buf();
    loop {
        if let Ok(c) = std::fs::canonicalize(&cur) {
            let mut out = c;
            for t in tail.iter().rev() {
                out.push(t);
            }
            return out;
        }
        match (cur.file_name().map(|f| f.to_owned()), cur.parent()) {
            (Some(name), Some(parent)) => {
                tail.push(name);
                cur = parent.to_path_buf();
            }
            _ => return p.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k: &str| m.get(k).map(OsString::from)
    }

    #[test]
    fn defaults_follow_home() {
        let r = Roots::from_env(&env(&[("HOME", "/home/u")])).unwrap();
        assert_eq!(r.config, PathBuf::from("/home/u/.config/terminal-delight"));
        assert_eq!(
            r.state,
            PathBuf::from("/home/u/.local/state/terminal-delight")
        );
        assert_eq!(
            r.builds,
            PathBuf::from("/home/u/.local/lib/terminal-delight")
        );
        assert_eq!(r.runtime(), None);
        assert_eq!(r.temp, PathBuf::from("/tmp"));
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn relative_xdg_is_reported_and_not_used() {
        let r = Roots::from_env(&env(&[("HOME", "/home/u"), ("XDG_STATE_HOME", "rel")])).unwrap();
        assert_eq!(
            r.state,
            PathBuf::from("/home/u/.local/state/terminal-delight")
        );
        assert_eq!(r.warnings.len(), 1);
    }

    #[test]
    fn home_must_be_absolute_and_not_root() {
        for h in ["", "rel", "/", "/home/../etc"] {
            assert!(Roots::from_env(&env(&[("HOME", h)])).is_err(), "{h}");
        }
    }

    #[test]
    fn inside_td_is_lexical_and_resolved() {
        let r = Roots::from_env(&env(&[
            ("HOME", "/home/u"),
            ("XDG_RUNTIME_DIR", "/run/user/7"),
        ]))
        .unwrap();
        assert!(r.is_inside_td(Path::new("/home/u/.config/terminal-delight/x")));
        assert!(r.is_inside_td(Path::new("/run/user/7/terminal-delight/sock")));
        assert!(!r.is_inside_td(Path::new("/home/u/terminal-delight-export")));
        assert!(!r.is_inside_td(Path::new("/home/u/.config")));
    }
}
