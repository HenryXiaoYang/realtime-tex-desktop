// The welcome window, shown when no project is open: the projects opened before (searchable,
// ↑ ↓ and Return to open one) and the way to start a new one or open a folder. ↓ takes the keys
// from the search to the list, where VoiceOver hears the project at hand; typing goes back.
use crate::ui::Tip;
use crate::ui::projects::{self, Projects, Recent, RtexDownload};
use crate::ui::tex_setup::{TexInstall, open_tex_setup};
use crate::ui::workspace::{CloseTab, open_settings};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Sizable, TitleBar};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What the app is, in a line.
const BLURB: &str = "Write LaTeX right on the typeset pages, and see every change as you type.";

pub struct Welcome {
    search: Entity<InputState>,
    /// The list, which holds the keys while ↑ ↓ move through it.
    list: FocusHandle,
    /// The project ↑ ↓ moved to, among those the search shows.
    selected: usize,
    /// The list's scroll, which follows the selection.
    scroll: ScrollHandle,
    /// The projects whose folders are gone, as of the window's coming to the front (a folder
    /// on a disk that hangs costs one look, not one a frame).
    missing: HashSet<PathBuf>,
    /// rtex, lualatex not found (looked for again as the window comes to the front).
    no_rtex: bool,
    no_tex: bool,
    icon: Option<Arc<Image>>,
    _subs: Vec<Subscription>,
}

/// rtex and lualatex both found: the pages can be typeset.
pub fn setup_ok() -> bool {
    crate::engine::rtex_found() && crate::engine::find_lualatex().is_some()
}

