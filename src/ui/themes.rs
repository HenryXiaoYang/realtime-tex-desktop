// The themes: the app's own (theme.json and themes/), those in the themes folder, and over the one
// chosen for each mode the accent and the colours changed in Settings › Colors, kept per theme as
// VS Code keeps its colour customizations. Every colour a theme can name is read off the kit's
// theme schema, so the colours a newer kit adds are listed without a change here.
use crate::ui::palette;
use crate::ui::settings::{AppSettings, support_dir};
use gpui_kit::component::highlighter::{HighlightTheme, HighlightThemeStyle, SyntaxColors};
use gpui_kit::component::{
    ActiveTheme, Theme, ThemeConfig, ThemeConfigColors, ThemeMode, ThemeRegistry, ThemeSet,
};
use gpui_kit::{App, Hsla, SharedString};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::LazyLock;

pub const DEFAULT_LIGHT: &str = "Realtime Tex Light";
pub const DEFAULT_DARK: &str = "Realtime Tex Dark";

/// The app's themes, in the order the theme lists show them; the themes folder's come after.
const BUILT_IN: [&str; 5] = [
    include_str!("theme.json"),
    include_str!("themes/solarized.json"),
    include_str!("themes/nord.json"),
    include_str!("themes/gruvbox.json"),
    include_str!("themes/high-contrast.json"),
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
    cx.observe_global::<ThemeRegistry>(|cx| {
        // reading the themes folder drops every theme not read from it
        if !ThemeRegistry::global(cx)
            .themes()
            .contains_key(DEFAULT_LIGHT)
        {
            add_built_in(cx);
        }
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

/// Where in a theme a colour key's colour is.
enum Place<'a> {
    /// The interface's colours: `colors`.
    Ui(&'a str),
    /// The source editor's: `highlight`.
    Highlight(&'a str),
    /// A syntax style's colour: `highlight.syntax`.
    Syntax(&'a str),
}

impl<'a> Place<'a> {
    fn of(key: &'a str) -> Self {
        if let Some(k) = key.strip_prefix("syntax.") {
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

/// Each mode's chosen theme, with the accent and its changed colours, in the theme's slots, where
/// every switch of mode (the system's too) takes it.
pub fn install(cx: &mut App) {
    let s = AppSettings::get(cx);
    let accent = palette::hex_color(&s.accent);
    let made = |mode| {
        chosen(s, mode, cx).map(|b| Rc::new(effective(&b, accent, s.colors.get(b.name.as_ref()))))
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
    let theme = effective(
        &base,
        palette::hex_color(&s.accent),
        s.colors.get(base.name.as_ref()),
    );
    let dir = dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", name.replace(['/', '\\', ':'], "-")));
    std::fs::write(&path, theme_file(&theme, &name))?;
    Ok(path)
}

/// `theme` as a theme file, named `name`, without the settings it leaves unset.
fn theme_file(theme: &ThemeConfig, name: &str) -> String {
    let mut t = theme.clone();
    t.name = name.to_string().into();
    t.is_default = false;
    let mut set = ThemeSet::default();
    set.name = t.name.clone();
    set.themes = vec![t];
    let mut json = serde_json::to_value(set).unwrap_or_default();
    strip_nulls(&mut json);
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

/// Settings › Colors' groups, each by the starts of its keys; a key has exactly one start.
const GROUPS: [(&str, &[&str]); 14] = [
    (
        "Window",
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
/// editor's, the syntax's.
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
    ui.into_iter().chain(editor).chain(syntax).collect()
}

/// A key's name in Settings: its words, the group's own prefix left out, "foreground" as "text".
fn label(key: &str) -> String {
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

/// Settings › Colors' groups, in order, each with its keys.
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
    (groups().iter().flat_map(|g| &g.keys))
        .map(|k| {
            let v = match Place::of(&k.key) {
                Place::Ui(key) => FIELD.get(key).map(|f| &colors[f]),
                Place::Highlight(key) => highlight.get(key),
                Place::Syntax(key) => syntax.get(key).map(|s| &s["color"]),
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

    /// The kit's theme schema, every colour of it, each in one group of Settings › Colors, and
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
                    assert!(keys.contains(&k), "{name}: {k}");
                    assert!(palette::hex_rgba(v.as_str().unwrap()).is_some(), "{k}: {v}");
                }
            }
        }
        assert_eq!(BUILT_IN_NAMES.len(), 9);
        assert_eq!(BUILT_IN_NAMES[..2], [DEFAULT_LIGHT, DEFAULT_DARK]);
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
    /// changed ones in it, its syntax colours too.
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
        let file = theme_file(&made, "Mine");
        assert!(!file.contains("null"), "{file}");
        let set: ThemeSet = serde_json::from_str(&file).unwrap();
        assert_eq!(set.themes[0].name, "Mine");
        let read = effective(&set.themes[0], None, None);
        assert_eq!(applied(&read).colors, applied(&made).colors);
        assert_eq!(read.highlight, made.highlight);
        assert_eq!(palette::hex(applied(&read).tab_active), "#112233");
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
