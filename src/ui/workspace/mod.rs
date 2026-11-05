// The window: title bar, project panel (files and outline), tabs of open files and status bar,
// laid out like Zed.
mod chrome;
mod files;
mod insert_tab;
mod inspector;
mod project_panel;
mod ribbon;
mod widgets;

use files::*;

use crate::render;
use crate::render::viewer;
use crate::ui::editor::{Doc, ZOOMS};
use crate::ui::settings;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState, Redo, Undo};
use gpui_kit::component::list::ListItem;
use gpui_kit::component::menu::{ContextMenuExt, DropdownMenu, PopupMenu, PopupMenuItem};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::tree::{TreeEvent, TreeItem, TreeState, tree};
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Selectable, Sizable, TitleBar, h_resizable, resizable_panel,
    v_resizable,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

actions!(
    realtime_tex,
    [
        Save,
        Quit,
        CloseTab,
        Export,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        ToggleSidebar,
        ToggleSource,
        Recompile,
        OpenSettings,
        ToggleInspector,
        InsertLink,
        InsertPicture,
        InsertTable,
        InsertEquation,
        InsertFootnote,
        InsertCrossReference,
        InsertCitation,
        InsertPageBreak,
        InsertContents,
        MinimizeWindow,
        ZoomWindow,
        ToggleFullScreen
    ]
);

/// Taller than gpui-kit's 34px so the toolbar buttons have room; the traffic lights are centred in it.
pub const TITLE_BAR_H: f32 = 40.0;

pub struct Workspace {
    root: PathBuf,
    tree: Entity<TreeState>,
    tabs: Vec<Entity<Doc>>,
    active: usize,
    resources: Arc<Mutex<render::Resources>>,
    sidebar: bool,
    /// The sidebar on the right, for the element at the caret.
    inspector: inspector::Inspector,
    /// The ribbon's open tab.
    ribbon_tab: ribbon::RibbonTab,
    /// The colour palette's hex field.
    color_hex: Entity<InputState>,
    /// The table grid's size fields, for sizing a table from the keyboard.
    table_rows: Entity<InputState>,
    table_cols: Entity<InputState>,
    status: String,
    /// Folders open in the project panel (paths): only they are read from disk, so a large
    /// folder (node_modules) costs nothing until it is opened.
    expanded: std::collections::HashSet<String>,
    /// The tree entry being renamed in place (its path) and its name field.
    rename: Option<(String, Entity<InputState>)>,
    _subs: Vec<Subscription>,
    _rename_sub: Option<Subscription>,
    _dialog_subs: Vec<Subscription>,
}

impl Workspace {
    pub fn new(
        root: PathBuf,
        main: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let resources: Arc<Mutex<render::Resources>> = Default::default();
        resources.lock().unwrap().root = root.clone();
        let mut this = Self {
            tree: cx.new(|cx| TreeState::new(cx)),
            root,
            tabs: vec![],
            active: 0,
            resources,
            sidebar: true,
            inspector: Default::default(),
            ribbon_tab: Default::default(),
            color_hex: cx.new(|cx| InputState::new(window, cx).placeholder("1A2B3C")),
            table_rows: cx.new(|cx| InputState::new(window, cx).default_value("3")),
            table_cols: cx.new(|cx| InputState::new(window, cx).default_value("3")),
            status: String::new(),
            rename: None,
            expanded: Default::default(),
            _subs: vec![],
            _rename_sub: None,
            _dialog_subs: vec![],
        };
        // a colour typed in the palette's hex field, on Enter
        let hex = this.color_hex.clone();
        this._subs.push(cx.subscribe_in(
            &hex,
            window,
            |this, input, ev: &InputEvent, window, cx| {
                if !matches!(ev, InputEvent::PressEnter { .. }) {
                    return;
                }
                let v = input
                    .read(cx)
                    .value()
                    .trim()
                    .trim_start_matches('#')
                    .to_string();
                if v.len() == 6 && v.chars().all(|c| c.is_ascii_hexdigit()) {
                    this.format(
                        crate::ui::editor::FormatOp::Color(v.to_ascii_uppercase()),
                        window,
                        cx,
                    );
                    input.update(cx, |s, cx| s.set_value("", window, cx));
                }
            },
        ));
        let sub = cx.subscribe(&this.tree, |this, _, ev: &TreeEvent, cx| {
            match ev {
                TreeEvent::Expanded(id) => this.expanded.insert(id.to_string()),
                TreeEvent::Collapsed(id) => this.expanded.remove(id.as_ref()),
            };
            // an opened folder is read now
            if matches!(ev, TreeEvent::Expanded(_)) {
                this.refresh_tree(cx);
            }
        });
        this._subs.push(sub);
        this.refresh_tree(cx);
        match main.or_else(|| find_main(&this.root)) {
            Some(m) => this.open(m, window, cx),
            None => {
                this.status =
                    "No .tex file with \\documentclass in this folder: pick one on the left".into()
            }
        }
        this
    }

    fn doc(&self) -> Option<&Entity<Doc>> {
        self.tabs.get(self.active)
    }

