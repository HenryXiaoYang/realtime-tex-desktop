// Sizing a table by dragging its lines on the page: with a table selected, each column's right
// line and each row's bottom line is a handle. A column's line sets its width (`p{…}`), a row's
// the room under it (`\\[…pt]`). The source follows the pointer as it moves, so the table is
// typeset as it is sized; the drag is one undo step, made at its first change.
use super::*;
use crate::document::table;
use crate::document::table_lines::{self, Lines};

const PT: f64 = 65536.0;
/// The least and most a dragged column is wide, in points.
const COL_LEAST: f64 = 10.0;
const COL_MOST: f64 = 500.0;
/// The most room a dragged row keeps under it, in points.
const ROW_MOST: f64 = 60.0;
/// How wide a handle is to grab, in pixels.
const GRAB: f32 = 9.0;

/// The line held.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Edge {
    Col(usize),
    Row(usize),
}

/// A line of the selected table held, being dragged.
pub(super) struct LineDrag {
    page: u32,
    edge: Edge,
    /// The lines when it was taken: they are drawn while the page typesets the new size.
    lines: Lines,
    /// Where the pointer was (sp) and the size then (pt).
    from: f64,
    size: f64,
    /// The size last written (pt): the source is changed when it differs.
    now: f64,
    /// A change has been made: the undo step is taken.
    began: bool,
}

impl Doc {
    /// The selected table's page and lines, when one block is selected and holds a table.
    pub(super) fn table_lines(&self) -> Option<(u32, Lines)> {
        if self.active.is_some() {
            return None;
        }
        let (lo, hi) = self.selection()?;
        if lo != hi {
            return None;
        }
        let b = self.blocks.get(lo)?;
        let table = table::find(&self.source, b.range.clone())?;
        self.pages_of(lo).find_map(|page| {
            let map = self.map_of(lo, page)?;
            let lines = table_lines::lines(&table, &self.source, &map)?;
            Some((page, lines))
        })
    }

    /// The handles on page `n`, and while one is dragged its ghost at the size it asks for.
    pub(super) fn render_table_handles(
        &self,
        n: u32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (page, lines, held) = match &self.line_drag {
            Some(d) => (d.page, d.lines.clone(), Some(d)),
            None => {
                let (page, lines) = self.table_lines()?;
                (page, lines, None)
            }
        };
        if page != n {
            return None;
        }
        let accent = palette::marks(self.dark_pages(cx), cx).accent;
        let px_of = |v: f64| page_px(v, self.zoom);
        let [bl, bt, br, bb] = lines.bbox.map(px_of);
        let (top, height) = (bt - 6.0, bb - bt + 12.0);
        let (left, width) = (bl - 6.0, br - bl + 12.0);
        let mut out = div().absolute().inset_0();
        for c in &lines.cols {
            let edge = Edge::Col(c.col);
            // the held line stands where the drag asks for it
            let x = match held {
                Some(d) if d.edge == edge => c.x + (d.now - d.size) * PT,
                _ => c.x,
            };
            let x = px_of(x);
            let on = held.is_some_and(|d| d.edge == edge);
            let size = c.width;
            out = out.child(
                div()
                    .id(("table-col-line", c.col))
                    .role(Role::Button)
                    .aria_label("Resize the column")
                    .absolute()
                    .left(px(x - GRAB / 2.0))
                    .top(px(top))
                    .w(px(GRAB))
                    .h(px(height))
                    .cursor_ew_resize()
                    .hover(|s| s.bg(accent.alpha(0.25)))
                    .child(
                        div()
                            .absolute()
                            .left(px(GRAB / 2.0 - 1.0))
                            .w(px(2.0))
                            .h_full()
                            .bg(accent.alpha(if on { 1.0 } else { 0.45 })),
                    )
                    .when(on, |d| {
                        d.child(
                            div()
                                .absolute()
                                .left(px(GRAB + 2.0))
                                .top_0()
                                .text_xs()
                                .text_color(accent)
                                .child(format!("{:.1} pt", held.map_or(size, |h| h.now))),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let Some((from, _, _)) = this.hit(page, ev.position) else {
                                return;
                            };
                            this.hold_line(page, edge, from, size, cx);
                        }),
                    ),
            );
        }
        for r in &lines.rows {
            let edge = Edge::Row(r.row);
            let y = match held {
                Some(d) if d.edge == edge => r.y + (d.now - d.size) * PT,
                _ => r.y,
            };
            let y = px_of(y);
            let on = held.is_some_and(|d| d.edge == edge);
            let size = r.space;
            out = out.child(
                div()
                    .id(("table-row-line", r.row))
                    .role(Role::Button)
                    .aria_label("Resize the row")
                    .absolute()
                    .left(px(left))
                    .top(px(y - GRAB / 2.0))
                    .w(px(width))
                    .h(px(GRAB))
                    .cursor_ns_resize()
                    .hover(|s| s.bg(accent.alpha(0.25)))
                    .child(
                        div()
                            .absolute()
                            .top(px(GRAB / 2.0 - 1.0))
                            .h(px(2.0))
                            .w_full()
                            .bg(accent.alpha(if on { 1.0 } else { 0.45 })),
                    )
                    .when(on, |d| {
                        d.child(
                            div()
                                .absolute()
                                .right_0()
                                .top(px(GRAB + 2.0))
                                .text_xs()
                                .text_color(accent)
                                .child(format!("{:.1} pt", held.map_or(size, |h| h.now))),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let Some((_, from, _)) = this.hit(page, ev.position) else {
                                return;
                            };
                            this.hold_line(page, edge, from, size, cx);
                        }),
                    ),
            );
        }
        Some(out.into_any_element())
    }

