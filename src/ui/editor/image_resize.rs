// Resizing a figure's picture by dragging: a handle at the corner of the selected figure's image.
// The page knows the image's line, not its own box, so the box is worked out from the width the
// source gives it (a percent of the line, centred when the figure is). Nothing is typeset while
// dragging: a ghost shows the new size, and the release writes it as the figure's width.
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
}

impl ImageBox {
    /// The box at `percent` of the line wide, its top where it is and its shape kept.
    pub(super) fn at(&self, percent: f64) -> [f64; 4] {
        let (x, line) = self.line;
        let w = line * percent / 100.0;
        let h = (self.rect[3] - self.rect[1]) * percent / self.percent;
        let left = if self.centered {
            x + (line - w) / 2.0
        } else {
            x
        };
        [left, self.rect[1], left + w, self.rect[1] + h]
    }
}

/// A drag of the handle under way.
pub(super) struct Resizing {
    img: ImageBox,
    percent: f64,
}

/// The smallest a picture is dragged to, in percent of the line.
const LEAST: f64 = 5.0;

impl Resizing {
    #[cfg(test)]
    pub(super) fn new(img: ImageBox) -> Self {
        Self {
            percent: img.percent,
            img,
        }
    }
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
        })
    }

    /// The handle on page `n` (and, while dragging, the new size outlined).
    pub(super) fn render_image_handle(&self, n: u32, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (img, percent) = match &self.resizing {
            Some(r) => (r.img.clone(), r.percent),
            None => {
                let img = self.image_box()?;
                let p = img.percent;
                (img, p)
            }
        };
        if img.page != n {
            return None;
        }
        let [l, t, r, b] = img.at(percent).map(|v| page_px(v, self.zoom));
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
                            .child(format!("{percent:.0}%")),
                    )
                })
                .child(
                    div()
                        .id("image-handle")
                        .role(Role::Button)
                        .aria_label("Resize the picture")
                        .absolute()
                        .left(px(r - size / 2.0))
                        .top(px(b - size / 2.0))
                        .size(px(size))
                        .bg(accent)
                        .border_1()
                        .border_color(gpui::white())
                        .cursor_nwse_resize()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.resizing = Some(Resizing {
                                    percent: start.percent,
                                    img: start.clone(),
                                });
                                cx.notify();
                            }),
                        ),
                )
                .into_any_element(),
        )
    }

    /// The pointer is at `pos` on page `page` during a drag: the width it asks for, to the percent.
    pub(super) fn resize_to(&mut self, page: u32, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(img) = self.resizing.as_ref().map(|r| r.img.clone()) else {
            return;
        };
        let Some((x, _, _)) = self.hit(page, pos).filter(|_| page == img.page) else {
            return;
        };
        let (left, line) = img.line;
        let w = if img.centered {
            2.0 * (x - (left + line / 2.0))
        } else {
            x - left
        };
        let percent = (w / line * 100.0).round().clamp(LEAST, 100.0);
        if let Some(r) = self.resizing.as_mut()
            && r.percent != percent
        {
            r.percent = percent;
            cx.notify();
        }
    }

    /// The drag ended: the figure gets the width, as one undo step.
    pub(super) fn resize_end(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.resizing.take() else {
            return;
        };
        if r.percent != r.img.percent {
            self.set_element_field("width", &format!("{}%", r.percent), cx);
        }
        cx.notify();
    }
}
