// Editing a block as source: opening the box over its lines (or at an insert spot), typing in it,
// sending the edits to rtex, undo, and leaving it.
use super::*;

impl Doc {
    /// Where the source box goes: over its block's lines on their first page; for new text not
    /// typeset yet, where it was opened; for text that lost its lines, where it last was.
    pub(super) fn box_place(&self) -> Option<BoxPlace> {
        let a = self.active.as_ref().filter(|a| !a.frame && !a.in_panel())?;
        // the box covers every block its text spans, from the first one's page
        let covered: Vec<&document::Block> = self
            .blocks
            .iter()
            .filter(|b| b.range.end.min(a.range.end) > b.range.start.max(a.range.start))
            .collect();
        let multi = covered.len() > 1;
        let set = if multi {
            covered
        } else {
            self.active_block().into_iter().collect()
        };
        let rows: Vec<&document::PlacedRow> = set.iter().flat_map(|b| &b.rows).collect();
        if let Some(page) = rows.iter().map(|r| r.page).min() {
            let rect = union(rows.iter().filter(|r| r.page == page).map(|r| r.bbox))?;
            let font = glyphmap::main_font(&rows.iter().map(|r| &r.row).collect::<Vec<_>>());
            let count = rows.iter().filter(|r| r.page == page).count();
            a.last.set(Some((page, rect, font.clone())));
            return Some(BoxPlace {
                page,
                rect,
                font,
                count,
                lines: true,
                multi,
            });
        }
        if let Some((page, rect, font)) = a.last.take() {
            a.last.set(Some((page, rect, font.clone())));
            return Some(BoxPlace {
                page,
                rect,
                font,
                count: 1,
                lines: false,
                multi,
            });
        }
        // new text: the size of the text it follows
        let g = a.gap.as_ref()?;
        let font = self
            .blocks
            .iter()
            .rev()
            .find(|b| b.range.end <= a.range.start)
            .and_then(|b| glyphmap::main_font(&b.rows.iter().map(|r| &r.row).collect::<Vec<_>>()));
        Some(BoxPlace {
            page: g.page,
            rect: [f64::NAN, g.y, f64::NAN, g.y],
            font,
            count: 0,
            lines: false,
            multi,
        })
    }

    pub(super) fn active_block(&self) -> Option<&document::Block> {
        let a = self.active.as_ref().filter(|a| !a.frame)?;
        let overlap =
            |r: &Range<usize>| r.end.min(a.range.end) as i64 - r.start.max(a.range.start) as i64;
        // new text is no block until it overlaps one; its neighbours only touch it
        let min = if a.gap.is_some() { 1 } else { 0 };
        self.blocks
            .iter()
            .filter(|b| (a.gap.is_none() && b.span == a.span) || overlap(&b.range) >= min)
            .max_by_key(|b| (overlap(&b.range), b.span == a.span))
    }

    pub(super) fn activate(
        &mut self,
        ix: usize,
        caret: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // the box open now keeps what was typed in it
        self.sync_box(true, cx);
        // stale spans only shift where the box opens; destructive actions wait for fresh ones
        let Some(text) = self
            .blocks
            .get(ix)
            .and_then(|b| self.source.get(b.range.clone()))
        else {
            return;
        };
        let text = text.to_string();
        self.checkpoint();
        self.selected = None;
        let b = &self.blocks[ix];
        self.active = Some(Active::new(b.span, b.range.clone(), false, None));
        self.edit_in_box(text, caret, window, cx);
    }

    /// Edit the preamble (`\documentclass` … `\begin{document}`) or the trailer, which print
    /// nothing on the page, in their strip above the first page or below the last.
    pub(super) fn activate_frame(
        &mut self,
        kind: &str,
        caret: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_box(true, cx);
        if !self.spans.iter().any(|s| s.kind == kind) {
            return;
        }
        self.checkpoint();
        let Some(sp) = self.spans.iter().find(|s| s.kind == kind) else {
            return;
        };
        let range = document::trim(
            &self.source,
            sp.range.start..sp.range.end.min(self.source.len()),
        );
        let text = self.source[range.clone()].to_string();
        self.active = Some(Active::new(sp.id, range, true, None));
        self.edit_in_box(text, caret, window, cx);
    }

