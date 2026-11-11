//! The window's one drag (see [`Drag`]).

use super::*;

/// The one drag the pointer can be making. A press sets it, replacing any
/// drag the window never saw end (a release the compositor took), and the
/// release takes it, so two drags at once cannot be written
/// (verify/drags: `APressDragsOnlyWhatItPressed`, `AReleaseEndsTheDrag`).
pub(crate) enum Drag {
    Floor(Entity<TerminalView>),
    Agents(u8),
    Slider(theme::GradeKey),
    Wheel(WheelTarget),
    Light,
    Track(usize),
    Split(u64),
    Pane(PaneDrag),
    Tab(TabDrag),
    Group(GroupDrag),
    Bar(BarDrag),
    Inbox(inbox_view::InboxDrag),
    TabWheel(TabPip),
    TabLight,
    FocusZoom,
    CardScale,
    FocusSel,
}

/// One accessor per kind, named after the field it replaced.
#[allow(dead_code)]
impl Workspace {
    pub(crate) fn floor_drag(&self) -> Option<&Entity<TerminalView>> {
        match &self.drag {
            Some(Drag::Floor(d)) => Some(d),
            _ => None,
        }
    }
    pub(crate) fn floor_drag_mut(&mut self) -> Option<&mut Entity<TerminalView>> {
        match &mut self.drag {
            Some(Drag::Floor(d)) => Some(d),
            _ => None,
        }
    }
    /// End this drag if it is the one being made.
    pub(crate) fn take_floor_drag(&mut self) -> Option<Entity<TerminalView>> {
        match self.drag.take() {
            Some(Drag::Floor(d)) => Some(d),
            other => {
                self.drag = other;
                None
            }
        }
    }
    pub(crate) fn agents_drag(&self) -> Option<&u8> {
        match &self.drag {
            Some(Drag::Agents(d)) => Some(d),
            _ => None,
        }
    }
    pub(crate) fn agents_drag_mut(&mut self) -> Option<&mut u8> {
        match &mut self.drag {
            Some(Drag::Agents(d)) => Some(d),
            _ => None,
        }
    }
    /// End this drag if it is the one being made.
    pub(crate) fn take_agents_drag(&mut self) -> Option<u8> {
        match self.drag.take() {
            Some(Drag::Agents(d)) => Some(d),
            other => {
                self.drag = other;
                None
            }
        }
    }
    pub(crate) fn slider_drag(&self) -> Option<&theme::GradeKey> {
        match &self.drag {
            Some(Drag::Slider(d)) => Some(d),
            _ => None,
        }
    }
    pub(crate) fn slider_drag_mut(&mut self) -> Option<&mut theme::GradeKey> {
        match &mut self.drag {
            Some(Drag::Slider(d)) => Some(d),
            _ => None,
        }
    }
    /// End this drag if it is the one being made.
    pub(crate) fn take_slider_drag(&mut self) -> Option<theme::GradeKey> {
        match self.drag.take() {
            Some(Drag::Slider(d)) => Some(d),
            other => {
                self.drag = other;
                None
            }
        }
    }
    pub(crate) fn wheel_drag(&self) -> Option<&WheelTarget> {
        match &self.drag {
            Some(Drag::Wheel(d)) => Some(d),
            _ => None,
        }
    }
    pub(crate) fn wheel_drag_mut(&mut self) -> Option<&mut WheelTarget> {
        match &mut self.drag {
            Some(Drag::Wheel(d)) => Some(d),
            _ => None,
        }
    }
    /// End this drag if it is the one being made.
    pub(crate) fn take_wheel_drag(&mut self) -> Option<WheelTarget> {
        match self.drag.take() {
            Some(Drag::Wheel(d)) => Some(d),
            other => {
                self.drag = other;
                None
            }
        }
    }
    pub(crate) fn light_drag(&self) -> bool {
        matches!(self.drag, Some(Drag::Light))
    }
    /// End this drag if it is the one being made.
    pub(crate) fn take_light_drag(&mut self) -> bool {
        let was = self.light_drag();
        if was {
            self.drag = None;
        }
        was
    }
    pub(crate) fn track_drag(&self) -> Option<&usize> {
        match &self.drag {
            Some(Drag::Track(d)) => Some(d),
            _ => None,
        }
    }
    pub(crate) fn track_drag_mut(&mut self) -> Option<&mut usize> {
        match &mut self.drag {
            Some(Drag::Track(d)) => Some(d),
            _ => None,
        }
    }
    /// End this drag if it is the one being made.
    pub(crate) fn take_track_drag(&mut self) -> Option<usize> {
        match self.drag.take() {
            Some(Drag::Track(d)) => Some(d),
            other => {
                self.drag = other;
                None
            }
        }
    }
    pub(crate) fn drag_split(&self) -> Option<&u64> {
        match &self.drag {
            Some(Drag::Split(d)) => Some(d),
            _ => None,
        }
    }
    pub(crate) fn drag_split_mut(&mut self) -> Option<&mut u64> {
        match &mut self.drag {
            Some(Drag::Split(d)) => Some(d),
            _ => None,
        }
    }
    /// End this drag if it is the one being made.
    pub(crate) fn take_drag_split(&mut self) -> Option<u64> {
        match self.drag.take() {
            Some(Drag::Split(d)) => Some(d),
            other => {
                self.drag = other;
                None
            }
        }
    }
    pub(crate) fn drag_pane(&self) -> Option<&PaneDrag> {
        match &self.drag {
            Some(Drag::Pane(d)) => Some(d),
            _ => None,
        }
    }
    pub(crate) fn drag_pane_mut(&mut self) -> Option<&mut PaneDrag> {
        match &mut self.drag {
            Some(Drag::Pane(d)) => Some(d),
            _ => None,
        }
    }
    /// End this drag if it is the one being made.
    pub(crate) fn take_drag_pane(&mut self) -> Option<PaneDrag> {
        match self.drag.take() {
            Some(Drag::Pane(d)) => Some(d),
            other => {
                self.drag = other;
                None
            }
        }
    }
    pub(crate) fn tab_drag(&self) -> Option<&TabDrag> {
        match &self.drag {
            Some(Drag::Tab(d)) => Some(d),
            _ => None,
        }
    }
    pub(crate) fn tab_drag_mut(&mut self) -> Option<&mut TabDrag> {
        match &mut self.drag {
            Some(Drag::Tab(d)) => Some(d),
            _ => None,
        }
    }
    /// End this drag if it is the one being made.
    pub(crate) fn take_tab_drag(&mut self) -> Option<TabDrag> {
        match self.drag.take() {
            Some(Drag::Tab(d)) => Some(d),
            other => {
                self.drag = other;
                None
            }
        }
    }
    pub(crate) fn group_drag(&self) -> Option<&GroupDrag> {
        match &self.drag {
            Some(Drag::Group(d)) => Some(d),
            _ => None,
        }
    }
    pub(crate) fn group_drag_mut(&mut self) -> Option<&mut GroupDrag> {
        match &mut self.drag {
            Some(Drag::Group(d)) => Some(d),
            _ => None,
        }
    }
    /// End this drag if it is the one being made.
    pub(crate) fn take_group_drag(&mut self) -> Option<GroupDrag> {
        match self.drag.take() {
            Some(Drag::Group(d)) => Some(d),
            other => {
                self.drag = other;
                None
            }
        }
    }
    pub(crate) fn bar_drag(&self) -> Option<&BarDrag> {
        match &self.drag {
            Some(Drag::Bar(d)) => Some(d),
            _ => None,
        }
    }
    pub(crate) fn bar_drag_mut(&mut self) -> Option<&mut BarDrag> {
        match &mut self.drag {
            Some(Drag::Bar(d)) => Some(d),
            _ => None,
        }
    }
    /// End this drag if it is the one being made.
    pub(crate) fn take_bar_drag(&mut self) -> Option<BarDrag> {
        match self.drag.take() {
            Some(Drag::Bar(d)) => Some(d),
            other => {
                self.drag = other;
                None
            }
        }
    }
    pub(crate) fn inbox_drag(&self) -> Option<&inbox_view::InboxDrag> {
        match &self.drag {
            Some(Drag::Inbox(d)) => Some(d),
            _ => None,
        }
    }
    pub(crate) fn inbox_drag_mut(&mut self) -> Option<&mut inbox_view::InboxDrag> {
        match &mut self.drag {
            Some(Drag::Inbox(d)) => Some(d),
            _ => None,
        }
    }
    /// End this drag if it is the one being made.
    pub(crate) fn take_inbox_drag(&mut self) -> Option<inbox_view::InboxDrag> {
        match self.drag.take() {
            Some(Drag::Inbox(d)) => Some(d),
            other => {
                self.drag = other;
                None
            }
        }
    }
    pub(crate) fn tab_wheel_drag(&self) -> Option<&TabPip> {
        match &self.drag {
            Some(Drag::TabWheel(d)) => Some(d),
            _ => None,
        }
    }
    pub(crate) fn tab_wheel_drag_mut(&mut self) -> Option<&mut TabPip> {
        match &mut self.drag {
            Some(Drag::TabWheel(d)) => Some(d),
            _ => None,
        }
    }
    /// End this drag if it is the one being made.
    pub(crate) fn take_tab_wheel_drag(&mut self) -> Option<TabPip> {
        match self.drag.take() {
            Some(Drag::TabWheel(d)) => Some(d),
            other => {
                self.drag = other;
                None
            }
        }
    }
    pub(crate) fn tab_light_drag(&self) -> bool {
        matches!(self.drag, Some(Drag::TabLight))
    }
    /// End this drag if it is the one being made.
    pub(crate) fn take_tab_light_drag(&mut self) -> bool {
        let was = self.tab_light_drag();
        if was {
            self.drag = None;
        }
        was
    }
    pub(crate) fn focus_zoom_drag(&self) -> bool {
        matches!(self.drag, Some(Drag::FocusZoom))
    }
    /// End this drag if it is the one being made.
    pub(crate) fn take_focus_zoom_drag(&mut self) -> bool {
        let was = self.focus_zoom_drag();
        if was {
            self.drag = None;
        }
        was
    }
    pub(crate) fn card_scale_drag(&self) -> bool {
        matches!(self.drag, Some(Drag::CardScale))
    }
    /// End this drag if it is the one being made.
    pub(crate) fn take_card_scale_drag(&mut self) -> bool {
        let was = self.card_scale_drag();
        if was {
            self.drag = None;
        }
        was
    }
    pub(crate) fn focus_sel_drag(&self) -> bool {
        matches!(self.drag, Some(Drag::FocusSel))
    }
    /// End this drag if it is the one being made.
    pub(crate) fn take_focus_sel_drag(&mut self) -> bool {
        let was = self.focus_sel_drag();
        if was {
            self.drag = None;
        }
        was
    }
}

