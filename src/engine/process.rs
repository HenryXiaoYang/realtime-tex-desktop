// The `rtex serve` child process: JSON commands on its stdin, a reader thread turning its stdout
// into messages. Also where rtex, its build files and TeX are found.
use super::protocol::{Msg, parse};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The process groups of the rtex runs going now (each rtex with the LaTeX runs it starts).
static RUNNING: Mutex<Vec<u32>> = Mutex::new(vec![]);

/// Every rtex stopped, with its LaTeX runs (the app is quitting, and its documents may not be
/// dropped before it exits).
pub fn stop_all() {
    for pgid in RUNNING.lock().unwrap().drain(..) {
        kill_group(pgid);
    }
}

/// Process group `pgid` ended: rtex and any lualatex it left running.
#[cfg(unix)]
pub(super) fn kill_group(pgid: u32) {
    let _ = Command::new("/bin/kill")
        .args(["-KILL", "--", &format!("-{pgid}")])
        .stderr(Stdio::null())
        .status();
}

/// Windows has no process groups to signal: rtex's process tree ended instead.
#[cfg(windows)]
pub(super) fn kill_group(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

pub struct Rtex {
    child: Child,
    stdin: ChildStdin,
    /// The file edited (relative to the root): the main file, or one it \input's.
    file: String,
}

impl Rtex {
    /// Start `rtex serve` for editing `file` (relative to `root`): on `file` itself when it is a
    /// document, else on the project's main file, which \input's it. Messages go to `tx` from a
    /// reader thread.
    pub fn start(
        root: &Path,
        file: &str,
        tx: futures::channel::mpsc::UnboundedSender<Msg>,
    ) -> Result<Self, String> {
        let main = main_for(root, file);
        let bin = find_binary().ok_or("rtex isn’t downloaded yet: the app gets it when it starts and is online. Or choose an rtex binary in Settings › Typesetting")?;
        let mut cmd = Command::new(&bin);
        // its own process group, so that rtex and its LaTeX runs end together
        #[cfg(unix)]
        cmd.process_group(0);
        let mut child = cmd
            .args(["serve", "--project"])
            .arg(root)
            .args(["--main", &main])
            .arg("--build")
            .arg(build_dir(root, file))
            .args(SERVE_ARGS.read().unwrap().iter())
            .current_dir(root)
            .env("PATH", tex_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                format!(
                    "{} didn’t run ({e}). Choose another rtex binary in Settings › Typesetting",
                    bin.display()
                )
            })?;
        RUNNING.lock().unwrap().push(child.id());
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let tail = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let tail2 = tail.clone();
        let log_tx = tx.clone();
        std::thread::spawn(move || {
            for l in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = log_tx.unbounded_send(Msg::Log(l.clone()));
                *tail2.lock().unwrap() = l;
            }
        });
        std::thread::spawn(move || {
            let log = std::env::var_os("RTD_LOG").is_some();
            for l in BufReader::new(stdout).lines().map_while(Result::ok) {
                // shortcut: an event cut at 1000 bytes (a layout's display lists run to megabytes),
                // a long diagnostic too; summarise per event if that hides errors
                let short = &l[..l.floor_char_boundary(1000)];
                if log {
                    eprintln!("<- {short}");
                }
                let _ = tx.unbounded_send(Msg::Log(short.to_string()));
                if let Some(m) = parse(&l)
                    && tx.unbounded_send(m).is_err()
                {
                    return;
                }
            }
            let _ = tx.unbounded_send(Msg::Exited(tail.lock().unwrap().clone()));
        });
        Ok(Self {
            child,
            stdin,
            file: file.to_string(),
        })
    }

    fn send(&mut self, v: Value) {
        if std::env::var_os("RTD_LOG").is_some() {
            eprintln!("-> {v}");
        }
        let _ = writeln!(self.stdin, "{v}");
    }

    /// Replace bytes [start, end) of the main file.
    pub fn edit(&mut self, start: usize, end: usize, text: &str) {
        let path = self.file.clone();
        self.send(json!({"cmd": "edit", "path": path, "start": start, "end": end, "text": text}));
    }

    /// Typeset the whole document to a PDF at `out`; answered by a PdfExported event.
    pub fn export_pdf(&mut self, out: &Path) {
        self.send(json!({"cmd": "export_pdf", "out": out}));
    }

    /// Run the full LaTeX compile now; answered by a LayoutUpdate.
    pub fn request_layout(&mut self) {
        self.send(json!({"cmd": "request_layout"}));
    }

    pub fn request_spans(&mut self) {
        let path = self.file.clone();
        self.send(json!({"cmd": "spans", "path": path}));
    }
}

