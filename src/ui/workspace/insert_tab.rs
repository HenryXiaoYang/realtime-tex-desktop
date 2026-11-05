// The ribbon's Insert tab: pages (a page break, space, the contents), a table, a picture,
// formulas, a footnote, a link (⌘K, in a dialog), and a cross-reference or citation chosen
// from a searchable list of the document's labels or its .bib entries.
use super::*;
use crate::document::insert::bib_line;
use gpui_kit::component::WindowExt;
use gpui_kit::component::dialog::DialogFooter;

/// One row of a picker: its group, what it shows, and the TeX it puts in.
#[derive(Clone)]
struct PickItem {
    group: SharedString,
    title: SharedString,
    detail: SharedString,
    value: String,
}

type OnPick = Rc<dyn Fn(&str, &mut Window, &mut App)>;
type OnOk = dyn Fn(&mut Window, &mut App);

/// The most rows a picker draws at once (a .bib can hold thousands): typing narrows them down.
const PICKER_ROWS: usize = 200;

/// A searchable list in a dialog: typing filters it, ↑ and ↓ move through it, Enter or a click
/// takes a row.
struct Picker {
    input: Entity<InputState>,
    items: Vec<PickItem>,
    empty: &'static str,
    on_pick: OnPick,
    /// The row ↑/↓ have moved to (among the matches).
    selected: usize,
    scroll: ScrollHandle,
    _sub: Subscription,
}

impl Picker {
    fn matches(&self, cx: &App) -> Vec<&PickItem> {
        let q = self.input.read(cx).value().to_lowercase();
        self.items
            .iter()
            .filter(|it| {
                let hay = format!("{} {} {}", it.title, it.detail, it.value).to_lowercase();
                q.split_whitespace().all(|w| hay.contains(w))
            })
            .collect()
    }

    /// The selected row taken.
    fn take(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let m = self.matches(cx);
        if let Some(v) = m
            .get(self.selected.min(m.len().saturating_sub(1)))
            .map(|it| it.value.clone())
        {
            (self.on_pick)(&v, window, cx);
            window.close_dialog(cx);
        }
    }
}

