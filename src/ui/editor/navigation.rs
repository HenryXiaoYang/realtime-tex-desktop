// Pointer and view: hit-testing the pages, clicks, scrolling to a source offset, zoom.
use super::*;

impl Doc {
    /// Where `pos` falls on `page`, in sp, and the block under it (within a few points).
    pub(super) fn hit(&self, page: u32, pos: Point<Pixels>) -> Option<(f64, f64, Option<usize>)> {
        let bounds = self.page_bounds.take();
        let origin = bounds
            .iter()
            .find(|(n, _)| *n == page)
            .map(|(_, b)| b.origin);
        self.page_bounds.set(bounds);
        let origin = origin?;
        let mut y = f32::from(pos.y - origin.y);
        // below an open source box, the page is shifted down by the box's extra height
        if let (Some(bb), Some((p, cut))) = (self.box_bounds.get(), self.box_cut.get())
            && p == page
            && self.active.is_some()
            && pos.y > bb.origin.y
        {
            y -= (f32::from(bb.size.height) - cut).max(0.0);
        }
        let to_sp = |v: f32| v as f64 / self.zoom as f64 * SP_PER_BP;
        let (x, y) = (to_sp(f32::from(pos.x - origin.x)), to_sp(y));
        let mut best: Option<(f64, usize)> = None;
        for (bi, b) in self.blocks.iter().enumerate() {
            for r in b.rows.iter().filter(|r| r.page == page) {
                let dy = if y < r.bbox[1] {
                    r.bbox[1] - y
                } else if y > r.bbox[3] {
                    y - r.bbox[3]
                } else {
                    0.0
                };
                if best.is_none_or(|(d, _)| dy < d) {
                    best = Some((dy, bi));
                }
            }
        }
        // between the lines of a block: that block; in wider empty space: none
        let bi = best.filter(|(dy, _)| *dy < 4.0 * 65536.0).map(|(_, bi)| bi);
        Some((x, y, bi))
    }

    /// A click on text puts the caret there; on a formula or a picture it selects that block
    /// (Shift: blocks up to it). A double-click on a formula, a macro's text or a picture edits
    /// its source, with the caret at the clicked character. A click beside the lines while a
    /// box is open only closes the box. (New blocks come from the insert line, `gap_at`.)
    pub(super) fn on_page_click(
        &mut self,
        page: u32,
        pos: Point<Pixels>,
        clicks: usize,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // a double-click edits what its first click hit: that click closed any open box, and the
        // page below it moved back up under the pointer
        let first = self
            .first_click
            .take()
            .filter(|f| clicks >= 2 && f.0 == page);
        let Some((x, y, bi)) = first
            .map(|f| (f.1, f.2, f.3))
            .or_else(|| self.hit(page, pos))
        else {
            return;
        };
        if clicks < 2 {
            self.first_click = Some((page, x, y, bi));
        }
        // Shift-click with the caret: text selected from it to the click
        if shift && clicks < 2 && self.active.is_none() && self.caret.is_some() {
            self.extend_to_click(page, x, y, cx);
            self.dragging = true;
            self.focus.focus(window, cx);
            return;
        }
        // Shift-click with no box open selects from the selection's anchor to this block
        if let (true, None, Some(i)) = (shift, &self.active, bi) {
            let anchor = self
                .selected
                .and_then(|(a, _)| self.block_at(a))
                .unwrap_or(i);
            self.set_selection(anchor, i, cx);
            self.focus.focus(window, cx);
            return;
        }
        self.selected = None;
        // with a box open, only a click on another block's text switches to it; the space
        // beside the lines closes the box
        let near = 4.0 * 65536.0;
        let bi = bi.filter(|&i| {
            self.active.is_none()
                || self.blocks[i]
                    .rows
                    .iter()
                    .any(|r| r.page == page && x >= r.bbox[0] - near && x <= r.bbox[2] + near)
        });
        if clicks < 2 {
            // the first click of a double-click lands here too: it places the caret (or
            // selects), the second edits
            self.leave(window, cx);
            self.caret = None;
            let placed = bi.is_some_and(|i| self.caret_click(i, page, x, y, cx));
            // a drag from here selects
            self.dragging = placed;
            if !placed {
                self.selected =
                    bi.map(|i| (self.blocks[i].range.start, self.blocks[i].range.start));
            }
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }
        let Some(bi) = bi else {
            return;
        };
        // running text: a double-click selects the word, a triple-click the paragraph
        if self.is_plain_text_at(bi, page, x, y) {
            if clicks >= 3 {
                self.select_block_text(bi, cx);
            } else {
                self.select_word(bi, page, x, y, cx);
            }
            return;
        }
        let b = &self.blocks[bi];
        // the character under the click, among the block's glyphs on that page
        let rows: Vec<&render::Row> = b
            .rows
            .iter()
            .filter(|r| r.page == page)
            .map(|r| &r.row)
            .collect();
        let gt = glyphmap::glyph_text_of(&rows);
        let Some(src) = self.source.get(b.range.clone()) else {
            return;
        };
        let caret = glyphmap::char_at(&gt, x, y)
            .and_then(|(i, after)| glyphmap::source_offset(&gt, i, after, src));
        self.activate(bi, caret.unwrap_or(0), window, cx);
    }

