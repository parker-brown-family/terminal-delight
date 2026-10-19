//! The ruler `scripts/idle-memory.sh --empty-gpui` measures Terminal Delight
//! against: one gpui window holding one line of text, and nothing else.
//!
//! Whatever this window holds, any gpui application on this machine pays — the
//! Vulkan device, the swapchain, gpui's own render targets, the driver's
//! libraries. Whatever Terminal Delight holds beyond it is Terminal Delight's.
//! Issue #953 asks for exactly that line: "the GPU figure matches what an empty
//! gpui window with one text element holds within 20%".
//!
//! It is titled the way a Terminal Delight window is (`terminal-delight —
//! <TD_SESSION>`), so `scripts/lib/hidden-window.sh` finds it, checks it landed
//! on the hidden workspace, and tears it down like any other.
//!
//! `EMPTY_GPUI_PATH=1` adds one filled triangle, drawn the way a sticky note
//! draws its paper (`PathBuilder` and `paint_path`): the one thing that makes
//! gpui create its path render targets, which since
//! `docs/patches/0006-lazy-path-targets.patch` wait for a path to be drawn.
use gpui::{
    canvas, div, point, prelude::*, px, rgb, size, App, Bounds, Context, PathBuilder, SharedString,
    Window, WindowBounds, WindowOptions,
};
use gpui_platform::application;

struct OneLine {
    path: bool,
}

impl Render for OneLine {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let path = self.path;
        div().size_full().child("one line of text").when(path, |d| {
            d.child(
                canvas(
                    |_, _, _| {},
                    |bounds, _, window, _| {
                        let o = bounds.origin;
                        let mut b = PathBuilder::fill();
                        b.move_to(point(o.x + px(10.), o.y + px(10.)));
                        b.line_to(point(o.x + px(90.), o.y + px(10.)));
                        b.line_to(point(o.x + px(50.), o.y + px(80.)));
                        b.close();
                        if let Ok(p) = b.build() {
                            window.paint_path(p, rgb(0xf5c542));
                        }
                    },
                )
                .size(px(100.)),
            )
        })
    }
}

fn main() {
    // hidden-window.sh waits for a window by asking `$TD ctl --pid <pid> ping`,
    // with $TD set to this binary. A ruler that opened a window for that would
    // open it on the desktop, outside the hidden workspace, and never close it:
    // so any argument at all answers the ping and exits, and only a bare launch
    // draws anything.
    if std::env::args().len() > 1 {
        if std::env::args().any(|a| a == "ping") {
            println!("0\tpong");
        }
        return;
    }
    let session = std::env::var("TD_SESSION").unwrap_or_else(|_| "empty-gpui".into());
    let path = std::env::var_os("EMPTY_GPUI_PATH").is_some();
    application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.), px(600.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(SharedString::from(format!("terminal-delight — {session}"))),
                    ..Default::default()
                }),
                app_id: Some("terminal-delight-empty-gpui".into()),
                ..Default::default()
            },
            |_, cx| cx.new(|_| OneLine { path }),
        )
        .unwrap();
    });
}
