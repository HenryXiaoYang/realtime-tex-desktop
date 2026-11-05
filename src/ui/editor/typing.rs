// Visual mode: a text caret on the typeset pages. Typing, Enter, Tab and Backspace become source
// edits at the caret (document::caret makes them); rtex re-typesets the block, and until the
// pages catch up the typed text is drawn where it goes. Input methods compose in place.
use super::*;
use document::caret::{self, CharMap, Edit};
use document::format::{Align, Style};

/// The caret: a source offset, kept on its text across edits.
pub(super) struct Caret {
    pub off: usize,
    /// Where a selection began; the selection runs from here to `off`.
    pub anchor: Option<usize>,
    /// Where Up and Down aim across (sp), from the column the caret was in.
    pub goal_x: Option<f64>,
    /// An input method's text being composed at the caret (not in the source yet), and the
    /// selection inside it (UTF-16).
    pub marked: Option<(String, Option<Range<usize>>)>,
}

/// Text typed since the pages last caught up, drawn where it goes until they do.
pub(super) struct Overlay {
    page: u32,
    /// Where it starts: x and baseline (sp), and the font of the text there.
    x: f64,
    y: f64,
    size: f64,
    font: Option<engine::FontDesc>,
    /// What was typed, each with the source bytes it became (`\%` for a %).
    units: Vec<(String, usize)>,
    /// The source offset after it.
    end: usize,
    /// A typeset update came in since the last of it was typed.
    seen: bool,
}

/// The undo step typing is in.
pub(super) struct Typing {
    at: Instant,
    /// Where it ended: typing elsewhere starts another step.
    end: usize,
    text: String,
    delete: bool,
}

/// A pause that ends a typing step.
const STEP_PAUSE: Duration = Duration::from_secs(1);

impl Doc {
    /// The caret map of block `bi` on `page`, cached until the blocks or the source change.
    pub(super) fn map_of(&self, bi: usize, page: u32) -> Option<Rc<CharMap>> {
        let b = self.blocks.get(bi)?;
        let key = (b.range.start, page);
        if let Some(m) = self.maps.borrow().get(&key) {
            return Some(m.clone());
        }
        let rows: Vec<&render::Row> = b
            .rows
            .iter()
            .filter(|r| r.page == page)
            .map(|r| &r.row)
            .collect();
        if rows.is_empty() {
            return None;
        }
        let src = self.source.get(b.range.clone())?;
        let gt = glyphmap::glyph_text_of(&rows);
        let mut m = caret::char_map(&gt, src, b.range.start);
        self.add_empty_items(bi, page, &mut m);
        self.add_empty_heading(bi, page, &mut m);
        let m = Rc::new(m);
        self.maps.borrow_mut().insert(key, m.clone());
        Some(m)
    }

    /// A block the caret goes into: running text with somewhere to stand, not a formula.
    pub(super) fn is_text_block(&self, bi: usize) -> bool {
        let Some(b) = self.blocks.get(bi) else {
            return false;
        };
        let display = self
            .source
            .get(b.range.clone())
            .is_none_or(document::is_display_math);
        !display
            && self
                .pages_of(bi)
                .any(|p| self.map_of(bi, p).is_some_and(|m| !m.stops.is_empty()))
    }

