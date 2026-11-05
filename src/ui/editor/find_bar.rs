// Find and Replace on the pages (⌘F, ⌘⌥F): a bar over the pages' top right, every match
// highlighted (the current one stronger), Enter / ⌘G to the next, ⇧Enter / ⌘⇧G to the one
// before, and the replacement put in as typed, keeping the markup a match runs through.
use super::*;
use crate::ui::Tip;
use document::find;
use gpui_kit::component::Selectable;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};

pub(super) struct Find {
    pub(super) query: Entity<InputState>,
    pub(super) with: Entity<InputState>,
    /// The replace row shows.
    replacing: bool,
    case: bool,
    pub(super) matches: Vec<Range<usize>>,
    current: Option<usize>,
    _subs: Vec<Subscription>,
}

impl Doc {
    /// ⌘F (with `replace`, ⌘⌥F): the bar opens, its query on the selection's text if any.
    pub(super) fn open_find(&mut self, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        let picked = self
            .selection_range()
            .map(|r| glyphmap::plain_text(&self.source[r]))
            .filter(|t| !t.contains('\n'));
        if self.find.is_none() {
            let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find"));
            let with = cx.new(|cx| InputState::new(window, cx).placeholder("Replace with"));
            let subs = vec![
                cx.subscribe_in(
                    &query,
                    window,
                    |this, _, ev: &InputEvent, window, cx| match ev {
                        InputEvent::Change => this.refind(cx),
                        InputEvent::PressEnter { shift, .. } => {
                            this.find_step(if *shift { -1 } else { 1 }, window, cx)
                        }
                        _ => {}
                    },
                ),
                cx.subscribe_in(&with, window, |this, _, ev: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = ev {
                        this.replace_current(window, cx);
                    }
                }),
            ];
            self.find = Some(Find {
                query,
                with,
                replacing: replace,
                case: false,
                matches: vec![],
                current: None,
                _subs: subs,
            });
        }
        let f = self.find.as_mut().expect("just made");
        f.replacing |= replace;
        let query = f.query.clone();
        query.update(cx, |s, cx| {
            if let Some(t) = picked {
                s.set_value(t, window, cx);
            }
            s.focus(window, cx);
        });
        self.refind(cx);
    }

    pub(super) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// The matches again, for the query as it is (after it or the source changed).
    pub(super) fn refind(&mut self, cx: &mut Context<Self>) {
        let Some(f) = self.find.as_ref() else { return };
        let q = f.query.read(cx).value().to_string();
        let blocks: Vec<Range<usize>> = self.blocks.iter().map(|b| b.range.clone()).collect();
        let matches = find::find(&self.source, &blocks, &q, f.case);
        let at = self
            .caret
            .as_ref()
            .map_or(0, |c| c.anchor.unwrap_or(c.off).min(c.off));
        if let Some(f) = self.find.as_mut() {
            // the current one: the match the selection is, else the first after the caret
            f.current = matches
                .iter()
                .position(|m| m.start >= at)
                .or((!matches.is_empty()).then_some(0));
            f.matches = matches;
        }
        cx.notify();
    }

    /// To the next (`dir` 1) or previous (-1) match, selected.
    pub(super) fn find_step(&mut self, dir: i32, _: &mut Window, cx: &mut Context<Self>) {
        let Some(f) = self.find.as_mut() else { return };
        if f.matches.is_empty() {
            return;
        }
        let n = f.matches.len() as i64;
        let sel = self
            .caret
            .as_ref()
            .map(|c| c.anchor.unwrap_or(c.off).min(c.off)..c.off.max(c.anchor.unwrap_or(c.off)));
        // from the match that is selected; else the current one is the next to go to
        let i = match (f.current, sel) {
            (Some(i), Some(s)) if f.matches[i] == s => {
                (i as i64 + dir as i64).rem_euclid(n) as usize
            }
            (Some(i), _) => i,
            (None, _) => 0,
        };
        f.current = Some(i);
        let m = f.matches[i].clone();
        self.select_match(m, cx);
    }

    fn select_match(&mut self, m: Range<usize>, cx: &mut Context<Self>) {
        self.set_caret(m.start, cx);
        self.extending(cx, |d, cx| {
            d.set_caret(m.end, cx);
        });
    }

    /// Replace the selected match (selecting it first when it is not) and go to the next.
    pub(super) fn replace_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(f) = self.find.as_ref() else { return };
        let Some(i) = f.current else { return };
        let m = f.matches[i].clone();
        if self.selection_range() != Some(m.clone()) {
            return self.select_match(m, cx);
        }
        let with = f.with.read(cx).value().to_string();
        let (r, text) = find::replace(&self.source, m, &with);
        self.checkpoint();
        self.replace(r.clone(), &text, cx);
        self.set_caret(r.start + text.len(), cx);
        self.refind(cx);
        self.find_step(1, window, cx);
    }

    /// Replace every match, as one undo step.
    pub(super) fn replace_all(&mut self, cx: &mut Context<Self>) {
        let Some(f) = self.find.as_ref() else { return };
        let with = f.with.read(cx).value().to_string();
        let matches = f.matches.clone();
        if matches.is_empty() {
            return;
        }
        self.checkpoint();
        for m in matches.iter().rev() {
            let (r, text) = find::replace(&self.source, m.clone(), &with);
            self.replace(r, &text, cx);
        }
        self.status = format!(
            "Replaced {} match{}",
            matches.len(),
            if matches.len() == 1 { "" } else { "es" }
        );
        self.refind(cx);
    }

    /// The matches on page `n`, the current one stronger.
    pub(super) fn render_matches(&self, n: u32, cx: &App) -> Vec<AnyElement> {
        let Some(f) = self.find.as_ref() else {
            return vec![];
        };
        let marks = palette::marks(self.dark_pages(cx), cx);
        let on_page = |m: &Range<usize>| {
            self.blocks.iter().any(|b| {
                b.range.start <= m.start
                    && m.end <= b.range.end
                    && b.rows.iter().any(|r| r.page == n)
            })
        };
        f.matches
            .iter()
            .enumerate()
            .filter(|(_, m)| on_page(m))
            .flat_map(|(i, m)| {
                let color = if f.current == Some(i) {
                    marks.match_current
                } else {
                    marks.match_other
                };
                self.highlights(n, m.clone(), color)
            })
            .collect()
    }

    /// The bar, over the pages' top right.
    pub(super) fn render_find_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let f = self.find.as_ref()?;
        let theme = cx.theme().clone();
        let count = match (f.current, f.matches.len()) {
            (_, 0) if f.query.read(cx).value().is_empty() => String::new(),
            (_, 0) => "No results".to_string(),
            (Some(i), n) => format!("{} of {n}", i + 1),
            (None, n) => format!("{n}"),
        };
        let tool = |id: &'static str, icon: &'static str, tip: &'static str| {
            Button::new(id)
                .icon(Icon::default().path(icon))
                .ghost()
                .xsmall()
                .tip(tip)
        };
        let row = div()
            .flex()
            .items_center()
            .gap_1()
            .child(div().w(px(220.0)).child(Input::new(&f.query).small()))
            .child(
                div()
                    .w(px(70.0))
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(count),
            )
            .child(
                tool("find-prev", "icons/chevron-up.svg", "Previous (⌘⇧G)")
                    .on_click(cx.listener(|d, _, w, cx| d.find_step(-1, w, cx))),
            )
            .child(
                tool("find-next", "icons/chevron-down.svg", "Next (⌘G)")
                    .on_click(cx.listener(|d, _, w, cx| d.find_step(1, w, cx))),
            )
            .child(
                tool("find-case", "icons/case-sensitive.svg", "Match case")
                    .selected(f.case)
                    .on_click(cx.listener(|d, _, _, cx| {
                        if let Some(f) = d.find.as_mut() {
                            f.case = !f.case;
                        }
                        d.refind(cx);
                    })),
            )
            .child(
                tool("find-replace", "icons/replace.svg", "Replace (⌘⌥F)")
                    .selected(f.replacing)
                    .on_click(cx.listener(|d, _, _, cx| {
                        if let Some(f) = d.find.as_mut() {
                            f.replacing = !f.replacing;
                        }
                        cx.notify();
                    })),
            )
            .child(
                tool("find-close", "icons/x.svg", "Close (Esc)")
                    .on_click(cx.listener(|d, _, w, cx| d.close_find(w, cx))),
            );
        let replace = f.replacing.then(|| {
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(div().w(px(220.0)).child(Input::new(&f.with).small()))
                .child(
                    Button::new("replace-one")
                        .label("Replace")
                        .xsmall()
                        .outline()
                        .on_click(cx.listener(|d, _, w, cx| d.replace_current(w, cx))),
                )
                .child(
                    Button::new("replace-all")
                        .label("All")
                        .xsmall()
                        .outline()
                        .on_click(cx.listener(|d, _, _, cx| d.replace_all(cx))),
                )
        });
        Some(
            div()
                .id("find-bar")
                .key_context("FindBar")
                .on_action(cx.listener(|d, _: &CloseFind, w, cx| d.close_find(w, cx)))
                .absolute()
                .top_2()
                .right_6()
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .shadow_md()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(row)
                .children(replace)
                .into_any_element(),
        )
    }
}
