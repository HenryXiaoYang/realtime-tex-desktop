// App settings: kept as a GPUI global, saved as JSON in
// <app data>/Realtime Tex Desktop/settings.json (~/Library/Application Support on the Mac), and edited in the Settings
// window (⌘,), built from gpui-kit's settings components.
use crate::ui::Tip;
use crate::ui::keymap;
use crate::ui::palette;
use crate::ui::themes;
use gpui_kit::base::Scrollbar;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use gpui_kit::component::input::{CursorBlink, Input, InputEvent, InputState};
use gpui_kit::component::select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState};
use gpui_kit::component::setting::{
    NumberFieldOptions, RenderOptions, SettingField, SettingGroup, SettingItem, SettingPage,
    Settings,
};
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable, Theme, ThemeMode, ThemeRegistry, TitleBar, h_flex,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    /// "system", "light" or "dark".
    pub theme: String,
    /// The accent colour, "#RRGGBB"; empty: the theme's own blue.
    pub accent: String,
    /// The theme in light mode and in dark mode, by name.
    pub light_theme: String,
    pub dark_theme: String,
    /// Colours changed from a theme's own: theme name to colour key (as theme files write it) to
    /// "#RRGGBB" or "#RRGGBBAA". They win over the accent.
    pub colors: BTreeMap<String, BTreeMap<String, String>>,
    pub ui_font_size: f64,
    /// Pages on dark paper in the dark theme.
    pub dark_pages: bool,
    /// The caret blinks (the pages' and every text field's).
    pub cursor_blink: bool,
    /// The Source mode editor and the preamble strip, at 100% zoom.
    pub source_font_size: f64,
    /// Their font family; empty: Menlo.
    pub source_font: String,
    /// The source box on the page: a font family, or empty for the document's own font.
    pub source_box_font: String,
    /// The zoom a file opens at.
    pub default_zoom: Zoom,
    /// A .tex file opens in Visual mode (its pages) or Source mode.
    pub open_tex_in: TexView,
    /// A pinch on the trackpad zooms the pages.
    pub pinch_zoom: bool,
    /// A formula's typeset result under its source while it is edited.
    pub live_preview: bool,
    /// A `\` typed on the pages opens the paragraph's source there; off, it is typed as text.
    pub backslash_opens_source: bool,
    pub line_numbers: bool,
    pub soft_wrap: bool,
    /// The LaTeX completion menu opens as you type.
    pub autocomplete: bool,
    /// Find starts out matching case.
    pub find_match_case: bool,
    /// At launch: the projects open when the app quit, or the welcome window.
    pub on_launch: OnLaunch,
    /// How many projects the recent list keeps.
    pub recent_kept: usize,
    /// rtex fetched at launch when it is nowhere the app looks.
    pub download_rtex: bool,
    /// The file tree shows names starting with a dot.
    pub show_hidden_files: bool,
    /// Names the file tree leaves out: comma-separated patterns (`*` any run, `?` one character).
    pub hide_files: String,
    /// The deepest heading the outline lists.
    pub outline_depth: OutlineDepth,
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
    /// The rtex binary; empty finds it ($RTEX_BIN, PATH, the app's own copy).
    pub rtex_path: String,
    /// Folders with lualatex and kpsewhich, added to PATH.
    pub tex_bin: String,
    /// The ribbon folded to its tab row.
    pub ribbon_collapsed: bool,
    /// The sidebar on the right closed (it stays so until shown again).
    pub inspector_closed: bool,
    /// The tour was started once (the first launch starts it no more).
    pub tour_seen: bool,
    /// Shortcuts changed from their defaults: command to key (empty: none).
    pub shortcuts: BTreeMap<String, String>,
}

/// The zoom a file opens at: saved as a number, or null for fit width (a number is what
/// settings files from before fit width hold).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(untagged)]
pub enum Zoom {
    Percent(f64),
    FitWidth,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TexView {
    #[default]
    Visual,
    Source,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum OnLaunch {
    #[default]
    Reopen,
    Welcome,
}

/// Each is how many heading levels the outline lists.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum OutlineDepth {
    Sections = 1,
    Subsections = 2,
    #[default]
    Subsubsections = 3,
    Paragraphs = 4,
}

impl OutlineDepth {
    /// Whether the outline lists a heading of `level` (0: \section).
    pub fn shows(self, level: usize) -> bool {
        level < self as usize
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            accent: String::new(),
            light_theme: themes::DEFAULT_LIGHT.into(),
            dark_theme: themes::DEFAULT_DARK.into(),
            colors: BTreeMap::new(),
            ui_font_size: 16.0,
            dark_pages: true,
            cursor_blink: true,
            source_font_size: 13.0,
            source_font: String::new(),
            source_box_font: String::new(),
            default_zoom: Zoom::Percent(100.0),
            open_tex_in: TexView::Visual,
            pinch_zoom: true,
            live_preview: true,
            backslash_opens_source: true,
            line_numbers: true,
            soft_wrap: true,
            autocomplete: true,
            find_match_case: false,
            on_launch: OnLaunch::Reopen,
            recent_kept: 30,
            download_rtex: true,
            show_hidden_files: false,
            hide_files: String::new(),
            outline_depth: OutlineDepth::Subsubsections,
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
            tour_seen: false,
            shortcuts: Default::default(),
        }
    }
}

impl Global for AppSettings {}

/// The open documents' rtex restart that waits for the engine settings to settle; a newer one
/// replaces (and so cancels) it.
struct EngineRestart {
    _task: Task<()>,
}

impl Global for EngineRestart {}

