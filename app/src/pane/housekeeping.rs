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

    /// A quiet visible shell receives output, mode changes and resizes through
    /// independent channels. Hidden streams intentionally suppress output events
    /// and still need the slow generation check for the workspace's previews.
    pub(super) fn housekeeping_delay(&self) -> Option<Duration> {
        let shown = self.shown.load(std::sync::atomic::Ordering::Relaxed);
        let mut delay = if !shown {
            if self.hosts_agent() {
                // An unrelated wake must not postpone the next scan by a full interval.
                Some(HIDDEN_INTERVAL.saturating_sub(self.last_think_scan.elapsed()))
            } else if self.offscreen_dirty {
                Some(OFFSCREEN_REFRESH.saturating_sub(self.offscreen_flushed.elapsed()))
            } else {
                None
            }
        } else if self.fx.active() {
            Some(Duration::from_millis(33))
        } else if self.hosts_agent() || self.fx_recent() {
            Some(Duration::from_millis(150))
        } else {
            None
        };
        for deadline in [
            self.pending_pty.map(|(_, since)| since + RESIZE_SETTLE),
            self.completion_deadline(),
        ]
        .into_iter()
        .flatten()
        {
            let wait = deadline.saturating_duration_since(Instant::now());
            delay = Some(delay.map_or(wait, |d| d.min(wait)));
        }
        delay.map(|d| d.max(Duration::from_millis(33)))
    }

    pub(super) fn fx_recent(&self) -> bool {
        self.last_screen_output
            .is_some_and(|t| t.elapsed() < Duration::from_millis(750))
    }
}
