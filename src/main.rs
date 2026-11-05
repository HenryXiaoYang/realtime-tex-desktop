// Realtime Tex Desktop: a WYSIWYG LaTeX editor on the realtime-tex engine. The window is laid out
// like Zed: a title bar with the project and Export, a resizable project panel (files and
// outline), tabs of open files, and a status bar.
mod dev;
mod document;
mod engine;
mod render;
mod ui;

use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
use gpui_kit::*;
use std::path::PathBuf;
use ui::projects::{CloseProject, NewProject, OpenProject};
use ui::workspace::*;
use ui::{editor, settings};

fn main() {
    // `realtime-tex-desktop [folder or .tex file]`; with nothing, what was open last
    let arg = std::env::args().nth(1).map(PathBuf::from);
    let app = gpui_kit::application().with_assets(gpui_kit::assets::AllAssets);
    app.on_reopen(ui::projects::reopened);
    app.run(move |cx| {
        gpui_kit::init(cx);
        settings::AppSettings::init(cx);
        cx.on_action(|_: &OpenSettings, cx| open_settings(cx));
        // the fixed editing keys first, then every command's key (the settings may have
        // changed some): later bindings win
        editor::bind_keys(cx);
        ui::keymap::bind_all(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        // no rtex or lualatex outlives the app
        cx.on_app_quit(|_| async { engine::stop_all() }).detach();
        // every command has a place in the menu bar, where its shortcut shows
        use editor::{
            AlignCenter, AlignJustify, AlignLeft, AlignRight, CloseFind, FindNext, FindPrev,
            FontBigger, FontSmaller, FormatBold, FormatItalic, FormatUnderline, Heading1, Heading2,
            Heading3, InsertMath, OpenFind, OpenReplace, PastePlain,
        };
        let menu = |name: &str, items: Vec<MenuItem>| Menu {
            name: name.to_string().into(),
            items,
            disabled: false,
        };
        cx.set_menus([
            menu(
                "Realtime Tex Desktop",
                vec![
                    MenuItem::action("Settings…", OpenSettings),
                    MenuItem::separator(),
                    MenuItem::os_submenu("Services", SystemMenuType::Services),
                    MenuItem::separator(),
                    MenuItem::action("Quit Realtime Tex Desktop", Quit),
                ],
            ),
            menu(
                "File",
                vec![
                    MenuItem::action("New Project…", NewProject),
                    MenuItem::action("Open…", OpenProject),
                    MenuItem::separator(),
                    MenuItem::action("Save", Save),
                    MenuItem::action("Recompile", Recompile),
                    MenuItem::action("Export PDF…", Export),
                    MenuItem::separator(),
                    MenuItem::action("Close Tab", CloseTab),
                    MenuItem::action("Close Project", CloseProject),
                ],
            ),
            menu(
                "Edit",
                vec![
                    MenuItem::os_action("Undo", Undo, OsAction::Undo),
                    MenuItem::os_action("Redo", Redo, OsAction::Redo),
                    MenuItem::separator(),
                    MenuItem::os_action("Cut", Cut, OsAction::Cut),
                    MenuItem::os_action("Copy", Copy, OsAction::Copy),
                    MenuItem::os_action("Paste", Paste, OsAction::Paste),
                    MenuItem::action("Paste and Match Style", PastePlain),
                    MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
                    MenuItem::separator(),
                    MenuItem::submenu(menu(
                        "Find",
                        vec![
                            MenuItem::action("Find…", OpenFind),
                            MenuItem::action("Find and Replace…", OpenReplace),
                            MenuItem::action("Find Next", FindNext),
                            MenuItem::action("Find Previous", FindPrev),
                            MenuItem::action("Hide Find Bar", CloseFind),
                        ],
                    )),
                ],
            ),
            menu(
                "Insert",
                vec![
                    MenuItem::action("Picture…", InsertPicture),
                    MenuItem::action("Table", InsertTable),
                    MenuItem::separator(),
                    MenuItem::action("Formula", InsertMath),
                    MenuItem::action("Equation", InsertEquation),
                    MenuItem::separator(),
                    MenuItem::action("Link…", InsertLink),
                    MenuItem::action("Cross-reference…", InsertCrossReference),
                    MenuItem::action("Footnote", InsertFootnote),
                    MenuItem::action("Citation…", InsertCitation),
                    MenuItem::separator(),
                    MenuItem::action("Page Break", InsertPageBreak),
                    MenuItem::action("Table of Contents", InsertContents),
                ],
            ),
            menu(
                "Format",
                vec![
                    MenuItem::action("Bold", FormatBold),
                    MenuItem::action("Italic", FormatItalic),
                    MenuItem::action("Underline", FormatUnderline),
                    MenuItem::separator(),
                    MenuItem::action("Bigger", FontBigger),
                    MenuItem::action("Smaller", FontSmaller),
                    MenuItem::separator(),
                    MenuItem::action("Section", Heading1),
                    MenuItem::action("Subsection", Heading2),
                    MenuItem::action("Subsubsection", Heading3),
                    MenuItem::separator(),
                    MenuItem::action("Align Left", AlignLeft),
                    MenuItem::action("Center", AlignCenter),
                    MenuItem::action("Align Right", AlignRight),
                    MenuItem::action("Justify", AlignJustify),
                ],
            ),
            menu(
                "View",
                vec![
                    MenuItem::action("Zoom In", ZoomIn),
                    MenuItem::action("Zoom Out", ZoomOut),
                    MenuItem::action("Actual Size", ZoomReset),
                    MenuItem::action("Fit Width", FitWidth),
                    MenuItem::separator(),
                    MenuItem::action("Show or Hide Project Panel", ToggleSidebar),
                    MenuItem::action("Show or Hide Sidebar", ToggleInspector),
                    MenuItem::action("Switch Visual / Source", ToggleSource),
                    MenuItem::separator(),
                    MenuItem::action("Enter Full Screen", ToggleFullScreen),
                ],
            ),
            menu(
                "Window",
                vec![
                    MenuItem::action("Minimize", MinimizeWindow),
                    MenuItem::action("Zoom", ZoomWindow),
                ],
            ),
        ]);
        cx.activate(true);
        // the projects open when the app quit, else the welcome window
        ui::projects::init(cx);
        ui::projects::start(arg, cx);
    });
}