/// Where the app keeps what it remembers.
pub fn support_dir() -> PathBuf {
    crate::engine::app_dir()
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
        themes::init(cx);
        if let Err(e) = ThemeRegistry::watch_dir(themes::dir(), cx, |_| {}) {
            eprintln!("the themes folder: {e}");
        }
        crate::ui::workspace::place_notifications(cx);
        Self::apply(cx);
    }

    pub fn get(cx: &App) -> &AppSettings {
        cx.global::<AppSettings>()
    }

    /// Change, save and apply; every window redraws with the new values, and the open
    /// documents' rtex starts again on changed engine settings.
    pub fn set(cx: &mut App, f: impl FnOnce(&mut AppSettings)) {
        let engine_before = Self::get(cx).engine();
        f(cx.global_mut::<AppSettings>());
        if Self::get(cx).engine() != engine_before {
            // once they stop changing: a field sets them at each keystroke
            let restart = cx.spawn(async |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
                cx.update(crate::ui::projects::restart_engines);
            });
            cx.set_global(EngineRestart { _task: restart });
        }
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
        themes::install(cx);
        match s.theme.as_str() {
            "light" => Theme::change(ThemeMode::Light, None, cx),
            "dark" => Theme::change(ThemeMode::Dark, None, cx),
            _ => Theme::sync_system_appearance(None, cx),
        }
        Theme::update(cx, |t| {
            t.font_size = px(s.ui_font_size as f32);
            t.cursor_blink = if s.cursor_blink {
                CursorBlink::default()
            } else {
                CursorBlink::steady()
            };
        });
        crate::engine::configure(&s.rtex_path, &s.tex_bin);
        crate::engine::configure_serve(s.serve_args());
    }

    /// What `rtex serve` is started with.
    fn engine(&self) -> (String, String, Vec<String>) {
        (
            self.rtex_path.clone(),
            self.tex_bin.clone(),
            self.serve_args(),
        )
    }

    /// The family Source mode and the preamble strip show source in.
    pub fn source_family(&self) -> SharedString {
        match self.source_font.trim() {
            "" => crate::ui::editor::MONO.into(),
            f => f.to_string().into(),
        }
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

/// The accents offered beside a colour of one's own: each named, and its setting ("" is the
/// theme's own blue, theme.json's).
pub(crate) const ACCENTS: [(&str, &str); 8] = [
    ("Blue", ""),
    ("Purple", "#7C4DDB"),
    ("Pink", "#C93A85"),
    ("Red", "#CF3A3A"),
    ("Orange", "#C2570C"),
    ("Green", "#1E7F47"),
    ("Teal", "#0E7C86"),
    ("Graphite", "#5B6472"),
];
pub(crate) const THEME_BLUE: &str = "#3557D4";

/// A ring around a swatch, in its colour when `on`.
fn ring(color: Hsla, on: bool) -> Div {
    div()
        .size(px(22.0))
        .rounded_full()
        .border_2()
        .border_color(if on { color } else { transparent_black() })
        .flex()
        .items_center()
        .justify_center()
}

struct AccentField {
    picker: Entity<ColorPickerState>,
    _sub: Subscription,
}

/// The accent: a round swatch for each of ACCENTS, the one in use ringed, then the color picker
/// for any other colour, which shows that colour while it is the accent.
fn accent_field(options: &RenderOptions, window: &mut Window, cx: &mut App) -> Div {
    let current = palette::hex_color(&AppSettings::get(cx).accent)
        .map(palette::hex)
        .unwrap_or_default();
    let custom = !ACCENTS.iter().any(|(_, v)| *v == current);
    let state = window.use_keyed_state(field_key("accent", options), cx, |window, cx| {
        let picker = cx.new(|cx| ColorPickerState::new(window, cx));
        let _sub = cx.subscribe(&picker, |_, _, ev: &ColorPickerEvent, cx| {
            if let ColorPickerEvent::Change(Some(c)) = ev {
                let v = palette::hex(Hsla { a: 1.0, ..*c });
                if AppSettings::get(cx).accent != v {
                    AppSettings::set(cx, |s| s.accent = v);
                }
            }
        });
        AccentField { picker, _sub }
    });
    // the picker opens at the accent in use
    let shown = palette::hex_color(if current.is_empty() {
        THEME_BLUE
    } else {
        &current
    })
    .unwrap_or_default();
    let picker = state.read(cx).picker.clone();
    if picker.read(cx).value().map(palette::hex) != Some(palette::hex(shown)) {
        picker.update(cx, |p, cx| p.set_value(shown, window, cx));
    }
    h_flex()
        .gap_0p5()
        .children(ACCENTS.map(|(name, value)| {
            let color = palette::hex_color(if value.is_empty() { THEME_BLUE } else { value })
                .unwrap_or_default();
            Button::new(SharedString::from(format!("accent-{name}")))
                .ghost()
                .p_0()
                .size(px(26.0))
                .rounded_full()
                .disabled(options.is_disabled())
                .accessibility_label(name)
                .tooltip(name)
                .child(
                    ring(color, !custom && current == value)
                        .child(div().size(px(14.0)).rounded_full().bg(color)),
                )
                .on_click(move |_, _, cx| AppSettings::set(cx, |s| s.accent = value.into()))
        }))
        .child(
            ring(shown, custom).child(
                ColorPicker::new(&picker)
                    .with_size(px(14.0))
                    .anchor(Anchor::TopRight)
                    .accessibility_label("Custom color")
                    .when(!custom, |p| {
                        p.icon(Icon::default().path("icons/pipette.svg"))
                    }),
            ),
        )
}

/// A group of settings. The page leaves 1 rem beside its list for focus rings, but the
/// scrollbar's track, opaque while it shows, takes 16 px of the list's right edge: the group
/// keeps clear of it the 3 px ring of a control on its right edge, at every interface font size.
fn group(cx: &App) -> SettingGroup {
    let clear = Scrollbar::width() + px(4.0) - cx.theme().font_size;
    SettingGroup::new().pr(clear.max(px(0.0)))
}

/// Keyboard shortcuts: each command and its key; click the key and press a new one.
fn shortcuts_page(cx: &App) -> SettingPage {
    let all = keymap::commands();
    let mut page = SettingPage::new("Keyboard shortcuts");
    for name in ["File", "Edit", "Format", "View"] {
        let items: Vec<SettingItem> = all
            .iter()
            .filter(|c| c.group == name)
            .map(|c| {
                let (id, label) = (c.id, c.label);
                SettingItem::render(move |_, window, cx| shortcut_row(id, label, window, cx))
                    .keywords([label, "shortcut", "key"])
            })
            .collect();
        page = page.group(group(cx).title(name).items(items));
    }
    page.group(
        group(cx).item(
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

/// A switch for a setting.
fn switch(get: fn(&AppSettings) -> bool, set: fn(&mut AppSettings, bool)) -> SettingField<bool> {
    SettingField::switch(
        move |cx| get(AppSettings::get(cx)),
        move |v, cx| AppSettings::set(cx, |s| set(s, v)),
    )
    .default_value(get(&AppSettings::default()))
}

/// A number field for a setting, from `min` to `max` by `step`.
fn number(
    (min, max, step): (f64, f64, f64),
    get: fn(&AppSettings) -> f64,
    set: fn(&mut AppSettings, f64),
) -> SettingField<f64> {
    SettingField::number_input(
        NumberFieldOptions { min, max, step },
        move |cx| get(AppSettings::get(cx)),
        move |v, cx| AppSettings::set(cx, |s| set(s, v)),
    )
    .default_value(get(&AppSettings::default()))
}

/// A dropdown of `options`, each a value and its label.
fn choice<T: PartialEq + Copy + 'static, L: Into<SharedString>>(
    options: impl IntoIterator<Item = (T, L)>,
    get: fn(&AppSettings) -> T,
    set: fn(&mut AppSettings, T),
) -> SettingField<SharedString> {
    let (values, labels): (Vec<T>, Vec<SharedString>) =
        options.into_iter().map(|(v, l)| (v, l.into())).unzip();
    let values = Rc::new(values);
    // the dropdown's keys are the options' places in the list
    let key = {
        let values = values.clone();
        move |v: T| -> SharedString {
            let at = values.iter().position(|o| *o == v).unwrap_or_default();
            at.to_string().into()
        }
    };
    let default = key(get(&AppSettings::default()));
    let keys = labels
        .into_iter()
        .enumerate()
        .map(|(i, l)| (i.to_string().into(), l))
        .collect();
    SettingField::dropdown(
        keys,
        move |cx| key(get(AppSettings::get(cx))),
        move |k, cx| {
            if let Some(&v) = k.parse::<usize>().ok().and_then(|i| values.get(i)) {
                AppSettings::set(cx, |s| set(s, v));
            }
        },
    )
    .default_value(default)
}

/// The key of a field's own state: its kind and its place in the Settings window.
fn field_key(kind: &str, options: &RenderOptions) -> SharedString {
    format!(
        "{kind}-{}-{}-{}",
        options.page_ix(),
        options.group_ix(),
        options.item_ix()
    )
    .into()
}

/// `field`, which the page's reset puts back to the setting's default.
fn resettable(
    field: SettingField<SharedString>,
    get: fn(&AppSettings) -> &String,
    set: fn(&mut AppSettings, String),
) -> SettingField<SharedString> {
    field.on_reset(
        move |cx| get(AppSettings::get(cx)) != get(&AppSettings::default()),
        move |_, cx| AppSettings::set(cx, |s| set(s, get(&AppSettings::default()).clone())),
    )
}

struct TextField {
    input: Entity<InputState>,
    _sub: Subscription,
}

/// A text field whose placeholder says what empty means; the setting follows each keystroke,
/// trimmed.
fn text_input(
    placeholder: &'static str,
    get: fn(&AppSettings) -> &String,
    set: fn(&mut AppSettings, String),
    options: &RenderOptions,
    window: &mut Window,
    cx: &mut App,
) -> Input {
    let state = window.use_keyed_state(field_key("text", options), cx, |window, cx| {
        let value = get(AppSettings::get(cx)).clone();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .default_value(value)
        });
        let _sub = cx.subscribe(&input, move |_, input, ev: &InputEvent, cx| {
            let v = input.read(cx).value().trim().to_string();
            if matches!(ev, InputEvent::Change) && *get(AppSettings::get(cx)) != v {
                AppSettings::set(cx, |s| set(s, v));
            }
        });
        TextField { input, _sub }
    });
    let input = state.read(cx).input.clone();
    // changed elsewhere (the page's reset, Choose…): shown; a space being typed is kept
    let value = get(AppSettings::get(cx)).clone();
    if input.read(cx).value().trim() != value {
        input.update(cx, |s, cx| s.set_value(value, window, cx));
    }
    Input::new(&input)
        .disabled(options.is_disabled())
        .with_size(options.size())
}

/// A text setting's field.
fn text(
    placeholder: &'static str,
    get: fn(&AppSettings) -> &String,
    set: fn(&mut AppSettings, String),
) -> SettingField<SharedString> {
    let field = SettingField::render(move |options: &RenderOptions, window, cx| {
        let field = text_input(placeholder, get, set, options, window, cx);
        if options.layout() == Axis::Horizontal {
            field.w_64()
        } else {
            field.w_full()
        }
    });
    resettable(field, get, set)
}

/// A path's text field, and Choose… beside it for the system's file (or folder) chooser.
fn path(
    placeholder: &'static str,
    folder: bool,
    get: fn(&AppSettings) -> &String,
    set: fn(&mut AppSettings, String),
) -> SettingField<SharedString> {
    let field = SettingField::render(move |options: &RenderOptions, window, cx| {
        h_flex()
            .w_full()
            .gap_2()
            .child(text_input(placeholder, get, set, options, window, cx).flex_1())
            .child(
                Button::new(field_key("choose", options))
                    .label("Choose…")
                    .outline()
                    .with_size(options.size())
                    .disabled(options.is_disabled())
                    .on_click(move |_, _, cx| choose_path(folder, set, cx)),
            )
    });
    resettable(field, get, set)
}

/// The system's chooser, for a file or a folder; the one chosen becomes the setting.
fn choose_path(folder: bool, set: fn(&mut AppSettings, String), cx: &mut App) {
    let rx = cx.prompt_for_paths(PathPromptOptions {
        files: !folder,
        directories: folder,
        multiple: false,
        prompt: Some("Choose".into()),
    });
    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(paths))) = rx.await
            && let Some(p) = paths.first()
        {
            let p = p.to_string_lossy().into_owned();
            cx.update(|cx| AppSettings::set(cx, |s| set(s, p)));
        }
    })
    .detach();
}

