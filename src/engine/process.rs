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
    let all = std::mem::take(&mut *RUNNING.lock().unwrap());
    for pgid in all {
        kill_group(pgid);
        forget(pgid);
    }
}

/// rtex's group `pgid` started for the build folder `build`: ended by `stop_all` when the
/// process exits (the test binary too, whose documents are never dropped), and recorded on disk
/// so that the next launch ends it after a crash.
#[cfg_attr(not(unix), allow(unused_variables))]
fn track(pgid: u32, build: &Path) {
    RUNNING.lock().unwrap().push(pgid);
    #[cfg(unix)]
    {
        extern "C" fn at_exit() {
            stop_all();
        }
        static AT_EXIT: std::sync::Once = std::sync::Once::new();
        AT_EXIT.call_once(|| unsafe {
            libc::atexit(at_exit);
        });
        let _ = std::fs::create_dir_all(groups_dir());
        let _ = std::fs::write(
            groups_dir().join(pgid.to_string()),
            format!("{}\n{}", std::process::id(), build.display()),
        );
    }
}

/// rtex's group `pgid` ended.
fn forget(pgid: u32) {
    RUNNING.lock().unwrap().retain(|p| *p != pgid);
    #[cfg(unix)]
    let _ = std::fs::remove_file(groups_dir().join(pgid.to_string()));
}

/// A file per rtex group running, named for the group, holding the app's pid and the build
/// folder.
#[cfg(unix)]
fn groups_dir() -> PathBuf {
    cache_dir().join("realtime-tex-desktop/groups")
}

/// SIGTERM, SIGINT and SIGHUP end the app through `exit`, whose hook stops every rtex (their
/// default ends it at once, leaving rtex's LaTeX runs behind).
#[cfg(unix)]
pub fn exit_on_signals() {
    use std::io::Read;
    use std::os::fd::IntoRawFd;
    use std::sync::atomic::{AtomicI32, Ordering};
    static WAKE: AtomicI32 = AtomicI32::new(-1);
    // a signal handler may only write to a socket: the exit, its locks and its allocations, run
    // on a thread
    extern "C" fn on_signal(sig: libc::c_int) {
        let b = sig as u8;
        unsafe { libc::write(WAKE.load(Ordering::Relaxed), (&raw const b).cast(), 1) };
    }
    let Ok((tx, mut rx)) = std::os::unix::net::UnixStream::pair() else {
        return;
    };
    WAKE.store(tx.into_raw_fd(), Ordering::Relaxed);
    std::thread::spawn(move || {
        let mut sig = [0u8];
        if rx.read_exact(&mut sig).is_ok() {
            std::process::exit(128 + sig[0] as i32);
        }
    });
    for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        unsafe { libc::signal(sig, on_signal as *const () as libc::sighandler_t) };
    }
}

/// The rtex groups an app that is gone left running (it was killed or crashed), ended: only
/// their processes still working in the recorded build folder, never one that took a group's
/// id since.
#[cfg(unix)]
pub(super) fn end_leftover_groups() {
    end_groups_in(&groups_dir());
}

#[cfg(unix)]
fn end_groups_in(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let ps = Command::new("/bin/ps")
        .args(["-axo", "pid=,pgid=,command="])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    for e in rd.flatten() {
        let record = std::fs::read_to_string(e.path()).unwrap_or_default();
        let pgid = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok());
        let app = record
            .split_once('\n')
            .and_then(|(app, build)| Some((app.parse::<i32>().ok()?, build)));
        let (Some(pgid), Some((app, build))) = (pgid, app) else {
            let _ = std::fs::remove_file(e.path());
            continue;
        };
        if app > 0 && unsafe { libc::kill(app, 0) } == 0 {
            continue;
        }
        if Path::new(build).is_absolute() {
            for l in ps.lines() {
                let mut ids = l.split_whitespace().map(|n| n.parse::<i32>());
                if let (Some(Ok(pid)), Some(Ok(g))) = (ids.next(), ids.next())
                    && g as u32 == pgid
                    && pid > 1
                    && l.contains(build)
                {
                    unsafe { libc::kill(pid, libc::SIGKILL) };
                }
            }
        }
        let _ = std::fs::remove_file(e.path());
    }
}

