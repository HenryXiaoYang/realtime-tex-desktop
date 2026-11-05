// The keyboard shortcuts you can change: every command with its default key and where it
// works, the overrides from the settings applied over them, and recording a new key from the
// Settings window. Plain editing keys (arrows, Enter, Backspace, Delete, Tab, Escape, ⌘C ⌘X
// ⌘V ⌘A) stay fixed; they are bound with the editor.
use crate::ui::editor::*;
use crate::ui::projects::{NewProject, OpenProject};
use crate::ui::settings::AppSettings;
use crate::ui::workspace::*;
use gpui_kit::component::input::{Redo, Undo};
use gpui_kit::*;

/// A command with a shortcut.
pub struct Command {
    /// Its name in the settings file.
    pub id: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    /// The key contexts it is bound in (None: anywhere).
    pub contexts: &'static [Option<&'static str>],
    pub key: &'static str,
    pub action: fn() -> Box<dyn Action>,
}

const ANY: &[Option<&str>] = &[None];
const PAGES: &[Option<&str>] = &[Some("Caret")];
const BOTH: &[Option<&str>] = &[Some("Caret"), Some("Pages")];

macro_rules! cmd {
    ($id:literal, $group:literal, $label:literal, $ctx:expr, $key:literal, $action:expr) => {
        Command {
            id: $id,
            group: $group,
            label: $label,
            contexts: $ctx,
            key: $key,
            action: || Box::new($action),
        }
    };
}