/// A family in a font setting's list, drawn in its own face; `name` "" is the setting's default.
#[derive(Clone)]
struct FontChoice {
    name: SharedString,
    label: SharedString,
    face: Option<SharedString>,
}

impl FontChoice {
    fn named(n: &str) -> Self {
        let n: SharedString = n.to_string().into();
        FontChoice {
            name: n.clone(),
            label: n.clone(),
            face: Some(n),
        }
    }

    fn preview(&self) -> Div {
        div()
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis()
            .when_some(self.face.clone(), |d, f| d.font_family(f))
            .child(self.label.clone())
    }
}

impl SelectItem for FontChoice {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn display_title(&self) -> Option<AnyElement> {
        Some(self.preview().into_any_element())
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.preview()
    }

    fn value(&self) -> &SharedString {
        &self.name
    }
}

/// A font setting's list: its default first (`default`, drawn in `face`), then the installed
/// families but that face, then `current` when it is none of them (a font since removed).
fn font_choices(
    default: &str,
    face: Option<&str>,
    names: &[String],
    current: &str,
) -> Vec<FontChoice> {
    let mut list = vec![FontChoice {
        name: "".into(),
        label: default.to_string().into(),
        face: face.map(|f| f.to_string().into()),
    }];
    list.extend(
        names
            .iter()
            .filter(|n| Some(n.as_str()) != face)
            .map(|n| FontChoice::named(n)),
    );
    if !current.is_empty() && Some(current) != face && !names.iter().any(|n| n == current) {
        list.push(FontChoice::named(current));
    }
    list
}

type FontSelect = SelectState<SearchableVec<FontChoice>>;

struct FontField {
    select: Entity<FontSelect>,
    /// The setting the list last showed.
    shown: Option<String>,
    _sub: Subscription,
}

impl FontField {
    fn new(
        default: &'static str,
        face: Option<&'static str>,
        get: fn(&AppSettings) -> &String,
        set: fn(&mut AppSettings, String),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let names = crate::ui::font_names(cx);
        let items = font_choices(default, face, names, get(AppSettings::get(cx)));
        let select = cx.new(|cx| {
            SelectState::new(SearchableVec::new(items), None, window, cx).searchable(true)
        });
        let _sub = cx.subscribe(&select, move |_, _, ev: &SelectEvent<_>, cx| {
            if let SelectEvent::Confirm(Some(name)) = ev
                && get(AppSettings::get(cx)) != name.as_ref()
            {
                let name = name.to_string();
                AppSettings::set(cx, |s| set(s, name));
            }
        });
        FontField {
            select,
            shown: None,
            _sub,
        }
    }
}

/// A searchable list of the installed fonts, each drawn in its face; the first, `default`
/// (drawn in `face`), leaves the setting empty.
fn font(
    default: &'static str,
    face: Option<&'static str>,
    get: fn(&AppSettings) -> &String,
    set: fn(&mut AppSettings, String),
) -> SettingField<SharedString> {
    let field = SettingField::render(move |options: &RenderOptions, window, cx| {
        let state = window.use_keyed_state(field_key("font", options), cx, |window, cx| {
            FontField::new(default, face, get, set, window, cx)
        });
        // changed elsewhere (the page's reset): picked in the list
        let value = get(AppSettings::get(cx)).clone();
        if state.read(cx).shown.as_ref() != Some(&value) {
            let pick: SharedString = match face {
                Some(f) if f == value => "".into(),
                _ => value.clone().into(),
            };
            let select = state.read(cx).select.clone();
            select.update(cx, |s, cx| s.set_selected_value(&pick, window, cx));
            state.update(cx, |f, _| f.shown = Some(value));
        }
        let select = Select::new(&state.read(cx).select)
            .disabled(options.is_disabled())
            .with_size(options.size())
            .menu_max_h(px(320.0));
        if options.layout() == Axis::Horizontal {
            select.w_64()
        } else {
            select.w_full()
        }
    });
    resettable(field, get, set)
}

