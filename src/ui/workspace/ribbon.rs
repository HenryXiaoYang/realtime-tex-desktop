// The ribbon under the file tabs, as in Word: the Home tab's groups (clipboard, font, paragraph,
// styles) acting on the selection or the caret of a .tex file in Visual mode, its buttons lit by
// what the text at the caret is set in. A double-click on the tab folds it to its tab row.
use super::*;
use crate::document::format::{Align, Font, SIZES, Size, Style};
use crate::ui::editor::{FormatOp, RibbonState};
use gpui_kit::component::input::{Copy, Cut, Paste};

/// Colours offered for text: xcolor's base names.
const COLORS: &[(&str, &str)] = &[
    ("Black", "black"),
    ("Dark gray", "darkgray"),
    ("Gray", "gray"),
    ("Red", "red"),
    ("Orange", "orange"),
    ("Brown", "brown"),
    ("Olive", "olive"),
    ("Green", "green"),
    ("Teal", "teal"),
    ("Cyan", "cyan"),
    ("Blue", "blue"),
    ("Violet", "violet"),
    ("Purple", "purple"),
    ("Magenta", "magenta"),
];

/// Point sizes offered beside the named ones.
const POINTS: &[f32] = &[
    8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 36.0,
];

/// How a named size reads in the menu.
fn size_label(s: &str) -> &'static str {
    match s {
        "tiny" => "Tiny",
        "scriptsize" => "Script",
        "footnotesize" => "Footnote",
        "small" => "Small",
        "normalsize" => "Normal",
        "large" => "Large",
        "Large" => "Larger",
        "LARGE" => "Largest",
        "huge" => "Huge",
        _ => "Hugest",
    }
}

impl Workspace {
    /// Apply a formatting operation to the open document.
    fn format(&self, op: FormatOp, window: &mut Window, cx: &mut Context<Self>) {
        self.with_doc(cx, |d, cx| d.format(op, window, cx));
    }