impl Drop for Rtex {
    /// rtex asked to quit, given a moment to, then ended with whatever it started (a killed rtex
    /// used to leave its lualatex runs behind).
    fn drop(&mut self) {
        self.send(json!({"cmd": "quit"}));
        let until = Instant::now() + Duration::from_millis(300);
        while Instant::now() < until && matches!(self.child.try_wait(), Ok(None)) {
            std::thread::sleep(Duration::from_millis(10));
        }
        let pgid = self.child.id();
        kill_group(pgid);
        let _ = self.child.kill();
        let _ = self.child.wait();
        RUNNING.lock().unwrap().retain(|p| *p != pgid);
    }
}

/// The rtex binary and the TeX folders from the settings (read from background threads too).
static CONFIG: std::sync::RwLock<(String, String)> =
    std::sync::RwLock::new((String::new(), String::new()));

pub fn configure(rtex_path: &str, tex_bin: &str) {
    *CONFIG.write().unwrap() = (rtex_path.to_string(), tex_bin.to_string());
}

/// More options for `rtex serve` (its fast-path budget, picture cache, diagnostics folder).
static SERVE_ARGS: std::sync::RwLock<Vec<String>> = std::sync::RwLock::new(vec![]);

pub fn configure_serve(args: Vec<String>) {
    *SERVE_ARGS.write().unwrap() = args;
}

/// The binary from the settings, else $RTEX_BIN, else `rtex` on PATH, else the copy inside the
/// app, else the newest one the app downloaded.
pub(super) fn find_binary() -> Option<PathBuf> {
    let configured = CONFIG.read().unwrap().0.clone();
    if !configured.is_empty() {
        return Some(configured.into());
    }
    if let Some(p) = std::env::var_os("RTEX_BIN") {
        return Some(p.into());
    }
    let exe = format!("rtex{}", std::env::consts::EXE_SUFFIX);
    if let Some(p) = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(&exe))
            .find(|p| p.is_file())
    }) {
        return Some(p);
    }
    // the copy inside the app (scripts/dmg.sh puts it in Contents/Resources)
    if let Some(p) = std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.join("../Resources/rtex/bin").join(&exe)))
        .filter(|p| p.is_file())
    {
        return Some(p);
    }
    super::download::downloaded()
}

/// The project's main file: main.tex, else the first .tex at its top with \documentclass.
pub fn find_main(root: &Path) -> Option<String> {
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
    names.into_iter().find(|n| is_document(&root.join(n)))
}

fn is_document(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|t| t.contains("\\documentclass"))
}

/// The file LaTeX runs to typeset `file`: itself when it is a document, else the main file.
/// shortcut: assumes the main file \input's it; check the \input tree if projects have several
fn main_for(root: &Path, file: &str) -> String {
    if is_document(&root.join(file)) {
        return file.to_string();
    }
    find_main(root).unwrap_or_else(|| file.to_string())
}

/// rtex's build files for one main file, kept out of the user's folder:
/// <cache>/realtime-tex-desktop/<hash>. Each open file has its own session, so its own folder.
pub fn build_dir(root: &Path, main: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (root, main).hash(&mut h);
    cache_dir().join(format!("realtime-tex-desktop/{:016x}", h.finish()))
}

/// Every build folder removed but those in `keep` (the open documents', in use by their rtex).
pub fn clear_build_cache(keep: &[PathBuf]) {
    clear_builds(&cache_dir().join("realtime-tex-desktop"), keep);
}

