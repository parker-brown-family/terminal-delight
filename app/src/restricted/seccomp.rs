//! Namespaces hide ambient services; this filter also blocks a Unix socket
//! appearing later inside the shared work tree. Only preconnected channels can
//! send. bubblewrap installs the filter after setup, before workload code.
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

#[derive(Clone, Copy)]
struct Insn {
    code: u16,
    jt: u8,
    jf: u8,
    k: u32,
}
fn insn(code: u16, jt: u8, jf: u8, k: u32) -> Insn {
    Insn { code, jt, jf, k }
}
const ALLOW: u32 = 0x7fff0000;
const DENY: u32 = 0x00050000 | libc::EPERM as u32;
const KILL: u32 = 0x80000000;

fn arch() -> io::Result<u32> {
    #[cfg(target_arch = "x86_64")]
    {
        Ok(0xc000003e)
    }
    #[cfg(target_arch = "aarch64")]
    {
        Ok(0xc00000b7)
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        Err(io::Error::other(
            "Restricted syscall profile unavailable on this architecture",
        ))
    }
}

fn program() -> io::Result<Vec<Insn>> {
    // seccomp_data: nr at 0, arch at 4, args[4] at 48 (64-bit address).
    let mut p = vec![
        insn(0x20, 0, 0, 4),
        insn(0x15, 1, 0, arch()?),
        insn(0x06, 0, 0, KILL),
        insn(0x20, 0, 0, 0),
    ];
    #[cfg(target_arch = "x86_64")]
    p.extend([insn(0x35, 0, 1, 0x40000000), insn(0x06, 0, 0, KILL)]); // no x32 alternate syscall numbers
    for nr in [
        libc::SYS_socket,
        libc::SYS_bind,
        libc::SYS_listen,
        libc::SYS_accept,
        libc::SYS_accept4,
        libc::SYS_connect,
        libc::SYS_sendmsg,
        libc::SYS_sendmmsg,
        libc::SYS_recvmsg,
        libc::SYS_recvmmsg,
        libc::SYS_io_uring_setup,
        libc::SYS_io_uring_enter,
        libc::SYS_io_uring_register,
    ] {
        p.extend([insn(0x15, 0, 1, nr as u32), insn(0x06, 0, 0, DENY)]);
    }
    // Local unnamed Unix pairs remain available for internal IPC. No other
    // address family may create a pair; new unconnected sockets are forbidden,
    // including families that do not obey network namespaces (e.g. vsock).
    p.extend([
        insn(0x15, 0, 3, libc::SYS_socketpair as u32),
        insn(0x20, 0, 0, 16),
        insn(0x15, 1, 0, libc::AF_UNIX as u32),
        insn(0x06, 0, 0, DENY),
        insn(0x20, 0, 0, 0),
    ]);
    // sendto(NULL) sends on a preconnected socket, including the TD gateway.
    // A non-null destination can reach a Unix pathname without connect().
    p.extend([
        insn(0x15, 0, 5, libc::SYS_sendto as u32),
        insn(0x20, 0, 0, 52),
        insn(0x15, 0, 2, 0),
        insn(0x20, 0, 0, 48),
        insn(0x15, 1, 0, 0),
        insn(0x06, 0, 0, DENY),
        insn(0x06, 0, 0, ALLOW),
    ]);
    Ok(p)
}

pub(super) fn write(path: &Path) -> io::Result<File> {
    let mut file = File::create(path)?;
    for i in program()? {
        file.write_all(&i.code.to_ne_bytes())?;
        file.write_all(&[i.jt, i.jf])?;
        file.write_all(&i.k.to_ne_bytes())?;
    }
    drop(file);
    File::open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn evaluate_with_domain(architecture: u32, nr: u32, destination: u64, domain: u32) -> u32 {
        let p = program().unwrap();
        let (mut pc, mut a) = (0, 0);
        loop {
            let i = p[pc];
            match i.code {
                0x20 => {
                    a = match i.k {
                        0 => nr,
                        4 => architecture,
                        16 => domain,
                        48 => destination as u32,
                        52 => (destination >> 32) as u32,
                        _ => panic!("offset"),
                    }
                }
                0x15 => {
                    pc += if a == i.k {
                        i.jt as usize
                    } else {
                        i.jf as usize
                    }
                }
                0x35 => {
                    pc += if a >= i.k {
                        i.jt as usize
                    } else {
                        i.jf as usize
                    }
                }
                0x06 => return i.k,
                _ => panic!("instruction"),
            }
            pc += 1;
        }
    }
    fn evaluate(architecture: u32, nr: u32, destination: u64) -> u32 {
        evaluate_with_domain(architecture, nr, destination, libc::AF_UNIX as u32)
    }
    #[test]
    fn only_unnamed_unix_pairs_are_available_for_internal_ipc() {
        let a = arch().unwrap();
        for domain in [
            libc::AF_UNIX,
            libc::AF_INET,
            libc::AF_INET6,
            libc::AF_VSOCK,
            libc::AF_NETLINK,
        ] {
            assert_eq!(
                evaluate_with_domain(a, libc::SYS_socket as u32, 0, domain as u32),
                DENY
            );
            assert_eq!(
                evaluate_with_domain(a, libc::SYS_socketpair as u32, 0, domain as u32),
                if domain == libc::AF_UNIX { ALLOW } else { DENY }
            );
        }
        for nr in [
            libc::SYS_bind,
            libc::SYS_listen,
            libc::SYS_accept,
            libc::SYS_accept4,
            libc::SYS_recvmsg,
            libc::SYS_recvmmsg,
        ] {
            assert_eq!(evaluate(a, nr as u32, 0), DENY);
        }
    }
    #[test]
    fn no_new_connection_or_addressed_send_can_bypass_the_preconnected_channel() {
        let a = arch().unwrap();
        for nr in [
            libc::SYS_connect,
            libc::SYS_sendmsg,
            libc::SYS_sendmmsg,
            libc::SYS_io_uring_setup,
            libc::SYS_io_uring_enter,
            libc::SYS_io_uring_register,
        ] {
            assert_eq!(evaluate(a, nr as u32, 0), DENY);
        }
        assert_eq!(evaluate(a, libc::SYS_sendto as u32, 0), ALLOW);
        for pointer in [1, 0x100000000, 0xffffffffffffffff] {
            assert_eq!(evaluate(a, libc::SYS_sendto as u32, pointer), DENY);
        }
        assert_eq!(evaluate(a, libc::SYS_read as u32, 0), ALLOW);
        assert_eq!(evaluate(a, libc::SYS_write as u32, 0), ALLOW);
        assert_eq!(evaluate(a ^ 1, libc::SYS_read as u32, 0), KILL);
        #[cfg(target_arch = "x86_64")]
        assert_eq!(evaluate(a, 0x40000000 | libc::SYS_connect as u32, 0), KILL);
    }
}
