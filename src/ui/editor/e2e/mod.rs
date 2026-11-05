// End-to-end tests of the pages with a real rtex: typing, the ribbon, the Insert tab, the
// sidebar, each typesetting a document and checking what the pages and the source do. They are
// #[ignore]d (they need a TeX installation): `cargo test --release <name> -- --ignored
// --nocapture`, with RTD_TYPE=/abs/copy.tex for those that take a document (use a copy: rtex
// writes beside it).
mod insert;
mod perf;
mod sidebar;
mod typing;

use super::Doc;
use crate::engine;
use crate::ui::settings::AppSettings;
use gpui_kit::{Entity, TestAppContext, VisualTestContext, Window};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// The document RTD_TYPE names: its folder and file name.
pub(super) fn rtd_type() -> (PathBuf, String) {
    let file = std::env::var("RTD_TYPE").expect("RTD_TYPE=/abs/path/main.tex");
    let path = PathBuf::from(&file);
    (
        path.parent().unwrap().to_path_buf(),
        path.file_name().unwrap().to_string_lossy().to_string(),
    )
}

/// A fresh folder for a test's document `tex` (and pic.png beside it).
pub(super) fn scratch(name: &str, tex: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rtd-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.tex"), tex).unwrap();
    std::fs::write(dir.join("pic.png"), PNG).unwrap();
    dir
}

/// `main` in `root` opened in a window of its own, with rtex, laid out (`blocks` of them on the
/// pages at least) and settled.
pub(super) fn open<'a>(
    cx: &'a mut TestAppContext,
    root: PathBuf,
    main: &str,
    blocks: usize,
) -> (Entity<Doc>, &'a mut VisualTestContext) {
    // rtex wakes the test from its own thread
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_global(AppSettings::default());
    });
    engine::configure("", "/Library/TeX/texbin");
    let main = main.to_string();
    let (doc, cx) = cx.add_window_view(|window: &mut Window, cx| {
        Doc::new(root, main, Default::default(), window, cx)
    });
    wait(cx, &doc, "the first layout", |d| {
        d.spans_pending == 0 && d.blocks.iter().filter(|b| !b.rows.is_empty()).count() > blocks
    });
    calm(cx, &doc);
    (doc, cx)
}

/// Run the app until `done` holds for the doc (rtex answers from its own process).
pub(super) fn wait(
    cx: &mut VisualTestContext,
    doc: &Entity<Doc>,
    what: &str,
    done: impl Fn(&Doc) -> bool,
) {
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        cx.run_until_parked();
        if cx.read(|cx| done(doc.read(cx))) {
            return;
        }
        assert!(Instant::now() < until, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Run until rtex has typeset everything and stays so for a while: LaTeX may run again
/// (references) just after a layout comes in.
pub(super) fn calm(cx: &mut VisualTestContext, doc: &Entity<Doc>) {
    let settled = |d: &Doc| d.spans_pending == 0 && !d.compiling;
    let mut since = Instant::now();
    loop {
        wait(cx, doc, "the edit typeset", settled);
        std::thread::sleep(Duration::from_millis(30));
        cx.run_until_parked();
        if !cx.read(|cx| settled(doc.read(cx))) {
            since = Instant::now();
        } else if since.elapsed() > Duration::from_millis(300) {
            return;
        }
    }
}

fn src(cx: &mut VisualTestContext, doc: &Entity<Doc>) -> String {
    cx.read(|cx| doc.read(cx).source.clone())
}

/// The edit typesets without a LaTeX error, and one undo takes it back.
pub(super) fn typesets_and_undoes(
    cx: &mut VisualTestContext,
    doc: &Entity<Doc>,
    base: &str,
    what: &str,
) {
    calm(cx, doc);
    let status = cx.read(|cx| doc.read(cx).status.clone());
    assert!(!status.contains("LaTeX"), "{what}: {status}");
    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    assert_eq!(src(cx, doc), base, "{what}: one undo step");
    calm(cx, doc);
}

/// A 1×1 PNG.
pub(super) const PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0,
    0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 0, 0, 3, 1,
    1, 0, 201, 254, 146, 239, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

/// Every kind of element the sidebar knows, in the ways people write them.
pub(super) const DOC: &str = r"\documentclass[11pt]{article}
\usepackage{graphicx}
\usepackage{amsmath}
\usepackage{enumitem}
\begin{document}
\section{Intro}\label{sec:intro}
Some text pointing at Figure~\ref{fig:a}.
\subsection*{Unnumbered}
\begin{figure}[htbp]
  \centering
  \includegraphics[width=0.5\linewidth]{pic.png}
  \caption{A picture}
  \label{fig:a}
\end{figure}
\begin{figure}[h]
\centering
\includegraphics{pic}
\end{figure}
\begin{table}[t]
  \centering
  \begin{tabular}{|l|r|}
\hline
a & b \\
\hline
  \end{tabular}
  \caption{Numbers}
\end{table}
\begin{equation}
  E = mc^2
  \label{eq:e}
\end{equation}
\begin{equation*}
  a = b
\end{equation*}
\begin{align}
  x &= 1 \\
  y &= 2
\end{align}
\[ z = 3 \]
\begin{itemize}
  \item one
  \item two
\end{itemize}
\begin{enumerate}[label=(\alph*)]
  \item first
\end{enumerate}
\includegraphics[height=2cm]{pic.png}

\begin{minipage}{0.4\linewidth}
Inside a minipage.
\end{minipage}

\begin{center}
Centred text.
\end{center}

Last paragraph.
\end{document}
";