/// Set or clear one kind: `Some` starts it (replacing any other), `None`
/// ends it if it is the one being made.
#[allow(dead_code)]
impl Workspace {
    pub(crate) fn set_floor_drag(&mut self, d: Option<Entity<TerminalView>>) {
        match d {
            Some(d) => self.drag = Some(Drag::Floor(d)),
            None => {
                self.take_floor_drag();
            }
        }
    }
    pub(crate) fn set_agents_drag(&mut self, d: Option<u8>) {
        match d {
            Some(d) => self.drag = Some(Drag::Agents(d)),
            None => {
                self.take_agents_drag();
            }
        }
    }
    pub(crate) fn set_slider_drag(&mut self, d: Option<theme::GradeKey>) {
        match d {
            Some(d) => self.drag = Some(Drag::Slider(d)),
            None => {
                self.take_slider_drag();
            }
        }
    }
    pub(crate) fn set_wheel_drag(&mut self, d: Option<WheelTarget>) {
        match d {
            Some(d) => self.drag = Some(Drag::Wheel(d)),
            None => {
                self.take_wheel_drag();
            }
        }
    }
    pub(crate) fn set_track_drag(&mut self, d: Option<usize>) {
        match d {
            Some(d) => self.drag = Some(Drag::Track(d)),
            None => {
                self.take_track_drag();
            }
        }
    }
    pub(crate) fn set_drag_split(&mut self, d: Option<u64>) {
        match d {
            Some(d) => self.drag = Some(Drag::Split(d)),
            None => {
                self.take_drag_split();
            }
        }
    }
    pub(crate) fn set_drag_pane(&mut self, d: Option<PaneDrag>) {
        match d {
            Some(d) => self.drag = Some(Drag::Pane(d)),
            None => {
                self.take_drag_pane();
            }
        }
    }
    pub(crate) fn set_tab_drag(&mut self, d: Option<TabDrag>) {
        match d {
            Some(d) => self.drag = Some(Drag::Tab(d)),
            None => {
                self.take_tab_drag();
            }
        }
    }
    pub(crate) fn set_group_drag(&mut self, d: Option<GroupDrag>) {
        match d {
            Some(d) => self.drag = Some(Drag::Group(d)),
            None => {
                self.take_group_drag();
            }
        }
    }
    pub(crate) fn set_bar_drag(&mut self, d: Option<BarDrag>) {
        match d {
            Some(d) => self.drag = Some(Drag::Bar(d)),
            None => {
                self.take_bar_drag();
            }
        }
    }
    pub(crate) fn set_inbox_drag(&mut self, d: Option<inbox_view::InboxDrag>) {
        match d {
            Some(d) => self.drag = Some(Drag::Inbox(d)),
            None => {
                self.take_inbox_drag();
            }
        }
    }
    pub(crate) fn set_tab_wheel_drag(&mut self, d: Option<TabPip>) {
        match d {
            Some(d) => self.drag = Some(Drag::TabWheel(d)),
            None => {
                self.take_tab_wheel_drag();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(cx: &mut gpui::TestAppContext) -> (Entity<Workspace>, &mut gpui::VisualTestContext) {
        cx.executor().allow_parking();
        cx.update(|cx| {
            theme::init_embedded(cx, theme::house_outer());
            skin::init_embedded(cx);
        });
        let seed = session::PaneRestore {
            program: Some(("/bin/sh".into(), vec!["-c".into(), "exec /bin/cat".into()])),
            ..Default::default()
        };
        let window = {
            let _fork = crate::testsync::forks_and_locks();
            cx.open_window(size(px(1200.), px(800.)), |w, cx| {
                Workspace::build(true, true, Some(seed), None, w, cx)
            })
        };
        let ws = window.root(cx).unwrap();
        let cx = gpui::VisualTestContext::from_window(window.into(), cx).into_mut();
        let stops: Vec<_> = ws
            .read_with(cx, |ws, _| ws.every_view())
            .into_iter()
            .map(|p| p.read_with(cx, |v, _| v.fixture_shutdown()))
            .collect();
        cx.on_quit(move || stops.into_iter().for_each(|s| s()));
        (ws, cx)
    }

    /// The bug path of verify/drags, one step per action: a drag starts (a
    /// split), its release never reaches the window, a press starts another
    /// before the pointer moves, and one release follows. The press drags
    /// only what it pressed, and the release leaves no drag behind. With a
    /// field per drag, the split's stayed set and came back to life.
    #[gpui::test]
    fn a_lost_release_never_brings_a_drag_back_to_life(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx) = window(cx);
        cx.run_until_parked();
        cx.update(|_, cx| {
            ws.update(cx, |ws, _| {
                ws.set_drag_split(Some(1));
                // A press on FOCUS's text-size control: live-only, so its
                // release persists nothing a test would leave behind.
                ws.drag = Some(Drag::FocusZoom);
            })
        });
        ws.read_with(cx, |ws, _| {
            assert!(
                ws.drag_split().is_none(),
                "the press drags only what it pressed"
            );
            assert!(ws.focus_zoom_drag());
        });
        cx.simulate_mouse_up(
            point(px(600.), px(400.)),
            MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.run_until_parked();
        ws.read_with(cx, |ws, _| {
            assert!(
                ws.drag.is_none(),
                "one release ends the drag, and none is left"
            );
        });
    }
}
