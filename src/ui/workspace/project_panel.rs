// The project panel: the file tree and the section outline, one above the other.
use super::*;
use crate::ui::Tip;
use gpui_kit::component::scroll::{ScrollableElement, ScrollbarAxis};

impl Workspace {
    /// Scroll the outline so row `i` is in view, moving it only as far as it must.
    fn show_outline_row(&self, i: usize) {
        let (top, bottom) = (i as f32 * OUTLINE_ROW, (i + 1) as f32 * OUTLINE_ROW);
        let (offset, height) = (
            self.outline_scroll.offset(),
            f32::from(self.outline_scroll.bounds().size.height),
        );
        let from = -f32::from(offset.y);
        let to = if top < from {
            top
        } else if bottom > from + height {
            bottom - height
        } else {
            return;
        };
        self.outline_scroll.set_offset(point(offset.x, px(-to)));
    }

    pub(super) fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
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
        let depth = settings::AppSettings::get(cx).outline_depth;
        let outline: Vec<_> = self
            .doc()
            .map(|d| {
                d.read(cx)
                    .outline
                    .iter()
                    .filter(|h| depth.shows(h.level))
                    .map(|h| (h.offset, h.level, h.number.clone(), h.title.clone()))
                    .collect()
            })
            .unwrap_or_default();
        // the heading the pages are under is marked, and the outline scrolls to it when the
        // reader moves on to another
        let reading = self.doc().and_then(|d| d.read(cx).reading_offset());
        let current =
            reading.and_then(|at| crate::document::heading_at(outline.iter().map(|h| h.0), at));
        if current != self.outline_seen {
            self.outline_seen = current;
            if let Some(i) = current.and_then(|c| outline.iter().position(|h| h.0 == c)) {
                self.show_outline_row(i);
            }
        }
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
                    .h(px(OUTLINE_ROW))
                    .items_center()
                    .rounded_md()
                    .hover(|s| s.bg(theme.sidebar_accent))
                    .when(current == Some(off), |d| {
                        d.bg(theme.sidebar_accent).font_weight(FontWeight::MEDIUM)
                    })
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
        let outline = outline.collect::<Vec<_>>();
        let (files_open, outline_open) = (self.files_open, self.outline_open);
        // a section's title bar: the arrow and the title fold it, the buttons beside are its own
        let header = |id: &'static str, title: &'static str, open: bool, cx: &mut Context<Self>| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .h_8()
                .px_1()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.muted_foreground)
                .child(
                    div()
                        .id(id)
                        .role(Role::Button)
                        .aria_label(format!(
                            "{} {title}",
                            if open { "Collapse" } else { "Expand" }
                        ))
                        .flex()
                        .flex_1()
                        .items_center()
                        .gap_1()
                        .h_full()
                        .px_1()
                        .cursor_pointer()
                        .child(
                            Icon::default()
                                .path(if open {
                                    "icons/chevron-down.svg"
                                } else {
                                    "icons/chevron-right.svg"
                                })
                                .size_3(),
                        )
                        .child(title)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if id == "fold-files" {
                                this.files_open = !this.files_open;
                            } else {
                                this.outline_open = !this.outline_open;
                            }
                            cx.notify();
                        })),
                )
        };
        let files = div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                header("fold-files", "PROJECT", files_open, cx).when(files_open, |h| {
                    h.child(
                        div()
                            .flex()
                            .child(
                                icon_button("new-file", "icons/file-plus.svg", "New File")
                                    .on_click(cx.listener(|this, _, w, cx| {
                                        this.new_entry("", false, w, cx)
                                    })),
                            )
                            .child(
                                icon_button("new-folder", "icons/folder-plus.svg", "New Folder")
                                    .on_click(cx.listener(|this, _, w, cx| {
                                        this.new_entry("", true, w, cx)
                                    })),
                            )
                            .child(
                                icon_button("collapse", "icons/list-collapse.svg", "Collapse All")
                                    .on_click(cx.listener(|this, _, _, cx| this.collapse_tree(cx))),
                            ),
                    )
                }),
            )
            .when(files_open, |d| {
                d.child(
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
                )
            });
        let outline = div()
            .size_full()
            .flex()
            .flex_col()
            .child(header("fold-outline", "OUTLINE", outline_open, cx))
            .when(outline_open, |d| {
                d.child(
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
                )
            });
        // files above, outline below, with a divider to drag between them; when one is folded
        // the other takes the room (its title alone stays)
        let sections = if files_open && outline_open {
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
                )
                .into_any_element()
        } else {
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .when(files_open, |d| d.flex_1().min_h_0())
                        .child(files),
                )
                .child(
                    div()
                        .when(outline_open, |d| d.flex_1().min_h_0())
                        .child(outline),
                )
                .into_any_element()
        };
        div()
            .size_full()
            // the theme's sidebar, set off from the pages
            .bg(crate::ui::palette::panel(&theme))
            .text_color(theme.sidebar_foreground)
            .child(sections)
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

/// How tall each outline row is, in px.
const OUTLINE_ROW: f32 = 24.0;

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

    /// The outline marks the heading the pages are under and scrolls to it as they move: a
    /// jump to a section far down brings its row into view. `cargo test --release
    /// outline_follows_the_pages -- --ignored --nocapture`.
    #[gpui_kit::test]
    #[ignore]
    fn outline_follows_the_pages(cx: &mut TestAppContext) {
        let mut main = String::from("\\documentclass{article}\n\\begin{document}\n");
        for i in 0..40 {
            main.push_str(&format!(
                "\\section{{Part {i}}}\n{}\n\n",
                "Some text. ".repeat(60)
            ));
        }
        main.push_str("\\end{document}\n");
        let dir = std::env::temp_dir().join(format!("rtd-outline-follow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.tex"), &main).unwrap();
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
        let until = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let doc = loop {
            cx.run_until_parked();
            let doc = cx.read(|cx| ws.read(cx).doc().cloned());
            if let Some(d) = doc
                && cx.read(|cx| d.read(cx).page_position().1 >= 3)
            {
                break d;
            }
            assert!(
                std::time::Instant::now() < until,
                "timed out waiting for the layout"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        let heading = |cx: &mut gpui_kit::VisualTestContext, i: usize| {
            cx.read(|cx| doc.read(cx).outline[i].offset)
        };
        for _ in 0..3 {
            cx.run_until_parked();
        }
        let first = heading(cx, 0);
        assert_eq!(cx.read(|cx| ws.read(cx).outline_seen), Some(first));

        let target = heading(cx, 30);
        doc.update(cx, |d, cx| d.jump_to(target, cx));
        for _ in 0..3 {
            cx.run_until_parked();
        }
        cx.read(|cx| {
            let ws = ws.read(cx);
            assert_eq!(ws.outline_seen, Some(target), "marked");
            let (y, h) = (
                -f32::from(ws.outline_scroll.offset().y),
                f32::from(ws.outline_scroll.bounds().size.height),
            );
            assert!(
                y <= 30.0 * super::OUTLINE_ROW && 31.0 * super::OUTLINE_ROW <= y + h,
                "row 30 in view: scrolled {y} of {h}"
            );
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
