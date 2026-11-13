// TeX Live installed by the app for itself when no LuaLaTeX is found: TeX Live's own installer
// (install-tl, the same on macOS, Windows and Linux) run unattended from a profile, the full
// scheme without documentation or sources, into a folder of the app's (no administrator
// password, nothing changed outside it). Its lines say how far it is:
// "Installing [0012/4556, time/total: 00:31/41:02]: amsmath [2k]". The folder counts as
// installed only once its LuaLaTeX has run and found its format, which `.complete` records: a
// folder without it is an install cut short, removed by the next.
use super::download::{HTTPS_ONLY, curl, said, sha_hex};
use super::process::{Os, command, exe, home, kill_group, try_lock};
use sha2::Sha512;
use std::io::{BufRead, BufReader};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

/// mirror.ctan.org picks a mirror by the address alone, and from some places the one it picks
/// barely moves; it is the fallback.
const REDIRECTOR: &str = "https://mirror.ctan.org";

/// CTAN mirrors on each continent; the fastest from here is used.
const MIRRORS: &[&str] = &[
    REDIRECTOR,
    "https://mirrors.mit.edu/CTAN",
    "https://mirror.math.princeton.edu/pub/CTAN",
    "https://ftp.fau.de/ctan",
    "https://ftp.gwdg.de/pub/ctan",
    "https://mirror.init7.net/ctan",
    "https://mirrors.tuna.tsinghua.edu.cn/CTAN",
    "https://mirrors.ustc.edu.cn/CTAN",
    "https://ftp.jaist.ac.jp/pub/CTAN",
    "https://mirror.aarnet.edu.au/pub/CTAN",
];

const TLNET: &str = "systems/texlive/tlnet";

const MARKER: &str = ".complete";

/// scheme-full without documentation, with room for the installer's downloads.
const NEEDED: u64 = 6_000_000_000;

/// TeX Live being installed by the app, or how that went: what the setup window shows. The
/// install's thread moves it on; Cancel only asks it to stop.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum TexInstall {
    #[default]
    Idle,
    Running(TexProgress),
    /// Cancel pressed: the install stops, and what it put in is being removed.
    Cancelling,
    /// Installed, and its LuaLaTeX runs; what the installer complained of on the way, if anything.
    Done(Vec<String>),
    Failed(String),
}

/// Why an install stopped short.
enum Stop {
    Cancelled,
    Failed(String),
}

impl From<String> for Stop {
    fn from(why: String) -> Self {
        Stop::Failed(why)
    }
}

impl From<&str> for Stop {
    fn from(why: &str) -> Self {
        Stop::Failed(why.into())
    }
}

/// The install going on: whether Cancel was pressed, and the program it waits for (its process
/// group), for Cancel and for quitting.
#[derive(Default)]
struct Job {
    cancelled: bool,
    child: Option<u32>,
}

static JOB: Mutex<Option<Job>> = Mutex::new(None);

/// The job for as long as it is kept: none if an install is going on already.
struct JobGuard;

impl JobGuard {
    fn start() -> Option<Self> {
        let mut job = JOB.lock().unwrap();
        if job.is_some() {
            return None;
        }
        *job = Some(Job::default());
        Some(JobGuard)
    }
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        *JOB.lock().unwrap() = None;
    }
}

/// Cancel pressed: the stage going on stops, and the program it runs is ended. Pressed before
/// the install starts a program, it starts none.
pub fn cancel_tex_install() {
    if let Some(job) = JOB.lock().unwrap().as_mut() {
        job.cancelled = true;
        if let Some(pid) = job.child.take() {
            kill_group(pid);
        }
    }
}

fn check() -> Result<(), Stop> {
    match JOB.lock().unwrap().as_ref() {
        Some(job) if job.cancelled => Err(Stop::Cancelled),
        _ => Ok(()),
    }
}

/// `cmd` started unless Cancel was pressed, and ended by Cancel while it runs (with what it
/// starts: its own process group).
fn spawn(cmd: &mut Command) -> Result<Child, Stop> {
    let mut job = JOB.lock().unwrap();
    if job.as_ref().is_some_and(|j| j.cancelled) {
        return Err(Stop::Cancelled);
    }
    #[cfg(unix)]
    cmd.process_group(0);
    let child = cmd.spawn().map_err(|e| {
        format!(
            "couldn’t start {} ({e})",
            cmd.get_program().to_string_lossy()
        )
    })?;
    if let Some(job) = job.as_mut() {
        job.child = Some(child.id());
    }
    Ok(child)
}

/// The program spawned has ended; whether Cancel was pressed meanwhile.
fn ended() -> Result<(), Stop> {
    if let Some(job) = JOB.lock().unwrap().as_mut() {
        job.child = None;
    }
    check()
}

/// `cmd` run to its end, unless cancelled; what it said on failure.
fn run(cmd: &mut Command, what: &str) -> Result<(), Stop> {
    let child = spawn(
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped()),
    )?;
    let out = child.wait_with_output();
    ended()?;
    let out = out.map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("couldn’t {what}: {}", said(&out.stderr)).into())
    }
}

