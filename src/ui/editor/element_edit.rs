// The sidebar's side of a document: the element at the caret or selected (else the document),
// its fields, and setting one as an undo step, with the packages it needs.
use super::*;
use document::element::{self, Element, Field};

impl Doc {
    /// The element the sidebar shows: the selected block's, the caret's, else the document.
    pub fn element(&self) -> Element {
        if self.active.is_none()
            && let Some((lo, _)) = self.selection()
        {
            let b = self.blocks[lo].range.clone();
            return element::element_at(&self.source, Some(b.clone()), b.start);
        }
        let Some(c) = self.caret.as_ref().filter(|_| self.active.is_none()) else {
            return element::element_at(&self.source, None, 0);
        };
        let block = self.block_of(c.off).map(|i| self.blocks[i].range.clone());
        element::element_at(&self.source, block, c.off)
    }

    /// The element and its fields now.
    pub fn element_fields(&self) -> (Element, Vec<Field>) {
        let el = self.element();
        let fields = element::fields(&self.source, &el);
        (el, fields)
    }

    /// Give field `key` of the element at hand `value`, as an undo step.
    pub fn set_element_field(&mut self, key: &str, value: &str, cx: &mut Context<Self>) {
        let el = self.element();
        self.set_field_of(&el, key, value, cx);
    }

    /// Give field `key` of the element of `kind` starting at `start` `value`: a field typed in
    /// a moment ago, applied after the caret may have moved on to another element.
    pub fn set_field_at(
        &mut self,
        kind: element::Kind,
        start: usize,
        key: &str,
        value: &str,
        cx: &mut Context<Self>,
    ) {
        let el = if kind == element::Kind::Document {
            element::element_at(&self.source, None, 0)
        } else {
            let Some(b) = self.block_of(start).map(|i| self.blocks[i].range.clone()) else {
                return;
            };
            element::element_at(&self.source, Some(b), (start + 1).min(self.source.len()))
        };
        if el.kind == kind && el.range.start == start {
            self.set_field_of(&el, key, value, cx);
        }
    }

