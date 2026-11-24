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
    /// A column's or row's edge held, to size it.
    drag: Option<Drag>,
    _subs: Vec<Subscription>,
}

/// An edge of the grid being dragged: what it sizes, where the pointer was, and the size then.
#[derive(Clone, Copy)]
enum Drag {
    Col { col: usize, from: f32, width: f32 },
    Row { row: usize, from: f32, space: f32 },
}

/// How a column is drawn when its width is not fixed, and the least and most it is drawn.
const COL_PX: f32 = 112.0;
const COL_LEAST: f32 = 40.0;
const COL_MOST: f32 = 360.0;
/// The most room under a row that is drawn.
const ROW_MOST: f32 = 48.0;

/// Colours the cell tools offer: shading and text.
const SHADES: &[(&str, &str, u32)] = &[
    ("yellow!35", "Yellow", 0xfde68a),
    ("green!25", "Green", 0xbbf7d0),
    ("cyan!25", "Blue", 0xbae6fd),
    ("red!20", "Red", 0xfecaca),
    ("gray!25", "Grey", 0xd4d4d8),
];
const INKS: &[(&str, &str, u32)] = &[
    ("red", "Red", 0xdc2626),
    ("blue", "Blue", 0x2563eb),
    ("green!50!black", "Green", 0x15803d),
];

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
            drag: None,
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

    pub(super) fn focus_cell(
        &mut self,
        at: (usize, usize),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    /// Pixels the page draws a point as.
    fn px_per_pt(&self) -> f32 {
        page_px(65536.0, self.zoom)
    }

    /// An edge taken at pointer position `at` (`col` or `row` sized): one undo step for the drag.
    pub(super) fn table_drag_start(
        &mut self,
        col: Option<usize>,
        row: Option<usize>,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(t) = self.live_table() else {
            return;
        };
        let ppp = self.px_per_pt();
        let drag = match (col, row) {
            (Some(col), _) => {
                let width = t
                    .col_width(&self.source, col)
                    .map_or(COL_PX, |w| (w as f32 * ppp).clamp(COL_LEAST, COL_MOST));
                Drag::Col {
                    col,
                    from: f32::from(at.x),
                    width,
                }
            }
            (_, Some(row)) => Drag::Row {
                row,
                from: f32::from(at.y),
                space: t.row_space(&self.source, row) as f32,
            },
            _ => return,
        };
        self.checkpoint();
        if let Some(e) = self.table.as_mut() {
            e.drag = Some(drag);
        }
        cx.notify();
    }

    /// The pointer is at `at` with an edge held: its size follows, to the tenth of a point.
    pub(super) fn table_drag_to(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = self.table.as_ref().and_then(|e| e.drag) else {
            return;
        };
        let Some(t) = self.live_table() else {
            return;
        };
        let ppp = self.px_per_pt();
        let change = match drag {
            Drag::Col { col, from, width } => {
                let px = (width + f32::from(at.x) - from).clamp(COL_LEAST, COL_MOST);
                let pt = (f64::from(px / ppp) * 10.0).round() / 10.0;
                t.set_col_width(&self.source, col, pt)
            }
            Drag::Row { row, from, space } => {
                let pt = f64::from(space + (f32::from(at.y) - from) / ppp)
                    .clamp(0.0, f64::from(ROW_MOST / ppp));
                t.set_row_space(&self.source, row, (pt * 2.0).round() / 2.0)
                    .map(|c| (c, vec![]))
            }
        };
        let Some((ch, packages)) = change else {
            return;
        };
        if self.source[ch.range.clone()] != ch.text {
            self.replace(ch.range, &ch.text, cx);
            self.add_packages(&packages, cx);
        }
    }

    pub(super) fn table_drag_end(&mut self, cx: &mut Context<Self>) {
        if let Some(e) = self.table.as_mut()
            && e.drag.take().is_some()
        {
            cx.notify();
        }
    }

    /// The focused column back to the width its text asks for.
    fn table_auto_width(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some((_, c)) = self.focused_cell(window, cx) else {
            return;
        };
        if let Some(ch) = self.live_table().and_then(|t| t.auto_col(&self.source, c)) {
            self.checkpoint();
            self.replace(ch.range, &ch.text, cx);
        }
    }

    /// The focused cell's text made `f` of: bold, shaded… (its input takes it, the source follows).
    pub(super) fn table_cell_format(
        &mut self,
        f: impl Fn(&str) -> String,
        packages: &[&str],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((r, c)) = self.focused_cell(window, cx) else {
            return;
        };
        let Some(state) = self
            .table
            .as_ref()
            .and_then(|t| t.cells.get(r)?.get(c))
            .cloned()
        else {
            return;
        };
        let now = state.read(cx).value().to_string();
        let new = f(&now);
        if new == now {
            return;
        }
        self.checkpoint();
        state.update(cx, |s, cx| s.set_value(new.clone(), window, cx));
        // what typing does: the cell goes into the source
        self.table_cell_changed(r, c, cx);
        self.add_packages(packages, cx);
        cx.notify();
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
        let live = self.live_table();
        let ppp = self.px_per_pt();
        let col_px = |c: usize| {
            live.as_ref()
                .and_then(|t| t.col_width(&self.source, c))
                .map_or(COL_PX, |w| (w as f32 * ppp).clamp(COL_LEAST, COL_MOST))
        };
        let total: f32 = (0..cols).map(col_px).sum::<f32>() + cols as f32 * 4.0;
        let width = (total + 20.0).clamp(300.0, 760.0);
        let x = page_px(l.min(r), self.zoom).min((page_w - width).max(0.0));
        let top = page_px(last.bbox[3], self.zoom) + 6.0;
        // above each column its width, with its edge to drag
        let heads: Vec<_> = (0..cols)
            .map(|c| {
                let fixed = live.as_ref().and_then(|t| t.col_width(&self.source, c));
                div()
                    .relative()
                    .w(px(col_px(c)))
                    .h(px(16.0))
                    .flex_none()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(fixed.map_or("auto".to_string(), |w| format!("{w:.0} pt")))
                    .child(
                        div()
                            .id(("table-col-edge", c))
                            .role(Role::Button)
                            .aria_label("Resize the column")
                            .absolute()
                            .right(px(-4.0))
                            .top_0()
                            .w(px(6.0))
                            .h_full()
                            .rounded_sm()
                            .bg(theme.border)
                            .cursor_ew_resize()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.table_drag_start(Some(c), None, ev.position, cx);
                                }),
                            ),
                    )
            })
            .collect();
        let grid = edit
            .cells
            .iter()
            .enumerate()
            .map(|(ri, line)| {
                let space = live
                    .as_ref()
                    .map_or(0.0, |t| t.row_space(&self.source, ri) as f32 * ppp)
                    .min(ROW_MOST);
                div()
                    .flex()
                    .flex_col()
                    .child(div().flex().gap_1().children(line.iter().enumerate().map(
                        |(ci, state)| {
                            div()
                                .w(px(col_px(ci)))
                                .flex_none()
                                .child(Input::new(state).small())
                        },
                    )))
                    // under the row: the room it keeps, with an edge to drag
                    .child(
                        div()
                            .id(("table-row-edge", ri))
                            .role(Role::Button)
                            .aria_label("Resize the row")
                            .h(px(4.0 + space))
                            .w_full()
                            .flex()
                            .items_end()
                            .cursor_ns_resize()
                            .child(div().w_full().h(px(2.0)).rounded_sm().bg(theme.border))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.table_drag_start(None, Some(ri), ev.position, cx);
                                }),
                            ),
                    )
            })
            .collect::<Vec<_>>();
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
        let (shade, bold, italic, ink) = edit
            .cells
            .get(edit.at.0)
            .and_then(|l| l.get(edit.at.1))
            .map(|c| table::cell_look(c.read(cx).value().as_ref()))
            .unwrap_or_default();
        let swatch = |id: (&'static str, usize), name: &'static str, rgb: u32, on: bool| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(name)
                .size(px(18.0))
                .flex_none()
                .rounded_sm()
                .border_1()
                .border_color(if on { theme.primary } else { theme.border })
                .bg(gpui::rgb(rgb))
                .cursor_pointer()
        };
        let shades: Vec<_> = SHADES
            .iter()
            .enumerate()
            .map(|(i, (tex, name, rgb))| {
                swatch(
                    ("table-shade", i),
                    name,
                    *rgb,
                    shade.as_deref() == Some(*tex),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.table_cell_format(
                            |t| table::shade_cell(t, Some(tex)),
                            &["colortbl", "xcolor"],
                            window,
                            cx,
                        );
                    }),
                )
            })
            .collect();
        let inks: Vec<_> = INKS
            .iter()
            .enumerate()
            .map(|(i, (tex, name, rgb))| {
                swatch(("table-ink", i), name, *rgb, ink.as_deref() == Some(*tex)).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.table_cell_format(
                            |t| table::colour_cell(t, Some(tex)),
                            &["xcolor"],
                            window,
                            cx,
                        );
                    }),
                )
            })
            .collect();
        let cell_tool = |id: &'static str,
                         label: &'static str,
                         on: bool,
                         cmd: &'static str,
                         cx: &mut Context<Self>| {
            chip(id, label, on).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.table_cell_format(|t| table::toggle_cell(t, cmd), &[], window, cx);
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
                .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                    if this.table.as_ref().is_some_and(|t| t.drag.is_some()) {
                        match ev.pressed_button {
                            Some(MouseButton::Left) => this.table_drag_to(ev.position, cx),
                            _ => this.table_drag_end(cx),
                        }
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.table_drag_end(cx)),
                )
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
                        .gap_0()
                        .child(div().flex().gap_1().children(heads))
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
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_0p5()
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("Cell"),
                        )
                        .child(cell_tool("table-cell-b", "B", bold, "textbf", cx))
                        .child(cell_tool("table-cell-i", "I", italic, "textit", cx))
                        .children(shades)
                        .child(chip("table-shade-none", "No fill", false).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.table_cell_format(
                                    |t| table::shade_cell(t, None),
                                    &[],
                                    window,
                                    cx,
                                );
                            }),
                        ))
                        .children(inks)
                        .child(chip("table-ink-none", "Auto ink", false).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.table_cell_format(
                                    |t| table::colour_cell(t, None),
                                    &[],
                                    window,
                                    cx,
                                );
                            }),
                        ))
                        .child(chip("table-col-auto", "Auto width", false).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.table_auto_width(window, cx);
                            }),
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
