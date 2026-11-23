// A TikZ picture's points as dots to drag: under the TikZ editor, a small canvas with a dot for
// each `(x,y)` written in the picture's source. Dragging a dot rewrites the two numbers in the
// editor's text (to a quarter of a unit), and the picture above retypesets as for any typing.
// The canvas keeps its scale while a dot is held, so it does not run away from the pointer.
use super::*;
use crate::document::tikz;

const CANVAS_H: f32 = 150.0;
const MARGIN: f32 = 18.0;
/// The most a unit is drawn as, in pixels (a lone point is not blown up).
const MOST: f64 = 80.0;
/// What a dot is moved by at least.
const STEP: f64 = 0.25;

/// How the picture's units map onto the canvas.
#[derive(Clone, Copy)]
pub(super) struct View {
    min_x: f64,
    max_y: f64,
    scale: f64,
    ox: f32,
    oy: f32,
}

impl View {
    fn fit(pts: &[tikz::Point], w: f32, h: f32) -> Self {
        let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for p in pts {
            (x0, x1) = (x0.min(p.x), x1.max(p.x));
            (y0, y1) = (y0.min(p.y), y1.max(p.y));
        }
        let (sx, sy) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        let scale = (f64::from(w - 2.0 * MARGIN) / sx)
            .min(f64::from(h - 2.0 * MARGIN) / sy)
            .min(MOST)
            .max(1.0);
        Self {
            min_x: x0,
            max_y: y1,
            scale,
            ox: (w - (sx * scale) as f32) / 2.0,
            oy: (h - (sy * scale) as f32) / 2.0,
        }
    }

    /// Where (`x`, `y`) is on the canvas.
    fn at(&self, x: f64, y: f64) -> (f32, f32) {
        (
            self.ox + ((x - self.min_x) * self.scale) as f32,
            self.oy + ((self.max_y - y) * self.scale) as f32,
        )
    }
}

/// A dot held: which, and where it and the pointer were when it was taken.
pub(super) struct Pointing {
    ix: usize,
    from: Point<Pixels>,
    start: (f64, f64),
    view: View,
    now: (f64, f64),
}

impl Doc {
    /// The points of the picture in the TikZ editor, as the editor's text has them now.
    fn picture_points(&self, cx: &App) -> Vec<tikz::Point> {
        match &self.active {
            Some(a) if a.panel == Some(Panel::Source) => tikz::points(&self.input.read(cx).value()),
            _ => Vec::new(),
        }
    }

    /// The canvas of dots, `width` wide, under the TikZ editor; none for a picture without plain
    /// points.
    pub(super) fn render_points(&self, width: f32, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pts = self.picture_points(cx);
        if pts.is_empty() {
            return None;
        }
        let theme = cx.theme().clone();
        let view = self
            .pointing
            .as_ref()
            .map_or_else(|| View::fit(&pts, width, CANVAS_H), |p| p.view);
        let held = self.pointing.as_ref();
        let dots = pts.iter().enumerate().map(|(i, p)| {
            let (x, y) = view.at(p.x, p.y);
            let on = held.is_some_and(|h| h.ix == i);
            let size = if on { 12.0 } else { 10.0 };
            div()
                .id(("tikz-point", i))
                .role(Role::Button)
                .aria_label(format!("Point ({}, {})", tikz::show(p.x), tikz::show(p.y)))
                .absolute()
                .left(px(x - size / 2.0))
                .top(px(y - size / 2.0))
                .size(px(size))
                .rounded_full()
                .bg(theme.primary)
                .border_1()
                .border_color(theme.popover)
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.point_down(i, ev.position, width, cx);
                    }),
                )
        });
        let label = held.map(|h| {
            let (x, y) = view.at(h.now.0, h.now.1);
            div()
                .absolute()
                .left(px((x + 10.0).min(width - 70.0)))
                .top(px((y - 22.0).max(0.0)))
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format!(
                    "({}, {})",
                    tikz::show(h.now.0),
                    tikz::show(h.now.1)
                ))
        });
        Some(
            div()
                .id("tikz-points")
                .relative()
                .w(px(width))
                .h(px(CANVAS_H))
                .overflow_hidden()
                .rounded_md()
                .border_1()
                .border_color(theme.border)
                .bg(theme.secondary)
                .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, window, cx| {
                    if this.pointing.is_none() {
                        return;
                    }
                    // a release off the canvas was not seen: the drag ends with the next move
                    match ev.pressed_button {
                        Some(MouseButton::Left) => this.point_to(ev.position, window, cx),
                        _ => this.point_end(cx),
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.point_end(cx)),
                )
                .children(dots)
                .children(label)
                .into_any_element(),
        )
    }

    /// Dot `ix` taken with the pointer at `pos`.
    pub(super) fn point_down(
        &mut self,
        ix: usize,
        pos: Point<Pixels>,
        width: f32,
        cx: &mut Context<Self>,
    ) {
        let pts = self.picture_points(cx);
        let Some(p) = pts.get(ix) else {
            return;
        };
        // one undo step for the whole drag
        self.sync_box(true, cx);
        self.checkpoint();
        self.pointing = Some(Pointing {
            ix,
            from: pos,
            start: (p.x, p.y),
            view: View::fit(&pts, width, CANVAS_H),
            now: (p.x, p.y),
        });
        cx.notify();
    }

    /// The pointer is at `pos` during a drag: the dot goes there, to the step.
    pub(super) fn point_to(
        &mut self,
        pos: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(h) = self.pointing.as_ref() else {
            return;
        };
        let (dx, dy) = (f32::from(pos.x - h.from.x), f32::from(pos.y - h.from.y));
        let x = tikz::snap(h.start.0 + f64::from(dx) / h.view.scale, STEP);
        let y = tikz::snap(h.start.1 - f64::from(dy) / h.view.scale, STEP);
        if (x, y) == h.now {
            return;
        }
        let ix = h.ix;
        let text = self.input.read(cx).value().to_string();
        let Some(new) = tikz::moved(&text, ix, x, y) else {
            return;
        };
        self.input.update(cx, |s, cx| {
            let at = s.cursor();
            s.set_value(new, window, cx);
            let c = at.min(s.value().len());
            s.set_selected_range(c..c, cx);
        });
        self.sync_box(true, cx);
        if let Some(h) = self.pointing.as_mut() {
            h.now = (x, y);
        }
        cx.notify();
    }

    /// The dot let go.
    pub(super) fn point_end(&mut self, cx: &mut Context<Self>) {
        if self.pointing.take().is_some() {
            cx.notify();
        }
    }
}
