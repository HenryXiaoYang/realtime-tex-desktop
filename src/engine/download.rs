// rtex fetched by the app for itself: the latest release for this system from rtex's GitHub
// releases, unpacked into the app's own folder (curl and tar, which macOS, Windows 10+ and Linux
// all have). shortcut: a copy once there is kept, newer releases are not fetched; check the
// latest tag at launch if rtex starts changing often.
use super::process::{app_dir, find_binary};
use std::path::{Path, PathBuf};
use std::process::Command;

const RELEASES: &str = "https://github.com/HenryXiaoYang/realtime-tex/releases/latest/download";

/// The release built for this system.
const TARGET: Option<&str> = match (
    std::env::consts::OS.as_bytes(),
    std::env::consts::ARCH.as_bytes(),
) {
    (b"macos", b"aarch64") => Some("aarch64-apple-darwin"),
    (b"macos", b"x86_64") => Some("x86_64-apple-darwin"),
    (b"linux", b"aarch64") => Some("aarch64-unknown-linux-gnu"),
    (b"linux", b"x86_64") => Some("x86_64-unknown-linux-gnu"),
    (b"windows", b"x86_64") => Some("x86_64-pc-windows-msvc"),
    _ => None,
};

fn dir() -> PathBuf {
    app_dir().join("rtex")
}

fn exe() -> String {
    format!("rtex{}", std::env::consts::EXE_SUFFIX)
}

/// rtex is nowhere the app looks (an rtex chosen in the settings counts, there or not).
pub fn needs_download() -> bool {
    find_binary().is_none()
}

/// The newest rtex downloaded: each release unpacks to rtex-<version>-<target>.
pub(super) fn downloaded() -> Option<PathBuf> {
    std::fs::read_dir(dir())
        .ok()?
        .flatten()
        .filter_map(|e| {
            let v = version(&e.file_name().to_string_lossy())?;
            let bin = e.path().join("bin").join(exe());
            bin.is_file().then_some((v, bin))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, bin)| bin)
}

/// 0.0.10 from rtex-0.0.10-aarch64-apple-darwin, as numbers (0.0.10 is newer than 0.0.9).
fn version(folder: &str) -> Option<Vec<u32>> {
    folder
        .strip_prefix("rtex-")?
        .split('-')
        .next()?
        .split('.')
        .map(|n| n.parse().ok())
        .collect()
}

/// Download and unpack the latest rtex; its binary. Blocks: run it off the main thread.
pub fn download_rtex() -> Result<PathBuf, String> {
    let target = TARGET.ok_or("rtex isn’t made for this kind of computer")?;
    let kind = if cfg!(windows) { "zip" } else { "tar.gz" };
    let work = dir().join(format!(".download-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let got = fetch(target, kind, &work);
    let _ = std::fs::remove_dir_all(&work);
    got
}

fn fetch(target: &str, kind: &str, work: &Path) -> Result<PathBuf, String> {
    let archive = work.join(format!("rtex.{kind}"));
    run(
        Command::new("curl")
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
            .arg(format!("{RELEASES}/rtex-{target}.{kind}")),
        "download it",
    )?;
    // tar reads zip files too (bsdtar, on macOS and Windows)
    run(
        Command::new("tar")
            .arg("-xf")
            .arg(&archive)
            .arg("-C")
            .arg(work),
        "unpack it",
    )?;
    let top = std::fs::read_dir(work)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.is_dir() && version(&p.file_name().unwrap_or_default().to_string_lossy()).is_some()
        })
        .ok_or("the download had no rtex in it")?;
    let dest = dir().join(top.file_name().unwrap_or_default());
    if !dest.join("bin").join(exe()).is_file() {
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::rename(&top, &dest).map_err(|e| e.to_string())?;
    }
    let bin = dest.join("bin").join(exe());
    if bin.is_file() {
        Ok(bin)
    } else {
        Err("the download had no rtex in it".into())
    }
}

/// `cmd` run to its end; what it said on failure (curl: "Could not resolve host: github.com").
fn run(cmd: &mut Command, what: &str) -> Result<(), String> {
    match cmd.output() {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            let said = String::from_utf8_lossy(&o.stderr);
            let said = said
                .trim()
                .trim_start_matches("curl: ")
                .trim_start_matches(|c: char| {
                    c == '(' || c == ')' || c.is_ascii_digit() || c == ' '
                });
            Err(format!("couldn’t {what}: {said}"))
        }
        Err(e) => Err(format!("couldn’t {what} ({e})")),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn versions_order_as_numbers() {
        let v = |s| super::version(s).unwrap();
        assert!(v("rtex-0.0.10-aarch64-apple-darwin") > v("rtex-0.0.9-aarch64-apple-darwin"));
        assert_eq!(v("rtex-1.2.3-x86_64-pc-windows-msvc"), vec![1, 2, 3]);
        assert!(super::version(".download-123").is_none());
    }

    /// The latest release fetched and unpacked where the app looks, and it runs.
    /// `cargo test downloads_the_latest_rtex -- --ignored` (online; into the app's folder).
    #[test]
    #[ignore]
    fn downloads_the_latest_rtex() {
        let bin = super::download_rtex().unwrap();
        assert_eq!(super::downloaded(), Some(bin.clone()));
        let out = std::process::Command::new(&bin)
            .arg("--help")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