/// Half a megabyte of the installer from `mirror`, for how fast it comes.
fn probe(mirror: &str) -> Command {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let mut c = command("curl");
    c.args(HTTPS_ONLY)
        .args([
            "--silent",
            "--location",
            "--range",
            "0-524287",
            "--max-time",
            "5",
        ])
        .args(["-o", null, "-w", "%{http_code} %{speed_download}"])
        .arg(format!("{mirror}/{TLNET}/install-tl-unx.tar.gz"));
    c
}

/// The mirrors that answered, fastest first (all tried at once, 5 s each at most), then
/// mirror.ctan.org.
fn mirrors() -> Vec<&'static str> {
    let tries: Vec<_> = MIRRORS
        .iter()
        .map(|m| {
            std::thread::spawn(move || {
                let out = probe(m).stdin(Stdio::null()).output().ok()?;
                let said = String::from_utf8_lossy(&out.stdout).to_string();
                let (code, speed) = said.split_once(' ')?;
                let speed: f64 = speed.trim().parse().ok()?;
                // a mirror that timed out reports what little it sent: only a whole answer counts
                (out.status.success() && matches!(code, "200" | "206")).then_some((*m, speed))
            })
        })
        .collect();
    let mut ranked: Vec<_> = tries
        .into_iter()
        .filter_map(|t| t.join().ok().flatten())
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut list: Vec<_> = ranked.into_iter().map(|(m, _)| m).collect();
    if !list.contains(&REDIRECTOR) {
        list.push(REDIRECTOR);
    }
    list
}

/// Where the app's TeX Live goes: beside the system's apps' data, not in a folder that roams
/// (Windows) or is backed up as settings.
fn tex_dir() -> PathBuf {
    let env = |k| std::env::var_os(k).map(PathBuf::from);
    if cfg!(target_os = "macos") {
        home().join("Library/Realtime Tex Desktop/texlive")
    } else if cfg!(windows) {
        env("LOCALAPPDATA")
            .unwrap_or_else(|| home().join("AppData/Local"))
            .join("Realtime Tex Desktop/texlive")
    } else {
        env("XDG_DATA_HOME")
            .unwrap_or_else(|| home().join(".local/share"))
            .join("realtime-tex-desktop/texlive")
    }
}

/// Held while TeX Live is installed or removed there: one copy of the app at a time.
fn lock_path(dir: &Path) -> PathBuf {
    dir.with_extension("lock")
}

/// Where install-tl is downloaded and unpacked.
fn work_dir(dir: &Path) -> PathBuf {
    dir.with_file_name("texlive-installer")
}

/// The app's TeX Live's bin folder, once it is installed.
pub(super) fn own_tex_bin() -> Option<PathBuf> {
    installed(&tex_dir())
}

/// `dir`'s bin folder if its install finished.
fn installed(dir: &Path) -> Option<PathBuf> {
    dir.join(MARKER).is_file().then(|| plat_bin(dir)).flatten()
}

/// bin/<platform> (universal-darwin, x86_64-linux, windows…).
fn plat_bin(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir.join("bin"))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|d| d.join(exe("lualatex")).is_file())
}

