// The project panel: the file tree and the section outline, one above the other.
use super::*;
use crate::ui::Tip;
use gpui_kit::component::scroll::{ScrollableElement, ScrollbarAxis};

impl Workspace {
    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let icon_button = |id: &'static str, icon: &'static str, tip: &'static str| {
            Button::new(id)
                .icon(Icon::default().path(icon))
                .ghost()
                .xsmall()
                .tip(tip)
        };
        let weak = cx.entity().downgrade();
        let renaming = self.rename.clone();
        let rename_error = self.rename_error.clone();
        let menu_weak = weak.clone();
        let root = self.root.clone();
        let file_tree = tree(&self.tree, move |ix, entry, _selected, _, cx| {
            let item = entry.item();
            let id = item.id.to_string();
            let guides = indent_guides(entry.depth(), cx);
            if id.ends_with(PLACEHOLDER) {
                return ListItem::new(ix).children(guides).child(
                    div()
                        .text_sm()
                        .whitespace_nowrap()
                        .italic()
                        .text_color(cx.theme().muted_foreground)
                        .pl(px(entry.depth() as f32 * TREE_INDENT + TREE_INDENT))
                        .child(item.label.clone()),
                );
            }
            let field = renaming
                .as_ref()
                .filter(|(r, _)| *r == id)
                .map(|(_, state)| state.clone());
            // why the name typed cannot be, under the field (which stays open with it)
            let field_error = field.as_ref().and_then(|_| rename_error.clone());
            // as VS Code: a folder's arrow in the column a file's icon is in, no folder icon
            let icon = if entry.is_folder() {
                if entry.is_expanded() {
                    "icons/chevron-down.svg"
                } else {
                    "icons/chevron-right.svg"
                }
            } else if id.ends_with(".pdf") || id.ends_with(".png") || id.ends_with(".jpg") {
                "icons/file-image.svg"
            } else if id.ends_with(".tex") {
                "icons/file-text.svg"
            } else {
                "icons/file.svg"
            };
            let weak = weak.clone();
            ListItem::new(ix)
                .children(guides)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_sm()
                        .whitespace_nowrap()
                        .pl(px(entry.depth() as f32 * TREE_INDENT))
                        .child(widgets::tool_icon(icon, true, cx).xsmall().flex_shrink_0())
                        .map(|row| match &field {
                            // Escape cancels before the field's own bindings see it
                            Some(state) => row.child({
                                let weak = weak.clone();
                                div()
                                    .flex_1()
                                    .capture_key_down(move |ev, _, cx| {
                                        if ev.keystroke.key == "escape" {
                                            let _ =
                                                weak.update(cx, |this, cx| this.cancel_rename(cx));
                                            cx.stop_propagation();
                                        }
                                    })
                                    .flex()
                                    .flex_col()
                                    .child(Input::new(state).xsmall())
                                    .children(field_error.clone().map(|e| {
                                        div()
                                            .pt_0p5()
                                            .text_xs()
                                            .text_color(cx.theme().danger)
                                            .child(e)
                                    }))
                            }),
                            None => row.child(item.label.clone()),
                        }),
                )
                .on_click(move |_, window, cx| {
                    if field.is_none() && !id.ends_with('/') {
                        let id = id.clone();
                        let _ = weak.update(cx, |this, cx| {
                            if this.root.join(&id).is_file() {
                                this.open(id, window, cx)
                            }
                        });
                    }
                })
        });
        let file_tree = file_tree.context_menu(move |_, entry, menu, _, _| {
            let rel = entry.item().id.to_string();
            Self::tree_menu(&menu_weak, &root, &rel, entry.is_folder(), menu)
        });
        // the tree's list cannot measure its rows, so a hidden copy of their names, each
        // indented as its row is (12 px of padding a side, its depth, an icon and a gap), sets
        // how wide the rows are
        // shortcut: lays out every row's name each frame, fine for a LaTeX project's tree;
        // measure only the longest if trees get large
        let tree_state = self.tree.read(cx);
        let row_widths: Vec<Div> = (0..)
            .map_while(|ix| tree_state.entry(ix))
            .map(|e| {
                div()
                    .pl(px(24.0 + (e.depth() + 1) as f32 * TREE_INDENT))
                    .child(e.item().label.clone())
            })
            .collect();
        let outline: Vec<_> = self
            .doc()
            .map(|d| {
                d.read(cx)
                    .outline
                    .iter()
                    .map(|h| (h.offset, h.level, h.number.clone(), h.title.clone()))
                    .collect()
            })
            .unwrap_or_default();
        // each heading's line runs down beside the headings under it, at the start of its
        // number, as the file tree's do under a folder
        let mut open: Vec<usize> = vec![];
        let outline =
            outline.into_iter().map(|(off, level, number, title)| {
                let title_copy = title.clone();
                while open.last().is_some_and(|l| *l >= level) {
                    open.pop();
                }
                let guides: Vec<Div> = open
                    .iter()
                    .map(|l| guide_line(12.0 + *l as f32 * OUTLINE_INDENT + 3.5, cx))
                    .collect();
                open.push(level);
                div()
                    .id(("heading", off))
                    .relative()
                    .children(guides)
                    .flex()
                    .whitespace_nowrap()
                    .gap_2()
                    .text_sm()
                    .pl(px(12.0 + level as f32 * OUTLINE_INDENT))
                    .py_0p5()
                    .rounded_md()
                    .hover(|s| s.bg(theme.sidebar_accent))
                    .cursor_pointer()
                    .children(number.map(|n| div().text_color(theme.link).child(n)))
                    .child(title)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.with_doc(cx, |d, cx| d.jump_to(off, cx))
                    }))
                    .context_menu({
                        let weak = cx.entity().downgrade();
                        move |menu, _, _| {
                            let item = |label: &'static str, f: fn(&mut Doc, usize, &mut Window, &mut Context<Doc>)| {
                                let weak = weak.clone();
                                PopupMenuItem::new(label).on_click(move |_, window, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        if let Some(d) = this.doc() {
                                            d.update(cx, |d, cx| f(d, off, window, cx));
                                        }
                                    });
                                })
                            };
                            let t = title_copy.clone();
                            menu.item(item("Go to Section", |d, off, _, cx| d.jump_to(off, cx)))
                                .item(item("Edit Heading", |d, off, w, cx| d.edit_at(off, w, cx)))
                                .item(item("Show in Source", |d, off, w, cx| d.source_at(off, w, cx)))
                                .separator()
                                .item(PopupMenuItem::new("Copy Title").on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(t.clone()))
                                }))
                        }
                    })
            });
        let header = |title: &'static str| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .h_8()
                .px_3()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.muted_foreground)
                .child(title)
        };
        let files = div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                header("PROJECT").child(
                    div()
                        .flex()
                        .child(
                            icon_button("new-file", "icons/file-plus.svg", "New File").on_click(
                                cx.listener(|this, _, w, cx| this.new_entry("", false, w, cx)),
                            ),
                        )
                        .child(
                            icon_button("new-folder", "icons/folder-plus.svg", "New Folder")
                                .on_click(
                                    cx.listener(|this, _, w, cx| this.new_entry("", true, w, cx)),
                                ),
                        )
                        .child(
                            icon_button("collapse", "icons/list-collapse.svg", "Collapse All")
                                .on_click(cx.listener(|this, _, _, cx| this.collapse_tree(cx))),
                        ),
                ),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("files-scroll")
                            .size_full()
                            .flex()
                            .overflow_x_scroll()
                            // a wheel's vertical turn scrolls the list in it, not this sideways
                            .restrict_scroll_to_axis()
                            .track_scroll(&self.files_scroll)
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .min_w_full()
                                    .h_full()
                                    .px_1()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .h_0()
                                            .overflow_hidden()
                                            .text_sm()
                                            .whitespace_nowrap()
                                            .children(row_widths),
                                    )
                                    .child(file_tree),
                            ),
                    )
                    .horizontal_scrollbar(&self.files_scroll),
            );
        let outline = div()
            .size_full()
            .flex()
            .flex_col()
            .child(header("OUTLINE"))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("outline")
                            .size_full()
                            .flex()
                            .items_start()
                            .overflow_scroll()
                            .track_scroll(&self.outline_scroll)
                            // one column as wide as the longest heading, so each row's hover
                            // spans it
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .min_w_full()
                                    .px_1()
                                    .flex()
                                    .flex_col()
                                    .children(outline),
                            ),
                    )
                    .scrollbar(&self.outline_scroll, ScrollbarAxis::Both),
            );
        // files above, outline below, with a divider to drag between them
        div()
            .size_full()
            // the theme's sidebar, set off from the pages
            .bg(crate::ui::palette::panel(&theme))
            .text_color(theme.sidebar_foreground)
            .child(
                v_resizable("sidebar-split")
                    .child(
                        resizable_panel()
                            .size_range(px(80.0)..px(4000.0))
                            .child(files),
                    )
                    .child(
                        resizable_panel()
                            .size(px(320.0))
                            .size_range(px(60.0)..px(4000.0))
                            .child(outline),
                    ),
            )
    }
}