/// A theme list for `mode`: the themes of that mode, the one in use picked (the app's own when
/// the chosen one is gone).
fn theme_list(
    mode: ThemeMode,
    get: fn(&AppSettings) -> &String,
    set: fn(&mut AppSettings, String),
    cx: &App,
) -> SettingField<SharedString> {
    let names = themes::names(mode, cx);
    SettingField::dropdown(
        names.into_iter().map(|n| (n.clone(), n)).collect(),
        move |cx| {
            let s = AppSettings::get(cx);
            themes::chosen(s, mode, cx).map_or_else(|| get(s).clone().into(), |t| t.name.clone())
        },
        move |v, cx| AppSettings::set(cx, |s| set(s, v.to_string())),
    )
    .default_value(get(&AppSettings::default()).clone())
}

/// Every colour of the theme on screen, by group, each changeable; the theme's name heads the
/// page's first group, with Export and Reset all.
fn colors_page(cx: &App) -> SettingPage {
    let name = themes::active(cx).unwrap_or_default();
    let shown = Rc::new(themes::shown(cx));
    let changed = AppSettings::get(cx).colors.contains_key(name.as_ref());
    let actions = SettingItem::render(move |_, _, _| {
        h_flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("colors-export")
                    .label("Export theme")
                    .small()
                    .outline()
                    .on_click(|_, _, cx| match themes::export(cx) {
                        Ok(path) => cx.reveal_path(&path),
                        Err(e) => eprintln!("export theme: {e}"),
                    }),
            )
            .child(
                Button::new("colors-reset-all")
                    .label("Reset all colors")
                    .small()
                    .outline()
                    .disabled(!changed)
                    .on_click(|_, _, cx| {
                        if let Some(name) = themes::active(cx) {
                            AppSettings::set(cx, |s| {
                                s.colors.remove(name.as_ref());
                            });
                        }
                    }),
            )
    })
    .keywords(["export", "reset", "theme"]);
    let mut page = SettingPage::new("Colors").group(group(cx).title(name).item(actions));
    for g in themes::groups() {
        let items = g.keys.iter().map(|k| {
            let (key, color) = (k.key.clone(), shown.get(&k.key).copied().flatten());
            SettingItem::new(
                k.label.clone(),
                SettingField::render(move |_: &RenderOptions, window, cx| {
                    color_field(&key, color, window, cx)
                }),
            )
            .keywords([k.key.clone()])
        });
        page = page.group(group(cx).title(g.title).items(items.collect::<Vec<_>>()));
    }
    page
}

struct ColorRow {
    picker: Entity<ColorPickerState>,
    _sub: Subscription,
}

/// A colour's field: its value, a swatch that opens the colour picker (transparency shows over a
/// checkerboard), and, while it is changed, reset to the theme's own. "Auto": a source colour the
/// theme leaves to the editor.
fn color_field(key: &str, color: Option<Hsla>, window: &mut Window, cx: &mut App) -> Div {
    #[cfg(test)]
    tests::COLOR_ROWS.with(|r| r.borrow_mut().insert(key.to_string()));
    // keyed by the colour, not the row's place: a search moves rows
    let state = window.use_keyed_state(SharedString::from(format!("color-{key}")), cx, {
        let key = key.to_string();
        move |window, cx| {
            let picker = cx.new(|cx| ColorPickerState::new(window, cx));
            let _sub = cx.subscribe(&picker, move |_, _, ev: &ColorPickerEvent, cx| {
                if let ColorPickerEvent::Change(Some(c)) = ev {
                    themes::set_color(&key, Some(palette::hex(*c)), cx);
                }
            });
            ColorRow { picker, _sub }
        }
    });
    let picker = state.read(cx).picker.clone();
    if picker.read(cx).value().map(palette::hex) != color.map(palette::hex) {
        picker.update(cx, |p, cx| match color {
            Some(c) => p.set_value(c, window, cx),
            None => p.clear_value(window, cx),
        });
    }
    let changed = themes::active(cx).is_some_and(|name| {
        let colors = &AppSettings::get(cx).colors;
        colors
            .get(name.as_ref())
            .is_some_and(|c| c.contains_key(key))
    });
    let theme = cx.theme();
    h_flex()
        .gap_2()
        .when(changed, |row| {
            let key = key.to_string();
            row.child(
                Button::new(SharedString::from(format!("color-reset-{key}")))
                    .icon(Icon::default().path("icons/undo.svg"))
                    .ghost()
                    .xsmall()
                    .tip("Back to the theme's color")
                    .on_click(move |_, _, cx| themes::set_color(&key, None, cx)),
            )
        })
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(color.map_or("Auto".into(), palette::hex)),
        )
        .child(
            div()
                .rounded(theme.radius)
                .bg(checkerboard(theme.muted_foreground.opacity(0.4), 4.0))
                .child(ColorPicker::new(&picker).anchor(Anchor::TopRight)),
        )
}

/// Clear: the build files of the documents not open now removed, in the background (an open
/// document's rtex is using its own).
fn clear_cache_button(options: &RenderOptions, window: &mut Window, cx: &mut App) -> Button {
    let cleared = window.use_keyed_state("cache-cleared", cx, |_, _| false);
    let done = *cleared.read(cx);
    Button::new("clear-cache")
        .label(if done { "Cleared" } else { "Clear" })
        .outline()
        .with_size(options.size())
        .disabled(done)
        .on_click(move |_, window, cx| {
            let keep = crate::ui::projects::open_build_dirs(cx);
            cx.background_executor()
                .spawn(async move { crate::engine::clear_build_cache(&keep) })
                .detach();
            cleared.update(cx, |c, _| *c = true);
            window.refresh();
        })
}