/// `dir`'s TeX works: its LuaLaTeX runs and its kpsewhich finds LuaLaTeX's format. install-tl's
/// exit status does not say so: it counts every step that failed, a format of no use here among
/// them.
fn usable(dir: &Path) -> bool {
    let Some(bin) = plat_bin(dir) else {
        return false;
    };
    let ok = |c: &mut Command| {
        c.stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .is_ok_and(|o| o.status.success() && !o.stdout.trim_ascii().is_empty())
    };
    ok(command(bin.join(exe("lualatex"))).arg("--version"))
        && ok(command(bin.join(exe("kpsewhich"))).args(["-engine=luahbtex", "lualatex.fmt"]))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub enum Stage {
    /// A fast mirror found, and install-tl itself downloaded.
    #[default]
    Installer,
    /// The list of packages, from a mirror.
    PackageList,
    Packages,
    /// The file database, fonts and formats built (some minutes).
    Finishing,
    /// LuaLaTeX run, to see that it works.
    Verifying,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TexProgress {
    pub stage: Stage,
    /// Packages begun, and how many there are.
    pub done: usize,
    pub total: usize,
    /// Seconds since the packages began, and the installer's estimate of the whole (from the
    /// sizes done so far).
    pub elapsed: u64,
    pub estimate: Option<u64>,
    /// Downloaded so far (the packages done), in bytes.
    pub bytes: u64,
    pub package: String,
    /// The size of the package begun (counted in `bytes` once the next one begins).
    begun: u64,
}

impl TexProgress {
    /// Bytes a second, on average since the packages began.
    pub fn speed(&self) -> Option<u64> {
        (self.elapsed > 0).then(|| self.bytes / self.elapsed)
    }

    /// Seconds left, as the installer estimates.
    pub fn left(&self) -> Option<u64> {
        self.estimate.map(|t| t.saturating_sub(self.elapsed))
    }

    /// Done, 0 to 1: by size while the installer can tell, else by packages.
    pub fn fraction(&self) -> f32 {
        match (self.stage, self.estimate) {
            (Stage::Finishing | Stage::Verifying, _) => 1.0,
            (Stage::Packages, Some(t)) if t > 0 => (self.elapsed as f32 / t as f32).min(1.0),
            (Stage::Packages, _) if self.total > 0 => self.done as f32 / self.total as f32,
            _ => 0.0,
        }
    }

    /// One of install-tl's lines read into the progress; whether it changed.
    fn read(&mut self, line: &str) -> bool {
        if line.starts_with("Loading ") && self.stage < Stage::PackageList {
            self.stage = Stage::PackageList;
            return true;
        }
        if let Some(rest) = line.strip_prefix("Installing [") {
            let Some((counts, rest)) = rest.split_once(", time/total: ") else {
                return false;
            };
            let Some((times, rest)) = rest.split_once("]: ") else {
                return false;
            };
            let (done, total) = counts.split_once('/').unwrap_or_default();
            let (passed, whole) = times.split_once('/').unwrap_or_default();
            let (name, size) = rest.rsplit_once(" [").unwrap_or((rest, ""));
            self.stage = Stage::Packages;
            self.done = done.parse().unwrap_or(self.done);
            self.total = total.parse().unwrap_or(self.total);
            self.elapsed = seconds(passed).unwrap_or(self.elapsed);
            self.estimate = seconds(whole);
            self.bytes += self.begun;
            self.begun = size.trim_end_matches("k]").parse::<u64>().unwrap_or(0) * 1024;
            self.package = name.to_string();
            return true;
        }
        let finishing = [
            "running mktexlsr",
            "writing fmtutil.cnf",
            "running fmtutil",
            "running updmap",
        ];
        if self.stage == Stage::Packages && finishing.iter().any(|f| line.contains(f)) {
            self.stage = Stage::Finishing;
            self.bytes += std::mem::take(&mut self.begun);
            return true;
        }
        false
    }
}

/// 01:02:03 or 02:03 as seconds; ??:?? (not known yet) as none.
fn seconds(t: &str) -> Option<u64> {
    t.split(':')
        .try_fold(0u64, |s, part| Some(s * 60 + part.parse::<u64>().ok()?))
}

/// Install TeX Live, telling `report` how far it is; how it ended (Idle when cancelled). Blocks
/// for as long as the install takes (most of an hour): run it on a thread of its own.
pub fn install_tex(report: impl Fn(TexProgress)) -> TexInstall {
    let dir = tex_dir();
    if let Err(why) = preflight(&dir) {
        return TexInstall::Failed(why);
    }
    install_into(&dir, &report, mirrors, |work, mirror, all| {
        fetch_installer(&dir, "scheme-full", work, mirror, all)
    })
}

/// What install-tl needs that the app cannot give it, said before an hour goes into it.
fn preflight(dir: &Path) -> Result<(), String> {
    if let Some(c) = awkward(dir, Os::THIS) {
        return Err(format!(
            "TeX Live’s installer refuses a folder whose path has “{c}” in it, as {} has. Install a TeX distribution yourself instead",
            dir.display()
        ));
    }
    let tools: &[&str] = if cfg!(windows) {
        &["curl", "tar"]
    } else {
        &["curl", "tar", "perl"]
    };
    for tool in tools {
        let found = command(tool)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok();
        if !found {
            let how = if cfg!(target_os = "linux") {
                format!(" (on Debian and Ubuntu: sudo apt install {tool})")
            } else {
                String::new()
            };
            return Err(format!(
                "TeX Live’s installer needs {tool}. Install it{how}, then try again"
            ));
        }
    }
    Ok(())
}

/// A character that kpathsea reads as syntax in a path, which install-tl refuses in its folder's.
fn awkward(dir: &Path, os: Os) -> Option<char> {
    let path = dir.to_string_lossy();
    // install-tl turns Windows' \ into / first; C: is a drive there
    let path = if os == Os::Windows {
        path.replace('\\', "/")
    } else {
        path.into_owned()
    };
    path.chars().find(|c| {
        matches!(c, ',' | ';' | '\\' | '{' | '}' | '$') || (*c == ':' && os != Os::Windows)
    })
}

/// Install TeX Live into `dir`, with install-tl from `installer` (one mirror of `mirrors` at a
/// time: the next if one fails before any package is in).
fn install_into(
    dir: &Path,
    report: &dyn Fn(TexProgress),
    mirrors: impl FnOnce() -> Vec<&'static str>,
    installer: impl FnMut(&Path, &str, &[&str]) -> Result<Command, Stop>,
) -> TexInstall {
    let Some(_job) = JobGuard::start() else {
        return TexInstall::Failed("TeX Live is being installed already".into());
    };
    if let Some(parent) = dir.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Some(_lock) = try_lock(&lock_path(dir)) else {
        return TexInstall::Failed(
            "another copy of the app is installing TeX Live. Try again once it is done".into(),
        );
    };
    // another copy of the app finished it meanwhile
    if installed(dir).is_some() {
        return TexInstall::Done(vec![]);
    }
    let work = work_dir(dir);
    let got = attempts(dir, &work, report, mirrors, installer);
    let _ = std::fs::remove_dir_all(&work);
    match got {
        Ok(problems) => TexInstall::Done(problems),
        Err(stop) => {
            let _ = std::fs::remove_dir_all(dir);
            match stop {
                Stop::Cancelled => TexInstall::Idle,
                Stop::Failed(why) => TexInstall::Failed(why),
            }
        }
    }
}

fn attempts(
    dir: &Path,
    work: &Path,
    report: &dyn Fn(TexProgress),
    mirrors: impl FnOnce() -> Vec<&'static str>,
    mut installer: impl FnMut(&Path, &str, &[&str]) -> Result<Command, Stop>,
) -> Result<Vec<String>, Stop> {
    check()?;
    let mirrors = mirrors();
    let mut why = String::from("no mirror answered");
    for mirror in mirrors.iter().take(2) {
        check()?;
        // an install that stopped half way is not finished: install-tl starts again from nothing
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(work);
        std::fs::create_dir_all(work).map_err(|e| e.to_string())?;
        let mut progress = TexProgress::default();
        report(progress.clone());
        let cmd = match installer(work, mirror, &mirrors) {
            Ok(cmd) => cmd,
            Err(Stop::Failed(w)) => {
                why = w;
                continue;
            }
            Err(Stop::Cancelled) => return Err(Stop::Cancelled),
        };
        let ran = run_installer(cmd, &mut progress, report)?;
        let began = progress.stage >= Stage::Packages;
        progress.stage = Stage::Verifying;
        report(progress.clone());
        let works = usable(dir);
        check()?;
        if works {
            std::fs::write(dir.join(MARKER), "").map_err(|e| e.to_string())?;
            return Ok(ran.problems);
        }
        why = format!("TeX Live’s installer stopped: {}", ran.why);
        // packages in: another mirror would start the hour again
        if began {
            break;
        }
    }
    Err(why.into())
}

/// install-tl from `mirror`, checked and unpacked in `work`, ready to install into `dir`.
fn fetch_installer(
    dir: &Path,
    scheme: &str,
    work: &Path,
    mirror: &str,
    mirrors: &[&str],
) -> Result<Command, Stop> {
    if let Some(free) = free_space(dir)
        && free < NEEDED
    {
        return Err(format!(
            "TeX Live needs about 6 GB free, and the disk with {} has {:.1} GB",
            dir.display(),
            free as f64 / 1e9
        )
        .into());
    }
    let name = if cfg!(windows) {
        "install-tl.zip"
    } else {
        "install-tl-unx.tar.gz"
    };
    let archive = work.join(name);
    run(
        curl()
            .arg("-o")
            .arg(&archive)
            .arg(format!("{mirror}/{TLNET}/{name}")),
        "download TeX Live’s installer",
    )?;
    // its checksum from another mirror, so that a bad one cannot vouch for its own copy
    let other = mirrors.iter().find(|m| **m != mirror).unwrap_or(&mirror);
    let sum = work.join(format!("{name}.sha512"));
    run(
        curl()
            .arg("-o")
            .arg(&sum)
            .arg(format!("{other}/{TLNET}/{name}.sha512")),
        "download the installer’s checksum",
    )?;
    let published = std::fs::read_to_string(&sum).map_err(|e| e.to_string())?;
    check_sha512(&archive, &published)?;
    run(
        command("tar").arg("-xf").arg(&archive).arg("-C").arg(work),
        "unpack TeX Live’s installer",
    )?;
    // the archive's one folder: install-tl-<date>
    let installer = std::fs::read_dir(work)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.is_dir()
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("install-tl"))
        })
        .ok_or("TeX Live’s installer was not in its download")?;
    let profile = work.join("realtime-tex.profile");
    std::fs::write(&profile, profile_text(dir, scheme, Os::THIS)).map_err(|e| e.to_string())?;
    let mut cmd = if cfg!(windows) {
        // run as itself, not through `cmd /C`, which loses the quotes around a path with spaces
        let mut c = command(installer.join("install-tl-windows.bat"));
        c.arg("-no-gui");
        c
    } else {
        let mut c = command("perl");
        c.arg(installer.join("install-tl"));
        c
    };
    cmd.arg("-profile")
        .arg(&profile)
        .arg("-repository")
        .arg(format!("{mirror}/{TLNET}"))
        .current_dir(work);
    Ok(cmd)
}

