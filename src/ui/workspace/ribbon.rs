// The ribbon under the file tabs, as in Word: the Home tab's groups (clipboard, font, paragraph,
// styles) acting on the selection or the caret of a .tex file in Visual mode, its buttons lit by
// what the text at the caret is set in; the Insert tab is in insert_tab.rs. A double-click on a
// tab folds the ribbon to its tab row.
use super::*;
use crate::document::format::{Align, Font, SIZES, Size, Style};
use crate::ui::Tip;
use crate::ui::editor::{FormatOp, RibbonState};
use gpui_kit::component::input::{Copy, Cut, Paste};

/// The ribbon's tabs.
#[derive(Clone, Copy, PartialEq, Default)]
pub(super) enum RibbonTab {
    #[default]
    Home,
    Insert,
}

/// The installed font families, sorted, for the font menu. Listing them costs CoreText some 55 ms
/// and showing each in its face loads it (about 140 ms for all, the first time): done once, off
/// the main thread, at launch, so the menu opens at once.
static FONT_NAMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

fn list_fonts(ts: &TextSystem) -> Vec<String> {
    let mut names = ts.all_font_names();
    names.sort();
    names.dedup();
    names.retain(|n| !n.starts_with('.'));
    names
}

/// The font list read, and every face loaded, in the background.
pub(super) fn warm_fonts(cx: &App) {
    let ts = cx.text_system().clone();
    cx.background_executor()
        .spawn(async move {
            let names = FONT_NAMES.get_or_init(|| list_fonts(&ts));
            for n in names {
                let id = ts.resolve_font(&font(n.clone()));
                let _ = ts.advance(id, px(13.0), 'A');
            }
        })
        .detach();
}

/// The installed families (listed now if the background has not yet).
fn font_names(cx: &App) -> &'static [String] {
    FONT_NAMES.get_or_init(|| list_fonts(cx.text_system()))
}

/// Faces that show what Serif, Sans and Mono look like (the system's, always there).
const SERIF_FACE: &str = "Times New Roman";
const SANS_FACE: &str = "Helvetica Neue";
const MONO_FACE: &str = "Menlo";

