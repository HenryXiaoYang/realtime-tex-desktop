// How the window's width shapes the workspace: below some widths the project panel, then the
// sidebar, step aside for the pages (a toggle brings one back for as long as the window stays
// narrow), and the Insert tab drops its buttons' names when they do not fit (it measures itself
// each frame); the Home tab scrolls instead.
use super::*;
use std::cell::Cell;

/// The window width from which the project panel shows (beside the pages and the sidebar).
const LEFT_FROM: f32 = 1100.0;
/// The window width from which the sidebar shows.
const RIGHT_FROM: f32 = 940.0;
/// The smallest window: half of a 1440 pt screen, a ribbon tab and a page at 75%.
pub const MIN_WINDOW: (f32, f32) = (720.0, 480.0);

/// What the window's width allows, worked out once a frame.
#[derive(Default)]
pub(super) struct Fit {
    /// The project panel, the sidebar, shown although the window is narrow (until it widens).
    left_anyway: bool,
    right_anyway: bool,
    pub(super) left: bool,
    pub(super) right: bool,
    /// The ribbon's width last frame.
    pub(super) ribbon_w: Rc<Cell<f32>>,
    /// The ribbon's scroll: how far its groups run past it.
    pub(super) ribbon_scroll: ScrollHandle,
    /// The Insert tab shows its buttons' names no more (they ran past the ribbon), and how wide
    /// they made it then.
    insert_compact: Rc<Cell<bool>>,
    insert_full_w: Rc<Cell<f32>>,
}

impl Workspace {
    /// Which panels show, for the window's width now.
    pub(super) fn fit_panels(&mut self, window: &Window, cx: &App) {
        let w = f32::from(window.viewport_size().width);
        let inspector = self.inspector_visible(cx);
        let fit = &mut self.fit;
        if w >= LEFT_FROM {
            fit.left_anyway = false;
        }
        if w >= RIGHT_FROM {
            fit.right_anyway = false;
        }
        fit.left = self.sidebar && (w >= LEFT_FROM || fit.left_anyway);
        fit.right = inspector && (w >= RIGHT_FROM || fit.right_anyway);
    }

    /// The project panel shown or hidden: one the window's width hid comes back first.
    pub(super) fn toggle_left(&mut self, cx: &mut Context<Self>) {
        if self.sidebar && !self.fit.left {
            self.fit.left_anyway = true;
        } else {
            self.sidebar = !self.fit.left;
            self.fit.left_anyway = false;
        }
        cx.notify();
    }

    /// The sidebar shown or hidden, likewise (the choice to close it is remembered).
    pub(super) fn toggle_right(&mut self, cx: &mut Context<Self>) {
        let closed = settings::AppSettings::get(cx).inspector_closed;
        if !closed && !self.fit.right {
            self.fit.right_anyway = true;
        } else if self.fit.right_anyway {
            self.fit.right_anyway = false;
        } else {
            settings::AppSettings::set(cx, |s| s.inspector_closed = !closed);
        }
        cx.notify();
    }

    /// Whether the Insert tab shows its buttons as icons alone.
    pub(super) fn insert_compact(&self) -> bool {
        self.fit.insert_compact.get()
    }

    /// A canvas over the ribbon that, on the Insert tab (`on`), drops its buttons' names when
    /// they ran past the ribbon last frame and gives them back once it is as wide as they made
    /// it: measured, not guessed.
    pub(super) fn watch_insert(&self, on: bool) -> impl IntoElement {
        let (scroll, compact, full) = (
            self.fit.ribbon_scroll.clone(),
            self.fit.insert_compact.clone(),
            self.fit.insert_full_w.clone(),
        );
        canvas(
            move |b, window, _| {
                let view = f32::from(b.size.width);
                let over = f32::from(scroll.max_offset().x);
                if !on {
                } else if !compact.get() && over > 1.0 {
                    full.set(view + over);
                    compact.set(true);
                    window.refresh();
                } else if compact.get() && view >= full.get() {
                    compact.set(false);
                    window.refresh();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
    }

    /// A canvas that keeps the ribbon's width for the next frame, redrawing when it changed.
    pub(super) fn measure_ribbon(&self) -> impl IntoElement {
        measure(self.fit.ribbon_w.clone())
    }
}

/// A canvas over an element (absolute, its size) that keeps the element's width in `w` for
/// the next frame, redrawing when it changed.
pub(super) fn measure(w: Rc<Cell<f32>>) -> impl IntoElement {
    canvas(
        move |b, window, _| {
            let now = f32::from(b.size.width);
            if (now - w.get()).abs() > 1.0 {
                w.set(now);
                window.refresh();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

#[cfg(test)]
mod e2e {
    use super::super::Workspace;
    use super::MIN_WINDOW;
    use crate::engine;
    use crate::ui::settings::AppSettings;
    use gpui_kit::{TestAppContext, Window, px, size};

    /// The workspace at three widths: the panels step aside, a toggle brings a
    /// hidden panel back until the window widens. `cargo test --release layout_follows_the_width
    /// -- --ignored --nocapture` (opens a document with rtex).
    #[gpui_kit::test]
    #[ignore]
    fn layout_follows_the_width(cx: &mut TestAppContext) {
        let dir = std::env::temp_dir().join(format!("rtd-layout-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("main.tex"),
            "\\documentclass{article}\n\\begin{document}\nSome text.\n\\end{document}\n",
        )
        .unwrap();
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        engine::configure("", "/Library/TeX/texbin");
        let (ws, cx) = cx.add_window_view(|window: &mut Window, cx| {
            Workspace::new(dir.clone(), Some("main.tex".into()), window, cx)
        });
        let at = |w: f32, cx: &mut gpui_kit::VisualTestContext| {
            cx.simulate_resize(size(px(w), px(900.0)));
            // a few frames: the ribbon measures itself on one and fits on the next
            for _ in 0..3 {
                cx.run_until_parked();
            }
            cx.read(|cx| {
                let ws = ws.read(cx);
                (ws.fit.left, ws.fit.right, ws.fit.ribbon_w.get())
            })
        };
        let wide = at(1460.0, cx);
        let middle = at(1000.0, cx);
        let narrow = at(800.0, cx);
        let least = at(MIN_WINDOW.0, cx);
        println!("1460: {wide:?}\n1000: {middle:?}\n 800: {narrow:?}\n 720: {least:?}");
        assert!(wide.0 && wide.1, "both panels at 1460");
        assert!(
            !middle.0 && middle.1,
            "the project panel steps aside at 1000"
        );
        assert!(!narrow.0 && !narrow.1, "both at 800");
        assert!(narrow.2 > middle.2 - 400.0, "the pages get the room");
        // a toggle while narrow brings the panel back; widening forgets that
        ws.update(cx, |ws, cx| ws.toggle_left(cx));
        assert!(at(800.0, cx).0, "shown on request");
        assert!(at(1460.0, cx).0);
        assert!(!at(800.0, cx).0, "narrow again: aside again");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
