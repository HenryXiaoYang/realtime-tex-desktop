// The window: the workspace around the open files, each file's editor, and the settings.
pub mod editor;
pub mod keymap;
pub mod palette;
pub mod projects;
pub mod settings;
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
        let tip: SharedString = tip.into();
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

/// A tooltip as a name: without its shortcut, `Bold (⌘B)` is `Bold`.
fn spoken(tip: &str) -> String {
    tip.split(" (").next().unwrap_or(tip).trim().to_string()
}

#[cfg(test)]
mod tests {
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
