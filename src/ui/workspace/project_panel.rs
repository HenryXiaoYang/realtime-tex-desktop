// The project panel: the file tree and the section outline, one above the other.
use super::*;
use crate::ui::Tip;

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
        let menu_weak = weak.clone();
        let root = self.root.clone();
        let file_tree = tree(&self.tree, move |ix, entry, _selected, _, cx| {
            let item = entry.item();
            let id = item.id.to_string();
            let guides = indent_guides(entry.depth(), entry.is_folder(), cx);
            if id.ends_with(PLACEHOLDER) {
                return ListItem::new(ix).children(guides).child(
                    div()
                        .text_sm()
                        .italic()
                        .text_color(cx.theme().muted_foreground)
                        .pl(px(entry.depth() as f32 * 12.0 + 16.0))
                        .child(item.label.clone()),
                );
            }
            let field = renaming
                .as_ref()
                .filter(|(r, _)| *r == id)
                .map(|(_, state)| state.clone());
            let icon = if entry.is_folder() {
                if entry.is_expanded() {
                    "icons/folder-open.svg"
                } else {
                    "icons/folder.svg"
                }
            } else if id.ends_with(".pdf") || id.ends_with(".png") || id.ends_with(".jpg") {
                "icons/file-image.svg"
            } else if id.ends_with(".tex") {
                "icons/file-text.svg"
            } else {
                "icons/file.svg"
            };
            let chevron = entry.is_folder().then(|| {
                Icon::default()
                    .path(if entry.is_expanded() {
                        "icons/chevron-down.svg"
                    } else {
                        "icons/chevron-right.svg"
                    })
                    .xsmall()
            });
            let weak = weak.clone();
            ListItem::new(ix)
                .children(guides)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_sm()
                        .pl(px(entry.depth() as f32 * 12.0
                            + if entry.is_folder() { 0.0 } else { 16.0 }))
                        .children(chevron)
                        .child(Icon::default().path(icon).xsmall())
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
                                    .child(Input::new(state).xsmall())
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
        let outline =
            outline.into_iter().map(|(off, level, number, title)| {
                let title_copy = title.clone();
                div()
                    .id(("heading", off))
                    .flex()
                    .gap_2()
                    .text_sm()
                    .pl(px(12.0 + level as f32 * 12.0))
                    .py_0p5()
                    .rounded_md()
                    .hover(|s| s.bg(theme.sidebar_accent))
                    .cursor_pointer()
                    .children(number.map(|n| div().text_color(theme.muted_foreground).child(n)))
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
            .child(div().flex_1().min_h_0().px_1().child(file_tree));
        let outline = div()
            .size_full()
            .flex()
            .flex_col()
            .child(header("OUTLINE"))
            .child(
                div()
                    .id("outline")
                    .flex_1()
                    .overflow_y_scroll()
                    .px_1()
                    .children(outline),
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

/// A tree row's indent guides: a thin line for each folder it is in, running the row's height so
/// the rows' lines join up (as VS Code draws them). A folder's arrow sits on the line of the
/// folder it is in: there the line stops, clear of the arrow.
fn indent_guides(depth: usize, folder: bool, cx: &App) -> Vec<AnyElement> {
    // the panel's text, faint: it shows on any panel (the theme's border is the dark panel's
    // own gray)
    let color = cx.theme().sidebar_foreground.opacity(0.2);
    (0..depth)
        .filter(|level| !(folder && level + 1 == depth))
        .map(|level| {
            // from the row's content (an absolute child sits in its parent, inside the row's
            // padding): under the middle of the folder's icon (its arrow 12 px, a 4 px gap, the
            // icon 12 px); through the row's padding above and below (4 px)
            div()
                .absolute()
                .top(px(-4.0))
                .bottom(px(-4.0))
                .left(px(level as f32 * 12.0 + 21.5))
                .w(px(1.0))
                .bg(color)
                .into_any_element()
        })
        .collect()
}
