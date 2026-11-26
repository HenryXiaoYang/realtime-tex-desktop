// Resizing a figure's picture by dragging: eight handles on the selected figure's image. A corner
// scales it, its shape kept; a side stretches it, the width alone (left, right) or the height
// alone (top, bottom). The page knows the image's line, not its own box, so the box is worked
// out from the width the source gives it (a percent of the line, centred when the figure is).
// Nothing is typeset while dragging: a ghost shows the new size, and the release writes it as
// the figure's width and, where the shape changed, its height.
use super::*;
use crate::document::element::{self, Kind};

/// The selected figure's picture, where the page has it.
#[derive(Clone)]
pub(super) struct ImageBox {
    pub(super) page: u32,
    /// The picture's box (sp): left, top, right, bottom.
    pub(super) rect: [f64; 4],
    /// The line it sits in: where it starts and how wide it is (sp).
    line: (f64, f64),
    centered: bool,
    /// Its width now, in percent of the line.
    pub(super) percent: f64,
    /// The source gives its height too (a stretched picture).
    pub(super) height_set: bool,
}

impl ImageBox {
    /// The box at `percent` of the line wide, its top where it is and its shape kept.
    #[cfg(test)]
    pub(super) fn at(&self, percent: f64) -> [f64; 4] {
        self.sized(percent, self.height() * percent / self.percent)
    }

    /// The box at `percent` of the line wide and `height` (sp) tall, its top where it is.
    pub(super) fn sized(&self, percent: f64, height: f64) -> [f64; 4] {
        let (x, line) = self.line;
        let w = line * percent / 100.0;
        let left = if self.centered {
            x + (line - w) / 2.0
        } else {
            x
        };
        [left, self.rect[1], left + w, self.rect[1] + height]
    }

    /// Its height now (sp).
    pub(super) fn height(&self) -> f64 {
        self.rect[3] - self.rect[1]
    }
}

/// A centimetre, in sp (65536 sp to the TeX point, 72.27 points to the inch).
const CM: f64 = 65536.0 * 72.27 / 2.54;
/// The least and the most a picture is stretched to in height.
const LEAST_HEIGHT: f64 = 0.2 * CM;
const MOST_HEIGHT: f64 = 40.0 * CM;

/// `height` (sp) as a length for the source: `3.2cm`.
fn centimetres(height: f64) -> String {
    let v = format!("{:.2}", height / CM);
    format!("{}cm", v.trim_end_matches('0').trim_end_matches('.'))
}

/// Which of the eight handles is held: the side of the picture it is on, -1 left or top, 1 right
/// or bottom, 0 in the middle of an edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Grip {
    dx: i8,
    dy: i8,
}

impl Grip {
    /// The corner at the bottom right: what the picture had alone before the other seven.
    pub(super) const SE: Grip = Grip { dx: 1, dy: 1 };
    #[cfg(test)]
    pub(super) fn new(dx: i8, dy: i8) -> Grip {
        Grip { dx, dy }
    }
    const ALL: [Grip; 8] = [
        Grip { dx: -1, dy: -1 },
        Grip { dx: 0, dy: -1 },
        Grip { dx: 1, dy: -1 },
        Grip { dx: 1, dy: 0 },
        Self::SE,
        Grip { dx: 0, dy: 1 },
        Grip { dx: -1, dy: 1 },
        Grip { dx: -1, dy: 0 },
    ];

    /// Where on `rect` (left, top, right, bottom) the handle sits.
    fn at(self, rect: [f32; 4]) -> (f32, f32) {
        let [l, t, r, b] = rect;
        let at = |d: i8, lo: f32, hi: f32| match d {
            -1 => lo,
            1 => hi,
            _ => (lo + hi) / 2.0,
        };
        (at(self.dx, l, r), at(self.dy, t, b))
    }
}

/// A drag of a handle under way.
pub(super) struct Resizing {
    img: ImageBox,
    percent: f64,
    /// The height it asks for (sp).
    height: f64,
    grip: Grip,
}

/// The smallest a picture is dragged to, in percent of the line.
const LEAST: f64 = 5.0;

impl Resizing {
    #[cfg(test)]
    pub(super) fn new(img: ImageBox) -> Self {
        Self::held(img, Grip::SE)
    }

    #[cfg(test)]
    pub(super) fn held(img: ImageBox, grip: Grip) -> Self {
        Self {
            percent: img.percent,
            height: img.height(),
            img,
            grip,
        }
    }
}

fn grip_id(g: Grip) -> usize {
    ((g.dx + 1) * 3 + g.dy + 1) as usize
}

impl Doc {
    /// The picture of the one figure selected, when its width is all that sizes it.
    pub(super) fn image_box(&self) -> Option<ImageBox> {
        if self.active.is_some() {
            return None;
        }
        let (lo, hi) = self.selection()?;
        let b = self.blocks.get(lo).filter(|_| lo == hi)?;
        let el = self.element();
        if el.kind != Kind::Figure {
            return None;
        }
        let percent = element::image_width_percent(&self.source, &el)?;
        let height_set = element::fields(&self.source, &el)
            .iter()
            .any(|f| f.key == "height" && !f.value.trim().is_empty());
        // its line is the tallest (a caption is one text line)
        let height = |r: &document::PlacedRow| r.bbox[3] - r.bbox[1];
        let row = b
            .rows
            .iter()
            .max_by(|a, b| height(a).total_cmp(&height(b)))?;
        let [x, top, right, bottom] = row.bbox;
        let line = right - x;
        let text = &self.source[b.range.clone()];
        let centered = text.contains("\\centering") || text.contains("\\begin{center}");
        let w = line * percent / 100.0;
        let left = if centered { x + (line - w) / 2.0 } else { x };
        Some(ImageBox {
            page: row.page,
            rect: [left, top, left + w, bottom],
            line: (x, line),
            centered,
            percent,
            height_set,
        })
    }

