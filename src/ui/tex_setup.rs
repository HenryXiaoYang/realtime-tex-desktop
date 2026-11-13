// Setting up LaTeX: the pages are typeset by LuaLaTeX, which comes with a TeX distribution, and
// the way to one differs by system. The window offers TeX Live installed by the app (with how far
// it is, how fast and how long is left) or the system's own way; it may close while the install
// goes on, and comes back when it is done.
use crate::engine::{self, Stage, TexProgress};
use crate::ui::projects;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::progress::Progress;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, TitleBar};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// TeX Live being installed by the app, or how that went.
#[derive(Default, Clone, PartialEq)]
pub enum TexInstall {
    #[default]
    Idle,
    Running(TexProgress),
    Failed(String),
    Done,
}

impl Global for TexInstall {}

/// The window, while it is open.
struct SetupWindow(Option<AnyWindowHandle>);

impl Global for SetupWindow {}

/// The setup window in front (opened if it is not).
pub fn open_tex_setup(cx: &mut App) {
    if let Some(w) = cx.try_global::<SetupWindow>().and_then(|w| w.0)
        && w.update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(620.0), px(560.0)), cx)),
        window_min_size: Some(size(px(480.0), px(420.0))),
        ..TitleBar::window_options()
    };
    let opened = gpui_kit::open_window(options, cx, |window, cx| {
        window.set_window_title("Set Up LaTeX");
        window.on_window_should_close(cx, |_, cx| {
            cx.set_global(SetupWindow(None));
            true
        });
        cx.new(TexSetup::new)
    });
    if let Ok((h, _)) = opened {
        cx.set_global(SetupWindow(Some(h)));
    }
}

/// Install TeX Live on a thread of its own; when it is done, the documents' rtex restarted on it
/// and the window back in front to say so.
pub fn install(cx: &mut App) {
    if matches!(cx.try_global::<TexInstall>(), Some(TexInstall::Running(_))) {
        return;
    }
    cx.set_global(TexInstall::Running(TexProgress::default()));
    let (tx, mut rx) = futures::channel::mpsc::unbounded();
    std::thread::spawn(move || {
        let progress = tx.clone();
        let got = engine::install_tex(|p| {
            let _ = progress.unbounded_send(Ok(p));
        });
        let _ = tx.unbounded_send(Err(got));
    });
    cx.spawn(async move |cx| {
        use futures::StreamExt;
        while let Some(m) = rx.next().await {
            cx.update(|cx| match m {
                Ok(p) => {
                    // Cancel set it back to Idle: what the installer says after that is late
                    if matches!(cx.global::<TexInstall>(), TexInstall::Running(_)) {
                        cx.set_global(TexInstall::Running(p));
                    }
                }
                Err(got) => {
                    if !matches!(cx.global::<TexInstall>(), TexInstall::Running(_)) {
                        return;
                    }
                    cx.set_global(match got {
                        Ok(_) => TexInstall::Done,
                        Err(why) => TexInstall::Failed(why),
                    });
                    if cx.global::<TexInstall>() == &TexInstall::Done {
                        projects::tex_installed(cx);
                        open_tex_setup(cx);
                    }
                }
            });
        }
    })
    .detach();
}

/// The install stopped, and what it put in removed.
fn cancel(cx: &mut App) {
    cx.set_global(TexInstall::Idle);
    engine::cancel_tex_install();
}

/// A distribution the system installs itself, and where to get it.
const OWN_WAY: &[(&str, &str)] = if cfg!(target_os = "macos") {
    &[("Get MacTeX", "https://tug.org/mactex/")]
} else if cfg!(windows) {
    &[
        ("Get MiKTeX", "https://miktex.org/download"),
        ("Get TeX Live", "https://tug.org/texlive/windows.html"),
    ]
} else {
    &[("TeX Live’s Page", "https://tug.org/texlive/")]
};

const OWN_WAY_TEXT: &str = if cfg!(target_os = "macos") {
    "MacTeX is TeX Live packaged for the Mac (about 6 GB; it asks for your password). Download it, open the .pkg, then check again."
} else if cfg!(windows) {
    "MiKTeX fetches each package the first time a document needs it; TeX Live’s Windows installer puts them all in at once. Install either, then check again."
} else {
    "From your distribution’s packages, in a terminal, then check again:"
};

/// Linux: the command for each family of distributions.
const LINUX_COMMANDS: &[(&str, &str)] = &[
    ("Debian, Ubuntu", "sudo apt install texlive-full"),
    ("Fedora", "sudo dnf install texlive-scheme-full"),
    ("Arch", "sudo pacman -S texlive"),
];

pub struct TexSetup {
    /// LuaLaTeX found, as of the last look.
    found: bool,
    _subs: Vec<Subscription>,
}

impl TexSetup {
    fn new(cx: &mut Context<Self>) -> Self {
        let sub = cx.observe_global::<TexInstall>(|this, cx| {
            this.found = engine::find_lualatex().is_some();
            cx.notify();
        });
        Self {
            found: engine::find_lualatex().is_some(),
            _subs: vec![sub],
        }
    }