    /// The insert line for the pointer at `y` (sp) on `page`: where it is drawn (sp) and where
    /// a block added from it goes (see `gap_in`).
    pub(super) fn gap_at(&self, page: u32, y: f64) -> Option<(f64, Spot)> {
        let spans: Vec<(usize, f64, f64)> = self
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| {
                let (top, bottom) = b
                    .rows
                    .iter()
                    .filter(|r| r.page == page)
                    .fold((f64::MAX, f64::MIN), |(t, bo), r| {
                        (t.min(r.bbox[1]), bo.max(r.bbox[3]))
                    });
                (top <= bottom).then_some((i, top, bottom))
            })
            .collect();
        let gap = gap_in(spans, y)?;
        // under the page's last block only if it ends here: one that goes on over the page
        // break would take the new block after its rest, not where the line is
        if let (_, Spot::Below(i)) = gap
            && self.blocks[i].rows.iter().any(|r| r.page > page)
        {
            return None;
        }
        Some(gap)
    }

    /// The pointer moved over page `n`: show the insert line where it is, if anywhere.
    pub(super) fn on_page_hover(&mut self, page: u32, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let gap = self
            .active
            .is_none()
            .then(|| self.hit(page, pos))
            .flatten()
            .and_then(|(_, y, _)| self.gap_at(page, y))
            .map(|(y, spot)| (page, y, spot));
        let key = |g: &Option<(u32, f64, Spot)>| g.map(|(p, y, _)| (p, y.to_bits()));
        if key(&gap) != key(&self.hover_gap) {
            self.hover_gap = gap;
            cx.notify();
        }
    }

    /// Where new text at `spot` goes: its offset, whether the blank line separating it goes
    /// before it, and where its box opens (page, y in sp).
    pub(super) fn place(&self, spot: Spot) -> Option<(usize, bool, u32, f64)> {
        match spot {
            Spot::Above(ix) | Spot::Below(ix) => {
                let b = self.blocks.get(ix)?;
                let below = matches!(spot, Spot::Below(_));
                let r = if below { b.rows.last() } else { b.rows.first() }?;
                Some(if below {
                    (b.range.end, true, r.page, r.bbox[3])
                } else {
                    (b.range.start, false, r.page, r.bbox[1])
                })
            }
            // after the last block above the click
            Spot::Gap(page, y) => {
                let above = |b: &document::Block| {
                    b.rows
                        .last()
                        .is_some_and(|r| r.page < page || (r.page == page && r.bbox[3] <= y))
                };
                // the box opens right under the block it follows (or over the one it precedes)
                let here =
                    |r: Option<&document::PlacedRow>| r.filter(|r| r.page == page).map(|r| r.bbox);
                match self.blocks.iter().rposition(above) {
                    Some(i) => {
                        let b = &self.blocks[i];
                        let y = here(b.rows.last()).map_or(y, |bb| bb[3] + GAP_PAD);
                        Some((b.range.end, true, page, y))
                    }
                    None => {
                        let b = self.blocks.first()?;
                        let y = here(b.rows.first()).map_or(y, |bb| y.min(bb[1]));
                        Some((b.range.start, false, page, y))
                    }
                }
            }
        }
    }

    /// Open a box for a new paragraph at `spot`, starting with `text` (the caret at its MARK, or
    /// at its end).
    pub(super) fn insert_at(
        &mut self,
        spot: Spot,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_box(true, cx);
        // offsets from blocks an edit has not re-spanned yet could land inside a word
        if self.spans_pending > 0 {
            return;
        }
        let Some((at, sep_before, page, y)) = self.place(spot) else {
            return;
        };
        self.checkpoint();
        let caret = text.find(complete::MARK).unwrap_or(text.len());
        let text = text.replace(complete::MARK, "");
        self.active = Some(Active::new(
            u64::MAX,
            at..at,
            false,
            Some(Gap {
                page,
                y,
                sep_before,
            }),
        ));
        self.edit_in_box(text, caret, window, cx);
        self.on_input(window, cx);
    }

    /// Remember the source as it is before an edit.
    pub(super) fn checkpoint(&mut self) {
        if self.undo.last() != Some(&self.source) {
            self.undo.push(self.source.clone());
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
        }
    }

    /// Back to the source before the last edit (⌘Z with no editor focused).
    pub fn undo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // a box opened without a change left a checkpoint equal to now
        while self.undo.last() == Some(&self.source) {
            self.undo.pop();
        }
        let Some(prev) = self.undo.pop() else { return };
        self.leave(window, cx);
        let mut redo = std::mem::take(&mut self.redo);
        redo.push(self.source.clone());
        self.replace(0..self.source.len(), &prev, cx);
        self.redo = redo;
        self.after_history();
    }

    pub fn redo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(next) = self.redo.pop() else { return };
        self.leave(window, cx);
        self.undo.push(self.source.clone());
        let redo = std::mem::take(&mut self.redo);
        self.replace(0..self.source.len(), &next, cx);
        self.redo = redo;
        self.after_history();
    }

    /// After undo or redo: the caret at the end of what changed, a new typing step.
    fn after_history(&mut self) {
        self.typing = None;
        self.overlay = None;
        let end = self.last_edit.end.min(self.source.len());
        if let Some(c) = self.caret.as_mut() {
            c.off = end;
            c.anchor = None;
            c.goal_x = None;
        }
    }

    pub(super) fn edit_in_box(
        &mut self,
        text: String,
        caret: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input.update(cx, |s, cx| {
            s.set_value(text, window, cx);
            let c = caret.min(s.value().len());
            s.set_selected_range(c..c, cx);
            s.focus(window, cx);
        });
        if let Some((old, ..)) = self.live.take() {
            let _ = window.drop_image(old);
        }
        self.render_live(window, cx);
        cx.notify();
    }

    pub fn leave(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_box(window, cx);
    }

    /// Close the source box or the formula editor; for a formula, where the caret belongs after it.
    pub(super) fn close_box(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        // nothing open: no redraw (a click on the title bar must not disturb its window drag)
        if self.active.is_none() && self.live.is_none() {
            return None;
        }
        // what a command left half typed in it goes in now
        self.sync_box(true, cx);
        let after = self.end_formula(cx);
        self.active = None;
        self.table = None;
        self.pointing = None;
        // a hidden box keeps focus otherwise and takes ⌘Z; the pages take it (and its actions)
        if self.input.focus_handle(cx).is_focused(window) {
            self.focus.focus(window, cx);
        }
        if let Some((old, ..)) = self.live.take() {
            let _ = window.drop_image(old);
        }
        cx.notify();
        after
    }

    pub(super) fn on_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .active
            .as_ref()
            .is_some_and(|a| a.panel == Some(Panel::Formula))
            && self.dollar_closes(window, cx)
        {
            return;
        }
        self.sync_box(false, cx);
    }

    /// The box's text into the source, unless (`!force`) a command is still being typed in it:
    /// that waits until it is whole (or the box closes), so LaTeX never sees `\\ti` or `\\`.
    pub(super) fn sync_box(&mut self, force: bool, cx: &mut Context<Self>) {
        // the table editor writes its cells as they change; the box holds nothing of it
        if self
            .active
            .as_ref()
            .is_some_and(|a| a.panel == Some(Panel::Table))
        {
            return;
        }
        let (mut new, at) = self
            .input
            .read_with(cx, |s, _| (s.value().to_string(), s.cursor()));
        if !force && document::caret::unfinished(&new, at) {
            return;
        }
        let Some(a) = self.active.as_ref() else {
            return;
        };
        let region = a.range.clone();
        match &a.gap {
            Some(g) if !new.is_empty() && g.sep_before => new.insert_str(0, "\n\n"),
            Some(_) if !new.is_empty() => new.push_str("\n\n"),
            _ => {}
        }
        if self.replace(region.clone(), &new, cx)
            && let Some(a) = self.active.as_mut()
        {
            a.range.end = region.start + new.len();
        }
    }

    /// rtex's first reply, before any edit, says what revision its count began at.
    pub(super) fn note_revision(&mut self, rev: u64) {
        if self.first_revision.is_none() && self.edits_sent == 0 {
            self.first_revision = Some(rev);
        }
    }

    /// An edit in the code editor: the whole file is its region.
    pub(super) fn on_code(&mut self, cx: &mut Context<Self>) {
        let new = self.code.read(cx).value().to_string();
        self.replace(0..self.source.len(), &new, cx);
    }

    /// Make `source[region]` read `new`, telling rtex only the changed bytes so it keeps the span
    /// (and its id) of the edited block. False when nothing changed.
    pub(super) fn replace(
        &mut self,
        region: Range<usize>,
        new: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        // a range from an out-of-date layout: never cut the source at a wrong place
        let Some(old) = self.source.get(region.clone()) else {
            return false;
        };
        if old == new {
            return false;
        }
        // a new edit ends the redo history (undo and redo put theirs back)
        self.redo.clear();
        // the selection is offsets: keep it on its blocks across edits before them
        if let Some((a, h)) = self.selected {
            let shift = |o: usize| {
                if o >= region.end {
                    Some(o + new.len() - old.len())
                } else {
                    (o <= region.start).then_some(o)
                }
            };
            self.selected = shift(a).zip(shift(h));
        }
        let p = old
            .char_indices()
            .zip(new.chars())
            .find(|((_, a), b)| a != b)
            .map_or(old.len().min(new.len()), |((i, _), _)| i);
        let max_s = (old.len() - p).min(new.len() - p);
        let mut s = old
            .bytes()
            .rev()
            .zip(new.bytes().rev())
            .take(max_s)
            .take_while(|(a, b)| a == b)
            .count();
        while !old.is_char_boundary(old.len() - s) || !new.is_char_boundary(new.len() - s) {
            s -= 1;
        }
        let (start, end) = (region.start + p, region.end - s);
        let text = &new[p..new.len() - s];
        if let Some(r) = self.rtex.as_mut() {
            r.edit(start, end, text);
            self.edits_sent += 1;
            self.edited_at = Instant::now();
            r.request_spans();
            self.spans_pending += 1;
        }
        // the spans and the caret follow the edit until rtex's spans come back: the blocks stay
        // on their text, and new text at a block's start or end joins that block
        let mv = |o: usize| o + text.len() - (end - start);
        let to_next = start == end && self.spans.iter().any(|s| s.range.start == start);
        for sp in &mut self.spans {
            let r = &mut sp.range;
            if r.start > end || (r.start == end && start < end) {
                (r.start, r.end) = (mv(r.start), mv(r.end));
            } else if r.end > start || (r.end == start && !to_next) {
                (r.start, r.end) = (r.start.min(start), mv(r.end.max(end)));
            }
        }
        let follow = |o: usize| {
            if o <= start {
                o
            } else if o >= end {
                mv(o)
            } else {
                start + text.len()
            }
        };
        if let Some(c) = self.caret.as_mut() {
            c.off = follow(c.off);
            c.anchor = c.anchor.map(follow);
        }
        self.last_edit = start..start + text.len();
        self.source.replace_range(start..end, text);
        self.blocks = document::blocks(&self.spans, &self.source, &self.preview);
        self.maps.borrow_mut().clear();
        self.page_text.borrow_mut().clear();
        self.restart_blink(cx);
        if self.find.is_some() {
            self.refind(cx);
        }
        self.outline = document::outline(&self.source);
        self.dirty = true;
        let s = AppSettings::get(cx);
        if s.autosave {
            // restarted by each edit: saves once typing pauses for the delay
            let delay = Duration::from_secs_f64(s.autosave_delay.max(0.5));
            self.autosave = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(delay).await;
                let _ = this.update(cx, |this, cx| this.save(cx));
            }));
        }
        cx.notify();
        true
    }

    /// The source offset the user is working at on the pages, for the code editor to open at: the
    /// caret in the box being edited, else the caret, else the selection, else the first block in
    /// view.
    fn source_focus(&self, cx: &App) -> Option<usize> {
        if let Some(a) = &self.active {
            let inside = (!a.in_panel()).then(|| self.input.read(cx).cursor());
            return Some(a.range.start + inside.unwrap_or(0));
        }
        if let Some(c) = &self.caret {
            return Some(c.off);
        }
        if let Some((lo, _)) = self.selection() {
            return self.blocks.get(lo).map(|b| b.range.start);
        }
        let top = -f32::from(self.scroll.offset().y);
        self.blocks
            .iter()
            .find(|b| {
                b.rows
                    .last()
                    .is_some_and(|r| self.page_y(r.page) + page_px(r.bbox[3], self.zoom) > top)
            })
            .map(|b| b.range.start)
    }

    /// Switch a .tex file between its typeset pages and its source in the code editor.
    pub fn set_source_mode(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        if on == self.source_mode || !self.tex {
            return;
        }
        if on {
            self.checkpoint();
            // the editor sees the file as edited on the pages, with the caret where editing was
            let at = self.source_focus(cx);
            self.leave(window, cx);
            let text = self.source.clone();
            self.code.update(cx, |s, cx| {
                if s.value().as_ref() != text {
                    s.set_value(text, window, cx);
                }
                if let Some(c) = at {
                    s.set_selected_range(c..c, cx);
                }
                s.focus(window, cx);
            });
        }
        self.source_mode = on;
        cx.notify();
    }

    /// Escape leaves the block; Up on its first line or Down on its last moves to the next one.
    pub(super) fn on_box_key(
        &mut self,
        ks: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.table.is_some() {
            return self.on_table_key(ks, window, cx);
        }
        if self.active.is_none() || !self.input.focus_handle(cx).is_focused(window) {
            return;
        }
        // the arrows and Escape belong to an open completion menu
        if self.input.read(cx).completion_menu_state().open {
            return;
        }
        let key = ks.key.as_str();
        if key == "escape" {
            self.finish_formula(window, cx);
            cx.stop_propagation();
            return;
        }
        // the formula editor has no neighbouring blocks to move to
        if self.active.as_ref().is_some_and(|a| a.in_panel()) {
            return;
        }
        if ks.modifiers.modified() || (key != "up" && key != "down") {
            return;
        }
        let (text, cursor) = self
            .input
            .read_with(cx, |s, _| (s.value().to_string(), s.cursor()));
        let line = text[..cursor.min(text.len())].matches('\n').count();
        let (first, last) = (line == 0, line == text.matches('\n').count());
        let n = self.blocks.len();
        // the preamble sits before block 0 and the trailer after the last block
        let ix = match self.active.as_ref() {
            Some(a) if a.frame => {
                if a.range.start == 0 {
                    -1
                } else {
                    n as i64
                }
            }
            // a box over several blocks leaves from its first (up) or last (down) one
            Some(a) => {
                let r = a.range.clone();
                let covered =
                    |b: &document::Block| b.range.end.min(r.end) > b.range.start.max(r.start);
                let ix = if key == "up" {
                    self.blocks.iter().position(covered)
                } else {
                    self.blocks.iter().rposition(covered)
                };
                let Some(ix) = ix.or_else(|| {
                    let cur = self.active_block()?.range.start;
                    self.blocks.iter().position(|b| b.range.start == cur)
                }) else {
                    return;
                };
                ix as i64
            }
            None => return,
        };
        let target = match key {
            "up" if first => ix - 1,
            "down" if last => ix + 1,
            _ => return,
        };
        if target < -1 || target > n as i64 {
            return;
        }
        if target == -1 {
            self.activate_frame("Preamble", usize::MAX, window, cx);
        } else if target == n as i64 {
            self.activate_frame("Trailer", 0, window, cx);
        } else if key == "up" {
            let len = self.blocks[target as usize].range.len();
            self.activate(target as usize, len, window, cx);
        } else {
            self.activate(target as usize, 0, window, cx);
        }
        cx.stop_propagation();
    }
}