/// Every command you can give a shortcut, in the order the settings list them.
pub fn commands() -> Vec<Command> {
    vec![
        cmd!(
            "new-project",
            "File",
            "New Project",
            ANY,
            "cmd-shift-n",
            NewProject
        ),
        cmd!("open", "File", "Open", ANY, "cmd-o", OpenProject),
        cmd!("save", "File", "Save", ANY, "cmd-s", Save),
        cmd!("close-tab", "File", "Close Tab", ANY, "cmd-w", CloseTab),
        cmd!("export", "File", "Export PDF", ANY, "cmd-alt-e", Export),
        cmd!(
            "recompile",
            "File",
            "Recompile",
            ANY,
            "cmd-shift-r",
            Recompile
        ),
        cmd!("settings", "File", "Settings", ANY, "cmd-,", OpenSettings),
        cmd!("quit", "File", "Quit", ANY, "cmd-q", Quit),
        cmd!("undo", "Edit", "Undo", ANY, "cmd-z", Undo),
        cmd!("redo", "Edit", "Redo", ANY, "cmd-shift-z", Redo),
        cmd!(
            "paste-plain",
            "Edit",
            "Paste as Plain Text",
            PAGES,
            "cmd-shift-v",
            PastePlain
        ),
        cmd!("find", "Edit", "Find", BOTH, "cmd-f", OpenFind),
        cmd!(
            "replace",
            "Edit",
            "Find and Replace",
            BOTH,
            "cmd-alt-f",
            OpenReplace
        ),
        cmd!("find-next", "Edit", "Find Next", BOTH, "cmd-g", FindNext),
        cmd!(
            "find-previous",
            "Edit",
            "Find Previous",
            BOTH,
            "cmd-shift-g",
            FindPrev
        ),
        cmd!("math", "Edit", "Insert Formula", PAGES, "cmd-m", InsertMath),
        cmd!("link", "Insert", "Link", PAGES, "cmd-k", InsertLink),
        cmd!("picture", "Insert", "Picture", PAGES, "", InsertPicture),
        cmd!("table", "Insert", "Table", PAGES, "", InsertTable),
        cmd!("equation", "Insert", "Equation", PAGES, "", InsertEquation),
        cmd!("footnote", "Insert", "Footnote", PAGES, "", InsertFootnote),
        cmd!(
            "cross-reference",
            "Insert",
            "Cross-reference",
            PAGES,
            "",
            InsertCrossReference
        ),
        cmd!("citation", "Insert", "Citation", PAGES, "", InsertCitation),
        cmd!(
            "page-break",
            "Insert",
            "Page Break",
            PAGES,
            "",
            InsertPageBreak
        ),
        cmd!(
            "move-up",
            "Edit",
            "Move Paragraph Up",
            PAGES,
            "ctrl-shift-up",
            MoveParagraphUp
        ),
        cmd!(
            "move-down",
            "Edit",
            "Move Paragraph Down",
            PAGES,
            "ctrl-shift-down",
            MoveParagraphDown
        ),
        cmd!(
            "move-block-up",
            "Edit",
            "Move Selected Blocks Up",
            &[Some("Pages")],
            "ctrl-shift-up",
            MoveBlockUp
        ),
        cmd!(
            "move-block-down",
            "Edit",
            "Move Selected Blocks Down",
            &[Some("Pages")],
            "ctrl-shift-down",
            MoveBlockDown
        ),
        cmd!(
            "duplicate",
            "Edit",
            "Duplicate Selected Blocks",
            &[Some("Pages")],
            "cmd-d",
            DuplicateBlock
        ),
        cmd!("bold", "Format", "Bold", PAGES, "cmd-b", FormatBold),
        cmd!("italic", "Format", "Italic", PAGES, "cmd-i", FormatItalic),
        cmd!(
            "underline",
            "Format",
            "Underline",
            PAGES,
            "cmd-u",
            FormatUnderline
        ),
        cmd!(
            "bigger",
            "Format",
            "Bigger Text",
            PAGES,
            "cmd-shift-.",
            FontBigger
        ),
        cmd!(
            "smaller",
            "Format",
            "Smaller Text",
            PAGES,
            "cmd-shift-,",
            FontSmaller
        ),
        cmd!(
            "heading-1",
            "Format",
            "Section",
            PAGES,
            "cmd-alt-1",
            Heading1
        ),
        cmd!(
            "heading-2",
            "Format",
            "Subsection",
            PAGES,
            "cmd-alt-2",
            Heading2
        ),
        cmd!(
            "heading-3",
            "Format",
            "Subsubsection",
            PAGES,
            "cmd-alt-3",
            Heading3
        ),
        cmd!(
            "align-left",
            "Format",
            "Align Left",
            PAGES,
            "cmd-l",
            AlignLeft
        ),
        cmd!(
            "align-center",
            "Format",
            "Center",
            PAGES,
            "cmd-e",
            AlignCenter
        ),
        cmd!(
            "align-right",
            "Format",
            "Align Right",
            PAGES,
            "cmd-r",
            AlignRight
        ),
        cmd!("justify", "Format", "Justify", PAGES, "cmd-j", AlignJustify),
        cmd!("zoom-in", "View", "Zoom In", ANY, "cmd-=", ZoomIn),
        cmd!("zoom-out", "View", "Zoom Out", ANY, "cmd--", ZoomOut),
        cmd!("zoom-reset", "View", "Actual Size", ANY, "cmd-0", ZoomReset),
        cmd!(
            "project-panel",
            "View",
            "Show or Hide Project Panel",
            ANY,
            "cmd-\\",
            ToggleSidebar
        ),
        cmd!(
            "inspector",
            "View",
            "Show or Hide Sidebar",
            ANY,
            "cmd-alt-\\",
            ToggleInspector
        ),
        cmd!(
            "source",
            "View",
            "Switch Visual / Source",
            ANY,
            "cmd-shift-e",
            ToggleSource
        ),
    ]
}

/// The key `c` has now: the settings' override (empty: none), else its default.
pub fn key_of(c: &Command, cx: &App) -> String {
    AppSettings::get(cx)
        .shortcuts
        .get(c.id)
        .cloned()
        .unwrap_or_else(|| c.key.to_string())
}

/// `key` bound to `action` in each of `contexts` (a key that does not parse binds nothing).
fn bind_action(
    key: &str,
    contexts: &[Option<&str>],
    action: impl Fn() -> Box<dyn Action>,
    cx: &mut App,
) {
    if key.is_empty() {
        return;
    }
    let bindings: Vec<KeyBinding> = contexts
        .iter()
        .filter_map(|ctx| {
            let pred = match ctx {
                Some(c) => Some(std::rc::Rc::new(KeyBindingContextPredicate::parse(c).ok()?)),
                None => None,
            };
            KeyBinding::load(key, action(), pred, false, None, cx).ok()
        })
        .collect();
    cx.bind_keys(bindings);
}

