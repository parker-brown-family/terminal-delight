//! A bounded hidden clock; terminal output and protocol replies never wait on it.
use super::*;

pub(super) const RESIZE_SETTLE: Duration = Duration::from_millis(140);
pub(super) const HIDDEN_INTERVAL: Duration = Duration::from_secs(1);

impl TerminalView {
    /// Called during workspace rendering, with only a shared pane reference.
    /// Repeated draws must not turn this into an output-rate wakeup.
    pub(crate) fn mark_shown(&self, shown: bool) {
        if self.shown.swap(shown, std::sync::atomic::Ordering::Relaxed) != shown {
            self.wake_housekeeping();
        }
    }

    pub(super) fn wake_housekeeping(&self) {
        // Full means a wake is already queued. Disconnected means the view's
        // clock has ended. Neither needs another task or an unbounded queue.
        let _ = self.housekeeping_wake.borrow_mut().try_send(());
    }

    pub(super) fn stage_pty_size(&mut self, target: PtySize) {
        let told = PtySize {
            grid: self.grid,
            cell_px: self.cell_px,
        };
        if target == told {
            self.pending_pty = None;
        } else if !self.pending_pty.is_some_and(|(staged, _)| staged == target) {
            self.pending_pty = Some((target, Instant::now()));
            self.wake_housekeeping();
        }
    }

    /// Only a real completed thinking spell needs the short bell debounce.
    /// An ineligible/stale stop stamp must not keep a hidden pane awake.
    pub(super) fn completion_deadline(&self) -> Option<Instant> {
        let start = self.think_since?;
        let end = self.not_thinking_since?;
        (self.mode.is_agent()
            && !self.bell
            && self.agent_thinking == Some(false)
            && end.saturating_duration_since(start) > Duration::from_millis(1200))
        .then_some(end + Duration::from_millis(300))
    }

    pub(super) fn housekeeping_delay(&self) -> Duration {
        let shown = self.shown.load(std::sync::atomic::Ordering::Relaxed);
        let mut delay = if shown && self.fx.active() {
            Duration::from_millis(33)
        } else if shown {
            Duration::from_millis(150)
        } else {
            HIDDEN_INTERVAL.saturating_sub(self.last_think_scan.elapsed())
        };
        if let Some((_, since)) = self.pending_pty {
            delay = delay.min(RESIZE_SETTLE.saturating_sub(since.elapsed()));
        }
        if let Some(at) = self.completion_deadline() {
            delay = delay.min(at.saturating_duration_since(Instant::now()));
        }
        // A due bell may still be held by scroll-settle. Retry at a bounded
        // rate rather than spinning on an already expired deadline.
        delay.max(Duration::from_millis(33))
    }
}
