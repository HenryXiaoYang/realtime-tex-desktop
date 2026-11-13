// The window: the workspace around the open files, each file's editor, and the settings.
pub mod editor;
pub mod keymap;
pub mod palette;
pub mod projects;
pub mod settings;
pub mod tex_setup;
pub mod welcome;
pub mod workspace;

use gpui_kit::SharedString;
use gpui_kit::component::button::Button;

/// The tooltip of an icon button, which is also its name for VoiceOver: an icon gives a button
/// no name, and a tooltip only describes it. The name leaves out the shortcut in brackets.
pub trait Tip {
    fn tip(self, tip: impl Into<SharedString>) -> Self;
}

impl Tip for Button {
    fn tip(self, tip: impl Into<SharedString>) -> Self {
        let tip: SharedString = keys_in(&tip.into()).into();
        let name = spoken(&tip);
        self.accessibility_label(name).tooltip(tip)
    }
}

/// What a file error means, in a sentence a person reads in a notification: the common ones in
/// plain words, the rest as the system says them.
pub fn plain_error(e: &std::io::Error) -> String {
    use std::io::ErrorKind::*;
    match e.kind() {
        PermissionDenied => "You don’t have permission to change this file or its folder".into(),
        StorageFull => "The disk is full".into(),
        ReadOnlyFilesystem => "The disk is read-only".into(),
        NotFound => "The file or its folder is no longer there".into(),
        AlreadyExists => "Something with that name is already there".into(),
        _ => {
            // "Operation timed out (os error 60)": the code means nothing to a person
            let s = e.to_string();
            s.split(" (os error").next().unwrap_or(&s).to_string()
        }
    }
}

/// `text` cut to `max` characters (not bytes: CJK and emoji are safe), with an ellipsis.
pub fn clip(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", text[..at].trim_end()),
        None => text.to_string(),
    }
}

/// `text` with its Mac shortcuts (⌘⇧R) as Windows and Linux write them (Ctrl+Shift+R).
pub fn keys_in(text: &str) -> String {
    if cfg!(target_os = "macos") {
        return text.to_string();
    }
    keys_off_mac(text)
}

fn keys_off_mac(text: &str) -> String {
    const MODS: &str = "⌃⌥⇧⌘";
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if !MODS.contains(c) {
            out.push(c);
            continue;
        }
        let mut mods = vec![c];
        while let Some(m) = chars.next_if(|m| MODS.contains(*m)) {
            mods.push(m);
        }
        for (of, name) in [("⌃⌘", "Ctrl+"), ("⌥", "Alt+"), ("⇧", "Shift+")] {
            if mods.iter().any(|m| of.contains(*m)) {
                out.push_str(name);
            }
        }
        match chars.next() {
            Some('⌫') => out.push_str("Backspace"),
            Some('↩') => out.push_str("Enter"),
            Some(k) => out.push(k),
            None => {}
        }
    }
    out
}

/// A tooltip as a name: without its shortcut, `Bold (⌘B)` is `Bold`.
fn spoken(tip: &str) -> String {
    tip.split(" (").next().unwrap_or(tip).trim().to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn shortcuts_read_as_the_platform_writes_them() {
        assert_eq!(
            super::keys_off_mac("Recompile (⌘⇧R), then ⌥⌘\\ or ⌘⌫"),
            "Recompile (Ctrl+Shift+R), then Ctrl+Alt+\\ or Ctrl+Backspace"
        );
    }

    #[test]
    fn errors_read_as_sentences() {
        use std::io::{Error, ErrorKind};
        let denied = Error::from(ErrorKind::PermissionDenied);
        assert_eq!(
            super::plain_error(&denied),
            "You don’t have permission to change this file or its folder"
        );
        let other = Error::from_raw_os_error(60);
        assert!(
            !super::plain_error(&other).contains("os error"),
            "{}",
            super::plain_error(&other)
        );
        assert_eq!(super::clip("短い文字列です", 3), "短い文…");
        assert_eq!(super::clip("short", 10), "short");
    }

    #[test]
    fn tooltips_name_without_their_shortcuts() {
        assert_eq!(super::spoken("Bold (⌘B)"), "Bold");
        assert_eq!(super::spoken("Hide the sidebar (⌘⌥\\)"), "Hide the sidebar");
        assert_eq!(super::spoken("Text colour"), "Text colour");
    }
}