/// Process group `pgid` ended: rtex and any lualatex it left running.
#[cfg(unix)]
pub(super) fn kill_group(pgid: u32) {
    // -1 and -0 would name every process and the app's own group
    if pgid > 1 {
        unsafe { libc::kill(-(pgid as i32), libc::SIGKILL) };
    }
}

/// Windows has no process groups to signal: rtex's process tree ended instead.
#[cfg(windows)]
pub(super) fn kill_group(pid: u32) {
    let _ = command("taskkill")
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
        let build = build_dir(root, file);
        let mut child = cmd
            .args(["serve", "--project"])
            .arg(root)
            .args(["--main", &main])
            .arg("--build")
            .arg(&build)
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
        track(child.id(), &build);
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
        forget(pgid);
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Os {
    Mac,
    Windows,
    Linux,
}

impl Os {
    pub(super) const THIS: Os = if cfg!(target_os = "macos") {
        Os::Mac
    } else if cfg!(windows) {
        Os::Windows
    } else {
        Os::Linux
    };
}

/// `name` as this system names a program: lualatex, lualatex.exe.
pub(super) fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// `program`, without a console window of its own on Windows.
pub(super) fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// `path` (created if need be) locked for this process alone while the file is kept; none while
/// another holds it. The system frees it when the holder dies, so a crash leaves no stale lock.
pub(super) fn try_lock(path: &Path) -> Option<std::fs::File> {
    let file = std::fs::File::options()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)
        .ok()?;
    file.try_lock().ok()?;
    Some(file)
}

/// Where the TeX in use came from, in the order they are preferred.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TexSource {
    /// A folder chosen in the settings.
    Settings,
    Path,
    /// Where a distribution installs itself (MacTeX, TeX Live, MiKTeX, TinyTeX, Homebrew…).
    Usual,
    /// The TeX Live the app installed.
    App,
}

/// A folder with both lualatex and kpsewhich: one TeX, whose programs agree on its files.
#[derive(Clone, Debug, PartialEq)]
pub struct TexCandidate {
    pub bin_dir: PathBuf,
    pub source: TexSource,
}

/// The TeX the pages typeset with: the first folder of the settings, PATH, the usual places and
/// the app's own TeX Live with both lualatex and kpsewhich.
pub fn find_tex() -> Option<TexCandidate> {
    let settings = CONFIG.read().unwrap().1.clone();
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    candidates([
        (
            TexSource::Settings,
            std::env::split_paths(&settings).collect(),
        ),
        (
            TexSource::Path,
            std::env::split_paths(&search_path()).collect(),
        ),
        (
            TexSource::Usual,
            usual_bins(Os::THIS, Path::new("/"), &home(), env),
        ),
        (
            TexSource::App,
            super::tex_install::own_tex_bin().into_iter().collect(),
        ),
    ])
    .into_iter()
    .next()
}

/// The folders of `groups` with both lualatex and kpsewhich, in order, each once.
fn candidates(groups: impl IntoIterator<Item = (TexSource, Vec<PathBuf>)>) -> Vec<TexCandidate> {
    let mut seen = std::collections::HashSet::new();
    groups
        .into_iter()
        .flat_map(|(source, dirs)| dirs.into_iter().map(move |d| (source, d)))
        .filter(|(_, d)| {
            !d.as_os_str().is_empty()
                && d.join(exe("lualatex")).is_file()
                && d.join(exe("kpsewhich")).is_file()
                && seen.insert(d.clone())
        })
        .map(|(source, bin_dir)| TexCandidate { bin_dir, source })
        .collect()
}

