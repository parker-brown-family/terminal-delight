//! Validate exactly the tree handed to bubblewrap, without reopening children
//! through names outside that pinned tree. Concurrent host mutation is outside
//! the sandbox's direction of protection; any observed replacement fails closed.
use std::ffi::CStr;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
    mount: u64,
}

fn identity(file: &File) -> io::Result<Identity> {
    let metadata = file.metadata()?;
    let mut stat: libc::statx = unsafe { std::mem::zeroed() };
    // SAFETY: the fd stays live, the empty C string and output are valid, and
    // AT_EMPTY_PATH inspects this fd itself, including an O_PATH symlink fd.
    let code = unsafe {
        libc::statx(
            file.as_raw_fd(),
            c"".as_ptr(),
            libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
            libc::STATX_MNT_ID,
            &mut stat,
        )
    };
    if code != 0 {
        return Err(io::Error::last_os_error());
    }
    if stat.stx_mask & libc::STATX_MNT_ID == 0 {
        return Err(io::Error::other(
            "workdir mount identity unavailable; launch refused",
        ));
    }
    Ok(Identity {
        device: metadata.dev(),
        inode: metadata.ino(),
        mount: stat.stx_mnt_id,
    })
}

fn open_dir(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

fn open_at(parent: &File, name: &CStr, flags: i32) -> io::Result<File> {
    // SAFETY: the parent and C string stay alive for openat. Ownership of a
    // successful descriptor passes exactly once to File.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

struct Directory(*mut libc::DIR);
impl Drop for Directory {
    fn drop(&mut self) {
        unsafe {
            libc::closedir(self.0);
        }
    }
}

fn scan(dir: &File, root: &Identity, count: &mut usize, depth: usize) -> io::Result<()> {
    if depth > 128 {
        return Err(io::Error::other(
            "workdir exceeds 128 directory levels; choose a smaller tree",
        ));
    }
    // Open '.' relative to the pinned fd: an independent directory offset,
    // never a /proc/self/fd pathname and never the selected path again.
    let listing = open_at(dir, c".", libc::O_RDONLY | libc::O_DIRECTORY)?;
    use std::os::fd::IntoRawFd;
    let fd = listing.into_raw_fd();
    let raw = unsafe { libc::fdopendir(fd) };
    if raw.is_null() {
        unsafe {
            libc::close(fd);
        }
        return Err(io::Error::last_os_error());
    }
    let entries = Directory(raw);
    loop {
        // readdir uses errno to distinguish end-of-directory from failure.
        unsafe {
            *libc::__errno_location() = 0;
        }
        let entry = unsafe { libc::readdir(entries.0) };
        if entry.is_null() {
            let error = io::Error::last_os_error();
            return if error.raw_os_error() == Some(0) {
                Ok(())
            } else {
                Err(error)
            };
        }
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        if name == c"." || name == c".." {
            continue;
        }
        *count += 1;
        if *count > 200_000 {
            return Err(io::Error::other(
                "workdir validation exceeds 200000 entries; choose a smaller tree",
            ));
        }
        // O_PATH cannot activate a FIFO or device. O_NOFOLLOW pins symlinks
        // themselves, so none can redirect the recursive walk outside the grant.
        let child = open_at(dir, name, libc::O_PATH)?;
        let id = identity(&child)?;
        if id.mount != root.mount || id.device != root.device {
            return Err(io::Error::other("workdir contains a nested host mount"));
        }
        let metadata = child.metadata()?;
        if metadata.is_dir() {
            scan(&child, root, count, depth + 1)?;
        } else if metadata.is_file() && metadata.nlink() != 1 {
            return Err(io::Error::other(
                "workdir contains a hard-linked file; use an independent copy",
            ));
        } else if !(metadata.is_file() || metadata.file_type().is_symlink()) {
            return Err(io::Error::other(
                "workdir contains a socket, pipe or device; remove host channels first",
            ));
        }
    }
}

fn policy(canonical: &Path, runtime: &Path, home: Option<&Path>) -> io::Result<()> {
    let home = fs::canonicalize(
        home.ok_or_else(|| io::Error::other("account home unavailable; launch refused"))?,
    )?;
    let relative = canonical.strip_prefix(&home).map_err(|_| io::Error::other("workdir must be below your account home; copy external work into a dedicated project directory"))?;
    if relative.as_os_str().is_empty()
        || relative.components().any(
            |part| !matches!(part, Component::Normal(name) if !name.as_bytes().starts_with(b".")),
        )
    {
        return Err(io::Error::other(
            "choose a directory below your account home with no dot-directory in its path",
        ));
    }
    if canonical.starts_with(runtime) || runtime.starts_with(canonical) {
        return Err(io::Error::other("workdir overlaps protected runtime state"));
    }
    Ok(())
}

pub(super) fn validate(
    path: &Path,
    runtime: &Path,
    home: Option<&Path>,
) -> io::Result<(PathBuf, File)> {
    validate_after_pin(path, runtime, home, || {})
}

fn validate_after_pin(
    path: &Path,
    runtime: &Path,
    home: Option<&Path>,
    after_pin: impl FnOnce(),
) -> io::Result<(PathBuf, File)> {
    let canonical = fs::canonicalize(path)?;
    policy(&canonical, runtime, home)?;
    let pinned = open_dir(&canonical)?;
    let root = identity(&pinned)?;
    after_pin();
    // Both the positive/negative entry checks and mount checks use this fd.
    scan(&pinned, &root, &mut 0, 0)?;
    if identity(&open_dir(&canonical)?)? != root {
        return Err(io::Error::other(
            "workdir changed after it was pinned; launch refused",
        ));
    }
    Ok((canonical, pinned))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::os::unix::net::UnixListener;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let mut random = [0; 8];
            use std::io::Read;
            File::open("/dev/urandom")
                .unwrap()
                .read_exact(&mut random)
                .unwrap();
            let root =
                std::env::temp_dir().join(format!("td-work-{:x}", u64::from_ne_bytes(random)));
            fs::create_dir_all(root.join("home/project/nested")).unwrap();
            Self(root)
        }
        fn home(&self) -> PathBuf {
            self.0.join("home")
        }
        fn work(&self) -> PathBuf {
            self.home().join("project")
        }
        fn check(&self, path: &Path) -> io::Result<(PathBuf, File)> {
            validate(path, &self.0.join("runtime"), Some(&self.home()))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_swapped_path_cannot_hide_the_pinned_trees_host_socket() {
        let f = Fixture::new();
        let _socket = UnixListener::bind(f.work().join("host.sock")).unwrap();
        let error = validate_after_pin(&f.work(), &f.0.join("runtime"), Some(&f.home()), || {
            fs::rename(f.work(), f.home().join("moved")).unwrap();
            fs::create_dir(f.work()).unwrap();
        })
        .unwrap_err();
        assert!(error.to_string().contains("socket"), "{error}");
    }

    #[test]
    fn even_a_clean_replacement_is_not_the_selected_pinned_directory() {
        let f = Fixture::new();
        let error = validate_after_pin(&f.work(), &f.0.join("runtime"), Some(&f.home()), || {
            fs::rename(f.work(), f.home().join("moved")).unwrap();
            fs::create_dir(f.work()).unwrap();
        })
        .unwrap_err();
        assert!(error.to_string().contains("changed"), "{error}");
    }

    #[test]
    fn home_descendants_are_explicit_grants_but_hidden_and_external_roots_are_refused() {
        let f = Fixture::new();
        fs::write(f.work().join("allowed"), "granted").unwrap();
        fs::create_dir(f.work().join(".git")).unwrap();
        symlink("/etc/shadow", f.work().join("unmounted-target")).unwrap();
        assert!(f.check(&f.work()).is_ok());
        for path in [
            f.home(),
            f.0.clone(),
            f.home().join(".aws"),
            f.home().join(".kube/project"),
            f.home().join("project/.secrets"),
            f.0.join("external"),
        ] {
            fs::create_dir_all(&path).unwrap();
            assert!(f.check(&path).is_err(), "accepted {}", path.display());
        }
        for path in [
            "/tmp", "/var", "/root", "/boot", "/opt", "/srv", "/mnt", "/media",
        ] {
            assert!(f.check(Path::new(path)).is_err(), "accepted {path}");
        }
        symlink(f.home().join(".aws"), f.home().join("alias")).unwrap();
        assert!(f.check(&f.home().join("alias")).is_err());
        assert!(validate(&f.work(), &f.0.join("runtime"), None).is_err());
    }

    #[test]
    fn nested_entries_are_checked_by_fd_and_symlinks_are_not_followed() {
        let f = Fixture::new();
        let nested = f.work().join("nested/file");
        fs::write(&nested, "granted").unwrap();
        fs::hard_link(&nested, f.0.join("outside-alias")).unwrap();
        assert!(f
            .check(&f.work())
            .unwrap_err()
            .to_string()
            .contains("hard-linked"));
        fs::remove_file(f.0.join("outside-alias")).unwrap();
        let _socket = UnixListener::bind(f.work().join("nested/socket")).unwrap();
        assert!(f
            .check(&f.work())
            .unwrap_err()
            .to_string()
            .contains("socket"));
    }

    #[test]
    fn mount_identity_compares_mount_id_even_for_the_same_device() {
        let f = Fixture::new();
        let pinned = open_dir(&f.work()).unwrap();
        let mut root = identity(&pinned).unwrap();
        root.mount = root.mount.checked_add(1).unwrap();
        assert!(scan(&pinned, &root, &mut 0, 0)
            .unwrap_err()
            .to_string()
            .contains("nested host mount"));
    }
}