fn clear_builds(dir: &Path, keep: &[PathBuf]) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        // only what build_dir names: nothing else that may share the folder
        let build = name.len() == 16 && name.bytes().all(|b| b.is_ascii_hexdigit());
        if build && !keep.contains(&e.path()) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

pub(super) fn home() -> PathBuf {
    std::env::home_dir().unwrap_or_default()
}

/// The app's own folder: its settings, its projects list, the rtex it downloaded.
pub fn app_dir() -> PathBuf {
    data_dir().join("Realtime Tex Desktop")
}

/// Where apps keep what they remember: ~/Library/Application Support, %APPDATA%, ~/.config.
fn data_dir() -> PathBuf {
    let env = |k| std::env::var_os(k).map(PathBuf::from);
    if cfg!(target_os = "macos") {
        home().join("Library/Application Support")
    } else if cfg!(windows) {
        env("APPDATA").unwrap_or_else(|| home().join("AppData/Roaming"))
    } else {
        env("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"))
    }
}

/// Where apps keep files they can make again: ~/Library/Caches, %LOCALAPPDATA%, ~/.cache.
fn cache_dir() -> PathBuf {
    let env = |k| std::env::var_os(k).map(PathBuf::from);
    if cfg!(target_os = "macos") {
        home().join("Library/Caches")
    } else if cfg!(windows) {
        env("LOCALAPPDATA").unwrap_or_else(|| home().join("AppData/Local"))
    } else {
        env("XDG_CACHE_HOME").unwrap_or_else(|| home().join(".cache"))
    }
}

/// Where a TeX distribution puts lualatex when it is not on PATH (an app started from the
/// Finder or the desktop does not get the shell's PATH).
pub fn default_tex_bin() -> &'static str {
    if cfg!(target_os = "macos") {
        "/Library/TeX/texbin"
    } else {
        ""
    }
}

/// lualatex, if this PATH has it.
pub fn find_lualatex() -> Option<PathBuf> {
    let exe = format!("lualatex{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(&tex_path())
        .map(|d| d.join(&exe))
        .find(|p| p.is_file())
}

/// Whether rtex is where the app looks.
pub fn rtex_found() -> bool {
    find_binary().is_some_and(|p| p.is_file())
}

/// `kpsewhich <name>`: where TeX finds a file.
pub fn kpsewhich(name: &str) -> Option<PathBuf> {
    let out = Command::new("kpsewhich")
        .arg(name)
        .env("PATH", tex_path())
        .output()
        .ok()?;
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| PathBuf::from(s))
}

/// PATH with the TeX folders from the settings (several: separated as PATH separates them), and
/// on the Mac MacTeX's and Homebrew's, which an app launched from the Finder does not inherit.
pub fn tex_path() -> std::ffi::OsString {
    let extra = CONFIG.read().unwrap().1.clone();
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&extra).collect();
    dirs.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    if cfg!(target_os = "macos") {
        dirs.extend(
            ["/Library/TeX/texbin", "/usr/local/bin", "/opt/homebrew/bin"].map(PathBuf::from),
        );
    }
    // the TeX Live the app installed, last: one of the system's own comes first
    dirs.extend(super::tex_install::own_tex_bin());
    dirs.retain(|d| !d.as_os_str().is_empty());
    std::env::join_paths(dirs).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::clear_builds;

    #[test]
    fn clearing_keeps_the_open_documents_builds_and_what_is_not_a_build() {
        let dir = std::env::temp_dir().join(format!("rtd-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in ["0123456789abcdef", "fedcba9876543210", "texlive"] {
            std::fs::create_dir_all(dir.join(d).join("bg")).unwrap();
        }
        clear_builds(&dir, &[dir.join("fedcba9876543210")]);
        assert!(
            !dir.join("0123456789abcdef").exists(),
            "a closed document's build goes"
        );
        assert!(dir.join("fedcba9876543210").exists(), "an open one's stays");
        assert!(dir.join("texlive").exists(), "not a build folder");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
