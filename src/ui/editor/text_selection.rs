// Selecting text on the pages: from the caret's anchor to the caret, by dragging, Shift-click,
// Shift with any caret move, a double-click (a word), a triple-click (a paragraph) or ⌘A; drawn
// as highlights over the glyphs; deleted, typed over, copied (plain text, and the TeX for a
// paste back here), cut and pasted.
use super::*;
use document::caret;

/// What a copy puts beside the plain text: the TeX, for a paste back into a document.
const TEX_TAG: &str = "realtime-tex:";

impl Doc {
    /// The selected source, when the caret has one.
    pub(super) fn selection_range(&self) -> Option<Range<usize>> {
        let c = self.caret.as_ref()?;
        let a = c.anchor.filter(|a| *a != c.off)?;
        Some(a.min(c.off)..a.max(c.off))
    }

    /// Run the caret move `f`, keeping (or starting) the selection from where the caret was.
    pub(super) fn extending(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Context<Self>),
    ) {
        let Some(c) = self.caret.as_ref() else { return };
        let anchor = c.anchor.unwrap_or(c.off);
        f(self, cx);
        if let Some(c) = self.caret.as_mut() {
            c.anchor = (c.off != anchor).then_some(anchor);
        }
        cx.notify();
    }

    /// ← or → with a selection: the caret to its start (or end), the selection gone.
    pub(super) fn collapse(&mut self, to_end: bool, cx: &mut Context<Self>) -> bool {
        let Some(r) = self.selection_range() else {
            return false;
        };
        self.set_caret(if to_end { r.end } else { r.start }, cx);
        true
    }

    /// Delete the selection, keeping the markup it only partly holds; an undo step. False with
    /// none.
    pub(super) fn delete_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(r) = self.selection_range() else {
            return false;
        };
        let e = caret::delete_range(&self.source, r.start, r.end);
        self.checkpoint();
        self.typing = None;
        self.overlay = None;
        self.replace(e.range.clone(), &e.text, cx);
        let mut at = e.caret.min(self.source.len());
        // a \textbf{} it emptied goes too
        while let Some(g) = caret::empty_group(&self.source, at) {
            self.replace(g.clone(), "", cx);
            at = g.start;
        }
        if let Some(c) = self.caret.as_mut() {
            c.off = at;
            c.anchor = None;
            c.goal_x = None;
        }
        cx.notify();
        true
    }

    /// The stop on `page` nearest (`x`, `y`) sp, in any block there.
    fn stop_on_page(&self, page: u32, x: f64, y: f64) -> Option<usize> {
        let stops = self.page_stops(page);
        caret::nearest_stop(&stops, x, y).map(|i| stops[i].off)
    }

    /// The pointer dragged to `pos` on `page` with the button down: the caret follows it, the
    /// selection grows from where the drag began.
    pub(super) fn drag_to(&mut self, page: u32, pos: Point<Pixels>, cx: &mut Context<Self>) {
        if !self.dragging || self.active.is_some() {
            return;
        }
        let Some((x, y, _)) = self.hit(page, pos) else {
            return;
        };
        let Some(off) = self.stop_on_page(page, x, y) else {
            return;
        };
        let Some(c) = self.caret.as_mut() else { return };
        if c.off == off {
            return;
        }
        let anchor = c.anchor.unwrap_or(c.off);
        c.off = off;
        c.anchor = (off != anchor).then_some(anchor);
        c.goal_x = None;
        self.typing = None;
        self.restart_blink(cx);
        cx.notify();
    }

    /// Shift-click at (`x`, `y`) on `page`: the selection from the caret to there.
    pub(super) fn extend_to_click(
        &mut self,
        page: u32,
        x: f64,
        y: f64,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(off) = self.stop_on_page(page, x, y) else {
            return false;
        };
        self.extending(cx, |d, cx| d.set_caret(off, cx));
        true
    }

    /// A double-click on running text: the word there (a run of letters and digits).
    pub(super) fn select_word(
        &mut self,
        bi: usize,
        page: u32,
        x: f64,
        y: f64,
        cx: &mut Context<Self>,
    ) {
        let Some(m) = self.map_of(bi, page) else {
            return;
        };
        let Some(i) = m.stop_near(x, y) else { return };
        let wordy = |k: usize| m.chars.get(k).is_some_and(|c| c.ch.is_alphanumeric());
        let k = m.stops[i].k;
        // the word the click is in: from the char at the stop, else the one before it
        let at = if wordy(k) {
            k
        } else if k > 0 && wordy(k - 1) {
            k - 1
        } else {
            return;
        };
        let (mut a, mut b) = (at, at + 1);
        while a > 0 && wordy(a - 1) {
            a -= 1;
        }
        while wordy(b) {
            b += 1;
        }
        let stop = |k: usize| m.stops.iter().find(|s| s.k == k).map(|s| s.off);
        if let (Some(lo), Some(hi)) = (stop(a), stop(b)) {
            self.set_caret(lo, cx);
            self.extending(cx, |d, cx| d.set_caret(hi, cx));
        }
    }

    /// A triple-click: all the text of block `bi`.
    pub(super) fn select_block_text(&mut self, bi: usize, cx: &mut Context<Self>) {
        let pages: Vec<u32> = self.blocks[bi].rows.iter().map(|r| r.page).collect();
        let (Some(&first), Some(&last)) = (pages.iter().min(), pages.iter().max()) else {
            return;
        };
        let lo = self
            .map_of(bi, first)
            .and_then(|m| m.stops.first().map(|s| s.off));
        let hi = self
            .map_of(bi, last)
            .and_then(|m| m.stops.last().map(|s| s.off));
        if let (Some(lo), Some(hi)) = (lo, hi) {
            self.set_caret(lo, cx);
            self.extending(cx, |d, cx| d.set_caret(hi, cx));
        }
    }

    /// ⌘A: all the text of the document, from its first text block's start to its last's end.
    pub(super) fn select_all_text(&mut self, cx: &mut Context<Self>) {
        let text: Vec<usize> = (0..self.blocks.len())
            .filter(|&b| self.is_text_block(b))
            .collect();
        let (Some(&first), Some(&last)) = (text.first(), text.last()) else {
            return;
        };
        let edge = |d: &Self, b: usize, end: bool| {
            let pages: Vec<u32> = d.blocks[b].rows.iter().map(|r| r.page).collect();
            let p = if end {
                pages.iter().max()
            } else {
                pages.iter().min()
            }?;
            let m = d.map_of(b, *p)?;
            let s = if end { m.stops.last() } else { m.stops.first() }?;
            Some(s.off)
        };
        if let (Some(lo), Some(hi)) = (edge(self, first, false), edge(self, last, true)) {
            self.set_caret(lo, cx);
            self.extending(cx, |d, cx| d.set_caret(hi, cx));
        }
    }

    /// The selection's highlights on page `n`: a box over each run of selected glyphs on a line.
    pub(super) fn render_selection(&self, n: u32) -> Vec<AnyElement> {
        let Some(sel) = self.selection_range().filter(|_| self.active.is_none()) else {
            return vec![];
        };
        let mut boxes: Vec<[f64; 4]> = vec![];
        for bi in 0..self.blocks.len() {
            let b = &self.blocks[bi];
            if b.range.end < sel.start
                || b.range.start > sel.end
                || !b.rows.iter().any(|r| r.page == n)
            {
                continue;
            }
            let Some(m) = self.map_of(bi, n) else {
                continue;
            };
            // a macro's text or a formula is selected when the text on both sides of it is
            let mut prev_end = vec![None; m.chars.len()];
            let mut last = None;
            for (k, c) in m.chars.iter().enumerate() {
                prev_end[k] = last;
                if let Some(r) = &c.src {
                    last = Some(r.end);
                }
            }
            let mut next_start = None;
            let mut picked = vec![false; m.chars.len()];
            for (k, c) in m.chars.iter().enumerate().rev() {
                picked[k] = match &c.src {
                    Some(r) => r.start >= sel.start && r.end <= sel.end,
                    None => {
                        prev_end[k].is_some_and(|e| e >= sel.start)
                            && next_start.is_some_and(|s| s <= sel.end)
                    }
                };
                if let Some(r) = &c.src {
                    next_start = Some(r.start);
                }
            }
            for (c, _) in m.chars.iter().zip(&picked).filter(|(_, p)| **p) {
                let [x, y, w, size] = c.bbox;
                let (top, bottom) = (y - 0.8 * size, y + 0.25 * size);
                match boxes.last_mut() {
                    Some(r) if (r[1] - top).abs() < 0.3 * size && x <= r[2] + 0.5 * size => {
                        r[2] = r[2].max(x + w);
                        r[3] = r[3].max(bottom);
                        r[1] = r[1].min(top);
                    }
                    _ => boxes.push([x, top, x + w, bottom]),
                }
            }
        }
        boxes
            .into_iter()
            .map(|r| {
                div()
                    .absolute()
                    .left(px(page_px(r[0], self.zoom)))
                    .top(px(page_px(r[1], self.zoom)))
                    .w(px(page_px(r[2] - r[0], self.zoom).max(1.0)))
                    .h(px(page_px(r[3] - r[1], self.zoom)))
                    .bg(rgba(0x3b82f650))
                    .into_any_element()
            })
            .collect()
    }

    /// ⌘C: the selection as plain text, and as its TeX (for a paste back here) when that stands
    /// on its own.
    pub(super) fn copy_selection(&self, cx: &mut App) -> bool {
        let Some(r) = self.selection_range() else {
            return false;
        };
        let tex = &self.source[r];
        let plain = glyphmap::plain_text(tex);
        cx.write_to_clipboard(if caret::balanced(tex) {
            ClipboardItem::new_string_with_metadata(plain, format!("{TEX_TAG}{tex}"))
        } else {
            ClipboardItem::new_string(plain)
        });
        true
    }

    /// ⌘V at the caret, over the selection: TeX copied here goes in as it was; anything else
    /// as plain text, escaped.
    pub(super) fn paste_at_caret(&mut self, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let Some(text) = item.text() else { return };
        let tex = item
            .metadata()
            .and_then(|m| m.strip_prefix(TEX_TAG))
            .filter(|t| caret::balanced(t))
            .map(str::to_string);
        if !self.delete_selection(cx) {
            self.checkpoint();
        }
        self.overlay = None;
        self.typing = None;
        let Some(off) = self.caret.as_ref().map(|c| c.off) else {
            return;
        };
        let ins = tex.unwrap_or_else(|| caret::escape(&text));
        if self.replace(off..off, &ins, cx)
            && let Some(c) = self.caret.as_mut()
        {
            c.off = off + ins.len();
        }
    }
}
