// The formula editor: a panel under the formula being edited, with its TeX and symbols to click.
// The page keeps the formula typeset where it is and redraws it as the TeX changes, so there is
// no separate preview; the paragraph around it stays as it is. Opened by a double-click on a
// formula or by typing `$`; Escape or a second `$` closes it.
use super::*;
use crate::document::math;

/// The caret's place in a symbol's TeX.
const HERE: char = '‸';

/// The symbol buttons by tab: what the button shows and the TeX it puts at the caret.
const TABS: &[(&str, &[(&str, &str)])] = &[
    (
        "Structure",
        &[
            ("a/b", r"\frac{‸}{}"),
            ("√", r"\sqrt{‸}"),
            ("xⁿ", "^{‸}"),
            ("xₙ", "_{‸}"),
            ("∑", r"\sum_{‸}^{}"),
            ("∫", r"\int_{‸}^{}"),
            ("lim", r"\lim_{‸}"),
            ("( )", r"\left(‸\right)"),
            ("[ ]", r"\left[‸\right]"),
            ("{ }", r"\left\{‸\right\}"),
            ("| |", r"\left|‸\right|"),
            ("vec", r"\vec{‸}"),
            ("hat", r"\hat{‸}"),
            ("bar", r"\overline{‸}"),
            ("text", r"\text{‸}"),
            ("matrix", "\\begin{pmatrix}\n‸\n\\end{pmatrix}"),
            ("cases", "\\begin{cases}\n‸\n\\end{cases}"),
        ],
    ),
    (
        "Greek",
        &[
            ("α", r"\alpha"),
            ("β", r"\beta"),
            ("γ", r"\gamma"),
            ("δ", r"\delta"),
            ("ε", r"\varepsilon"),
            ("ζ", r"\zeta"),
            ("η", r"\eta"),
            ("θ", r"\theta"),
            ("λ", r"\lambda"),
            ("μ", r"\mu"),
            ("ν", r"\nu"),
            ("ξ", r"\xi"),
            ("π", r"\pi"),
            ("ρ", r"\rho"),
            ("σ", r"\sigma"),
            ("τ", r"\tau"),
            ("φ", r"\varphi"),
            ("χ", r"\chi"),
            ("ψ", r"\psi"),
            ("ω", r"\omega"),
            ("Γ", r"\Gamma"),
            ("Δ", r"\Delta"),
            ("Θ", r"\Theta"),
            ("Λ", r"\Lambda"),
            ("Ξ", r"\Xi"),
            ("Π", r"\Pi"),
            ("Σ", r"\Sigma"),
            ("Φ", r"\Phi"),
            ("Ψ", r"\Psi"),
            ("Ω", r"\Omega"),
        ],
    ),
    (
        "Operators",
        &[
            ("±", r"\pm"),
            ("∓", r"\mp"),
            ("×", r"\times"),
            ("÷", r"\div"),
            ("·", r"\cdot"),
            ("∘", r"\circ"),
            ("⊕", r"\oplus"),
            ("⊗", r"\otimes"),
            ("∞", r"\infty"),
            ("∂", r"\partial"),
            ("∇", r"\nabla"),
            ("∏", r"\prod_{‸}^{}"),
            ("∮", r"\oint"),
            ("…", r"\ldots"),
            ("⋯", r"\cdots"),
            ("′", "'"),
        ],
    ),
    (
        "Relations",
        &[
            ("≤", r"\le"),
            ("≥", r"\ge"),
            ("≠", r"\ne"),
            ("≈", r"\approx"),
            ("≡", r"\equiv"),
            ("∼", r"\sim"),
            ("≪", r"\ll"),
            ("≫", r"\gg"),
            ("∝", r"\propto"),
            ("∈", r"\in"),
            ("∉", r"\notin"),
            ("⊂", r"\subset"),
            ("⊆", r"\subseteq"),
            ("∪", r"\cup"),
            ("∩", r"\cap"),
            ("∅", r"\emptyset"),
            ("∀", r"\forall"),
            ("∃", r"\exists"),
            ("∧", r"\land"),
            ("∨", r"\lor"),
            ("¬", r"\neg"),
        ],
    ),
    (
        "Arrows",
        &[
            ("→", r"\to"),
            ("←", r"\leftarrow"),
            ("↔", r"\leftrightarrow"),
            ("⇒", r"\Rightarrow"),
            ("⇐", r"\Leftarrow"),
            ("⇔", r"\Leftrightarrow"),
            ("↦", r"\mapsto"),
            ("↑", r"\uparrow"),
            ("↓", r"\downarrow"),
        ],
    ),
];

/// The first `$` in `tex` that is not an escaped one (`\$`).
fn unescaped_dollar(tex: &str) -> Option<usize> {
    let mut prev = ' ';
    tex.char_indices().find_map(|(i, c)| {
        let hit = c == '$' && prev != '\\';
        prev = c;
        hit.then_some(i)
    })
}

