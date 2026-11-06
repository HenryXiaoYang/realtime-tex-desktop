// App settings: kept as a GPUI global, saved as JSON in
// <app data>/Realtime Tex Desktop/settings.json (~/Library/Application Support on the Mac), and edited in the Settings
// window (⌘,), built from gpui-kit's settings components.
use crate::ui::Tip;
use crate::ui::keymap;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::setting::{
    NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings,
};
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable, Theme, ThemeMode, ThemeRegistry, TitleBar,
};
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
    /// Save a file `autosave_delay` seconds after the last edit.
    pub autosave: bool,
    pub autosave_delay: f64,
    /// Open an exported PDF in the default viewer.
    pub open_after_export: bool,
    /// rtex's fast-path budget per paragraph, in ms (`--fast-budget-ms`).
    pub fast_budget_ms: f64,
    /// Reuse pictures (TikZ) a full run drew before, rather than drawing each again.
    pub picture_cache: bool,
    /// Where rtex writes a bundle per engine failure (`--debug-dir`); empty: none.
    pub debug_dir: String,
    /// The rtex binary; empty finds it ($RTEX_BIN, PATH, the VS Code extension's copy).
    pub rtex_path: String,
    /// Folders with lualatex and kpsewhich, added to PATH.
    pub tex_bin: String,
    /// The ribbon folded to its tab row.
    pub ribbon_collapsed: bool,
    /// The sidebar on the right closed (it stays so until shown again).
    pub inspector_closed: bool,
    /// Shortcuts changed from their defaults: command to key (empty: none).
    pub shortcuts: std::collections::BTreeMap<String, String>,
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
            autosave_delay: 1.0,
            open_after_export: false,
            fast_budget_ms: 50.0,
            picture_cache: true,
            debug_dir: String::new(),
            rtex_path: String::new(),
            tex_bin: crate::engine::default_tex_bin().into(),
            ribbon_collapsed: false,
            inspector_closed: false,
            shortcuts: Default::default(),
        }
    }
}

impl Global for AppSettings {}

/// Where the app keeps what it remembers.
pub fn support_dir() -> PathBuf {
    crate::engine::data_dir().join("Realtime Tex Desktop")
}

fn file() -> PathBuf {
    support_dir().join("settings.json")
}