    fn pages_of(&self, bi: usize) -> impl Iterator<Item = u32> + '_ {
        let set: BTreeSet<u32> = self.blocks[bi].rows.iter().map(|r| r.page).collect();
        set.into_iter()
    }

    /// The block holding source offset `off` (its ends included).
    pub(super) fn block_of(&self, off: usize) -> Option<usize> {
        self.blocks
            .iter()
            .position(|b| b.range.start <= off && off <= b.range.end)
    }

    /// Where the caret stands: its block, page, that page's map and the stop in it.
    fn caret_stop(&self) -> Option<(usize, u32, Rc<CharMap>, usize)> {
        let off = self.caret.as_ref()?.off;
        let bi = self.block_of(off).filter(|&bi| self.is_text_block(bi))?;
        let mut best = None;
        for page in self.pages_of(bi) {
            let Some(m) = self.map_of(bi, page) else {
                continue;
            };
            if let Some(i) = m.stop_for(off, &self.source) {
                let exact = m.stops[i].off == off;
                best = Some((bi, page, m, i));
                if exact {
                    break;
                }
            }
        }
        // before the block's first stop: that stop
        best.or_else(|| {
            let page = self.pages_of(bi).next()?;
            let m = self.map_of(bi, page)?;
            (!m.stops.is_empty()).then_some((bi, page, m, 0))
        })
    }

    /// The caret's page, x and baseline (sp) and font size, from the typeset text; between
    /// blocks (a new paragraph not typeset yet), under the block before.
    fn caret_at(&self) -> Option<(u32, f64, f64, f64)> {
        let off = self.caret.as_ref()?.off;
        if let Some((_, page, m, i)) = self.caret_stop() {
            let s = m.stops[i];
            if s.off != off
                && let Some(at) = self.item_spot(&m, off)
            {
                return Some((page, at.0, at.1, s.size));
            }
            return Some((page, s.x, s.y, s.size));
        }
        let size = |b: &document::Block| {
            glyphmap::main_font(&b.rows.iter().map(|r| &r.row).collect::<Vec<_>>())
                .and_then(|f| f.size)
                .unwrap_or(10.0 * 65536.0)
        };
        let left = |page: u32| {
            self.preview
                .pages
                .get(&page)
                .map(|dl| dl.lines.iter().map(|l| l.x).fold(f64::MAX, f64::min))
                .filter(|x| *x < f64::MAX)
                .unwrap_or(72.0 * 65536.0)
        };
        // in the gap between the blocks around it, above the text it precedes; blocks that
        // print nothing (\\newpage) have nowhere to draw it
        let shown = |b: &&document::Block| !b.rows.is_empty();
        let prev = self
            .blocks
            .iter()
            .rposition(|b| b.range.end <= off && shown(&b))
            .map(|i| &self.blocks[i]);
        let next = self
            .blocks
            .iter()
            .find(|b| b.range.start >= off && shown(b));
        let below = |b: &document::Block| {
            let r = b.rows.last()?;
            Some((r.page, r.bbox[3]))
        };
        let above = |b: &document::Block| {
            let r = b.rows.first()?;
            Some((r.page, r.bbox[1]))
        };
        match (prev.and_then(below), next.and_then(above)) {
            (Some((p, bottom)), Some((q, top))) if p == q && top > bottom => {
                let s = size(prev.or(next)?);
                // a line's baseline in the middle of the gap, never into the text below
                let y = ((bottom + top) / 2.0 + 0.35 * s)
                    .min(top - 0.05 * s)
                    .max(bottom + 0.1 * s);
                Some((p, left(p), y, s))
            }
            (_, Some((q, top))) => Some((q, left(q), top - 0.25 * size(next?), size(next?))),
            (Some((p, bottom)), None) => {
                let s = size(prev?);
                Some((p, left(p), bottom + 1.5 * s, s))
            }
            (None, None) => None,
        }
    }

    /// Where an empty item at `off` with no line of its own goes (inside a box, a minipage's
    /// list): under the item before, at its text's column, as far below as items are apart.
    fn item_spot(&self, m: &CharMap, off: usize) -> Option<(f64, f64)> {
        let src = &self.source;
        let it = caret::item_at(src, off).filter(|it| it.text.is_empty())?;
        let before = |it: &caret::Item| {
            caret::item_at(src, it.line.checked_sub(1)?)
                .filter(|p| p.list == it.list && p.item < it.item)
        };
        let lines = |it: &caret::Item| {
            let st: Vec<&caret::Stop> = m
                .stops
                .iter()
                .filter(|s| s.off >= it.text.start && s.off <= it.text.end)
                .collect();
            let first = **st.first()?;
            let last = st.iter().map(|s| s.y).fold(first.y, f64::max);
            Some((first, last))
        };
        let prev = before(&it)?;
        let (first, last) = lines(&prev)?;
        let gap = before(&prev)
            .and_then(|pp| lines(&pp))
            .map(|(_, pp_last)| first.y - pp_last)
            .filter(|g| *g > 0.0)
            .unwrap_or(1.5 * first.size);
        Some((first.x, last + gap))
    }

    /// The font of the text at the caret, for text typed there before it is typeset: the char
    /// before it (as typing takes its formatting), else the one after, else its block's.
    fn caret_font(&self) -> Option<engine::FontDesc> {
        if let Some((_, _, m, i)) = self.caret_stop() {
            let k = m.stops[i].k;
            let font = |c: Option<&caret::Char>| c.filter(|c| c.glyph).and_then(|c| c.font.clone());
            if let Some(f) =
                font(k.checked_sub(1).and_then(|j| m.chars.get(j))).or_else(|| font(m.chars.get(k)))
            {
                return Some(f);
            }
        }
        let off = self.caret.as_ref()?.off;
        let bi = self.block_of(off).or_else(|| {
            self.blocks
                .iter()
                .rposition(|b| b.range.end <= off && !b.rows.is_empty())
        })?;
        let b = &self.blocks[bi];
        glyphmap::main_font(&b.rows.iter().map(|r| &r.row).collect::<Vec<_>>())
    }

    /// Put the caret at `off`, ending any typing step there.
    pub(super) fn set_caret(&mut self, off: usize, cx: &mut Context<Self>) {
        let off = off.min(self.source.len());
        self.selected = None;
        self.caret = Some(Caret {
            off,
            anchor: None,
            goal_x: None,
            marked: None,
        });
        self.typing = None;
        self.restart_blink(cx);
        self.scroll_to(off, true, cx);
        cx.notify();
    }

    /// Show the caret and blink it from now, at the theme's rate: each move or edit restarts
    /// the blink, as in any editor. It stops when the caret goes.
    pub(super) fn restart_blink(&mut self, cx: &mut Context<Self>) {
        self.caret_on = true;
        let Some(interval) = cx.theme().cursor_blink.interval() else {
            self.blink = None;
            return;
        };
        self.blink = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(interval).await;
                let on = this.update(cx, |d, cx| {
                    d.caret_on = !d.caret_on;
                    cx.notify();
                    d.caret.is_some()
                });
                if !matches!(on, Ok(true)) {
                    break;
                }
            }
        }));
    }

    /// A click at (`x`, `y`) sp on `page` in block `bi`: the caret there. False when the
    /// block takes no caret (a formula, a picture).
    pub(super) fn caret_click(
        &mut self,
        bi: usize,
        page: u32,
        x: f64,
        y: f64,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.is_text_block(bi) {
            return false;
        }
        let Some(m) = self.map_of(bi, page) else {
            return false;
        };
        let Some(i) = m.stop_near(x, y) else {
            return false;
        };
        self.set_caret(m.stops[i].off, cx);
        true
    }

    /// An empty heading (`\\subsection{}`, just its number on the page): a stop after the
    /// number, where its title goes, so the caret can go there and Backspace can remove it.
    fn add_empty_heading(&self, bi: usize, page: u32, m: &mut CharMap) {
        let b = &self.blocks[bi];
        let Some(title) = self
            .source
            .get(b.range.clone())
            .and_then(|src| caret::heading_title(src, b.range.start))
        else {
            return;
        };
        if !m.stops.is_empty() || glyphmap::has_visible(&self.source[title.clone()]) {
            return;
        }
        let Some(r) = b.rows.iter().find(|r| r.page == page) else {
            return;
        };
        let last = m.chars.iter().rev().find(|c| c.glyph);
        let size = last.map_or(10.0 * 65536.0, |c| c.bbox[3]);
        let x = last.map_or(r.bbox[0], |c| c.bbox[0] + c.bbox[2] + 0.5 * size);
        m.stops.push(caret::Stop {
            k: m.chars.len(),
            off: title.start,
            x,
            y: r.bbox[3] - r.row.dl.lines[r.row.line].d,
            size,
        });
    }

    /// Stops on the lines of block `bi` that have no text of their own (empty list items,
    /// just their label): where each item's text goes, at the start of its line.
    fn add_empty_items(&self, bi: usize, page: u32, m: &mut CharMap) {
        let b = &self.blocks[bi];
        let base = |r: &document::PlacedRow| r.bbox[3] - r.row.dl.lines[r.row.line].d;
        let stops = m.stops.clone();
        let bare = |y: f64| !stops.iter().any(|s| (s.y - y).abs() < 0.5 * s.size);
        // an empty item's line holds its label, hanging left of the line, and nothing else (a
        // picture's lines have no stops either)
        let label_only = |r: &document::PlacedRow| {
            let l = &r.row.dl.lines[r.row.line];
            let glyphs: Vec<(f64, f64)> = l
                .items
                .iter()
                .filter_map(|it| it.as_array().filter(|a| a.len() >= 7 && a[0] == "g"))
                .map(|a| (a[4].as_f64().unwrap_or(0.0), a[6].as_f64().unwrap_or(0.0)))
                .collect();
            !glyphs.is_empty() && glyphs.iter().all(|(x, w)| x + w <= l.x + 32768.0)
        };
        let mut rows: Vec<&document::PlacedRow> = b
            .rows
            .iter()
            .filter(|r| r.page == page && bare(base(r)) && label_only(r))
            .collect();
        rows.sort_by(|a, b| base(a).total_cmp(&base(b)));
        let src = &self.source;
        for (k, r) in rows.iter().enumerate() {
            let line = base(r);
            // the text around it, and which of the bare lines between is this one
            let above: Vec<&caret::Stop> = stops.iter().filter(|s| s.y < line).collect();
            let lo = above.iter().map(|s| s.off).max().unwrap_or(b.range.start);
            let top = above.iter().map(|s| s.y).reduce(f64::max);
            let after = stops.iter().filter(|s| s.y > line).min_by_key(|s| s.off);
            let hi = after.map_or(b.range.end, |s| s.off);
            let n = rows[..k]
                .iter()
                .filter(|q| top.is_none_or(|t| base(q) > t))
                .count();
            let mut empty = vec![];
            let mut at = lo;
            while let Some(i) = src.get(at..hi).and_then(|t| t.find("\\item")) {
                let p = at + i + "\\item".len();
                if let Some(it) = caret::item_at(src, p).filter(|it| it.text.is_empty()) {
                    empty.push(it.text.start);
                }
                at = p;
            }
            let Some(&off) = empty.get(n) else { continue };
            let size = after
                .or(above.last().copied())
                .map_or(10.0 * 65536.0, |s| s.size);
            let stop = caret::Stop {
                k: after.map_or(m.chars.len(), |s| s.k),
                off,
                x: r.bbox[0],
                y: line,
                size,
            };
            let at = m
                .stops
                .iter()
                .position(|s| s.off > off)
                .unwrap_or(m.stops.len());
            m.stops.insert(at, stop);
        }
    }

    /// Whether the char under (`x`, `y`) on `page` in block `bi` is text the caret edits (not a
    /// formula or a macro's output, which a double-click opens as source).
    pub(super) fn is_plain_text_at(&self, bi: usize, page: u32, x: f64, y: f64) -> bool {
        let Some(m) = self.map_of(bi, page).filter(|_| self.is_text_block(bi)) else {
            return false;
        };
        let near = m.chars.iter().min_by(|a, b| {
            let d = |c: &caret::Char| {
                let (cx, cy) = (c.bbox[0] + c.bbox[2] / 2.0, c.bbox[1] - 0.25 * c.bbox[3]);
                (cy - y).abs() * 4.0 + (cx - x).abs()
            };
            d(a).total_cmp(&d(b))
        });
        near.is_some_and(|c| c.src.is_some())
    }

    /// The next (`dir` 1) or previous (-1) block and page after (`bi`, `page`) that the caret
    /// can stand in.
    fn next_spot(&self, bi: usize, page: u32, dir: i32) -> Option<(usize, u32)> {
        let mut seq: Vec<(usize, u32)> = vec![];
        for b in 0..self.blocks.len() {
            seq.extend(self.pages_of(b).map(|p| (b, p)));
        }
        let at = seq.iter().position(|s| *s == (bi, page))?;
        let mut k = at as i64;
        loop {
            k += dir as i64;
            let &(b, p) = seq.get(usize::try_from(k).ok()?)?;
            if self.is_text_block(b) && self.map_of(b, p).is_some_and(|m| !m.stops.is_empty()) {
                return Some((b, p));
            }
        }
    }

    /// The first (or last) stop at (`bi`, `page`).
    fn edge_stop(&self, bi: usize, page: u32, last: bool) -> Option<usize> {
        let m = self.map_of(bi, page)?;
        let s = if last {
            m.stops.last()
        } else {
            m.stops.first()
        };
        s.map(|s| s.off)
    }

    /// ← and →: one character, on into the block before or after.
    fn move_h(&mut self, dir: i32, cx: &mut Context<Self>) {
        let Some(off) = self.caret.as_ref().map(|c| c.off) else {
            return;
        };
        let target = match self.caret_stop() {
            Some((bi, page, m, i)) => {
                let exact = m.stops[i].off == off;
                let j = match dir {
                    d if d < 0 && exact => i as i64 - 1,
                    d if d < 0 => i as i64,
                    _ => i as i64 + 1,
                };
                match usize::try_from(j).ok().and_then(|j| m.stops.get(j)) {
                    Some(s) => Some(s.off),
                    None => self
                        .next_spot(bi, page, dir)
                        .and_then(|(b, p)| self.edge_stop(b, p, dir < 0)),
                }
            }
            // between blocks: into the one on that side
            None => {
                let bi = if dir < 0 {
                    self.blocks
                        .iter()
                        .rposition(|b| b.range.end <= off && !b.rows.is_empty())
                } else {
                    self.blocks
                        .iter()
                        .position(|b| b.range.start >= off && !b.rows.is_empty())
                };
                bi.and_then(|b| {
                    let p = if dir < 0 {
                        self.pages_of(b).last()
                    } else {
                        self.pages_of(b).next()
                    }?;
                    if self.is_text_block(b) {
                        self.edge_stop(b, p, dir < 0)
                    } else {
                        self.next_spot(b, p, dir)
                            .and_then(|(b, p)| self.edge_stop(b, p, dir < 0))
                    }
                })
            }
        };
        if let Some(t) = target {
            self.set_caret(t, cx);
        }
    }

    /// ⌥← and ⌥→: to the start of this or the previous word, or the end of this or the next.
    pub(super) fn move_word(&mut self, dir: i32, cx: &mut Context<Self>) {
        let Some((_, _, m, i)) = self.caret_stop() else {
            return self.move_h(dir, cx);
        };
        let wordy = |k: usize| m.chars.get(k).is_some_and(|c| c.ch.is_alphanumeric());
        let is_start = |s: &caret::Stop| wordy(s.k) && (s.k == 0 || !wordy(s.k - 1));
        let is_end = |s: &caret::Stop| s.k > 0 && wordy(s.k - 1) && !wordy(s.k);
        let off = m.stops[i].off;
        let found = if dir < 0 {
            m.stops[..i]
                .iter()
                .rev()
                .find(|s| is_start(s) && s.off < off)
        } else {
            m.stops[i + 1..].iter().find(|s| is_end(s))
        };
        match found {
            Some(s) => self.set_caret(s.off, cx),
            None => self.move_h(dir, cx),
        }
    }

    /// The stops of every text block on `page`.
    pub(super) fn page_stops(&self, page: u32) -> Vec<caret::Stop> {
        (0..self.blocks.len())
            .filter(|&b| {
                self.blocks[b].rows.iter().any(|r| r.page == page) && self.is_text_block(b)
            })
            .filter_map(|b| self.map_of(b, page))
            .flat_map(|m| m.stops.clone())
            .collect()
    }

    /// ↑ and ↓: the line above or below, near the column the caret came from.
    fn move_v(&mut self, dir: i32, cx: &mut Context<Self>) {
        let Some((page, x, y, size)) = self.caret_at() else {
            return;
        };
        let goal = self.caret.as_ref().and_then(|c| c.goal_x).unwrap_or(x);
        let same = |a: f64, b: f64, s: f64| (a - b).abs() < 0.5 * s;
        let pick = |stops: &[caret::Stop], ty: f64| {
            stops
                .iter()
                .filter(|s| same(s.y, ty, s.size))
                .min_by(|a, b| (a.x - goal).abs().total_cmp(&(b.x - goal).abs()))
                .copied()
        };
        let here = self.page_stops(page);
        let line = if dir > 0 {
            here.iter()
                .filter(|s| s.y > y + 0.5 * size)
                .map(|s| s.y)
                .reduce(f64::min)
        } else {
            here.iter()
                .filter(|s| s.y < y - 0.5 * size)
                .map(|s| s.y)
                .reduce(f64::max)
        };
        let target = match line {
            Some(ty) => pick(&here, ty),
            // the next page's first line, the previous page's last
            None => {
                let pages: Vec<u32> = self.preview.pages.keys().copied().collect();
                let at = pages.iter().position(|&p| p == page);
                let other = at.and_then(|a| {
                    let mut k = a as i64;
                    loop {
                        k += dir as i64;
                        let p = *pages.get(usize::try_from(k).ok()?)?;
                        let st = self.page_stops(p);
                        if !st.is_empty() {
                            return Some(st);
                        }
                    }
                });
                other.and_then(|st| {
                    let ys = st.iter().map(|s| s.y);
                    let ty = if dir > 0 {
                        ys.reduce(f64::min)
                    } else {
                        ys.reduce(f64::max)
                    }?;
                    pick(&st, ty)
                })
            }
        };
        if let Some(s) = target {
            self.set_caret(s.off, cx);
            if let Some(c) = self.caret.as_mut() {
                c.goal_x = Some(goal);
            }
        }
    }

    /// ⌘← and ⌘→: the start or end of the line.
    fn move_line_edge(&mut self, end: bool, cx: &mut Context<Self>) {
        let Some((_, _, m, i)) = self.caret_stop() else {
            return;
        };
        let s = m.stops[i];
        let line = m.stops.iter().filter(|t| (t.y - s.y).abs() < 0.5 * s.size);
        let t = if end {
            line.max_by(|a, b| a.x.total_cmp(&b.x))
        } else {
            line.min_by(|a, b| a.x.total_cmp(&b.x))
        };
        if let Some(t) = t.copied() {
            self.set_caret(t.off, cx);
        }
    }

    /// ⌘↑ and ⌘↓: the start or end of the document's text.
    fn move_doc_edge(&mut self, end: bool, cx: &mut Context<Self>) {
        let n = self.blocks.len();
        let mut order: Box<dyn Iterator<Item = usize>> = if end {
            Box::new((0..n).rev())
        } else {
            Box::new(0..n)
        };
        let Some(bi) = order.find(|&b| self.is_text_block(b)) else {
            return;
        };
        let page = if end {
            self.pages_of(bi).last()
        } else {
            self.pages_of(bi).next()
        };
        if let Some(off) = page.and_then(|p| self.edge_stop(bi, p, end)) {
            self.set_caret(off, cx);
        }
    }

    /// Start a new undo step unless this edit continues the one typing is in.
    fn step(&mut self, text: &str, delete: bool, force: bool) {
        let off = self.caret.as_ref().map_or(0, |c| c.off);
        let goes_on = self
            .typing
            .as_ref()
            .filter(|t| t.end == off && t.delete == delete && t.at.elapsed() < STEP_PAUSE);
        let new = match goes_on {
            Some(t) => force || (!delete && caret::new_step(&t.text, text)),
            None => true,
        };
        if new {
            self.checkpoint();
        }
        let before = if new {
            String::new()
        } else {
            self.typing.take().map(|t| t.text).unwrap_or_default()
        };
        self.typing = Some(Typing {
            at: Instant::now(),
            end: off,
            text: before + text,
            delete,
        });
    }

    /// Type `text` at the caret (escaped, so it prints as typed). `commit`: an input method's
    /// commit, an undo step of its own.
    pub(super) fn type_text(&mut self, text: &str, commit: bool, cx: &mut Context<Self>) {
        // typing over a selection replaces it, in the same undo step
        let replaced = self.delete_selection(cx);
        let Some(off) = self.caret.as_ref().map(|c| c.off) else {
            return;
        };
        // a space after a space, or before any text, prints nothing in TeX
        let src = &self.source;
        if text.trim().is_empty()
            && (off == 0 || src[..off].ends_with(|c: char| c.is_whitespace() || c == '~'))
        {
            return;
        }
        let esc = caret::escape(text);
        let at = self.caret_at();
        let font = self.caret_font();
        if replaced {
            self.typing = Some(Typing {
                at: Instant::now(),
                end: off,
                text: text.to_string(),
                delete: false,
            });
        } else {
            self.step(text, false, commit);
        }
        if !self.replace(off..off, &esc, cx) {
            return;
        }
        let end = off + esc.len();
        if let Some(c) = self.caret.as_mut() {
            c.off = end;
            c.goal_x = None;
        }
        if let Some(t) = self.typing.as_mut() {
            t.end = end;
        }
        match self.overlay.as_mut().filter(|o| o.end == off) {
            Some(o) => {
                o.units.push((text.to_string(), esc.len()));
                o.end = end;
                o.seen = false;
            }
            None => {
                self.overlay = at.map(|(page, x, y, size)| Overlay {
                    page,
                    x,
                    y,
                    size,
                    font,
                    units: vec![(text.to_string(), esc.len())],
                    end,
                    seen: false,
                })
            }
        }
    }

    /// Apply `e`, the caret where it says, as an undo step of its own.
    pub(super) fn apply(&mut self, e: Edit, cx: &mut Context<Self>) {
        self.checkpoint();
        self.typing = None;
        self.overlay = None;
        self.replace(e.range.clone(), &e.text, cx);
        if let Some(c) = self.caret.as_mut() {
            c.off = e.caret.min(self.source.len());
            c.goal_x = None;
        }
        cx.notify();
    }

    /// Backspace (`back`) or Delete: the char beside the caret, a whole macro or formula, or at
    /// a paragraph's or item's edge, joining it to the one before (after).
    fn delete(&mut self, back: bool, cx: &mut Context<Self>) {
        if self.delete_selection(cx) {
            return;
        }
        let Some(off) = self.caret.as_ref().map(|c| c.off) else {
            return;
        };
        // an empty heading goes whole, with the blank line after it
        if let Some(bi) = self.block_of(off)
            && let b = self.blocks[bi].range.clone()
            && let Some(title) = caret::heading_title(&self.source[b.clone()], b.start)
            && !glyphmap::has_visible(&self.source[title])
        {
            let ws = self.source[b.end..].len() - self.source[b.end..].trim_start().len();
            let e = Edit {
                range: b.start..b.end + ws,
                text: String::new(),
                caret: b.start,
            };
            return self.apply(e, cx);
        }
        // text typed but not typeset yet: its last char
        if back
            && let Some(o) = self.overlay.as_mut().filter(|o| o.end == off)
            && let Some((_, n)) = o.units.pop()
        {
            o.end -= n;
            o.seen = false;
            self.step("", true, false);
            self.replace(off - n..off, "", cx);
            if let Some(c) = self.caret.as_mut() {
                c.off = off - n;
            }
            if let Some(t) = self.typing.as_mut() {
                t.end = off - n;
            }
            return;
        }
        if back
            && let Some(it) = caret::item_at(&self.source, off)
            && off >= it.text.start
            && !glyphmap::has_visible(&self.source[it.text.start..off])
            && let Some(e) = caret::backspace_item(&self.source, &it, off)
        {
            return self.apply(e, cx);
        }
        if let Some((bi, page, m, i)) = self.caret_stop() {
            let gone = m.deletion(i, back, &self.source).or_else(|| {
                // the block goes on from the page before (after)
                let pages: Vec<u32> = self.pages_of(bi).collect();
                let at = pages.iter().position(|&p| p == page)?;
                let other = if back {
                    pages.get(at.checked_sub(1)?)
                } else {
                    pages.get(at + 1)
                }?;
                let m = self.map_of(bi, *other)?;
                let j = if back {
                    m.stops.len().checked_sub(1)?
                } else {
                    0
                };
                m.deletion(j, back, &self.source)
            });
            if let Some(r) = gone {
                self.overlay = None;
                self.step("", true, false);
                self.replace(r.clone(), "", cx);
                let mut at = r.start;
                // a \textbf{} with nothing left in it goes too
                if let Some(g) = caret::empty_group(&self.source, at) {
                    self.replace(g.clone(), "", cx);
                    at = g.start;
                }
                if let Some(c) = self.caret.as_mut() {
                    c.off = at;
                    c.goal_x = None;
                }
                if let Some(t) = self.typing.as_mut() {
                    t.end = at;
                }
                return;
            }
            return self.join(bi, back, cx);
        }
        // between blocks (an empty new paragraph): back to the block before
        if back && let Some(i) = self.blocks.iter().rposition(|b| b.range.end <= off) {
            let end = self.blocks[i].range.end;
            if self.source[end..off].trim().is_empty() {
                let e = Edit {
                    range: end..off,
                    text: String::new(),
                    caret: end,
                };
                self.apply(e, cx);
            }
        }
    }

    /// Backspace at a block's start (Delete at its end): one paragraph with the block before
    /// (after) when both are running text; otherwise the caret goes into that block, or it is
    /// selected (a formula, a picture).
    fn join(&mut self, bi: usize, back: bool, cx: &mut Context<Self>) {
        // the nearest block that prints something (not a \\newpage)
        let shown = |&o: &usize| !self.blocks[o].rows.is_empty();
        let other = if back {
            (0..bi).rev().find(shown)
        } else {
            (bi + 1..self.blocks.len()).find(shown)
        };
        let Some(other) = other else {
            return;
        };
        let (a, b) = if back { (other, bi) } else { (bi, other) };
        let (ra, rb) = (self.blocks[a].range.clone(), self.blocks[b].range.clone());
        let prose = |r: &Range<usize>| self.source.get(r.clone()).is_some_and(caret::is_prose);
        let between = self.source.get(ra.end..rb.start).unwrap_or("x");
        if prose(&ra) && prose(&rb) && between.trim().is_empty() {
            let e = Edit {
                range: ra.end..rb.start,
                text: " ".into(),
                caret: ra.end + usize::from(!back),
            };
            return self.apply(e, cx);
        }
        if self.is_text_block(other) {
            let page = if back {
                self.pages_of(other).last()
            } else {
                self.pages_of(other).next()
            };
            if let Some(off) = page.and_then(|p| self.edge_stop(other, p, back)) {
                self.set_caret(off, cx);
            }
        } else {
            self.caret = None;
            self.selected = Some((
                self.blocks[other].range.start,
                self.blocks[other].range.start,
            ));
            cx.notify();
        }
    }

    /// Enter: a new list item, or a paragraph break; Shift-Enter (`line`): a line break. Not
    /// in a table's cells (rows come with table editing).
    fn enter(&mut self, line: bool, cx: &mut Context<Self>) {
        self.delete_selection(cx);
        let Some(off) = self.caret.as_ref().map(|c| c.off) else {
            return;
        };
        if caret::in_table(&self.source, off) {
            return;
        }
        let item = caret::item_at(&self.source, off).filter(|it| off >= it.text.start);
        let e = match (&item, line) {
            (Some(it), false) => caret::enter_in_item(&self.source, it, off),
            (Some(it), true) => caret::line_break(&self.source, it.text.clone(), off),
            // an empty paragraph stays one: TeX has no blank paragraphs
            (None, _) => self.block_of(off).and_then(|bi| {
                let b = self.blocks[bi].range.clone();
                if line {
                    caret::line_break(&self.source, b, off)
                } else {
                    caret::enter_in_paragraph(&self.source, b, off)
                }
            }),
        };
        if let Some(e) = e {
            self.apply(e, cx);
        }
    }

    /// Tab (Shift-Tab: `out`) in a list: the item one level in (out).
    pub(super) fn tab(&mut self, out: bool, cx: &mut Context<Self>) {
        let Some(off) = self.caret.as_ref().map(|c| c.off) else {
            return;
        };
        let Some(it) = caret::item_at(&self.source, off) else {
            return;
        };
        let e = if out {
            caret::outdent_item(&self.source, &it, off)
        } else {
            caret::indent_item(&self.source, &it, off)
        };
        if let Some(e) = e {
            self.apply(e, cx);
        }
    }

    /// Escape: the caret's block selected instead, for the block commands.
    fn select_caret_block(&mut self, cx: &mut Context<Self>) {
        let Some(off) = self.caret.take().map(|c| c.off) else {
            return;
        };
        self.selected = self
            .block_of(off)
            .map(|i| (self.blocks[i].range.start, self.blocks[i].range.start));
        cx.notify();
    }

    /// ⌘M: a formula at the caret, edited as source in its paragraph's box.
    pub fn insert_math(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = format!("${}$", complete::MARK);
        self.edit_tex_at_caret(&text, window, cx);
    }

    /// The caret's paragraph opened as source, with `text` (TeX, the caret at its MARK) typed
    /// at the caret: a formula (⌘M), or a command begun with `\\`.
    pub(super) fn edit_tex_at_caret(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(off) = self.caret.as_ref().map(|c| c.off) else {
            return;
        };
        self.typing = None;
        self.overlay = None;
        if let Some(bi) = self.block_of(off) {
            let start = self.blocks[bi].range.start;
            self.activate(bi, off - start, window, cx);
            self.input
                .update(cx, |s, cx| s.replace_text_in_range(None, text, window, cx));
            return;
        }
        // in a new paragraph: a box of its own there
        let Some((page, _, y, size)) = self.caret_at() else {
            return;
        };
        self.checkpoint();
        self.sync_box(true, cx);
        self.active = Some(Active::new(
            u64::MAX,
            off..off,
            false,
            Some(Gap {
                page,
                y: y - 0.75 * size,
                sep_before: false,
            }),
        ));
        let caret = text.find(complete::MARK).unwrap_or(text.len());
        self.edit_in_box(text.replace(complete::MARK, ""), caret, window, cx);
        self.on_input(window, cx);
    }

    /// The block holding the caret moved up or down (⌃⇧↑, ⌃⇧↓), the caret with it.
    fn move_caret_block(&mut self, up: bool, cx: &mut Context<Self>) {
        let Some(off) = self.caret.as_ref().map(|c| c.off) else {
            return;
        };
        let Some(bi) = self.block_of(off) else {
            return;
        };
        let rel = off - self.blocks[bi].range.start;
        self.move_blocks(bi, bi, up, cx);
        if let Some((start, _)) = self.selected.take() {
            self.caret = Some(Caret {
                off: start + rel,
                anchor: None,
                goal_x: None,
                marked: None,
            });
        }
    }

    /// Caught up: the typed text is on the pages, so the overlay goes.
    pub(super) fn settle_overlay(&mut self) {
        let Some(o) = &self.overlay else { return };
        let fresh = self
            .images
            .get(&o.page)
            .is_some_and(|im| im.key.0 == self.versions.get(&o.page).copied().unwrap_or(0));
        // the page shows the typed text once its paragraph is typeset, even while a full run
        // (a heading's, for the contents) is still going
        if o.seen && self.spans_pending == 0 && fresh {
            self.overlay = None;
        }
    }

    pub(super) fn overlay_seen(&mut self) {
        if let Some(o) = self.overlay.as_mut() {
            o.seen = true;
        }
    }

    /// Width in px of `text` in `face` at `size` px.
    fn text_width(face: &Font, size: f32, text: &str, cx: &App) -> f32 {
        let ts = cx.text_system();
        let id = ts.resolve_font(face);
        text.chars()
            .map(|c| {
                ts.advance(id, px(size), c)
                    .map_or(size * 0.5, |a| f32::from(a.width))
            })
            .sum()
    }

    /// The caret on page `n`, with typed text not typeset yet and an input method's text being
    /// composed (underlined), drawn where they go.
    pub(super) fn render_caret(
        &self,
        n: u32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let c = self.caret.as_ref().filter(|_| self.active.is_none())?;
        if !self.focus.is_focused(window) {
            return None;
        }
        let overlay = self.overlay.as_ref().filter(|o| o.end == c.off);
        let (page, x, y, size, desc) = match overlay {
            Some(o) => (o.page, o.x, o.y, o.size, o.font.clone()),
            None => {
                let (p, x, y, s) = self.caret_at()?;
                (p, x, y, s, self.caret_font())
            }
        };
        if page != n {
            return None;
        }
        let fs = page_px(size, self.zoom);
        let face = self
            .doc_font(desc.as_ref(), cx)
            .unwrap_or_else(|| font(MONO));
        let typed: String = overlay.map_or(String::new(), |o| {
            o.units.iter().map(|u| u.0.as_str()).collect()
        });
        let (marked, sel) = c.marked.clone().unwrap_or_default();
        // the caret inside composed text: where the input method's selection ends
        let in_marked: String = match sel {
            Some(r) => {
                let s: Vec<u16> = marked.encode_utf16().collect();
                String::from_utf16_lossy(&s[..r.end.min(s.len())])
            }
            None => marked.clone(),
        };
        let w_typed = Self::text_width(&face, fs, &typed, cx);
        let w_caret = w_typed + Self::text_width(&face, fs, &in_marked, cx);
        // on TeX's baseline: GPUI sets a line's baseline its ascent below the line's top, after
        // half the line height the font's ascent and descent leave
        let lh = fs * 1.25;
        let (ascent, descent) = {
            let ts = cx.text_system();
            let id = ts.resolve_font(&face);
            (
                f32::from(ts.ascent(id, px(fs))),
                f32::from(ts.descent(id, px(fs))).abs(),
            )
        };
        let baseline = (lh - ascent - descent) / 2.0 + ascent;
        let left = page_px(x, self.zoom);
        let top = page_px(y, self.zoom) - baseline;
        // render's ink on its paper
        let fg = rgb(if self.dark_pages(cx) {
            0xe0e0e0
        } else {
            0x000000
        });
        let paper = rgb(if self.dark_pages(cx) {
            0x000000
        } else {
            0xffffff
        });
        let bounds = self.caret_bounds.clone();
        let text = (!typed.is_empty() || !marked.is_empty()).then(|| {
            div()
                .absolute()
                .left(px(left))
                .top(px(top))
                .h(px(lh))
                .line_height(px(lh))
                .whitespace_nowrap()
                .font_family(face.family.clone())
                .font_weight(face.weight)
                .when(face.style == FontStyle::Italic, |d| d.italic())
                .text_size(px(fs))
                .text_color(fg)
                // shortcut: hides what it covers and does not wrap; the pages catch up in ms
                .bg(paper)
                .child(typed.clone())
                .child(div().underline().child(marked.clone()))
        });
        let caret = div()
            .absolute()
            .left(px(left + w_caret))
            .top(px(page_px(y, self.zoom) - 0.8 * fs))
            .w(px(1.5))
            .h(px(fs * 1.05))
            .when(self.caret_on, |c| c.bg(fg))
            .child(
                canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            );
        // over the whole page: an absolute element is placed in its parent
        Some(
            div()
                .absolute()
                .inset_0()
                .children(text)
                .child(caret)
                .into_any_element(),
        )
    }
}

