// The themes: the app's own (theme.json and themes/), those in the themes folder, and over the one
// chosen for each mode its accent and the colours changed in Settings › Appearance, kept per theme
// as VS Code keeps its colour customizations. Every colour a theme can name is read off the kit's
// theme schema, so the colours a newer kit adds are listed without a change here; with them the
// app's own, those the editor draws on the pages, which a theme file names among its colours too.
use crate::ui::palette;
use crate::ui::settings::{AppSettings, support_dir};
use gpui_kit::component::highlighter::{HighlightTheme, HighlightThemeStyle, SyntaxColors};
use gpui_kit::component::{
    ActiveTheme, Theme, ThemeConfig, ThemeConfigColors, ThemeMode, ThemeRegistry, ThemeSet,
};
use gpui_kit::{App, Global, Hsla, SharedString};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::LazyLock;

pub const DEFAULT_LIGHT: &str = "Realtime Tex Light";
pub const DEFAULT_DARK: &str = "Realtime Tex Dark";

/// The app's themes, in the order the theme lists show them: its own, then the others by name
/// (those of VS Code made by scripts/themes/vscode_to_app.py); the themes folder's come after.
const BUILT_IN: [&str; 11] = [
    include_str!("theme.json"),
    include_str!("themes/catppuccin.json"),
    include_str!("themes/dracula.json"),
    include_str!("themes/github.json"),
    include_str!("themes/gruvbox.json"),
    include_str!("themes/high-contrast.json"),
    include_str!("themes/night-owl.json"),
    include_str!("themes/nord.json"),
    include_str!("themes/onedark-pro.json"),
    include_str!("themes/solarized.json"),
    include_str!("themes/tokyo-night.json"),
];

static BUILT_IN_NAMES: LazyLock<Vec<SharedString>> = LazyLock::new(|| {
    BUILT_IN
        .iter()
        .filter_map(|json| serde_json::from_str::<ThemeSet>(json).ok())
        .flat_map(|set| set.themes)
        .map(|t| t.name)
        .collect()
});

/// The themes folder: a theme file put there is in the theme lists at once.
pub fn dir() -> PathBuf {
    support_dir().join("themes")
}

fn add_built_in(cx: &mut App) {
    let registry = ThemeRegistry::global_mut(cx);
    for json in BUILT_IN {
        if let Err(e) = registry.load_themes_from_str(json) {
            eprintln!("a built-in theme: {e}");
        }
    }
}

/// The app's themes in the registry; each change to it (the themes folder read again) installs
/// the chosen themes again.
pub fn init(cx: &mut App) {
    add_built_in(cx);
    cx.set_global(page_colors());
    cx.observe_global::<ThemeRegistry>(|cx| {
        // reading the themes folder drops every theme not read from it
        if !ThemeRegistry::global(cx)
            .themes()
            .contains_key(DEFAULT_LIGHT)
        {
            add_built_in(cx);
        }
        // the kit drops the colours it does not know: the pages' are read here
        cx.set_global(page_colors());
        install(cx);
        Theme::change(cx.theme().mode, None, cx);
    })
    .detach();
}

/// The theme chosen for `mode`; the app's own when that one is gone (its file removed).
pub fn chosen(s: &AppSettings, mode: ThemeMode, cx: &App) -> Option<Rc<ThemeConfig>> {
    let (name, default) = if mode.is_dark() {
        (&s.dark_theme, DEFAULT_DARK)
    } else {
        (&s.light_theme, DEFAULT_LIGHT)
    };
    let themes = ThemeRegistry::global(cx).themes();
    (themes.get(name.as_str()).filter(|t| t.mode == mode))
        .or(themes.get(default))
        .cloned()
}

/// The theme on screen, under whose name a colour changed now is kept.
pub fn active(cx: &App) -> Option<SharedString> {
    chosen(AppSettings::get(cx), cx.theme().mode, cx).map(|t| t.name.clone())
}

/// The themes of `mode` a theme list offers: the app's first, then the themes folder's by name
/// (the kit's own fallbacks left out).
pub fn names(mode: ThemeMode, cx: &App) -> Vec<SharedString> {
    let registry = ThemeRegistry::global(cx);
    let kit: Vec<&SharedString> = registry
        .default_themes()
        .values()
        .map(|t| &t.name)
        .collect();
    let mut list: Vec<&SharedString> = (registry.themes().values())
        .filter(|t| t.mode == mode && !kit.contains(&&t.name))
        .map(|t| &t.name)
        .collect();
    let rank = |n: &SharedString| BUILT_IN_NAMES.iter().position(|b| b == n);
    list.sort_by_key(|n| (rank(n).unwrap_or(usize::MAX), n.to_lowercase()));
    list.into_iter().cloned().collect()
}