/// The Settings window's pages, the most used first on each.
fn pages(cx: &App) -> Vec<SettingPage> {
    let s = AppSettings::get(cx);
    let icon = |name: &str| Icon::default().path(format!("icons/{name}.svg"));
    let general = SettingPage::new("General")
        .icon(icon("sliders-horizontal"))
        .group(group(cx).title("Startup").items([
            SettingItem::new(
                "On launch",
                choice(
                    [
                        (OnLaunch::Reopen, "Reopen last projects"),
                        (OnLaunch::Welcome, "Show the Welcome window"),
                    ],
                    |s| s.on_launch,
                    |s, v| s.on_launch = v,
                ),
            ),
            SettingItem::new(
                "Recent projects kept",
                number(
                    (5.0, 100.0, 1.0),
                    |s| s.recent_kept as f64,
                    |s, v| s.recent_kept = v.round() as usize,
                ),
            ),
        ]))
        .group(
            group(cx).title("Saving").items([
                SettingItem::new("Auto-save", switch(|s| s.autosave, |s, v| s.autosave = v)),
                SettingItem::new(
                    "Auto-save delay (seconds)",
                    number(
                        (0.5, 60.0, 0.5),
                        |s| s.autosave_delay,
                        |s, v| s.autosave_delay = v,
                    ),
                )
                .disabled(!s.autosave),
            ]),
        )
        .group(group(cx).title("Export").item(SettingItem::new(
            "Open the PDF after export",
            switch(|s| s.open_after_export, |s, v| s.open_after_export = v),
        )));
    let appearance = SettingPage::new("Appearance")
        .icon(icon("palette"))
        .group(
            group(cx).title("Window").items([
                SettingItem::new(
                    "Theme",
                    SettingField::dropdown(
                        vec![
                            ("system".into(), "System".into()),
                            ("light".into(), "Light".into()),
                            ("dark".into(), "Dark".into()),
                        ],
                        |cx| AppSettings::get(cx).theme.clone().into(),
                        |v, cx| AppSettings::set(cx, |s| s.theme = v.to_string()),
                    )
                    .default_value(AppSettings::default().theme),
                ),
                SettingItem::new(
                    "Light theme",
                    theme_list(
                        ThemeMode::Light,
                        |s| &s.light_theme,
                        |s, v| s.light_theme = v,
                        cx,
                    ),
                ),
                SettingItem::new(
                    "Dark theme",
                    theme_list(
                        ThemeMode::Dark,
                        |s| &s.dark_theme,
                        |s, v| s.dark_theme = v,
                        cx,
                    ),
                ),
                SettingItem::new(
                    "Themes folder",
                    SettingField::render(|options: &RenderOptions, _, _| {
                        Button::new("themes-folder")
                            .label("Open")
                            .outline()
                            .with_size(options.size())
                            .on_click(|_, _, cx| {
                                let dir = themes::dir();
                                let _ = std::fs::create_dir_all(&dir);
                                cx.open_with_system(&dir);
                            })
                    }),
                ),
                SettingItem::new(
                    "Accent color",
                    resettable(
                        SettingField::render(accent_field),
                        |s| &s.accent,
                        |s, v| s.accent = v,
                    ),
                )
                .keywords(["colour", "highlight"]),
                SettingItem::new(
                    "Interface font size",
                    number(
                        (10.0, 24.0, 1.0),
                        |s| s.ui_font_size,
                        |s, v| s.ui_font_size = v,
                    ),
                ),
                SettingItem::new(
                    "Dark pages",
                    switch(|s| s.dark_pages, |s, v| s.dark_pages = v),
                ),
                SettingItem::new(
                    "Blinking caret",
                    switch(|s| s.cursor_blink, |s, v| s.cursor_blink = v),
                )
                .keywords(["cursor"]),
            ]),
        )
        .group(
            group(cx).title("Layout").items([
                SettingItem::new(
                    "Show ribbon tools",
                    switch(|s| !s.ribbon_collapsed, |s, v| s.ribbon_collapsed = !v),
                )
                .keywords(["toolbar"]),
                SettingItem::new(
                    "Show sidebar",
                    switch(|s| !s.inspector_closed, |s, v| s.inspector_closed = !v),
                )
                .keywords(["inspector"]),
            ]),
        );
    let mut zooms: Vec<(Zoom, SharedString)> = [50, 75, 100, 125, 150, 200]
        .map(|p| (Zoom::Percent(p.into()), format!("{p}%").into()))
        .into();
    zooms.insert(0, (Zoom::FitWidth, "Fit width".into()));
    // a zoom typed in before there was this list
    if let Zoom::Percent(p) = s.default_zoom
        && !zooms.iter().any(|z| z.0 == s.default_zoom)
    {
        zooms.push((s.default_zoom, format!("{p}%").into()));
    }
    let editor = SettingPage::new("Editor")
        .icon(icon("pencil-line"))
        .group(
            group(cx).title("Pages").items([
                SettingItem::new(
                    "Open files at",
                    choice(zooms, |s| s.default_zoom, |s, v| s.default_zoom = v),
                )
                .keywords(["zoom"]),
                SettingItem::new(
                    "Open .tex files in",
                    choice(
                        [(TexView::Visual, "Visual"), (TexView::Source, "Source")],
                        |s| s.open_tex_in,
                        |s, v| s.open_tex_in = v,
                    ),
                ),
                SettingItem::new(
                    "Live formula preview",
                    switch(|s| s.live_preview, |s, v| s.live_preview = v),
                ),
                SettingItem::new(
                    "Typing \\ opens the source",
                    switch(
                        |s| s.backslash_opens_source,
                        |s, v| s.backslash_opens_source = v,
                    ),
                )
                .keywords(["backslash"]),
                SettingItem::new(
                    "Pinch to zoom",
                    switch(|s| s.pinch_zoom, |s, v| s.pinch_zoom = v),
                ),
            ]),
        )
        .group(
            group(cx).title("Source").items([
                SettingItem::new(
                    "Source font",
                    font(
                        crate::ui::editor::MONO,
                        Some(crate::ui::editor::MONO),
                        |s| &s.source_font,
                        |s, v| s.source_font = v,
                    ),
                ),
                SettingItem::new(
                    "Source font size",
                    number(
                        (9.0, 28.0, 1.0),
                        |s| s.source_font_size,
                        |s, v| s.source_font_size = v,
                    ),
                ),
                SettingItem::new(
                    "Source box font",
                    font(
                        "Document font",
                        None,
                        |s| &s.source_box_font,
                        |s, v| s.source_box_font = v,
                    ),
                ),
                SettingItem::new(
                    "Line numbers",
                    switch(|s| s.line_numbers, |s, v| s.line_numbers = v),
                ),
                SettingItem::new("Soft wrap", switch(|s| s.soft_wrap, |s, v| s.soft_wrap = v)),
                SettingItem::new(
                    "Autocomplete",
                    switch(|s| s.autocomplete, |s, v| s.autocomplete = v),
                )
                .keywords(["completion"]),
            ]),
        )
        .group(group(cx).title("Find").item(SettingItem::new(
            "Match case by default",
            switch(|s| s.find_match_case, |s, v| s.find_match_case = v),
        )));
    let files = SettingPage::new("Files").icon(icon("folder")).group(
        group(cx).title("Project panel").items([
            SettingItem::new(
                "Show hidden files",
                switch(|s| s.show_hidden_files, |s, v| s.show_hidden_files = v),
            ),
            SettingItem::new(
                "Hide files matching",
                text("*.aux, *.log", |s| &s.hide_files, |s, v| s.hide_files = v),
            ),
            SettingItem::new(
                "Outline depth",
                choice(
                    [
                        (OutlineDepth::Sections, "Sections"),
                        (OutlineDepth::Subsections, "Subsections"),
                        (OutlineDepth::Subsubsections, "Subsubsections"),
                        (OutlineDepth::Paragraphs, "Paragraphs"),
                    ],
                    |s| s.outline_depth,
                    |s, v| s.outline_depth = v,
                ),
            ),
        ]),
    );
    let typesetting = SettingPage::new("Typesetting")
        .icon(icon("type"))
        .group(
            group(cx).title("Engine").items([
                SettingItem::new(
                    "rtex binary",
                    path("Automatic", false, |s| &s.rtex_path, |s, v| s.rtex_path = v),
                )
                .layout(Axis::Vertical)
                .keywords(["engine", "path"]),
                SettingItem::new(
                    "TeX folder",
                    path("Automatic", true, |s| &s.tex_bin, |s, v| s.tex_bin = v),
                )
                .layout(Axis::Vertical)
                .keywords(["lualatex", "path"]),
                SettingItem::new(
                    "Download rtex automatically",
                    switch(|s| s.download_rtex, |s, v| s.download_rtex = v),
                ),
            ]),
        )
        .group(
            group(cx).title("Performance").items([
                SettingItem::new(
                    "Paragraph time limit (ms)",
                    number(
                        (10.0, 1000.0, 10.0),
                        |s| s.fast_budget_ms,
                        |s, v| s.fast_budget_ms = v,
                    ),
                )
                .keywords(["budget", "live"]),
                SettingItem::new(
                    "Reuse drawn pictures",
                    switch(|s| s.picture_cache, |s, v| s.picture_cache = v),
                )
                .keywords(["tikz", "cache"]),
            ]),
        )
        .group(
            group(cx).title("Troubleshooting").items([
                SettingItem::new(
                    "Diagnostics folder",
                    path("None", true, |s| &s.debug_dir, |s, v| s.debug_dir = v),
                )
                .layout(Axis::Vertical)
                .keywords(["debug"]),
                SettingItem::new("Build cache", SettingField::render(clear_cache_button)),
            ]),
        );
    vec![
        general,
        appearance,
        colors_page(cx).icon(icon("swatch-book")),
        editor,
        files,
        typesetting,
        shortcuts_page(cx).icon(icon("keyboard")),
    ]
}