    fn set_field_of(&mut self, el: &Element, key: &str, value: &str, cx: &mut Context<Self>) {
        let Some(change) = element::set(&self.source, el, key, value) else {
            return;
        };
        if self.source.get(change.range.clone()) == Some(change.text.as_str()) {
            return;
        }
        self.checkpoint();
        self.typing = None;
        self.overlay = None;
        // the selection and caret stay on the element (they follow the edit)
        self.replace(change.range.clone(), &change.text, cx);
        self.add_packages(&change.packages, cx);
        cx.notify();
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::typing::tests::{calm, wait};
    use super::Doc;
    use crate::document::element::{self, Element, Input, Kind};
    use crate::engine;
    use crate::ui::settings::AppSettings;
    use gpui_kit::{TestAppContext, Window};
    use std::time::Instant;

    /// Every kind of element the sidebar knows, in the ways people write them.
    pub(in crate::ui::editor) const DOC: &str = r"\documentclass[11pt]{article}
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

    /// What to set a field to: every other choice, the toggle flipped, and for text an
    /// ordinary value, TeX's specials, and nothing.
    fn values(key: &str, input: &Input, now: &str) -> Vec<String> {
        match input {
            Input::Toggle => vec![if now == "true" { "false" } else { "true" }.into()],
            Input::Choice(opts) => opts
                .iter()
                .filter(|(v, _)| *v != now)
                .map(|(v, _)| v.to_string())
                .collect(),
            Input::Number => vec!["5".into(), "".into(), "abc".into()],
            Input::Text if key == "label" => vec!["sweep:x".into(), "".into()],
            Input::Text if key == "file" => vec!["pic".into()],
            Input::Text => vec!["Sweep text".into(), "50% & #1 {x} $y_2$".into(), "".into()],
        }
    }

    /// Every field of every element of a document set to several values through the sidebar's
    /// edit, typeset by rtex: each typesets without a LaTeX error, reads back as set, and one
    /// undo restores the source. `cargo test --release sidebar_everywhere -- --ignored
    /// --nocapture` (RTD_SWEEP_ONLY=Figure:width runs those).
    #[gpui_kit::test]
    #[ignore]
    fn sidebar_everywhere(cx: &mut TestAppContext) {
        let dir = std::env::temp_dir().join(format!("rtd-sidebar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.tex"), DOC).unwrap();
        std::fs::write(dir.join("pic.png"), super::super::inserting::tests::PNG).unwrap();
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        engine::configure("", "/Library/TeX/texbin");
        let (doc, cx) = cx.add_window_view(|window: &mut Window, cx| {
            Doc::new(
                dir.clone(),
                "main.tex".into(),
                Default::default(),
                window,
                cx,
            )
        });
        wait(cx, &doc, "the first layout", |d| {
            d.spans_pending == 0 && d.blocks.iter().filter(|b| !b.rows.is_empty()).count() > 8
        });
        calm(cx, &doc);
        assert!(
            !cx.read(|cx| doc.read(cx).status.contains("LaTeX")),
            "the fixture typesets"
        );
        let base = cx.read(|cx| doc.read(cx).source.clone());
        // the elements: each block's, and an \includegraphics inside one
        let mut els: Vec<Element> = cx.read(|cx| {
            let d = doc.read(cx);
            let mut v = vec![element::element_at(&d.source, None, 0)];
            for b in &d.blocks {
                let r = b.range.clone();
                v.push(element::element_at(&d.source, Some(r.clone()), r.start));
                if let Some(i) = d.source[r.clone()].find("\\includegraphics") {
                    v.push(element::element_at(
                        &d.source,
                        Some(r.clone()),
                        r.start + i + 1,
                    ));
                }
            }
            v
        });
        els.dedup();
        let only = std::env::var("RTD_SWEEP_ONLY").ok();
        let (mut failures, mut n, t0) = (vec![], 0, Instant::now());
        for el in &els {
            let fields = element::fields(&base, el);
            for f in &fields {
                for v in values(f.key, &f.input, &f.value) {
                    let what = format!(
                        "{}:{} = {v:?} (at {})",
                        el.kind.title(),
                        f.key,
                        el.range.start
                    );
                    if only.as_ref().is_some_and(|o| !what.starts_with(o.as_str())) {
                        continue;
                    }
                    n += 1;
                    // refused (not a number, unbalanced): nothing to check but that
                    let Some(change) = element::set(&base, el, f.key, &v) else {
                        continue;
                    };
                    // what the field shows after: the TeX it became
                    let want = f.key.ne("width").then(|| {
                        let mut s = base.clone();
                        s.replace_range(change.range.clone(), &change.text);
                        s
                    });
                    doc.update(cx, |d, cx| {
                        d.status.clear();
                        d.set_field_at(el.kind, el.range.start, f.key, &v, cx);
                    });
                    calm(cx, &doc);
                    let (src, status) = cx.read(|cx| {
                        let d = doc.read(cx);
                        (d.source.clone(), d.status.clone())
                    });
                    if status.contains("LaTeX") {
                        let r = change.range.clone();
                        failures.push(format!(
                            "{what} → {status}\n    {:?} → {:?}",
                            &base[r.clone()],
                            change.text
                        ));
                    }
                    // read back: the element again where it now is
                    let grown = src.len() as isize - base.len() as isize;
                    let start = if change.range.start < el.range.start {
                        (el.range.start as isize + grown).max(0) as usize
                    } else {
                        el.range.start
                    };
                    let end = (el.range.end as isize + grown).max(start as isize) as usize;
                    let el2 = Element {
                        kind: el.kind,
                        range: start..end.min(src.len()),
                    };
                    let back = element::fields(&src, &el2)
                        .into_iter()
                        .find(|g| g.key == f.key)
                        .map(|g| g.value);
                    let shown = |s: &str| s.trim().replace('\\', "");
                    if want.is_some_and(|w| w == src)
                        && el.kind != Kind::Document
                        && !matches!(f.input, Input::Number)
                        && back.as_deref().map(shown) != Some(shown(&v))
                    {
                        failures.push(format!("{what} → reads back {back:?}"));
                    }
                    if src != base {
                        doc.update_in(cx, |d, window, cx| d.undo(window, cx));
                        calm(cx, &doc);
                        if cx.read(|cx| doc.read(cx).source != base) {
                            failures.push(format!("{what} → undo did not restore the source"));
                            let b = base.clone();
                            doc.update(cx, |d, cx| {
                                d.replace(0..d.source.len(), &b, cx);
                            });
                            calm(cx, &doc);
                        }
                    }
                }
            }
        }
        println!(
            "{n} sidebar edits on {} elements in {:?}",
            els.len(),
            t0.elapsed()
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }
}