/// A symbol's TeX as it goes into the box: its caret mark in place; a command with no mark gets
/// a space after it, so the next letter typed does not join its name.
fn snippet(tex: &str) -> String {
    if tex.contains(HERE) {
        tex.replace(HERE, &complete::MARK.to_string())
    } else if tex.ends_with(|c: char| c.is_ascii_alphabetic()) {
        format!("{tex} ")
    } else {
        tex.to_string()
    }
}

impl Doc {
    /// The formula editor on the formula `at` is in (or at the edge of), the cursor at `cursor`
    /// (both source offsets), its TeX selected when `select`. False when `at` is in no formula.
    pub(super) fn open_formula(
        &mut self,
        at: usize,
        cursor: usize,
        select: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.sync_box(true, cx);
        let Some(body) =
            math::formula_at(&self.source, at).and_then(|f| math::body(&self.source, f))
        else {
            return false;
        };
        let Some(bi) = self.block_of(body.start) else {
            return false;
        };
        self.selected = None;
        self.caret = None;
        self.typing = None;
        self.overlay = None;
        let mut active = Active::new(self.blocks[bi].span, body.clone(), false, None);
        active.panel = Some(Panel::Formula);
        self.active = Some(active);
        let text = self.source[body.clone()].to_string();
        let len = text.len();
        self.edit_in_box(
            text,
            cursor.clamp(body.start, body.end) - body.start,
            window,
            cx,
        );
        if select {
            self.input
                .update(cx, |s, cx| s.set_selected_range(0..len, cx));
        }
        self.scroll_to(body.start, false, cx);
        true
    }

