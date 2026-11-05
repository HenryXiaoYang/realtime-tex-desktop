// The right-click menu on the pages and its Insert templates.
use super::*;

/// Starters for the Insert menu; MARK is where the caret goes.
pub(super) const TEMPLATES: &[(&str, &str)] = &[
    ("Section", "\\section{\u{E000}}"),
    ("Subsection", "\\subsection{\u{E000}}"),
    ("Equation", "\\begin{equation}\n  \u{E000}\n\\end{equation}"),
    ("Display Math", "\\[\n  \u{E000}\n\\]"),
    (
        "Bulleted List",
        "\\begin{itemize}\n  \\item \u{E000}\n\\end{itemize}",
    ),
    (
        "Numbered List",
        "\\begin{enumerate}\n  \\item \u{E000}\n\\end{enumerate}",
    ),
    (
        "Figure",
        "\\begin{figure}[h]\n  \\centering\n  \\includegraphics[width=0.8\\linewidth]{\u{E000}}\n  \\caption{}\n\\end{figure}",
    ),
    (
        "Table",
        "\\begin{table}[h]\n  \\centering\n  \\begin{tabular}{ll}\n    \u{E000} & \\\\\n  \\end{tabular}\n  \\caption{}\n\\end{table}",
    ),
];

impl Doc {
    /// The right-click menu on the pages: acting on the block clicked (found again by its
    /// start when an item runs, since the blocks may have changed meanwhile), or new text at the
    /// empty space clicked.
    pub(super) fn page_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
        type Act = Rc<dyn Fn(&mut Doc, usize, &mut Window, &mut Context<Doc>)>;
        let (weak, target) = (cx.entity().downgrade(), self.menu_target.clone());
        move |menu, window, cx| {
            let Some((page, y, off)) = target.get() else {
                return menu;
            };
            // `f` gets the block's index; in empty space, usize::MAX
            let item = {
                let weak = weak.clone();
                move |label: &'static str, f: Act| {
                    let weak = weak.clone();
                    PopupMenuItem::new(label).on_click(move |_, window, cx| {
                        let _ = weak.update(cx, |d, cx| match off {
                            Some(o) => {
                                if let Some(ix) = d.block_at(o) {
                                    f(d, ix, window, cx)
                                }
                            }
                            None => f(d, usize::MAX, window, cx),
                        });
                    })
                }
            };
            let spot = move |ix: usize| {
                if ix == usize::MAX {
                    Spot::Gap(page, y)
                } else {
                    Spot::Below(ix)
                }
            };
            let clip = cx
                .read_from_clipboard()
                .and_then(|c| c.text())
                .filter(|t| !t.is_empty());
            let paste = {
                let empty = clip.is_none();
                let clip = clip.unwrap_or_default();
                item(
                    if off.is_some() {
                        "Paste Below"
                    } else {
                        "Paste Here"
                    },
                    Rc::new(move |d, ix, w, cx| d.insert_at(spot(ix), &clip, w, cx)),
                )
                .disabled(empty)
            };
            let templates = {
                let item = item.clone();
                move |mut m: PopupMenu, _: &mut Window, _: &mut Context<PopupMenu>| {
                    for (i, (label, text)) in TEMPLATES.iter().enumerate() {
                        if i == 2 || i == 4 || i == 6 {
                            m = m.separator();
                        }
                        m = m.item(item(
                            label,
                            Rc::new(move |d, ix, w, cx| d.insert_at(spot(ix), text, w, cx)),
                        ));
                    }
                    m
                }
            };
            if off.is_none() {
                return menu
                    .item(item(
                        "Insert Paragraph Here",
                        Rc::new(move |d, ix, w, cx| d.insert_at(spot(ix), "", w, cx)),
                    ))
                    .submenu("Insert", window, cx, templates)
                    .separator()
                    .item(paste);
            }
            // the clicked element's settings, in the sidebar (right-click selected it)
            let kind = weak.upgrade().and_then(|d| {
                let d = d.read(cx);
                let r = d.blocks.get(d.block_at(off?)?)?.range.clone();
                let el = document::element::element_at(&d.source, Some(r.clone()), r.start);
                (el.kind != document::element::Kind::Document).then_some(el.kind)
            });
            let menu = match kind {
                Some(k) => menu
                    .item(
                        PopupMenuItem::new(format!("{} Settings", k.title())).on_click(
                            |_, _, cx| AppSettings::set(cx, |s| s.inspector_closed = false),
                        ),
                    )
                    .separator(),
                None => menu,
            };
            menu.item(item(
                "Edit Source",
                Rc::new(|d, ix, w, cx| {
                    let (lo, hi) = d.selection_with(ix);
                    d.activate_range(lo, hi, w, cx)
                }),
            ))
            .item(item(
                "Show in Source",
                Rc::new(|d, ix, w, cx| {
                    let (lo, hi) = d.selection_with(ix);
                    d.show_in_source(lo, hi, w, cx)
                }),
            ))
            .separator()
            // these act on all selected blocks when the clicked one is among them
            .item(item(
                "Cut",
                Rc::new(|d, ix, w, cx| {
                    let (lo, hi) = d.selection_with(ix);
                    d.copy_blocks(lo, hi, cx);
                    d.delete_blocks(lo, hi, w, cx);
                }),
            ))
            .item(item(
                "Copy",
                Rc::new(|d, ix, _, cx| {
                    let (lo, hi) = d.selection_with(ix);
                    d.copy_blocks(lo, hi, cx)
                }),
            ))
            .item(paste)
            .item(item(
                "Duplicate",
                Rc::new(|d, ix, _, cx| {
                    let (lo, hi) = d.selection_with(ix);
                    d.duplicate_blocks(lo, hi, cx)
                }),
            ))
            .item(item(
                "Delete",
                Rc::new(|d, ix, w, cx| {
                    let (lo, hi) = d.selection_with(ix);
                    d.delete_blocks(lo, hi, w, cx)
                }),
            ))
            .separator()
            .item(item(
                "Move Up",
                Rc::new(|d, ix, _, cx| {
                    let (lo, hi) = d.selection_with(ix);
                    d.move_blocks(lo, hi, true, cx)
                }),
            ))
            .item(item(
                "Move Down",
                Rc::new(|d, ix, _, cx| {
                    let (lo, hi) = d.selection_with(ix);
                    d.move_blocks(lo, hi, false, cx)
                }),
            ))
            .separator()
            .item(item(
                "Insert Paragraph Above",
                Rc::new(|d, ix, w, cx| d.insert_at(Spot::Above(ix), "", w, cx)),
            ))
            .item(item(
                "Insert Paragraph Below",
                Rc::new(|d, ix, w, cx| d.insert_at(Spot::Below(ix), "", w, cx)),
            ))
            .submenu("Insert Below", window, cx, templates)
        }
    }
}
