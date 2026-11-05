// App settings: kept as a GPUI global, saved as JSON in
// ~/Library/Application Support/Realtime Tex Desktop/settings.json, and edited in the Settings
// window (⌘,), built from gpui-kit's settings components.
use gpui_kit::component::setting::{
    NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings,
};
use gpui_kit::component::{ActiveTheme, Theme, ThemeMode, TitleBar};
use gpui_kit::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    /// "system", "light" or "dark".
    pub theme: String,
    pub ui_font_size: f64,
    /// Pages on dark paper in the dark theme.
    pub dark_pages: bool,
    /// The Source mode editor and the preamble strip, at 100% zoom.
    pub source_font_size: f64,
    /// The source box on the page: a font family, or empty for the document's own font.
    pub source_box_font: String,
    /// Zoom a file opens at, in percent.
    pub default_zoom: f64,
    /// A formula's typeset result under its source while it is edited.
    pub live_preview: bool,
    pub line_numbers: bool,
    pub soft_wrap: bool,
    /// Save a file a second after the last edit.
    pub autosave: bool,
    /// The rtex binary; empty finds it ($RTEX_BIN, PATH, the VS Code extension's copy).
    pub rtex_path: String,
    /// Folders with lualatex and kpsewhich, added to PATH.
    pub tex_bin: String,
    /// The ribbon folded to its tab row.
    pub ribbon_collapsed: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            ui_font_size: 16.0,
            dark_pages: true,
            source_font_size: 13.0,
            source_box_font: String::new(),
            default_zoom: 100.0,
            live_preview: true,
            line_numbers: true,
            soft_wrap: true,
            autosave: false,
            rtex_path: String::new(),
            tex_bin: "/Library/TeX/texbin".into(),
            ribbon_collapsed: false,
        }
    }
}

impl Global for AppSettings {}

fn file() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_default();
    PathBuf::from(home).join("Library/Application Support/Realtime Tex Desktop/settings.json")
}

impl AppSettings {
    /// Read the saved settings (defaults when there are none) and apply them.
    pub fn init(cx: &mut App) {
        let s = std::fs::read_to_string(file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        cx.set_global::<AppSettings>(s);
        Self::apply(cx);
    }

    pub fn get(cx: &App) -> &AppSettings {
        cx.global::<AppSettings>()
    }

    /// Change, save and apply; every window redraws with the new values.
    pub fn set(cx: &mut App, f: impl FnOnce(&mut AppSettings)) {
        f(cx.global_mut::<AppSettings>());
        let s = Self::get(cx);
        let path = file();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, serde_json::to_string_pretty(s).unwrap_or_default());
        Self::apply(cx);
        cx.refresh_windows();
    }

    fn apply(cx: &mut App) {
        let s = Self::get(cx).clone();
        match s.theme.as_str() {
            "light" => Theme::change(ThemeMode::Light, None, cx),
            "dark" => Theme::change(ThemeMode::Dark, None, cx),
            _ => Theme::sync_system_appearance(None, cx),
        }
        Theme::update(cx, |t| t.font_size = px(s.ui_font_size as f32));
        crate::engine::configure(&s.rtex_path, &s.tex_bin);
    }
}