fn bind(c: &Command, key: &str, cx: &mut App) {
    bind_action(key, c.contexts, c.action, cx);
}

/// `key` doing nothing where `c` works: what it was bound to there stops.
fn silence(c: &Command, key: &str, cx: &mut App) {
    bind_action(key, c.contexts, || Box::new(NoAction), cx);
}

/// Bind every command to its key (at launch, after the settings are read).
pub fn bind_all(cx: &mut App) {
    for c in commands() {
        bind(&c, &key_of(&c, cx), cx);
    }
    // the + beside = zooms in too
    cx.bind_keys([KeyBinding::new("cmd-+", ZoomIn, None)]);
}

/// Give command `id` key `key` (empty: none), saved; its old key stops working at once and the
/// new one starts. Later bindings win, so this layers over what is bound.
pub fn set_key(id: &str, key: &str, cx: &mut App) {
    let Some(c) = commands().into_iter().find(|c| c.id == id) else {
        return;
    };
    let old = key_of(&c, cx);
    let key = key.to_string();
    AppSettings::set(cx, |s| {
        if key == c.key {
            s.shortcuts.remove(c.id);
        } else {
            s.shortcuts.insert(c.id.to_string(), key.clone());
        }
    });
    if old != key {
        silence(&c, &old, cx);
        // another command that kept the old key gets it back (it was shared, as the two
        // move-up commands share theirs in different contexts)
        let sharing: Vec<Command> = commands()
            .into_iter()
            .filter(|o| o.id != c.id && key_of(o, cx) == old)
            .collect();
        for other in &sharing {
            bind(other, &old, cx);
        }
    }
    bind(&c, &key, cx);
}

/// Every command its default key again.
pub fn reset_all(cx: &mut App) {
    let changed: Vec<String> = AppSettings::get(cx).shortcuts.keys().cloned().collect();
    for id in changed {
        if let Some(c) = commands().into_iter().find(|c| c.id == id) {
            set_key(&id, c.key, cx);
        }
    }
}

/// The command other than `id` that has `key` where `id` works, if any.
pub fn clash<'a>(id: &str, key: &str, all: &'a [Command], cx: &App) -> Option<&'a Command> {
    let me = all.iter().find(|c| c.id == id)?;
    // where both work: the same context, or either anywhere (so the move-up commands for the
    // caret and for selected blocks share a key without clashing)
    let overlap = |o: &Command| {
        o.contexts.iter().any(|oc| {
            me.contexts
                .iter()
                .any(|mc| oc.is_none() || mc.is_none() || oc == mc)
        })
    };
    all.iter()
        .find(|o| o.id != id && !key.is_empty() && key_of(o, cx) == key && overlap(o))
}

/// A key as macOS writes it: ⌃⌥⇧⌘ then the key.
pub fn display(key: &str) -> String {
    if key.is_empty() {
        return "—".into();
    }
    let parts: Vec<&str> = key.split('-').collect();
    // "cmd--" is cmd and minus
    let (mods, k) = if key.ends_with("--") {
        (&parts[..parts.len() - 2], "-")
    } else {
        (&parts[..parts.len() - 1], parts[parts.len() - 1])
    };
    let mut out = String::new();
    for (m, sym) in [("ctrl", "⌃"), ("alt", "⌥"), ("shift", "⇧"), ("cmd", "⌘")] {
        if mods.contains(&m) {
            out.push_str(sym);
        }
    }
    let k = match k {
        "up" => "↑".to_string(),
        "down" => "↓".to_string(),
        "left" => "←".to_string(),
        "right" => "→".to_string(),
        "enter" => "↩".to_string(),
        "space" => "Space".to_string(),
        "backspace" => "⌫".to_string(),
        "tab" => "⇥".to_string(),
        k if k.chars().count() == 1 => k.to_uppercase(),
        k => {
            let mut c = k.chars();
            c.next().map_or(String::new(), |f| {
                f.to_uppercase().collect::<String>() + c.as_str()
            })
        }
    };
    out + &k
}

