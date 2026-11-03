
#[cfg(test)] mod zombie_probe {
 #[test] fn an_exited_child_must_not_be_reported_as_a_live_unknown() {
  let dir = crate::testsync::Scratch::new("zombie-probe");
  let exe = dir.join("td-exited");
  std::fs::copy("/usr/bin/true", &exe).unwrap();
  let mut child = std::process::Command::new(&exe).spawn().unwrap();
  let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::uninit();
  let observed = unsafe { libc::waitid(libc::P_PID, child.id(), info.as_mut_ptr(), libc::WEXITED | libc::WNOWAIT) };
  assert_eq!(observed, 0);
  let found = super::own_processes().into_iter().find(|p| p.pid == child.id());
  child.wait().unwrap();
  assert!(found.is_none(), "confirmed exited child returned as {found:?}");
 }
}
