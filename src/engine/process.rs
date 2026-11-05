// The `rtex serve` child process: JSON commands on its stdin, a reader thread turning its stdout
// into messages. Also where rtex, its build files and TeX are found.
use super::protocol::{Msg, parse};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

pub struct Rtex {
    child: Child,
    stdin: ChildStdin,
    pub main: String,
}

impl Rtex {
    /// Start `rtex serve` on `main` (relative to `root`); messages go to `tx` from a reader thread.
    pub fn start(
        root: &Path,
        main: &str,
        tx: futures::channel::mpsc::UnboundedSender<Msg>,
    ) -> Result<Self, String> {
        let bin = find_binary().ok_or("rtex not found: install it from the Realtime TeX VS Code extension, put it on PATH, or set RTEX_BIN")?;
        let mut child = Command::new(&bin)
            .args(["serve", "--project"])
            .arg(root)
            .args(["--main", main])
            .arg("--build")
            .arg(build_dir(root, main))
            .current_dir(root)
            .env("PATH", tex_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", bin.display()))?;
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let tail = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let tail2 = tail.clone();
        std::thread::spawn(move || {
            for l in BufReader::new(stderr).lines().map_while(Result::ok) {
                *tail2.lock().unwrap() = l;
            }
        });
        std::thread::spawn(move || {
            let log = std::env::var_os("RTD_LOG").is_some();
            for l in BufReader::new(stdout).lines().map_while(Result::ok) {
                if log {
                    eprintln!("<- {}", &l[..l.len().min(400)]);
                }
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
            main: main.to_string(),
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
        let path = self.main.clone();
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
        let path = self.main.clone();
        self.send(json!({"cmd": "spans", "path": path}));
    }
}

impl Drop for Rtex {
    fn drop(&mut self) {
        self.send(json!({"cmd": "quit"}));
        let _ = self.child.kill();
    }
}

/// The rtex binary and the TeX folders from the settings (read from background threads too).
static CONFIG: std::sync::RwLock<(String, String)> =
    std::sync::RwLock::new((String::new(), String::new()));

pub fn configure(rtex_path: &str, tex_bin: &str) {
    *CONFIG.write().unwrap() = (rtex_path.to_string(), tex_bin.to_string());
}

/// The binary from the settings, else $RTEX_BIN, else `rtex` on PATH, else the newest copy the
/// VS Code extension downloaded.
fn find_binary() -> Option<PathBuf> {
    let configured = CONFIG.read().unwrap().0.clone();
    if !configured.is_empty() {
        return Some(configured.into());
    }
    if let Some(p) = std::env::var_os("RTEX_BIN") {
        return Some(p.into());
    }
    if let Some(p) = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join("rtex"))
            .find(|p| p.is_file())
    }) {
        return Some(p);
    }
    // the copy inside the app (scripts/dmg.sh puts it in Contents/Resources)
    if let Some(p) = std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.join("../Resources/rtex/bin/rtex")))
        .filter(|p| p.is_file())
    {
        return Some(p);
    }
    let home = std::env::var_os("HOME")?;
    let dir = Path::new(&home).join("Library/Application Support/Code/User/globalStorage/henryxiaoyang.realtime-tex/rtex-release");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path().join("bin/rtex"))
        .filter(|p| p.is_file())
        .collect();
    // shortcut: string order of tags (v0.0.6 < v0.0.10 sorts wrong), fine while there are few releases
    versions.sort();
    versions.pop()
}

/// rtex's build files for one main file, kept out of the user's folder:
/// ~/Library/Caches/realtime-tex-desktop/<hash>. Each open file has its own session, so its own folder.
pub fn build_dir(root: &Path, main: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (root, main).hash(&mut h);
    let home = std::env::var_os("HOME").unwrap_or_default();
    Path::new(&home).join(format!(
        "Library/Caches/realtime-tex-desktop/{:016x}",
        h.finish()
    ))
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

/// PATH with MacTeX's bin folder, which a GUI app launched from Finder does not inherit.
pub fn tex_path() -> String {
    let p = std::env::var("PATH").unwrap_or_default();
    let extra = CONFIG.read().unwrap().1.clone();
    format!("{extra}:{p}:/Library/TeX/texbin:/usr/local/bin:/opt/homebrew/bin")
}