/// `base` with the accent (when there is one) and then `changed` (colour key to "#RRGGBB" or
/// "#RRGGBBAA") over it. Its name is its own: the registry's observer puts a theme it holds back
/// by name at every change to the registry.
pub fn effective(
    base: &ThemeConfig,
    accent: Option<Hsla>,
    changed: Option<&BTreeMap<String, String>>,
) -> ThemeConfig {
    let mut t = match accent {
        Some(a) => palette::accented(base, a),
        None => base.clone(),
    };
    t.name = format!("{} (applied)", base.name).into();
    // a theme with no syntax colours takes the kit's for its mode, not those of the theme before it
    let kit = if t.mode.is_dark() {
        HighlightTheme::default_dark()
    } else {
        HighlightTheme::default_light()
    };
    t.highlight.get_or_insert_with(|| kit.style.clone());
    if let Some(changed) = changed {
        change(&mut t, changed);
    }
    t
}

/// The key a theme's accent is kept under among its colours, in the settings file and in a theme
/// file alike; no colour of the kit's theme has it.
pub const ACCENT: &str = "accent";

/// The colours the editor draws on the pages, the app's own: each key and its name in Settings.
/// Unchanged, the editor makes them from the theme's (palette::marks).
pub const PAGES: [(&str, &str); 8] = [
    ("pages.selection", "Selected text"),
    ("pages.find.current", "Find match"),
    ("pages.find.other", "Other find matches"),
    ("pages.accent", "Selected block and insert line"),
    ("pages.source_box.background", "Source box background"),
    ("pages.source_box.foreground", "Source box text"),
    ("pages.paper", "Dark page paper"),
    ("pages.ink", "Dark page text"),
];

/// The page colours and accent each theme names, the app's and the themes folder's (theme name to
/// key to colour).
struct PageColors(HashMap<String, BTreeMap<String, String>>);

impl Global for PageColors {}

/// The page colours of the themes folder's themes, then of the app's: as in the registry, a file
/// in the folder wins over a theme of the app's of the same name.
fn page_colors() -> PageColors {
    let built_in = BUILT_IN.iter().map(|json| json.to_string());
    PageColors(page_colors_in(
        theme_files(&dir()).into_iter().chain(built_in),
    ))
}

