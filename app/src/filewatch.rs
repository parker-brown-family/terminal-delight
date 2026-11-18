//! File reloads on Linux. Watch the path components so a staged directory
//! rename, a newly created parent, or a symlink swap rebinds the leaf.
use async_io::Async;
use gpui::BackgroundExecutor;
use rustix::fs::inotify::{self, CreateFlags, Reader, WatchFlags};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

struct Watch {
    fd: Async<OwnedFd>,
    // None accepts every immediate child (a directory being inventoried).
    names: HashMap<i32, Option<HashSet<OsString>>>,
}

impl Watch {
    fn new(paths: &[PathBuf]) -> io::Result<Self> {
        let fd = inotify::init(CreateFlags::CLOEXEC | CreateFlags::NONBLOCK)?;
        let mut names: HashMap<i32, Option<HashSet<OsString>>> = HashMap::new();
        let flags = WatchFlags::CLOSE_WRITE
            | WatchFlags::CREATE
            | WatchFlags::DELETE
            | WatchFlags::MOVE
            | WatchFlags::ATTRIB
            | WatchFlags::MOVE_SELF
            | WatchFlags::DELETE_SELF;
        let mut add = |dir: &Path, name: Option<&std::ffi::OsStr>| -> io::Result<()> {
            match inotify::add_watch(&fd, dir, flags) {
                Ok(wd) => {
                    let watched = names.entry(wd).or_insert_with(|| Some(HashSet::new()));
                    match name {
                        Some(name) => {
                            if let Some(set) = watched {
                                set.insert(name.to_owned());
                            }
                        }
                        None => *watched = None,
                    }
                }
                Err(rustix::io::Errno::NOENT | rustix::io::Errno::NOTDIR) => {}
                Err(err) => return Err(err.into()),
            }
            Ok(())
        };
        for path in paths {
            let path = std::path::absolute(path)?;
            for part in path.ancestors() {
                if let (Some(dir), Some(name)) = (part.parent(), part.file_name()) {
                    add(dir, Some(name))?;
                }
            }
            if path.is_dir() {
                add(&path, None)?;
            }
            // A symlink to a file can be edited through the target's name.
            if path.is_file() {
                add(&path, None)?;
            }
        }
        Ok(Self {
            fd: Async::new(fd)?,
            names,
        })
    }

    fn drain(&self) -> io::Result<()> {
        let mut buf = [MaybeUninit::uninit(); 8192];
        let mut reader = Reader::new(self.fd.get_ref(), &mut buf);
        loop {
            let event = reader.next()?;
            // Events without names include overflow and inode removal.
            if event.file_name().is_none_or(|name| {
                self.names.get(&event.wd()).is_none_or(|names| {
                    names.as_ref().is_none_or(|set| {
                        set.contains(std::ffi::OsStr::from_bytes(name.to_bytes()))
                    })
                })
            }) {
                return Ok(());
            }
        }
    }

    async fn changed(&self) -> io::Result<()> {
        self.fd.read_with(|_| self.drain()).await
    }
}

pub(crate) struct Changes(Option<Watch>);
impl Changes {
    pub(crate) fn new(paths: &[PathBuf]) -> Self {
        Self(Watch::new(paths).ok())
    }

    pub(crate) async fn wait(&self, executor: &BackgroundExecutor) {
        if let Some(watch) = &self.0 {
            // gpui's deterministic dispatcher refuses a wake from an OS thread.
            // Drive the same kernel queue with its virtual clock in view tests.
            #[cfg(test)]
            loop {
                match watch.drain() {
                    Ok(()) => return,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        executor.timer(Duration::from_millis(50)).await;
                    }
                    Err(_) => break,
                }
            }
            #[cfg(not(test))]
            if watch.changed().await.is_ok() {
                return;
            }
        }
        // Preserve live reload when the process exhausts its inotify quota.
        executor.timer(Duration::from_secs(2)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;

    #[test]
    fn idle_file_watch_ignores_unrelated_writes_and_observes_atomic_replacement() {
        let root = std::env::temp_dir().join(format!("td-filewatch-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("skin.toml");
        let watch = Watch::new(std::slice::from_ref(&file)).unwrap();
        let temp = root.join("editor.tmp");
        std::fs::write(&temp, "one").unwrap();
        assert!(watch.changed().now_or_never().is_none());
        std::fs::rename(&temp, &file).unwrap();
        assert!(watch.changed().now_or_never().unwrap().is_ok());
        let watch = Watch::new(std::slice::from_ref(&file)).unwrap();
        std::fs::write(&file, "two").unwrap();
        assert!(watch.changed().now_or_never().unwrap().is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }
}
