// LaTeX autocomplete for the source editors, in the spirit of VS Code's LaTeX Workshop:
// `\` commands, `\begin{`/`\end{` environments, `\ref{` labels, `\cite{` keys, `\usepackage{`
// packages and `\input{`/`\includegraphics{` files. The editor has no snippets, so an item's
// text carries MARK where the caret goes and Doc removes it after the insert.
use crate::ui::settings::AppSettings;
use anyhow::Result;
use gpui_kit::component::input::{CompletionProvider, Rope, RopeExt};
use gpui_kit::{App, AppContext as _, Task, WeakEntity, Window};
use lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit,
    TextEdit,
};
use std::path::{Path, PathBuf};

pub const MARK: char = '\u{E000}';
const MAX_ITEMS: usize = 80;

/// Name and argument shape; the caret goes into the first `{}`.
const COMMANDS: &[(&str, &str)] = &[
    ("begin", "{}"),
    ("end", "{}"),
    ("section", "{}"),
    ("subsection", "{}"),
    ("subsubsection", "{}"),
    ("paragraph", "{}"),
    ("chapter", "{}"),
    ("part", "{}"),
    ("section*", "{}"),
    ("subsection*", "{}"),
    ("label", "{}"),
    ("ref", "{}"),
    ("eqref", "{}"),
    ("pageref", "{}"),
    ("autoref", "{}"),
    ("cref", "{}"),
    ("cite", "{}"),
    ("citep", "{}"),
    ("citet", "{}"),
    ("footnote", "{}"),
    ("emph", "{}"),
    ("textbf", "{}"),
    ("textit", "{}"),
    ("texttt", "{}"),
    ("textsc", "{}"),
    ("underline", "{}"),
    ("text", "{}"),
    ("mathrm", "{}"),
    ("mathbf", "{}"),
    ("mathit", "{}"),
    ("mathcal", "{}"),
    ("mathbb", "{}"),
    ("mathsf", "{}"),
    ("mathfrak", "{}"),
    ("operatorname", "{}"),
    ("frac", "{}{}"),
    ("dfrac", "{}{}"),
    ("tfrac", "{}{}"),
    ("sqrt", "{}"),
    ("binom", "{}{}"),
    ("overline", "{}"),
    ("hat", "{}"),
    ("bar", "{}"),
    ("vec", "{}"),
    ("dot", "{}"),
    ("ddot", "{}"),
    ("tilde", "{}"),
    ("widehat", "{}"),
    ("widetilde", "{}"),
    ("overbrace", "{}"),
    ("underbrace", "{}"),
    ("item", ""),
    ("usepackage", "{}"),
    ("documentclass", "{}"),
    ("input", "{}"),
    ("include", "{}"),
    ("includegraphics", "{}"),
    ("caption", "{}"),
    ("centering", ""),
    ("maketitle", ""),
    ("tableofcontents", ""),
    ("title", "{}"),
    ("author", "{}"),
    ("date", "{}"),
    ("today", ""),
    ("newcommand", "{}{}"),
    ("renewcommand", "{}{}"),
    ("url", "{}"),
    ("href", "{}{}"),
    ("hline", ""),
    ("toprule", ""),
    ("midrule", ""),
    ("bottomrule", ""),
    ("newpage", ""),
    ("clearpage", ""),
    ("noindent", ""),
    ("vspace", "{}"),
    ("hspace", "{}"),
    ("quad", ""),
    ("qquad", ""),
    ("left", ""),
    ("right", ""),
    ("cdot", ""),
    ("cdots", ""),
    ("ldots", ""),
    ("dots", ""),
    ("times", ""),
    ("div", ""),
    ("pm", ""),
    ("mp", ""),
    ("leq", ""),
    ("geq", ""),
    ("neq", ""),
    ("approx", ""),
    ("equiv", ""),
    ("sim", ""),
    ("propto", ""),
    ("infty", ""),
    ("partial", ""),
    ("nabla", ""),
    ("sum", ""),
    ("prod", ""),
    ("int", ""),
    ("iint", ""),
    ("oint", ""),
    ("lim", ""),
    ("log", ""),
    ("ln", ""),
    ("exp", ""),
    ("sin", ""),
    ("cos", ""),
    ("tan", ""),
    ("max", ""),
    ("min", ""),
    ("sup", ""),
    ("inf", ""),
    ("det", ""),
    ("in", ""),
    ("notin", ""),
    ("subset", ""),
    ("subseteq", ""),
    ("supset", ""),
    ("cup", ""),
    ("cap", ""),
    ("setminus", ""),
    ("emptyset", ""),
    ("forall", ""),
    ("exists", ""),
    ("neg", ""),
    ("land", ""),
    ("lor", ""),
    ("to", ""),
    ("mapsto", ""),
    ("rightarrow", ""),
    ("leftarrow", ""),
    ("Rightarrow", ""),
    ("Leftarrow", ""),
    ("iff", ""),
    ("implies", ""),
    ("langle", ""),
    ("rangle", ""),
    ("lfloor", ""),
    ("rfloor", ""),
    ("lceil", ""),
    ("rceil", ""),
    ("mid", ""),
    ("alpha", ""),
    ("beta", ""),
    ("gamma", ""),
    ("delta", ""),
    ("epsilon", ""),
    ("varepsilon", ""),
    ("zeta", ""),
    ("eta", ""),
    ("theta", ""),
    ("vartheta", ""),
    ("iota", ""),
    ("kappa", ""),
    ("lambda", ""),
    ("mu", ""),
    ("nu", ""),
    ("xi", ""),
    ("pi", ""),
    ("rho", ""),
    ("sigma", ""),
    ("tau", ""),
    ("upsilon", ""),
    ("phi", ""),
    ("varphi", ""),
    ("chi", ""),
    ("psi", ""),
    ("omega", ""),
    ("Gamma", ""),
    ("Delta", ""),
    ("Theta", ""),
    ("Lambda", ""),
    ("Xi", ""),
    ("Pi", ""),
    ("Sigma", ""),
    ("Phi", ""),
    ("Psi", ""),
    ("Omega", ""),
];