/// How much deeper each level of the file tree sits, in px: an icon's column (12) and the gap
/// after it (4), so a folder's files start under its name.
const TREE_INDENT: f32 = 16.0;

/// A tree row's indent guides: a thin line under each folder it is in, from that folder's
/// arrow down, running the row's height so the rows' lines join up (as VS Code draws them).
fn indent_guides(depth: usize, cx: &App) -> Vec<AnyElement> {
    (0..depth)
        .map(|level| {
            // from the row's content (an absolute child sits in its parent, inside the row's
            // padding): under the middle of the folder's arrow (12 px wide); through the row's
            // padding above and below (4 px)
            guide_line(level as f32 * TREE_INDENT + 5.5, cx)
                .top(px(-4.0))
                .bottom(px(-4.0))
                .into_any_element()
        })
        .collect()
}

/// How much deeper each level of the outline sits, in px.
const OUTLINE_INDENT: f32 = 20.0;

/// An indent guide `left` px into its row, its full height: the panel's text, faint, so it
/// shows on any panel (the theme's border is the dark panel's own gray).
fn guide_line(left: f32, cx: &App) -> Div {
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(left))
        .w(px(1.0))
        .bg(cx.theme().sidebar_foreground.opacity(0.2))
}

#[cfg(test)]
mod e2e {
    use super::super::Workspace;
    use crate::engine;
    use crate::ui::settings::AppSettings;
    use gpui_kit::{Pixels, TestAppContext, Window, px, size};

