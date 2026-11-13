// TeX Live installed by the app for itself when no LuaLaTeX is found: TeX Live's own installer
// (install-tl, the same on macOS, Windows and Linux) run unattended from a profile, the full
// scheme without documentation or sources, into a folder of the app's (no administrator
// password, nothing changed outside it). Its lines say how far it is:
// "Installing [0012/4556, time/total: 00:31/41:02]: amsmath [2k]".
use super::process::{home, kill_group};
use std::io::{BufRead, BufReader};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// CTAN mirrors on each continent; the fastest from here is used. mirror.ctan.org picks one by
/// the address alone, and from some places the one it picks barely moves.
const MIRRORS: &[&str] = &[
    "https://mirror.ctan.org",
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

/// The mirror that sent half a megabyte of the installer fastest, all tried at once (5 s each
/// at most); mirror.ctan.org when none answered.
fn fastest_mirror() -> &'static str {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let tries: Vec<_> = MIRRORS
        .iter()
        .map(|m| {
            std::thread::spawn(move || {
                let out = Command::new("curl")
                    .args([
                        "--silent",
                        "--location",
                        "--range",
                        "0-524287",
                        "--max-time",
                        "5",
                    ])
                    .args(["-o", null, "-w", "%{http_code} %{speed_download}"])
                    .arg(format!("{m}/{TLNET}/install-tl-unx.tar.gz"))
                    .output()
                    .ok()?;
                let said = String::from_utf8_lossy(&out.stdout).to_string();
                let (code, speed) = said.split_once(' ')?;
                let speed: f64 = speed.trim().parse().ok()?;
                // a mirror that timed out reports what little it sent: only a whole answer counts
                (out.status.success() && matches!(code, "200" | "206")).then_some((*m, speed))
            })
        })
        .collect();
    tries
        .into_iter()
        .filter_map(|t| t.join().ok().flatten())
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(MIRRORS[0], |(m, _)| m)
}

/// The installer running (its process group), for Cancel and for quitting.
static RUNNING: Mutex<Option<u32>> = Mutex::new(None);

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

/// Its bin folder once installed: bin/<platform> (universal-darwin, x86_64-linux, windows…).
pub(super) fn own_tex_bin() -> Option<PathBuf> {
    bin_in(&tex_dir())
}

fn bin_in(dir: &Path) -> Option<PathBuf> {
    let lualatex = format!("lualatex{}", std::env::consts::EXE_SUFFIX);
    std::fs::read_dir(dir.join("bin"))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|d| d.join(&lualatex).is_file())
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
            (Stage::Finishing, _) => 1.0,
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

/// The install stopped (Cancel, or the app quitting); what it had installed goes with it when
/// `install_tex` returns.
pub fn cancel_tex_install() {
    if let Some(pid) = RUNNING.lock().unwrap().take() {
        kill_group(pid);
    }
}

/// Install TeX Live, telling `report` how far it is; its bin folder. Blocks for as long as the
/// install takes (most of an hour): run it on a thread of its own.
pub fn install_tex(report: impl Fn(TexProgress)) -> Result<PathBuf, String> {
    let dir = tex_dir();
    let work = dir.with_file_name("texlive-installer");
    let _ = std::fs::remove_dir_all(&work);
    // an install that stopped half way is not finished: install-tl starts again from nothing
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let got = install(&dir, &work, "scheme-full", &report);
    let _ = std::fs::remove_dir_all(&work);
    if got.is_err() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    got
}