const ENVIRONMENTS: &[&str] = &[
    "equation",
    "equation*",
    "align",
    "align*",
    "gather",
    "gather*",
    "multline",
    "split",
    "cases",
    "matrix",
    "pmatrix",
    "bmatrix",
    "vmatrix",
    "itemize",
    "enumerate",
    "description",
    "figure",
    "table",
    "tabular",
    "center",
    "abstract",
    "theorem",
    "lemma",
    "proof",
    "definition",
    "corollary",
    "proposition",
    "remark",
    "example",
    "quote",
    "verbatim",
    "minipage",
    "tikzpicture",
    "document",
    "array",
    "thebibliography",
];

const PACKAGES: &[&str] = &[
    "amsmath",
    "amssymb",
    "amsthm",
    "mathtools",
    "graphicx",
    "hyperref",
    "cleveref",
    "geometry",
    "booktabs",
    "tikz",
    "pgfplots",
    "xcolor",
    "enumitem",
    "fontspec",
    "unicode-math",
    "babel",
    "biblatex",
    "natbib",
    "listings",
    "minted",
    "caption",
    "subcaption",
    "float",
    "siunitx",
    "microtype",
    "fancyhdr",
    "titlesec",
    "multirow",
    "array",
    "tabularx",
    "longtable",
    "algorithm",
    "algpseudocode",
    "bm",
    "physics",
];

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "pdf", "eps", "svg"];

pub struct Latex {
    pub root: PathBuf,
    /// The whole file the editor shows part of (the inline box edits one block).
    pub doc: WeakEntity<crate::ui::editor::Doc>,
}

#[derive(Debug, PartialEq)]
enum Ctx {
    Command,
    Env { end: bool },
    Ref,
    Cite,
    Package,
    File { images: bool },
}