/// Where distributions put their programs when PATH does not say (an app started from the Finder
/// or the Start menu has a PATH of its own), newest TeX Live first. Absolute paths are under
/// `sys`, `/` but in tests.
fn usual_bins(
    os: Os,
    sys: &Path,
    home: &Path,
    env: impl Fn(&str) -> Option<PathBuf>,
) -> Vec<PathBuf> {
    let texlive = || expand(&sys.join("usr/local/texlive/*/bin/*"));
    match os {
        Os::Mac => [vec![sys.join("Library/TeX/texbin")], texlive()]
            .concat()
            .into_iter()
            .chain(expand(&home.join("Library/TinyTeX/bin/*")))
            .chain(["opt/homebrew/bin", "usr/local/bin", "opt/local/bin"].map(|d| sys.join(d)))
            .collect(),
        Os::Linux => [texlive(), expand(&home.join(".TinyTeX/bin/*"))].concat(),
        Os::Windows => {
            let miktex = |root: Option<PathBuf>| root.map(|r| r.join("MiKTeX/miktex/bin/x64"));
            // C: names the drive's current folder, C:\ its top
            let drive = env("SystemDrive").map(|d| {
                let mut d = d.into_os_string();
                d.push(std::path::MAIN_SEPARATOR_STR);
                PathBuf::from(d)
            });
            [
                miktex(env("LOCALAPPDATA").map(|d| d.join("Programs"))),
                miktex(env("ProgramFiles")),
            ]
            .into_iter()
            .flatten()
            .chain(
                drive
                    .into_iter()
                    .flat_map(|d| expand(&d.join("texlive/*/bin/*"))),
            )
            .chain(
                env("APPDATA")
                    .into_iter()
                    .flat_map(|d| expand(&d.join("TinyTeX/bin/*"))),
            )
            .collect()
        }
    }
}

/// The folders `pattern` names, a `*` standing for any name there; a `*`'s names from the last
/// (2026 before 2025).
fn expand(pattern: &Path) -> Vec<PathBuf> {
    pattern
        .components()
        .fold(vec![PathBuf::new()], |found, part| {
            if part.as_os_str() != "*" {
                return found.into_iter().map(|d| d.join(part)).collect();
            }
            found
                .iter()
                .flat_map(|d| {
                    let mut names: Vec<PathBuf> = std::fs::read_dir(d)
                        .into_iter()
                        .flatten()
                        .flatten()
                        .map(|e| e.path())
                        .collect();
                    names.sort_by(|a, b| b.cmp(a));
                    names
                })
                .collect()
        })
}

/// PATH as the app got it, and on Windows as it is now: a TeX installed while the app runs puts
/// itself on the PATH in the registry, which the app's own PATH, fixed at launch, never sees.
fn search_path() -> std::ffi::OsString {
    #[allow(unused_mut)]
    let mut path = std::env::var_os("PATH").unwrap_or_default();
    #[cfg(windows)]
    for p in registry_paths() {
        path.push(";");
        path.push(p);
    }
    path
}

/// The system's PATH and the user's, from the registry, with their %VARIABLES% expanded.
#[cfg(windows)]
fn registry_paths() -> Vec<std::ffi::OsString> {
    use std::os::windows::ffi::OsStringExt;
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegGetValueW(
            key: isize,
            sub_key: *const u16,
            value: *const u16,
            flags: u32,
            kind: *mut u32,
            data: *mut u16,
            len: *mut u32,
        ) -> i32;
    }
    // HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER: sign-extended, as the system's headers make them
    const MACHINE: isize = 0x8000_0002u32 as i32 as isize;
    const USER: isize = 0x8000_0001u32 as i32 as isize;
    // any type; REG_EXPAND_SZ comes back expanded
    const RRF_RT_ANY: u32 = 0xffff;
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    [
        (
            MACHINE,
            r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        ),
        (USER, "Environment"),
    ]
    .into_iter()
    .filter_map(|(key, sub)| {
        let (sub, name) = (wide(sub), wide("Path"));
        // an environment variable's longest
        let mut buf = vec![0u16; 32768];
        let mut len = (buf.len() * 2) as u32;
        let read = unsafe {
            RegGetValueW(
                key,
                sub.as_ptr(),
                name.as_ptr(),
                RRF_RT_ANY,
                std::ptr::null_mut(),
                buf.as_mut_ptr(),
                &mut len,
            )
        };
        (read == 0).then(|| {
            buf.truncate(len as usize / 2);
            while buf.last() == Some(&0) {
                buf.pop();
            }
            std::ffi::OsString::from_wide(&buf)
        })
    })
    .collect()
}