/// The text of each theme file in `dir`.
fn theme_files(dir: &Path) -> Vec<String> {
    let files = std::fs::read_dir(dir).into_iter().flatten().flatten();
    (files.map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .collect()
}

/// The page colours and accents the theme files `sets` name; a theme named twice, the first one
/// read.
fn page_colors_in(
    sets: impl IntoIterator<Item = String>,
) -> HashMap<String, BTreeMap<String, String>> {
    let mut found = HashMap::new();
    for json in sets {
        let Ok(set) = serde_json::from_str::<Value>(&json) else {
            continue;
        };
        for t in set["themes"].as_array().into_iter().flatten() {
            let (Some(name), Some(colors)) = (t["name"].as_str(), t["colors"].as_object()) else {
                continue;
            };
            let pages: BTreeMap<String, String> = (colors.iter())
                .filter(|(k, _)| matches!(Place::of(k), Place::Pages) || *k == ACCENT)
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                .collect();
            found.entry(name.to_string()).or_insert(pages);
        }
    }
    found
}

/// The page colour (or the accent) `key` of the theme on screen: as changed in Settings, else as
/// its theme file names it; None: the editor makes it (the theme's own).
pub fn page_color(key: &str, cx: &App) -> Option<Hsla> {
    let name = active(cx)?;
    let changed = AppSettings::get(cx).colors.get(name.as_ref());
    let own = cx
        .try_global::<PageColors>()
        .and_then(|p| p.0.get(name.as_ref()));
    [changed, own]
        .into_iter()
        .flatten()
        .find_map(|c| palette::hex_rgba(c.get(key)?))
}

/// Where in a theme a colour key's colour is.
enum Place<'a> {
    /// The interface's colours: `colors`.
    Ui(&'a str),
    /// The source editor's: `highlight`.
    Highlight(&'a str),
    /// A syntax style's colour: `highlight.syntax`.
    Syntax(&'a str),
    /// The pages': the app's own, in the theme file's `colors` but not the kit's theme.
    Pages,
}

impl<'a> Place<'a> {
    fn of(key: &'a str) -> Self {
        if key.starts_with("pages.") {
            Place::Pages
        } else if let Some(k) = key.strip_prefix("syntax.") {
            Place::Syntax(k)
        } else if let Some(k) = key.strip_prefix("highlight.") {
            Place::Highlight(k)
        } else {
            Place::Ui(key)
        }
    }
}

/// `changed` written over `t`'s colours, through the theme file's own keys.
fn change(t: &mut ThemeConfig, changed: &BTreeMap<String, String>) {
    let (Ok(mut colors), Ok(mut highlight)) = (
        serde_json::to_value(&t.colors),
        serde_json::to_value(&t.highlight),
    ) else {
        return;
    };
    for (key, value) in changed {
        // the settings file can be edited by hand: a colour no one can read, or a key no theme
        // has, is left out
        if palette::hex_rgba(value).is_none() {
            continue;
        }
        let slot = match Place::of(key) {
            Place::Ui(k) => colors.get_mut(k),
            Place::Highlight(k) => highlight.get_mut(k),
            Place::Syntax(k) => (highlight.get_mut("syntax").and_then(|s| s.get_mut(k))).map(|s| {
                if !s.is_object() {
                    *s = Value::Object(Default::default());
                }
                &mut s["color"]
            }),
            Place::Pages => None,
        };
        if let Some(slot) = slot {
            *slot = Value::String(value.clone());
        }
    }
    if let Ok(c) = serde_json::from_value(colors) {
        t.colors = c;
    }
    if let Ok(h) = serde_json::from_value(highlight) {
        t.highlight = h;
    }
}

/// The accent changed in Settings among `changed`, a theme's changed colours.
fn accent_in(changed: Option<&BTreeMap<String, String>>) -> Option<Hsla> {
    palette::hex_color(changed?.get(ACCENT)?)
}

/// Each mode's chosen theme, with its accent and changed colours, in the theme's slots, where
/// every switch of mode (the system's too) takes it.
pub fn install(cx: &mut App) {
    let s = AppSettings::get(cx);
    let made = |mode| {
        chosen(s, mode, cx).map(|b| {
            let changed = s.colors.get(b.name.as_ref());
            Rc::new(effective(&b, accent_in(changed), changed))
        })
    };
    let (Some(light), Some(dark)) = (made(ThemeMode::Light), made(ThemeMode::Dark)) else {
        return;
    };
    Theme::update(cx, |t| {
        t.light_theme = light;
        t.dark_theme = dark;
    });
}

/// The theme on screen, its accent and changed colours in it, as a theme file of its own in the
/// themes folder ("<name> (custom).json"): there it is in the theme lists, and ready to share.
pub fn export(cx: &App) -> std::io::Result<PathBuf> {
    let s = AppSettings::get(cx);
    let base = chosen(s, cx.theme().mode, cx).ok_or(std::io::ErrorKind::NotFound)?;
    let name = match base.name.strip_suffix(" (custom)") {
        Some(_) => base.name.to_string(),
        None => format!("{} (custom)", base.name),
    };
    let changed = s.colors.get(base.name.as_ref());
    let theme = effective(&base, accent_in(changed), changed);
    let dir = dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", name.replace(['/', '\\', ':'], "-")));
    // the accent too, so the theme read back shows it as its own
    let pages = (PAGES.iter().map(|(k, _)| *k).chain([ACCENT]))
        .filter_map(|k| Some((k.to_string(), palette::hex(page_color(k, cx)?))))
        .collect();
    std::fs::write(&path, theme_file(&theme, &name, &pages))?;
    Ok(path)
}

/// `theme` as a theme file, named `name`, with the page colours and accent `pages` among its colours, without
/// the settings it leaves unset.
fn theme_file(theme: &ThemeConfig, name: &str, pages: &BTreeMap<String, String>) -> String {
    let mut t = theme.clone();
    t.name = name.to_string().into();
    t.is_default = false;
    let mut set = ThemeSet::default();
    set.name = t.name.clone();
    set.themes = vec![t];
    let mut json = serde_json::to_value(set).unwrap_or_default();
    strip_nulls(&mut json);
    if let Some(colors) = (json.pointer_mut("/themes/0/colors")).and_then(Value::as_object_mut) {
        colors.extend(
            pages
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone()))),
        );
    }
    serde_json::to_string_pretty(&json).unwrap_or_default()
}