/// What is being completed in `line` (the text before the caret on its line): the context and
/// the byte length of the typed prefix.
fn context(line: &str) -> Option<(Ctx, usize)> {
    let word = line
        .bytes()
        .rev()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'*' | b'@'))
        .count();
    let head = &line[..line.len() - word];
    if head.ends_with('\\') && !head.ends_with("\\\\") {
        return Some((Ctx::Command, word + 1));
    }
    // inside `\name{...` (or `\name[..]{...`), maybe after a comma
    let open = line.rfind('{')?;
    let arg = &line[open + 1..];
    if arg.contains('}') {
        return None;
    }
    let mut before = &line[..open];
    if before.ends_with(']') {
        before = &before[..before.rfind('[')?];
    }
    let name_len = before
        .bytes()
        .rev()
        .take_while(|b| b.is_ascii_alphabetic() || *b == b'*')
        .count();
    let name = &before[before.len() - name_len..];
    if name.is_empty() || !before[..before.len() - name_len].ends_with('\\') {
        return None;
    }
    let ctx = match name {
        "begin" => Ctx::Env { end: false },
        "end" => Ctx::Env { end: true },
        "usepackage" | "RequirePackage" => Ctx::Package,
        "input" | "include" | "subfile" => Ctx::File { images: false },
        "includegraphics" => Ctx::File { images: true },
        n if n.ends_with("ref") => Ctx::Ref,
        n if n.contains("cite") => Ctx::Cite,
        _ => return None,
    };
    // a list of keys: complete the one after the last comma
    let typed = match ctx {
        Ctx::Cite | Ctx::Package => arg.rsplit(',').next().unwrap_or("").trim_start().len(),
        _ => arg.len(),
    };
    Some((ctx, typed))
}

/// Environments opened before the caret and not closed yet, innermost first.
fn open_envs(text: &str) -> Vec<String> {
    let mut stack: Vec<String> = vec![];
    for (i, _) in text.match_indices('\\') {
        let rest = &text[i + 1..];
        let (end, rest) = if let Some(r) = rest.strip_prefix("begin{") {
            (false, r)
        } else if let Some(r) = rest.strip_prefix("end{") {
            (true, r)
        } else {
            continue;
        };
        let Some(close) = rest.find('}') else {
            continue;
        };
        let name = rest[..close].to_string();
        if end {
            if let Some(p) = stack.iter().rposition(|e| *e == name) {
                stack.truncate(p);
            }
        } else {
            stack.push(name);
        }
    }
    stack.reverse();
    stack
}

/// The arguments of `\cmd{...}` for each of `cmds` in `text`.
fn args_of<'a>(text: &'a str, cmds: &[&str]) -> Vec<&'a str> {
    let mut out = vec![];
    for cmd in cmds {
        let pat = format!("\\{cmd}{{");
        for (i, _) in text.match_indices(&pat) {
            let rest = &text[i + pat.len()..];
            if let Some(close) = rest.find('}') {
                out.push(&rest[..close]);
            }
        }
    }
    out
}

/// `@type{key,` entries of a .bib file.
fn bib_keys(bib: &str) -> Vec<String> {
    bib.split('@')
        .skip(1)
        .filter_map(|e| {
            let (kind, rest) = e.split_once('{')?;
            if kind.trim().eq_ignore_ascii_case("comment")
                || kind.trim().eq_ignore_ascii_case("string")
            {
                return None;
            }
            let key = rest.split(',').next()?.trim();
            (!key.is_empty() && !key.contains(char::is_whitespace)).then(|| key.to_string())
        })
        .collect()
}

/// Project files, relative to root, skipping hidden and build folders.
fn project_files(root: &Path) -> Vec<String> {
    let mut out = vec![];
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "node_modules" || name == "build" {
                continue;
            }
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if let Ok(rel) = p.strip_prefix(root) {
                out.push(rel.to_string_lossy().to_string());
            }
            if out.len() > 5000 {
                return out;
            }
        }
    }
    out.sort();
    out
}

fn ext(p: &str) -> String {
    Path::new(p)
        .extension()
        .map_or(String::new(), |e| e.to_string_lossy().to_lowercase())
}