    /// The handle on page `n` (and, while dragging, the new size outlined).
    pub(super) fn render_image_handle(&self, n: u32, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (img, percent, height) = match &self.resizing {
            Some(r) => (r.img.clone(), r.percent, r.height),
            None => {
                let img = self.image_box()?;
                let (p, h) = (img.percent, img.height());
                (img, p, h)
            }
        };
        if img.page != n {
            return None;
        }
        let [l, t, r, b] = img.sized(percent, height).map(|v| page_px(v, self.zoom));
        let accent = palette::marks(self.dark_pages(cx), cx).accent;
        let size = 10.0;
        let dragging = self.resizing.is_some();
        let start = img.clone();
        Some(
            div()
                .absolute()
                .inset_0()
                .when(dragging, |d| {
                    d.child(
                        div()
                            .absolute()
                            .left(px(l))
                            .top(px(t))
                            .w(px(r - l))
                            .h(px(b - t))
                            .border_1()
                            .border_dashed()
                            .border_color(accent),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(l))
                            .top(px(b + 4.0))
                            .text_xs()
                            .text_color(accent)
                            .child(format!("{percent:.0}% × {:.1} cm", height / CM)),
                    )
                })
                .children(Grip::ALL.into_iter().map(|grip| {
                    let (x, y) = grip.at([l, t, r, b]);
                    let start = start.clone();
                    let d = div()
                        .id(("image-handle", grip_id(grip)))
                        .role(Role::Button)
                        .aria_label("Resize the picture")
                        .absolute()
                        .left(px(x - size / 2.0))
                        .top(px(y - size / 2.0))
                        .size(px(size))
                        .bg(accent)
                        .border_1()
                        .border_color(gpui::white());
                    let d = match (grip.dx, grip.dy) {
                        (0, _) => d.cursor_ns_resize(),
                        (_, 0) => d.cursor_ew_resize(),
                        (a, b) if a == b => d.cursor_nwse_resize(),
                        _ => d.cursor_nesw_resize(),
                    };
                    d.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.resizing = Some(Resizing {
                                percent: start.percent,
                                height: start.height(),
                                img: start.clone(),
                                grip,
                            });
                            cx.notify();
                        }),
                    )
                }))
                .into_any_element(),
        )
    }

    /// The pointer is at `pos` on page `page` during a drag: the size it asks for. A corner asks
    /// for a width (the shape kept), a side for a width or a height alone.
    pub(super) fn resize_to(&mut self, page: u32, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(img) = self.resizing.as_ref().map(|r| r.img.clone()) else {
            return;
        };
        let Some((x, y, _)) = self.hit(page, pos).filter(|_| page == img.page) else {
            return;
        };
        let grip = self.resizing.as_ref().map_or(Grip::SE, |r| r.grip);
        let (left, line) = img.line;
        let [_, top, right, bottom] = img.rect;
        // the width the pointer asks for, in percent of the line
        let wide = |x: f64| {
            let w = match (grip.dx, img.centered) {
                (1, true) => 2.0 * (x - (left + line / 2.0)),
                (1, false) => x - left,
                (_, true) => 2.0 * ((left + line / 2.0) - x),
                (_, false) => right - x,
            };
            w / line * 100.0
        };
        let tall = |y: f64| if grip.dy == 1 { y - top } else { bottom - y };
        let h0 = img.height();
        let (percent, height) = match (grip.dx, grip.dy) {
            // the top or bottom side: the height alone
            (0, _) => (img.percent, tall(y).clamp(LEAST_HEIGHT, MOST_HEIGHT)),
            // the left or right side: the width alone
            (_, 0) => (wide(x).round().clamp(LEAST, 100.0), h0),
            // a corner follows whichever of the two the pointer has moved further, the shape kept
            _ => {
                let (a, b) = (wide(x), img.percent * tall(y) / h0);
                let far = |p: f64| (p - img.percent).abs();
                let want = if far(a) >= far(b) { a } else { b };
                let p = want.round().clamp(LEAST, 100.0);
                (p, h0 * p / img.percent)
            }
        };
        if let Some(r) = self.resizing.as_mut()
            && (r.percent != percent || r.height != height)
        {
            r.percent = percent;
            r.height = height;
            cx.notify();
        }
    }

    /// The drag ended: the figure gets the size, as one undo step. A corner writes the width (and
    /// the height where the source already gave one); a side that changes the shape writes what
    /// it changed, the width with the height held where it was.
    pub(super) fn resize_end(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.resizing.take() else {
            return;
        };
        let (wider, taller) = (
            r.percent != r.img.percent,
            (r.height - r.img.height()).abs() > CM / 100.0,
        );
        let corner = r.grip.dx != 0 && r.grip.dy != 0;
        let mut fields: Vec<(&str, String)> = vec![];
        if wider {
            fields.push(("width", format!("{}%", r.percent)));
        }
        let height = if corner {
            r.img.height_set && (wider || taller)
        } else if r.grip.dy == 0 {
            // the width alone: the height is held where it is, written unless it already is
            wider && !r.img.height_set
        } else {
            taller
        };
        if height {
            fields.push(("height", centimetres(r.height)));
        }
        if !fields.is_empty() {
            self.set_element_fields(&fields, cx);
        }
        cx.notify();
    }
}