impl Render for Picker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let mut list = div().flex().flex_col();
        let mut group: Option<SharedString> = None;
        let all: Vec<PickItem> = self.matches(cx).into_iter().cloned().collect();
        let total = all.len();
        self.selected = self.selected.min(total.saturating_sub(1));
        if all.is_empty() {
            list = list.child(
                div()
                    .p_2()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(if self.items.is_empty() {
                        self.empty
                    } else {
                        "Nothing matches. Try fewer letters"
                    }),
            );
        }
        // the list's children, headings included, to scroll the selected row into view
        let mut child = 0;
        let mut selected_child = 0;
        for (i, it) in all.into_iter().take(PICKER_ROWS).enumerate() {
            if group.as_ref() != Some(&it.group) {
                group = Some(it.group.clone());
                list = list.child(
                    div()
                        .px_2()
                        .pt_2()
                        .pb_1()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.muted_foreground)
                        .child(it.group.clone()),
                );
                child += 1;
            }
            if i == self.selected {
                selected_child = child;
            }
            let on_pick = self.on_pick.clone();
            let value = it.value.clone();
            let name = if it.detail.is_empty() {
                it.title.to_string()
            } else {
                format!("{}, {}", it.title, it.detail)
            };
            list = list.child(
                Button::new(("pick", i))
                    .ghost()
                    .w_full()
                    .h_auto()
                    .py_1()
                    .justify_start()
                    // ↑ and ↓ move through the rows: Tab goes past them
                    .tab_stop(false)
                    .selected(i == self.selected)
                    .accessibility_label(name)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_start()
                            .min_w_0()
                            .child(div().text_sm().child(it.title.clone()))
                            .when(!it.detail.is_empty(), |d| {
                                d.child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .child(it.detail.clone()),
                                )
                            }),
                    )
                    .on_click(move |_, window, cx| {
                        on_pick(&value, window, cx);
                        window.close_dialog(cx);
                    }),
            );
            child += 1;
        }
        if total > PICKER_ROWS {
            list = list.child(
                div()
                    .p_2()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!(
                        "Showing {PICKER_ROWS} of {total}. Type to narrow them down"
                    )),
            );
        }
        self.scroll.scroll_to_item(selected_child);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .capture_key_down(cx.listener(|p, ev: &KeyDownEvent, _, cx| {
                let n = p.matches(cx).len().min(PICKER_ROWS);
                match ev.keystroke.key.as_str() {
                    "down" if n > 0 => p.selected = (p.selected + 1).min(n - 1),
                    "up" => p.selected = p.selected.saturating_sub(1),
                    _ => return,
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .child(Input::new(&self.input))
            .child(
                div()
                    .id("picker-list")
                    .h(px(320.0))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(list),
            )
    }
}

impl Workspace {
    /// Whether the open document is a .tex file shown in Visual mode.
    pub(super) fn visual(&self, cx: &App) -> bool {
        self.doc().is_some_and(|d| {
            let d = d.read(cx);
            d.tex && !d.source_mode && d.view.is_none()
        })
    }

    /// The Insert tab's groups.
    pub(super) fn insert_body(&self, enabled: bool, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let pages = div()
            .flex()
            .gap_1()
            .child(self.big_tool(
                "ins-break",
                "icons/separator-horizontal.svg",
                "Page Break",
                enabled,
                cx,
                |this, _, cx| this.insert_block("\\newpage", cx),
            ))
            .child(self.space_menu(enabled, cx))
            .child(self.big_tool(
                "ins-toc",
                "icons/list-tree.svg",
                "Contents",
                enabled,
                cx,
                |this, _, cx| this.insert_block("\\tableofcontents", cx),
            ))
            .into_any_element();
        let tables = self.table_menu(enabled, cx);
        let pictures = self
            .big_tool(
                "ins-picture",
                "icons/image.svg",
                "Picture",
                enabled,
                cx,
                |this, _, cx| this.choose_picture(cx),
            )
            .into_any_element();
        let formulas = div()
            .flex()
            .gap_1()
            .child(self.big_tool(
                "ins-math",
                "icons/radical.svg",
                "Formula",
                enabled,
                cx,
                |this, window, cx| this.with_doc(cx, |d, cx| d.insert_math(window, cx)),
            ))
            .child(self.big_tool(
                "ins-display",
                "icons/sigma.svg",
                "Equation",
                enabled,
                cx,
                |this, window, cx| this.with_doc(cx, |d, cx| d.insert_display_math(window, cx)),
            ))
            .into_any_element();
        let links = div()
            .flex()
            .gap_1()
            .child(self.big_tool(
                "ins-link",
                "icons/link.svg",
                "Link",
                enabled,
                cx,
                |this, window, cx| this.open_link_dialog(window, cx),
            ))
            .child(self.big_tool(
                "ins-ref",
                "icons/bookmark.svg",
                "Cross-reference",
                enabled,
                cx,
                |this, window, cx| this.open_reference_picker(window, cx),
            ))
            .into_any_element();
        let refs = div()
            .flex()
            .gap_1()
            .child(self.big_tool(
                "ins-footnote",
                "icons/superscript.svg",
                "Footnote",
                enabled,
                cx,
                |this, window, cx| this.with_doc(cx, |d, cx| d.insert_footnote(window, cx)),
            ))
            .child(self.big_tool(
                "ins-cite",
                "icons/quote.svg",
                "Citation",
                enabled,
                cx,
                |this, window, cx| this.open_citation_picker(window, cx),
            ))
            .into_any_element();
        vec![
            Self::group("Pages", vec![pages], cx),
            Self::group("Tables", vec![tables], cx),
            Self::group("Illustrations", vec![pictures], cx),
            Self::group("Formulas", vec![formulas], cx),
            Self::group("Links", vec![links], cx),
            Self::last_group("References", vec![refs], cx),
        ]
    }

    /// A ribbon button with its icon over its name.
    fn big_tool(
        &self,
        id: &'static str,
        icon: &'static str,
        label: &'static str,
        enabled: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Button {
        widgets::big_button(id, icon, label, false, self.insert_compact(), cx)
            .disabled(!enabled)
            .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
    }

    pub(super) fn insert_block(&self, tex: &str, cx: &mut Context<Self>) {
        self.with_doc(cx, |d, cx| d.insert_block(tex, &[], cx));
    }

    /// Space: a menu of LaTeX's vertical skips.
    fn space_menu(&self, enabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let face = widgets::big_button(
            "ins-space",
            "icons/move-vertical.svg",
            "Space",
            true,
            self.insert_compact(),
            cx,
        );
        if !enabled {
            return face.disabled(true).into_any_element();
        }
        let weak = cx.entity().downgrade();
        gpui_kit::component::popover::Popover::new("ins-space-menu")
            .trigger(face)
            .content(move |_, _, cx| {
                let popover = cx.entity();
                let mut list = div().flex().flex_col().min_w(px(180.0));
                for (i, (label, tex)) in [
                    ("Small", "\\smallskip"),
                    ("Medium", "\\medskip"),
                    ("Large", "\\bigskip"),
                    ("Fill the rest of the page", "\\vfill"),
                ]
                .into_iter()
                .enumerate()
                {
                    let (weak, popover) = (weak.clone(), popover.clone());
                    list = list.child(
                        Button::new(("space", i))
                            .ghost()
                            .small()
                            .w_full()
                            .justify_start()
                            .label(label)
                            .on_click(move |_, window, cx| {
                                let _ = weak.update(cx, |this, cx| this.insert_block(tex, cx));
                                popover.update(cx, |p, cx| p.dismiss(window, cx));
                            }),
                    );
                }
                list
            })
            .into_any_element()
    }

    /// Table: a grid to size it, as in Word.
    fn table_menu(&self, enabled: bool, cx: &mut Context<Self>) -> AnyElement {
        if !enabled {
            return self
                .big_tool(
                    "ins-table-off",
                    "icons/table.svg",
                    "Table",
                    false,
                    cx,
                    |_, _, _| {},
                )
                .into_any_element();
        }
        let weak = cx.entity().downgrade();
        let face = widgets::big_button(
            "ins-table",
            "icons/table.svg",
            "Table",
            true,
            self.insert_compact(),
            cx,
        );
        widgets::table_grid(
            face,
            self.table_rows.clone(),
            self.table_cols.clone(),
            Rc::new(move |(rows, cols), _, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.with_doc(cx, |d, cx| d.insert_table(rows, cols, cx))
                });
            }),
        )
        .into_any_element()
    }

    /// The file chooser for a picture, then the picture as a figure at the caret.
    pub(super) fn choose_picture(&self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Insert".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update(cx, |this, cx| {
                    this.with_doc(cx, |d, cx| d.insert_pictures(&paths, cx))
                });
            }
        })
        .detach();
    }

    /// ⌘K: a link at the caret, its text the selection's.
    pub(super) fn open_link_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc().cloned() else {
            return;
        };
        if !self.visual(cx) {
            return;
        }
        let selected = doc.read(cx).selected_text();
        let text = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Text to show (optional)")
                .default_value(selected)
        });
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("https://"));
        let ok: Rc<OnOk> = {
            let (doc, text, url) = (doc.clone(), text.clone(), url.clone());
            Rc::new(move |window, cx| {
                let (t, u) = (
                    text.read(cx).value().to_string(),
                    url.read(cx).value().to_string(),
                );
                // no address yet: the dialog stays for one
                if u.trim().is_empty() {
                    return;
                }
                doc.update(cx, |d, cx| d.insert_link(&u, &t, cx));
                window.close_dialog(cx);
            })
        };
        // Enter in either field inserts
        self._dialog_subs = [&url, &text]
            .into_iter()
            .map(|input| {
                let ok = ok.clone();
                cx.subscribe_in(input, window, move |_, _, ev: &InputEvent, window, cx| {
                    if matches!(ev, InputEvent::PressEnter { .. }) {
                        ok(window, cx);
                    }
                })
            })
            .collect();
        let first = std::cell::Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            if first.replace(false) {
                url.focus_handle(cx).focus(window, cx);
            }
            let ok = ok.clone();
            dialog
                .title("Insert Link")
                .w(px(420.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(div().text_xs().child("Address"))
                        .child(Input::new(&url))
                        .child(div().text_xs().child("Text"))
                        .child(Input::new(&text)),
                )
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new("link-cancel")
                                .label("Cancel")
                                .outline()
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("link-ok")
                                .label("Insert")
                                .primary()
                                .on_click(move |_, window, cx| ok(window, cx)),
                        ),
                )
        });
    }

    /// Insert → Cross-reference: the document's labels, by kind.
    pub(super) fn open_reference_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc().cloned() else {
            return;
        };
        let mut labels = doc.read(cx).labels();
        let order = [
            "Chapter", "Section", "Figure", "Table", "Equation", "Item", "Label",
        ];
        labels.sort_by_key(|l| order.iter().position(|k| *k == l.kind));
        let items = labels
            .iter()
            .map(|l| PickItem {
                group: match l.kind {
                    "Chapter" | "Section" => "Headings".into(),
                    "Figure" => "Figures".into(),
                    "Table" => "Tables".into(),
                    "Equation" => "Equations".into(),
                    "Item" => "Numbered items".into(),
                    _ => "Other labels".into(),
                },
                title: if l.title.is_empty() {
                    l.key.clone().into()
                } else {
                    l.title.clone().into()
                },
                detail: if l.title.is_empty() {
                    SharedString::default()
                } else {
                    l.key.clone().into()
                },
                value: l.reference(),
            })
            .collect();
        let d = doc.clone();
        self.open_picker(
            "Insert Cross-reference",
            "No labels yet: give a figure, table, heading or equation a label in the sidebar.",
            items,
            Rc::new(move |tex, _, cx| d.update(cx, |d, cx| d.insert_inline(tex, &[], cx))),
            window,
            cx,
        );
    }

    /// Insert → Citation: the entries of the document's .bib files.
    pub(super) fn open_citation_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc().cloned() else {
            return;
        };
        let items = doc
            .read(cx)
            .bib_entries()
            .iter()
            .map(|e| PickItem {
                group: "Sources".into(),
                title: e.key.clone().into(),
                detail: bib_line(e).into(),
                value: e.key.clone(),
            })
            .collect();
        let d = doc.clone();
        self.open_picker(
            "Insert Citation",
            "No sources yet: put a .bib file beside the document, or name one with \\bibliography{…}.",
            items,
            Rc::new(move |key, _, cx| d.update(cx, |d, cx| d.insert_citation(key, cx))),
            window,
            cx,
        );
    }

    fn open_picker(
        &mut self,
        title: &'static str,
        empty: &'static str,
        items: Vec<PickItem>,
        on_pick: OnPick,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.visual(cx) {
            return;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let picker = cx.new(|cx| {
            let sub = cx.subscribe_in(
                &input,
                window,
                |p: &mut Picker, _, ev: &InputEvent, window, cx| match ev {
                    InputEvent::PressEnter { .. } => p.take(window, cx),
                    InputEvent::Change => {
                        p.selected = 0;
                        cx.notify();
                    }
                    _ => {}
                },
            );
            Picker {
                input: input.clone(),
                items,
                empty,
                on_pick,
                selected: 0,
                scroll: ScrollHandle::new(),
                _sub: sub,
            }
        });
        let first = std::cell::Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            if first.replace(false) {
                input.focus_handle(cx).focus(window, cx);
            }
            dialog.title(title).w(px(460.0)).child(picker.clone())
        });
    }
}
