// rtex fetched by the app for itself: one release of rtex's on GitHub, this system's build, checked
// against the SHA-256 the release published and unpacked into the app's own folder (curl and tar,
// which macOS, Windows 10+ and Linux all have). shortcut: the release is pinned; bump RTEX_TAG and
// BUILD from its SHA256SUMS when rtex changes.
use super::process::{app_dir, command, exe, find_binary, try_lock};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

const RELEASES: &str = "https://github.com/HenryXiaoYang/realtime-tex/releases/download";
const RTEX_TAG: &str = "v0.0.7";

/// This system's build in that release, and its SHA-256 (scripts/dmg.sh reads the Mac's here).
const BUILD: Option<(&str, &str)> = match (
    std::env::consts::OS.as_bytes(),
    std::env::consts::ARCH.as_bytes(),
) {
    (b"macos", b"aarch64") => Some((
        "rtex-aarch64-apple-darwin.tar.gz",
        "9c8090dcd2c286e603b87995a1952d2a252ee9167a5850e3048489eccfdbfb64",
    )),
    (b"macos", b"x86_64") => Some((
        "rtex-x86_64-apple-darwin.tar.gz",
        "f8fc0761ee1f9a9540d012bed09ca734f984a92ac2c1e96c36ec0858e9354c94",
    )),
    (b"linux", b"aarch64") => Some((
        "rtex-aarch64-unknown-linux-gnu.tar.gz",
        "96725f34cf1297a00fcd81ee33af43fcb92dbc2f7d0750ee484a2e1778a3e621",
    )),
    (b"linux", b"x86_64") => Some((
        "rtex-x86_64-unknown-linux-gnu.tar.gz",
        "f1c962c27f02fb760cb116b80bbf2930a51c26de2fc5b4a48f883953f2104340",
    )),
    (b"windows", b"x86_64") => Some((
        "rtex-x86_64-pc-windows-msvc.zip",
        "3f461d021057cd262eb654907001d012ecd50967fb2b8d74cd708f4b9080134d",
    )),
    _ => None,
};

/// curl's options for every download: https only, redirects too (a proxy from https_proxy still
/// works), and a stalled connection given up rather than waited on for ever.
pub(super) const HTTPS_ONLY: [&str; 4] = ["--proto", "=https", "--proto-redir", "=https"];

/// curl, for a download that fails on an error page.
pub(super) fn curl() -> Command {
    let mut c = command("curl");
    c.args(HTTPS_ONLY).args([
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--retry",
        "2",
        "--connect-timeout",
        "15",
        "--speed-limit",
        "1024",
        "--speed-time",
        "60",
    ]);
    c
}

/// A file's SHA-256 or SHA-512, in hex.
pub(super) fn sha_hex<D: Digest>(file: &Path) -> std::io::Result<String> {
    Ok(D::digest(std::fs::read(file)?)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// What curl or tar said on failure, without its prefix: "Could not resolve host: github.com".
pub(super) fn said(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .trim()
        .trim_start_matches("curl: ")
        .trim_start_matches(|c: char| c == '(' || c == ')' || c.is_ascii_digit() || c == ' ')
        .to_string()
}

fn dir() -> PathBuf {
    app_dir().join("rtex")
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
            let bin = e.path().join("bin").join(exe("rtex"));
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

/// Download and unpack rtex; its binary. Blocks: run it off the main thread.
pub fn download_rtex() -> Result<PathBuf, String> {
    let (asset, sha256) = BUILD.ok_or("rtex isn’t made for this kind of computer")?;
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    // another copy of the app getting it as well: this one waits, then finds it there
    let lock = std::fs::File::options()
        .create(true)
        .write(true)
        .truncate(false)
        .open(lock_path())
        .map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    if let Some(bin) = downloaded() {
        return Ok(bin);
    }
    remove_partial();
    let work = dir().join(".download");
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let got = fetch(asset, sha256, &work);
    let _ = std::fs::remove_dir_all(&work);
    got
}

fn lock_path() -> PathBuf {
    dir().join(".download.lock")
}

/// Downloads a crash or a quit cut short, removed (not one another copy of the app is making).
pub(super) fn clean_leftovers() {
    if let Some(_lock) = try_lock(&lock_path()) {
        remove_partial();
    }
}

fn remove_partial() {
    for e in std::fs::read_dir(dir()).into_iter().flatten().flatten() {
        if e.file_name().to_string_lossy().starts_with(".download") && e.path().is_dir() {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

fn fetch(asset: &str, sha256: &str, work: &Path) -> Result<PathBuf, String> {
    let archive = work.join(asset);
    run(
        curl()
            .arg("-o")
            .arg(&archive)
            .arg(format!("{RELEASES}/{RTEX_TAG}/{asset}")),
        "download it",
    )?;
    if sha_hex::<Sha256>(&archive).map_err(|e| e.to_string())? != sha256 {
        return Err("the download didn’t match rtex’s published checksum".into());
    }
    // tar reads zip files too (bsdtar, on macOS and Windows)
    run(
        command("tar").arg("-xf").arg(&archive).arg("-C").arg(work),
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
    if !dest.join("bin").join(exe("rtex")).is_file() {
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::rename(&top, &dest).map_err(|e| e.to_string())?;
    }
    let bin = dest.join("bin").join(exe("rtex"));
    if bin.is_file() {
        Ok(bin)
    } else {
        Err("the download had no rtex in it".into())
    }
}

/// `cmd` run to its end; what it said on failure.
fn run(cmd: &mut Command, what: &str) -> Result<(), String> {
    match cmd.stdin(std::process::Stdio::null()).output() {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(format!("couldn’t {what}: {}", said(&o.stderr))),
        Err(e) => Err(format!("couldn’t {what} ({e})")),
    }
}

#[cfg(test)]
pub(super) mod tests {
    #[test]
    fn versions_order_as_numbers() {
        let v = |s| super::version(s).unwrap();
        assert!(v("rtex-0.0.10-aarch64-apple-darwin") > v("rtex-0.0.9-aarch64-apple-darwin"));
        assert_eq!(v("rtex-1.2.3-x86_64-pc-windows-msvc"), vec![1, 2, 3]);
        assert!(super::version(".download-123").is_none());
    }

    /// Every pair of `args` consecutive in `cmd`'s arguments.
    pub(crate) fn has_args(cmd: &std::process::Command, args: &[[&str; 2]]) {
        let got: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        for pair in args {
            assert!(got.windows(2).any(|w| w == pair), "{pair:?} in {got:?}");
        }
    }

    #[test]
    fn every_download_is_https_only_and_gives_up_on_a_stalled_connection() {
        has_args(
            &super::curl(),
            &[
                ["--proto", "=https"],
                ["--proto-redir", "=https"],
                ["--connect-timeout", "15"],
                ["--speed-limit", "1024"],
                ["--speed-time", "60"],
            ],
        );
    }

    #[test]
    fn a_files_sha256_is_the_published_one() {
        let f = std::env::temp_dir().join(format!("rtd-sha256-{}", std::process::id()));
        std::fs::write(&f, "abc").unwrap();
        // FIPS 180-2's example
        assert_eq!(
            super::sha_hex::<sha2::Sha256>(&f).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_file(&f);
    }

    /// The pinned release fetched, checked and unpacked where the app looks, and it runs.
    /// `cargo test downloads_the_pinned_rtex -- --ignored` (online; into the app's folder).
    #[test]
    #[ignore]
    fn downloads_the_pinned_rtex() {
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