    /// `$` typed on the pages: an empty formula at the caret, open in the formula editor until
    /// the next `$`.
    pub(super) fn type_dollar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.delete_selection(cx);
        let Some(off) = self.caret.as_ref().map(|c| c.off) else {
            return;
        };
        self.checkpoint();
        self.typing = None;
        self.overlay = None;
        // beside another formula a space keeps the two apart: `$a$$ $` would read as `$$`
        let (before, after) = (
            self.source[..off].ends_with('$'),
            self.source[off..].starts_with('$'),
        );
        let tex = format!(
            "{}{}{}",
            if before { " " } else { "" },
            math::EMPTY_INLINE,
            if after { " " } else { "" }
        );
        if self.replace(off..off, &tex, cx) {
            let inside = off + usize::from(before) + 1;
            self.open_formula(inside, inside, true, window, cx);
        }
    }

    /// A `$` typed in the formula editor ends it (and goes no further into the TeX).
    pub(super) fn dollar_closes(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let mut tex = self.input.read(cx).value().to_string();
        let Some(i) = unescaped_dollar(&tex) else {
            return false;
        };
        tex.remove(i);
        self.input.update(cx, |s, cx| s.set_value(tex, window, cx));
        self.finish_formula(window, cx);
        true
    }

    /// Close the formula editor, the caret on the pages just after the formula.
    pub(super) fn finish_formula(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(off) = self.close_box(window, cx) {
            self.set_caret(off, cx);
            self.focus.focus(window, cx);
        }
    }

    /// With the formula editor open: what it edits is blank, so the whole formula goes. Returns
    /// where the caret belongs after it: where the formula was, or its end.
    pub(super) fn end_formula(&mut self, cx: &mut Context<Self>) -> Option<usize> {
        let a = self
            .active
            .as_ref()
            .filter(|a| a.panel == Some(Panel::Formula))?;
        let whole = math::formula_at(&self.source, a.range.start)?;
        if self.source[a.range.clone()].trim().is_empty() {
            self.replace(whole.clone(), "", cx);
            return Some(whole.start);
        }
        Some(whole.end)
    }

    /// Put a symbol's TeX at the cursor of the formula editor.
    pub(super) fn insert_symbol(&mut self, tex: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = snippet(tex);
        self.input.update(cx, |s, cx| {
            s.replace_text_in_range(None, &text, window, cx);
            s.focus(window, cx);
        });
    }

    /// A block holding a TikZ picture open in the source panel, the picture staying typeset above
    /// it; false when the block has none.
    pub(super) fn open_source_panel(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.sync_box(true, cx);
        let Some((span, range)) = self.blocks.get(ix).map(|b| (b.span, b.range.clone())) else {
            return false;
        };
        let Some(text) = self.source.get(range.clone()) else {
            return false;
        };
        if !text.contains("\\begin{tikzpicture}") {
            return false;
        }
        let text = text.to_string();
        self.checkpoint();
        self.selected = None;
        self.caret = None;
        self.typing = None;
        self.overlay = None;
        let mut active = Active::new(span, range, false, None);
        active.panel = Some(Panel::Source);
        self.active = Some(active);
        self.edit_in_box(text, 0, window, cx);
        true
    }

    /// The page and the bottom (sp) of the line the open formula is on, where the panel goes
    /// under; for a display formula or a source block, under it.
    pub(super) fn formula_anchor(&self, start: usize, panel: Option<Panel>) -> Option<(u32, f64)> {
        let b = self.active_block()?;
        let bi = self.blocks.iter().position(|x| std::ptr::eq(x, b))?;
        let bottom = |page: u32, from: f64| {
            b.rows
                .iter()
                .filter(|r| r.page == page && r.bbox[1] <= from && from <= r.bbox[3])
                .map(|r| r.bbox[3])
                .fold(from, f64::max)
        };
        if panel == Some(Panel::Source) || b.rows.iter().all(|r| r.display) {
            let page = b.rows.last()?.page;
            return Some((page, bottom(page, b.rows.last()?.bbox[3])));
        }
        // inline: the baseline where the formula starts, from the caret map of its page
        let at = self
            .pages_of(bi)
            .filter_map(|page| {
                let m = self.map_of(bi, page)?;
                let i = m.stop_for(start, &self.source)?;
                Some((page, m.stops[i].y))
            })
            .last();
        match at {
            Some((page, y)) => Some((page, bottom(page, y))),
            None => {
                let last = b.rows.last()?;
                Some((last.page, last.bbox[3]))
            }
        }
    }

    /// The formula editor on page `n`, under its formula: the TeX in an editor, then the symbols.
    pub(super) fn render_formula(
        &self,
        n: u32,
        dl: &engine::DisplayList,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let a = self
            .active
            .as_ref()
            .filter(|a| a.in_panel() && a.panel != Some(Panel::Table))?;
        let (start, panel) = (a.range.start, a.panel);
        let (page, bottom) = self.formula_anchor(start, panel)?;
        if page != n {
            return None;
        }
        let theme = cx.theme().clone();
        let st = AppSettings::get(cx);
        let (fs, family) = (st.source_font_size as f32, st.source_family());
        let (box_bg, box_fg) = palette::source_box(self.dark_pages(cx), cx);
        let (l, r) = dl.lines.iter().fold((f64::MAX, 0.0f64), |(l, r), x| {
            (l.min(x.x), r.max(x.x + x.w))
        });
        let page_w = page_px(dl.page_width.unwrap_or(40258437.0), self.zoom);
        let width = (page_px(r, self.zoom) - page_px(l, self.zoom)).clamp(360.0, 640.0);
        let x = page_px(l, self.zoom).min((page_w - width).max(0.0));
        let top = page_px(bottom, self.zoom) + 6.0;
        let formula = panel == Some(Panel::Formula);
        let tab = self.formula_tab.min(TABS.len() - 1);
        let tabs = TABS.iter().enumerate().map(|(i, (name, _))| {
            div()
                .id(("formula-tab", i))
                .role(Role::Button)
                .aria_label(*name)
                .px_2()
                .py_0p5()
                .rounded_md()
                .text_xs()
                .cursor_pointer()
                .when(i == tab, |d| {
                    d.bg(theme.secondary).text_color(theme.foreground)
                })
                .when(i != tab, |d| {
                    d.text_color(theme.muted_foreground)
                        .hover(|s| s.bg(theme.secondary))
                })
                .child(*name)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.formula_tab = i;
                        cx.notify();
                    }),
                )
        });
        let symbols = TABS[tab].1.iter().enumerate().map(|(i, (shown, tex))| {
            let tex = *tex;
            div()
                .id(("formula-symbol", i))
                .role(Role::Button)
                .aria_label(tex.replace(HERE, ""))
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tex.replace(HERE, ""))
                        .build(window, cx)
                })
                .min_w(px(30.0))
                .h(px(28.0))
                .px_1()
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .text_sm()
                .cursor_pointer()
                .hover(|s| s.bg(theme.secondary))
                .child(*shown)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.insert_symbol(tex, window, cx);
                    }),
                )
        });
        Some(
            div()
                .id("formula-editor")
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
                        .child(if formula { "Formula" } else { "TikZ" })
                        .child(if formula {
                            "Esc or $ to finish"
                        } else {
                            "Esc to finish"
                        }),
                )
                .child(
                    div()
                        .rounded_md()
                        .bg(box_bg)
                        .text_color(box_fg)
                        .child(self.box_editor(width - 18.0, fs, fs * 1.4, fs * 1.4, family, cx)),
                )
                .when(formula, |d| {
                    d.child(div().flex().gap_1().children(tabs))
                        .child(div().flex().flex_wrap().gap_0p5().children(symbols))
                })
                .when(!formula, |d| {
                    d.children(self.render_points(width - 18.0, cx))
                })
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{complete, snippet, unescaped_dollar};

    #[test]
    fn a_dollar_ends_the_formula_unless_escaped() {
        assert_eq!(unescaped_dollar("x^2 $"), Some(4));
        assert_eq!(unescaped_dollar("\\$5 + x"), None);
        assert_eq!(unescaped_dollar("a \\$ b $"), Some(7));
    }

    #[test]
    fn a_symbol_goes_in_with_its_caret_or_a_space() {
        let m = complete::MARK;
        assert_eq!(snippet(r"\frac{‸}{}"), format!("\\frac{{{m}}}{{}}"));
        assert_eq!(snippet(r"\alpha"), "\\alpha ");
        assert_eq!(snippet("^{‸}"), format!("^{{{m}}}"));
        assert_eq!(snippet("'"), "'");
    }
}
