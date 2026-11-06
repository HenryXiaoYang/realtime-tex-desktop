// The window around the documents: title bar, tab bar with the view switches, status bar.
use super::*;
use crate::ui::Tip;

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
                            .tooltip(crate::ui::keys_in("Typeset the whole document to a PDF (⌘⌥E)"))
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
                    .tip("Close Tab (⌘W)")
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
        let tools = self.doc().filter(|d| d.read(cx).tex).map(|d| {
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
                        .tip("Recompile: run the full LaTeX compile now (⌘⇧R)")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.with_doc(cx, |d, cx| d.recompile(window, cx))
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

    /// Visual (the typeset pages, edited in place) or Source (the file in a code editor).
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
                .tip(if to {
                    "Source: edit the LaTeX file (⌘⇧E)"
                } else {
                    "Visual: edit on the typeset pages (⌘⇧E)"
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
            .child(option("mode-visual", "icons/eye.svg", !source, false))
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
                    .tip("Zoom Out (⌘-)")
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
                        // the page as wide as the column, whatever the window
                        let fit = weak.clone();
                        menu = menu
                            .item(
                                PopupMenuItem::new("Fit Width").on_click(move |_, window, cx| {
                                    let _ = fit.update(cx, |this, cx| {
                                        this.with_doc(cx, |d, cx| d.fit_width(window, cx))
                                    });
                                }),
                            )
                            .separator();
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
                    .tip("Zoom In (⌘=)")
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
        let status = crate::ui::keys_in(&status);
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .h(px(30.0))
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
                            .selected(self.fit.left)
                            .tip("Show or Hide Project Panel (⌘\\)")
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_left(cx))),
                    )
                    .children(pages.map(|p| div().flex_none().child(p)))
                    .child(self.problems_button(cx))
                    .children(self.text_elsewhere(cx))
                    // a long message (a LaTeX error) cut short, whole in its tooltip
                    .child({
                        let text = if status.is_empty() {
                            "Ready".to_string()
                        } else {
                            status
                        };
                        let tip = text.clone();
                        div()
                            .id("status")
                            .truncate()
                            .tooltip(move |window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(tip.clone())
                                    .build(window, cx)
                            })
                            .child(text)
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_none()
                    .gap_2()
                    .pr_1()
                    .child(
                        Button::new("toggle-logs")
                            .icon(Icon::default().path("icons/logs.svg"))
                            .ghost()
                            .xsmall()
                            .selected(self.bottom == Some(Bottom::Logs))
                            .tip("Show or Hide the rtex Log")
                            .on_click(
                                cx.listener(|this, _, _, cx| this.toggle_bottom(Bottom::Logs, cx)),
                            ),
                    )
                    .child(
                        Button::new("settings")
                            .icon(Icon::default().path("icons/settings.svg"))
                            .ghost()
                            .xsmall()
                            .tip("Settings (⌘,)")
                            .on_click(|_, _, cx| open_settings(cx)),
                    )
                    .child(
                        Button::new("toggle-inspector")
                            .icon(Icon::default().path("icons/panel-right.svg"))
                            .ghost()
                            .xsmall()
                            .selected(self.fit.right)
                            .tip("Show or Hide Sidebar (⌥⌘\\)")
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_right(cx))),
                    ),
            )
    }

    /// For a main file that only \input's others: they are named, each a link that opens it,
    /// since its own pages have nothing to edit.
    fn text_elsewhere(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let files = self.doc()?.read(cx).text_elsewhere();
        (!files.is_empty()).then(|| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap_1()
                .child("To edit, open")
                .children(files.into_iter().enumerate().map(|(i, f)| {
                    Button::new(("text-elsewhere", i))
                        .label(f.clone())
                        .link()
                        .xsmall()
                        .on_click(
                            cx.listener(move |this, _, window, cx| {
                                this.open(f.clone(), window, cx)
                            }),
                        )
                }))
        })
    }

    /// The problems or the log under the pages, or neither: a second click closes it.
    fn toggle_bottom(&mut self, b: Bottom, cx: &mut Context<Self>) {
        self.bottom = (self.bottom != Some(b)).then_some(b);
        cx.notify();
    }

    /// The count of the open file's errors and warnings, in red while there are errors; a click
    /// lists them under the pages.
    fn problems_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let problems = self
            .doc()
            .map(|d| d.read(cx).problems.clone())
            .unwrap_or_default();
        let errors = problems.iter().filter(|p| p.severity == "error").count();
        let warnings = problems.len() - errors;
        let plural = |n: usize, w: &str| format!("{n} {w}{}", if n == 1 { "" } else { "s" });
        let (icon, color) = if errors > 0 {
            ("icons/circle-x.svg", theme.danger)
        } else if warnings > 0 {
            ("icons/triangle-alert.svg", theme.warning)
        } else {
            ("icons/circle-check.svg", theme.muted_foreground)
        };
        Button::new("toggle-problems")
            .icon(Icon::default().path(icon).text_color(color))
            .label(problems.len().to_string())
            .ghost()
            .xsmall()
            .selected(self.bottom == Some(Bottom::Problems))
            .tip(format!(
                "Problems: {}, {}",
                plural(errors, "error"),
                plural(warnings, "warning")
            ))
            .on_click(cx.listener(|this, _, _, cx| this.toggle_bottom(Bottom::Problems, cx)))
    }

    /// The open file's errors and warnings from its last full run; a click on one in this file
    /// brings its line into view.
    pub(super) fn render_problems(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let (problems, offsets) = match self.doc() {
            Some(d) => {
                let d = d.read(cx);
                let offs: Vec<_> = d.problems.iter().map(|p| d.problem_offset(p)).collect();
                (d.problems.clone(), offs)
            }
            None => (vec![], vec![]),
        };
        let rows = problems
            .into_iter()
            .zip(offsets)
            .enumerate()
            .map(|(i, (p, off))| {
                let (icon, color) = if p.severity == "error" {
                    ("icons/circle-x.svg", theme.danger)
                } else {
                    ("icons/triangle-alert.svg", theme.warning)
                };
                let place = match (&p.file, p.line) {
                    (Some(f), Some(l)) => format!("{}:{l}", f.trim_start_matches("./")),
                    (None, Some(l)) => format!("line {l}"),
                    (Some(f), None) => f.trim_start_matches("./").to_string(),
                    (None, None) => String::new(),
                };
                div()
                    .id(("problem", i))
                    .flex()
                    .items_start()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .when(off.is_some(), |d| {
                        d.cursor_pointer()
                            .hover(|s| s.bg(theme.list_hover))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(off) = off {
                                    this.with_doc(cx, |d, cx| d.jump_to(off, cx));
                                }
                            }))
                    })
                    .child(
                        Icon::default()
                            .path(icon)
                            .text_color(color)
                            .xsmall()
                            .mt(px(2.0)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(div().text_color(theme.foreground).child(p.message))
                                    .child(div().text_color(theme.muted_foreground).child(place)),
                            )
                            .children(p.context.filter(|c| !c.is_empty()).map(|c| {
                                div()
                                    .font_family(crate::ui::editor::MONO)
                                    .text_color(theme.muted_foreground)
                                    .truncate()
                                    .child(c)
                            })),
                    )
            });
        let rows: Vec<_> = rows.collect();
        div()
            .id("problems")
            .size_full()
            .overflow_y_scroll()
            .p_1()
            .bg(crate::ui::palette::panel(&theme))
            .border_t_1()
            .border_color(theme.border)
            .text_xs()
            .when(rows.is_empty(), |d| {
                d.p_2()
                    .text_color(theme.muted_foreground)
                    .child("No problems in the last LaTeX run")
            })
            .children(rows)
    }

    /// The open file's rtex log (its events and stderr), kept scrolled to its end as lines come in.
    pub(super) fn render_logs(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let log = self
            .doc()
            .map(|d| d.read(cx).log.clone())
            .unwrap_or_default();
        if log.len() != self.log_seen {
            self.log_seen = log.len();
            self.log_scroll.scroll_to_bottom();
        }
        div()
            .id("rtex-log")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.log_scroll)
            .p_2()
            .bg(crate::ui::palette::panel(&theme))
            .border_t_1()
            .border_color(theme.border)
            .font_family(crate::ui::editor::MONO)
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(if log.is_empty() {
                "rtex has written nothing yet".to_string()
            } else {
                log
            })
    }
}

/// What the panel under the pages shows.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Bottom {
    Problems,
    Logs,
}