    pub fn jump_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.scroll_to(offset, false, cx);
    }

    /// Scroll block at `offset` to near the top; with `if_hidden`, only when it is out of view.
    pub(super) fn scroll_to(&mut self, offset: usize, if_hidden: bool, cx: &mut Context<Self>) {
        let Some(b) = self
            .blocks
            .iter()
            .find(|b| b.range.contains(&offset) || b.range.start >= offset)
        else {
            return;
        };
        let Some(r) = b.rows.first() else { return };
        let mut y = PAGES_TOP;
        for (&n, dl) in &self.preview.pages {
            if n == r.page {
                break;
            }
            y += page_px(dl.page_height.unwrap_or(0.0), self.zoom) + PAGE_GAP;
        }
        let (top, bottom) = (
            y + page_px(r.bbox[1], self.zoom),
            y + page_px(r.bbox[3], self.zoom),
        );
        let view = (
            -f32::from(self.scroll.offset().y),
            f32::from(self.scroll.bounds().size.height),
        );
        if if_hidden && top >= view.0 && bottom <= view.0 + view.1 {
            return;
        }
        y = top - 40.0;
        let x = self.scroll.offset().x;
        self.scroll.set_offset(point(x, px(-y.max(0.0))));
        cx.notify();
    }

    /// Zoom to `z`, keeping the point `anchor` (window coordinates; default the view's centre)
    /// where it is on screen.
    pub fn set_zoom(
        &mut self,
        z: f32,
        anchor: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let z = z.clamp(0.25, 4.0);
        if (z - self.zoom).abs() < 1e-4 {
            return;
        }
        let view = self.scroll.bounds();
        let p = anchor.unwrap_or(view.center()) - view.origin;
        let off = self.scroll.offset();
        let ratio = z / self.zoom;
        // content under the anchor, scaled; the top padding does not scale
        let cx_ = (f32::from(p.x - off.x)) * ratio;
        let cy_ = (f32::from(p.y - off.y) - PAGES_TOP) * ratio + PAGES_TOP;
        self.zoom = z;
        self.zoomed_at = Instant::now();
        self.scroll.set_offset(point(
            px((f32::from(p.x) - cx_).min(0.0)),
            px((f32::from(p.y) - cy_).min(0.0)),
        ));
        self.render_live(window, cx);
        cx.notify();
    }

    /// One step along `ZOOMS`.
    pub fn zoom_step(&mut self, dir: i32, window: &mut Window, cx: &mut Context<Self>) {
        let z = self.zoom;
        let next = if dir > 0 {
            ZOOMS.iter().find(|&&v| v > z + 0.001)
        } else {
            ZOOMS.iter().rev().find(|&&v| v < z - 0.001)
        };
        if let Some(&v) = next {
            self.set_zoom(v, None, window, cx);
        }
    }
}
