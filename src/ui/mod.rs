// The window: the workspace around the open files, each file's editor, and the settings.
pub mod editor;
pub mod keymap;
pub mod palette;
pub mod settings;
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

/// A tooltip as a name: without its shortcut, `Bold (⌘B)` is `Bold`.
fn spoken(tip: &str) -> String {
    tip.split(" (").next().unwrap_or(tip).trim().to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn tooltips_name_without_their_shortcuts() {
        assert_eq!(super::spoken("Bold (⌘B)"), "Bold");
        assert_eq!(super::spoken("Hide the sidebar (⌘⌥\\)"), "Hide the sidebar");
        assert_eq!(super::spoken("Text colour"), "Text colour");
    }
}
