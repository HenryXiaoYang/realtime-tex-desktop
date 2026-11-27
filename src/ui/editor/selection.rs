// Selecting whole blocks on the pages and the commands on them: move, copy, cut, delete, duplicate.
use super::*;

impl Doc {
    /// The block starting at source offset `off` (or containing it).
    pub(super) fn block_at(&self, off: usize) -> Option<usize> {
        self.blocks
            .iter()
            .position(|b| b.range.start == off)
            .or_else(|| self.blocks.iter().position(|b| b.range.contains(&off)))
    }

    /// The selected blocks, first and last index.
    pub(super) fn selection(&self) -> Option<(usize, usize)> {
        let (a, h) = self.selected?;
        let (a, h) = (self.block_at(a)?, self.block_at(h)?);
        Some((a.min(h), a.max(h)))
    }

    /// The selection if it holds block `ix`, else just `ix` (a menu on one of several selected
    /// blocks acts on all of them).
    pub(super) fn selection_with(&self, ix: usize) -> (usize, usize) {
        self.selection()
            .filter(|(lo, hi)| (*lo..=*hi).contains(&ix))
            .unwrap_or((ix, ix))
    }

    pub(super) fn set_selection(&mut self, anchor: usize, head: usize, cx: &mut Context<Self>) {
        let (Some(a), Some(h)) = (self.blocks.get(anchor), self.blocks.get(head)) else {
            return;
        };
        self.selected = Some((a.range.start, h.range.start));
        self.caret = None;
        let off = h.range.start;
        self.scroll_to(off, true, cx);
        cx.notify();
    }

    /// No caret and nothing selected on the pages: Return and Escape go to the window.
    pub fn drop_caret(&mut self, cx: &mut Context<Self>) {
        self.caret = None;
        self.selected = None;
        cx.notify();
    }

    pub(super) fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        cx.notify();
    }

    /// Move the selection's head by `step` blocks, keeping its anchor (⇧↑, ⇧↓).
    pub(super) fn extend_selection(&mut self, step: isize, cx: &mut Context<Self>) {
        let Some((a, h)) = self.selected else { return };
        let (Some(a), Some(h)) = (self.block_at(a), self.block_at(h)) else {
            return;
        };
        let Some(h) = h
            .checked_add_signed(step)
            .filter(|h| *h < self.blocks.len())
        else {
            return;
        };
        self.set_selection(a, h, cx);
    }

    /// Source of blocks `lo..=hi` and its range, if the blocks match the source.
    pub(super) fn blocks_source(&self, lo: usize, hi: usize) -> Option<(Range<usize>, String)> {
        if self.spans_pending > 0 || lo > hi {
            return None;
        }
        let r = self.blocks.get(lo)?.range.start..self.blocks.get(hi)?.range.end;
        Some((r.clone(), self.source.get(r)?.to_string()))
    }

    /// Edit the block at source offset `off` (an outline heading).
    pub fn edit_at(&mut self, off: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.block_at(off) {
            self.set_source_mode(false, window, cx);
            self.scroll_to(off, true, cx);
            self.activate(ix, 0, window, cx);
        }
    }

    /// Source mode, at the block at source offset `off`.
    pub fn source_at(&mut self, off: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.block_at(off) {
            self.show_in_source(ix, ix, window, cx);
        }
    }

    /// Move blocks `lo..=hi` past the block above (`up`) or below, keeping the space between.
    pub(super) fn move_blocks(&mut self, lo: usize, hi: usize, up: bool, cx: &mut Context<Self>) {
        // swap the runs a and b, a before b
        let (a, b) = if up {
            (lo.wrapping_sub(1)..=lo.wrapping_sub(1), lo..=hi)
        } else {
            (lo..=hi, hi + 1..=hi + 1)
        };
        let (Some((ra, ta)), Some((rb, tb))) = (
            self.blocks_source(*a.start(), *a.end()),
            self.blocks_source(*b.start(), *b.end()),
        ) else {
            return;
        };
        let Some(sep) = self.source.get(ra.end..rb.start).map(str::to_string) else {
            return;
        };
        // where the selection's first and last blocks start after the swap
        let (sel_start, last) = if up {
            (rb.start, self.blocks[hi].range.start)
        } else {
            (ra.start, self.blocks[hi].range.start)
        };
        let moved = if up {
            ra.start
        } else {
            ra.start + tb.len() + sep.len()
        };
        self.checkpoint();
        if self.replace(ra.start..rb.end, &format!("{tb}{sep}{ta}"), cx) {
            self.selected = Some((moved, moved + last - sel_start));
        }
    }

    pub(super) fn copy_blocks(&self, lo: usize, hi: usize, cx: &mut App) {
        if let Some((_, t)) = self.blocks_source(lo, hi) {
            cx.write_to_clipboard(ClipboardItem::new_string(t));
        }
    }

    /// Remove blocks `lo..=hi` with the blank lines after them; the block after them is
    /// selected.
    pub(super) fn delete_blocks(
        &mut self,
        lo: usize,
        hi: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((r, _)) = self.blocks_source(lo, hi) else {
            return;
        };
        let ws = self.source[r.end..]
            .bytes()
            .take_while(|c| c.is_ascii_whitespace())
            .count();
        self.leave(window, cx);
        self.checkpoint();
        if self.replace(r.start..r.end + ws, "", cx) {
            self.selected = Some((r.start, r.start));
        }
    }

    /// A copy of blocks `lo..=hi` right after them.
    pub(super) fn duplicate_blocks(&mut self, lo: usize, hi: usize, cx: &mut Context<Self>) {
        let Some((r, t)) = self.blocks_source(lo, hi) else {
            return;
        };
        self.checkpoint();
        self.replace(r.end..r.end, &format!("\n\n{t}"), cx);
    }

    /// Show the file in Source mode with the caret at block `ix`.
    /// Source mode with blocks `lo..=hi` selected in it.
    pub(super) fn show_in_source(
        &mut self,
        lo: usize,
        hi: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((r, _)) = self.blocks_source(lo, hi) else {
            return;
        };
        self.leave(window, cx);
        self.selected = None;
        self.set_source_mode(true, window, cx);
        self.code.update(cx, |s, cx| s.set_selected_range(r, cx));
    }

    /// Edit blocks `lo..=hi` together in one box holding all their source.
    pub(super) fn activate_range(
        &mut self,
        lo: usize,
        hi: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if lo == hi {
            return self.activate(lo, 0, window, cx);
        }
        self.sync_box(true, cx);
        let Some((r, text)) = self.blocks_source(lo, hi) else {
            return;
        };
        self.checkpoint();
        self.selected = None;
        self.active = Some(Active::new(self.blocks[lo].span, r, false, None));
        self.edit_in_box(text, 0, window, cx);
    }
}