fn strip_nulls(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.retain(|_, v| !v.is_null());
            m.values_mut().for_each(strip_nulls);
        }
        Value::Array(a) => a.iter_mut().for_each(strip_nulls),
        _ => {}
    }
}

/// A colour a theme can name: its key (as theme files and the settings file write it) and the
/// name Settings shows.
pub struct ColorKey {
    pub key: String,
    pub label: String,
}

pub struct ColorGroup {
    pub title: &'static str,
    pub keys: Vec<ColorKey>,
}

/// The colour groups of Settings › Appearance, each by the starts of its keys; a key has exactly
/// one start.
const GROUPS: [(&str, &[&str]); 15] = [
    ("Pages", &["pages."]),
    (
        "Window colors",
        &[
            "background",
            "foreground",
            "border",
            "muted.",
            "window.",
            "group_box.",
            "accordion.",
            "overlay",
        ],
    ),
    ("Title & status bars", &["title_bar.", "status_bar."]),
    ("Tabs", &["tab.", "tab_bar."]),
    ("Sidebar", &["sidebar."]),
    ("Lists & tables", &["list.", "table.", "description_list."]),
    ("Buttons", &["button.", "primary.", "secondary."]),
    (
        "Inputs & controls",
        &[
            "input.",
            "ring",
            "caret",
            "selection.",
            "link",
            "switch.",
            "slider.",
            "progress.",
            "skeleton.",
        ],
    ),
    (
        "Menus & popovers",
        &["popover.", "accent.", "drag.", "drop_target."],
    ),
    ("Scrollbars", &["scrollbar."]),
    (
        "Status colors",
        &["danger.", "warning.", "info.", "success."],
    ),
    ("Base hues", &["base."]),
    ("Charts", &["chart."]),
    ("Source editor", &["highlight."]),
    ("Source syntax", &["syntax."]),
];

/// The groups whose starts `key` has (one, unless GROUPS is wrong).
fn groups_of(key: &str) -> impl Iterator<Item = &'static str> + '_ {
    GROUPS
        .iter()
        .filter(move |(_, starts)| starts.iter().any(|s| key.starts_with(s)))
        .map(|(title, _)| *title)
}