pub struct SettingsView;

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let d = AppSettings::default();
        let number = |min: f64, max: f64, step: f64| NumberFieldOptions { min, max, step };
        let appearance = SettingPage::new("Appearance").group(
            SettingGroup::new().title("Window").items([
                SettingItem::new(
                    "Theme",
                    SettingField::dropdown(
                        vec![
                            ("system".into(), "Follow the system".into()),
                            ("light".into(), "Light".into()),
                            ("dark".into(), "Dark".into()),
                        ],
                        |cx| AppSettings::get(cx).theme.clone().into(),
                        |v, cx| AppSettings::set(cx, |s| s.theme = v.to_string()),
                    )
                    .default_value(d.theme.clone()),
                ),
                SettingItem::new(
                    "Interface font size",
                    SettingField::number_input(
                        number(10.0, 24.0, 1.0),
                        |cx| AppSettings::get(cx).ui_font_size,
                        |v, cx| AppSettings::set(cx, |s| s.ui_font_size = v),
                    )
                    .default_value(d.ui_font_size),
                ),
                SettingItem::new(
                    "Dark pages",
                    SettingField::switch(
                        |cx| AppSettings::get(cx).dark_pages,
                        |v, cx| AppSettings::set(cx, |s| s.dark_pages = v),
                    )
                    .default_value(d.dark_pages),
                )
                .description("In the dark theme, show pages as light text on dark paper. Figures keep their colours; exported PDFs are unchanged."),
            ]),
        );
        let editor = SettingPage::new("Editor")
            .group(
                SettingGroup::new().title("Pages").items([
                    SettingItem::new(
                        "Default zoom (%)",
                        SettingField::number_input(
                            number(25.0, 400.0, 5.0),
                            |cx| AppSettings::get(cx).default_zoom,
                            |v, cx| AppSettings::set(cx, |s| s.default_zoom = v),
                        )
                        .default_value(d.default_zoom),
                    )
                    .description("The zoom a file opens at."),
                    SettingItem::new(
                        "Live formula preview",
                        SettingField::switch(
                            |cx| AppSettings::get(cx).live_preview,
                            |v, cx| AppSettings::set(cx, |s| s.live_preview = v),
                        )
                        .default_value(d.live_preview),
                    )
                    .description("Show a formula's typeset result under its source while you edit it."),
                ]),
            )
            .group(
                SettingGroup::new().title("Source").items([
                    SettingItem::new(
                        "Source font size",
                        SettingField::number_input(
                            number(9.0, 28.0, 1.0),
                            |cx| AppSettings::get(cx).source_font_size,
                            |v, cx| AppSettings::set(cx, |s| s.source_font_size = v),
                        )
                        .default_value(d.source_font_size),
                    )
                    .description("In Source mode and the preamble strip. The source box on the page matches the size of the text it edits."),
                    SettingItem::new(
                        "Source box font",
                        SettingField::input(
                            |cx| AppSettings::get(cx).source_box_font.clone().into(),
                            |v, cx| AppSettings::set(cx, |s| s.source_box_font = v.trim().to_string()),
                        )
                        .default_value(d.source_box_font.clone()),
                    )
                    .description("The font of the source box on the page, at the size of the text it edits. Empty: the font the document sets that text in. Or name a font family, such as Menlo.")
                    .layout(Axis::Vertical),
                    SettingItem::new(
                        "Line numbers",
                        SettingField::switch(
                            |cx| AppSettings::get(cx).line_numbers,
                            |v, cx| AppSettings::set(cx, |s| s.line_numbers = v),
                        )
                        .default_value(d.line_numbers),
                    ),
                    SettingItem::new(
                        "Soft wrap",
                        SettingField::switch(
                            |cx| AppSettings::get(cx).soft_wrap,
                            |v, cx| AppSettings::set(cx, |s| s.soft_wrap = v),
                        )
                        .default_value(d.soft_wrap),
                    ),
                ]),
            )
            .group(
                SettingGroup::new().title("Files").item(
                    SettingItem::new(
                        "Auto-save",
                        SettingField::switch(
                            |cx| AppSettings::get(cx).autosave,
                            |v, cx| AppSettings::set(cx, |s| s.autosave = v),
                        )
                        .default_value(d.autosave),
                    )
                    .description("Save a file a second after you stop typing. Edits are always saved when a tab or the window closes."),
                ),
            );
        let engine = SettingPage::new("Engine").group(
            SettingGroup::new()
                .title("realtime-tex")
                .description("Used by files opened after a change.")
                .items([
                    SettingItem::new(
                        "rtex binary",
                        SettingField::input(
                            |cx| AppSettings::get(cx).rtex_path.clone().into(),
                            |v, cx| AppSettings::set(cx, |s| s.rtex_path = v.trim().to_string()),
                        )
                        .default_value(d.rtex_path.clone()),
                    )
                    .description("Empty: $RTEX_BIN, else rtex on PATH, else the copy the Realtime TeX VS Code extension downloaded.")
                    .layout(Axis::Vertical),
                    SettingItem::new(
                        "TeX folder",
                        SettingField::input(
                            |cx| AppSettings::get(cx).tex_bin.clone().into(),
                            |v, cx| AppSettings::set(cx, |s| s.tex_bin = v.trim().to_string()),
                        )
                        .default_value(d.tex_bin.clone()),
                    )
                    .description("Where lualatex and kpsewhich are (MacTeX: /Library/TeX/texbin). Several folders: separate them with ':'.")
                    .layout(Axis::Vertical),
                ]),
        );
        // a drawn title bar, so it follows the theme like the main window's
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
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
                            .child("Settings"),
                    )
                    .bg(cx.theme().title_bar)
                    .border_color(cx.theme().title_bar_border),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(Settings::new("settings").pages([appearance, editor, engine])),
            )
    }
}