/// The insert line for `y` among blocks `(index, top, bottom)` on a page (sp): only in the
/// space between two blocks, or just under the last one, never over text.
pub(super) fn gap_in(mut spans: Vec<(usize, f64, f64)>, y: f64) -> Option<(f64, Spot)> {
    spans.sort_by(|a, b| a.1.total_cmp(&b.1));
    for w in spans.windows(2) {
        let ((a, _, a_bottom), (_, b_top, _)) = (w[0], w[1]);
        if y > a_bottom && y < b_top {
            return Some(((a_bottom + b_top) / 2.0, Spot::Below(a)));
        }
    }
    let last = spans.last()?;
    (y > last.2 && y < last.2 + END_BAND).then_some((last.2 + GAP_PAD, Spot::Below(last.0)))
}

/// How far under the last block the insert line shows, in sp.
pub(super) const END_BAND: f64 = 24.0 * 65536.0;

#[cfg(test)]
mod tests {
    use super::{Spot, gap_in};

    #[test]
    fn the_insert_line_shows_only_between_blocks() {
        let pt = 65536.0;
        // two blocks: 0 from 10 to 20pt, 1 from 30 to 40pt
        let spans = || vec![(1, 30.0 * pt, 40.0 * pt), (0, 10.0 * pt, 20.0 * pt)];
        let at = |y: f64| gap_in(spans(), y * pt).map(|(y, s)| (y / pt, s));
        assert!(matches!(at(25.0), Some((25.0, Spot::Below(0)))));
        assert!(matches!(at(21.0), Some((25.0, Spot::Below(0)))));
        // over a block's text, above the first block, far below the last: none
        assert!(at(19.0).is_none());
        assert!(at(5.0).is_none());
        assert!(at(80.0).is_none());
        // just under the last block
        assert!(matches!(at(50.0), Some((_, Spot::Below(1)))));
        assert!(gap_in(vec![], 5.0).is_none());
    }
}
