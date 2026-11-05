// The projects (folders) opened before, newest first, kept in projects.json beside the
// settings; each is marked open while its window is, and stays so when the app quits. At launch
// the app opens again those still open; with none, the welcome window lists them all.
use crate::ui::settings::support_dir;
use crate::ui::welcome;
use crate::ui::workspace::{MIN_WINDOW, TITLE_BAR_H, Workspace};
use gpui_kit::component::TitleBar;
use gpui_kit::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

actions!(realtime_tex, [NewProject, OpenProject, CloseProject]);

/// A project opened before.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Recent {
    pub root: PathBuf,
    /// The file it last showed, relative to its folder.
    #[serde(default)]
    pub file: Option<String>,
    /// Its window was open when the app last quit (or is open now).
    #[serde(default)]
    pub open: bool,
}

/// The recent projects, and the windows open now.
#[derive(Default)]
pub struct Projects {
    pub list: Vec<Recent>,
    /// Each project window: its folder, the window, its workspace.
    windows: Vec<(PathBuf, AnyWindowHandle, WeakEntity<Workspace>)>,
    welcome: Option<AnyWindowHandle>,
}

impl Global for Projects {}

/// How many projects the list keeps.
const KEPT: usize = 30;

fn file() -> PathBuf {
    support_dir().join("projects.json")
}

impl Projects {
    fn save(&self) {
        // tests open projects too: never the real file
        if cfg!(test) {
            return;
        }
        let _ = std::fs::create_dir_all(support_dir());
        let _ = std::fs::write(
            file(),
            serde_json::to_string_pretty(&self.list).unwrap_or_default(),
        );
    }
}

/// Read the list, and handle the project commands and the app's quitting.
pub fn init(cx: &mut App) {
    let list = match std::fs::read_to_string(file()) {
        Ok(t) => serde_json::from_str(&t).unwrap_or_else(|_| {
            // unreadable: kept aside, not written over by the next save
            let _ = std::fs::rename(file(), file().with_extension("json.bad"));
            Vec::new()
        }),
        Err(_) => Vec::new(),
    };
    cx.set_global(Projects {
        list,
        ..Default::default()
    });
    cx.on_action(|_: &NewProject, cx| new_project(cx));
    cx.on_action(|_: &OpenProject, cx| choose_and_open(cx));
    cx.on_action(|_: &CloseProject, cx| {
        let active = cx.active_window();
        let found = cx
            .global::<Projects>()
            .windows
            .iter()
            .find(|w| Some(w.1) == active)
            .cloned();
        if let Some((root, handle, ws)) = found {
            closing(&root, &ws, cx);
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    });
    // what each window shows is remembered, and nothing typed is lost; they stay open
    cx.on_app_quit(|cx| {
        let windows = cx.global::<Projects>().windows.clone();
        for (root, _, ws) in windows {
            let shown = ws
                .update(cx, |ws, cx| {
                    ws.save_all(cx);
                    ws.shown_file(cx)
                })
                .ok()
                .flatten();
            record(&mut cx.global_mut::<Projects>().list, &root, shown, true);
        }
        cx.global::<Projects>().save();
        async {}
    })
    .detach();
}

/// The app launched again (its Dock icon clicked) with no window open: the welcome window.
pub fn reopened(cx: &mut App) {
    if cx.windows().is_empty() {
        show_welcome(cx);
    }
}

/// At launch: `arg` (a folder or a .tex file), else the projects open when the app quit, else
/// the welcome window.
pub fn start(arg: Option<PathBuf>, cx: &mut App) {
    if let Some(p) = arg {
        return open(&p, cx);
    }
    let again = to_reopen(&cx.global::<Projects>().list);
    if again.is_empty() {
        return show_welcome(cx);
    }
    // the newest last, so its window is in front and it stays first in the list
    for r in again.into_iter().rev() {
        open_in(r.root, r.file, cx);
    }
}

/// Open `path`: a folder, or a .tex file in its folder. A project open already comes to the
/// front.
pub fn open(path: &Path, cx: &mut App) {
    let path = path.canonicalize().unwrap_or(path.to_path_buf());
    if path.is_file() {
        let root = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let name = path.file_name().map(|n| n.to_string_lossy().to_string());
        open_in(root, name, cx)
    } else {
        open_in(path, None, cx)
    }
}

/// Ask for a folder or a .tex file, and open it.
pub fn choose_and_open(cx: &mut App) {
    let rx = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: true,
        multiple: false,
        prompt: Some("Open".into()),
    });
    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(paths))) = rx.await
            && let Some(p) = paths.first().cloned()
        {
            cx.update(|cx| open(&p, cx));
        }
    })
    .detach();
}

