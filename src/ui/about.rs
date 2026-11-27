// The About window: the app's icon, name and version, what it is, and what it stands on (the rtex
// engine, which version of it, and where to read more). Opened from the app menu and Help.
use crate::ui::workspace::CloseTab;
use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme, TitleBar};
use gpui_kit::*;
use std::sync::Arc;

actions!(realtime_tex, [ShowAbout]);

/// What the app is, in a line.
const BLURB: &str = "Write LaTeX right on the typeset pages, and see every change as you type.";
const PROJECT_URL: &str = "https://github.com/HenryXiaoYang/realtime-tex-desktop";
const ENGINE_URL: &str = "https://github.com/HenryXiaoYang/realtime-tex";

/// The window, while it is open.
struct AboutWindow(Option<AnyWindowHandle>);

impl Global for AboutWindow {}

/// The About window in front (opened if it is not).
pub fn open_about(cx: &mut App) {
    if let Some(w) = cx.try_global::<AboutWindow>().and_then(|w| w.0)
        && w.update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(420.0), px(520.0)), cx)),
        is_resizable: false,
        ..TitleBar::window_options()
    };
    let opened = gpui_kit::open_window(options, cx, |window, cx| {
        window.set_window_title("About Realtime Tex Desktop");
        window.on_window_should_close(cx, |_, cx| {
            cx.set_global(AboutWindow(None));
            true
        });
        cx.new(|_| About::new())
    });
    if let Ok((h, _)) = opened {
        cx.set_global(AboutWindow(Some(h)));
    }
}

/// The engine's version as the app pins it: `0.0.8`.
pub fn engine_version() -> String {
    crate::engine::rtex_tag()
        .trim_start_matches('v')
        .to_string()
}

struct About {
    icon: Option<Arc<Image>>,
}

impl About {
    fn new() -> Self {
        Self {
            icon: crate::ui::app_icon::png()
                .map(|png| Arc::new(Image::from_bytes(ImageFormat::Png, png))),
        }
    }
}

impl Render for About {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let fact = |label: &'static str, value: String| {
            div()
                .flex()
                .justify_between()
                .gap_4()
                .text_sm()
                .child(div().text_color(theme.muted_foreground).child(label))
                .child(value)
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            // ⌘W closes this window as it closes any
            .on_action(cx.listener(|_, _: &CloseTab, window, cx| {
                cx.set_global(AboutWindow(None));
                window.remove_window();
            }))
            .child(
                TitleBar::new()
                    .child(
                        // moves the window, like the main window's title bar
                        div().id("about-drag").flex_1().h_full().on_mouse_down(
                            MouseButton::Left,
                            |ev, window, cx| {
                                cx.stop_propagation();
                                if ev.click_count == 2 {
                                    window.titlebar_double_click();
                                } else {
                                    window.start_window_move();
                                }
                            },
                        ),
                    )
                    .bg(theme.title_bar)
                    .border_color(theme.title_bar_border),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_4()
                    .px_8()
                    .pb_6()
                    .children(self.icon.clone().map(|icon| {
                        img(icon)
                            .size(px(112.0))
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
                            .p_3()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .rounded_lg()
                            .border_1()
                            .border_color(theme.border)
                            .child(fact("Typesetting by", format!("rtex {}", engine_version())))
                            .child(fact("Engine", "LuaLaTeX".into()))
                            .child(fact("Interface", "GPUI".into())),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("about-project")
                                    .outline()
                                    .label("Project on GitHub")
                                    .flex_1()
                                    .on_click(|_, _, cx| cx.open_url(PROJECT_URL)),
                            )
                            .child(
                                Button::new("about-engine")
                                    .outline()
                                    .label("About rtex")
                                    .flex_1()
                                    .on_click(|_, _, cx| cx.open_url(ENGINE_URL)),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_engine_version_is_the_pinned_tag_without_its_v() {
        let v = super::engine_version();
        assert!(!v.starts_with('v') && v.split('.').count() == 3, "{v}");
        assert!(v.split('.').all(|n| n.parse::<u32>().is_ok()), "{v}");
    }
}