    /// The ribbon, for a .tex file in Visual mode.
    pub(super) fn render_ribbon(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let doc = self.doc()?.read(cx);
        if !doc.tex || doc.source_mode || doc.view.is_some() {
            return None;
        }
        let state = doc.ribbon_state();
        let theme = cx.theme().clone();
        let collapsed = settings::AppSettings::get(cx).ribbon_collapsed;
        let tabs = div()
            .id("ribbon-tabs")
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .h_7()
            .child(
                div()
                    .id("ribbon-home")
                    .px_2()
                    .py_0p5()
                    .rounded_md()
                    .text_sm()
                    .cursor_pointer()
                    .when(!collapsed, |d| d.bg(theme.secondary))
                    .hover(|d| d.bg(theme.secondary))
                    .child("Home")
                    .tooltip(|window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(
                            "Double-click to fold the ribbon",
                        )
                        .build(window, cx)
                    })
                    .on_click(|ev, _, cx| {
                        // a double-click folds it or opens it again; a click on a folded one opens it
                        let folded = settings::AppSettings::get(cx).ribbon_collapsed;
                        if ev.click_count() >= 2 || folded {
                            settings::AppSettings::set(cx, |s| s.ribbon_collapsed = !folded);
                        }
                    }),
            );
        let ribbon = div()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.title_bar)
            .child(tabs);
        if collapsed {
            return Some(ribbon.into_any_element());
        }
        let body = div()
            .id("ribbon-home-body")
            .flex()
            .items_start()
            .gap_3()
            .px_3()
            .pb_2()
            .overflow_x_scroll()
            .child(self.clipboard_group(cx))
            .child(self.font_group(&state, cx))
            .child(self.paragraph_group(&state, cx))
            .child(self.styles_group(&state, cx));
        Some(ribbon.child(body).into_any_element())
    }

    /// A group of the ribbon: its tools, and its name under them.
    fn group(name: &'static str, rows: Vec<AnyElement>, cx: &App) -> AnyElement {
        let theme = cx.theme();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .pr_3()
            .border_r_1()
            .border_color(theme.border)
            .children(rows)
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .text_center()
                    .child(name),
            )
            .into_any_element()
    }

    /// A tool button: an icon, lit when `on`, with a tooltip.
    fn tool(
        &self,
        id: &'static str,
        icon: &'static str,
        tip: &'static str,
        (on, enabled): (bool, bool),
        op: FormatOp,
        cx: &mut Context<Self>,
    ) -> Button {
        Button::new(id)
            .icon(Icon::default().path(icon))
            .ghost()
            .small()
            .selected(on)
            .disabled(!enabled)
            .tooltip(tip)
            .on_click(cx.listener(move |this, _, window, cx| this.format(op.clone(), window, cx)))
    }

    fn clipboard_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let act = |id: &'static str, icon: &'static str, tip: &'static str, a: Box<dyn Action>| {
            Button::new(id)
                .icon(Icon::default().path(icon))
                .ghost()
                .small()
                .tooltip(tip)
                .on_click(move |_, window, cx| window.dispatch_action(a.boxed_clone(), cx))
        };
        let row = div()
            .flex()
            .gap_0p5()
            .child(act(
                "rb-paste",
                "icons/clipboard-paste.svg",
                "Paste (⌘V)",
                Box::new(Paste),
            ))
            .child(act(
                "rb-cut",
                "icons/scissors.svg",
                "Cut (⌘X)",
                Box::new(Cut),
            ))
            .child(act(
                "rb-copy",
                "icons/copy.svg",
                "Copy (⌘C)",
                Box::new(Copy),
            ))
            .child(act(
                "rb-undo",
                "icons/undo.svg",
                "Undo (⌘Z)",
                Box::new(Undo),
            ))
            .child(act(
                "rb-redo",
                "icons/redo.svg",
                "Redo (⌘⇧Z)",
                Box::new(Redo),
            ))
            .into_any_element();
        Self::group("Clipboard", vec![row], cx)
    }

    fn font_group(&self, st: &RibbonState, cx: &mut Context<Self>) -> AnyElement {
        let en = st.enabled;
        let weak = cx.entity().downgrade();
        let family = if st.has("texttt") {
            "Mono"
        } else if st.has("textsf") {
            "Sans"
        } else if st.formats.iter().any(|f| f == "fontspec") {
            "Font…"
        } else {
            "Serif"
        };
        let pick = {
            let weak = weak.clone();
            move |op: FormatOp| {
                let weak = weak.clone();
                move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                    let _ = weak.update(cx, |this, cx| this.format(op.clone(), window, cx));
                }
            }
        };
        let font_menu = {
            let pick = pick.clone();
            Button::new("rb-font")
                .label(family)
                .outline()
                .small()
                .w(px(130.0))
                .disabled(!en)
                .dropdown_caret(true)
                .tooltip("Font")
                .dropdown_menu(move |mut menu, _, cx| {
                    for (label, f) in [
                        ("Serif", Font::Serif),
                        ("Sans", Font::Sans),
                        ("Mono", Font::Mono),
                    ] {
                        menu =
                            menu.item(PopupMenuItem::new(label).on_click(pick(FormatOp::Font(f))));
                    }
                    menu = menu.separator().label("Installed fonts (LuaLaTeX)");
                    let mut names = cx.text_system().all_font_names();
                    names.sort();
                    names.dedup();
                    for n in names.into_iter().filter(|n| !n.starts_with('.')) {
                        menu = menu.item(
                            PopupMenuItem::new(n.clone())
                                .on_click(pick(FormatOp::Font(Font::Named(n)))),
                        );
                    }
                    menu.scrollable(true).max_h(px(420.0))
                })
        };
        let size_menu = {
            let pick = pick.clone();
            let now = st.size;
            Button::new("rb-size")
                .label(size_label(now))
                .outline()
                .small()
                .w(px(90.0))
                .disabled(!en)
                .dropdown_caret(true)
                .tooltip("Size")
                .dropdown_menu(move |mut menu, _, _| {
                    for s in SIZES {
                        menu = menu.item(
                            PopupMenuItem::new(size_label(s))
                                .checked(*s == now)
                                .on_click(pick(FormatOp::Size(Size::Named(s)))),
                        );
                    }
                    menu = menu.separator().label("Points");
                    for p in POINTS {
                        menu = menu.item(
                            PopupMenuItem::new(format!("{p} pt"))
                                .on_click(pick(FormatOp::Size(Size::Pt(*p)))),
                        );
                    }
                    menu.scrollable(true).max_h(px(420.0))
                })
        };
        let color_menu = {
            let pick = pick.clone();
            Button::new("rb-color")
                .icon(Icon::default().path("icons/palette.svg"))
                .ghost()
                .small()
                .disabled(!en)
                .dropdown_caret(true)
                .tooltip("Text colour")
                .dropdown_menu(move |mut menu, _, _| {
                    for (label, c) in COLORS {
                        menu = menu.item(
                            PopupMenuItem::new(*label)
                                .on_click(pick(FormatOp::Color(c.to_string()))),
                        );
                    }
                    menu
                })
        };
        let top = div()
            .flex()
            .gap_1()
            .child(font_menu)
            .child(size_menu)
            .child(self.tool(
                "rb-grow",
                "icons/a-arrow-up.svg",
                "Bigger (⌘⇧.)",
                (false, en),
                FormatOp::Grow(1),
                cx,
            ))
            .child(self.tool(
                "rb-shrink",
                "icons/a-arrow-down.svg",
                "Smaller (⌘⇧,)",
                (false, en),
                FormatOp::Grow(-1),
                cx,
            ))
            .into_any_element();
        let inline = |this: &Self, id, icon, tip, cmd: &'static str, cx: &mut Context<Self>| {
            this.tool(id, icon, tip, (st.has(cmd), en), FormatOp::Inline(cmd), cx)
        };
        let bottom = div()
            .flex()
            .gap_0p5()
            .child(inline(
                self,
                "rb-bold",
                "icons/bold.svg",
                "Bold (⌘B)",
                "textbf",
                cx,
            ))
            .child(inline(
                self,
                "rb-italic",
                "icons/italic.svg",
                "Italic (⌘I)",
                "textit",
                cx,
            ))
            .child(inline(
                self,
                "rb-underline",
                "icons/underline.svg",
                "Underline (⌘U)",
                "underline",
                cx,
            ))
            .child(inline(
                self,
                "rb-strike",
                "icons/strikethrough.svg",
                "Strikethrough",
                "sout",
                cx,
            ))
            .child(inline(
                self,
                "rb-sub",
                "icons/subscript.svg",
                "Subscript",
                "textsubscript",
                cx,
            ))
            .child(inline(
                self,
                "rb-sup",
                "icons/superscript.svg",
                "Superscript",
                "textsuperscript",
                cx,
            ))
            .child(inline(
                self,
                "rb-caps",
                "icons/case-sensitive.svg",
                "Small caps",
                "textsc",
                cx,
            ))
            .child(color_menu)
            .into_any_element();
        Self::group("Font", vec![top, bottom], cx)
    }

    fn paragraph_group(&self, st: &RibbonState, cx: &mut Context<Self>) -> AnyElement {
        let en = st.enabled;
        let list = st.list.as_deref();
        let align = st.align;
        let top = div()
            .flex()
            .gap_0p5()
            .child(self.tool(
                "rb-bullets",
                "icons/list.svg",
                "Bullets",
                (list == Some("itemize"), en),
                FormatOp::List("itemize"),
                cx,
            ))
            .child(self.tool(
                "rb-numbers",
                "icons/list-ordered.svg",
                "Numbering",
                (list == Some("enumerate"), en),
                FormatOp::List("enumerate"),
                cx,
            ))
            .child(self.tool(
                "rb-outdent",
                "icons/list-indent-decrease.svg",
                "Decrease indent (⇧Tab)",
                (false, en && list.is_some()),
                FormatOp::Indent(false),
                cx,
            ))
            .child(self.tool(
                "rb-indent",
                "icons/list-indent-increase.svg",
                "Increase indent (Tab)",
                (false, en && list.is_some()),
                FormatOp::Indent(true),
                cx,
            ))
            .into_any_element();
        let bottom = div()
            .flex()
            .gap_0p5()
            .child(self.tool(
                "rb-left",
                "icons/text-align-start.svg",
                "Align left (⌘L)",
                (align == Some(Align::Left), en),
                FormatOp::Align(Align::Left),
                cx,
            ))
            .child(self.tool(
                "rb-center",
                "icons/text-align-center.svg",
                "Center (⌘E)",
                (align == Some(Align::Center), en),
                FormatOp::Align(Align::Center),
                cx,
            ))
            .child(self.tool(
                "rb-right",
                "icons/text-align-end.svg",
                "Align right (⌘R)",
                (align == Some(Align::Right), en),
                FormatOp::Align(Align::Right),
                cx,
            ))
            .child(self.tool(
                "rb-justify",
                "icons/text-align-justify.svg",
                "Justify (⌘J)",
                (align == Some(Align::Justify), en),
                FormatOp::Align(Align::Justify),
                cx,
            ))
            .into_any_element();
        Self::group("Paragraph", vec![top, bottom], cx)
    }

    fn styles_group(&self, st: &RibbonState, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let mut styles: Vec<(&'static str, Style)> =
            vec![("Normal", Style::Normal), ("Title", Style::Title)];
        for h in &st.headings {
            let label = match *h {
                "part" => "Part",
                "chapter" => "Chapter",
                "section" => "Heading 1",
                "subsection" => "Heading 2",
                "subsubsection" => "Heading 3",
                _ => "Heading 4",
            };
            styles.push((label, Style::Heading(h)));
        }
        styles.push(("Quote", Style::Quote));
        let mut row = div().flex().gap_1();
        for (i, (label, style)) in styles.into_iter().enumerate() {
            let on = st.style == Some(style);
            let (size, weight) = match style {
                Style::Title => (px(15.0), FontWeight::NORMAL),
                Style::Heading("part" | "chapter") => (px(14.0), FontWeight::BOLD),
                Style::Heading("section") => (px(13.0), FontWeight::BOLD),
                Style::Heading(_) => (px(12.0), FontWeight::BOLD),
                _ => (px(12.0), FontWeight::NORMAL),
            };
            row = row.child(
                div()
                    .id(("rb-style", i))
                    .w(px(78.0))
                    .h(px(44.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .border_1()
                    .border_color(if on { theme.primary } else { theme.border })
                    .bg(theme.background)
                    .text_size(size)
                    .font_weight(weight)
                    .when(style == Style::Quote, |d| d.italic())
                    .cursor_pointer()
                    .hover(|d| d.bg(theme.secondary))
                    .when(!st.enabled, |d| d.opacity(0.5))
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.format(FormatOp::Style(style), window, cx)
                    })),
            );
        }
        Self::group("Styles", vec![row.into_any_element()], cx)
    }
}
