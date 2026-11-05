// Drawing a typeset page: its image, split around the source box, with the selection outline,
// the insert line and the compile spinner.
use super::*;

impl Doc {
    pub(super) fn render_page(
        &self,
        n: u32,
        dl: &engine::DisplayList,
        at: Option<&BoxPlace>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (w, h) = (
            page_px(dl.page_width.unwrap_or(40258437.0), self.zoom),
            page_px(dl.page_height.unwrap_or(52099153.0), self.zoom),
        );
        // a full LaTeX run is under way: the last pages stay (with what was typed drawn over
        // them) and a small spinner in the corner says they are out of date
        let compiling = self.compiling;
        let image = self.images.get(&n).map(|im| im.img.clone());
        let muted = cx.theme().muted_foreground;
        let spinner = compiling.then(|| {
            div().absolute().top_2().right_2().child(
                Spinner::new()
                    .icon(Icon::default().path("icons/loader-circle.svg"))
                    .ease(|t| t)
                    .with_size(Size::Small)
                    .color(muted),
            )
        });
        let slice = |top: f32, height: f32| {
            div()
                .relative()
                .w(px(w))
                .h(px(height))
                .overflow_hidden()
                .children(
                    image
                        .clone()
                        .map(|i| img(i).absolute().top(px(-top)).left_0().w(px(w)).h(px(h))),
                )
        };
        let bounds = self.page_bounds.clone();
        let page = div()
            .id(("page", n as usize))
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(w))
            // render::DARK_PAPER, under the page image while it loads
            .bg(rgb(if self.dark_pages(cx) {
                0x000000
            } else {
                0xffffff
            }))
            .shadow_md()
            .cursor_text()
            .child(
                canvas(
                    move |b, _, _| {
                        let mut v = bounds.take();
                        v.retain(|(p, _)| *p != n);
                        v.push((n, b));
                        bounds.set(v);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                    // a double-click edits on its release, so the new box gets no half a click
                    // (which it would take as selecting a word)
                    if ev.click_count < 2 {
                        this.on_page_click(n, ev.position, 1, ev.modifiers.shift, window, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseUpEvent, window, cx| {
                    this.dragging = false;
                    if ev.click_count >= 2 && this.active.is_none() {
                        let clicks = ev.click_count;
                        this.on_page_click(n, ev.position, clicks, ev.modifiers.shift, window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .on_mouse_move(cx.listener(move |this, ev: &MouseMoveEvent, _, cx| {
                if ev.pressed_button == Some(MouseButton::Left) {
                    this.drag_to(n, ev.position, cx);
                } else {
                    this.dragging = false;
                    this.on_page_hover(n, ev.position, cx)
                }
            }))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if !hovered && this.hover_gap.is_some_and(|g| g.0 == n) {
                    this.hover_gap = None;
                    cx.notify();
                }
            }))
            // runs before the context menu (on the page column) builds its items
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                    let hit = this.hit(n, ev.position);
                    let off = hit
                        .and_then(|(_, _, bi)| bi)
                        .map(|i| this.blocks[i].range.start);
                    this.menu_target.set(hit.map(|(_, y, _)| (n, y, off)));
                    if this.active.is_none() {
                        this.caret = None;
                        let ix = off.and_then(|o| this.block_at(o));
                        let inside = ix.is_some_and(|i| this.selection_with(i) != (i, i));
                        match (ix, this.selected) {
                            (Some(i), Some((a, _))) if ev.modifiers.shift => {
                                if let Some(a) = this.block_at(a) {
                                    this.set_selection(a, i, cx)
                                }
                            }
                            // a menu on one of several selected blocks keeps them all
                            (Some(_), _) if inside => {}
                            _ => this.selected = off.map(|o| (o, o)),
                        }
                        this.focus.focus(window, cx);
                        cx.notify();
                    }
                }),
            );
        // the selected block's outline, while no box is open
        let pad = 3.0 * self.zoom;
        let outline = self
            .selection()
            .filter(|_| self.active.is_none())
            .and_then(|(lo, hi)| {
                union(
                    self.blocks[lo..=hi]
                        .iter()
                        .flat_map(|b| &b.rows)
                        .filter(|r| r.page == n)
                        .map(|r| r.bbox),
                )
            })
            .map(|r| {
                let (x, y) = (
                    page_px(r[0], self.zoom) - pad,
                    page_px(r[1], self.zoom) - pad,
                );
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(page_px(r[2], self.zoom) + pad - x))
                    .h(px(page_px(r[3], self.zoom) + pad - y))
                    .border_2()
                    // a blue that reads on white and on dark paper
                    .border_color(rgb(0x3b82f6))
            });
        let insert = self
            .hover_gap
            .filter(|g| g.0 == n && self.active.is_none())
            .map(|(_, y, spot)| self.render_insert_line(dl, y, spot, cx));
        // text being edited that went on over a page break: its source is all in the box on the
        // first page, so its typeset rest is blanked here
        let paper = rgb(if self.dark_pages(cx) {
            0x000000
        } else {
            0xffffff
        });
        let blank = self.edited_rows_after(at.map(|a| a.page), n).map(|r| {
            let pad = 2.0 * self.zoom;
            // from the text column's left edge: list labels hang left of their line's box
            let left = dl.lines.iter().map(|l| l.x).fold(r[0], f64::min);
            let (x, y) = (
                page_px(left, self.zoom) - pad,
                page_px(r[1], self.zoom) - pad,
            );
            div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(page_px(r[2], self.zoom) + pad - x))
                .h(px(page_px(r[3], self.zoom) + pad - y))
                .bg(paper)
        });
        let Some(at) = at else {
            self.box_cut.set(None);
            return page
                .h(px(h))
                .child(slice(0.0, h))
                .children(blank)
                .children(outline)
                .children(insert)
                .children(self.render_matches(n))
                .children(self.render_selection(n))
                .children(self.render_caret(n, window, cx))
                .children(spinner)
                .into_any_element();
        };
        // the source box replaces the block's lines; the rest of the page moves down
        let (editor, top, bottom) = self.render_box(at, dl, cx);
        self.box_cut.set(Some((n, bottom - top)));
        let rest = (h - bottom).max(0.0);
        page.child(slice(0.0, top))
            .child(editor)
            .child(
                div()
                    .relative()
                    .w(px(w))
                    .h(px(rest))
                    .overflow_hidden()
                    .children(image.map(|i| {
                        img(i)
                            .absolute()
                            .top(px(-bottom))
                            .left_0()
                            .w(px(w))
                            .h(px(h))
                    })),
            )
            .children(spinner)
            .into_any_element()
    }

    /// The box of the edited text's rows on page `n` when the box itself is on an earlier page.
    pub(super) fn edited_rows_after(&self, box_page: Option<u32>, n: u32) -> Option<[f64; 4]> {
        let a = self.active.as_ref().filter(|a| !a.frame)?;
        let box_page = box_page.or_else(|| self.box_place().map(|p| p.page))?;
        if n <= box_page {
            return None;
        }
        let overlap =
            |b: &document::Block| b.range.end.min(a.range.end) > b.range.start.max(a.range.start);
        union(
            self.blocks
                .iter()
                .filter(|b| overlap(b))
                .flat_map(|b| &b.rows)
                .filter(|r| r.page == n)
                .map(|r| r.bbox),
        )
    }

    /// The insert line at `y` (sp): a thin rule across the text column with a + in the middle,
    /// which adds a block there.
    pub(super) fn render_insert_line(
        &self,
        dl: &engine::DisplayList,
        y: f64,
        spot: Spot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pw = dl.page_width.unwrap_or(40258437.0);
        let (l, r) = dl.lines.iter().fold((f64::MAX, f64::MIN), |(l, r), x| {
            (l.min(x.x), r.max(x.x + x.w))
        });
        let (l, r) = if l < r { (l, r) } else { (pw * 0.2, pw * 0.8) };
        let (left, right) = (page_px(l, self.zoom), page_px(r, self.zoom));
        let size = 11.0;
        let accent = rgb(0x3b82f6);
        let rule = || div().flex_1().h(px(1.0)).bg(rgba(0x3b82f699));
        div()
            .absolute()
            .left(px(left))
            .w(px(right - left))
            .top(px(page_px(y, self.zoom) - size / 2.0))
            .h(px(size))
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(rule())
            .child(
                div()
                    .id("insert-block")
                    .size(px(size))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(accent)
                    .text_color(rgb(0xffffff))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0x2563eb)))
                    .child(Icon::default().path("icons/plus.svg").size(px(8.0)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.hover_gap = None;
                            this.insert_at(spot, "", window, cx);
                        }),
                    ),
            )
            .child(rule())
            .into_any_element()
    }
}