/// lualatex of the TeX in use.
pub fn find_lualatex() -> Option<PathBuf> {
    find_tex().map(|t| t.bin_dir.join(exe("lualatex")))
}

/// Whether rtex is where the app looks.
pub fn rtex_found() -> bool {
    find_binary().is_some_and(|p| p.is_file())
}

/// `kpsewhich <name>`: where TeX finds a file.
pub fn kpsewhich(name: &str) -> Option<PathBuf> {
    let out = command(find_tex()?.bin_dir.join(exe("kpsewhich")))
        .arg(name)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| PathBuf::from(s))
}

/// PATH for rtex and its LaTeX runs: the TeX in use first, so that every TeX program is its
/// own, then the settings' folders (several: separated as PATH separates them), PATH, and on the
/// Mac MacTeX's and Homebrew's, which an app launched from the Finder does not inherit.
pub fn tex_path() -> std::ffi::OsString {
    let extra = CONFIG.read().unwrap().1.clone();
    let mut dirs: Vec<PathBuf> = find_tex().map(|t| t.bin_dir).into_iter().collect();
    dirs.extend(std::env::split_paths(&extra));
    dirs.extend(std::env::split_paths(&search_path()));
    if cfg!(target_os = "macos") {
        dirs.extend(
            ["/Library/TeX/texbin", "/usr/local/bin", "/opt/homebrew/bin"].map(PathBuf::from),
        );
    }
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| !d.as_os_str().is_empty() && seen.insert(d.clone()));
    std::env::join_paths(dirs).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{Os, TexCandidate, TexSource, candidates, clear_builds, try_lock, usual_bins};
    use std::path::{Path, PathBuf};

    /// A TeX's bin folder under `root`: lualatex, and kpsewhich unless `half`.
    fn tex_at(root: &Path, dir: &str, half: bool) -> PathBuf {
        let d = root.join(dir);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(super::exe("lualatex")), "").unwrap();
        if !half {
            std::fs::write(d.join(super::exe("kpsewhich")), "").unwrap();
        }
        d
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rtd-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn found(os: Os, root: &Path, env: &[(&str, &str)]) -> Vec<PathBuf> {
        let env: Vec<(String, PathBuf)> = env
            .iter()
            .map(|(k, v)| (k.to_string(), root.join(v)))
            .collect();
        let dirs = usual_bins(os, root, &root.join("home"), |k| {
            env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone())
        });
        candidates([(TexSource::Usual, dirs)])
            .into_iter()
            .map(|c| c.bin_dir)
            .collect()
    }

    #[test]
    fn the_mac_finds_mactex_then_tex_live_newest_first_then_tinytex_and_the_package_managers() {
        let r = scratch("mac-tex");
        let texbin = tex_at(&r, "Library/TeX/texbin", false);
        let tl2025 = tex_at(&r, "usr/local/texlive/2025/bin/universal-darwin", false);
        let tl2026 = tex_at(&r, "usr/local/texlive/2026/bin/universal-darwin", false);
        let tiny = tex_at(&r, "home/Library/TinyTeX/bin/universal-darwin", false);
        let ports = tex_at(&r, "opt/local/bin", false);
        tex_at(&r, "opt/homebrew/bin", true);
        assert_eq!(
            found(Os::Mac, &r, &[]),
            [texbin, tl2026, tl2025, tiny, ports],
            "a folder with lualatex but no kpsewhich is not a TeX"
        );
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn linux_finds_tex_live_newest_first_then_tinytex() {
        let r = scratch("linux-tex");
        let tl2024 = tex_at(&r, "usr/local/texlive/2024/bin/x86_64-linux", false);
        let tl2026 = tex_at(&r, "usr/local/texlive/2026/bin/x86_64-linux", false);
        let tiny = tex_at(&r, "home/.TinyTeX/bin/x86_64-linux", false);
        assert_eq!(found(Os::Linux, &r, &[]), [tl2026, tl2024, tiny]);
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn windows_finds_miktex_for_the_user_and_for_all_then_tex_live_then_tinytex() {
        let r = scratch("windows-tex");
        let env = [
            ("LOCALAPPDATA", "local"),
            ("ProgramFiles", "programs"),
            ("SystemDrive", "c"),
            ("APPDATA", "roaming"),
        ];
        let user = tex_at(&r, "local/Programs/MiKTeX/miktex/bin/x64", false);
        let all = tex_at(&r, "programs/MiKTeX/miktex/bin/x64", false);
        let tl2025 = tex_at(&r, "c/texlive/2025/bin/windows", false);
        let tl2026 = tex_at(&r, "c/texlive/2026/bin/windows", false);
        let tiny = tex_at(&r, "roaming/TinyTeX/bin/windows", false);
        assert_eq!(
            found(Os::Windows, &r, &env),
            [user, all, tl2026, tl2025, tiny]
        );
        assert_eq!(found(Os::Windows, &r, &[]), Vec::<PathBuf>::new());
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn the_settings_win_over_path_and_path_over_the_usual_places_each_folder_once() {
        let r = scratch("tex-order");
        let (mine, on_path, usual) = (
            tex_at(&r, "mine", false),
            tex_at(&r, "path", false),
            tex_at(&r, "usual", false),
        );
        let half = tex_at(&r, "half", true);
        let got = candidates([
            (TexSource::Settings, vec![half, mine.clone()]),
            (TexSource::Path, vec![on_path.clone(), mine.clone()]),
            (TexSource::Usual, vec![usual.clone()]),
        ]);
        assert_eq!(
            got,
            [
                TexCandidate {
                    bin_dir: mine,
                    source: TexSource::Settings
                },
                TexCandidate {
                    bin_dir: on_path,
                    source: TexSource::Path
                },
                TexCandidate {
                    bin_dir: usual,
                    source: TexSource::Usual
                },
            ]
        );
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn a_lock_is_one_holders_until_dropped_and_a_leftover_file_is_no_lock() {
        let r = scratch("lock");
        let path = r.join("texlive.lock");
        std::fs::write(&path, "4242").unwrap();
        let held = try_lock(&path).expect("a lock file nobody holds is free");
        assert!(try_lock(&path).is_none(), "held, it is not given twice");
        drop(held);
        assert!(
            try_lock(&path).is_some(),
            "free again once its holder is gone"
        );
        let _ = std::fs::remove_dir_all(&r);
    }

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

    /// A stand-in for an rtex group: a process in a group of its own whose command line names
    /// `build`, as rtex's and its lualatex runs' do.
    #[cfg(unix)]
    fn group_in(build: &Path) -> std::process::Child {
        use std::os::unix::process::CommandExt;
        std::fs::create_dir_all(build).unwrap();
        std::fs::write(build.join("log"), "").unwrap();
        std::process::Command::new("/usr/bin/tail")
            .arg("-f")
            .arg(build.join("log"))
            .process_group(0)
            // not the caller's stdout: a child test's reader waits for it to close
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap()
    }

    #[cfg(unix)]
    fn group_alive(pgid: u32) -> bool {
        unsafe { libc::kill(-(pgid as i32), 0) == 0 }
    }

    #[cfg(unix)]
    #[test]
    fn a_launch_ends_the_groups_a_gone_app_left_and_nothing_that_took_their_ids() {
        let r = scratch("groups");
        let records = r.join("groups");
        std::fs::create_dir_all(&records).unwrap();
        let gone = {
            let mut c = std::process::Command::new("/usr/bin/true").spawn().unwrap();
            c.wait().unwrap();
            c.id()
        };
        let record = |g: &std::process::Child, app: u32, build: &Path| {
            let text = format!("{app}\n{}", build.display());
            std::fs::write(records.join(g.id().to_string()), text).unwrap();
        };
        let mut left = group_in(&r.join("left"));
        record(&left, gone, &r.join("left"));
        let mut reused = group_in(&r.join("other"));
        record(&reused, gone, &r.join("before"));
        let mut open = group_in(&r.join("open"));
        record(&open, std::process::id(), &r.join("open"));
        super::end_groups_in(&records);
        let ended = |c: &mut std::process::Child| {
            (0..50).any(|_| {
                std::thread::sleep(std::time::Duration::from_millis(10));
                c.try_wait().unwrap().is_some()
            })
        };
        let left_ended = ended(&mut left);
        let reused_ended = ended(&mut reused);
        let open_ended = ended(&mut open);
        for c in [&mut left, &mut reused, &mut open] {
            let _ = c.kill();
            let _ = c.wait();
        }
        let kept: Vec<String> = std::fs::read_dir(&records)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        let _ = std::fs::remove_dir_all(&r);
        assert!(left_ended, "a gone app's group, still in its build, ends");
        assert!(
            !reused_ended,
            "a group whose id another process took, outside the build, is left alone"
        );
        assert!(!open_ended, "a running app's group is left alone");
        assert_eq!(
            kept,
            [open.id().to_string()],
            "only the running app's record stays"
        );
    }

    /// The process that started rtex ending, by returning from main (a test binary whose
    /// documents are never dropped) or by SIGTERM, ends rtex's group too. Each runs this test
    /// again as a child process, which starts the group and ends.
    #[cfg(unix)]
    #[test]
    fn rtexs_group_ends_with_the_process_on_exit_and_on_sigterm() {
        use std::io::BufRead;
        if let Some(how) = std::env::var_os("RTD_GROUP_CHILD") {
            let build = PathBuf::from(std::env::var_os("RTD_GROUP_BUILD").unwrap());
            // never waited for: the exit or the signal under test ends it
            let pgid = group_in(&build).id();
            super::track(pgid, &build);
            if how == "term" {
                super::exit_on_signals();
            }
            println!("pgid={pgid}");
            if how == "term" {
                std::thread::sleep(std::time::Duration::from_secs(60));
            }
            return;
        }
        let r = scratch("group-exit");
        for how in ["exit", "term"] {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "engine::process::tests::rtexs_group_ends_with_the_process_on_exit_and_on_sigterm",
                    "--exact",
                    "--nocapture",
                ])
                .env("RTD_GROUP_CHILD", how)
                .env("RTD_GROUP_BUILD", r.join(how))
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let mut out = std::io::BufReader::new(child.stdout.take().unwrap()).lines();
            let pgid: u32 = out
                .find_map(|l| l.ok()?.strip_prefix("pgid=")?.parse().ok())
                .expect("the child started its group");
            if how == "term" {
                unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
            }
            out.for_each(drop);
            let status = child.wait().unwrap();
            let ended = (0..300).any(|_| {
                std::thread::sleep(std::time::Duration::from_millis(10));
                !group_alive(pgid)
            });
            if !ended {
                super::kill_group(pgid);
                let _ = std::fs::remove_file(super::groups_dir().join(pgid.to_string()));
            }
            assert!(ended, "{how}: the group outlived the process ({status})");
            assert!(
                !super::groups_dir().join(pgid.to_string()).exists(),
                "{how}: its record is removed"
            );
        }
        let _ = std::fs::remove_dir_all(&r);
    }
}