/// The source as an input method sees it: with the composed text at the caret.
fn with_marked(src: &str, c: &Caret) -> String {
    match &c.marked {
        Some((m, _)) => format!("{}{m}{}", &src[..c.off], &src[c.off..]),
        None => src.to_string(),
    }
}

/// What the input handler is asked, on stderr, with `RTD_LOG_INPUT` set (to see what macOS and
/// input methods send).
fn log_input(what: std::fmt::Arguments) {
    if std::env::var_os("RTD_LOG_INPUT").is_some() {
        eprintln!("input: {what}");
    }
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Byte offset in `s` of UTF-16 offset `u`.
fn byte_of(s: &str, u: usize) -> usize {
    let mut n = 0;
    for (i, ch) in s.char_indices() {
        if n >= u {
            return i;
        }
        n += ch.len_utf16();
    }
    s.len()
}

impl EntityInputHandler for Doc {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        log_input(format_args!("text for {range:?}"));
        let c = self.caret.as_ref()?;
        let text = with_marked(&self.source, c);
        let (a, b) = (byte_of(&text, range.start), byte_of(&text, range.end));
        *adjusted = Some(utf16_len(&text[..a])..utf16_len(&text[..b]));
        Some(text[a..b].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let c = self.caret.as_ref()?;
        let at = utf16_len(&self.source[..c.off])
            + c.marked
                .as_ref()
                .map_or(0, |(m, s)| s.as_ref().map_or(utf16_len(m), |s| s.end));
        Some(UTF16Selection {
            range: at..at,
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let c = self.caret.as_ref()?;
        let (m, _) = c.marked.as_ref()?;
        let at = utf16_len(&self.source[..c.off]);
        Some(at..at + utf16_len(m))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        log_input(format_args!("unmark"));
        if let Some((m, _)) = self.caret.as_mut().and_then(|c| c.marked.take()) {
            self.type_text(&m, true, cx);
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        log_input(format_args!(
            "replace {range:?} with {text:?}; caret {:?} selection {:?}",
            self.caret
                .as_ref()
                .map(|c| (c.off, c.anchor, c.marked.clone())),
            self.selection_range()
        ));
        let Some(c) = self.caret.as_mut() else { return };
        if c.marked.take().is_some() {
            return self.type_text(text, true, cx);
        }
        // a backslash starts a command: nobody types one to print it, so TeX it is, in the
        // paragraph's source box (with its completions) at the caret
        if text.starts_with('\\') {
            self.delete_selection(cx);
            let text = format!("{text}{}", complete::MARK);
            return self.edit_tex_at_caret(&text, window, cx);
        }
        // a range other than the caret: the accent menu replacing the char before it
        if let Some(r) = range {
            let (a, b) = (byte_of(&self.source, r.start), byte_of(&self.source, r.end));
            if a < b && b <= self.source.len() {
                self.step(text, false, true);
                let esc = caret::escape(text);
                self.replace(a..b, &esc, cx);
                if let Some(c) = self.caret.as_mut() {
                    c.off = a + esc.len();
                }
                return;
            }
        }
        self.type_text(text, false, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        new_text: &str,
        new_selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        log_input(format_args!("mark {new_text:?} selecting {new_selected:?}"));
        let Some(c) = self.caret.as_mut() else { return };
        c.marked = (!new_text.is_empty()).then(|| (new_text.to_string(), new_selected));
        self.restart_blink(cx);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        self.caret_bounds.get()
    }

    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

/// The caret keys, on the pages while the caret is there.
pub(super) fn caret_actions(view: Stateful<Div>, cx: &mut Context<Doc>) -> Stateful<Div> {
    view.on_action(cx.listener(|d, _: &CaretLeft, _, cx| {
        if !d.collapse(false, cx) {
            d.move_h(-1, cx)
        }
    }))
    .on_action(cx.listener(|d, _: &CaretRight, _, cx| {
        if !d.collapse(true, cx) {
            d.move_h(1, cx)
        }
    }))
    .on_action(cx.listener(|d, _: &SelectLeft, _, cx| d.extending(cx, |d, cx| d.move_h(-1, cx))))
    .on_action(cx.listener(|d, _: &SelectRight, _, cx| d.extending(cx, |d, cx| d.move_h(1, cx))))
    .on_action(cx.listener(|d, _: &SelectUp, _, cx| d.extending(cx, |d, cx| d.move_v(-1, cx))))
    .on_action(cx.listener(|d, _: &SelectDown, _, cx| d.extending(cx, |d, cx| d.move_v(1, cx))))
    .on_action(
        cx.listener(|d, _: &SelectWordLeft, _, cx| d.extending(cx, |d, cx| d.move_word(-1, cx))),
    )
    .on_action(
        cx.listener(|d, _: &SelectWordRight, _, cx| d.extending(cx, |d, cx| d.move_word(1, cx))),
    )
    .on_action(cx.listener(|d, _: &SelectLineStart, _, cx| {
        d.extending(cx, |d, cx| d.move_line_edge(false, cx))
    }))
    .on_action(cx.listener(|d, _: &SelectLineEnd, _, cx| {
        d.extending(cx, |d, cx| d.move_line_edge(true, cx))
    }))
    .on_action(cx.listener(|d, _: &SelectDocStart, _, cx| {
        d.extending(cx, |d, cx| d.move_doc_edge(false, cx))
    }))
    .on_action(
        cx.listener(|d, _: &SelectDocEnd, _, cx| {
            d.extending(cx, |d, cx| d.move_doc_edge(true, cx))
        }),
    )
    .on_action(cx.listener(|d, _: &CaretUp, _, cx| d.move_v(-1, cx)))
    .on_action(cx.listener(|d, _: &CaretDown, _, cx| d.move_v(1, cx)))
    .on_action(cx.listener(|d, _: &CaretWordLeft, _, cx| d.move_word(-1, cx)))
    .on_action(cx.listener(|d, _: &CaretWordRight, _, cx| d.move_word(1, cx)))
    .on_action(cx.listener(|d, _: &CaretLineStart, _, cx| d.move_line_edge(false, cx)))
    .on_action(cx.listener(|d, _: &CaretLineEnd, _, cx| d.move_line_edge(true, cx)))
    .on_action(cx.listener(|d, _: &CaretDocStart, _, cx| d.move_doc_edge(false, cx)))
    .on_action(cx.listener(|d, _: &CaretDocEnd, _, cx| d.move_doc_edge(true, cx)))
    .on_action(cx.listener(|d, _: &Backspace, _, cx| d.delete(true, cx)))
    .on_action(cx.listener(|d, _: &DeleteForward, _, cx| d.delete(false, cx)))
    .on_action(cx.listener(|d, _: &NewParagraph, _, cx| d.enter(false, cx)))
    .on_action(cx.listener(|d, _: &LineBreak, _, cx| d.enter(true, cx)))
    .on_action(cx.listener(|d, _: &Indent, _, cx| d.tab(false, cx)))
    .on_action(cx.listener(|d, _: &Outdent, _, cx| d.tab(true, cx)))
    .on_action(cx.listener(|d, _: &SelectCaretBlock, _, cx| d.select_caret_block(cx)))
    .on_action(cx.listener(|d, _: &InsertMath, w, cx| d.insert_math(w, cx)))
    .on_action(cx.listener(|d, _: &FormatBold, w, cx| d.format(FormatOp::Inline("textbf"), w, cx)))
    .on_action(
        cx.listener(|d, _: &FormatItalic, w, cx| d.format(FormatOp::Inline("textit"), w, cx)),
    )
    .on_action(
        cx.listener(|d, _: &FormatUnderline, w, cx| d.format(FormatOp::Inline("underline"), w, cx)),
    )
    .on_action(cx.listener(|d, _: &FontBigger, w, cx| d.format(FormatOp::Grow(1), w, cx)))
    .on_action(cx.listener(|d, _: &FontSmaller, w, cx| d.format(FormatOp::Grow(-1), w, cx)))
    .on_action(cx.listener(|d, _: &Heading1, w, cx| {
        d.format(FormatOp::Style(Style::Heading("section")), w, cx)
    }))
    .on_action(cx.listener(|d, _: &Heading2, w, cx| {
        d.format(FormatOp::Style(Style::Heading("subsection")), w, cx)
    }))
    .on_action(cx.listener(|d, _: &Heading3, w, cx| {
        d.format(FormatOp::Style(Style::Heading("subsubsection")), w, cx)
    }))
    .on_action(cx.listener(|d, _: &AlignLeft, w, cx| d.format(FormatOp::Align(Align::Left), w, cx)))
    .on_action(
        cx.listener(|d, _: &AlignCenter, w, cx| d.format(FormatOp::Align(Align::Center), w, cx)),
    )
    .on_action(
        cx.listener(|d, _: &AlignRight, w, cx| d.format(FormatOp::Align(Align::Right), w, cx)),
    )
    .on_action(
        cx.listener(|d, _: &AlignJustify, w, cx| d.format(FormatOp::Align(Align::Justify), w, cx)),
    )
    .on_action(cx.listener(|d, _: &MoveParagraphUp, _, cx| {
        if d.caret.is_some() {
            d.move_caret_block(true, cx)
        } else {
            cx.propagate()
        }
    }))
    .on_action(cx.listener(|d, _: &MoveParagraphDown, _, cx| {
        if d.caret.is_some() {
            d.move_caret_block(false, cx)
        } else {
            cx.propagate()
        }
    }))
}

#[cfg(test)]
pub(super) mod tests {
    use super::{Doc, caret};
    use crate::document::{self, glyphmap};
    use crate::engine;
    use crate::ui::settings::AppSettings;
    use gpui_kit::{Entity, EntityInputHandler, TestAppContext, VisualTestContext, Window};
    use std::time::{Duration, Instant};

    /// Run the app until `done` holds for the doc (rtex answers from its own process).
    pub(in crate::ui::editor) fn wait(
        cx: &mut VisualTestContext,
        doc: &Entity<Doc>,
        what: &str,
        done: impl Fn(&Doc) -> bool,
    ) {
        let until = Instant::now() + Duration::from_secs(60);
        loop {
            cx.run_until_parked();
            if cx.read(|cx| done(doc.read(cx))) {
                return;
            }
            assert!(Instant::now() < until, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Run until rtex has typeset everything and stays so for a while: LaTeX may run again
    /// (references) just after a layout comes in.
    pub(in crate::ui::editor) fn calm(cx: &mut VisualTestContext, doc: &Entity<Doc>) {
        let settled = |d: &Doc| d.spans_pending == 0 && !d.compiling;
        let mut since = Instant::now();
        loop {
            wait(cx, doc, "the edit typeset", settled);
            std::thread::sleep(Duration::from_millis(30));
            cx.run_until_parked();
            if !cx.read(|cx| settled(doc.read(cx))) {
                since = Instant::now();
            } else if since.elapsed() > Duration::from_millis(300) {
                return;
            }
        }
    }

    fn typed(doc: &Doc, bi: usize) -> String {
        let b = &doc.blocks[bi];
        let rows: Vec<_> = b.rows.iter().map(|r| &r.row).collect();
        glyphmap::glyph_text_of(&rows).text.iter().collect()
    }

    /// Typing on the pages of a real document through rtex: `RTD_TYPE=/abs/main.tex cargo test
    /// --release type_on_pages -- --ignored --nocapture` (use a copy: rtex builds next to it).
    #[gpui_kit::test]
    #[ignore]
    fn type_on_pages(cx: &mut TestAppContext) {
        let file = std::env::var("RTD_TYPE").expect("RTD_TYPE=/abs/path/main.tex");
        let path = std::path::PathBuf::from(&file);
        let (root, main) = (
            path.parent().unwrap().to_path_buf(),
            path.file_name().unwrap().to_string_lossy().to_string(),
        );
        // rtex answers from its own thread
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        engine::configure("", "/Library/TeX/texbin");
        let (doc, cx) = cx.add_window_view(|window: &mut Window, cx| {
            Doc::new(root, main, Default::default(), window, cx)
        });
        wait(cx, &doc, "the first layout", |d| {
            d.spans_pending == 0
                && !d.compiling
                && d.blocks.iter().filter(|b| !b.rows.is_empty()).count() > 3
        });
        // a plain paragraph: the caret before its second word
        let (bi, off) = cx.read(|cx| {
            let d = doc.read(cx);
            let bi = (0..d.blocks.len())
                .find(|&b| {
                    d.is_text_block(b)
                        && d.source[d.blocks[b].range.clone()].split(' ').count() > 6
                        && document::caret::is_prose(&d.source[d.blocks[b].range.clone()])
                        && !d.source[d.blocks[b].range.clone()].starts_with('\\')
                })
                .expect("a plain paragraph");
            let b = &d.blocks[bi];
            let src = &d.source[b.range.clone()];
            (bi, b.range.start + src.find(' ').unwrap() + 1)
        });
        let before = cx.read(|cx| doc.read(cx).source.clone());
        let page = cx.read(|cx| doc.read(cx).blocks[bi].rows[0].page);
        doc.update(cx, |d, cx| {
            let m = d.map_of(bi, page).unwrap();
            assert!(
                m.stops.iter().any(|s| s.off == off),
                "a stop before the second word"
            );
            d.set_caret(off, cx);
        });
        // the caret is drawn, with what was typed until the page catches up
        doc.update_in(cx, |d, window, cx| d.focus.focus(window, cx));
        let t0 = Instant::now();
        doc.update_in(cx, |d, window, cx| {
            for ch in "Quick 50% ".chars() {
                d.replace_text_in_range(None, &ch.to_string(), window, cx);
            }
        });
        let src = cx.read(|cx| doc.read(cx).source.clone());
        assert!(src.contains("Quick 50\\% "), "typed text is escaped");
        assert_eq!(
            cx.read(|cx| doc.read(cx).caret.as_ref().unwrap().off),
            off + 11
        );
        wait(cx, &doc, "the typeset paragraph", |d| {
            d.spans_pending == 0 && typed(d, bi).contains("Quick 50% ")
        });
        println!("typed → typeset in {:?}", t0.elapsed());
        wait(cx, &doc, "the overlay to go", |d| d.overlay.is_none());
        // the caret is drawn on its page, at its line
        doc.update(cx, |d, cx| {
            d.caret_bounds.set(None);
            cx.notify();
        });
        wait(cx, &doc, "the caret drawn", |d| {
            d.caret_bounds.get().is_some()
        });
        // typed text shows in the document's font, as the text before the caret is set: its
        // OpenType twin here (the test text system has no fonts to register it with)
        let twin = |d: &Doc| {
            let name = d.caret_font().and_then(|f| f.name).unwrap_or_default();
            let file = super::super::source_box::latin_modern(&name).expect("a Latin Modern twin");
            assert!(
                engine::kpsewhich(&file).is_some(),
                "{file} is in the TeX tree"
            );
            file
        };
        assert!(cx.read(|cx| twin(doc.read(cx))).ends_with("-regular.otf"));
        doc.update(cx, |d, cx| {
            // in the body (the preamble's \\textbf is not on the pages)
            let at = d.source.find("\\textbf{Mistakes").unwrap() + "\\textbf{".len() + 2;
            d.set_caret(at, cx);
        });
        assert!(
            cx.read(|cx| twin(doc.read(cx))).ends_with("-bold.otf"),
            "bold inside \\textbf"
        );
        // it blinks, and a move shows it again at once
        doc.update(cx, |d, cx| d.set_caret(off + 11, cx));
        assert!(cx.read(|cx| doc.read(cx).caret_on));
        cx.executor().advance_clock(Duration::from_millis(600));
        cx.run_until_parked();
        assert!(!cx.read(|cx| doc.read(cx).caret_on), "off after a blink");
        doc.update(cx, |d, cx| d.set_caret(off + 11, cx));
        assert!(cx.read(|cx| doc.read(cx).caret_on), "on again after a move");
        cx.read(|cx| {
            let d = doc.read(cx);
            let b = d.caret_bounds.get().unwrap();
            let (page, _, y, _) = d.caret_at().unwrap();
            let pages = d.page_bounds.take();
            d.page_bounds.set(pages.clone());
            let (_, pb) = pages.iter().find(|(n, _)| *n == page).unwrap();
            let want = pb.origin.y + gpui_kit::px(super::page_px(y, d.zoom));
            assert!(
                (b.origin.y - want).abs() < gpui_kit::px(30.0) && b.origin.x > pb.origin.x,
                "caret at {b:?}, its line at y {want:?} on page {pb:?}"
            );
        });
        // the caret is on its text again, after what was typed
        cx.read(|cx| {
            let d = doc.read(cx);
            let (_, _, m, i) = d.caret_stop().unwrap();
            assert_eq!(m.stops[i].off, off + 11);
        });
        // one undo step per word: "Quick ", "50% "
        doc.update_in(cx, |d, window, cx| d.undo(window, cx));
        assert!(cx.read(|cx| doc.read(cx).source.contains("Quick ")));
        doc.update_in(cx, |d, window, cx| d.undo(window, cx));
        assert_eq!(cx.read(|cx| doc.read(cx).source.clone()), before);
        // Enter splits the paragraph; Backspace joins it again
        doc.update(cx, |d, cx| d.set_caret(off, cx));
        doc.update(cx, |d, cx| d.enter(false, cx));
        let split = cx.read(|cx| doc.read(cx).source.clone());
        assert_eq!(split.len(), before.len() + 2);
        assert_eq!(&split[off..off + 2], "\n\n");
        wait(cx, &doc, "two paragraphs", |d| {
            d.spans_pending == 0 && d.block_of(off + 2) == Some(bi + 1)
        });
        doc.update(cx, |d, cx| d.delete(true, cx));
        let joined = cx.read(|cx| doc.read(cx).source.clone());
        assert_eq!(joined.replace("  ", " "), before.replace("  ", " "));
        // an input method composes at the caret without touching the source, then commits
        let base = cx.read(|cx| doc.read(cx).source.clone());
        doc.update_in(cx, |d, window, cx| {
            d.replace_and_mark_text_in_range(None, "ni", None, window, cx);
            assert_eq!(d.source, base);
            assert!(d.marked_text_range(window, cx).is_some());
            d.replace_text_in_range(None, "你", window, cx);
            assert!(d.marked_text_range(window, cx).is_none());
        });
        assert_eq!(
            cx.read(|cx| doc.read(cx).source.len()),
            base.len() + "你".len()
        );
        doc.update_in(cx, |d, window, cx| d.undo(window, cx));
        assert_eq!(cx.read(|cx| doc.read(cx).source.clone()), base);
        wait(cx, &doc, "the undo typeset", |d| d.spans_pending == 0);
        // a backslash opens the paragraph's source with the command begun at the caret
        let base = cx.read(|cx| doc.read(cx).source.clone());
        doc.update_in(cx, |d, window, cx| {
            d.set_caret(off, cx);
            d.replace_text_in_range(None, "\\", window, cx);
        });
        cx.run_until_parked();
        doc.update_in(cx, |d, window, cx| {
            assert!(d.active.is_some(), "the source box is open");
            let v = d.input.read(cx).value().to_string();
            let cur = d.input.read(cx).cursor();
            assert!(!v.contains(super::complete::MARK), "the caret mark is gone");
            assert_eq!(&v[cur - 1..cur], "\\", "the caret after the backslash");
            assert!(!d.source.contains("textbackslash"));
            // a lone backslash waits in the box: it is half a command
            assert_eq!(d.source, base, "nothing in the source yet");
            d.leave(window, cx);
        });
        doc.update_in(cx, |d, window, cx| {
            while d.source != base {
                d.undo(window, cx);
            }
        });
        wait(cx, &doc, "the undo", |d| {
            d.spans_pending == 0 && !d.compiling
        });
        // Enter at the end of a list item adds one; Tab nests it; undo takes both back
        let item = cx.read(|cx| {
            let d = doc.read(cx);
            // the end of a list's last item, which has a line of its own for a new item
            d.source.find("\n\\end{itemize}").unwrap()
        });
        let base = cx.read(|cx| doc.read(cx).source.clone());
        // where the caret was, at the end of the item before
        let (_, x0, y0, _) = doc.update(cx, |d, cx| {
            d.set_caret(item, cx);
            let at = d.caret_at().unwrap();
            d.enter(false, cx);
            at
        });
        let one = cx.read(|cx| doc.read(cx).source.clone());
        assert!(
            one[item..].starts_with("\n    \\item "),
            "{:?}",
            &one[item..item + 20]
        );
        // the caret stands on the new item's line, not after the item before, at once and
        // once it is typeset
        let (_, _, y_now, _) = cx.read(|cx| doc.read(cx).caret_at().unwrap());
        assert!(y_now > y0, "below the item before at once: {y0} → {y_now}");
        calm(cx, &doc);
        let (_, x1, y1, size) = cx.read(|cx| doc.read(cx).caret_at().unwrap());
        assert!(y1 > y0 + size, "a line lower: {y0} → {y1}");
        assert!(x1 < x0, "at the line's start: {x0} → {x1}");
        // a click on the empty item's line puts the caret back in it
        let (bi, page) = cx.read(|cx| {
            let d = doc.read(cx);
            let bi = d.block_of(item).unwrap();
            (bi, d.blocks[bi].rows[0].page)
        });
        let want = cx.read(|cx| doc.read(cx).caret.as_ref().unwrap().off);
        doc.update(cx, |d, cx| {
            d.set_caret(item, cx);
            assert!(d.caret_click(bi, page, x1 + 20.0 * 65536.0, y1 - 3.0 * 65536.0, cx));
            assert_eq!(d.caret.as_ref().unwrap().off, want, "in the empty item");
        });
        // and the arrows stop there, from the item before and back
        doc.update(cx, |d, cx| {
            d.set_caret(item, cx);
            d.move_v(1, cx);
            assert_eq!(d.caret.as_ref().unwrap().off, want, "↓ into the empty item");
            d.move_v(-1, cx);
            assert!(d.caret.as_ref().unwrap().off < item + 1, "↑ out of it");
            d.set_caret(item, cx);
            d.move_h(1, cx);
            assert_eq!(d.caret.as_ref().unwrap().off, want, "→ into the empty item");
            d.move_h(-1, cx);
            assert_eq!(d.caret.as_ref().unwrap().off, item, "← back");
            d.set_caret(want, cx);
        });
        doc.update(cx, |d, cx| d.type_text("x", false, cx));
        doc.update(cx, |d, cx| d.tab(false, cx));
        assert!(
            cx.read(|cx| doc.read(cx).source[item..].contains("\\begin{itemize}\n      \\item x"))
        );
        for _ in 0..3 {
            doc.update_in(cx, |d, window, cx| d.undo(window, cx));
        }
        assert_eq!(cx.read(|cx| doc.read(cx).source.clone()), base);
        // Enter at the start of a heading: a new paragraph before it, the caret there
        let heading = cx.read(|cx| doc.read(cx).source.find("\\subsection*{Which").unwrap());
        let start = heading + "\\subsection*{".len();
        let (hp, _, hy, _) = doc.update(cx, |d, cx| {
            d.set_caret(start, cx);
            let at = d.caret_at().unwrap();
            d.enter(false, cx);
            at
        });
        calm(cx, &doc);
        let (p, _, y, _) = cx.read(|cx| doc.read(cx).caret_at().unwrap());
        assert!((p, y) < (hp, hy), "the caret is above the heading");
        doc.update(cx, |d, cx| d.type_text("New", false, cx));
        let s = cx.read(|cx| doc.read(cx).source.clone());
        assert!(s[heading..].starts_with("New\n\n\\subsection*{Which"));
        doc.update_in(cx, |d, window, cx| {
            d.undo(window, cx);
            d.undo(window, cx);
        });
        wait(cx, &doc, "the undo", |d| {
            d.spans_pending == 0 && !d.compiling
        });
        // a selected word: highlighted, typed over in one undo step, copied and pasted as TeX
        let before = cx.read(|cx| doc.read(cx).source.clone());
        doc.update(cx, |d, cx| {
            d.set_caret(off, cx);
            d.extending(cx, |d, cx| d.move_word(1, cx));
            let r = d.selection_range().expect("a word selected");
            assert!(
                !d.source[r.clone()].contains(' '),
                "{:?}",
                &d.source[r.clone()]
            );
            let on = d.blocks[d.block_of(off).unwrap()].rows[0].page;
            assert!(!d.render_selection(on).is_empty(), "it is highlighted");
            assert!(d.copy_selection(cx));
            d.type_text("W", false, cx);
            assert!(d.selection_range().is_none());
        });
        let word = cx.read(|cx| cx.read_from_clipboard().and_then(|c| c.text()).unwrap());
        assert!(
            cx.read(|cx| doc.read(cx).source[off..].starts_with(&format!(
                "W{}",
                &before[off + word.len()..off + word.len() + 3]
            )))
        );
        doc.update_in(cx, |d, window, cx| d.undo(window, cx));
        assert_eq!(
            cx.read(|cx| doc.read(cx).source.clone()),
            before,
            "one undo step"
        );
        calm(cx, &doc);
        doc.update(cx, |d, cx| {
            d.set_caret(off, cx);
            d.paste_at_caret(false, cx);
        });
        assert!(cx.read(|cx| doc.read(cx).source[off..].starts_with(&format!("{word}{word}"))));
        doc.update_in(cx, |d, window, cx| d.undo(window, cx));
        assert_eq!(cx.read(|cx| doc.read(cx).source.clone()), before);
        calm(cx, &doc);
        // the caret moves through the text and back
        wait(cx, &doc, "one paragraph", |d| d.spans_pending == 0);
        doc.update(cx, |d, cx| {
            d.set_caret(off, cx);
            let start = off;
            d.move_h(1, cx);
            assert_eq!(d.caret.as_ref().unwrap().off, start + 1);
            d.move_h(-1, cx);
            assert_eq!(d.caret.as_ref().unwrap().off, start);
            d.move_v(1, cx);
            d.move_v(-1, cx);
        });
    }

    /// Every edit at the start, middle and end of every block of a real document, through
    /// rtex: none may break the TeX, lose the caret or fail to undo. `RTD_TYPE=/abs/copy.tex
    /// cargo test --release edit_everywhere -- --ignored --nocapture`.
    #[gpui_kit::test]
    #[ignore]
    fn edit_everywhere(cx: &mut TestAppContext) {
        let file = std::env::var("RTD_TYPE").expect("RTD_TYPE=/abs/path/main.tex");
        let path = std::path::PathBuf::from(&file);
        let (root, main) = (
            path.parent().unwrap().to_path_buf(),
            path.file_name().unwrap().to_string_lossy().to_string(),
        );
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        engine::configure("", "/Library/TeX/texbin");
        let (doc, cx) = cx.add_window_view(|window: &mut Window, cx| {
            Doc::new(root, main, Default::default(), window, cx)
        });
        let settled = |d: &Doc| d.spans_pending == 0 && !d.compiling;
        wait(cx, &doc, "the first layout", |d| {
            settled(d) && d.blocks.iter().filter(|b| !b.rows.is_empty()).count() > 3
        });
        let base = cx.read(|cx| doc.read(cx).source.clone());
        // the spots to edit at: each text block's first, middle and last stop
        let spots: Vec<(usize, String, [usize; 3])> = cx.read(|cx| {
            let d = doc.read(cx);
            (0..d.blocks.len())
                .filter(|&b| d.is_text_block(b))
                .filter_map(|b| {
                    let page = d.blocks[b].rows.first()?.page;
                    let m = d.map_of(b, page)?;
                    let st: Vec<usize> = m.stops.iter().map(|s| s.off).collect();
                    let name: String = d.source[d.blocks[b].range.clone()]
                        .chars()
                        .take(40)
                        .collect();
                    Some((b, name, [st[0], st[st.len() / 2], *st.last()?]))
                })
                .collect()
        });
        type Op = (
            &'static str,
            usize,
            fn(&mut Doc, &mut gpui_kit::Context<Doc>),
        );
        let ops: [Op; 9] = [
            ("select-down-delete", 1, |d, cx| {
                d.extending(cx, |d, cx| {
                    d.move_v(1, cx);
                    d.move_v(1, cx);
                });
                d.delete(true, cx);
            }),
            ("select-word-type", 1, |d, cx| {
                d.extending(cx, |d, cx| d.move_word(1, cx));
                d.type_text("Zq", false, cx);
            }),
            ("type", 1, |d, cx| d.type_text("Zq", false, cx)),
            ("enter@start", 0, |d, cx| d.enter(false, cx)),
            ("enter@mid", 1, |d, cx| d.enter(false, cx)),
            ("enter@end", 2, |d, cx| d.enter(false, cx)),
            ("shift-enter@mid", 1, |d, cx| d.enter(true, cx)),
            ("backspace@start", 0, |d, cx| d.delete(true, cx)),
            ("delete@end", 2, |d, cx| d.delete(false, cx)),
        ];
        let mut failures = vec![];
        let t0 = Instant::now();
        // RTD_SWEEP_ONLY=7:enter@end runs one edit and shows it
        let only = std::env::var("RTD_SWEEP_ONLY").ok();
        for (bi, name, at) in &spots {
            for (op, pos, f) in &ops {
                if only
                    .as_ref()
                    .is_some_and(|o| !format!("{bi}:{op}").starts_with(o.as_str()))
                {
                    continue;
                }
                let off = at[*pos];
                let (before, in_item) = doc.update(cx, |d, cx| {
                    d.status.clear();
                    d.set_caret(off, cx);
                    let at = d.caret_at();
                    let item =
                        caret::item_at(&d.source, off).is_some_and(|it| off >= it.text.start);
                    f(d, cx);
                    (at, item)
                });
                calm(cx, &doc);
                let (changed, status, caret) = cx.read(|cx| {
                    let d = doc.read(cx);
                    // a caret, or (Backspace reaching a picture) that block selected, on the page
                    let shown = d.caret_at().is_some()
                        || d.caret.is_none()
                            && d.selection()
                                .is_some_and(|(lo, _)| !d.blocks[lo].rows.is_empty());
                    (d.source != base, d.status.clone(), shown)
                });
                let what = format!("block {bi} {name:?}: {op}");
                if status.contains("LaTeX") {
                    let edit = cx.read(|cx| {
                        let d = doc.read(cx);
                        let r = d.last_edit.clone();
                        d.source[r.start.saturating_sub(20)..(r.end + 20).min(d.source.len())]
                            .to_string()
                    });
                    failures.push(format!("{what} → {status} (around {edit:?})"));
                }
                // Enter moves the caret up into a new paragraph before the text (at its start),
                // or down to the new line (in its middle, at its end)
                let after = cx.read(|cx| doc.read(cx).caret_at());
                if only.is_some() {
                    cx.read(|cx| {
                        let d = doc.read(cx);
                        let c = d.caret.as_ref().unwrap().off;
                        let r = d.last_edit.clone();
                        println!("edit {r:?} caret {c} before {before:?} after {after:?}");
                        println!(
                            "around: {:?}",
                            &d.source[r.start.saturating_sub(60)..(r.end + 60).min(d.source.len())]
                        );
                        let bi = d.block_of(c);
                        println!(
                            "caret block {bi:?} stop {:?}",
                            d.caret_stop().map(|(b, p, m, i)| (b, p, m.stops[i]))
                        );
                        if let Some(bi) = bi {
                            for r in &d.blocks[bi].rows {
                                println!(
                                    "  row p{} top {:.1} bottom {:.1}",
                                    r.page,
                                    r.bbox[1] / 65536.0,
                                    r.bbox[3] / 65536.0
                                );
                            }
                        }
                    });
                }
                if changed
                    && op.starts_with("enter")
                    && let (Some(b), Some(a)) = (before, after)
                {
                    let (b, a) = ((b.0, b.2), (a.0, a.2));
                    // before an item's text, a new item opens above and the caret stays with
                    // the text, which moves down
                    let ok = if *pos == 0 && !in_item { a < b } else { a > b };
                    if !ok {
                        failures.push(format!("{what} → caret went from {b:?} to {a:?}"));
                    }
                }
                if !caret {
                    failures.push(format!("{what} → no caret, nothing selected"));
                }
                if changed {
                    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
                    calm(cx, &doc);
                    if cx.read(|cx| doc.read(cx).source != base) {
                        failures.push(format!("{what} → undo did not restore the source"));
                        let b = base.clone();
                        doc.update(cx, |d, cx| {
                            d.replace(0..d.source.len(), &b, cx);
                        });
                        calm(cx, &doc);
                    }
                }
            }
        }
        println!(
            "{} edits at {} spots in {:?}",
            spots.len() * ops.len(),
            spots.len(),
            t0.elapsed()
        );
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    /// Enter before a heading, then a command typed there: `\\title{AAA}` goes in as TeX, and
    /// nothing reaches the source (or moves the pages) while it is half typed.
    /// `RTD_TYPE=/abs/copy.tex cargo test --release command_in_a_new_paragraph -- --ignored`.
    #[gpui_kit::test]
    #[ignore]
    fn command_in_a_new_paragraph(cx: &mut TestAppContext) {
        let file = std::env::var("RTD_TYPE").expect("RTD_TYPE=/abs/path/main.tex");
        let path = std::path::PathBuf::from(&file);
        let (root, main) = (
            path.parent().unwrap().to_path_buf(),
            path.file_name().unwrap().to_string_lossy().to_string(),
        );
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        engine::configure("", "/Library/TeX/texbin");
        let (doc, cx) = cx.add_window_view(|window: &mut Window, cx| {
            Doc::new(root, main, Default::default(), window, cx)
        });
        wait(cx, &doc, "the first layout", |d| {
            d.spans_pending == 0 && d.blocks.iter().filter(|b| !b.rows.is_empty()).count() > 3
        });
        calm(cx, &doc);
        let base = cx.read(|cx| doc.read(cx).source.clone());
        // a click before "Which one?", Enter: a new paragraph above it
        let (bi, page, x, y) = cx.read(|cx| {
            let d = doc.read(cx);
            let w = d.source.find("{Which one").unwrap() + 1;
            let bi = d.block_of(w).unwrap();
            let page = d.blocks[bi].rows[0].page;
            let m = d.map_of(bi, page).unwrap();
            let s = m.stops.iter().find(|s| s.off == w).copied().unwrap();
            (bi, page, s.x + 65536.0, s.y - 3.0 * 65536.0)
        });
        doc.update(cx, |d, cx| {
            assert!(d.caret_click(bi, page, x, y, cx));
            d.enter(false, cx);
        });
        calm(cx, &doc);
        let split = cx.read(|cx| doc.read(cx).source.clone());
        // a backslash: the source box, and nothing in the source yet
        doc.update_in(cx, |d, window, cx| {
            d.replace_text_in_range(None, "\\", window, cx)
        });
        cx.run_until_parked();
        for ch in "titl".chars() {
            let input = cx.read(|cx| doc.read(cx).input.clone());
            cx.update(|window, cx| {
                input.update(cx, |s, cx| {
                    s.replace_text_in_range(None, &ch.to_string(), window, cx)
                })
            });
            cx.run_until_parked();
        }
        cx.read(|cx| {
            let d = doc.read(cx);
            assert!(
                d.active.as_ref().is_some_and(|a| a.gap.is_some()),
                "a box in the new paragraph"
            );
            assert_eq!(d.input.read(cx).value().to_string(), "\\titl");
            assert_eq!(
                d.source, split,
                "a half-typed command stays out of the source"
            );
            assert!(
                d.box_place().is_some_and(|p| !p.lines),
                "the box stays in the gap"
            );
        });
        for ch in "e{AAA}".chars() {
            let input = cx.read(|cx| doc.read(cx).input.clone());
            cx.update(|window, cx| {
                input.update(cx, |s, cx| {
                    s.replace_text_in_range(None, &ch.to_string(), window, cx)
                })
            });
            cx.run_until_parked();
        }
        let s = cx.read(|cx| doc.read(cx).source.clone());
        assert!(
            s.contains("\\title{AAA}\n\n"),
            "the whole command goes in as TeX"
        );
        assert!(!s.contains("textbackslash"));
        calm(cx, &doc);
        assert!(cx.read(|cx| !doc.read(cx).status.contains("LaTeX")));
        // a command left half typed goes in when the box closes
        let input = cx.read(|cx| doc.read(cx).input.clone());
        cx.update(|window, cx| {
            input.update(cx, |s, cx| {
                s.replace_text_in_range(None, " \\date", window, cx)
            })
        });
        cx.run_until_parked();
        assert!(!cx.read(|cx| doc.read(cx).source.contains("AAA} \\date")));
        doc.update_in(cx, |d, window, cx| d.leave(window, cx));
        assert!(cx.read(|cx| doc.read(cx).source.contains("\\title{AAA} \\date")));
        doc.update_in(cx, |d, window, cx| {
            while d.source != base && !d.undo.is_empty() {
                d.undo(window, cx);
            }
        });
        assert_eq!(
            cx.read(|cx| doc.read(cx).source.clone()),
            base,
            "undo restores it all"
        );
        calm(cx, &doc);
        // an empty heading (just "5.1" on the page) takes the caret and goes with Backspace
        doc.update(cx, |d, cx| {
            assert!(d.caret_click(bi, page, x, y, cx));
            d.enter(false, cx);
        });
        calm(cx, &doc);
        doc.update_in(cx, |d, window, cx| {
            d.replace_text_in_range(None, "\\", window, cx)
        });
        cx.run_until_parked();
        let input = cx.read(|cx| doc.read(cx).input.clone());
        cx.update(|window, cx| {
            input.update(cx, |s, cx| {
                s.replace_text_in_range(None, "subsection{}", window, cx)
            })
        });
        cx.run_until_parked();
        doc.update_in(cx, |d, window, cx| d.leave(window, cx));
        calm(cx, &doc);
        let (hb, hp, hx, hy) = cx.read(|cx| {
            let d = doc.read(cx);
            let hb = (0..d.blocks.len())
                .find(|&b| &d.source[d.blocks[b].range.clone()] == "\\subsection{}")
                .expect("the empty heading is a block");
            let r = &d.blocks[hb].rows[0];
            (
                hb,
                r.page,
                r.bbox[0] + 40.0 * 65536.0,
                r.bbox[3] - 3.0 * 65536.0,
            )
        });
        doc.update(cx, |d, cx| {
            assert!(
                d.caret_click(hb, hp, hx, hy, cx),
                "the caret goes into the empty heading"
            );
            d.delete(true, cx);
        });
        assert!(
            !cx.read(|cx| doc.read(cx).source.contains("\\subsection{}")),
            "Backspace removes it"
        );
    }

    /// The ribbon's operations on a real document through rtex: each typesets without a LaTeX
    /// error and undoes in one step. `RTD_TYPE=/abs/copy.tex cargo test --release
    /// format_on_pages -- --ignored --nocapture`.
    #[gpui_kit::test]
    #[ignore]
    fn format_on_pages(cx: &mut TestAppContext) {
        use super::super::FormatOp;
        use crate::document::format::{Align, Size, Style};
        let file = std::env::var("RTD_TYPE").expect("RTD_TYPE=/abs/path/main.tex");
        let path = std::path::PathBuf::from(&file);
        let (root, main) = (
            path.parent().unwrap().to_path_buf(),
            path.file_name().unwrap().to_string_lossy().to_string(),
        );
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        engine::configure("", "/Library/TeX/texbin");
        let (doc, cx) = cx.add_window_view(|window: &mut Window, cx| {
            Doc::new(root, main, Default::default(), window, cx)
        });
        wait(cx, &doc, "the first layout", |d| {
            d.spans_pending == 0 && d.blocks.iter().filter(|b| !b.rows.is_empty()).count() > 3
        });
        calm(cx, &doc);
        let base = cx.read(|cx| doc.read(cx).source.clone());
        // a plain paragraph's second word
        let off = cx.read(|cx| {
            let d = doc.read(cx);
            let b = d
                .blocks
                .iter()
                .find(|b| d.source[b.range.clone()].starts_with("Used together"))
                .expect("the paragraph");
            b.range.start + "Used ".len()
        });
        let select_word = |d: &mut Doc, cx: &mut gpui_kit::Context<Doc>| {
            d.set_caret(off, cx);
            d.extending(cx, |d, cx| d.move_word(1, cx));
        };
        let ops: Vec<(&str, FormatOp, &str)> = vec![
            ("bold", FormatOp::Inline("textbf"), "\\textbf{together}"),
            ("italic", FormatOp::Inline("textit"), "\\textit{together}"),
            (
                "size",
                FormatOp::Size(Size::Named("Large")),
                "{\\Large together}",
            ),
            (
                "colour",
                FormatOp::Color("red".into()),
                "\\textcolor{red}{together}",
            ),
            (
                "bullets",
                FormatOp::List("itemize"),
                "\\begin{itemize}\n  \\item Used together",
            ),
            (
                "heading",
                FormatOp::Style(Style::Heading("subsection")),
                "\\subsection{Used together",
            ),
            (
                "center",
                FormatOp::Align(Align::Center),
                "\\begin{center}\nUsed together",
            ),
        ];
        for (name, op, want) in ops {
            doc.update_in(cx, |d, window, cx| {
                d.status.clear();
                select_word(d, cx);
                d.format(op.clone(), window, cx);
            });
            let src = cx.read(|cx| doc.read(cx).source.clone());
            assert!(
                src.contains(want),
                "{name}: {:?}",
                &src[off.saturating_sub(30)..(off + 80).min(src.len())]
            );
            if name == "colour" {
                assert!(
                    src.contains("\\usepackage{xcolor}") || base.contains("xcolor"),
                    "xcolor added"
                );
            }
            calm(cx, &doc);
            let status = cx.read(|cx| doc.read(cx).status.clone());
            assert!(!status.contains("LaTeX"), "{name}: {status}");
            if name == "bold" {
                // the ribbon shows it, and the button again takes it off
                cx.read(|cx| assert!(doc.read(cx).ribbon_state().has("textbf"), "bold lit"));
                doc.update_in(cx, |d, window, cx| {
                    d.format(FormatOp::Inline("textbf"), window, cx)
                });
                let src = cx.read(|cx| doc.read(cx).source.clone());
                assert!(!src.contains("\\textbf{together}"), "bold off again");
                doc.update_in(cx, |d, window, cx| d.undo(window, cx));
            }
            doc.update_in(cx, |d, window, cx| d.undo(window, cx));
            assert_eq!(
                cx.read(|cx| doc.read(cx).source.clone()),
                base,
                "{name}: one undo step"
            );
            calm(cx, &doc);
        }
        // Find: every printed "together"; ⌘G steps through them; Replace All in one undo step
        doc.update_in(cx, |d, window, cx| {
            d.set_caret(0, cx);
            d.open_find(true, window, cx);
            let q = d.find.as_ref().unwrap().query.clone();
            q.update(cx, |s, cx| s.set_value("together", window, cx));
            d.refind(cx);
            let n = d.find.as_ref().unwrap().matches.len();
            assert!(n >= 2, "{n} matches");
            d.find_step(1, window, cx);
            let first = d.selection_range().unwrap();
            assert!(d.source[first.clone()].eq_ignore_ascii_case("together"));
            d.find_step(1, window, cx);
            assert!(
                d.selection_range().unwrap().start > first.start,
                "the next one"
            );
            let w = d.find.as_ref().unwrap().with.clone();
            w.update(cx, |s, cx| s.set_value("in step", window, cx));
            d.replace_all(cx);
            assert!(!d.source.to_lowercase().contains("used together"));
            assert!(d.find.as_ref().unwrap().matches.is_empty(), "none left");
        });
        calm(cx, &doc);
        assert!(cx.read(|cx| !doc.read(cx).status.contains("LaTeX")));
        doc.update_in(cx, |d, window, cx| d.undo(window, cx));
        assert_eq!(
            cx.read(|cx| doc.read(cx).source.clone()),
            base,
            "Replace All: one undo step"
        );
    }
}
