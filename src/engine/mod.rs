// The realtime-tex engine, run as `rtex serve`.
mod download;
mod process;
mod protocol;
mod tex_install;

pub use download::{download_rtex, needs_download};
pub use tex_install::{Stage, TexInstall, TexProgress, cancel_tex_install, install_tex};

pub use process::*;
pub use protocol::*;

/// What a download or an install cut short by a crash or a quit left behind, removed, and the
/// rtex runs a crashed app left running ended, on a thread of its own (half a TeX Live is
/// gigabytes).
pub fn clean_leftovers() {
    std::thread::spawn(|| {
        download::clean_leftovers();
        tex_install::clean_leftovers();
        #[cfg(unix)]
        process::end_leftover_groups();
    });
}