/// The command whose new key is being recorded in the Settings window, and a clash waiting for
/// Replace or Cancel: (command, key, the command that has it).
#[derive(Default)]
pub struct Recording {
    pub id: Option<&'static str>,
    pub clash: Option<(&'static str, String, &'static str)>,
    _keys: Option<Subscription>,
}

impl Global for Recording {}

/// Start recording a key for `id`: the next key pressed (with a modifier, or a function key)
/// becomes its shortcut, unless another command has it; Escape cancels.
pub fn record(id: &'static str, cx: &mut App) {
    let sub = cx.intercept_keystrokes(|ev, window, cx| {
        let Some(id) = cx.global::<Recording>().id else {
            return;
        };
        let ks = &ev.keystroke;
        let key = ks.key.as_str();
        if [
            "shift", "control", "alt", "platform", "function", "cmd", "ctrl",
        ]
        .contains(&key)
        {
            return;
        }
        cx.stop_propagation();
        if key == "escape" && !ks.modifiers.modified() {
            cx.set_global(Recording::default());
            window.refresh();
            return;
        }
        // a plain letter would type: a shortcut needs ⌘, ⌃ or ⌥ (or is a function key)
        let m = &ks.modifiers;
        if !(m.platform || m.control || m.alt || key.starts_with('f') && key.len() > 1) {
            return;
        }
        let text = ks.unparse(OperatingSystem::Mac);
        let all = commands();
        let other = clash(id, &text, &all, cx).map(|o| o.id);
        match other {
            Some(o) => {
                let r = cx.global_mut::<Recording>();
                r.id = None;
                r.clash = Some((id, text, o));
                r._keys = None;
            }
            None => {
                cx.set_global(Recording::default());
                set_key(id, &text, cx);
            }
        }
        window.refresh();
    });
    cx.set_global(Recording {
        id: Some(id),
        clash: None,
        _keys: Some(sub),
    });
}

#[cfg(test)]
mod tests {
    use super::{bind_all, display, reset_all, set_key};
    use crate::ui::editor::FormatBold;
    use crate::ui::settings::AppSettings;
    use crate::ui::workspace::InsertLink;
    use gpui_kit::{Action, App, KeyContext, Keystroke, TestAppContext};

    /// What `key` does in the caret's context: the action's name, if any.
    fn does(key: &'static str, cx: &App) -> Option<String> {
        let ctx = [KeyContext::parse("Caret").unwrap()];
        let (bindings, _) = cx
            .key_bindings()
            .borrow()
            .bindings_for_input(&[Keystroke::parse(key).unwrap()], &ctx);
        bindings.first().map(|b| b.action().name().to_string())
    }

    #[gpui_kit::test]
    fn a_new_key_moves_its_command(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
            crate::ui::editor::bind_keys(cx);
            bind_all(cx);
            let bold = FormatBold.name().to_string();
            assert_eq!(does("cmd-b", cx), Some(bold.clone()));
            assert_eq!(does("cmd-k", cx), Some(InsertLink.name().to_string()));
            set_key("bold", "cmd-shift-b", cx);
            assert_eq!(does("cmd-shift-b", cx), Some(bold.clone()));
            assert_ne!(
                does("cmd-b", cx),
                Some(bold.clone()),
                "the old key no longer bolds"
            );
            assert_eq!(
                AppSettings::get(cx)
                    .shortcuts
                    .get("bold")
                    .map(String::as_str),
                Some("cmd-shift-b")
            );
            reset_all(cx);
            assert_eq!(does("cmd-b", cx), Some(bold));
            assert!(AppSettings::get(cx).shortcuts.is_empty());
        });
    }

    #[test]
    fn keys_read_as_on_a_mac() {
        assert_eq!(display("cmd-shift-z"), "⇧⌘Z");
        assert_eq!(display("cmd--"), "⌘-");
        assert_eq!(display("ctrl-shift-up"), "⌃⇧↑");
        assert_eq!(display("cmd-alt-e"), "⌥⌘E");
        assert_eq!(display(""), "—");
    }
}
