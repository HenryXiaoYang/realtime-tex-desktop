// The welcome window, shown when no project is open: the projects opened before (searchable,
// ↑ ↓ and Return to open one) and the way to start a new one or open a folder.
use crate::ui::Tip;
use crate::ui::projects::{self, Projects, Recent};
use crate::ui::workspace::open_settings;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Icon, IconName, Sizable, TitleBar};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::path::Path;

pub struct Welcome {
    search: Entity<InputState>,
    /// The project ↑ ↓ moved to, among those the search shows.
    selected: usize,
    /// The list's scroll, which follows the selection.
    scroll: ScrollHandle,
    _sub: Subscription,
}

/// Open the welcome window.
pub fn open_window(cx: &mut App) -> Option<AnyWindowHandle> {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(820.0), px(540.0)), cx)),
        window_min_size: Some(size(px(660.0), px(440.0))),
        ..TitleBar::window_options()
    };
    let opened = gpui_kit::open_window(options, cx, |window, cx| {
        window.on_window_should_close(cx, |_, cx| {
            projects::welcome_closed(cx);
            true
        });
        cx.new(|cx| Welcome::new(window, cx))
    });
    opened.ok().map(|(h, _)| h)
}

/// Tell what went wrong, over the window in front.
pub fn report(message: String, cx: &mut App) {
    if let Some(w) = cx.active_window().or_else(|| cx.windows().first().copied()) {
        let _ = w.update(cx, |_, window, cx| {
            // the answer is only OK
            drop(window.prompt(PromptLevel::Warning, &message, None, &["OK"], cx));
        });
    }
}

impl Welcome {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search projects"));
        search.update(cx, |s, cx| s.focus(window, cx));
        let _sub = cx.subscribe_in(&search, window, |this, _, ev: &InputEvent, _, cx| {
            match ev {
                InputEvent::Change => this.selected = 0,
                InputEvent::PressEnter { .. } => this.open_selected(cx),
                _ => {}
            }
            cx.notify();
        });
        Self {
            search,
            selected: 0,
            scroll: ScrollHandle::new(),
            _sub,
        }
    }

    /// The projects the search finds, newest first.
    fn shown(&self, cx: &App) -> Vec<Recent> {
        let q = self.search.read(cx).value().trim().to_lowercase();
        cx.global::<Projects>()
            .list
            .iter()
            .filter(|r| q.is_empty() || r.root.to_string_lossy().to_lowercase().contains(&q))
            .cloned()
            .collect()
    }

    fn open_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(r) = self.shown(cx).get(self.selected) {
            let root = r.root.clone();
            cx.defer(move |cx| projects::open(&root, cx));
        }
    }

    /// ↑ ↓ through the projects, ⌘⌫ takes the one at hand off the list, Escape clears the
    /// search; typing goes to the search.
    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let shown = self.shown(cx);
        let n = shown.len();
        if ev.keystroke.modifiers.platform && ev.keystroke.key == "backspace" {
            if let Some(r) = shown.get(self.selected) {
                projects::forget(&r.root, cx);
                self.selected = self.selected.min(n.saturating_sub(2));
            }
            cx.stop_propagation();
            return cx.notify();
        }
        match ev.keystroke.key.as_str() {
            "down" if n > 0 => self.selected = (self.selected + 1).min(n - 1),
            "up" => self.selected = self.selected.saturating_sub(1),
            "escape" if !self.search.read(cx).value().is_empty() => {
                self.search.update(cx, |s, cx| s.set_value("", window, cx));
            }
            _ => return,
        }
        // a small window shows a few: the selection stays in view
        self.scroll.scroll_to_item(self.selected);
        cx.stop_propagation();
        cx.notify();
    }

    fn sidebar(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .w(px(220.0))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_5()
            .p_4()
            .bg(crate::ui::palette::panel(theme))
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Realtime Tex Desktop"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(concat!("Version ", env!("CARGO_PKG_VERSION"))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h_8()
                    .px_2()
                    .rounded_md()
                    .bg(theme.list_active)
                    .text_sm()
                    .child(Icon::default().path("icons/folder-open.svg").small())
                    .child("Projects"),
            )
            .child(div().flex_1())
            .child(
                Button::new("welcome-settings")
                    .ghost()
                    .small()
                    .icon(Icon::default().path("icons/settings.svg"))
                    .label("Settings")
                    .self_start()
                    .on_click(|_, _, cx| open_settings(cx)),
            )
    }

    fn header(&self) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_5()
            .pt_4()
            .pb_3()
            // the search gives up room; the buttons keep their names
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&self.search)
                        .prefix(Icon::new(IconName::Search).small())
                        .cleanable(true),
                ),
            )
            .child(new_button("welcome-new").primary().flex_shrink_0())
            .child(open_button("welcome-open").outline().flex_shrink_0())
    }

    fn row(&self, ix: usize, r: &Recent, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let name = r
            .root
            .file_name()
            .map_or(r.root.to_string_lossy(), |n| n.to_string_lossy())
            .to_string();
        let there = r.root.is_dir();
        let selected = ix == self.selected;
        let root = r.root.clone();
        let forget = r.root.clone();
        let place = tilde(&r.root);
        div()
            .id(("project", ix))
            .group("project-row")
            .flex()
            .items_center()
            .gap_3()
            .h(px(52.0))
            .px_2()
            .rounded_md()
            .cursor_pointer()
            .when(selected, |d| d.bg(theme.list_active))
            .when(!selected, |d| d.hover(|s| s.bg(theme.list_hover)))
            .role(Role::Button)
            .aria_label(format!("{name}, {place}"))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected = ix;
                let root = root.clone();
                cx.defer(move |cx| projects::open(&root, cx));
            }))
            .child(avatar(&name, there))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .when(!there, |d| d.text_color(theme.muted_foreground))
                            .child(name),
                    )
                    .child(
                        div()
                            .text_xs()
                            .truncate()
                            .text_color(if there {
                                theme.muted_foreground
                            } else {
                                theme.danger
                            })
                            .child(if there {
                                place
                            } else {
                                format!("Not found: {place}")
                            }),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .when(!selected, |d| {
                        d.invisible().group_hover("project-row", |s| s.visible())
                    })
                    .child(
                        Button::new(("forget", ix))
                            .ghost()
                            .xsmall()
                            .icon(Icon::default().path("icons/x.svg"))
                            .tip("Remove from the list (⌘⌫)")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                projects::forget(&forget, cx);
                                this.selected = this.selected.saturating_sub(1);
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    /// No project yet: what the app is, and the two ways in.
    fn first_run(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_6()
            .px_10()
            .pb_10()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Welcome to Realtime Tex Desktop"),
                    )
                    .child(
                        div()
                            .max_w(px(380.0))
                            .text_sm()
                            .text_center()
                            .text_color(theme.muted_foreground)
                            .child("Write LaTeX on the typeset pages. Start a project, or open a folder with your .tex files."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_center()
                    .gap_3()
                    .child(new_button("first-new").primary().large())
                    .child(open_button("first-open").outline().large()),
            )
    }
}

fn new_button(id: &'static str) -> Button {
    Button::new(id)
        .icon(Icon::default().path("icons/folder-plus.svg"))
        .label("New Project")
        .tooltip("New Project (⌘⇧N)")
        .on_click(|_, _, cx| projects::new_project(cx))
}

fn open_button(id: &'static str) -> Button {
    Button::new(id)
        .icon(Icon::default().path("icons/folder-open.svg"))
        .label("Open")
        .tooltip("Open a folder or a .tex file (⌘O)")
        .on_click(|_, _, cx| projects::choose_and_open(cx))
}

/// A project's tile: its initials on a colour of its own (gray when its folder is gone).
fn avatar(name: &str, there: bool) -> impl IntoElement {
    let hue = name
        .bytes()
        .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32))
        % 360;
    // dark enough that white initials stand 4.5:1 on it
    let bg = hsla(hue as f32 / 360.0, if there { 0.5 } else { 0.0 }, 0.38, 1.0);
    div()
        .size(px(32.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(bg)
        .text_color(white())
        .text_xs()
        .font_weight(FontWeight::BOLD)
        .child(initials(name))
}

/// Up to two letters for `name`: the first of each of its first two words.
fn initials(name: &str) -> String {
    let words: Vec<&str> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let s: String = match words.as_slice() {
        [] => "?".into(),
        [one] => one.chars().take(1).collect(),
        [a, b, ..] => a.chars().take(1).chain(b.chars().take(1)).collect(),
    };
    s.to_uppercase()
}

/// `p`, the home folder as ~.
fn tilde(p: &Path) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let s = p.to_string_lossy().to_string();
    match s.strip_prefix(&home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => {
            format!("~{rest}")
        }
        _ => s,
    }
}