/// The keys of a serialized schema struct.
fn keys_of(v: serde_json::Result<Value>) -> Vec<String> {
    match v {
        Ok(Value::Object(m)) => m.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

/// Every colour key a theme can name: the interface's (those the kit applies), the source
/// editor's, the syntax's, the pages'.
fn all_keys() -> Vec<String> {
    let ui = keys_of(serde_json::to_value(ThemeConfigColors::default()))
        .into_iter()
        .filter(|k| FIELD.contains_key(k));
    let editor = keys_of(serde_json::to_value(HighlightThemeStyle::default()))
        .into_iter()
        .filter(|k| k != "syntax")
        .map(|k| format!("highlight.{k}"));
    let syntax = keys_of(serde_json::to_value(SyntaxColors::default()))
        .into_iter()
        .map(|k| format!("syntax.{k}"));
    let pages = PAGES.iter().map(|(k, _)| k.to_string());
    ui.into_iter()
        .chain(editor)
        .chain(syntax)
        .chain(pages)
        .collect()
}

/// A key's name in Settings: its words, the group's own prefix left out, "foreground" as "text".
fn label(key: &str) -> String {
    if let Some((_, named)) = PAGES.iter().find(|(k, _)| *k == key) {
        return named.to_string();
    }
    let named = match key {
        "ring" => "Focus ring",
        "overlay" => "Behind dialogs",
        "accent.background" => "Hover background",
        "accent.foreground" => "Hover text",
        "selection.background" => "Selection",
        _ => "",
    };
    if !named.is_empty() {
        return named.into();
    }
    let rest = [
        "syntax.",
        "highlight.editor_",
        "highlight.editor.",
        "highlight.",
        "base.",
    ]
    .iter()
    .find_map(|p| key.strip_prefix(p))
    .unwrap_or(key);
    let mut words: Vec<&str> = rest
        .split(['.', '_'])
        .map(|w| match w {
            "foreground" => "text",
            "uri" => "URI",
            w => w,
        })
        .collect();
    // "base.blue.light": light blue
    if words.len() > 1 && words.last() == Some(&"light") {
        words.rotate_right(1);
    }
    let text = words.join(" ");
    let mut c = text.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

static GROUPED: LazyLock<Vec<ColorGroup>> = LazyLock::new(|| {
    let keys = all_keys();
    GROUPS
        .iter()
        .map(|(title, _)| ColorGroup {
            title,
            keys: (keys.iter())
                .filter(|k| groups_of(k).next() == Some(*title))
                .map(|k| ColorKey {
                    key: k.clone(),
                    label: label(k),
                })
                .collect(),
        })
        .collect()
});

/// The colour groups of Settings › Appearance, in order, each with its keys.
pub fn groups() -> &'static [ColorGroup] {
    &GROUPED
}

/// The theme's colour (`Theme`'s field) each interface key sets: found by applying a theme that
/// names each key a colour of its own.
static FIELD: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    let keys = keys_of(serde_json::to_value(ThemeConfigColors::default()));
    // faint: the kit caps the opacity of selections and picked rows
    let mark = |i: usize| format!("#31{i:02x}9720");
    let colors: serde_json::Map<String, Value> = (keys.iter().enumerate())
        .map(|(i, k)| (k.clone(), Value::String(mark(i))))
        .collect();
    let mut config = ThemeConfig::default();
    config.colors = serde_json::from_value(Value::Object(colors)).unwrap_or_default();
    let mut theme = Theme::default();
    theme.apply_config(&Rc::new(config));
    let Ok(Value::Object(fields)) = serde_json::to_value(theme.colors) else {
        return HashMap::new();
    };
    (keys.iter().enumerate())
        .filter_map(|(i, k)| {
            let field = fields.iter().find(|(_, v)| v.as_str() == Some(&mark(i)))?;
            Some((k.clone(), field.0.clone()))
        })
        .collect()
});

/// Each colour key's colour on screen; None for a source colour the theme leaves to the editor.
pub fn shown(cx: &App) -> HashMap<String, Option<Hsla>> {
    let theme = cx.theme();
    let colors = serde_json::to_value(theme.colors).unwrap_or_default();
    let highlight = serde_json::to_value(&theme.highlight_theme.style).unwrap_or_default();
    let syntax = &highlight["syntax"];
    // as the editor draws them now; the dark pages' paper and ink while pages are white too
    let dark = palette::dark_pages(cx);
    let marks = palette::marks(dark, cx);
    let (box_bg, box_fg) = palette::source_box(dark, cx);
    let page = |key: &str| match key {
        "pages.selection" => Some(marks.text_selection),
        "pages.find.current" => Some(marks.match_current),
        "pages.find.other" => Some(marks.match_other),
        "pages.accent" => Some(marks.accent),
        "pages.source_box.background" => Some(box_bg),
        "pages.source_box.foreground" => Some(box_fg),
        "pages.paper" => Some(palette::paper(true, cx)),
        "pages.ink" => Some(palette::ink(true, cx)),
        _ => None,
    };
    (groups().iter().flat_map(|g| &g.keys))
        .map(|k| {
            let v = match Place::of(&k.key) {
                Place::Ui(key) => FIELD.get(key).map(|f| &colors[f]),
                Place::Highlight(key) => highlight.get(key),
                Place::Syntax(key) => syntax.get(key).map(|s| &s["color"]),
                Place::Pages => return (k.key.clone(), page(&k.key)),
            };
            let color = v.and_then(|v| serde_json::from_value(v.clone()).ok());
            (k.key.clone(), color)
        })
        .collect()
}