impl AppSettings {
    /// Read the saved settings (defaults when there are none) and apply them.
    pub fn init(cx: &mut App) {
        let s = std::fs::read_to_string(file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        cx.set_global::<AppSettings>(s);
        install_theme(cx);
        crate::ui::workspace::place_notifications(cx);
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
        // tests change settings too: never the real file
        if !cfg!(test) {
            let _ = std::fs::write(&path, serde_json::to_string_pretty(s).unwrap_or_default());
        }
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
        crate::engine::configure_serve(s.serve_args());
    }

    /// The options `rtex serve` gets from these settings.
    fn serve_args(&self) -> Vec<String> {
        let mut a = vec![
            "--fast-budget-ms".into(),
            (self.fast_budget_ms.round() as u64).to_string(),
        ];
        if !self.picture_cache {
            a.push("--no-picture-cache".into());
        }
        if !self.debug_dir.is_empty() {
            a.extend(["--debug-dir".into(), self.debug_dir.clone()]);
        }
        a
    }
}

/// The app's own colours (theme.json), light and dark: an accent for what acts and what is
/// picked, softer grays than black on white. Every switch of mode, the system's too, takes them.
fn install_theme(cx: &mut App) {
    let registry = ThemeRegistry::global_mut(cx);
    if let Err(e) = registry.load_themes_from_str(include_str!("theme.json")) {
        eprintln!("the app's theme: {e}");
        return;
    }
    let get = |name: &str| registry.themes().get(name).cloned();
    let (light, dark) = (get("Realtime Tex Light"), get("Realtime Tex Dark"));
    Theme::update(cx, |t| {
        if let Some(l) = light {
            t.light_theme = l;
        }
        if let Some(d) = dark {
            t.dark_theme = d;
        }
    });
}

/// Keyboard shortcuts: each command and its key; click the key and press a new one.
fn shortcuts_page() -> SettingPage {
    let all = keymap::commands();
    let mut page = SettingPage::new("Keyboard shortcuts");
    for group in ["File", "Edit", "Format", "View"] {
        let items: Vec<SettingItem> = all
            .iter()
            .filter(|c| c.group == group)
            .map(|c| {
                let (id, label) = (c.id, c.label);
                SettingItem::render(move |_, window, cx| shortcut_row(id, label, window, cx))
                    .keywords([label, "shortcut", "key"])
            })
            .collect();
        page = page.group(SettingGroup::new().title(group).items(items));
    }
    page.group(
        SettingGroup::new().item(
            SettingItem::render(|_, _, cx| {
                let changed = !AppSettings::get(cx).shortcuts.is_empty();
                div().flex().justify_end().child(
                    Button::new("shortcuts-reset-all")
                        .label("Reset all to defaults")
                        .small()
                        .outline()
                        .disabled(!changed)
                        .on_click(|_, window, cx| {
                            keymap::reset_all(cx);
                            window.refresh();
                        }),
                )
            })
            .keywords(["reset", "shortcuts"]),
        ),
    )
}

/// A command's row: its name, its key (click to record another), and reset; or, after a
/// recorded key another command has, Replace and Cancel.
fn shortcut_row(id: &'static str, label: &'static str, _: &mut Window, cx: &mut App) -> AnyElement {
    let all = keymap::commands();
    let Some(c) = all.iter().find(|c| c.id == id) else {
        return div().into_any_element();
    };
    let key = keymap::key_of(c, cx);
    let theme = cx.theme().clone();
    let rec = cx.try_global::<keymap::Recording>();
    let recording = rec.is_some_and(|r| r.id == Some(id));
    let clash = rec
        .and_then(|r| r.clash.clone())
        .filter(|(who, ..)| *who == id);
    let row = div()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .child(div().text_sm().child(label));
    if let Some((_, new, other)) = clash {
        let other_label = all
            .iter()
            .find(|c| c.id == other)
            .map_or(other, |c| c.label);
        return row
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.danger)
                            .child(format!("{} is {other_label}'s", keymap::display(&new))),
                    )
                    .child(
                        Button::new(SharedString::from(format!("sc-replace-{id}")))
                            .label("Replace")
                            .xsmall()
                            .primary()
                            .on_click(move |_, window, cx| {
                                cx.set_global(keymap::Recording::default());
                                keymap::set_key(other, "", cx);
                                keymap::set_key(id, &new, cx);
                                window.refresh();
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("sc-cancel-{id}")))
                            .label("Cancel")
                            .xsmall()
                            .outline()
                            .on_click(|_, window, cx| {
                                cx.set_global(keymap::Recording::default());
                                window.refresh();
                            }),
                    ),
            )
            .into_any_element();
    }
    let changed = key != c.key;
    let default = c.key.clone();
    row.child(
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                Button::new(SharedString::from(format!("sc-{id}")))
                    .label(if recording {
                        "Press keys…".to_string()
                    } else {
                        keymap::display(&key)
                    })
                    .small()
                    .outline()
                    .w(px(120.0))
                    .tooltip("Click, then press the new shortcut (Escape cancels)")
                    .on_click(move |_, window, cx| {
                        keymap::record(id, cx);
                        window.refresh();
                    }),
            )
            .child(
                Button::new(SharedString::from(format!("sc-reset-{id}")))
                    .icon(Icon::default().path("icons/undo.svg"))
                    .ghost()
                    .xsmall()
                    .tip("Back to the default")
                    .disabled(!changed)
                    .on_click(move |_, window, cx| {
                        keymap::set_key(id, &default, cx);
                        window.refresh();
                    }),
            ),
    )
    .into_any_element()
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
        )
        .group(
            SettingGroup::new().title("Workspace").items([
                SettingItem::new(
                    "Fold the ribbon",
                    SettingField::switch(
                        |cx| AppSettings::get(cx).ribbon_collapsed,
                        |v, cx| AppSettings::set(cx, |s| s.ribbon_collapsed = v),
                    )
                    .default_value(d.ribbon_collapsed),
                )
                .description("Show only the ribbon's tabs; click one to open it again. Double-clicking a tab does the same."),
                SettingItem::new(
                    "Show the sidebar",
                    SettingField::switch(
                        |cx| !AppSettings::get(cx).inspector_closed,
                        |v, cx| AppSettings::set(cx, |s| s.inspector_closed = !v),
                    )
                    .default_value(!d.inspector_closed),
                )
                .description("The panel on the right with the settings of the element at the caret. A narrow window hides it until it widens."),
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
                SettingGroup::new().title("Files").items([
                    SettingItem::new(
                        "Auto-save",
                        SettingField::switch(
                            |cx| AppSettings::get(cx).autosave,
                            |v, cx| AppSettings::set(cx, |s| s.autosave = v),
                        )
                        .default_value(d.autosave),
                    )
                    .description("Save a file once you stop typing. Edits are always saved when a tab or the window closes."),
                    SettingItem::new(
                        "Auto-save delay (seconds)",
                        SettingField::number_input(
                            number(0.5, 60.0, 0.5),
                            |cx| AppSettings::get(cx).autosave_delay,
                            |v, cx| AppSettings::set(cx, |s| s.autosave_delay = v),
                        )
                        .default_value(d.autosave_delay),
                    )
                    .description("How long typing pauses before an auto-save."),
                    SettingItem::new(
                        "Open the PDF after export",
                        SettingField::switch(
                            |cx| AppSettings::get(cx).open_after_export,
                            |v, cx| AppSettings::set(cx, |s| s.open_after_export = v),
                        )
                        .default_value(d.open_after_export),
                    )
                    .description("Show an exported PDF in your PDF viewer (Preview, unless you chose another)."),
                ]),
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
                    .description("Where lualatex and kpsewhich are (MacTeX: /Library/TeX/texbin). Several folders: separate them as PATH does (':', on Windows ';').")
                    .layout(Axis::Vertical),
                ]),
        )
        .group(
            SettingGroup::new()
                .title("Typesetting")
                .description("Used by files opened after a change.")
                .items([
                    SettingItem::new(
                        "Live paragraph budget (ms)",
                        SettingField::number_input(
                            number(10.0, 1000.0, 10.0),
                            |cx| AppSettings::get(cx).fast_budget_ms,
                            |v, cx| AppSettings::set(cx, |s| s.fast_budget_ms = v),
                        )
                        .default_value(d.fast_budget_ms),
                    )
                    .description("A paragraph that takes longer than this to typeset three times in a row waits for the full LaTeX run instead, until the next layout. Higher keeps more paragraphs live on a slow Mac."),
                    SettingItem::new(
                        "Reuse drawn pictures",
                        SettingField::switch(
                            |cx| AppSettings::get(cx).picture_cache,
                            |v, cx| AppSettings::set(cx, |s| s.picture_cache = v),
                        )
                        .default_value(d.picture_cache),
                    )
                    .description("A full run keeps TikZ pictures it drew before when they did not change. Turn off if a picture shows an old version."),
                    SettingItem::new(
                        "Diagnostics folder",
                        SettingField::input(
                            |cx| AppSettings::get(cx).debug_dir.clone().into(),
                            |v, cx| AppSettings::set(cx, |s| s.debug_dir = v.trim().to_string()),
                        )
                        .default_value(d.debug_dir.clone()),
                    )
                    .description("Empty: none. A folder where rtex writes what it was given each time its engine fails, and a line per live paragraph: attach it to an rtex bug report.")
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
                    .child(Settings::new("settings").pages([
                        appearance,
                        editor,
                        engine,
                        shortcuts_page(),
                    ])),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::AppSettings;

    #[test]
    fn rtex_gets_the_engine_settings() {
        let d = AppSettings::default();
        assert_eq!(d.serve_args(), ["--fast-budget-ms", "50"]);
        let s = AppSettings {
            fast_budget_ms: 120.4,
            picture_cache: false,
            debug_dir: "/tmp/rtex debug".into(),
            ..d
        };
        assert_eq!(
            s.serve_args(),
            [
                "--fast-budget-ms",
                "120",
                "--no-picture-cache",
                "--debug-dir",
                "/tmp/rtex debug"
            ]
        );
        // a settings file from before these settings reads with their defaults
        let old: AppSettings = serde_json::from_str(r#"{"autosave":true}"#).unwrap();
        assert_eq!(old.autosave_delay, 1.0);
        assert!(old.picture_cache);
    }
}