    fn with_doc(&self, cx: &mut Context<Self>, f: impl FnOnce(&mut Doc, &mut Context<Doc>)) {
        if let Some(d) = self.doc() {
            d.update(cx, f);
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        window.set_window_edited(self.tabs.iter().any(|d| d.read(cx).dirty));
        // a click outside the page view (project panel, tabs, status bar) closes the source box
        let leave =
            |this: &mut Self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>| {
                this.with_doc(cx, |d, cx| d.leave(window, cx))
            };
        let inspector_visible = self.inspector_visible(cx);
        let inspector = if inspector_visible {
            self.render_inspector(window, cx)
        } else {
            div().into_any_element()
        };
        let center = div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .on_mouse_down(MouseButton::Left, cx.listener(leave))
                    .child(self.render_tabs(cx)),
            )
            .children(self.render_ribbon(cx))
            .child(div().flex_1().min_h_0().children(self.doc().cloned()).when(
                self.tabs.is_empty(),
                |d| {
                    d.flex()
                        .items_center()
                        .justify_center()
                        .text_color(theme.muted_foreground)
                        .child("Open a file from the project panel")
                },
            ));
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .on_action(cx.listener(|this, _: &Save, _, cx| this.with_doc(cx, |d, cx| d.save(cx))))
            .on_action(cx.listener(|this, _: &Undo, window, cx| {
                this.with_doc(cx, |d, cx| d.undo(window, cx))
            }))
            .on_action(cx.listener(|this, _: &Redo, window, cx| {
                this.with_doc(cx, |d, cx| d.redo(window, cx))
            }))
            .on_action(
                cx.listener(|this, _: &CloseTab, window, cx| this.close(this.active, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Export, _, cx| this.with_doc(cx, |d, cx| d.export(cx))),
            )
            .on_action(
                cx.listener(|this, _: &Recompile, _, cx| {
                    this.with_doc(cx, |d, cx| d.recompile(cx))
                }),
            )
            .on_action(cx.listener(|this, _: &ZoomIn, window, cx| {
                this.with_doc(cx, |d, cx| d.zoom_step(1, window, cx))
            }))
            .on_action(cx.listener(|this, _: &ZoomOut, window, cx| {
                this.with_doc(cx, |d, cx| d.zoom_step(-1, window, cx))
            }))
            .on_action(cx.listener(|this, _: &ZoomReset, window, cx| {
                this.with_doc(cx, |d, cx| d.set_zoom(1.0, None, window, cx))
            }))
            .on_action(cx.listener(|this, _: &ToggleSource, window, cx| {
                this.with_doc(cx, |d, cx| {
                    let on = !d.source_mode;
                    d.set_source_mode(on, window, cx)
                })
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.sidebar = !this.sidebar;
                cx.notify();
            }))
            .on_action(
                cx.listener(|this, _: &InsertLink, window, cx| this.open_link_dialog(window, cx)),
            )
            // the Insert menu: what the Insert tab does, for the keyboard and the menu bar
            .on_action(cx.listener(|this, _: &InsertPicture, _, cx| this.choose_picture(cx)))
            .on_action(cx.listener(|this, _: &InsertTable, _, cx| {
                this.with_doc(cx, |d, cx| d.insert_table(3, 3, cx))
            }))
            .on_action(cx.listener(|this, _: &InsertEquation, window, cx| {
                this.with_doc(cx, |d, cx| d.insert_display_math(window, cx))
            }))
            .on_action(cx.listener(|this, _: &InsertFootnote, window, cx| {
                this.with_doc(cx, |d, cx| d.insert_footnote(window, cx))
            }))
            .on_action(cx.listener(|this, _: &InsertCrossReference, window, cx| {
                this.open_reference_picker(window, cx)
            }))
            .on_action(cx.listener(|this, _: &InsertCitation, window, cx| {
                this.open_citation_picker(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &InsertPageBreak, _, cx| this.insert_block("\\newpage", cx)),
            )
            .on_action(cx.listener(|this, _: &InsertContents, _, cx| {
                this.insert_block("\\tableofcontents", cx)
            }))
            .on_action(|_: &MinimizeWindow, window, _| window.minimize_window())
            .on_action(|_: &ZoomWindow, window, _| window.zoom_window())
            .on_action(|_: &ToggleFullScreen, window, _| window.toggle_fullscreen())
            .on_action(cx.listener(|_, _: &ToggleInspector, _, cx| {
                settings::AppSettings::set(cx, |s| s.inspector_closed = !s.inspector_closed);
            }))
            // the title bar keeps its mouse to itself: it moves the window
            .child(self.render_title_bar(cx))
            .child(
                div().flex_1().min_h_0().child(
                    h_resizable("workspace")
                        .child(
                            resizable_panel()
                                .size(px(260.0))
                                .size_range(px(160.0)..px(640.0))
                                .visible(self.sidebar)
                                .child(
                                    div()
                                        .size_full()
                                        .on_mouse_down(MouseButton::Left, cx.listener(leave))
                                        .child(self.render_sidebar(cx)),
                                ),
                        )
                        .child(resizable_panel().child(center))
                        .child(
                            resizable_panel()
                                .size(px(280.0))
                                .size_range(px(220.0)..px(520.0))
                                .visible(inspector_visible)
                                .child(inspector),
                        ),
                ),
            )
            .child(
                div()
                    .on_mouse_down(MouseButton::Left, cx.listener(leave))
                    .child(self.render_status_bar(cx)),
            )
    }
}

/// The Settings window.
pub fn open_settings(cx: &mut App) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(860.0), px(620.0)), cx)),
        ..TitleBar::window_options()
    };
    let _ = gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| settings::SettingsView));
}
