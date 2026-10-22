pub mod claude;
pub mod codex;

use crate::state::*;
use serde_json::Value;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

#[derive(Debug)]
pub struct Outcome {
    pub kind: &'static str,
    pub evidence: Value,
}

impl Outcome {
    pub fn unconfirmed(reason: impl Into<String>) -> Self {
        Self {
            kind: "delivery_unconfirmed",
            evidence: serde_json::json!({"reason":reason.into()}),
        }
    }
    pub fn rejected(reason: impl Into<String>) -> Self {
        Self {
            kind: "definite_nonacceptance",
            evidence: serde_json::json!({"reason":reason.into()}),
        }
    }
}

pub fn connect(path: &Path) -> Result<UnixStream> {
    use std::os::unix::ffi::OsStrExt;
    let name = path.as_os_str().as_bytes();
    if name.contains(&0) || name.len() >= 108 {
        return Err("Invalid Unix socket path".into());
    }
    let fd = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
            0,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let stream = unsafe { UnixStream::from_raw_fd(fd) };
    let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    addr.sun_family = libc::AF_UNIX as _;
    for (to, from) in addr.sun_path.iter_mut().zip(name) {
        *to = *from as _;
    }
    let rc = unsafe {
        libc::connect(
            fd,
            &addr as *const _ as *const libc::sockaddr,
            std::mem::size_of_val(&addr) as _,
        )
    };
    if rc < 0 {
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() != Some(libc::EINPROGRESS) {
            return Err(e.to_string());
        }
        let mut poll = libc::pollfd {
            fd,
            events: libc::POLLOUT,
            revents: 0,
        };
        if unsafe { libc::poll(&mut poll, 1, 2000) } <= 0 {
            return Err("Native connect timed out".into());
        }
        let mut error = 0i32;
        let mut len = std::mem::size_of_val(&error) as libc::socklen_t;
        if unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                &mut error as *mut _ as *mut _,
                &mut len,
            )
        } < 0
            || error != 0
        {
            return Err("Native connect failed".into());
        }
    }
    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    Ok(stream)
}

pub fn peer(stream: &UnixStream) -> Result<ProcessKey> {
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of_val(&cred) as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut _ as *mut _,
            &mut len,
        )
    } < 0
        || cred.uid != unsafe { libc::geteuid() }
        || cred.pid <= 0
    {
        return Err("Native peer is not a same-user live process".into());
    }
    crate::identity::process(cred.pid as u32)
}