/// The colour `key` of the theme on screen set to `value` ("#RRGGBB[AA]"), or, None, back to
/// the theme's own.
pub fn set_color(key: &str, value: Option<String>, cx: &mut App) {
    let Some(name) = active(cx) else {
        return;
    };
    let now = AppSettings::get(cx).colors.get(name.as_ref());
    if now.and_then(|c| c.get(key)) == value.as_ref() {
        return;
    }
    AppSettings::set(cx, |s| {
        let colors = s.colors.entry(name.to_string()).or_default();
        match value {
            Some(v) => colors.insert(key.to_string(), v),
            None => colors.remove(key),
        };
        if colors.is_empty() {
            s.colors.remove(name.as_ref());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The kit's theme schema, every colour of it, each in one colour group of Settings, and
    /// each interface colour found on the theme: a newer kit's colours cannot go missing unseen.
    #[test]
    fn every_colour_of_the_schema_is_in_one_group() {
        let keys = all_keys();
        assert!(keys.len() > 150, "{}", keys.len());
        // the one the kit has in its schema but never applies
        let ui = keys_of(serde_json::to_value(ThemeConfigColors::default()));
        let unused: Vec<_> = ui.iter().filter(|k| !keys.contains(k)).collect();
        assert_eq!(unused, ["group_box.title.foreground"]);
        for k in &keys {
            let g: Vec<_> = groups_of(k).collect();
            assert_eq!(g.len(), 1, "{k} is in {g:?}");
        }
        let listed: usize = groups().iter().map(|g| g.keys.len()).sum();
        assert_eq!(listed, keys.len());
        assert!(groups().iter().all(|g| !g.keys.is_empty()));
        // a few names, as Settings shows them
        assert_eq!(label("tab.active.foreground"), "Tab active text");
        assert_eq!(label("base.blue.light"), "Light blue");
        assert_eq!(label("highlight.editor_active_line"), "Active line");
        assert_eq!(
            label("highlight.editor.gutter.background"),
            "Gutter background"
        );
        assert_eq!(label("syntax.keyword"), "Keyword");
        assert_eq!(label("ring"), "Focus ring");
    }

    /// Every built-in theme reads, and every colour in it is one the schema has (a key the kit
    /// does not read is dropped without a word), written as a colour.
    #[test]
    fn the_built_in_themes_read() {
        let keys = all_keys();
        for json in BUILT_IN {
            assert!(serde_json::from_str::<ThemeSet>(json).is_ok());
            let set: Value = serde_json::from_str(json).unwrap();
            for t in set["themes"].as_array().unwrap() {
                let name = &t["name"];
                let mut named: Vec<(String, &Value)> = Vec::new();
                for (k, v) in t["colors"].as_object().unwrap() {
                    named.push((k.clone(), v));
                }
                for (k, v) in t["highlight"].as_object().into_iter().flatten() {
                    match v.as_object() {
                        Some(syntax) if k == "syntax" => {
                            for (k, style) in syntax {
                                named.push((format!("syntax.{k}"), &style["color"]));
                            }
                        }
                        _ => named.push((format!("highlight.{k}"), v)),
                    }
                }
                for (k, v) in named {
                    assert!(keys.contains(&k) || k == ACCENT, "{name}: {k}");
                    assert!(palette::hex_rgba(v.as_str().unwrap()).is_some(), "{k}: {v}");
                }
            }
        }
        assert_eq!(BUILT_IN_NAMES.len(), 21);
        assert_eq!(BUILT_IN_NAMES[..2], [DEFAULT_LIGHT, DEFAULT_DARK]);
    }

    /// Every built-in theme reads on screen: text 4.5:1 on what it is drawn on (comments, text
    /// meant to recede, 3:1), a primary button's text 4.5:1, the focus ring, the caret and the
    /// pages' accent 3:1 on what they mark. Solarized, Gruvbox and Nord were made by hand before
    /// these floors and keep their palettes as designed, some accents near 3:1.
    #[test]
    fn the_built_in_themes_read_on_screen() {
        let white = palette::hex_color("#FFFFFF").unwrap();
        let mut short = Vec::new();
        for json in BUILT_IN {
            let set: ThemeSet = serde_json::from_str(json).unwrap();
            if ["Solarized", "Gruvbox", "Nord"].contains(&set.name.as_ref()) {
                continue;
            }
            for config in &set.themes {
                let t = applied(&effective(config, None, None));
                let style = config.highlight.clone().unwrap_or_default();
                let editor = style.editor_background.unwrap_or(t.background);
                let mut floor = |what: String, fg: Hsla, bg: Hsla, ratio: f32| {
                    let r = palette::contrast(fg, bg);
                    if r < ratio {
                        let (fg, bg) = (palette::hex(fg), palette::hex(bg));
                        short.push(format!("{}: {what} {fg} on {bg} {r:.2}:1", config.name));
                    }
                };
                for (what, bg) in [
                    ("background", t.background),
                    ("title bar", t.title_bar),
                    ("status bar", t.status_bar),
                    ("muted", t.muted),
                ] {
                    floor(format!("text on {what}"), t.foreground, bg, 4.5);
                    floor(format!("muted text on {what}"), t.muted_foreground, bg, 4.5);
                }
                let pairs = [
                    ("sidebar text", t.sidebar_foreground, t.sidebar),
                    ("popover text", t.popover_foreground, t.popover),
                    ("active tab text", t.tab_active_foreground, t.tab_active),
                    ("primary button text", t.primary_foreground, t.primary),
                    (
                        "editor text",
                        style.editor_foreground.unwrap_or(t.foreground),
                        editor,
                    ),
                ];
                for (what, fg, bg) in pairs {
                    floor(what.into(), fg, bg, 4.5);
                }
                floor("focus ring".into(), t.ring, t.background, 3.0);
                floor("caret".into(), t.caret, editor, 3.0);
                let syntax = serde_json::to_value(&style.syntax).unwrap();
                for (k, s) in syntax.as_object().unwrap() {
                    let Some(color) = s["color"].as_str().and_then(palette::hex_rgba) else {
                        continue;
                    };
                    let ratio = if k.starts_with("comment") { 3.0 } else { 4.5 };
                    floor(format!("syntax.{k}"), color, editor, ratio);
                }
                // the page accent the theme names, on each paper its mode shows
                let pages: Value = serde_json::from_str(json).unwrap();
                let own = (pages["themes"].as_array().unwrap().iter())
                    .find(|p| p["name"] == config.name.as_ref())
                    .map(|p| p["colors"].clone())
                    .unwrap_or_default();
                let color = |k: &str| own[k].as_str().and_then(palette::hex_rgba);
                if let Some(accent) = color("pages.accent") {
                    for paper in [Some(white), color("pages.paper")].into_iter().flatten() {
                        floor("page accent".into(), accent, paper, 3.0);
                    }
                }
            }
        }
        assert!(short.is_empty(), "{}", short.join("\n"));
    }

    /// The themes made from VS Code's are there in their modes under the names their users know,
    /// and keep the colours they know: the editor's background, and the comments' colour and
    /// style as each source theme's own `comment` rule names them (scripts/themes/sources).
    #[test]
    fn the_vscode_themes_keep_their_colours() {
        let all: Vec<ThemeConfig> = (BUILT_IN.iter())
            .flat_map(|json| serde_json::from_str::<ThemeSet>(json).unwrap().themes)
            .collect();
        // (colour as theme.json writes it, its font style)
        let syntax = |t: &ThemeConfig, k: &str| {
            let style = serde_json::to_value(t.highlight.clone().unwrap_or_default()).unwrap();
            let s = &style["syntax"][k];
            let color = s["color"]
                .as_str()
                .and_then(palette::hex_rgba)
                .map(palette::hex);
            (
                color.unwrap_or_default(),
                s["font_style"].as_str().map(str::to_string),
            )
        };
        let (dark, light) = (ThemeMode::Dark, ThemeMode::Light);
        for (name, mode, background, comment) in [
            (
                "Catppuccin Latte",
                light,
                "#EFF1F5",
                Some(("#7C7F93", true)),
            ),
            (
                "Catppuccin Frappé",
                dark,
                "#303446",
                Some(("#949CBB", true)),
            ),
            (
                "Catppuccin Macchiato",
                dark,
                "#24273A",
                Some(("#939AB7", true)),
            ),
            ("Catppuccin Mocha", dark, "#1E1E2E", Some(("#9399B2", true))),
            ("Dracula", dark, "#282A36", Some(("#6272A4", false))),
            ("GitHub Dark", dark, "#0D1117", Some(("#8B949E", false))),
            ("GitHub Light", light, "#FFFFFF", Some(("#6E7781", false))),
            ("Night Owl", dark, "#011627", Some(("#637777", true))),
            ("Light Owl", light, "#FBFBFB", None),
            ("One Dark Pro", dark, "#282C34", Some(("#7F848E", true))),
            ("Tokyo Night", dark, "#1A1B26", None),
            ("Tokyo Night Light", light, "#E6E7ED", None),
        ] {
            let t = (all.iter().find(|t| t.name == name)).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(t.mode, mode, "{name}");
            assert_eq!(t.colors.background.as_deref(), Some(background), "{name}");
            if let Some((color, italic)) = comment {
                let (c, style) = syntax(t, "comment");
                assert_eq!(c, color, "{name}");
                assert_eq!(style.as_deref() == Some("italic"), italic, "{name}");
            }
        }
        // Tokyo Night's keywords, and its comments' italic from a rule of their own
        let tokyo = all.iter().find(|t| t.name == "Tokyo Night").unwrap();
        assert_eq!(syntax(tokyo, "keyword").0, "#BB9AF7");
        assert_eq!(syntax(tokyo, "comment").1.as_deref(), Some("italic"));
    }

    /// The page colours and accent a theme of the app's names reach the pages, as a theme file's
    /// in the themes folder do; one without them leaves the pages to the editor.
    #[test]
    fn the_built_in_themes_page_colours_are_read() {
        let found = page_colors().0;
        let dracula = &found["Dracula"];
        assert_eq!(dracula["pages.paper"], "#282A36");
        assert_eq!(dracula[ACCENT], "#BD93F9");
        assert!(found[DEFAULT_DARK].is_empty());
    }

    fn theme_json(name: &str) -> ThemeConfig {
        let set: ThemeSet = serde_json::from_str(BUILT_IN[0]).unwrap();
        set.themes.into_iter().find(|t| t.name == name).unwrap()
    }

    /// What `config` colours, as the theme on screen would be with it.
    fn applied(config: &ThemeConfig) -> Theme {
        let mut t = Theme::default();
        t.apply_config(&Rc::new(config.clone()));
        t
    }

    /// A theme file written by Export reads back as the same colours, the accent's and the
    /// changed ones in it, its syntax colours and page colours too, and names its accent.
    #[test]
    fn an_exported_theme_reads_back_the_same() {
        let base = theme_json(DEFAULT_DARK);
        let changed = BTreeMap::from([
            ("tab.active.background".to_string(), "#112233".to_string()),
            ("syntax.keyword".to_string(), "#FF000080".to_string()),
            (
                "highlight.editor.background".to_string(),
                "#101010".to_string(),
            ),
        ]);
        let made = effective(&base, palette::hex_color("#C93A85"), Some(&changed));
        let pages = BTreeMap::from([
            ("pages.selection".to_string(), "#FFCC0066".to_string()),
            ("pages.paper".to_string(), "#101830".to_string()),
            (ACCENT.to_string(), "#C93A85".to_string()),
        ]);
        let file = theme_file(&made, "Mine", &pages);
        assert!(!file.contains("null"), "{file}");
        let set: ThemeSet = serde_json::from_str(&file).unwrap();
        assert_eq!(set.themes[0].name, "Mine");
        let read = effective(&set.themes[0], None, None);
        assert_eq!(applied(&read).colors, applied(&made).colors);
        assert_eq!(read.highlight, made.highlight);
        assert_eq!(palette::hex(applied(&read).tab_active), "#112233");
        // the kit reads the file and leaves the page colours and accent out; the themes
        // folder's reading keeps them
        let dir = std::env::temp_dir().join(format!("rtex-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("mine.json"), &file).unwrap();
        std::fs::write(dir.join("notes.txt"), "not a theme").unwrap();
        let found = page_colors_in(theme_files(&dir));
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(found.get("Mine"), Some(&pages));
        assert_eq!(found.len(), 1);
    }

    /// A page colour changed in Settings is no colour of the kit's theme: the theme as before.
    #[test]
    fn page_colours_leave_the_kits_theme_alone() {
        let base = theme_json(DEFAULT_DARK);
        let changed = BTreeMap::from([("pages.paper".to_string(), "#FF0000".to_string())]);
        let made = effective(&base, None, Some(&changed));
        assert_eq!(applied(&made).colors, applied(&base).colors);
        assert_eq!(label("pages.paper"), "Dark page paper");
        assert!(groups()[0].title == "Pages" && groups()[0].keys.len() == PAGES.len());
    }

    /// A colour no one can read, or a key no theme has, changes nothing.
    #[test]
    fn a_bad_colour_in_the_settings_file_is_left_out() {
        let base = theme_json(DEFAULT_LIGHT);
        let changed = BTreeMap::from([
            ("background".to_string(), "red".to_string()),
            ("no.such.key".to_string(), "#FF0000".to_string()),
            ("syntax.nothing".to_string(), "#FF0000".to_string()),
        ]);
        let made = effective(&base, None, Some(&changed));
        assert_eq!(applied(&made).colors, applied(&base).colors);
    }
}
