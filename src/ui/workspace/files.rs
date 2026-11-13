// The project's files: the project panel's tree, opening files in tabs, and the file commands
// (new, rename, duplicate, trash).
use super::*;

/// A file command on a project path.
type FileAction = dyn Fn(&mut Workspace, String, &mut Window, &mut Context<Workspace>);

impl Workspace {
    pub(super) fn refresh_tree(&mut self, cx: &mut Context<Self>) {
        let items = list_dir(
            &self.root,
            &self.root,
            &self.expanded,
            settings::AppSettings::get(cx),
        );
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
            self.status = format!("Can’t open {path}: it isn’t a text file");
            cx.notify();
            return;
        }
        let (root, resources) = (self.root.clone(), self.resources.clone());
        let doc = cx.new(|cx| Doc::new(root, path, resources, window, cx));
        // what it has to tell (also from opening: rtex not found)
        self.show_notices(&doc, window, cx);
        self._subs
            .push(cx.observe_in(&doc, window, |this, doc, window, cx| {
                this.show_notices(&doc, window, cx);
                cx.notify();
            }));
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
            Err(e) => {
                self.status = format!("Couldn’t create {}: {e}", path.display());
                let name = path
                    .file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().to_string());
                self.notify_failed(
                    format!("Couldn’t create {name}"),
                    crate::ui::plain_error(&e),
                    window,
                    cx,
                );
            }
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
        self._rename_sub = Some(cx.subscribe_in(
            &state,
            window,
            |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { .. } => this.commit_rename(true, window, cx),
                InputEvent::Blur => this.commit_rename(false, window, cx),
                // typing again: the error under the field goes
                InputEvent::Change if this.rename_error.take().is_some() => cx.notify(),
                _ => {}
            },
        ));
        self.rename = Some((rel, state));
        self.rename_error = None;
        cx.notify();
    }

    pub(super) fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.rename = None;
        self.rename_error = None;
        self._rename_sub = None;
        cx.notify();
    }

    /// The name typed taken (Enter: `enter`; else the field was left). A name that cannot be
    /// keeps the field open after Enter, with why under it and the name as typed; leaving the
    /// field then gives the rename up.
    pub(super) fn commit_rename(
        &mut self,
        enter: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((rel, state)) = self.rename.clone() else {
            return;
        };
        let name = state.read(cx).value().trim().to_string();
        let old = self.root.join(&rel);
        let new = old.with_file_name(&name);
        let invalid = if name.contains('/') || name.starts_with('.') {
            Some("A name can’t contain / or start with a dot".to_string())
        } else if new != old && new.exists() {
            Some(format!("{name} is already in this folder"))
        } else {
            None
        };
        if let Some(why) = invalid.clone().filter(|_| enter && !name.is_empty()) {
            self.rename_error = Some(why);
            cx.notify();
            return;
        }
        self.rename = None;
        self.rename_error = None;
        self._rename_sub = None;
        if name.is_empty() || new == old || invalid.is_some() {
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
                    self.status = format!("Couldn’t rename {rel}: {e}");
                    self.notify_failed(
                        format!("Couldn’t rename {rel}"),
                        crate::ui::plain_error(&e),
                        window,
                        cx,
                    );
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
        let failed = match trash_command(&path).output() {
            Ok(o) if o.status.success() => None,
            Ok(o) => Some(String::from_utf8_lossy(&o.stderr).trim().to_string()),
            Err(e) => Some(e.to_string()),
        };
        self.status = match &failed {
            None => format!("Moved {rel} to the Trash"),
            Some(why) => format!("Couldn’t move {rel} to the Trash: {why}"),
        };
        if let Some(why) = failed {
            self.notify_failed(format!("Couldn’t move {rel} to the Trash"), why, window, cx);
        }
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
            self.status = format!("Couldn’t duplicate {rel}: {e}");
            self.notify_failed(
                format!("Couldn’t duplicate {rel}"),
                crate::ui::plain_error(&e),
                window,
                cx,
            );
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
        let shell = |label: &'static str, reveal: bool| {
            let abs = abs.clone();
            item(
                label,
                Rc::new(move |_, _, _, cx| {
                    if reveal {
                        cx.reveal_path(&abs)
                    } else {
                        cx.open_with_system(&abs)
                    }
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
                .item(shell("Open in Default App", false))
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
        .item(shell(REVEAL, true))
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

/// The entries of `dir` the settings do not hide, folders first. Only folders in `expanded` are
/// read; the others get a stand-in child (so they show as folders) until they are opened.
pub(super) fn list_dir(
    root: &Path,
    dir: &Path,
    expanded: &std::collections::HashSet<String>,
    s: &settings::AppSettings,
) -> Vec<TreeItem> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut entries: Vec<_> = rd
        .flatten()
        .filter(|e| !hidden(&e.file_name().to_string_lossy(), s))
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
                list_dir(root, &path, expanded, s)
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

/// Whether the file tree leaves `name` out: a dot file (unless they show), or a name matching
/// one of the comma-separated patterns.
pub(super) fn hidden(name: &str, s: &settings::AppSettings) -> bool {
    let name: Vec<char> = name.chars().collect();
    (name.first() == Some(&'.') && !s.show_hidden_files)
        || s.hide_files
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .any(|p| glob(&p.chars().collect::<Vec<_>>(), &name))
}

/// `name` matches `pat`, where `*` stands for any run of characters and `?` for one.
fn glob(pat: &[char], name: &[char]) -> bool {
    match pat.split_first() {
        None => name.is_empty(),
        Some(('*', rest)) => (0..=name.len()).any(|i| glob(rest, &name[i..])),
        Some((p, rest)) => name
            .split_first()
            .is_some_and(|(n, name)| (*p == '?' || p == n) && glob(rest, name)),
    }
}

/// The id ending of the stand-in row of a folder not read yet (or empty).
pub(super) const PLACEHOLDER: &str = "\u{1}";

/// What the file manager is called here.
const REVEAL: &str = if cfg!(target_os = "macos") {
    "Reveal in Finder"
} else if cfg!(windows) {
    "Show in Explorer"
} else {
    "Show in File Manager"
};

/// The command that moves `path` to the Trash (the Recycle Bin on Windows), recoverably.
fn trash_command(path: &Path) -> std::process::Command {
    if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("/usr/bin/trash");
        c.arg(path);
        c
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("powershell");
        let kind = if path.is_dir() { "Directory" } else { "File" };
        // a quoted PowerShell string: a ' inside is written ''
        let quoted = path.to_string_lossy().replace('\'', "''");
        c.args([
            "-NoProfile",
            "-Command",
            &format!(
                "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::Delete{kind}('{quoted}', 'OnlyErrorDialogs', 'SendToRecycleBin')"
            ),
        ]);
        c
    } else {
        let mut c = std::process::Command::new("gio");
        c.arg("trash").arg(path);
        c
    }
}

#[cfg(test)]
mod tests {
    use super::{PLACEHOLDER, hidden, list_dir};
    use crate::ui::settings::AppSettings;
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
        let d = AppSettings::default();
        let closed = list_dir(&root, &root, &HashSet::new(), &d);
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
        let items = list_dir(&root, &root, &open, &d);
        assert!(items[1].is_expanded());
        assert_eq!(names(&items[1].children), ["figures/a.png"]);
        // an open empty folder shows its "empty" row
        assert!(items[0].children[0].id.ends_with(PLACEHOLDER));
        assert!(!items[2].is_expanded());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_settings_hide_dot_files_and_names_matching_patterns() {
        let d = AppSettings::default();
        assert!(hidden(".git", &d));
        assert!(!hidden("main.aux", &d), "nothing else by default");
        let s = AppSettings {
            show_hidden_files: true,
            hide_files: " *.aux, build ,f?g.*, ".into(),
            ..d
        };
        assert!(!hidden(".git", &s), "dot files shown");
        assert!(hidden("main.aux", &s));
        assert!(hidden("build", &s), "a folder by its name");
        assert!(!hidden("build2", &s), "a pattern matches the whole name");
        assert!(
            hidden("fig.pdf", &s) && hidden("fög.png", &s),
            "? is one character"
        );
        assert!(!hidden("main.tex", &s) && !hidden("figs.pdf", &s));
    }
}

#[cfg(test)]
mod e2e {
    use super::super::Workspace;
    use crate::ui::settings::AppSettings;
    use gpui_kit::{AppContext, TestAppContext, WindowOptions};

    /// A name that cannot be keeps the rename field open after Enter, with why under it; leaving
    /// the field then gives the rename up, and nothing is renamed.
    #[gpui_kit::test]
    fn a_bad_name_is_said_at_the_field(cx: &mut TestAppContext) {
        let dir = std::env::temp_dir().join(format!("rtd-rename-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        std::fs::write(dir.join("b.txt"), "b").unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        let (window, ws) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                    let d = dir.clone();
                    cx.new(|cx| Workspace::new(d, Some("a.txt".into()), window, cx))
                })
            })
            .unwrap();
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |this, cx| {
                this.start_rename("a.txt".into(), window, cx);
                let field = this.rename.clone().unwrap().1;
                field.update(cx, |s, cx| s.set_value("b.txt", window, cx));
                this.commit_rename(true, window, cx);
                assert!(this.rename.is_some(), "the field stays open");
                let why = this.rename_error.clone().unwrap_or_default();
                assert!(why.contains("already"), "{why}");
                this.commit_rename(false, window, cx);
                assert!(
                    this.rename.is_none() && this.rename_error.is_none(),
                    "given up"
                );
            });
        })
        .unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "a");
        assert_eq!(std::fs::read_to_string(dir.join("b.txt")).unwrap(), "b");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
