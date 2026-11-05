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

/// A searchable list in a dialog; Enter or a click takes a row.
struct Picker {
    input: Entity<InputState>,
    items: Vec<PickItem>,
    empty: &'static str,
    on_pick: OnPick,
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
}

impl Render for Picker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let mut list = div().flex().flex_col();
        let mut group: Option<SharedString> = None;
        let matches: Vec<PickItem> = self.matches(cx).into_iter().cloned().collect();
        if matches.is_empty() {
            list = list.child(
                div()
                    .p_2()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(if self.items.is_empty() {
                        self.empty
                    } else {
                        "Nothing matches"
                    }),
            );
        }
        for (i, it) in matches.into_iter().enumerate() {
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
            }
            let on_pick = self.on_pick.clone();
            let value = it.value.clone();
            list = list.child(
                div()
                    .id(("pick", i))
                    .flex()
                    .flex_col()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|d| d.bg(theme.secondary))
                    .child(div().text_sm().child(it.title.clone()))
                    .when(!it.detail.is_empty(), |d| {
                        d.child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(it.detail.clone()),
                        )
                    })
                    .on_click(move |_, window, cx| {
                        on_pick(&value, window, cx);
                        window.close_dialog(cx);
                    }),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(Input::new(&self.input))
            .child(
                div()
                    .id("picker-list")
                    .h(px(320.0))
                    .overflow_y_scroll()
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
            Self::group("References", vec![refs], cx),
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
    ) -> Stateful<Div> {
        let theme = cx.theme().clone();
        div()
            .id(id)
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .min_w(px(56.0))
            .rounded_md()
            .text_xs()
            .when(enabled, |d| {
                d.cursor_pointer()
                    .hover(|d| d.bg(theme.secondary))
                    .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
            })
            .when(!enabled, |d| d.opacity(0.5))
            .child(Icon::default().path(icon).size(px(20.0)))
            .child(label)
    }

    fn insert_block(&self, tex: &str, cx: &mut Context<Self>) {
        self.with_doc(cx, |d, cx| d.insert_block(tex, &[], cx));
    }

    fn space_menu(&self, enabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let weak = cx.entity().downgrade();
        Button::new("ins-space")
            .icon(Icon::default().path("icons/move-vertical.svg"))
            .label("Space")
            .ghost()
            .small()
            .disabled(!enabled)
            .dropdown_caret(true)
            .tooltip("Vertical space")
            .dropdown_menu(move |mut menu, _, _| {
                for (label, tex) in [
                    ("Small", "\\smallskip"),
                    ("Medium", "\\medskip"),
                    ("Large", "\\bigskip"),
                    ("Fill the page", "\\vfill"),
                ] {
                    let weak = weak.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        let _ = weak.update(cx, |this, cx| this.insert_block(tex, cx));
                    }));
                }
                menu
            })
            .into_any_element()
    }

    fn table_menu(&self, enabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let weak = cx.entity().downgrade();
        Button::new("ins-table")
            .icon(Icon::default().path("icons/table.svg"))
            .label("Table")
            .ghost()
            .small()
            .disabled(!enabled)
            .dropdown_caret(true)
            .tooltip("Table")
            .dropdown_menu(move |mut menu, _, _| {
                for (rows, cols) in [(2, 2), (3, 3), (4, 3), (4, 4), (5, 4), (6, 5)] {
                    let weak = weak.clone();
                    menu = menu.item(
                        PopupMenuItem::new(format!("{rows} rows × {cols} columns")).on_click(
                            move |_, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.with_doc(cx, |d, cx| d.insert_table(rows, cols, cx))
                                });
                            },
                        ),
                    );
                }
                menu
            })
            .into_any_element()
    }

    /// The file chooser for a picture, then the picture as a figure at the caret.
    fn choose_picture(&self, cx: &mut Context<Self>) {
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
                doc.update(cx, |d, cx| d.insert_link(&u, &t, cx));
                window.close_dialog(cx);
            })
        };
        let sub = {
            let ok = ok.clone();
            cx.subscribe_in(&url, window, move |_, _, ev: &InputEvent, window, cx| {
                if matches!(ev, InputEvent::PressEnter { .. }) {
                    ok(window, cx);
                }
            })
        };
        self._dialog_sub = Some(sub);
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
    fn open_reference_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    fn open_citation_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            "No .bib file with entries beside the document.",
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
                    InputEvent::PressEnter { .. } => {
                        if let Some(v) = p.matches(cx).first().map(|it| it.value.clone()) {
                            (p.on_pick)(&v, window, cx);
                            window.close_dialog(cx);
                        }
                    }
                    InputEvent::Change => cx.notify(),
                    _ => {}
                },
            );
            Picker {
                input: input.clone(),
                items,
                empty,
                on_pick,
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
