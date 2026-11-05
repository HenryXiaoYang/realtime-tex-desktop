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
            cx.set_menus([
                Menu {
                    name: "Realtime Tex Desktop".into(),
                    items: vec![
                        MenuItem::action("Settings…", OpenSettings),
                        MenuItem::separator(),
                        MenuItem::action("Quit Realtime Tex Desktop", Quit),
                    ],
                    disabled: false,
                },
                Menu {
                    name: "File".into(),
                    items: vec![
                        MenuItem::action("Save", Save),
                        MenuItem::action("Recompile", Recompile),
                        MenuItem::action("Export PDF…", Export),
                        MenuItem::separator(),
                        MenuItem::action("Close Tab", CloseTab),
                    ],
                    disabled: false,
                },
                Menu {
                    name: "Edit".into(),
                    items: vec![
                        MenuItem::action("Undo", Undo),
                        MenuItem::action("Redo", Redo),
                        MenuItem::separator(),
                        MenuItem::action("Cut", Cut),
                        MenuItem::action("Copy", Copy),
                        MenuItem::action("Paste", Paste),
                        MenuItem::action("Select All", SelectAll),
                    ],
                    disabled: false,
                },
                Menu {
                    name: "View".into(),
                    items: vec![
                        MenuItem::action("Zoom In", ZoomIn),
                        MenuItem::action("Zoom Out", ZoomOut),
                        MenuItem::action("Actual Size", ZoomReset),
                        MenuItem::separator(),
                        MenuItem::action("Toggle Project Panel", ToggleSidebar),
                        MenuItem::action("Toggle Source / Rendered", ToggleSource),
                    ],
                    disabled: false,
                },
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