    fn progress(&self, p: &TexProgress, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let what = match p.stage {
            Stage::Installer => {
                "Finding a fast mirror and getting TeX Live’s installer…".to_string()
            }
            Stage::PackageList => "Getting the list of packages…".to_string(),
            Stage::Packages => format!("Installing {} of {} packages", p.done, p.total),
            Stage::Finishing => {
                "Finishing: building the fonts’ lists and the formats (a few minutes)…".into()
            }
        };
        let mut facts = vec![];
        if p.stage == Stage::Packages {
            facts.push(format!("{:.0}%", p.fraction() * 100.0));
            if let Some(s) = p.speed() {
                facts.push(format!("{}/s", bytes(s)));
            }
            facts.push(format!("{} so far", bytes(p.bytes)));
            facts.push(match p.left() {
                Some(s) => format!("about {} left", duration(s)),
                None => "working out the time left".into(),
            });
        }
        let waiting = matches!(
            p.stage,
            Stage::Installer | Stage::PackageList | Stage::Finishing
        );
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(what))
            .child(
                Progress::new("tex-progress")
                    .value(p.fraction() * 100.0)
                    .loading(waiting)
                    .accessibility_label("TeX Live installed"),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .gap_3()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(facts.join(" · "))
                    .child(div().truncate().child(p.package.clone())),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("You can close this window and go on working: the install goes on, and this window comes back when it’s done."),
            )
            .child(
                div().flex().justify_end().child(
                    Button::new("tex-cancel")
                        .label("Cancel")
                        .outline()
                        .small()
                        .on_click(|_, _, cx| cancel(cx)),
                ),
            )
    }

    fn own_way(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let mono = theme.mono_font_family.clone();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Or install it yourself"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(OWN_WAY_TEXT),
            )
            .when(cfg!(target_os = "linux"), |d| {
                d.children(LINUX_COMMANDS.iter().map(|(who, cmd)| {
                    div()
                        .flex()
                        .gap_3()
                        .text_sm()
                        .child(
                            div()
                                .w(px(110.0))
                                .text_color(theme.muted_foreground)
                                .child(*who),
                        )
                        .child(div().font_family(mono.clone()).child(*cmd))
                }))
            })
            .child(
                div()
                    .flex()
                    .gap_2()
                    .pt_1()
                    .children(OWN_WAY.iter().enumerate().map(|(i, (label, url))| {
                        Button::new(("tex-own", i))
                            .label(*label)
                            .outline()
                            .small()
                            .on_click(move |_, _, cx| cx.open_url(url))
                    }))
                    .child(
                        Button::new("tex-check")
                            .label("Check Again")
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.found = engine::find_lualatex().is_some();
                                if this.found {
                                    projects::tex_installed(cx);
                                }
                                cx.notify();
                            })),
                    ),
            )
    }
}

/// 5.2 MB, 830 KB.
fn bytes(b: u64) -> String {
    match b {
        b if b >= 1 << 30 => format!("{:.1} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b => format!("{} KB", b / 1024),
    }
}

/// 1 h 5 min, 12 min, 40 s.
fn duration(s: u64) -> String {
    match s {
        s if s >= 3600 => format!("{} h {} min", s / 3600, s % 3600 / 60),
        s if s >= 60 => format!("{} min", s.div_ceil(60)),
        s => format!("{s} s"),
    }
}

impl Render for TexSetup {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let state = cx.try_global::<TexInstall>().cloned().unwrap_or_default();
        let card = || {
            div()
                .p_4()
                .flex()
                .flex_col()
                .gap_2()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
        };
        let body: AnyElement = match &state {
            TexInstall::Running(p) => card().child(self.progress(p, cx)).into_any_element(),
            _ if self.found => card()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Icon::default()
                                .path("icons/circle-check.svg")
                                .text_color(theme.success),
                        )
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("LuaLaTeX is ready"),
                        ),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("The pages typeset with it. Open a project, or take the tour."),
                )
                .into_any_element(),
            _ => div()
                .flex()
                .flex_col()
                .gap_4()
                .child(
                    card()
                        .border_color(theme.primary)
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Install TeX Live for this app"),
                        )
                        .child(div().text_sm().text_color(theme.muted_foreground).child(
                            "The complete TeX Live without its documentation: every LaTeX package, about 4,500 of them, in about 5 GB. No administrator password, and nothing changed outside the app. It takes 20 minutes to an hour, depending on the connection.",
                        ))
                        .when_some(
                            match &state {
                                TexInstall::Failed(why) => Some(why.clone()),
                                _ => None,
                            },
                            |d, why| {
                                d.child(
                                    div()
                                        .text_sm()
                                        .text_color(theme.danger)
                                        .child(format!("Couldn’t install TeX Live: {why}.")),
                                )
                            },
                        )
                        .child(
                            div().flex().pt_1().child(
                                Button::new("tex-install")
                                    .label(if matches!(state, TexInstall::Failed(_)) {
                                        "Try Again"
                                    } else {
                                        "Install TeX Live"
                                    })
                                    .primary()
                                    .small()
                                    .on_click(|_, _, cx| install(cx)),
                            ),
                        ),
                )
                .child(card().child(self.own_way(cx)))
                .into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                TitleBar::new()
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Set Up LaTeX"),
                    )
                    .bg(theme.title_bar)
                    .border_color(theme.title_bar_border),
            )
            .child(
                div()
                    .id("tex-setup")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(div().text_sm().child(
                        "The pages are typeset by LuaLaTeX, which comes with a TeX distribution. This computer doesn’t have one yet.",
                    ))
                    .child(body),
            )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sizes_and_times_read_as_people_say_them() {
        assert_eq!(super::bytes(5 * 1024 * 1024 + 200 * 1024), "5.2 MB");
        assert_eq!(super::bytes(830 * 1024), "830 KB");
        assert_eq!(super::duration(3900), "1 h 5 min");
        assert_eq!(super::duration(700), "12 min");
        assert_eq!(super::duration(40), "40 s");
    }
}
