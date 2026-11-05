// The window around the documents: title bar, tab bar with the view switches, status bar.
use super::*;

impl Workspace {
    pub(super) fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let project = self
            .root
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().to_string());
        let can_export = self.doc().is_some_and(|d| d.read(cx).tex);
        TitleBar::new()
            .h(px(TITLE_BAR_H))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .h_full()
                    .items_center()
                    .justify_between()
                    .pr_2()
                    .child(
                        // the bar's empty part moves the window: AppKit's drag starts from the
                        // press itself (the title bar's own start-on-move did not drag here),
                        // and a double-click zooms as in any Mac title bar
                        div()
                            .id("title-drag")
                            .flex_1()
                            .h_full()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .on_mouse_down(MouseButton::Left, |ev, window, cx| {
                                cx.stop_propagation();
                                if ev.click_count == 2 {
                                    window.titlebar_double_click();
                                } else {
                                    window.start_window_move();
                                }
                            })
                            .child(Icon::default().path("icons/folder-open.svg").small())
                            .child(project),
                    )
                    .child(
                        Button::new("export")
                            .icon(Icon::default().path("icons/file-output.svg"))
                            .label("Export PDF")
                            .small()
                            .primary()
                            .disabled(!can_export)
                            .tooltip("Typeset the whole document to a PDF (⌘E)")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.with_doc(cx, |d, cx| d.export(cx))
                            })),
                    ),
            )
            .bg(theme.title_bar)
            .border_color(theme.title_bar_border)
    }

    pub(super) fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        // Zed's tabs: an unsaved file shows a dot before its name; the close button stays at the end
        let accent = cx.theme().blue;
        let tabs: Vec<Tab> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(ix, d)| {
                let d = d.read(cx);
                let close = Button::new(("close", ix))
                    .icon(Icon::default().path("icons/x.svg"))
                    .ghost()
                    .xsmall()
                    .tooltip("Close Tab (⌘W)")
                    .on_click(cx.listener(move |this, _, window, cx| this.close(ix, window, cx)));
                let lead = if d.dirty {
                    div()
                        .size(px(6.0))
                        .rounded_full()
                        .bg(accent)
                        .into_any_element()
                } else {
                    Icon::default()
                        .path(if d.tex {
                            "icons/file-text.svg"
                        } else {
                            "icons/file.svg"
                        })
                        .xsmall()
                        .into_any_element()
                };
                Tab::new()
                    .label(d.name().to_string())
                    .small()
                    .prefix(div().pl_2().flex().items_center().w(px(20.0)).child(lead))
                    .suffix(close)
            })
            .collect();
        let viewing = self.doc().filter(|d| d.read(cx).view.is_some()).map(|d| {
            let zoom = d.read(cx).zoom;
            self.render_zoom(zoom, cx).into_any_element()
        });
        let tools =
            self.doc().filter(|d| d.read(cx).tex).map(|d| {
                let d = d.read(cx);
                let (zoom, source) = (d.zoom, d.source_mode);
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .children((!source).then(|| self.render_zoom(zoom, cx)))
                    .child(
                        Button::new("recompile")
                            .icon(Icon::default().path("icons/refresh-cw.svg"))
                            .xsmall()
                            .ghost()
                            .tooltip("Run the full LaTeX compile now (⌘R)")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.with_doc(cx, |d, cx| d.recompile(cx))
                            })),
                    )
                    .child(self.render_mode_switch(source, cx))
            });
        TabBar::new("tabs")
            .small()
            .selected_index(self.active)
            .on_click(move |ix, _, cx| {
                let _ = weak.update(cx, |this, cx| this.activate(*ix, cx));
            })
            .children(tabs)
            .when_some(tools, |bar, tools| bar.suffix(tools))
            .when_some(viewing, |bar, zoom| bar.suffix(zoom))
    }

    /// Rendered (the typeset pages, edited in place) or Source (the file in a code editor).
    pub(super) fn render_mode_switch(
        &self,
        source: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let option = |id: &'static str, icon: &'static str, on: bool, to: bool| {
            Button::new(id)
                .icon(Icon::default().path(icon))
                .xsmall()
                .ghost()
                .selected(on)
                .tooltip(if to {
                    "Source: edit the LaTeX file (⌘⇧E)"
                } else {
                    "Rendered: edit on the typeset pages (⌘⇧E)"
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.with_doc(cx, |d, cx| d.set_source_mode(to, window, cx))
                }))
        };
        div()
            .flex()
            .items_center()
            .p_0p5()
            .mr_2()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .child(option("mode-rendered", "icons/eye.svg", !source, false))
            .child(option("mode-source", "icons/code.svg", source, true))
    }

    pub(super) fn render_zoom(&self, zoom: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        div()
            .flex()
            .items_center()
            .gap_0p5()
            .px_2()
            .child(
                Button::new("zoom-out")
                    .icon(Icon::default().path("icons/zoom-out.svg"))
                    .ghost()
                    .xsmall()
                    .tooltip("Zoom Out (⌘-)")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.with_doc(cx, |d, cx| d.zoom_step(-1, window, cx))
                    })),
            )
            .child(
                Button::new("zoom")
                    .label(format!("{}%", (zoom * 100.0).round()))
                    .ghost()
                    .xsmall()
                    .dropdown_caret(true)
                    .dropdown_menu(move |mut menu, _, _| {
                        for z in ZOOMS {
                            let weak = weak.clone();
                            menu = menu.item(
                                PopupMenuItem::new(format!("{}%", (z * 100.0).round()))
                                    .checked((z - zoom).abs() < 0.001)
                                    .on_click(move |_, window, cx| {
                                        let _ = weak.update(cx, |this, cx| {
                                            this.with_doc(cx, |d, cx| {
                                                d.set_zoom(z, None, window, cx)
                                            })
                                        });
                                    }),
                            );
                        }
                        menu
                    }),
            )
            .child(
                Button::new("zoom-in")
                    .icon(Icon::default().path("icons/zoom-in.svg"))
                    .ghost()
                    .xsmall()
                    .tooltip("Zoom In (⌘=)")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.with_doc(cx, |d, cx| d.zoom_step(1, window, cx))
                    })),
            )
    }

    pub(super) fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let (status, pages) = match self.doc() {
            Some(d) => {
                let d = d.read(cx);
                let status = if !self.status.is_empty() {
                    self.status.clone()
                } else if d.status.is_empty() && d.compiling {
                    "Compiling… (this part is not live: the page updates after the full LaTeX run)"
                        .to_string()
                } else {
                    d.status.clone()
                };
                let (page, total) = d.page_position();
                (
                    status,
                    (d.tex && total > 0).then(|| format!("Page {page} of {total}")),
                )
            }
            None => (self.status.clone(), None),
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .h(px(26.0))
            .p_1()
            .gap_2()
            .text_xs()
            .bg(theme.status_bar)
            .border_t_1()
            .border_color(theme.status_bar_border)
            .text_color(theme.muted_foreground)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(
                        Button::new("toggle-sidebar")
                            .icon(Icon::default().path("icons/panel-left.svg"))
                            .ghost()
                            .xsmall()
                            .selected(self.sidebar)
                            .tooltip("Toggle Project Panel (⌘B)")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sidebar = !this.sidebar;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("settings")
                            .icon(Icon::default().path("icons/settings.svg"))
                            .ghost()
                            .xsmall()
                            .tooltip("Settings (⌘,)")
                            .on_click(|_, _, cx| open_settings(cx)),
                    )
                    .child(div().truncate().child(if status.is_empty() {
                        "Ready".to_string()
                    } else {
                        status
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .pr_2()
                    .children(pages)
                    .child("LuaLaTeX · rtex"),
            )
    }
}
