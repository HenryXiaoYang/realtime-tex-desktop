// The project's files: the project panel's tree, opening files in tabs, and the file commands
// (new, rename, duplicate, trash).
use super::*;

/// A file command on a project path.
type FileAction = dyn Fn(&mut Workspace, String, &mut Window, &mut Context<Workspace>);

impl Workspace {
    pub(super) fn refresh_tree(&mut self, cx: &mut Context<Self>) {
        let items = list_dir(&self.root, &self.root, &self.expanded);
        self.tree.update(cx, |t, cx| t.set_items(items, cx));
    }

    /// Show `path` in a tab, opening it if needed. Binary files (images, PDFs) do not open.
    pub(crate) fn open(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.tabs.iter().position(|d| d.read(cx).path == path) {
            self.activate(ix, cx);
            return;
        }
        // PDFs and images open in a viewer; other binary files do not open
        const BINARY: &[&str] = &["gif", "eps", "svg", "zip", "gz", "synctex"];
        let binary = Path::new(&path)
            .extension()
            .is_some_and(|e| BINARY.iter().any(|b| e.eq_ignore_ascii_case(b)));
        if !viewer::can_view(&path)
            && (binary
                || std::fs::read(self.root.join(&path))
                    .map_or(true, |b| std::str::from_utf8(&b).is_err()))
        {
            self.status = format!("Cannot open {path}: it is not a text file");
            cx.notify();
            return;
        }
        let (root, resources) = (self.root.clone(), self.resources.clone());
        let doc = cx.new(|cx| Doc::new(root, path, resources, window, cx));
        self._subs.push(cx.observe(&doc, |_, _, cx| cx.notify()));
        self.tabs.push(doc);
        self.activate(self.tabs.len() - 1, cx);
    }

    pub(super) fn activate(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.active = ix;
        self.status.clear();
        if let Some(path) = self.doc().map(|d| d.read(cx).path.clone()) {
            self.reveal(&path, cx);
        }
        if let Some(d) = self.doc() {
            let id = SharedString::from(d.read(cx).path.clone());
            self.tree.update(cx, |t, cx| {
                let ix = t.index_of(&id);
                t.set_selected_index(ix, cx)
            });
        }
        cx.notify();
    }

    /// Close a tab; its unsaved edits are saved first, so nothing is lost.
    pub(super) fn close(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if ix >= self.tabs.len() {
            return;
        }
        let doc = self.tabs.remove(ix);
        doc.update(cx, |d, cx| {
            d.save(cx);
            d.release_images(window);
        });
        if self.active > ix || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
        let active = self.active;
        self.activate(active, cx);
    }

    pub fn save_all(&mut self, cx: &mut Context<Self>) {
        for d in &self.tabs {
            d.update(cx, |d, cx| d.save(cx));
        }
    }