/// `file` is what CTAN's .sha512 for it ("<hex>  install-tl-unx.tar.gz") says.
fn check_sha512(file: &Path, published: &str) -> Result<(), String> {
    let want = published
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let got = sha_hex::<Sha512>(file).map_err(|e| e.to_string())?;
    if want.len() == 128 && got == want {
        Ok(())
    } else {
        Err("TeX Live’s installer didn’t match its published checksum".into())
    }
}

/// Bytes free on the disk of `path`, or of the nearest folder above it that exists.
fn free_space(path: &Path) -> Option<u64> {
    path.ancestors().find_map(free_at)
}

#[cfg(unix)]
// the fields' types differ by system
#[allow(clippy::unnecessary_cast)]
fn free_at(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    (unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0)
        .then(|| s.f_bavail as u64 * s.f_frsize as u64)
}

#[cfg(windows)]
fn free_at(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            dir: *const u16,
            free_to_caller: *mut u64,
            total: *mut u64,
            free: *mut u64,
        ) -> i32;
    }
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut free = 0u64;
    (unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } != 0)
        .then_some(free)
}

/// How install-tl ended: the problems it reported (with a working TeX they are warnings), and
/// the last thing it said.
struct Ran {
    problems: Vec<String>,
    why: String,
}

fn run_installer(
    mut cmd: Command,
    progress: &mut TexProgress,
    report: &dyn Fn(TexProgress),
) -> Result<Ran, Stop> {
    let mut child = spawn(
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )?;
    // its warnings, without the line that only points at them
    let stderr = child.stderr.take().unwrap();
    let warned = std::thread::spawn(move || {
        BufReader::new(stderr)
            .lines()
            .map_while(Result::ok)
            .map(|l| l.trim().to_string())
            .filter(|l| {
                !l.is_empty()
                    && !l.contains("errors in installation reported above")
                    && !l.contains("Summary of warnings")
            })
            .collect::<Vec<_>>()
    });
    let (mut last, mut failed, mut listing) = (String::new(), None, false);
    for line in BufReader::new(child.stdout.take().unwrap())
        .lines()
        .map_while(Result::ok)
    {
        if progress.read(&line) {
            report(progress.clone());
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // "The following (inessential) packages failed to install properly:", then their names
        if listing && failed.is_none() {
            failed = Some(line.to_string());
        }
        listing |= line.contains("failed to install properly");
        last = line.to_string();
    }
    let status = child.wait();
    ended()?;
    let status = status.map_err(|e| e.to_string())?;
    let warned = warned.join().unwrap_or_default();
    let mut problems = if status.success() {
        vec![]
    } else {
        warned[warned.len().saturating_sub(5)..].to_vec()
    };
    problems.extend(failed.map(|p| format!("packages that didn’t install: {p}")));
    Ok(Ran {
        problems,
        why: warned.last().cloned().unwrap_or(last),
    })
}

/// The answers install-tl would ask for: everything but documentation and sources, in `dir`,
/// for this user only, nothing outside it touched (no PATH, no file types, no menu entries).
fn profile_text(dir: &Path, scheme: &str, os: Os) -> String {
    // install-tl reads / on Windows too
    let d = dir.to_string_lossy().replace('\\', "/");
    // on Windows the only way to no uninstaller entry and no registry keys of its own; elsewhere
    // it would only stop ~/texmf being read
    let portable = u8::from(os == Os::Windows);
    format!(
        "selected_scheme {scheme}
TEXDIR {d}
TEXMFLOCAL {d}/texmf-local
TEXMFSYSVAR {d}/texmf-var
TEXMFSYSCONFIG {d}/texmf-config
TEXMFVAR {d}/texmf-var
TEXMFCONFIG {d}/texmf-config
TEXMFHOME ~/texmf
instopt_adjustpath 0
instopt_adjustrepo 1
instopt_letter 0
instopt_portable {portable}
instopt_write18_restricted 1
tlpdbopt_autobackup 0
tlpdbopt_create_formats 1
tlpdbopt_desktop_integration 0
tlpdbopt_file_assocs 0
tlpdbopt_generate_updmap 0
tlpdbopt_install_docfiles 0
tlpdbopt_install_srcfiles 0
tlpdbopt_post_code 1
tlpdbopt_w32_multi_user 0
"
    )
}

/// An install a crash or a quit cut short, removed (not one another copy of the app is making).
pub(super) fn clean_leftovers() {
    clean_leftovers_in(&tex_dir());
}

fn clean_leftovers_in(dir: &Path) {
    if let Some(_lock) = try_lock(&lock_path(dir))
        && dir.is_dir()
        && installed(dir).is_none()
    {
        // a finished install from before the marker existed is kept, not downloaded again
        if usable(dir) {
            let _ = std::fs::write(dir.join(MARKER), "");
        } else {
            let _ = std::fs::remove_dir_all(dir);
            let _ = std::fs::remove_dir_all(work_dir(dir));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Os, Stage, TexInstall, TexProgress};
    use std::path::{Path, PathBuf};

    #[test]
    fn the_installers_lines_say_how_far_it_is() {
        let mut p = TexProgress::default();
        assert!(p.read("Loading https://mirror.example/tlnet/tlpkg/texlive.tlpdb"));
        assert_eq!(p.stage, Stage::PackageList);
        assert!(p.read("Installing [0001/4556, time/total: ??:??/??:??]: 12many [376k]"));
        assert_eq!((p.done, p.total, p.estimate, p.bytes), (1, 4556, None, 0));
        assert!(p.read("Installing [0002/4556, time/total: 00:10/01:02:30]: 2up [66k]"));
        assert_eq!(p.package, "2up");
        assert_eq!(
            (p.elapsed, p.estimate, p.bytes),
            (10, Some(3750), 376 * 1024)
        );
        assert_eq!(p.speed(), Some(376 * 1024 / 10));
        assert_eq!(p.left(), Some(3740));
        assert!(p.fraction() > 0.0 && p.fraction() < 0.01);
        assert!(!p.read("some other line"));
        assert!(p.read("[12:00] running mktexlsr /x/texmf-dist ..."));
        assert_eq!((p.stage, p.bytes), (Stage::Finishing, (376 + 66) * 1024));
    }

    /// The keys install-tl 2026's read_profile takes: any other stops it ("Unknown key(s) in
    /// profile"); path keys must not be empty.
    const ACCEPTED: &[&str] = &[
        "selected_scheme",
        "TEXDIR",
        "TEXMFHOME",
        "TEXMFLOCAL",
        "TEXMFCONFIG",
        "TEXMFSYSCONFIG",
        "TEXMFVAR",
        "TEXMFSYSVAR",
        "instopt_adjustpath",
        "instopt_adjustrepo",
        "instopt_letter",
        "instopt_portable",
        "instopt_write18_restricted",
        "tlpdbopt_autobackup",
        "tlpdbopt_backupdir",
        "tlpdbopt_create_formats",
        "tlpdbopt_desktop_integration",
        "tlpdbopt_file_assocs",
        "tlpdbopt_generate_updmap",
        "tlpdbopt_install_docfiles",
        "tlpdbopt_install_srcfiles",
        "tlpdbopt_post_code",
        "tlpdbopt_sys_bin",
        "tlpdbopt_sys_info",
        "tlpdbopt_sys_man",
        "tlpdbopt_w32_multi_user",
    ];

    #[test]
    fn the_profile_installs_everything_but_docs_for_this_user_on_each_system() {
        let win = super::profile_text(
            Path::new(r"C:\Users\a b\texlive"),
            "scheme-full",
            Os::Windows,
        );
        assert!(win.contains("TEXDIR C:/Users/a b/texlive\n"));
        assert!(
            win.contains("instopt_portable 1\n"),
            "no uninstaller entry or registry keys on Windows"
        );
        for os in [Os::Mac, Os::Linux] {
            let t = super::profile_text(Path::new("/home/a/texlive"), "scheme-full", os);
            assert!(t.contains("instopt_portable 0\n"), "~/texmf still read");
        }
        for os in [Os::Mac, Os::Linux, Os::Windows] {
            let t = super::profile_text(Path::new("/x/texlive"), "scheme-full", os);
            for want in [
                "selected_scheme scheme-full\n",
                "tlpdbopt_install_docfiles 0\n",
                "instopt_adjustpath 0\n",
                "tlpdbopt_desktop_integration 0\n",
                "tlpdbopt_file_assocs 0\n",
            ] {
                assert!(t.contains(want), "{want} for {os:?}");
            }
            for line in t.lines() {
                let (key, value) = line.split_once(' ').unwrap();
                assert!(ACCEPTED.contains(&key), "{key} is not a profile key");
                assert!(!value.trim().is_empty(), "{key} is empty");
            }
        }
    }

    #[test]
    fn a_path_kpathsea_would_misread_is_refused() {
        let a = |p: &str, os| super::awkward(Path::new(p), os);
        assert_eq!(a("/Users/a b/Library/x/texlive", Os::Mac), None);
        assert_eq!(
            a(r"C:\Users\a b\AppData\Local\x\texlive", Os::Windows),
            None
        );
        assert_eq!(a("/home/a,b/.local/share/x", Os::Linux), Some(','));
        assert_eq!(a("/home/a:b/x", Os::Linux), Some(':'));
        assert_eq!(a(r"C:\Users\a$b\x", Os::Windows), Some('$'));
        assert_eq!(a(r"C:\Users\{a}\x", Os::Windows), Some('{'));
    }

    #[test]
    fn the_installers_archive_matches_ctans_sha512() {
        let f = std::env::temp_dir().join(format!("rtd-sha512-{}", std::process::id()));
        std::fs::write(&f, "abc").unwrap();
        // FIPS 180-2's example
        let abc = "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f";
        assert!(super::check_sha512(&f, &format!("{abc}  install-tl-unx.tar.gz\n")).is_ok());
        let wrong = abc.replace("ddaf", "ddae");
        assert!(super::check_sha512(&f, &format!("{wrong}  install-tl-unx.tar.gz")).is_err());
        assert!(super::check_sha512(&f, "<html>not found</html>").is_err());
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn mirrors_are_probed_over_https_only() {
        crate::engine::download::tests::has_args(
            &super::probe("https://m"),
            &[
                ["--proto", "=https"],
                ["--proto-redir", "=https"],
                ["--max-time", "5"],
            ],
        );
    }

    #[test]
    fn a_tree_without_its_marker_is_not_installed() {
        let dir = scratch("marker").join("texlive");
        std::fs::create_dir_all(dir.join("bin/x")).unwrap();
        std::fs::write(dir.join("bin/x").join(super::exe("lualatex")), "").unwrap();
        assert_eq!(
            super::installed(&dir),
            None,
            "cut short: lualatex is there, no marker"
        );
        std::fs::write(dir.join(super::MARKER), "").unwrap();
        assert_eq!(super::installed(&dir), Some(dir.join("bin/x")));
    }

    #[cfg(unix)]
    #[test]
    fn a_working_tree_without_its_marker_is_kept_a_broken_one_removed() {
        use std::os::unix::fs::PermissionsExt;
        let tree = |name: &str, works: bool| {
            let dir = scratch(name).join("texlive");
            let bin = dir.join("bin/x");
            std::fs::create_dir_all(&bin).unwrap();
            let body = if works { "echo ok" } else { "exit 1" };
            for p in ["lualatex", "kpsewhich"] {
                let f = bin.join(p);
                std::fs::write(&f, format!("#!/bin/sh\n{body}\n")).unwrap();
                std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            dir
        };
        let old = tree("old-install", true);
        super::clean_leftovers_in(&old);
        assert_eq!(
            super::installed(&old),
            Some(old.join("bin/x")),
            "kept and marked"
        );
        let broken = tree("cut-short", false);
        super::clean_leftovers_in(&broken);
        assert!(!broken.exists(), "a tree whose LuaLaTeX fails is removed");
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rtd-tl-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The whole install with a stand-in for install-tl: a shell script, given the folder.
    #[cfg(unix)]
    mod lifecycle {
        use super::super::{
            JOB, Stage, Stop, TexInstall, TexProgress, cancel_tex_install, install_into, installed,
        };
        use super::scratch;
        use std::path::{Path, PathBuf};
        use std::process::Command;
        use std::sync::Mutex;
        use std::time::{Duration, Instant};

        /// One install per process at a time, as in the app.
        static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

        /// A TeX put in $1 whose lualatex runs and whose kpsewhich finds LuaLaTeX's format.
        const TEX: &str = r#"b="$1/bin/fake"; mkdir -p "$b"
printf '#!/bin/sh\necho "This is LuaHBTeX"\n' > "$b/lualatex"
printf '#!/bin/sh\n[ "$1" = -engine=luahbtex ] && [ "$2" = lualatex.fmt ] && echo /f/lualatex.fmt\n' > "$b/kpsewhich"
chmod +x "$b/lualatex" "$b/kpsewhich"
"#;

        /// The same, but its format is not there.
        const BROKEN_TEX: &str = r#"b="$1/bin/fake"; mkdir -p "$b"
printf '#!/bin/sh\necho "This is LuaHBTeX"\n' > "$b/lualatex"
printf '#!/bin/sh\nexit 1\n' > "$b/kpsewhich"
chmod +x "$b/lualatex" "$b/kpsewhich"
"#;

        const PACKAGE: &str = "echo 'Installing [0001/0002, time/total: 00:01/00:02]: a [1k]'\n";

        fn script(dir: &Path, body: &str) -> Command {
            let mut c = Command::new("sh");
            c.arg("-c").arg(body).arg("sh").arg(dir);
            c
        }

        /// install_into with `body` as install-tl, from one mirror.
        fn install(dir: &Path, body: &str, report: &dyn Fn(TexProgress)) -> TexInstall {
            install_into(
                dir,
                report,
                || vec!["https://only"],
                |_, _, _| Ok(script(dir, body)),
            )
        }

        fn fresh(name: &str) -> PathBuf {
            scratch(name).join("texlive")
        }

        #[test]
        fn a_whole_install_is_marked_and_found_and_not_done_twice() {
            let _one = ONE_AT_A_TIME.lock().unwrap();
            let dir = fresh("success");
            let body = format!("{TEX}{PACKAGE}echo 'running mktexlsr'\nexit 0");
            let seen = Mutex::new(vec![]);
            let got = install(&dir, &body, &|p| seen.lock().unwrap().push(p.stage));
            assert_eq!(got, TexInstall::Done(vec![]));
            assert_eq!(installed(&dir), Some(dir.join("bin/fake")));
            assert!(
                !dir.with_file_name("texlive-installer").exists(),
                "the installer's folder goes"
            );
            let seen = seen.into_inner().unwrap();
            assert!(seen.contains(&Stage::Packages) && seen.last() == Some(&Stage::Verifying));
            let again = install_into(&dir, &|_| {}, Vec::new, |_, _, _| {
                panic!("an installed TeX Live is not installed again")
            });
            assert_eq!(again, TexInstall::Done(vec![]));
        }

        #[test]
        fn errors_in_steps_that_spare_lualatex_are_warnings_and_the_tree_stays() {
            let _one = ONE_AT_A_TIME.lock().unwrap();
            let dir = fresh("usable");
            let body = format!(
                "{TEX}{PACKAGE}echo 'fmtutil-sys: xetex.fmt failed' >&2\necho 'install-tl: errors in installation reported above' >&2\nexit 1"
            );
            let got = install(&dir, &body, &|_| {});
            assert_eq!(
                got,
                TexInstall::Done(vec!["fmtutil-sys: xetex.fmt failed".into()])
            );
            assert!(installed(&dir).is_some(), "an hour's install is kept");
        }

        #[test]
        fn a_tex_whose_lualatex_does_not_work_is_removed_and_said_why() {
            let _one = ONE_AT_A_TIME.lock().unwrap();
            let dir = fresh("broken");
            let body = format!("{BROKEN_TEX}{PACKAGE}echo 'mktexlsr: no space' >&2\nexit 1");
            let got = install(&dir, &body, &|_| {});
            assert!(
                matches!(&got, TexInstall::Failed(why) if why.contains("mktexlsr: no space")),
                "{got:?}"
            );
            assert!(!dir.exists());
        }

        #[test]
        fn a_tex_that_exits_well_but_does_not_work_is_not_installed() {
            let _one = ONE_AT_A_TIME.lock().unwrap();
            let dir = fresh("broken-ok");
            let got = install(&dir, &format!("{BROKEN_TEX}{PACKAGE}exit 0"), &|_| {});
            assert!(matches!(got, TexInstall::Failed(_)), "{got:?}");
            assert!(!dir.exists());
        }

        #[test]
        fn cancel_while_packages_install_ends_the_installer_and_removes_its_work() {
            let _one = ONE_AT_A_TIME.lock().unwrap();
            let dir = fresh("cancel");
            let body = format!("{TEX}{PACKAGE}sleep 30\nexit 0");
            let start = Instant::now();
            let got = install(&dir, &body, &|p| {
                if p.stage == Stage::Packages {
                    cancel_tex_install();
                }
            });
            assert_eq!(got, TexInstall::Idle);
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "the installer was ended"
            );
            assert!(!dir.exists());
            assert!(JOB.lock().unwrap().is_none(), "a new install may start");
        }

        #[test]
        fn cancel_before_the_installer_starts_starts_nothing() {
            let _one = ONE_AT_A_TIME.lock().unwrap();
            for during in ["probe", "download"] {
                let dir = fresh(&format!("early-{during}"));
                let ran = dir.with_file_name("installer-ran");
                let tried = Mutex::new(0);
                let got = install_into(
                    &dir,
                    &|_| {},
                    || {
                        if during == "probe" {
                            cancel_tex_install();
                        }
                        vec!["https://only"]
                    },
                    |_, _, _| {
                        *tried.lock().unwrap() += 1;
                        cancel_tex_install();
                        Ok(script(&dir, &format!("touch '{}'", ran.display())))
                    },
                );
                assert_eq!(got, TexInstall::Idle, "cancelled during the {during}");
                assert!(
                    !ran.exists(),
                    "no installer started after Cancel ({during})"
                );
                assert_eq!(*tried.lock().unwrap(), usize::from(during == "download"));
                assert!(JOB.lock().unwrap().is_none());
            }
        }

        #[test]
        fn a_mirror_that_fails_before_any_package_is_followed_by_the_next_once() {
            let _one = ONE_AT_A_TIME.lock().unwrap();
            let dir = fresh("retry");
            let tried = Mutex::new(vec![]);
            let got = install_into(
                &dir,
                &|_| {},
                || vec!["https://a", "https://b", "https://c"],
                |_, mirror, _| {
                    tried.lock().unwrap().push(mirror.to_string());
                    match mirror {
                        "https://a" => Err(Stop::Failed("couldn’t download it".into())),
                        "https://b" => Ok(script(
                            &dir,
                            "echo 'Loading x'\necho 'cannot load tlpdb' >&2\nexit 1",
                        )),
                        _ => Ok(script(&dir, &format!("{TEX}{PACKAGE}exit 0"))),
                    }
                },
            );
            assert!(
                matches!(&got, TexInstall::Failed(why) if why.contains("cannot load tlpdb")),
                "{got:?}"
            );
            assert_eq!(*tried.lock().unwrap(), ["https://a", "https://b"]);

            let tried = Mutex::new(vec![]);
            let got = install_into(
                &dir,
                &|_| {},
                || vec!["https://a", "https://b"],
                |_, mirror, _| {
                    tried.lock().unwrap().push(mirror.to_string());
                    Ok(match mirror {
                        "https://a" => script(&dir, &format!("{BROKEN_TEX}{PACKAGE}exit 1")),
                        _ => script(&dir, &format!("{TEX}{PACKAGE}exit 0")),
                    })
                },
            );
            assert!(matches!(got, TexInstall::Failed(_)), "{got:?}");
            assert_eq!(
                *tried.lock().unwrap(),
                ["https://a"],
                "packages were in: not started again elsewhere"
            );
        }

        #[test]
        fn a_second_copy_of_the_app_does_not_install_over_the_first() {
            let _one = ONE_AT_A_TIME.lock().unwrap();
            let dir = fresh("locked");
            std::fs::create_dir_all(dir.join("bin")).unwrap();
            let _other = super::super::try_lock(&super::super::lock_path(&dir)).unwrap();
            let got = install_into(&dir, &|_| {}, Vec::new, |_, _, _| {
                panic!("nothing is installed while another copy installs")
            });
            assert!(matches!(got, TexInstall::Failed(_)), "{got:?}");
            assert!(dir.join("bin").exists(), "its folder is left alone");
        }
    }

    /// A real TeX Live (this Mac's, read only) passes the check the app makes of its own.
    /// `cargo test --release a_real_tex_live_is_usable -- --ignored`
    #[test]
    #[ignore]
    fn a_real_tex_live_is_usable() {
        assert!(super::usable(Path::new("/usr/local/texlive/2026")));
        assert!(!super::usable(Path::new("/nonexistent/texlive")));
    }

    /// The installer run for real, with TeX Live's small scheme in a folder of its own: its
    /// lines read, a lualatex that runs at the end. `cargo test --release an_install_runs --
    /// --ignored --nocapture` (online, some minutes).
    #[test]
    #[ignore]
    fn an_install_runs() {
        let base = std::env::temp_dir().join("rtd-texlive-test");
        let _ = std::fs::remove_dir_all(&base);
        let dir = base.join("texlive");
        let seen = std::sync::Mutex::new(vec![]);
        let got = super::install_into(
            &dir,
            &|p: TexProgress| seen.lock().unwrap().push(p),
            super::mirrors,
            |work, mirror, all| super::fetch_installer(&dir, "scheme-basic", work, mirror, all),
        );
        let seen = seen.into_inner().unwrap();
        let last = seen
            .iter()
            .rev()
            .find(|p| p.stage == Stage::Packages)
            .cloned();
        println!("{} updates; last packages: {last:?}; {got:?}", seen.len());
        assert!(matches!(got, TexInstall::Done(_)));
        assert!(seen.iter().any(|p| p.stage == Stage::Finishing));
        let last = last.unwrap();
        assert_eq!(last.done, last.total);
        assert!(last.bytes > 1_000_000 && last.speed().is_some());
        assert!(super::installed(&dir).is_some());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// One of the mirrors answers from here. `cargo test a_mirror_answers -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn a_mirror_answers() {
        let m = super::mirrors();
        println!("fastest first: {m:?}");
        assert!(m.len() > 1);
    }
}
