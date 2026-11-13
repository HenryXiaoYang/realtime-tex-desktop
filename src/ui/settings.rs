// App settings: kept as a GPUI global, saved as JSON in
// <app data>/Realtime Tex Desktop/settings.json (~/Library/Application Support on the Mac), and edited in the Settings
// window (⌘,), built from gpui-kit's settings components.
use crate::ui::Tip;
use crate::ui::keymap;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{CursorBlink, Input, InputEvent, InputState};
use gpui_kit::component::setting::{
    NumberFieldOptions, RenderOptions, SettingField, SettingGroup, SettingItem, SettingPage,
    Settings,
};
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable, Theme, ThemeMode, ThemeRegistry, TitleBar,
};
use gpui_kit::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    /// "system", "light" or "dark".
    pub theme: String,
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
    pub shortcuts: std::collections::BTreeMap<String, String>,
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
        install_theme(cx);
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

struct TextField {
    input: Entity<InputState>,
    _sub: Subscription,
}

/// A text field whose placeholder says what empty means; the setting follows each keystroke,
/// trimmed.
fn text(
    placeholder: &'static str,
    get: fn(&AppSettings) -> &String,
    set: fn(&mut AppSettings, String),
) -> SettingField<SharedString> {
    SettingField::render(move |options: &RenderOptions, window, cx| {
        let key = format!(
            "text-{}-{}-{}",
            options.page_ix(),
            options.group_ix(),
            options.item_ix()
        );
        let state = window.use_keyed_state(SharedString::from(key), cx, |window, cx| {
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
        // changed elsewhere (the page's reset): shown; a space being typed is kept
        let value = get(AppSettings::get(cx)).clone();
        if input.read(cx).value().trim() != value {
            input.update(cx, |s, cx| s.set_value(value, window, cx));
        }
        let field = Input::new(&input)
            .disabled(options.is_disabled())
            .with_size(options.size());
        if options.layout() == Axis::Horizontal {
            field.w_64()
        } else {
            field.w_full()
        }
    })
    .on_reset(
        move |cx| get(AppSettings::get(cx)) != get(&AppSettings::default()),
        move |_, cx| AppSettings::set(cx, |s| set(s, get(&AppSettings::default()).clone())),
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
        .group(SettingGroup::new().title("Startup").items([
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
            SettingGroup::new().title("Saving").items([
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
        .group(SettingGroup::new().title("Export").item(SettingItem::new(
            "Open the PDF after export",
            switch(|s| s.open_after_export, |s, v| s.open_after_export = v),
        )));
    let appearance = SettingPage::new("Appearance")
        .icon(icon("palette"))
        .group(
            SettingGroup::new().title("Window").items([
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
            SettingGroup::new().title("Layout").items([
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
            SettingGroup::new().title("Pages").items([
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
            SettingGroup::new().title("Source").items([
                SettingItem::new(
                    "Source font",
                    text("Menlo", |s| &s.source_font, |s, v| s.source_font = v),
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
                    text(
                        "Document font",
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
        .group(SettingGroup::new().title("Find").item(SettingItem::new(
            "Match case by default",
            switch(|s| s.find_match_case, |s, v| s.find_match_case = v),
        )));
    let files = SettingPage::new("Files").icon(icon("folder")).group(
        SettingGroup::new().title("Project panel").items([
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
            SettingGroup::new().title("Engine").items([
                SettingItem::new(
                    "rtex binary",
                    text("Automatic", |s| &s.rtex_path, |s, v| s.rtex_path = v),
                )
                .layout(Axis::Vertical)
                .keywords(["engine", "path"]),
                SettingItem::new(
                    "TeX folder",
                    text("Automatic", |s| &s.tex_bin, |s, v| s.tex_bin = v),
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
            SettingGroup::new().title("Performance").items([
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
            SettingGroup::new().title("Troubleshooting").items([
                SettingItem::new(
                    "Diagnostics folder",
                    text("None", |s| &s.debug_dir, |s, v| s.debug_dir = v),
                )
                .layout(Axis::Vertical)
                .keywords(["debug"]),
                SettingItem::new("Build cache", SettingField::render(clear_cache_button)),
            ]),
        );
    vec![
        general,
        appearance,
        editor,
        files,
        typesetting,
        shortcuts_page().icon(icon("keyboard")),
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
        AppSettings, OnLaunch, OutlineDepth, SettingsView, TexView, Zoom, choice, pages, text,
    };
    use gpui_kit::component::setting::{AnySettingField, SelectIndex, Settings};
    use gpui_kit::{Context, IntoElement, Render, TestAppContext, Window};

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
        // and what this version writes reads back the same
        let new = AppSettings {
            default_zoom: Zoom::FitWidth,
            on_launch: OnLaunch::Welcome,
            outline_depth: OutlineDepth::Paragraphs,
            ..s
        };
        let json = serde_json::to_string(&new).unwrap();
        assert!(json.contains(r#""default_zoom":null"#), "{json}");
        assert!(json.contains(r#""on_launch":"welcome""#), "{json}");
        assert_eq!(serde_json::from_str::<AppSettings>(&json).unwrap(), new);
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
        assert_eq!(n, 6);
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
        let font = text("Menlo", |s| &s.source_font, |s, v| s.source_font = v);
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
}