    /// A new file or folder in `dir` (relative; "" is the project root), then its name is
    /// edited in place.
    pub(super) fn new_entry(
        &mut self,
        dir: &str,
        folder: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (stem, ext) = if folder {
            ("New Folder", "")
        } else {
            ("untitled", ".tex")
        };
        let parent = self.root.join(dir);
        let path = (1..)
            .map(|n| {
                parent.join(if n == 1 {
                    format!("{stem}{ext}")
                } else {
                    format!("{stem} {n}{ext}")
                })
            })
            .find(|p| !p.exists())
            .unwrap();
        let res = if folder {
            std::fs::create_dir(&path)
        } else {
            std::fs::write(&path, "")
        };
        if !dir.is_empty() {
            self.expanded.insert(dir.to_string());
        }
        self.refresh_tree(cx);
        match res {
            Err(e) => self.status = format!("Could not create {}: {e}", path.display()),
            Ok(()) => {
                if let Some(rel) = self.rel(&path) {
                    self.start_rename(rel, window, cx);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn rel(&self, path: &Path) -> Option<String> {
        Some(
            path.strip_prefix(&self.root)
                .ok()?
                .to_string_lossy()
                .to_string(),
        )
    }

    /// Edit the name of the tree entry at `rel` in place; Enter or a click away renames it,
    /// Escape cancels.
    pub(super) fn start_rename(
        &mut self,
        rel: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = Path::new(&rel)
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().to_string());
        // the name without its extension is selected, as in Finder
        let stem = Path::new(&name).file_stem().map_or(name.len(), |s| s.len());
        let state = cx.new(|cx| InputState::new(window, cx).default_value(name));
        state.update(cx, |s, cx| {
            s.set_selected_range(0..stem, cx);
            s.focus(window, cx);
        });
        self._rename_sub =
            Some(
                cx.subscribe_in(&state, window, |this, _, ev: &InputEvent, window, cx| {
                    if matches!(ev, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                        this.commit_rename(window, cx);
                    }
                }),
            );
        self.rename = Some((rel, state));
        cx.notify();
    }

    pub(super) fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.rename = None;
        self._rename_sub = None;
        cx.notify();
    }

    pub(super) fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((rel, state)) = self.rename.take() else {
            return;
        };
        self._rename_sub = None;
        let name = state.read(cx).value().trim().to_string();
        let old = self.root.join(&rel);
        let new = old.with_file_name(&name);
        if name.is_empty() || new == old {
        } else if name.contains('/') || name.starts_with('.') {
            self.status = format!("“{name}” cannot be a name: no / and no dot at the start");
        } else if new.exists() {
            self.status = format!("{name} already exists here: choose another name");
        } else if let Some(new_rel) = self.rel(&new) {
            // open files under it are saved, closed, and reopened under the new name
            let moved = self.close_under(&rel, true, window, cx);
            match std::fs::rename(&old, &new) {
                Ok(()) => {
                    for p in moved {
                        self.open(format!("{new_rel}{}", &p[rel.len()..]), window, cx);
                    }
                }
                Err(e) => {
                    self.status = format!("Could not rename {rel}: {e}");
                    for p in moved {
                        self.open(p, window, cx);
                    }
                }
            }
        }
        self.refresh_tree(cx);
        cx.notify();
    }

    /// Close the tabs of `rel` and everything under it (saving them, or dropping their edits);
    /// returns their paths.
    pub(super) fn close_under(
        &mut self,
        rel: &str,
        save: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<String> {
        let under = |p: &str| p == rel || p.starts_with(&format!("{rel}/"));
        let mut closed = vec![];
        while let Some(ix) = self.tabs.iter().position(|d| under(&d.read(cx).path)) {
            let doc = self.tabs[ix].clone();
            closed.push(doc.read(cx).path.clone());
            if save {
                self.close(ix, window, cx);
            } else {
                self.tabs.remove(ix);
                doc.update(cx, |d, _| d.release_images(window));
                self.active = self.active.min(self.tabs.len().saturating_sub(1));
            }
        }
        let active = self.active;
        self.activate(active, cx);
        closed
    }

    /// Move to the Trash (recoverable), closing its tabs without saving.
    pub(super) fn trash(&mut self, rel: String, window: &mut Window, cx: &mut Context<Self>) {
        self.close_under(&rel, false, window, cx);
        let path = self.root.join(&rel);
        self.status = match std::process::Command::new("/usr/bin/trash")
            .arg(&path)
            .output()
        {
            Ok(o) if o.status.success() => format!("Moved {rel} to the Trash"),
            Ok(o) => format!(
                "Could not move {rel} to the Trash: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            ),
            Err(e) => format!("Could not move {rel} to the Trash: {e}"),
        };
        self.refresh_tree(cx);
        cx.notify();
    }

    /// A copy next to the file: “name copy.ext”, “name copy 2.ext”…
    pub(super) fn duplicate(&mut self, rel: String, window: &mut Window, cx: &mut Context<Self>) {
        self.save_all(cx);
        let path = self.root.join(&rel);
        let stem = path
            .file_stem()
            .map_or(String::new(), |s| s.to_string_lossy().to_string());
        let ext = path
            .extension()
            .map_or(String::new(), |e| format!(".{}", e.to_string_lossy()));
        let copy = (1..)
            .map(|n| {
                path.with_file_name(if n == 1 {
                    format!("{stem} copy{ext}")
                } else {
                    format!("{stem} copy {n}{ext}")
                })
            })
            .find(|p| !p.exists())
            .unwrap();
        if let Err(e) = std::fs::copy(&path, &copy) {
            self.status = format!("Could not duplicate {rel}: {e}");
        }
        self.refresh_tree(cx);
        if let Some(r) = self.rel(&copy) {
            self.start_rename(r, window, cx);
        }
    }

    /// The right-click menu of a tree entry.
    pub(super) fn tree_menu(
        weak: &WeakEntity<Self>,
        root: &Path,
        rel: &str,
        folder: bool,
        menu: PopupMenu,
    ) -> PopupMenu {
        let item = |label: &'static str, f: Rc<FileAction>| {
            let (weak, rel) = (weak.clone(), rel.to_string());
            PopupMenuItem::new(label).on_click(move |_, window, cx| {
                let rel = rel.clone();
                let _ = weak.update(cx, |this, cx| f(this, rel, window, cx));
            })
        };
        let abs = root.join(rel);
        let shell = |label: &'static str, args: &'static [&'static str]| {
            let abs = abs.clone();
            item(
                label,
                Rc::new(move |_, _, _, _| {
                    let _ = std::process::Command::new("open")
                        .args(args)
                        .arg(&abs)
                        .spawn();
                }),
            )
        };
        let copy = |label: &'static str, text: String| {
            item(
                label,
                Rc::new(move |_, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))
                }),
            )
        };
        // new entries go into a folder, or next to a file
        let dir = if folder {
            rel.to_string()
        } else {
            Path::new(rel)
                .parent()
                .map_or(String::new(), |p| p.to_string_lossy().to_string())
        };
        let (d1, d2) = (dir.clone(), dir);
        let mut menu = menu;
        if !folder {
            menu = menu
                .item(item(
                    "Open",
                    Rc::new(|this, rel, w, cx| this.open(rel, w, cx)),
                ))
                .item(shell("Open in Default App", &[]))
                .separator();
        }
        menu.item(item(
            "New File",
            Rc::new(move |this, _, w, cx| this.new_entry(&d1, false, w, cx)),
        ))
        .item(item(
            "New Folder",
            Rc::new(move |this, _, w, cx| this.new_entry(&d2, true, w, cx)),
        ))
        .separator()
        .item(shell("Reveal in Finder", &["-R"]))
        .item(copy("Copy Path", abs.to_string_lossy().to_string()))
        .item(copy("Copy Relative Path", rel.to_string()))
        .separator()
        .item(item(
            "Rename…",
            Rc::new(|this, rel, w, cx| this.start_rename(rel, w, cx)),
        ))
        .item(
            item(
                "Duplicate",
                Rc::new(|this, rel, w, cx| this.duplicate(rel, w, cx)),
            )
            .disabled(folder),
        )
        .item(item(
            "Move to Trash",
            Rc::new(|this, rel, w, cx| this.trash(rel, w, cx)),
        ))
    }

    pub(super) fn collapse_tree(&mut self, cx: &mut Context<Self>) {
        self.expanded.clear();
        self.refresh_tree(cx);
    }

    /// Open the folders on the way to `rel`, so its entry shows.
    pub(super) fn reveal(&mut self, rel: &str, cx: &mut Context<Self>) {
        let mut dir = Path::new(rel).parent();
        let mut added = false;
        while let Some(d) = dir.filter(|d| !d.as_os_str().is_empty()) {
            added |= self.expanded.insert(d.to_string_lossy().to_string());
            dir = d.parent();
        }
        if added {
            self.refresh_tree(cx);
        }
    }
}