/// Ask where a new project goes, make its folder with a main.tex, and open it.
pub fn new_project(cx: &mut App) {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    // beside the newest project, else in Documents
    let dir = cx
        .global::<Projects>()
        .list
        .first()
        .and_then(|r| r.root.parent().map(Path::to_path_buf))
        .filter(|d| d.is_dir())
        .unwrap_or_else(|| home.join("Documents"));
    let rx = cx.prompt_for_new_path(&dir, Some("Untitled"));
    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(path))) = rx.await {
            cx.update(|cx| match create(&path) {
                Ok(()) => open_in(path, Some("main.tex".into()), cx),
                Err(e) => welcome::report(format!("Could not create the project: {e}"), cx),
            });
        }
    })
    .detach();
}

/// A project's folder at `path`, with a main.tex to start from (one there already is kept).
fn create(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    let main = path.join("main.tex");
    if !main.exists() {
        let name = path
            .file_name()
            .map_or("Untitled".into(), |n| n.to_string_lossy().to_string());
        std::fs::write(main, template(&name))?;
    }
    Ok(())
}

/// A new project's main.tex, titled `name`.
fn template(name: &str) -> String {
    let title = crate::document::caret::escape(name);
    format!(
        "\\documentclass[12pt]{{article}}\n\\usepackage[margin=1in]{{geometry}}\n\n\\title{{{title}}}\n\\author{{}}\n\\date{{\\today}}\n\n\\begin{{document}}\n\\maketitle\n\n\\section{{Introduction}}\n\nStart writing here.\n\n\\end{{document}}\n"
    )
}

/// Open project `root` (showing `file`, else the file it showed last) in a window of its own,
/// or bring its window to the front.
fn open_in(root: PathBuf, file: Option<String>, cx: &mut App) {
    let open = cx
        .global::<Projects>()
        .windows
        .iter()
        .find(|w| w.0 == root)
        .cloned();
    if let Some((_, handle, ws)) = open {
        let _ = handle.update(cx, |_, window, cx| {
            window.activate_window();
            if let Some(f) = file {
                let _ = ws.update(cx, |ws, cx| ws.open(f, window, cx));
            }
        });
        return;
    }
    if !root.is_dir() {
        return welcome::report(format!("{} is not there any more", root.display()), cx);
    }
    // the file it showed last, while it is still there
    let file = file.or_else(|| {
        let list = &cx.global::<Projects>().list;
        list.iter()
            .find(|r| r.root == root)
            .and_then(|r| r.file.clone())
            .filter(|f| root.join(f).is_file())
    });
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
    let at = root.clone();
    let title = root
        .file_name()
        .map_or(root.to_string_lossy(), |n| n.to_string_lossy())
        .to_string();
    let opened = gpui_kit::open_window(options, cx, move |window, cx| {
        // for the Window menu, Mission Control and VoiceOver
        window.set_window_title(&title);
        let ws = cx.new(|cx| Workspace::new(at.clone(), file, window, cx));
        let weak = ws.downgrade();
        // nothing is lost on close: unsaved edits are written to their files
        window.on_window_should_close(cx, move |_, cx| {
            closing(&at, &weak, cx);
            true
        });
        ws
    });
    let Ok((handle, ws)) = opened else {
        return welcome::report("Could not open a window".into(), cx);
    };
    let p = cx.global_mut::<Projects>();
    p.windows.push((root.clone(), handle, ws.downgrade()));
    let shown = ws.read(cx).shown_file(cx);
    let p = cx.global_mut::<Projects>();
    record(&mut p.list, &root, shown, true);
    p.save();
    // a project open: the welcome window has done its work
    if let Some(w) = p.welcome.take() {
        let _ = w.update(cx, |_, window, _| window.remove_window());
    }
}