pub struct SettingsView;

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .child(Settings::new("settings").pages(pages(cx))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppSettings, FontField, OnLaunch, OutlineDepth, SettingsView, TexView, Zoom, choice, font,
        font_choices, group, pages,
    };
    use crate::ui::editor::MONO;
    use crate::ui::palette;
    use crate::ui::themes;
    use gpui_kit::base::Scrollbar;
    use gpui_kit::component::select::SelectEvent;
    use gpui_kit::component::setting::{
        AnySettingField, SelectIndex, SettingField, SettingItem, SettingPage, Settings,
    };
    use gpui_kit::component::{
        ActiveTheme, Theme, ThemeConfig, ThemeMode, ThemeRegistry, ThemeSet,
    };
    use gpui_kit::{
        App, AppContext, Axis, Context, IntoElement, Pixels, Render, Styled, TestAppContext,
        Window, canvas, px, size,
    };
    use std::cell::{Cell, RefCell};
    use std::collections::{BTreeMap, HashSet};
    use std::rc::Rc;

    thread_local! {
        /// The colour rows drawn, by key.
        pub(super) static COLOR_ROWS: RefCell<HashSet<String>> = RefCell::default();
    }

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

    /// The last version's file: its zoom a number; the settings added since at their defaults.
    #[test]
    fn an_old_settings_file_still_reads() {
        let old = r#"{
            "theme": "dark", "ui_font_size": 15.0, "dark_pages": true,
            "source_font_size": 13.0, "source_box_font": "", "default_zoom": 90.0,
            "live_preview": true, "line_numbers": true, "soft_wrap": true, "autosave": true,
            "autosave_delay": 2.0, "open_after_export": false, "fast_budget_ms": 50.0,
            "picture_cache": true, "debug_dir": "", "rtex_path": "",
            "tex_bin": "/Library/TeX/texbin", "ribbon_collapsed": false,
            "inspector_closed": false, "tour_seen": true, "shortcuts": {}
        }"#;
        let s: AppSettings = serde_json::from_str(old).unwrap();
        assert_eq!(s.default_zoom, Zoom::Percent(90.0));
        assert_eq!((s.theme.as_str(), s.autosave_delay), ("dark", 2.0));
        assert_eq!(
            (s.on_launch, s.recent_kept, s.outline_depth, s.open_tex_in),
            (
                OnLaunch::Reopen,
                30,
                OutlineDepth::Subsubsections,
                TexView::Visual
            )
        );
        assert!(s.cursor_blink && s.download_rtex && s.autocomplete && s.pinch_zoom);
        assert!(!s.show_hidden_files && s.hide_files.is_empty() && s.source_font.is_empty());
        assert!(s.accent.is_empty() && s.colors.is_empty());
        assert_eq!(
            (s.light_theme.as_str(), s.dark_theme.as_str()),
            (themes::DEFAULT_LIGHT, themes::DEFAULT_DARK)
        );
        // and what this version writes reads back the same
        let new = AppSettings {
            default_zoom: Zoom::FitWidth,
            on_launch: OnLaunch::Welcome,
            outline_depth: OutlineDepth::Paragraphs,
            accent: "#7C4DDB".into(),
            dark_theme: "Nord".into(),
            colors: BTreeMap::from([(
                "Nord".to_string(),
                BTreeMap::from([("syntax.keyword".to_string(), "#FF000080".to_string())]),
            )]),
            ..s
        };
        let json = serde_json::to_string(&new).unwrap();
        assert!(json.contains(r#""default_zoom":null"#), "{json}");
        assert!(json.contains(r#""on_launch":"welcome""#), "{json}");
        assert_eq!(serde_json::from_str::<AppSettings>(&json).unwrap(), new);
    }

    /// The accent colours the theme at launch and on each change, and a switch of mode, the
    /// system's too, keeps it; empty, the theme is theme.json's own.
    #[gpui_kit::test]
    fn the_accent_survives_a_switch_of_mode(cx: &mut TestAppContext) {
        let hex = |c| palette::hex(c);
        // the theme registry's observer in place, which puts back a theme it holds by name
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            cx.set_global(AppSettings {
                theme: "light".into(),
                accent: "#C93A85".into(),
                ..AppSettings::default()
            });
            // as AppSettings::init does
            themes::init(cx);
            AppSettings::apply(cx);
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(hex(cx.theme().primary), "#C93A85");
            Theme::change(ThemeMode::Dark, None, cx);
            assert!(cx.theme().mode.is_dark());
            assert_eq!(hex(cx.theme().primary), "#C93A85");
            assert_ne!(hex(cx.theme().ring), "#86A1FF");
            AppSettings::set(cx, |s| s.accent = "#1E7F47".into());
            assert_eq!(hex(cx.theme().primary), "#1E7F47");
            Theme::change(ThemeMode::Dark, None, cx);
            assert_eq!(hex(cx.theme().primary), "#1E7F47");
            // the theme's own: theme.json's light and dark blues
            AppSettings::set(cx, |s| s.accent = String::new());
            assert_eq!(hex(cx.theme().primary), "#3557D4");
            assert_eq!(hex(cx.theme().ring), "#3557D4");
            Theme::change(ThemeMode::Dark, None, cx);
            assert_eq!(hex(cx.theme().primary), "#4A68E8");
            assert_eq!(hex(cx.theme().ring), "#86A1FF");
            assert_eq!(hex(cx.theme().link), "#86A1FF");
            // a colour no one can read is the theme's own
            AppSettings::set(cx, |s| s.accent = "blue".into());
            assert_eq!(hex(cx.theme().primary), "#3557D4");
        });
        cx.run_until_parked();
    }

    /// The app as AppSettings::init leaves it, with `s`.
    fn started(s: AppSettings, cx: &mut TestAppContext) {
        // the theme registry's observer in place, which puts back a theme it holds by name
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            cx.set_global(s);
            themes::init(cx);
            AppSettings::apply(cx);
        });
        cx.run_until_parked();
    }

    /// theme.json's theme `name`, as the registry reads it.
    fn theme_json(name: &str) -> ThemeConfig {
        let set: ThemeSet = serde_json::from_str(include_str!("theme.json")).unwrap();
        set.themes.into_iter().find(|t| t.name == name).unwrap()
    }

    /// With nothing set, the colours on screen are theme.json's, in either mode.
    #[gpui_kit::test]
    fn no_colour_settings_is_theme_json(cx: &mut TestAppContext) {
        started(AppSettings::default(), cx);
        cx.update(|cx| {
            for (mode, name) in [
                (ThemeMode::Light, themes::DEFAULT_LIGHT),
                (ThemeMode::Dark, themes::DEFAULT_DARK),
            ] {
                Theme::change(mode, None, cx);
                let mut want = Theme::default();
                want.apply_config(&Rc::new(theme_json(name)));
                assert_eq!(cx.theme().colors, want.colors, "{name}");
            }
        });
    }

    fn set(key: &str, value: &str, cx: &mut App) {
        themes::set_color(key, Some(value.into()), cx);
    }

    /// A colour changed in the dark theme is on screen at once, and stays through every switch of
    /// mode and every change to the registry (a theme file added): the registry's observer puts
    /// back a theme it holds by name, so a colour set on the theme on screen alone would be lost.
    #[gpui_kit::test]
    fn a_changed_colour_survives_a_switch_of_mode(cx: &mut TestAppContext) {
        started(
            AppSettings {
                theme: "dark".into(),
                ..AppSettings::default()
            },
            cx,
        );
        let hex = |c| palette::hex(c);
        cx.update(|cx| {
            set("tab.active.background", "#123456", cx);
            assert_eq!(hex(cx.theme().tab_active), "#123456");
            // the accent is under it
            AppSettings::set(cx, |s| s.accent = "#C93A85".into());
            assert_eq!(hex(cx.theme().tab_active), "#123456");
            assert_eq!(hex(cx.theme().primary), "#C93A85");
            set("primary.background", "#00FF0080", cx);
            assert_eq!(hex(cx.theme().primary), "#00FF0080");
            Theme::change(ThemeMode::Light, None, cx);
            Theme::change(ThemeMode::Dark, None, cx);
            assert_eq!(hex(cx.theme().tab_active), "#123456");
            ThemeRegistry::global_mut(cx);
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(hex(cx.theme().tab_active), "#123456");
            assert_eq!(hex(cx.theme().primary), "#00FF0080");
            themes::set_color("tab.active.background", None, cx);
            assert_eq!(hex(cx.theme().tab_active), "#1D1E22");
        });
    }

    /// Colours are changed per theme: the dark theme's leave the light one as it is; another dark
    /// theme shows its own, and the first one's come back with it.
    #[gpui_kit::test]
    fn changed_colours_belong_to_their_theme(cx: &mut TestAppContext) {
        started(
            AppSettings {
                theme: "dark".into(),
                ..AppSettings::default()
            },
            cx,
        );
        let hex = |c| palette::hex(c);
        cx.update(|cx| {
            set("background", "#FF0000", cx);
            assert_eq!(hex(cx.theme().background), "#FF0000");
            assert!(AppSettings::get(cx).colors[themes::DEFAULT_DARK].contains_key("background"));
            AppSettings::set(cx, |s| s.theme = "light".into());
            assert_eq!(hex(cx.theme().background), "#FFFFFF");
            AppSettings::set(cx, |s| {
                s.theme = "dark".into();
                s.dark_theme = "Nord".into();
            });
            assert_eq!(hex(cx.theme().background), "#2E3440");
            assert_eq!(themes::active(cx).as_deref(), Some("Nord"));
            assert!(
                AppSettings::get(cx)
                    .colors
                    .contains_key(themes::DEFAULT_DARK)
            );
            AppSettings::set(cx, |s| s.dark_theme = themes::DEFAULT_DARK.into());
            assert_eq!(hex(cx.theme().background), "#FF0000");
            // a theme no longer there: the app's own
            AppSettings::set(cx, |s| s.dark_theme = "Gone".into());
            assert_eq!(themes::active(cx).as_deref(), Some(themes::DEFAULT_DARK));
        });
    }

    /// A syntax colour changed is the one the source editor's highlighter gives its tokens, the
    /// theme's weight and style kept.
    #[gpui_kit::test]
    fn a_changed_syntax_colour_reaches_the_highlighter(cx: &mut TestAppContext) {
        started(
            AppSettings {
                theme: "dark".into(),
                dark_theme: "Gruvbox Dark".into(),
                ..AppSettings::default()
            },
            cx,
        );
        cx.update(|cx| {
            let style = |name: &str, cx: &App| cx.theme().highlight_theme.style(name).unwrap();
            assert_eq!(palette::hex(style("keyword", cx).color.unwrap()), "#FB4934");
            set("syntax.keyword", "#00FF00", cx);
            set("syntax.function", "#0000FF", cx);
            assert_eq!(palette::hex(style("keyword", cx).color.unwrap()), "#00FF00");
            let function = style("function", cx);
            assert_eq!(palette::hex(function.color.unwrap()), "#0000FF");
            assert_eq!(function.font_weight, Some(gpui_kit::FontWeight::BOLD));
            // and what Settings shows
            assert_eq!(
                themes::shown(cx)["syntax.keyword"]
                    .map(palette::hex)
                    .as_deref(),
                Some("#00FF00")
            );
        });
    }

    /// A theme file in the themes folder is offered in its mode's list, after the app's own,
    /// which the registry drops when it reads the folder (its own reading starts a file watcher
    /// thread the test scheduler refuses, so the test reads the file as it does: a registry of
    /// the folder's themes alone).
    #[gpui_kit::test]
    fn a_theme_in_the_themes_folder_is_offered(cx: &mut TestAppContext) {
        started(AppSettings::default(), cx);
        let dir = std::env::temp_dir().join(format!("rtex-themes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = r##"{"name": "Mine", "themes": [
            {"name": "Midnight Mine", "mode": "dark", "colors": {"background": "#010203"}}
        ]}"##;
        std::fs::write(dir.join("mine.json"), file).unwrap();
        cx.update(|cx| {
            let mut read = ThemeRegistry::default();
            let json = std::fs::read_to_string(dir.join("mine.json")).unwrap();
            read.load_themes_from_str(&json).unwrap();
            cx.set_global(read);
        });
        cx.run_until_parked();
        cx.update(|cx| {
            let dark = themes::names(ThemeMode::Dark, cx);
            assert_eq!(dark[0], themes::DEFAULT_DARK, "{dark:?}");
            assert_eq!(
                dark.last().map(|n| n.as_ref()),
                Some("Midnight Mine"),
                "{dark:?}"
            );
            assert!(!dark.iter().any(|n| n == "Default Dark"), "{dark:?}");
            let light = themes::names(ThemeMode::Light, cx);
            assert_eq!(
                light,
                [
                    themes::DEFAULT_LIGHT,
                    "Solarized Light",
                    "Gruvbox Light",
                    "High Contrast Light"
                ]
            );
            AppSettings::set(cx, |s| {
                s.theme = "dark".into();
                s.dark_theme = "Midnight Mine".into();
            });
            assert_eq!(palette::hex(cx.theme().background), "#010203");
        });
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The Colors page draws a row for every colour of the theme schema.
    #[gpui_kit::test]
    fn the_colors_page_has_a_row_per_colour(cx: &mut TestAppContext) {
        started(AppSettings::default(), cx);
        // after General and Appearance
        let colors = 2;
        COLOR_ROWS.with(|r| r.borrow_mut().clear());
        let t = std::time::Instant::now();
        let (_, vcx) = cx.add_window_view(|_, _| OnPage(colors));
        // tall enough that the page's list draws every group
        vcx.simulate_resize(size(px(1000.0), px(200_000.0)));
        vcx.run_until_parked();
        eprintln!("the Colors page, every row, drew in {:?}", t.elapsed());
        let t = std::time::Instant::now();
        vcx.update(|window, _| window.refresh());
        vcx.run_until_parked();
        eprintln!("and again in {:?}", t.elapsed());
        let keys: HashSet<String> = (themes::groups().iter())
            .flat_map(|g| g.keys.iter().map(|k| k.key.clone()))
            .collect();
        COLOR_ROWS.with(|r| assert_eq!(*r.borrow(), keys));
    }

    #[test]
    fn the_outline_lists_headings_down_to_its_depth() {
        let o = crate::document::outline(
            "\\section{A}\n\\subsection{B}\n\\subsubsection{C}\n\\paragraph{D}\n",
        );
        let titles = |d: OutlineDepth| {
            o.iter()
                .filter(|h| d.shows(h.level))
                .map(|h| h.title.as_str())
                .collect::<String>()
        };
        assert_eq!(titles(OutlineDepth::Sections), "A");
        assert_eq!(titles(OutlineDepth::Subsections), "AB");
        assert_eq!(titles(OutlineDepth::Subsubsections), "ABC");
        assert_eq!(titles(OutlineDepth::Paragraphs), "ABCD");
    }

    /// The Settings window opened on page `0`.
    struct OnPage(usize);

    impl Render for OnPage {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            Settings::new("settings")
                .pages(pages(cx))
                .default_selected_index(SelectIndex {
                    page_ix: self.0,
                    group_ix: None,
                })
        }
    }

    /// The window, and each of its pages, draws (the test text system has no fonts: nothing
    /// about how it looks is checked).
    #[gpui_kit::test]
    fn every_page_draws(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        cx.add_window_view(|_, _| SettingsView).1.run_until_parked();
        let n = cx.update(|cx| pages(cx).len());
        assert_eq!(n, 7);
        for i in 0..n {
            cx.add_window_view(|_, _| OnPage(i)).1.run_until_parked();
        }
    }

    /// A page's reset puts a dropdown's and a text field's settings back to their defaults.
    #[gpui_kit::test]
    fn reset_brings_back_the_defaults(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        let zoom = choice(
            [
                (Zoom::FitWidth, "Fit width"),
                (Zoom::Percent(100.0), "100%"),
            ],
            |s| s.default_zoom,
            |s, v| s.default_zoom = v,
        );
        let font = font(
            MONO,
            Some(MONO),
            |s| &s.source_font,
            |s, v| s.source_font = v,
        );
        let (_, cx) = cx.add_window_view(|_, _| SettingsView);
        cx.update(|window, cx| {
            assert!(!zoom.is_resettable(cx) && !font.is_resettable(cx));
            AppSettings::set(cx, |s| {
                s.default_zoom = Zoom::FitWidth;
                s.source_font = "Fira Code".into();
            });
            assert!(zoom.is_resettable(cx) && font.is_resettable(cx));
            zoom.reset(window, cx);
            font.reset(window, cx);
            let s = AppSettings::get(cx);
            assert_eq!(s.default_zoom, Zoom::Percent(100.0));
            assert_eq!(s.source_font, "");
        });
    }

    /// A font list: the default first (meaning empty), the installed families without it, and
    /// a font no longer installed kept last.
    #[test]
    fn a_font_list_starts_with_its_default() {
        let names = ["Fira Code", "Menlo", "Monaco"].map(String::from);
        let list = font_choices("Menlo", Some("Menlo"), &names, "Gone Mono");
        let got: Vec<(&str, &str)> = list
            .iter()
            .map(|c| (c.name.as_ref(), c.label.as_ref()))
            .collect();
        assert_eq!(
            got,
            [
                ("", "Menlo"),
                ("Fira Code", "Fira Code"),
                ("Monaco", "Monaco"),
                ("Gone Mono", "Gone Mono"),
            ]
        );
        assert_eq!(list[0].face.as_deref(), Some("Menlo"));
        assert_eq!(list[1].face.as_deref(), Some("Fira Code"));
        let doc = font_choices("Document font", None, &names, "Monaco");
        assert_eq!(
            (doc[0].name.as_ref(), doc[0].label.as_ref()),
            ("", "Document font")
        );
        assert!(doc[0].face.is_none());
        assert_eq!(doc.len(), 4);
    }

    /// A font picked in the list is the setting; the first item empties it.
    #[gpui_kit::test]
    fn picking_a_font_sets_it(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        let (_, cx) = cx.add_window_view(|_, _| SettingsView);
        let field = cx.update(|window, cx| {
            cx.new(|cx| {
                FontField::new(
                    MONO,
                    Some(MONO),
                    |s| &s.source_font,
                    |s, v| s.source_font = v,
                    window,
                    cx,
                )
            })
        });
        let select = cx.update(|_, cx| field.read(cx).select.clone());
        for name in ["Fira Code", ""] {
            select.update(cx, |_, cx| cx.emit(SelectEvent::Confirm(Some(name.into()))));
            cx.run_until_parked();
            cx.update(|_, cx| assert_eq!(AppSettings::get(cx).source_font, name));
        }
    }

    /// A page with one full-width field, which notes where its right edge is drawn, at an
    /// interface font size.
    struct Probe(Rc<Cell<Pixels>>, f32);

    impl Render for Probe {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            // the app's Root sets it from the theme
            window.set_rem_size(px(self.1));
            let edge = self.0.clone();
            let field = SettingField::render(move |_, _, _| {
                let edge = edge.clone();
                canvas(move |b, _, _| edge.set(b.right()), |_, _, _, _| {})
                    .w_full()
                    .h(px(8.0))
            });
            let item = SettingItem::new("Field", field).layout(Axis::Vertical);
            Settings::new("settings")
                .pages(vec![SettingPage::new("Page").group(group(cx).item(item))])
        }
    }

    /// The focus ring (3 px) of a control on a group's right edge stays clear of the
    /// scrollbar's track, which covers the list's right edge while it shows.
    #[gpui_kit::test]
    fn focus_rings_clear_the_scrollbar(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        for size in [10.0, 16.0, 24.0] {
            cx.update(|cx| AppSettings::set(cx, |s| s.ui_font_size = size));
            let edge = Rc::new(Cell::new(px(0.0)));
            let (_, vcx) = cx.add_window_view(|_, _| Probe(edge.clone(), size as f32));
            vcx.run_until_parked();
            let track = vcx.update(|window, _| window.viewport_size().width) - Scrollbar::width();
            assert!(edge.get() > px(0.0), "the field was drawn");
            assert!(
                edge.get() + px(3.0) < track,
                "at {size} px the ring reaches {:?}, the track starts at {track:?}",
                edge.get() + px(3.0)
            );
        }
    }
}