/// Candidates for `ctx`: (label, text to insert, detail, kind).
fn candidates(
    ctx: &Ctx,
    root: &Path,
    source: &str,
    before: &str,
    indent: &str,
) -> Vec<(String, String, String, CompletionItemKind)> {
    let mut out = vec![];
    match ctx {
        Ctx::Command => {
            for (name, args) in COMMANDS {
                let text = match args.find("{}") {
                    Some(i) => format!("\\{name}{}{MARK}{}", &args[..i + 1], &args[i + 1..]),
                    None => format!("\\{name}"),
                };
                out.push((
                    format!("\\{name}{args}"),
                    text,
                    String::new(),
                    CompletionItemKind::FUNCTION,
                ));
            }
            // commands the file defines or uses
            let mut seen: std::collections::HashSet<&str> = COMMANDS.iter().map(|c| c.0).collect();
            for (i, _) in source.match_indices('\\') {
                let n = source[i + 1..]
                    .bytes()
                    .take_while(|b| b.is_ascii_alphabetic())
                    .count();
                let name = &source[i + 1..i + 1 + n];
                if n > 1 && seen.insert(name) {
                    out.push((
                        format!("\\{name}"),
                        format!("\\{name}"),
                        "in this file".into(),
                        CompletionItemKind::TEXT,
                    ));
                }
            }
        }
        Ctx::Env { end } => {
            let open = open_envs(before);
            let mut names: Vec<String> = open.clone();
            for e in ENVIRONMENTS
                .iter()
                .map(|e| e.to_string())
                .chain(args_of(source, &["begin"]).into_iter().map(String::from))
            {
                if !names.contains(&e) {
                    names.push(e);
                }
            }
            for e in names {
                let text = if *end {
                    format!("{e}}}")
                } else {
                    format!("{e}}}\n{indent}  {MARK}\n{indent}\\end{{{e}}}")
                };
                let detail = if open.contains(&e) {
                    "open".into()
                } else {
                    String::new()
                };
                out.push((e, text, detail, CompletionItemKind::MODULE));
            }
        }
        Ctx::Ref | Ctx::Cite => {
            let mut keys: Vec<(String, String)> = vec![];
            let mut add = |k: &str, from: &str| {
                if !k.is_empty() && !keys.iter().any(|(x, _)| x == k) {
                    keys.push((k.to_string(), from.to_string()));
                }
            };
            let cite = *ctx == Ctx::Cite;
            for f in project_files(root) {
                let e = ext(&f);
                if cite && e == "bib" {
                    let bib = std::fs::read_to_string(root.join(&f)).unwrap_or_default();
                    bib_keys(&bib).iter().for_each(|k| add(k, &f));
                }
                if e == "tex" {
                    let tex = std::fs::read_to_string(root.join(&f)).unwrap_or_default();
                    let cmds: &[&str] = if cite { &["bibitem"] } else { &["label"] };
                    args_of(&tex, cmds).iter().for_each(|k| add(k, &f));
                }
            }
            // the open file's unsaved labels
            let cmds: &[&str] = if cite { &["bibitem"] } else { &["label"] };
            args_of(source, cmds)
                .iter()
                .for_each(|k| add(k, "this file"));
            for (k, from) in keys {
                let kind = if cite {
                    CompletionItemKind::REFERENCE
                } else {
                    CompletionItemKind::CONSTANT
                };
                out.push((k.clone(), k, from, kind));
            }
        }
        Ctx::Package => {
            for p in PACKAGES {
                out.push((
                    p.to_string(),
                    p.to_string(),
                    String::new(),
                    CompletionItemKind::MODULE,
                ));
            }
        }
        Ctx::File { images } => {
            for f in project_files(root) {
                let e = ext(&f);
                if *images && IMAGE_EXTS.contains(&e.as_str()) {
                    out.push((f.clone(), f, String::new(), CompletionItemKind::FILE));
                } else if !images && e == "tex" {
                    let f = f.strip_suffix(".tex").unwrap_or(&f).to_string();
                    out.push((f.clone(), f, String::new(), CompletionItemKind::FILE));
                }
            }
        }
    }
    out
}

/// Prefix matches first, then other matches containing the query, case-insensitively.
fn rank<T>(query: &str, items: Vec<(String, T)>) -> Vec<(String, T)> {
    let q = query.to_lowercase();
    let (mut prefix, mut inner) = (vec![], vec![]);
    for (label, t) in items {
        let l = label.to_lowercase();
        if l.starts_with(&q) {
            prefix.push((label, t));
        } else if q.len() > 1 && l.contains(q.trim_start_matches('\\')) {
            inner.push((label, t));
        }
    }
    prefix.extend(inner);
    prefix.truncate(MAX_ITEMS);
    prefix
}

