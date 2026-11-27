// A TikZ picture's points as dots to drag: under the TikZ editor, a small canvas with a dot for
// each `(x,y)` written in the picture's source. Dragging a dot rewrites the two numbers in the
// editor's text (to a quarter of a unit), and the picture above retypesets as for any typing.
// The canvas keeps its scale while a dot is held, so it does not run away from the pointer.
use super::*;
use crate::document::tikz;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use std::cell::Cell;
use std::rc::Rc;

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
        // a picture without points yet: room for a few units
        if pts.is_empty() {
            (x0, x1, y0, y1) = (0.0, 4.0, 0.0, 3.0);
        }
        let (sx, sy) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        let scale = (f64::from(w - 2.0 * MARGIN) / sx)
            .min(f64::from(h - 2.0 * MARGIN) / sy)
            .clamp(1.0, MOST);
        Self {
            min_x: x0,
            max_y: y1,
            scale,
            ox: (w - (sx * scale) as f32) / 2.0,
            oy: (h - (sy * scale) as f32) / 2.0,
        }
    }

    /// The picture's place at canvas position (`px`, `py`).
    fn place(&self, px: f32, py: f32) -> (f64, f64) {
        (
            self.min_x + f64::from(px - self.ox) / self.scale,
            self.max_y - f64::from(py - self.oy) / self.scale,
        )
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

/// What the TikZ canvas has besides a dot held: the statement picked, a shape waiting to be
/// placed, a label being edited, a circle's radius held.
#[derive(Default)]
pub(super) struct Ui {
    stmt: Option<usize>,
    placing: Option<tikz::Shape>,
    label: Option<LabelEdit>,
    radius: Option<RadiusDrag>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

struct LabelEdit {
    ix: usize,
    input: Entity<InputState>,
    _sub: Subscription,
}

struct RadiusDrag {
    ix: usize,
    from: f32,
    r0: f64,
    view: View,
}

const STROKES: &[(&str, &str, u32)] = &[
    ("black", "Black", 0x111827),
    ("red", "Red", 0xdc2626),
    ("blue", "Blue", 0x2563eb),
    ("green!50!black", "Green", 0x15803d),
];
const FILLS: &[(&str, &str, u32)] = &[
    ("yellow!40", "Yellow", 0xfde68a),
    ("cyan!30", "Blue", 0xbae6fd),
    ("red!25", "Red", 0xfecaca),
    ("green!30", "Green", 0xbbf7d0),
    ("gray!30", "Grey", 0xd4d4d8),
];
const WIDTHS: &[(&str, &str)] = &[
    ("thin", "Thin"),
    ("thick", "Thick"),
    ("very thick", "Bold"),
    ("ultra thick", "Heavy"),
];

impl Doc {
    /// The TeX of the picture in the TikZ editor, as the editor's text has it now.
    fn picture_text(&self, cx: &App) -> Option<String> {
        match &self.active {
            Some(a) if a.panel == Some(Panel::Source) => {
                let t = self.input.read(cx).value().to_string();
                t.contains("\\begin{tikzpicture}").then_some(t)
            }
            _ => None,
        }
    }

    /// The points of the picture in the TikZ editor, as the editor's text has them now.
    fn picture_points(&self, cx: &App) -> Vec<tikz::Point> {
        self.picture_text(cx)
            .map(|t| tikz::points(&t))
            .unwrap_or_default()
    }

    /// The editor's text made `new`, as one undo step; the picture retypesets as for typing.
    fn tikz_set(&mut self, new: String, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_box(true, cx);
        self.checkpoint();
        self.input.update(cx, |s, cx| {
            let at = s.cursor();
            s.set_value(new, window, cx);
            let c = at.min(s.value().len());
            s.set_selected_range(c..c, cx);
        });
        self.sync_box(true, cx);
        cx.notify();
    }

    /// Statement `stmt` drawn as `edit` says.
    pub(super) fn tikz_style(
        &mut self,
        edit: tikz::Style<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(text), Some(ix)) = (self.picture_text(cx), self.tikz_ui.stmt) else {
            return;
        };
        let Some(st) = tikz::statements(&text).into_iter().nth(ix) else {
            return;
        };
        if let Some(new) = tikz::styled(&text, &st, &edit) {
            self.tikz_set(new, window, cx);
        }
    }

    /// A shape of kind `shape` is to be placed with the next click on the canvas.
    pub(super) fn tikz_place_next(&mut self, shape: tikz::Shape, cx: &mut Context<Self>) {
        self.tikz_ui.placing = (self.tikz_ui.placing != Some(shape)).then_some(shape);
        cx.notify();
    }

    /// A new shape at (`x`, `y`), picked as the statement to style.
    pub(super) fn tikz_add(
        &mut self,
        shape: tikz::Shape,
        x: f64,
        y: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = self.picture_text(cx) else {
            return;
        };
        let Some(new) = tikz::added(&text, shape, tikz::snap(x, STEP), tikz::snap(y, STEP)) else {
            return;
        };
        self.tikz_set(new.clone(), window, cx);
        self.tikz_ui.placing = None;
        self.tikz_ui.stmt = tikz::statements(&new).len().checked_sub(1);
        if shape == tikz::Shape::Text {
            let labels = tikz::labels(&new);
            if let Some(ix) = labels.len().checked_sub(1) {
                self.tikz_label(ix, window, cx);
            }
        }
    }

    /// Label `ix` taken for editing: its text in an input under the canvas.
    pub(super) fn tikz_label(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.picture_text(cx) else {
            return;
        };
        let Some(l) = tikz::labels(&text).into_iter().nth(ix) else {
            return;
        };
        self.tikz_ui.stmt = tikz::statements(&text)
            .iter()
            .position(|s| s.range.contains(&l.text.start));
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |s, cx| {
            s.set_value(text[l.text.clone()].to_string(), window, cx);
            s.focus(window, cx);
        });
        let sub = cx.subscribe_in(
            &input,
            window,
            move |this, state, ev: &InputEvent, window, cx| {
                if !matches!(ev, InputEvent::Change) {
                    return;
                }
                let (Some(text), value) =
                    (this.picture_text(cx), state.read(cx).value().to_string())
                else {
                    return;
                };
                // the picture's text, with this label's changed (a `}` of its own would end it)
                if value.contains(['{', '}']) && !document::caret::balanced(&value) {
                    return;
                }
                if let Some(new) = tikz::set_label(&text, ix, &value)
                    && new != text
                {
                    this.input.update(cx, |s, cx| {
                        let at = s.cursor();
                        s.set_value(new, window, cx);
                        let c = at.min(s.value().len());
                        s.set_selected_range(c..c, cx);
                    });
                    this.sync_box(true, cx);
                }
            },
        );
        self.sync_box(true, cx);
        self.checkpoint();
        self.tikz_ui.label = Some(LabelEdit {
            ix,
            input,
            _sub: sub,
        });
        cx.notify();
    }

    #[cfg(test)]
    pub(super) fn tikz_ui_label_input(&self) -> Option<Entity<InputState>> {
        self.tikz_ui.label.as_ref().map(|l| l.input.clone())
    }

    #[cfg(test)]
    pub(super) fn tikz_pick_circle(&mut self, ix: usize, from: f32, cx: &mut Context<Self>) {
        self.radius_down(ix, from, 600.0, cx);
    }

    /// A circle's radius handle taken with the pointer at `from` (canvas x).
    fn radius_down(&mut self, ix: usize, from: f32, width: f32, cx: &mut Context<Self>) {
        let Some(text) = self.picture_text(cx) else {
            return;
        };
        let pts = tikz::points(&text);
        let Some(c) = tikz::circles(&text).into_iter().nth(ix) else {
            return;
        };
        self.sync_box(true, cx);
        self.checkpoint();
        self.tikz_ui.stmt = tikz::stmt_of(&text, c.center);
        self.tikz_ui.radius = Some(RadiusDrag {
            ix,
            from,
            r0: c.r,
            view: View::fit(&pts, width, CANVAS_H),
        });
        cx.notify();
    }

    /// The pointer at canvas x `x` with a radius held.
    pub(super) fn radius_to(&mut self, x: f32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.tikz_ui.radius.as_ref() else {
            return;
        };
        let r = tikz::snap(d.r0 + f64::from(x - d.from) / d.view.scale, STEP).max(STEP);
        let Some(text) = self.picture_text(cx) else {
            return;
        };
        if let Some(new) = tikz::set_radius(&text, d.ix, r)
            && new != text
        {
            self.input.update(cx, |s, cx| {
                let at = s.cursor();
                s.set_value(new, window, cx);
                let c = at.min(s.value().len());
                s.set_selected_range(c..c, cx);
            });
            self.sync_box(true, cx);
            cx.notify();
        }
    }

    /// The canvas of dots, `width` wide, under the TikZ editor, with the tools for shapes, labels
    /// and styles; none for a source panel that holds no picture.
    pub(super) fn render_points(&self, width: f32, cx: &mut Context<Self>) -> Option<AnyElement> {
        let text = self.picture_text(cx)?;
        let pts = tikz::points(&text);
        let theme = cx.theme().clone();
        let ui = &self.tikz_ui;
        let view = match (&self.pointing, &ui.radius) {
            (Some(p), _) => p.view,
            (_, Some(r)) => r.view,
            _ => View::fit(&pts, width, CANVAS_H),
        };
        let held = self.pointing.as_ref();
        let stmts = tikz::statements(&text);
        let sel = ui.stmt.and_then(|i| stmts.get(i).cloned());
        let in_sel = |at: usize| sel.as_ref().is_some_and(|s| s.range.contains(&at));
        let dots: Vec<_> = pts
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let (x, y) = view.at(p.x, p.y);
                let on = held.is_some_and(|h| h.ix == i);
                let size = if on { 12.0 } else { 10.0 };
                let picked = in_sel(p.x_at.start);
                div()
                    .id(("tikz-point", i))
                    .role(Role::Button)
                    .aria_label(format!("Point ({}, {})", tikz::show(p.x), tikz::show(p.y)))
                    .absolute()
                    .left(px(x - size / 2.0))
                    .top(px(y - size / 2.0))
                    .size(px(size))
                    .rounded_full()
                    .bg(if picked { theme.ring } else { theme.primary })
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
            })
            .collect();
        // rectangles outlined between their corners, circles as rings with a handle for the radius
        let outline = |rect: [f32; 4], round: bool, picked: bool| {
            div()
                .absolute()
                .left(px(rect[0]))
                .top(px(rect[1]))
                .w(px(rect[2]))
                .h(px(rect[3]))
                .border_1()
                .border_color(if picked {
                    theme.ring
                } else {
                    theme.muted_foreground
                })
                .when(round, |d| d.rounded_full())
        };
        let rects: Vec<_> = tikz::rects(&text)
            .into_iter()
            .map(|r| {
                let (a, b) = (&pts[r.a], &pts[r.b]);
                let (x0, y0) = view.at(a.x.min(b.x), a.y.max(b.y));
                let (x1, y1) = view.at(a.x.max(b.x), a.y.min(b.y));
                outline([x0, y0, x1 - x0, y1 - y0], false, in_sel(a.x_at.start))
            })
            .collect();
        let rings: Vec<_> = tikz::circles(&text)
            .into_iter()
            .enumerate()
            .flat_map(|(i, c)| {
                let p = &pts[c.center];
                let (cx0, cy0) = view.at(p.x, p.y);
                let r = (c.r * view.scale) as f32;
                let ring = outline(
                    [cx0 - r, cy0 - r, 2.0 * r, 2.0 * r],
                    true,
                    in_sel(p.x_at.start),
                );
                let handle = div()
                    .id(("tikz-radius", i))
                    .role(Role::Button)
                    .aria_label("Circle radius")
                    .absolute()
                    .left(px(cx0 + r - 4.0))
                    .top(px(cy0 - 4.0))
                    .size(px(8.0))
                    .bg(theme.ring)
                    .border_1()
                    .border_color(theme.popover)
                    .cursor_ew_resize()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let x = f32::from(ev.position.x);
                            this.radius_down(i, x, width, cx);
                        }),
                    );
                [ring.into_any_element(), handle.into_any_element()]
            })
            .collect();
        let labels: Vec<_> = tikz::labels(&text)
            .into_iter()
            .enumerate()
            .filter_map(|(i, l)| {
                let p = pts.get(l.anchor?)?;
                let (x, y) = view.at(p.x, p.y);
                let shown: String = text[l.text.clone()].chars().take(16).collect();
                let editing = ui.label.as_ref().is_some_and(|e| e.ix == i);
                Some(
                    div()
                        .id(("tikz-label", i))
                        .role(Role::Button)
                        .aria_label(format!("Label {shown}"))
                        .absolute()
                        .left(px((x + 6.0).min(width - 40.0)))
                        .top(px((y - 22.0).max(0.0)))
                        .px_1()
                        .rounded_sm()
                        .text_xs()
                        .border_1()
                        .border_color(if editing { theme.ring } else { theme.border })
                        .bg(theme.popover)
                        .cursor_text()
                        .child(if shown.is_empty() {
                            "…".to_string()
                        } else {
                            shown
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.tikz_label(i, window, cx);
                            }),
                        ),
                )
            })
            .collect();
        let drag_label = held.map(|h| {
            let (x, y) = view.at(h.now.0, h.now.1);
            div()
                .absolute()
                .left(px((x + 10.0).min(width - 70.0)))
                .top(px((y + 8.0).min(CANVAS_H - 18.0)))
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format!(
                    "({}, {})",
                    tikz::show(h.now.0),
                    tikz::show(h.now.1)
                ))
        });
        let placing = ui.placing;
        let bounds = ui.bounds.clone();
        let canvas_el = div()
            .id("tikz-points")
            .relative()
            .w(px(width))
            .h(px(CANVAS_H))
            .overflow_hidden()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.secondary)
            .when(placing.is_some(), |d| d.cursor_crosshair())
            .child(
                canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                    let Some(shape) = this.tikz_ui.placing else {
                        this.tikz_ui.stmt = None;
                        this.tikz_ui.label = None;
                        cx.notify();
                        return;
                    };
                    let b = this.tikz_ui.bounds.get();
                    let text = this.picture_text(cx).unwrap_or_default();
                    let view = View::fit(&tikz::points(&text), width, CANVAS_H);
                    let (x, y) = view.place(
                        f32::from(ev.position.x - b.origin.x),
                        f32::from(ev.position.y - b.origin.y),
                    );
                    this.tikz_add(shape, x, y, window, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, window, cx| {
                let left = ev.pressed_button == Some(MouseButton::Left);
                if this.tikz_ui.radius.is_some() {
                    if left {
                        this.radius_to(f32::from(ev.position.x), window, cx);
                    } else {
                        this.tikz_ui.radius = None;
                    }
                } else if this.pointing.is_some() {
                    // a release off the canvas was not seen: the drag ends with the next move
                    if left {
                        this.point_to(ev.position, window, cx);
                    } else {
                        this.point_end(cx);
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.tikz_ui.radius = None;
                    this.point_end(cx);
                }),
            )
            .children(rects)
            .children(rings)
            .children(dots)
            .children(labels)
            .children(drag_label);
        // the tool rows: a button of them is filled when what it sets is what is picked
        let chip = |id: (&'static str, usize), label: &'static str, on: bool| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(label)
                .px_2()
                .h(px(22.0))
                .flex()
                .items_center()
                .rounded_md()
                .text_xs()
                .cursor_pointer()
                .when(on, |d| d.bg(theme.secondary))
                .hover(|s| s.bg(theme.secondary))
                .child(label)
        };
        let shapes: Vec<_> = [
            (tikz::Shape::Line, "+ Line"),
            (tikz::Shape::Rectangle, "+ Rectangle"),
            (tikz::Shape::Circle, "+ Circle"),
            (tikz::Shape::Text, "+ Text"),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (shape, name))| {
            chip(("tikz-add", i), name, placing == Some(shape)).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.tikz_place_next(shape, cx);
                }),
            )
        })
        .collect();
        let look = sel.as_ref().map(|s| tikz::look(&text, s));
        let swatch = |id: (&'static str, usize), name: &'static str, rgb: u32, on: bool| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(name)
                .size(px(16.0))
                .flex_none()
                .rounded_sm()
                .border_1()
                .border_color(if on { theme.ring } else { theme.border })
                .bg(gpui::rgb(rgb))
                .cursor_pointer()
        };
        let style_row = look.map(|look| {
            let strokes: Vec<_> = STROKES
                .iter()
                .enumerate()
                .map(|(i, (tex, name, rgb))| {
                    swatch(
                        ("tikz-stroke", i),
                        name,
                        *rgb,
                        look.stroke.as_deref() == Some(*tex),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.tikz_style(tikz::Style::Stroke(Some(tex)), window, cx);
                        }),
                    )
                })
                .collect();
            let fills: Vec<_> = FILLS
                .iter()
                .enumerate()
                .map(|(i, (tex, name, rgb))| {
                    swatch(
                        ("tikz-fill", i),
                        name,
                        *rgb,
                        look.fill.as_deref() == Some(*tex),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.tikz_style(tikz::Style::Fill(Some(tex)), window, cx);
                        }),
                    )
                })
                .collect();
            let widths: Vec<_> = WIDTHS
                .iter()
                .enumerate()
                .map(|(i, (tex, name))| {
                    chip(("tikz-width", i), name, look.width.as_deref() == Some(*tex))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.tikz_style(tikz::Style::Width(Some(tex)), window, cx);
                            }),
                        )
                })
                .collect();
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Line"),
                )
                .children(strokes)
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Fill"),
                )
                .children(fills)
                .child(chip(("tikz-nofill", 0), "No fill", false).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.tikz_style(tikz::Style::Fill(None), window, cx);
                    }),
                ))
                .children(widths)
                .child(
                    chip(("tikz-dashed", 0), "Dashed", look.dashed).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.tikz_style(tikz::Style::Dashed, window, cx);
                        }),
                    ),
                )
                .child(chip(("tikz-arrow", 0), "Arrow", look.arrow).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.tikz_style(tikz::Style::Arrow, window, cx);
                    }),
                ))
        });
        let label_row = ui.label.as_ref().map(|l| {
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Label"),
                )
                .child(div().flex_1().child(Input::new(&l.input).small()))
        });
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(canvas_el)
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_0p5()
                        .children(shapes)
                        .child(div().text_xs().text_color(theme.muted_foreground).child(
                            if placing.is_some() {
                                "Click the canvas to place it"
                            } else {
                                "Drag a dot, a circle's edge; click a label to edit it"
                            },
                        )),
                )
                .children(style_row)
                .children(label_row)
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
        self.tikz_ui.stmt = self.picture_text(cx).and_then(|t| tikz::stmt_of(&t, ix));
        self.tikz_ui.label = None;
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