/// The folder's main file: main.tex, else the first .tex file with \documentclass.
pub(super) fn find_main(root: &Path) -> Option<String> {
    if root.join("main.tex").is_file() {
        return Some("main.tex".into());
    }
    let mut names: Vec<String> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".tex"))
        .collect();
    names.sort();
    names.into_iter().find(|n| {
        std::fs::read_to_string(root.join(n)).is_ok_and(|t| t.contains("\\documentclass"))
    })
}

/// The entries of `dir`, folders first. Only folders in `expanded` are read; the others get a
/// stand-in child (so they show as folders) until they are opened.
pub(super) fn list_dir(
    root: &Path,
    dir: &Path,
    expanded: &std::collections::HashSet<String>,
) -> Vec<TreeItem> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut entries: Vec<_> = rd
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .collect();
    // folders first, like Zed's project panel
    entries.sort_by_key(|e| (!e.path().is_dir(), e.file_name()));
    entries
        .into_iter()
        .map(|e| {
            let path = e.path();
            let id = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            let item = TreeItem::new(id.clone(), e.file_name().to_string_lossy().to_string());
            if !path.is_dir() {
                return item;
            }
            let open = expanded.contains(&id);
            let kids = if open {
                list_dir(root, &path, expanded)
            } else {
                vec![]
            };
            // an empty folder still shows as one: a muted "empty" row inside it
            let kids = if kids.is_empty() {
                vec![TreeItem::new(format!("{id}/{PLACEHOLDER}"), "empty").disabled(true)]
            } else {
                kids
            };
            item.children(kids).expanded(open)
        })
        .collect()
}

/// The id ending of the stand-in row of a folder not read yet (or empty).
pub(super) const PLACEHOLDER: &str = "\u{1}";

#[cfg(test)]
mod tests {
    use super::{PLACEHOLDER, list_dir};
    use std::collections::HashSet;

    #[test]
    fn folders_are_read_only_when_open() {
        let root = std::env::temp_dir().join("rtd-tree-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        std::fs::create_dir_all(root.join("figures")).unwrap();
        std::fs::create_dir_all(root.join("empty")).unwrap();
        std::fs::write(root.join("figures/a.png"), "").unwrap();
        std::fs::write(root.join("main.tex"), "").unwrap();
        let names = |items: &[gpui_kit::component::tree::TreeItem]| {
            items.iter().map(|i| i.id.to_string()).collect::<Vec<_>>()
        };
        let closed = list_dir(&root, &root, &HashSet::new());
        assert_eq!(
            names(&closed),
            ["empty", "figures", "node_modules", "main.tex"]
        );
        // closed folders hold only their stand-in, nothing read from disk
        for f in &closed[..3] {
            assert!(f.is_folder() && !f.is_expanded());
            assert_eq!(f.children.len(), 1);
            assert!(f.children[0].id.ends_with(PLACEHOLDER));
        }
        let open: HashSet<String> = ["figures".to_string(), "empty".to_string()].into();
        let items = list_dir(&root, &root, &open);
        assert!(items[1].is_expanded());
        assert_eq!(names(&items[1].children), ["figures/a.png"]);
        // an open empty folder shows its "empty" row
        assert!(items[0].children[0].id.ends_with(PLACEHOLDER));
        assert!(!items[2].is_expanded());
        let _ = std::fs::remove_dir_all(&root);
    }
}