impl CompletionProvider for Latex {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        _: CompletionContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<Result<CompletionResponse>> {
        let empty = || Task::ready(Ok(CompletionResponse::Array(vec![])));
        let pos = text.offset_to_position(offset);
        let line_start = text.line_start_offset(pos.line as usize);
        let line = text.slice(line_start..offset).to_string();
        // not inside a comment
        if line
            .match_indices('%')
            .any(|(i, _)| i == 0 || line.as_bytes()[i - 1] != b'\\')
        {
            return empty();
        }
        let Some((ctx, typed)) = context(&line) else {
            return empty();
        };
        let query = line[line.len() - typed..].to_string();
        if ctx == Ctx::Command && query.len() < 2 {
            return empty();
        }
        let start = offset - typed;
        // swallow a closing brace the editor auto-inserted after the caret
        let mut end = offset;
        if ctx != Ctx::Command && text.chars_at(offset).next() == Some('}') {
            end += 1;
        }
        let range =
            lsp_types::Range::new(text.offset_to_position(start), text.offset_to_position(end));
        let indent: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        let before = text.slice(0..offset).to_string();
        let source = self
            .doc
            .upgrade()
            .map(|d| d.read(cx).source.clone())
            .unwrap_or_else(|| text.to_string());
        let root = self.root.clone();
        cx.background_spawn(async move {
            let mut items = candidates(&ctx, &root, &source, &before, &indent);
            // environment items close their own brace; put back the one swallowed for the rest
            let close = end > offset && !matches!(ctx, Ctx::Env { .. });
            let ranked = rank(
                &query,
                items.drain(..).map(|(l, t, d, k)| (l, (t, d, k))).collect(),
            );
            let items = ranked
                .into_iter()
                .map(|(label, (mut new_text, detail, kind))| {
                    if close {
                        new_text.push('}');
                    }
                    CompletionItem {
                        label,
                        kind: Some(kind),
                        detail: (!detail.is_empty()).then_some(detail),
                        text_edit: Some(CompletionTextEdit::Edit(TextEdit { range, new_text })),
                        ..Default::default()
                    }
                })
                .collect();
            Ok(CompletionResponse::Array(items))
        })
    }

    fn is_completion_trigger(&self, _: usize, new_text: &str, cx: &mut App) -> bool {
        // every typed character, so the menu follows (and closes on) each keystroke
        AppSettings::get(cx).autocomplete && new_text.chars().count() == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_context() {
        assert_eq!(context("text \\fra"), Some((Ctx::Command, 4)));
        assert_eq!(context("\\\\"), None);
        assert_eq!(context("\\begin{ali"), Some((Ctx::Env { end: false }, 3)));
        assert_eq!(context("  \\end{"), Some((Ctx::Env { end: true }, 0)));
        assert_eq!(context("see \\eqref{eq:"), Some((Ctx::Ref, 3)));
        assert_eq!(context("\\cite{knuth, lam"), Some((Ctx::Cite, 3)));
        assert_eq!(context("\\citep[p.~3]{kn"), Some((Ctx::Cite, 2)));
        assert_eq!(
            context("\\includegraphics[width=3cm]{fig"),
            Some((Ctx::File { images: true }, 3))
        );
        assert_eq!(context("\\textbf{bo"), None);
        assert_eq!(context("\\ref{a} and"), None);
    }

    #[test]
    fn tracks_open_environments() {
        let t = "\\begin{document}\\begin{align}x\\end{align}\\begin{itemize}";
        assert_eq!(open_envs(t), vec!["itemize", "document"]);
    }

    #[test]
    fn reads_bib_keys() {
        let bib = "@comment{x}\n@article{knuth84,\n title={TeX}}\n@Book{ lamport94 , }";
        assert_eq!(bib_keys(bib), vec!["knuth84", "lamport94"]);
    }

    #[test]
    fn environment_items_wrap_the_caret() {
        let items = candidates(
            &Ctx::Env { end: false },
            Path::new("/nonexistent"),
            "",
            "",
            "",
        );
        let (_, text, ..) = items.iter().find(|i| i.0 == "itemize").unwrap();
        assert_eq!(text, &format!("itemize}}\n  {MARK}\n\\end{{itemize}}"));
    }
}