fn install(
    dir: &Path,
    work: &Path,
    scheme: &str,
    report: &impl Fn(TexProgress),
) -> Result<PathBuf, String> {
    let mut progress = TexProgress::default();
    report(progress.clone());
    if !cfg!(windows) && Command::new("perl").arg("-v").output().is_err() {
        return Err("TeX Live’s installer needs Perl. Install it (on Debian and Ubuntu: sudo apt install perl), then try again".into());
    }
    let archive = work.join(if cfg!(windows) {
        "install-tl.zip"
    } else {
        "install-tl-unx.tar.gz"
    });
    let name = archive
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let tlnet = format!("{}/{TLNET}", fastest_mirror());
    let fetched = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--retry",
            "2",
            "-o",
        ])
        .arg(&archive)
        .arg(format!("{tlnet}/{name}"))
        .output()
        .map_err(|e| format!("couldn’t download TeX Live’s installer ({e})"))?;
    if !fetched.status.success() {
        return Err(format!(
            "couldn’t download TeX Live’s installer: {}",
            String::from_utf8_lossy(&fetched.stderr)
                .trim()
                .trim_start_matches("curl: ")
        ));
    }
    let unpacked = Command::new("tar")
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(work)
        .status()
        .map_err(|e| e.to_string())?;
    if !unpacked.success() {
        return Err("couldn’t unpack TeX Live’s installer".into());
    }
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
    std::fs::write(&profile, profile_text(dir, scheme)).map_err(|e| e.to_string())?;
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/C")
            .arg(installer.join("install-tl-windows.bat"))
            .arg("-no-gui");
        c
    } else {
        let mut c = Command::new("perl");
        c.arg(installer.join("install-tl"));
        c
    };
    cmd.arg("-profile")
        .arg(&profile)
        .arg("-repository")
        .arg(&tlnet)
        .current_dir(work)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // its own process group, so that Cancel ends it with what it runs
    #[cfg(unix)]
    cmd.process_group(0);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("couldn’t start TeX Live’s installer ({e})"))?;
    *RUNNING.lock().unwrap() = Some(child.id());
    // its warnings, the last of which says why it stopped
    let stderr = child.stderr.take().unwrap();
    let said = std::thread::spawn(move || {
        BufReader::new(stderr)
            .lines()
            .map_while(Result::ok)
            .filter(|l| !l.trim().is_empty())
            .last()
    });
    let mut last = String::new();
    for line in BufReader::new(child.stdout.take().unwrap())
        .lines()
        .map_while(Result::ok)
    {
        if progress.read(&line) {
            report(progress.clone());
        }
        if !line.trim().is_empty() {
            last = line;
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let cancelled = RUNNING.lock().unwrap().take().is_none();
    let why = said.join().ok().flatten().unwrap_or(last);
    if cancelled {
        return Err("cancelled".into());
    }
    match bin_in(dir) {
        Some(bin) if status.success() => Ok(bin),
        _ => Err(format!("TeX Live’s installer stopped: {}", why.trim())),
    }
}

/// The answers install-tl would ask for: everything but documentation and sources, in `dir`,
/// for this user only, nothing outside it touched (no PATH, no file types, no menu entries).
fn profile_text(dir: &Path, scheme: &str) -> String {
    // install-tl reads / on Windows too
    let d = dir.to_string_lossy().replace('\\', "/");
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
instopt_portable 0
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

#[cfg(test)]
mod tests {
    use super::{Stage, TexProgress};

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

    #[test]
    fn the_profile_installs_everything_but_docs_for_this_user() {
        let t = super::profile_text(std::path::Path::new(r"C:\Users\a b\texlive"), "scheme-full");
        assert!(t.contains("TEXDIR C:/Users/a b/texlive\n"));
        assert!(t.contains("selected_scheme scheme-full\n"));
        assert!(
            t.contains("tlpdbopt_install_docfiles 0\n") && t.contains("instopt_adjustpath 0\n")
        );
    }

    /// The installer run for real, with TeX Live's small scheme in a folder of its own: its
    /// lines read, a lualatex that runs at the end. `cargo test --release an_install_runs --
    /// --ignored --nocapture` (online, some minutes).
    #[test]
    #[ignore]
    fn an_install_runs() {
        let base = std::env::temp_dir().join("rtd-texlive-test");
        let _ = std::fs::remove_dir_all(&base);
        let (dir, work) = (base.join("texlive"), base.join("work"));
        std::fs::create_dir_all(&work).unwrap();
        let seen = std::sync::Mutex::new(vec![]);
        let got = super::install(&dir, &work, "scheme-basic", &|p: TexProgress| {
            seen.lock().unwrap().push(p)
        });
        let seen = seen.into_inner().unwrap();
        let last = seen
            .iter()
            .rev()
            .find(|p| p.stage == Stage::Packages)
            .cloned();
        println!("{} updates; last packages: {last:?}", seen.len());
        let bin = got.unwrap();
        assert!(seen.iter().any(|p| p.stage == Stage::Finishing));
        let last = last.unwrap();
        assert_eq!(last.done, last.total);
        assert!(last.bytes > 1_000_000 && last.speed().is_some());
        let v = std::process::Command::new(bin.join("lualatex"))
            .arg("--version")
            .output()
            .unwrap();
        assert!(v.status.success());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// One of the mirrors answers from here. `cargo test a_mirror_answers -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn a_mirror_answers() {
        let m = super::fastest_mirror();
        println!("fastest: {m}");
        assert!(super::MIRRORS.contains(&m));
    }
}
