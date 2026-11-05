// Realtime Tex Desktop: a WYSIWYG LaTeX editor on the realtime-tex engine. The window is laid out
// like Zed: a title bar with the project and Export, a resizable project panel (files and
// outline), tabs of open files, and a status bar.
mod dev;
mod document;
mod engine;
mod render;
mod ui;

use gpui_kit::component::TitleBar;
use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
use gpui_kit::*;
use std::path::PathBuf;
use ui::workspace::*;
use ui::{editor, settings};

fn main() {
    // `realtime-tex-desktop [folder or .tex file]`; with nothing, ask for a folder
    let arg = std::env::args().nth(1).map(PathBuf::from);
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
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
                FontBigger, FontSmaller, FormatBold, FormatItalic, FormatUnderline, Heading1,
                Heading2, Heading3, InsertMath, OpenFind, OpenReplace, PastePlain,
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
                        MenuItem::action("Save", Save),
                        MenuItem::action("Recompile", Recompile),
                        MenuItem::action("Export PDF…", Export),
                        MenuItem::separator(),
                        MenuItem::action("Close Tab", CloseTab),
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
                        MenuItem::action("Heading 1", Heading1),
                        MenuItem::action("Heading 2", Heading2),
                        MenuItem::action("Heading 3", Heading3),
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
            let open = move |root: PathBuf, main: Option<String>, cx: &mut App| {
                let titlebar = TitlebarOptions {
                    traffic_light_position: Some(point(px(13.0), px(TITLE_BAR_H / 2.0 - 6.0))),
                    ..TitleBar::title_bar_options()
                };
                let options = WindowOptions {
                    window_bounds: Some(WindowBounds::centered(size(px(1460.0), px(900.0)), cx)),
                    titlebar: Some(titlebar),
                    window_min_size: Some(size(px(MIN_WINDOW.0), px(MIN_WINDOW.1))),
                    ..TitleBar::window_options()
                };
                gpui_kit::open_window(options, cx, move |window, cx| {
                    let ws = cx.new(|cx| Workspace::new(root, main, window, cx));
                    let weak = ws.downgrade();
                    // nothing is lost on close: unsaved edits are written to their files
                    window.on_window_should_close(cx, move |_, cx| {
                        let _ = weak.update(cx, |this, cx| this.save_all(cx));
                        true
                    });
                    ws
                })
                .expect("failed to open window");
            };
            match arg {
                Some(p) if p.is_file() => {
                    let p = p.canonicalize().unwrap_or(p);
                    open(
                        p.parent().unwrap().to_path_buf(),
                        p.file_name().map(|n| n.to_string_lossy().to_string()),
                        cx,
                    )
                }
                Some(p) => open(p.canonicalize().unwrap_or(p), None, cx),
                None => {
                    let rx = cx.prompt_for_paths(PathPromptOptions {
                        files: true,
                        directories: true,
                        multiple: false,
                        prompt: Some("Open".into()),
                    });
                    cx.spawn(async move |cx| {
                        if let Ok(Ok(Some(paths))) = rx.await {
                            let p = paths[0].clone();
                            cx.update(|cx| {
                                if p.is_file() {
                                    open(
                                        p.parent().unwrap().to_path_buf(),
                                        p.file_name().map(|n| n.to_string_lossy().to_string()),
                                        cx,
                                    )
                                } else {
                                    open(p, None, cx)
                                }
                            });
                        } else {
                            cx.update(|cx| cx.quit());
                        }
                    })
                    .detach();
                }
            }
        });
}