/// Open the welcome window.
pub fn open_window(cx: &mut App) -> Option<AnyWindowHandle> {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(780.0), px(520.0)), cx)),
        window_min_size: Some(size(px(660.0), px(420.0))),
        ..TitleBar::window_options()
    };
    let opened = gpui_kit::open_window(options, cx, |window, cx| {
        // for the Window menu, Mission Control and VoiceOver (the drawn title bar shows its own)
        window.set_window_title("Welcome to Realtime Tex Desktop");
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
        let typed = cx.subscribe_in(&search, window, |this, _, ev: &InputEvent, _, cx| {
            match ev {
                InputEvent::Change => this.selected = 0,
                InputEvent::PressEnter { .. } => this.open_selected(cx),
                _ => {}
            }
            cx.notify();
        });
        // back in front: a disk may have come or gone
        let front = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.check_folders(cx);
                this.check_tools();
                cx.notify();
            }
        });
        // rtex downloaded, or not: looked for again
        let download = cx.observe_global::<RtexDownload>(|this, cx| {
            this.check_tools();
            cx.notify();
        });
        let installing = cx.observe_global::<TexInstall>(|this, cx| {
            this.check_tools();
            cx.notify();
        });
        let mut this = Self {
            search,
            list: cx.focus_handle(),
            selected: 0,
            scroll: ScrollHandle::new(),
            missing: HashSet::new(),
            no_rtex: false,
            no_tex: false,
            icon: crate::ui::app_icon::png()
                .map(|png| Arc::new(Image::from_bytes(ImageFormat::Png, png))),
            _subs: vec![typed, front, download, installing],
        };
        this.check_folders(cx);
        this.check_tools();
        this
    }

    fn check_tools(&mut self) {
        self.no_rtex = !crate::engine::rtex_found();
        self.no_tex = crate::engine::find_lualatex().is_none();
    }

    /// What is missing for the pages to typeset, and the way to get it.
    fn setup(&self, cx: &App) -> Option<impl IntoElement + use<>> {
        if !self.no_rtex && !self.no_tex {
            return None;
        }
        let theme = cx.theme();
        let line = |text: String, button: Button| {
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    Icon::default()
                        .path("icons/triangle-alert.svg")
                        .small()
                        .text_color(theme.warning),
                )
                .child(div().flex_1().min_w_0().text_sm().child(text))
                .child(button.small().outline().flex_shrink_0())
        };
        Some(
            div()
                .id("setup")
                .mx_5()
                .mt_3()
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .rounded_md()
                .border_1()
                .border_color(theme.warning)
                .role(Role::Alert)
                .when(self.no_rtex, |d| {
                    let get = |label: &'static str| {
                        Button::new("setup-rtex")
                            .label(label)
                            .on_click(|_, _, cx| projects::ensure_rtex(cx))
                    };
                    d.child(match cx.try_global::<RtexDownload>() {
                        Some(RtexDownload::Running) => line(
                            "Downloading rtex, the engine that typesets the pages…".into(),
                            Button::new("setup-rtex").label("Downloading…").disabled(true),
                        ),
                        Some(RtexDownload::Failed(why)) => line(
                            format!("Couldn’t download rtex ({why}). Check the connection and try again, or choose an rtex binary in Settings."),
                            get("Try Again"),
                        ),
                        _ => line(
                            "rtex, the engine that typesets the pages, isn’t here yet.".into(),
                            get("Download rtex"),
                        ),
                    })
                })
                .when(self.no_tex, |d| {
                    let text = match cx.try_global::<TexInstall>() {
                        Some(TexInstall::Running(p)) => format!(
                            "Installing TeX Live for LuaLaTeX: {:.0}% done.",
                            p.fraction() * 100.0
                        ),
                        _ => "LuaLaTeX isn’t installed, so no page can be typeset.".into(),
                    };
                    d.child(line(
                        text,
                        Button::new("setup-tex")
                            .label("Set Up LaTeX…")
                            .on_click(|_, _, cx| open_tex_setup(cx)),
                    ))
                }),
        )
    }

    /// Which projects' folders are gone.
    fn check_folders(&mut self, cx: &App) {
        self.missing = cx
            .global::<Projects>()
            .list
            .iter()
            .filter(|r| !r.root.is_dir())
            .map(|r| r.root.clone())
            .collect();
    }

    /// The projects the search finds, newest first: those it finds in their names, then those
    /// it finds in their places.
    fn shown(&self, cx: &App) -> Vec<Recent> {
        let q = self.search.read(cx).value().to_string();
        let mut found: Vec<(u8, Recent)> = cx
            .global::<Projects>()
            .list
            .iter()
            .filter_map(|r| matches(&r.root, &q).map(|rank| (rank, r.clone())))
            .collect();
        // stable: newest first within each
        found.sort_by_key(|(rank, _)| *rank);
        found.into_iter().map(|(_, r)| r).collect()
    }

    fn open_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(r) = self.shown(cx).get(self.selected) {
            let root = r.root.clone();
            cx.defer(move |cx| projects::open(&root, cx));
        }
    }

    /// Project `root`, whose folder is gone, clicked: say so, and offer to take it off the list.
    fn ask_to_forget(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let name = root
            .file_name()
            .map_or(root.to_string_lossy(), |n| n.to_string_lossy())
            .to_string();
        let detail = format!(
            "{} may be on a disk that is not connected, or it was moved or deleted.",
            tilde(&root)
        );
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("“{name}” is not there any more"),
            Some(&detail),
            &["Remove from List", "Cancel"],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(0) {
                let _ = this.update(cx, |_, cx| {
                    projects::forget(&root, cx);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// The keys: ↑ ↓ through the projects (↓ takes the keys from the search to the list, ↑ off
    /// the top gives them back), Return opens, ⌘⌫ takes the one at hand off the list, Escape
    /// clears the search or goes back to it; typing in the list goes to the search.
    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let shown = self.shown(cx);
        let n = shown.len();
        let in_list = self.list.is_focused(window);
        let ks = &ev.keystroke;
        let to_search = |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
            this.search.update(cx, |s, cx| s.focus(window, cx));
        };
        if ks.modifiers.platform && ks.key == "backspace" {
            if let Some(r) = shown.get(self.selected) {
                projects::forget(&r.root, cx);
                self.selected = self.selected.min(n.saturating_sub(2));
            }
        } else if ks.modifiers.platform || ks.modifiers.control {
            return;
        } else {
            match ks.key.as_str() {
                "down" if n > 0 => {
                    if in_list {
                        self.selected = (self.selected + 1).min(n - 1);
                    }
                    window.focus(&self.list, cx);
                }
                "up" if in_list && self.selected == 0 => to_search(self, window, cx),
                "up" => self.selected = self.selected.saturating_sub(1),
                "enter" if in_list => self.open_selected(cx),
                "escape" if in_list => to_search(self, window, cx),
                "escape" if !self.search.read(cx).value().is_empty() => {
                    self.search.update(cx, |s, cx| s.set_value("", window, cx));
                }
                // typing in the list goes on in the search: the key again, there
                _ if in_list
                    && (ks.key == "backspace"
                        || ks.key_char.as_ref().is_some_and(|c| !c.trim().is_empty())) =>
                {
                    to_search(self, window, cx);
                    let again = ks.clone();
                    window.defer(cx, move |window, cx| {
                        window.dispatch_keystroke(again, cx);
                    });
                }
                _ => return,
            }
        }
        // a small window shows a few: the selection stays in view
        self.scroll.scroll_to_item(self.selected);
        cx.stop_propagation();
        cx.notify();
    }

    /// The search, over the list of projects.
    fn header(&self) -> impl IntoElement {
        div().px_5().pt_4().pb_3().child(
            Input::new(&self.search)
                .prefix(Icon::new(IconName::Search).small())
                .cleanable(true),
        )
    }

    fn row(&self, ix: usize, r: &Recent, in_list: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let name = r
            .root
            .file_name()
            .map_or(r.root.to_string_lossy(), |n| n.to_string_lossy())
            .to_string();
        let there = !self.missing.contains(&r.root);
        let selected = ix == self.selected;
        let root = r.root.clone();
        let forget = r.root.clone();
        let place = tilde(&r.root);
        let spoken = if there {
            format!("{name}, {place}")
        } else {
            format!("{name}, not found, {place}")
        };
        // the project, an option of the list; VoiceOver hears the keys for it (the list shows
        // VoiceOver its options alone, not the remove button beside each)
        let option = div()
            .id(("project", ix))
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .gap_3()
            .cursor_pointer()
            .role(Role::ListBoxOption)
            .aria_label(spoken)
            .aria_description("Return opens it; Command-Delete removes it from the list")
            .aria_selected(selected)
            .when(selected, |d| d.aria_active_descendant())
            .on_click(cx.listener(move |this, _, window, cx| {
                this.selected = ix;
                let root = root.clone();
                if root.is_dir() {
                    cx.defer(move |cx| projects::open(&root, cx));
                } else {
                    this.missing.insert(root.clone());
                    this.ask_to_forget(root, window, cx);
                }
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
            );
        div()
            .group("project-row")
            .flex()
            .items_center()
            .gap_1()
            .h(px(52.0))
            .px_2()
            .rounded_md()
            // the keys' place: a ring round the project at hand while the list has them
            .border_1()
            .border_color(if selected && in_list {
                theme.ring
            } else {
                transparent_black()
            })
            .when(selected, |d| d.bg(theme.list_active))
            .when(!selected, |d| d.hover(|s| s.bg(theme.list_hover)))
            .child(option)
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

    /// The left pane: the icon, the name, what the app is, and the ways in.
    fn brand(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .w(px(280.0))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_4()
            .px_6()
            .pb_6()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .children(self.icon.clone().map(|icon| {
                img(icon)
                    .size(px(128.0))
                    .flex_shrink_0()
                    .role(Role::Image)
                    .aria_label("Realtime Tex Desktop icon")
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_center()
                            .child("Realtime Tex Desktop"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(concat!("Version ", env!("CARGO_PKG_VERSION"))),
                    )
                    .child(
                        div()
                            .pt_1()
                            .text_sm()
                            .text_center()
                            .text_color(theme.muted_foreground)
                            .child(BLURB),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .pt_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(new_button("welcome-new").primary().w_full())
                    .child(open_button("welcome-open").outline().w_full())
                    .child(tour_button("welcome-tour").outline().w_full()),
            )
    }

    /// No project yet: the list's place says where they will show up.
    fn no_projects(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .px_8()
            .pb_6()
            .text_center()
            .child(
                Icon::default()
                    .path("icons/folder-open.svg")
                    .size_8()
                    .text_color(theme.muted_foreground),
            )
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child("No recent projects"),
            )
            .child(
                div()
                    .max_w(px(300.0))
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("Start a project, or open a folder with your .tex files. Projects you open show up here."),
            )
    }
}

fn new_button(id: &'static str) -> Button {
    Button::new(id)
        .icon(Icon::default().path("icons/folder-plus.svg"))
        .label("New Project")
        .tooltip(crate::ui::keys_in("New Project (⌘⇧N)"))
        .on_click(|_, _, cx| projects::new_project(cx))
}

fn open_button(id: &'static str) -> Button {
    Button::new(id)
        .icon(Icon::default().path("icons/folder-open.svg"))
        .label("Open")
        .tooltip(crate::ui::keys_in("Open a folder or a .tex file (⌘O)"))
        .on_click(|_, _, cx| projects::choose_and_open(cx))
}

fn tour_button(id: &'static str) -> Button {
    Button::new(id)
        .icon(Icon::default().path("icons/compass.svg"))
        .label("Tour")
        .tooltip("How the app works, step by step, in a scratch project")
        .on_click(|_, _, cx| projects::start_tour(cx))
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

/// Whether search `q` finds project `root`: 0 in its name, 1 in its place (home as ~, so the
/// home folder's own name finds nothing), None not at all; an empty search finds all.
fn matches(root: &Path, q: &str) -> Option<u8> {
    let q = q.trim().to_lowercase();
    let name = root.file_name().map(|n| n.to_string_lossy().to_lowercase());
    if q.is_empty() || name.is_some_and(|n| n.contains(&q)) {
        Some(0)
    } else if tilde(root).to_lowercase().contains(&q) {
        Some(1)
    } else {
        None
    }
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
    match std::env::home_dir().and_then(|h| p.strip_prefix(h).ok().map(Path::to_path_buf)) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        None => p.to_string_lossy().to_string(),
    }
}

impl Render for Welcome {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shown = self.shown(cx);
        self.selected = self.selected.min(shown.len().saturating_sub(1));
        let none_yet = cx.global::<Projects>().list.is_empty();
        let theme = cx.theme().clone();
        let body: AnyElement = if none_yet {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(self.header())
                .child(self.no_projects(cx))
                .into_any_element()
        } else {
            let in_list = self.list.is_focused(window);
            let rows: Vec<AnyElement> = shown
                .iter()
                .enumerate()
                .map(|(i, r)| self.row(i, r, in_list, cx))
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
                        .track_focus(&self.list)
                        .px_3()
                        .pb_3()
                        .role(Role::ListBox)
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
            // ⌘W closes this window as it closes any
            .on_action(cx.listener(|_, _: &CloseTab, window, cx| {
                projects::welcome_closed(cx);
                window.remove_window();
            }))
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
                            .gap_2()
                            .on_mouse_down(MouseButton::Left, |ev, window, cx| {
                                cx.stop_propagation();
                                if ev.click_count == 2 {
                                    window.titlebar_double_click();
                                } else {
                                    window.start_window_move();
                                }
                            }),
                    )
                    .child(
                        div().pr_2().child(
                            Button::new("welcome-settings")
                                .ghost()
                                .small()
                                .icon(Icon::default().path("icons/settings.svg"))
                                .tip("Settings (⌘,)")
                                .on_click(|_, _, cx| open_settings(cx)),
                        ),
                    )
                    .bg(theme.title_bar)
                    .border_color(theme.title_bar_border),
            )
            .children(self.setup(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.brand(cx))
                    .child(body),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{initials, matches, tilde};
    use std::path::Path;

    #[test]
    fn a_project_is_named_by_its_initials() {
        assert_eq!(initials("realtime-tex-desktop"), "RT");
        assert_eq!(initials("thesis"), "T");
        assert_eq!(initials("CS HL B1"), "CH");
        assert_eq!(initials("__"), "?");
    }

    #[test]
    fn the_search_finds_names_first_and_not_the_home_folder() {
        let home = std::env::var("HOME").unwrap();
        let user = Path::new(&home)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let p = Path::new(&home).join("School/Lab Report");
        assert_eq!(matches(&p, ""), Some(0));
        assert_eq!(matches(&p, "lab"), Some(0));
        assert_eq!(matches(&p, " REPORT "), Some(0));
        assert_eq!(matches(&p, "school"), Some(1));
        assert_eq!(matches(&p, "essay"), None);
        // every project is under home: its name finds none of them
        assert_eq!(matches(&p, &format!("/{user}/")), None);
    }

    #[test]
    fn home_is_a_tilde() {
        let home = std::env::var("HOME").unwrap();
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(
            tilde(&Path::new(&home).join("Documents").join("x")),
            format!("~{sep}Documents{sep}x")
        );
        assert_eq!(tilde(Path::new("/tmp/x")), "/tmp/x");
        assert_eq!(
            tilde(Path::new(&format!("{home}2/x"))),
            format!("{home}2/x")
        );
    }
}