impl Render for Welcome {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shown = self.shown(cx);
        self.selected = self.selected.min(shown.len().saturating_sub(1));
        let none_yet = cx.global::<Projects>().list.is_empty();
        let theme = cx.theme().clone();
        let body: AnyElement = if none_yet {
            self.first_run(cx).into_any_element()
        } else {
            let rows: Vec<AnyElement> = shown
                .iter()
                .enumerate()
                .map(|(i, r)| self.row(i, r, cx))
                .collect();
            let q = self.search.read(cx).value().trim().to_string();
            // as wide as the window leaves it, whatever the paths' length: they shorten
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .flex()
                .flex_col()
                .child(self.header())
                .child(
                    div()
                        .id("projects")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll)
                        .px_3()
                        .pb_3()
                        .role(Role::List)
                        .aria_label("Recent projects")
                        .children(rows)
                        .when(shown.is_empty(), |d| {
                            d.child(
                                div()
                                    .pt_8()
                                    .text_center()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child(format!("No project matches “{q}”")),
                            )
                        }),
                )
                .into_any_element()
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .capture_key_down(cx.listener(Self::on_key))
            .child(
                TitleBar::new()
                    .child(
                        // moves the window, like the main window's title bar
                        div()
                            .id("title-drag")
                            .flex_1()
                            .h_full()
                            .flex()
                            .items_center()
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
                            .child("Welcome"),
                    )
                    .bg(theme.title_bar)
                    .border_color(theme.title_bar_border),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.sidebar(cx))
                    .child(body),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{initials, tilde};
    use std::path::Path;

    #[test]
    fn a_project_is_named_by_its_initials() {
        assert_eq!(initials("realtime-tex-desktop"), "RT");
        assert_eq!(initials("thesis"), "T");
        assert_eq!(initials("CS HL B1"), "CH");
        assert_eq!(initials("__"), "?");
    }

    #[test]
    fn home_is_a_tilde() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(
            tilde(&Path::new(&home).join("Documents/x")),
            "~/Documents/x"
        );
        assert_eq!(tilde(Path::new("/tmp/x")), "/tmp/x");
        assert_eq!(
            tilde(Path::new(&format!("{home}2/x"))),
            format!("{home}2/x")
        );
    }
}