    /// A line taken at `from` (sp) when its size is `size` (pt).
    pub(super) fn hold_line(
        &mut self,
        page: u32,
        edge: Edge,
        from: f64,
        size: f64,
        cx: &mut Context<Self>,
    ) {
        let Some((_, lines)) = self.table_lines() else {
            return;
        };
        self.line_drag = Some(LineDrag {
            page,
            edge,
            lines,
            from,
            size,
            now: size,
            began: false,
        });
        cx.notify();
    }

    /// The pointer is at `pos` on page `page` with a line held: the size it asks for is written.
    pub(super) fn line_drag_to(&mut self, page: u32, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((edge, from, size, now, began)) = self
            .line_drag
            .as_ref()
            .filter(|d| d.page == page)
            .map(|d| (d.edge, d.from, d.size, d.now, d.began))
        else {
            return;
        };
        let Some((x, y, _)) = self.hit(page, pos) else {
            return;
        };
        let want = match edge {
            Edge::Col(_) => {
                (((size + (x - from) / PT).clamp(COL_LEAST, COL_MOST)) * 10.0).round() / 10.0
            }
            Edge::Row(_) => (((size + (y - from) / PT).clamp(0.0, ROW_MOST)) * 2.0).round() / 2.0,
        };
        if want == now {
            return;
        }
        let Some(b) = self.selection().and_then(|(lo, _)| self.blocks.get(lo)) else {
            return;
        };
        let Some(t) = table::find(&self.source, b.range.clone()) else {
            return;
        };
        let change = match edge {
            Edge::Col(c) => t.set_col_width(&self.source, c, want),
            Edge::Row(r) => t.set_row_space(&self.source, r, want).map(|c| (c, vec![])),
        };
        let Some((ch, packages)) = change else {
            return;
        };
        if !began {
            self.checkpoint();
        }
        if self.source[ch.range.clone()] != ch.text {
            self.replace(ch.range, &ch.text, cx);
            self.add_packages(&packages, cx);
        }
        if let Some(d) = self.line_drag.as_mut() {
            d.now = want;
            d.began = true;
        }
        cx.notify();
    }

    pub(super) fn line_drag_end(&mut self, cx: &mut Context<Self>) {
        if self.line_drag.take().is_some() {
            cx.notify();
        }
    }
}
