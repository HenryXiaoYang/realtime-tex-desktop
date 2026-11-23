// The table editor: a grid of cells under the table being edited, one input each, written into
// the source as they change, with buttons to add and remove rows and columns. The page keeps the
// table typeset in place and redraws it as the cells change. Opened from the table's context menu
// ("Edit Table") or a double-click on the table beside its text; Escape closes it.
use super::*;
use crate::document::table;
use gpui_kit::component::input::{Input, InputState};

pub(super) struct TableEdit {
    /// Each row's cells (a row with a `\multicolumn` has fewer).
    cells: Vec<Vec<Entity<InputState>>>,
    /// The cell last known to have focus.
    at: (usize, usize),
    _subs: Vec<Subscription>,
}

impl TableEdit {
    #[cfg(test)]
    pub(super) fn cell_count(&self) -> usize {
        self.cells.iter().map(|l| l.len()).sum()
    }

    #[cfg(test)]
    pub(super) fn cell(&self, r: usize, c: usize) -> Entity<InputState> {
        self.cells[r][c].clone()
    }
}

#[derive(Clone, Copy)]
pub(super) enum Op {
    RowBelow,
    DelRow,
    ColRight,
    DelCol,
}

/// A cell's TeX the table can take: its braces balance and it holds no `&` or `\\` of its own
/// (half-typed, it waits until it is whole).
fn cell_ok(tex: &str) -> bool {
    let mut prev = ' ';
    let mut escaped = false;
    for c in tex.chars() {
        if escaped {
            if c == '\\' {
                return false;
            }
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '&' {
            return false;
        }
        prev = c;
    }
    !escaped && prev != '\\' && document::caret::balanced(tex)
}

impl Doc {
    /// The table editor on the table in block `ix`; false when the block has no `tabular` it
    /// can read.
    pub(super) fn open_table(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.sync_box(true, cx);
        let Some((span, range)) = self.blocks.get(ix).map(|b| (b.span, b.range.clone())) else {
            return false;
        };
        if table::find(&self.source, range.clone()).is_none() {
            return false;
        }
        self.checkpoint();
        self.selected = None;
        self.caret = None;
        self.typing = None;
        self.overlay = None;
        let mut active = Active::new(span, range, false, None);
        active.panel = Some(Panel::Table);
        self.active = Some(active);
        self.build_table((0, 0), window, cx);
        true
    }

    /// The table in the block being edited, as the source has it now.
    fn live_table(&self) -> Option<table::Table> {
        let b = self.active_block()?;
        table::find(&self.source, b.range.clone())
    }

    /// One input per cell of the table as it is now, the focus on `focus` (kept inside it).
    fn build_table(&mut self, focus: (usize, usize), window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = self.live_table() else {
            return;
        };
        let texts: Vec<Vec<String>> = t
            .rows
            .iter()
            .map(|r| {
                r.cells
                    .iter()
                    .map(|c| self.source[c.clone()].replace('\n', " "))
                    .collect()
            })
            .collect();
        let (mut cells, mut subs) = (vec![], vec![]);
        for (r, row) in texts.into_iter().enumerate() {
            let mut line = vec![];
            for (c, text) in row.into_iter().enumerate() {
                let state = cx.new(|cx| InputState::new(window, cx));
                state.update(cx, |s, cx| s.set_value(text, window, cx));
                subs.push(cx.subscribe_in(
                    &state,
                    window,
                    move |this, _, ev: &InputEvent, _, cx| {
                        if matches!(ev, InputEvent::Change) {
                            this.table_cell_changed(r, c, cx);
                        }
                    },
                ));
                line.push(state);
            }
            cells.push(line);
        }
        let r = focus.0.min(cells.len().saturating_sub(1));
        let c = focus
            .1
            .min(cells.get(r).map_or(0, |l| l.len()).saturating_sub(1));
        if let Some(state) = cells.get(r).and_then(|l| l.get(c)) {
            state.update(cx, |s, cx| s.focus(window, cx));
        }
        self.table = Some(TableEdit {
            cells,
            at: (r, c),
            _subs: subs,
        });
        cx.notify();
    }

    /// Cell (`r`, `c`) was typed in: its text goes into the source.
    fn table_cell_changed(&mut self, r: usize, c: usize, cx: &mut Context<Self>) {
        let Some(text) = self
            .table
            .as_ref()
            .and_then(|t| t.cells.get(r)?.get(c))
            .map(|s| s.read(cx).value().to_string())
        else {
            return;
        };
        let Some(t) = self.live_table() else {
            return;
        };
        let Some(ch) = t.set_cell(&self.source, r, c, &text) else {
            return;
        };
        // nothing new (the input was just filled from the source), or not whole TeX yet
        if !cell_ok(&text) || self.source[ch.range.clone()].replace('\n', " ") == text.trim() {
            return;
        }
        self.replace(ch.range, &ch.text, cx);
    }

    /// The cell with focus (else the last that had it).
    fn focused_cell(&self, window: &Window, cx: &App) -> Option<(usize, usize)> {
        let t = self.table.as_ref()?;
        let hit = t.cells.iter().enumerate().find_map(|(r, line)| {
            let c = line
                .iter()
                .position(|s| s.focus_handle(cx).is_focused(window))?;
            Some((r, c))
        });
        hit.or(Some(t.at))
    }

    fn focus_cell(&mut self, at: (usize, usize), window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = self.table.as_mut() else {
            return;
        };
        let Some(state) = t.cells.get(at.0).and_then(|l| l.get(at.1)).cloned() else {
            return;
        };
        t.at = at;
        state.update(cx, |s, cx| s.focus(window, cx));
    }

    /// Add or remove a row or column next to the focused cell.
    pub(super) fn table_op(&mut self, op: Op, window: &mut Window, cx: &mut Context<Self>) {
        let Some((r, c)) = self.focused_cell(window, cx) else {
            return;
        };
        let Some(t) = self.live_table() else {
            return;
        };
        let src = &self.source;
        let changes = match op {
            Op::RowBelow => t.add_row(src, r).map(|c| vec![c]),
            Op::DelRow => t.del_row(src, r).map(|c| vec![c]),
            Op::ColRight => t.add_col(src, c),
            Op::DelCol => t.del_col(src, c),
        };
        let Some(changes) = changes else {
            return;
        };
        let (rows, cols) = (t.rows.len(), t.cols());
        let new = table::apply(&self.source, changes);
        self.replace(0..self.source.len(), &new, cx);
        let focus = match op {
            Op::RowBelow => (r + 1, c),
            Op::DelRow => (r.min(rows.saturating_sub(2)), c),
            Op::ColRight => (r, c + 1),
            Op::DelCol => (r, c.min(cols.saturating_sub(2))),
        };
        self.build_table(focus, window, cx);
    }

    /// The focused cell's column made `l`, `c` or `r`.
    pub(super) fn table_align(&mut self, to: char, window: &Window, cx: &mut Context<Self>) {
        let Some((_, c)) = self.focused_cell(window, cx) else {
            return;
        };
        let Some(ch) = self
            .live_table()
            .and_then(|t| t.set_align(&self.source, c, to))
        else {
            return;
        };
        self.replace(ch.range, &ch.text, cx);
    }

    /// The table ruled as `style` (booktabs added for its rules).
    pub(super) fn table_borders(&mut self, style: table::Borders, cx: &mut Context<Self>) {
        let Some(changes) = self
            .live_table()
            .and_then(|t| t.set_borders(&self.source, style))
        else {
            return;
        };
        let new = table::apply(&self.source, changes);
        self.replace(0..self.source.len(), &new, cx);
        if style == table::Borders::Booktabs {
            self.add_packages(&["booktabs"], cx);
        }
    }

    /// The first row made bold, or plain again.
    pub(super) fn table_header(&mut self, cx: &mut Context<Self>) {
        let Some(t) = self.live_table() else {
            return;
        };
        let Some(changes) = t.set_header(&self.source, !t.header(&self.source)) else {
            return;
        };
        let new = table::apply(&self.source, changes);
        self.replace(0..self.source.len(), &new, cx);
    }

    /// The table editor's keys: Tab and Enter move between cells (past the last, a new row),
    /// the arrows up and down between rows, Escape closes it.
    pub(super) fn on_table_key(
        &mut self,
        ks: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((r, c)) = self.focused_cell(window, cx) else {
            return;
        };
        let Some(t) = self.table.as_ref() else {
            return;
        };
        let (rows, row_len) = (t.cells.len(), t.cells[r].len());
        let shift = ks.modifiers.shift;
        if ks.modifiers.modified() && !shift {
            return;
        }
        let target = match ks.key.as_str() {
            "escape" => {
                self.leave(window, cx);
                cx.stop_propagation();
                return;
            }
            "tab" if shift => match (r, c) {
                (0, 0) => None,
                (_, 0) => Some((r - 1, t.cells[r - 1].len() - 1)),
                _ => Some((r, c - 1)),
            },
            "tab" if c + 1 < row_len => Some((r, c + 1)),
            "tab" if r + 1 < rows => Some((r + 1, 0)),
            "enter" if shift => r.checked_sub(1).map(|r| (r, c)),
            "enter" | "down" if r + 1 < rows => Some((r + 1, c)),
            "up" if r > 0 => Some((r - 1, c)),
            // past the last cell: a new row, the focus in its first (Enter: the same column)
            "tab" | "enter" => {
                let col = if ks.key == "tab" { 0 } else { c };
                self.table_op(Op::RowBelow, window, cx);
                self.focus_cell((r + 1, col), window, cx);
                cx.stop_propagation();
                return;
            }
            _ => return,
        };
        cx.stop_propagation();
        if let Some(at) = target {
            self.focus_cell(at, window, cx);
        }
    }

    /// The table editor on page `n`, under its table.
    pub(super) fn render_table(
        &self,
        n: u32,
        dl: &engine::DisplayList,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let edit = self.table.as_ref()?;
        let rows = &self.active_block()?.rows;
        let last = rows.last()?;
        if last.page != n {
            return None;
        }
        let theme = cx.theme().clone();
        let (l, r) = dl.lines.iter().fold((f64::MAX, 0.0f64), |(l, r), x| {
            (l.min(x.x), r.max(x.x + x.w))
        });
        let cols = edit.cells.iter().map(|l| l.len()).max().unwrap_or(1);
        let page_w = page_px(dl.page_width.unwrap_or(40258437.0), self.zoom);
        let width = (cols as f32 * 118.0 + 20.0).clamp(300.0, 720.0);
        let x = page_px(l.min(r), self.zoom).min((page_w - width).max(0.0));
        let top = page_px(last.bbox[3], self.zoom) + 6.0;
        let grid = edit.cells.iter().map(|line| {
            div().flex().gap_1().children(line.iter().map(|state| {
                div()
                    .w(px(112.0))
                    .flex_none()
                    .child(Input::new(state).small())
            }))
        });
        // a button of the tool rows, filled when what it sets is what the table has
        let chip = |id: &'static str, label: &'static str, on: bool| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(label)
                .px_2()
                .h(px(24.0))
                .flex()
                .items_center()
                .rounded_md()
                .text_xs()
                .cursor_pointer()
                .when(on, |d| d.bg(theme.secondary))
                .hover(|s| s.bg(theme.secondary))
                .child(label)
        };
        let tool = |id: &'static str, label: &'static str, op: Op, cx: &mut Context<Self>| {
            chip(id, label, false).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.table_op(op, window, cx);
                }),
            )
        };
        let live = self.live_table();
        let align = live.as_ref().and_then(|t| t.align(&self.source, edit.at.1));
        let borders = live.as_ref().map(|t| t.borders(&self.source));
        let header = live.as_ref().is_some_and(|t| t.header(&self.source));
        let align_tool =
            |id: &'static str, label: &'static str, to: char, cx: &mut Context<Self>| {
                chip(id, label, align == Some(to)).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.table_align(to, window, cx);
                    }),
                )
            };
        let border_tool = |id: &'static str,
                           label: &'static str,
                           style: table::Borders,
                           cx: &mut Context<Self>| {
            chip(id, label, borders == Some(style)).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.table_borders(style, cx);
                }),
            )
        };
        Some(
            div()
                .id("table-editor")
                .absolute()
                .left(px(x))
                .top(px(top))
                .w(px(width))
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .text_color(theme.popover_foreground)
                .shadow_lg()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Table")
                        .child("Tab next cell · Esc to finish"),
                )
                .child(
                    div()
                        .id("table-cells")
                        .max_h(px(280.0))
                        .overflow_scroll()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .children(grid),
                )
                .child(
                    div()
                        .flex()
                        .gap_0p5()
                        .child(tool("table-row-add", "+ Row", Op::RowBelow, cx))
                        .child(tool("table-row-del", "− Row", Op::DelRow, cx))
                        .child(tool("table-col-add", "+ Column", Op::ColRight, cx))
                        .child(tool("table-col-del", "− Column", Op::DelCol, cx)),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_0p5()
                        .child(align_tool("table-align-l", "Left", 'l', cx))
                        .child(align_tool("table-align-c", "Center", 'c', cx))
                        .child(align_tool("table-align-r", "Right", 'r', cx))
                        .child(chip("table-header", "Bold header", header).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.table_header(cx);
                            }),
                        )),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_0p5()
                        .child(border_tool(
                            "table-b-none",
                            "No rules",
                            table::Borders::None,
                            cx,
                        ))
                        .child(border_tool(
                            "table-b-lines",
                            "Lines",
                            table::Borders::Lines,
                            cx,
                        ))
                        .child(border_tool(
                            "table-b-grid",
                            "Grid",
                            table::Borders::Grid,
                            cx,
                        ))
                        .child(border_tool(
                            "table-b-book",
                            "Booktabs",
                            table::Borders::Booktabs,
                            cx,
                        )),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::cell_ok;

    #[test]
    fn a_cell_waits_until_it_is_whole_tex() {
        assert!(cell_ok("plain {and} \\textbf{bold} \\& more"));
        assert!(!cell_ok("a & b"));
        assert!(!cell_ok("\\textbf{open"));
        assert!(!cell_ok("end \\"));
        assert!(!cell_ok("two \\\\ lines"));
    }
}