/// Project `root`'s window closing: its edits saved, it marked closed with the file it shows;
/// the last one gone, the welcome window.
fn closing(root: &Path, ws: &WeakEntity<Workspace>, cx: &mut App) {
    let shown = ws
        .update(cx, |ws, cx| {
            ws.save_all(cx);
            ws.shown_file(cx)
        })
        .ok()
        .flatten();
    let p = cx.global_mut::<Projects>();
    p.windows.retain(|w| w.0 != root);
    if let Some(r) = p.list.iter_mut().find(|r| r.root == root) {
        r.open = false;
        r.file = shown.or(r.file.take());
    }
    p.save();
    if p.windows.is_empty() {
        cx.defer(show_welcome);
    }
}

/// The welcome window, in front (opened if it is not).
pub fn show_welcome(cx: &mut App) {
    if let Some(w) = cx.global::<Projects>().welcome
        && w.update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    if let Some(w) = welcome::open_window(cx) {
        cx.global_mut::<Projects>().welcome = Some(w);
    }
}

/// The window closing is the welcome window: forget it.
pub(crate) fn welcome_closed(cx: &mut App) {
    cx.global_mut::<Projects>().welcome = None;
}

/// Take `root` out of the list.
pub fn forget(root: &Path, cx: &mut App) {
    let p = cx.global_mut::<Projects>();
    p.list.retain(|r| r.root != root);
    p.save();
}

/// `root` first in `list`, open or not, showing `file` (else what it showed before).
fn record(list: &mut Vec<Recent>, root: &Path, file: Option<String>, open: bool) {
    let before = list
        .iter()
        .position(|r| r.root == root)
        .map(|i| list.remove(i));
    list.insert(
        0,
        Recent {
            root: root.to_path_buf(),
            file: file.or(before.and_then(|r| r.file)),
            open,
        },
    );
    list.truncate(KEPT);
}

/// The projects to open again at launch: those open when the app quit, still there.
fn to_reopen(list: &[Recent]) -> Vec<Recent> {
    list.iter()
        .filter(|r| r.open && r.root.is_dir())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{KEPT, Recent, create, record, to_reopen};
    use std::path::{Path, PathBuf};

    fn r(root: &str, open: bool) -> Recent {
        Recent {
            root: root.into(),
            file: None,
            open,
        }
    }

    #[test]
    fn the_newest_comes_first_once_and_keeps_its_file() {
        let mut list = vec![r("/a", false), r("/b", false)];
        list[1].file = Some("ch1.tex".into());
        record(&mut list, Path::new("/b"), None, true);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].root, PathBuf::from("/b"));
        assert!(list[0].open);
        assert_eq!(list[0].file.as_deref(), Some("ch1.tex"));
        record(&mut list, Path::new("/b"), Some("main.tex".into()), true);
        assert_eq!(list[0].file.as_deref(), Some("main.tex"));
        for i in 0..40 {
            record(&mut list, Path::new(&format!("/p{i}")), None, false);
        }
        assert_eq!(list.len(), KEPT);
    }

    #[test]
    fn launch_opens_what_was_open_and_is_still_there() {
        let here = std::env::temp_dir();
        let list = vec![
            r(here.to_str().unwrap(), true),
            r("/no/such/folder", true),
            r("/", false),
        ];
        let again = to_reopen(&list);
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].root, here);
    }

    #[test]
    fn a_new_project_starts_from_a_titled_document() {
        let dir = std::env::temp_dir().join(format!("rtd-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("Essay #1");
        create(&path).unwrap();
        let tex = std::fs::read_to_string(path.join("main.tex")).unwrap();
        assert!(tex.contains("\\title{Essay \\#1}"));
        assert!(tex.contains("\\begin{document}") && tex.contains("\\end{document}"));
        // one there already is kept
        std::fs::write(path.join("main.tex"), "mine").unwrap();
        create(&path).unwrap();
        assert_eq!(
            std::fs::read_to_string(path.join("main.tex")).unwrap(),
            "mine"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