/// How big a named size shows in its menu (pixels, roughly LaTeX's at 10 pt).
fn size_px(s: &str) -> f32 {
    match s {
        "tiny" => 9.0,
        "scriptsize" => 10.0,
        "footnotesize" => 11.0,
        "small" => 12.0,
        "normalsize" => 13.0,
        "large" => 15.0,
        "Large" => 17.0,
        "LARGE" => 20.0,
        "huge" => 23.0,
        _ => 26.0,
    }
}

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
    pub(super) fn format(&self, op: FormatOp, window: &mut Window, cx: &mut Context<Self>) {
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
        let tab = self.ribbon_tab;
        // the kit's tab list: VoiceOver hears "Home, tab 1 of 2, selected"
        let tab_ix = match tab {
            RibbonTab::Home => 0,
            RibbonTab::Insert => 1,
        };
        let mut bar = TabBar::new("ribbon-tabs")
            .underline()
            .small()
            .selected_index(tab_ix);
        for (name, t) in [("Home", RibbonTab::Home), ("Insert", RibbonTab::Insert)] {
            bar = bar.child(Tab::new().label(name).on_click(cx.listener(
                move |this, ev: &ClickEvent, _, cx| {
                    // a double-click folds it or opens it again; a click on a folded one
                    // (or another tab) opens it
                    let folded = settings::AppSettings::get(cx).ribbon_collapsed;
                    if ev.click_count() >= 2 || folded {
                        settings::AppSettings::set(cx, |s| s.ribbon_collapsed = !folded);
                    }
                    this.ribbon_tab = t;
                    cx.notify();
                },
            )));
        }
        let tabs = div()
            .id("ribbon-tabs-row")
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .h_7()
            .tooltip(|window, cx| {
                gpui_kit::component::tooltip::Tooltip::new("Double-click a tab to fold the ribbon")
                    .build(window, cx)
            })
            .child(bar);
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
        let groups = match tab {
            // at full size: a narrow window scrolls the ribbon
            RibbonTab::Home => vec![
                self.clipboard_group(cx),
                self.font_group(&state, cx),
                self.paragraph_group(&state, cx),
                self.styles_group(&state, cx),
            ],
            RibbonTab::Insert => self.insert_body(state.enabled, cx),
        };
        let body = div()
            .id("ribbon-body")
            .flex()
            .gap_3()
            .px_3()
            .pt_2()
            .pb_2()
            .overflow_x_scroll()
            .track_scroll(&self.fit.ribbon_scroll)
            .children(groups);
        // the ribbon's width, and (the Insert tab) whether its buttons' names fit, for next frame
        let body = div()
            .relative()
            .child(self.measure_ribbon())
            .child(body)
            .child(self.watch_insert(tab == RibbonTab::Insert));
        Some(ribbon.child(body).into_any_element())
    }

    /// Colour `spec` on the selection (or at the caret), and the colour button's from now on.
    pub(super) fn pick_color(&mut self, spec: String, window: &mut Window, cx: &mut Context<Self>) {
        self.last_color = spec.clone();
        self.format(FormatOp::Color(spec), window, cx);
    }

    /// A group of the ribbon: its tools, and its name under them; a rule after it.
    pub(super) fn group(name: &'static str, rows: Vec<AnyElement>, cx: &App) -> AnyElement {
        Self::group_ruled(name, rows, true, cx)
    }

    /// The tab's last group, with no rule after it.
    pub(super) fn last_group(name: &'static str, rows: Vec<AnyElement>, cx: &App) -> AnyElement {
        Self::group_ruled(name, rows, false, cx)
    }

    /// A group at least as wide as its tools (a narrow window scrolls the ribbon, it does not
    /// squeeze them); the room the ribbon has beyond, shared out among its groups, its tools
    /// in the middle of theirs.
    fn group_ruled(name: &'static str, rows: Vec<AnyElement>, rule: bool, cx: &App) -> AnyElement {
        let theme = cx.theme();
        div()
            .flex()
            .flex_col()
            .flex_grow(1.0)
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .when(rule, |d| d.pr_3().border_r_1().border_color(theme.border))
            .children(rows)
            .child(
                // at the group's foot, so the names line up across the ribbon
                div()
                    .mt_auto()
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
            .tip(tip)
            .on_click(cx.listener(move |this, _, window, cx| this.format(op.clone(), window, cx)))
    }

    fn clipboard_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let act = |id: &'static str, icon: &'static str, tip: &'static str, a: Box<dyn Action>| {
            Button::new(id)
                .icon(Icon::default().path(icon))
                .ghost()
                .small()
                .tip(tip)
                .on_click(move |_, window, cx| window.dispatch_action(a.boxed_clone(), cx))
        };
        // paste, cut and copy above; undo and redo centred under them
        let top = div()
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
            .into_any_element();
        let bottom = div()
            .flex()
            .justify_center()
            .gap_0p5()
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
        Self::group("Clipboard", vec![top, bottom], cx)
    }

    fn font_group(&self, st: &RibbonState, cx: &mut Context<Self>) -> AnyElement {
        let en = st.enabled;
        let weak = cx.entity().downgrade();
        // the font at the caret, shown in itself
        let (family, face): (SharedString, &str) = if let Some(f) = &st.font {
            (f.clone().into(), f.as_str())
        } else if st.has("texttt") {
            ("Mono".into(), MONO_FACE)
        } else if st.has("textsf") {
            ("Sans".into(), SANS_FACE)
        } else {
            ("Serif".into(), SERIF_FACE)
        };
        let face: SharedString = face.to_string().into();
        let spoken_family = family.clone();
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
            let now = st.font.clone();
            Button::new("rb-font")
                .child(
                    div()
                        .w(px(104.0))
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .font_family(face)
                        .child(family),
                )
                .outline()
                .small()
                .disabled(!en)
                .dropdown_caret(true)
                .tooltip("Font")
                .accessibility_label(format!("Font: {spoken_family}"))
                .dropdown_menu(move |mut menu, _, cx| {
                    // each name in its own face, as Word shows them
                    let face_item = |label: SharedString, face: SharedString| {
                        PopupMenuItem::element(move |_, _| {
                            div().font_family(face.clone()).child(label.clone())
                        })
                    };
                    menu = menu.label("Document fonts");
                    for (label, face, f) in [
                        ("Serif", SERIF_FACE, Font::Serif),
                        ("Sans", SANS_FACE, Font::Sans),
                        ("Mono", MONO_FACE, Font::Mono),
                    ] {
                        menu = menu.item(
                            face_item(label.into(), face.into()).on_click(pick(FormatOp::Font(f))),
                        );
                    }
                    menu = menu.separator().label("Installed fonts (LuaLaTeX)");
                    for n in font_names(cx).iter().cloned() {
                        let checked = now.as_deref() == Some(n.as_str());
                        menu = menu.item(
                            face_item(n.clone().into(), n.clone().into())
                                .checked(checked)
                                .on_click(pick(FormatOp::Font(Font::Named(n)))),
                        );
                    }
                    menu.scrollable(true).max_h(px(480.0)).min_w(px(240.0))
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
                .accessibility_label(format!("Size: {}", size_label(now)))
                .dropdown_menu(move |mut menu, _, _| {
                    for s in SIZES {
                        let px_size = px(size_px(s));
                        menu = menu.item(
                            PopupMenuItem::element(move |_, _| {
                                div().text_size(px_size).child(size_label(s))
                            })
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
            let (pick, put) = (weak.clone(), weak.clone());
            widgets::color_palette(
                st.color.clone(),
                &self.last_color,
                self.color_hex.clone(),
                Rc::new(move |spec, window, cx| {
                    let _ = pick.update(cx, |this, cx| this.pick_color(spec, window, cx));
                }),
                Rc::new(move |(), window, cx| {
                    let _ = put.update(cx, |this, cx| {
                        this.format(FormatOp::Color(this.last_color.clone()), window, cx)
                    });
                }),
                en,
                cx,
            )
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
        let mut styles: Vec<(&'static str, Style)> =
            vec![("Normal", Style::Normal), ("Title", Style::Title)];
        for h in &st.headings {
            let label = match *h {
                "part" => "Part",
                "chapter" => "Chapter",
                // LaTeX's own names, as the source has them
                "section" => "Section",
                "subsection" => "Subsection",
                "subsubsection" => "Subsubsection",
                _ => "Paragraph",
            };
            styles.push((label, Style::Heading(h)));
        }
        styles.push(("Quote", Style::Quote));
        // two rows of small buttons, as the font's and the paragraph's tools are, filled column
        // by column (Normal over Title, Part over Section…) and spread across the group
        let weight = |style: Style| match style {
            Style::Heading(_) => FontWeight::BOLD,
            _ => FontWeight::NORMAL,
        };
        let mut row = div().w_full().flex().justify_center().gap_1();
        for (c, pair) in styles.chunks(2).enumerate() {
            let mut column = div().flex_1().max_w(px(160.0)).flex().flex_col().gap_1();
            for (k, &(label, style)) in pair.iter().enumerate() {
                column = column.child(
                    Button::new(("rb-style", c * 2 + k))
                        .ghost()
                        .small()
                        .w_full()
                        .selected(st.style == Some(style))
                        .disabled(!st.enabled)
                        .accessibility_label(format!("Style: {label}"))
                        // each style named in its own look
                        .child(
                            div()
                                .text_sm()
                                .font_weight(weight(style))
                                .whitespace_nowrap()
                                .when(style == Style::Quote, |d| d.italic())
                                .child(label),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.format(FormatOp::Style(style), window, cx)
                        })),
                );
            }
            row = row.child(column);
        }
        Self::last_group("Styles", vec![row.into_any_element()], cx)
    }
}