    /// How far the file tree and the outline scroll sideways, how wide the tree's list is
    /// against the panel, and how far the list and the outline scroll down, for a folder of `files`.
    fn panel_scroll(cx: &mut TestAppContext, name: &str, files: &[(&str, &str)]) -> [Pixels; 6] {
        let dir = std::env::temp_dir().join(format!("rtd-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (f, text) in files {
            std::fs::write(dir.join(f), text).unwrap();
        }
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        engine::configure("", "/Library/TeX/texbin");
        let (ws, cx) = cx.add_window_view(|window: &mut Window, cx| {
            Workspace::new(dir.clone(), Some("main.tex".into()), window, cx)
        });
        cx.simulate_resize(size(px(1460.0), px(900.0)));
        for _ in 0..3 {
            cx.run_until_parked();
        }
        let r = cx.read(|cx| {
            let ws = ws.read(cx);
            let list = ws
                .tree
                .read(cx)
                .scroll_handle()
                .0
                .borrow()
                .base_handle
                .clone();
            [
                ws.files_scroll.max_offset().x,
                ws.outline_scroll.max_offset().x,
                list.bounds().size.width,
                ws.files_scroll.bounds().size.width,
                list.max_offset().y,
                ws.outline_scroll.max_offset().y,
            ]
        });
        let _ = std::fs::remove_dir_all(&dir);
        r
    }

    /// A long file name and a long section title scroll their lists sideways, the tree's rows
    /// as wide as the longest, and both still scroll down. `cargo test --release
    /// long_names_scroll_sideways -- --ignored --nocapture`.
    #[gpui_kit::test]
    #[ignore]
    fn long_names_scroll_sideways(cx: &mut TestAppContext) {
        let long_title = "A section title far too long for the project panel to show in full";
        let main = format!(
            "\\documentclass{{article}}\n\\begin{{document}}\n\\section{{{long_title}}}\nText.\n{}\\end{{document}}\n",
            "\\section{Short}\n".repeat(60)
        );
        let many: Vec<String> = (0..60).map(|i| format!("f{i:02}.tex")).collect();
        let mut files = vec![
            ("main.tex", main.as_str()),
            (
                "a_file_name_far_too_long_for_the_project_panel_to_show.tex",
                "",
            ),
        ];
        files.extend(many.iter().map(|f| (f.as_str(), "")));
        let [tree_x, outline_x, list_w, panel_w, list_y, outline_y] =
            panel_scroll(cx, "long", &files);
        println!(
            "long: tree x {tree_x:?}, outline x {outline_x:?}, list w {list_w:?}, panel w {panel_w:?}, list y {list_y:?}, outline y {outline_y:?}"
        );
        assert!(tree_x > px(0.0), "the tree scrolls sideways");
        assert!(outline_x > px(0.0), "the outline scrolls sideways");
        assert!(
            list_w > panel_w,
            "the tree's rows run as wide as the longest"
        );
        assert!(list_y > px(0.0), "the tree still scrolls down");
        assert!(outline_y > px(0.0), "the outline still scrolls down");
    }

    /// Short names: the tree fits, nothing to scroll sideways.
    #[gpui_kit::test]
    #[ignore]
    fn short_names_do_not_scroll(cx: &mut TestAppContext) {
        let main = "\\documentclass{article}\n\\begin{document}\n\\section{Intro}\nText.\n\\end{document}\n";
        let [tree_x, outline_x, ..] =
            panel_scroll(cx, "short", &[("main.tex", main), ("a.tex", "")]);
        println!("short: tree x {tree_x:?}, outline x {outline_x:?}");
        assert_eq!(tree_x, px(0.0));
        assert_eq!(outline_x, px(0.0));
    }
}
