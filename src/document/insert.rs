// What the Insert tab puts in: a figure for a picture, a table, a cross-reference, a
// citation, a link; and what it offers to point at — the labels in the document and the
// entries of its .bib files.
use super::caret::{MARK, group_close};
use std::path::Path;

/// TikZ pictures to start from, for the Insert tab: a name and the TeX, its caret at MARK. Plain
/// TikZ, no libraries, so only `\usepackage{tikz}` is needed.
pub const TIKZ: &[(&str, &str)] = &[
    (
        "Shapes and arrows",
        "\\begin{center}\n\\begin{tikzpicture}\n  \\draw[thick] (0,0) rectangle (2,1);\n  \\draw[->, thick] (2,0.5) -- (3.5,0.5);\n  \\draw[thick] (4,0.5) circle (0.5);\u{E001}\n\\end{tikzpicture}\n\\end{center}",
    ),
    (
        "Axes and curve",
        "\\begin{center}\n\\begin{tikzpicture}\n  \\draw[->] (-0.2,0) -- (4,0) node[right] {$x$};\n  \\draw[->] (0,-0.2) -- (0,3) node[above] {$y$};\n  \\draw[thick, blue, domain=0:3.4, samples=60] plot (\\x, {0.25*\\x*\\x});\u{E001}\n\\end{tikzpicture}\n\\end{center}",
    ),
    (
        "Flowchart",
        "\\begin{center}\n\\begin{tikzpicture}\n  \\node[draw, rounded corners] (a) at (0,0) {Start};\n  \\node[draw] (b) at (3,0) {Step};\n  \\node[draw, rounded corners] (c) at (6,0) {End};\u{E001}\n  \\draw[->] (a) -- (b);\n  \\draw[->] (b) -- (c);\n\\end{tikzpicture}\n\\end{center}",
    ),
    (
        "Graph",
        "\\begin{center}\n\\begin{tikzpicture}\n  \\node[draw, circle] (1) at (0,0) {1};\n  \\node[draw, circle] (2) at (2,1) {2};\n  \\node[draw, circle] (3) at (2,-1) {3};\u{E001}\n  \\draw (1) -- (2) -- (3) -- (1);\n\\end{tikzpicture}\n\\end{center}",
    ),
    (
        "Grid",
        "\\begin{center}\n\\begin{tikzpicture}\n  \\draw[help lines] (0,0) grid (4,3);\n  \\draw[->] (0,0) -- (4,0);\n  \\draw[->] (0,0) -- (0,3);\u{E001}\n\\end{tikzpicture}\n\\end{center}",
    ),
];

/// A figure showing `file` (relative to the document), its caption the caret's place.
pub fn figure(file: &str, label: &str, caption: &str) -> String {
    format!(
        "\\begin{{figure}}[htbp]\n  \\centering\n  \\includegraphics[width=0.8\\linewidth]{{{file}}}\n  \\caption{{{caption}{MARK}}}\n  \\label{{{label}}}\n\\end{{figure}}"
    )
}

/// A `rows` × `cols` table with lines, its header row named, the caret in its first cell.
pub fn table(rows: usize, cols: usize, label: &str) -> String {
    let mut t = format!(
        "\\begin{{table}}[htbp]\n  \\centering\n  \\begin{{tabular}}{{|{}}}\n    \\hline\n",
        "l|".repeat(cols)
    );
    for r in 0..rows {
        let cells: Vec<String> = (0..cols)
            .map(|c| match (r, c) {
                (0, 0) => format!("Column 1{MARK}"),
                (0, c) => format!("Column {}", c + 1),
                _ => String::new(),
            })
            .collect();
        t.push_str(&format!("    {} \\\\\n    \\hline\n", cells.join(" & ")));
    }
    t.push_str(&format!(
        "  \\end{{tabular}}\n  \\caption{{Table}}\n  \\label{{{label}}}\n\\end{{table}}"
    ));
    t
}

/// A label `prefix:name` not yet in `src` (name from `stem`, kept to letters, digits, `-`).
pub fn new_label(src: &str, prefix: &str, stem: &str) -> String {
    let name: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let name = name.trim_matches('-');
    let name = if name.is_empty() { "1" } else { name };
    let taken = |l: &str| src.contains(&format!("\\label{{{l}}}"));
    let mut label = format!("{prefix}:{name}");
    let mut n = 2;
    while taken(&label) {
        label = format!("{prefix}:{name}-{n}");
        n += 1;
    }
    label
}

/// `\href` to `url` showing `text` (TeX already), or `\url` when there is no text.
pub fn link(url: &str, text: &str) -> String {
    // an address typed without its scheme (`example.com/a`) is a web one
    let url = url.trim();
    let url = if !url.contains(':') && !url.starts_with('#') && url.contains('.') {
        format!("https://{url}")
    } else {
        url.to_string()
    };
    // spaces as %20; a brace the URL cannot balance would end the argument
    let url: String = url
        .trim()
        .chars()
        .map(|c| match c {
            ' ' => "%20".to_string(),
            '{' => "%7B".into(),
            '}' => "%7D".into(),
            '\\' => "%5C".into(),
            c => c.to_string(),
        })
        .collect();
    let url = url.replace('%', "\\%").replace('#', "\\#");
    if text.trim().is_empty() {
        format!("\\url{{{url}}}")
    } else {
        format!("\\href{{{url}}}{{{text}}}")
    }
}

/// A label in the document, with what it names.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub key: String,
    /// Figure, Table, Equation, Section, Item or Label.
    pub kind: &'static str,
    /// Its caption or heading, for the picker.
    pub title: String,
}

impl Label {
    /// The cross-reference to it, as Word writes one: "Figure 2", "(3)".
    pub fn reference(&self) -> String {
        match self.kind {
            "Equation" => format!("(\\ref{{{}}})", self.key),
            "Item" | "Label" => format!("\\ref{{{}}}", self.key),
            k => format!("{k}~\\ref{{{}}}", self.key),
        }
    }
}

/// The labels in `src`, in order.
pub fn labels(src: &str) -> Vec<Label> {
    let mut out = vec![];
    let mut from = 0;
    while let Some(i) = src[from..].find("\\label{").map(|i| i + from) {
        from = i + 7;
        let Some(close) = group_close(src, i + 6) else {
            continue;
        };
        let key = src[i + 7..close].trim().to_string();
        if key.is_empty() || in_comment(src, i) {
            continue;
        }
        let (kind, title) = what_is_labelled(src, i);
        out.push(Label { key, kind, title });
    }
    out
}

/// Whether `at` is on a line after a `%`.
fn in_comment(src: &str, at: usize) -> bool {
    let line = &src[src[..at].rfind('\n').map_or(0, |i| i + 1)..at];
    line.match_indices('%')
        .any(|(j, _)| !line[..j].ends_with('\\'))
}

/// The kind of thing the `\label` at `at` names, and its title.
fn what_is_labelled(src: &str, at: usize) -> (&'static str, String) {
    // the innermost environment open at `at`
    let before = &src[..at];
    let mut depth: Vec<&str> = vec![];
    let mut pos = 0;
    while pos < before.len() {
        let b = before[pos..].find("\\begin{").map(|j| j + pos);
        let e = before[pos..].find("\\end{").map(|j| j + pos);
        let (k, open) = match (b, e) {
            (Some(b), Some(e)) if b < e => (b, true),
            (Some(b), None) => (b, true),
            (_, Some(e)) => (e, false),
            (None, None) => break,
        };
        let name_at = k + if open { 7 } else { 5 };
        if in_comment(src, k) {
            pos = name_at;
            continue;
        }
        let name = before[name_at..]
            .split('}')
            .next()
            .unwrap_or("")
            .trim_end_matches('*');
        if open {
            depth.push(name);
        } else if let Some(p) = depth.iter().rposition(|n| *n == name) {
            depth.truncate(p);
        }
        pos = name_at;
    }
    let env = depth.iter().rev().find(|n| {
        matches!(
            **n,
            "figure"
                | "table"
                | "equation"
                | "align"
                | "gather"
                | "multline"
                | "eqnarray"
                | "enumerate"
        )
    });
    let caption = || {
        // a \caption in the same environment, before or after the label
        let start = depth
            .last()
            .and_then(|n| before.rfind(&format!("\\begin{{{n}")))
            .unwrap_or(at);
        let end = src[at..].find("\\end{").map_or(src.len(), |j| at + j);
        // the caption just before the label (a subfigure's come earlier), else after it
        let found = src[start..at]
            .rfind("\\caption{")
            .map(|c| start + c)
            .or_else(|| src[at..end].find("\\caption{").map(|c| at + c));
        found
            .and_then(|c| group_close(src, c + 8).map(|e| plain(&src[c + 9..e])))
            .unwrap_or_default()
    };
    match env.copied() {
        Some("figure") => ("Figure", caption()),
        Some("table") => ("Table", caption()),
        Some("enumerate") => ("Item", String::new()),
        Some(_) => ("Equation", String::new()),
        None => {
            // a heading just before it
            let line_start = before.trim_end().rfind('\n').map_or(0, |i| i + 1);
            let prev = before[line_start..].trim_start();
            match prev.strip_prefix('\\').and_then(|p| {
                let name: String = p.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
                matches!(
                    name.as_str(),
                    "part" | "chapter" | "section" | "subsection" | "subsubsection" | "paragraph"
                )
                .then_some(name)
            }) {
                Some(name) => {
                    let open = before.len() - prev.len();
                    let brace = src[open..].find('{').map(|j| open + j);
                    let title = brace
                        .and_then(|b| group_close(src, b).map(|e| plain(&src[b + 1..e])))
                        .unwrap_or_default();
                    (
                        if name == "chapter" {
                            "Chapter"
                        } else {
                            "Section"
                        },
                        title,
                    )
                }
                None => ("Label", String::new()),
            }
        }
    }
}

/// TeX as readable text, roughly: commands and braces dropped.
fn plain(tex: &str) -> String {
    let mut out = String::new();
    let mut chars = tex.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if chars.peek().is_some_and(|c| !c.is_ascii_alphabetic()) {
                    out.extend(chars.next());
                } else {
                    let mut name = String::new();
                    while let Some(c) = chars.next_if(|c| c.is_ascii_alphabetic()) {
                        name.push(c);
                    }
                    // logos print their name
                    if ["TeX", "LaTeX", "LaTeXe", "BibTeX"].contains(&name.as_str()) {
                        out.push_str(&name);
                    }
                }
            }
            '{' | '}' | '$' => {}
            '~' => out.push(' '),
            c => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The .bib files `src` (in `dir`) cites from: those `\bibliography` and `\addbibresource`
/// name, else every .bib in `dir`.
pub fn bib_files(src: &str, dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    for cmd in ["\\bibliography", "\\addbibresource"] {
        let mut from = 0;
        while let Some(i) = src[from..].find(cmd).map(|i| i + from) {
            from = i + cmd.len();
            // \addbibresource[options]{file}
            let mut open = from;
            if src[open..].starts_with('[') {
                match src[open..].find(']') {
                    Some(e) => open += e + 1,
                    None => continue,
                }
            }
            if !src[open..].starts_with('{') || in_comment(src, i) {
                continue;
            }
            let Some(close) = group_close(src, open) else {
                continue;
            };
            for name in src[open + 1..close].split(',') {
                let name = name.trim();
                let name = if name.ends_with(".bib") {
                    name.to_string()
                } else {
                    format!("{name}.bib")
                };
                out.push(dir.join(name));
            }
        }
    }
    if out.is_empty()
        && let Ok(rd) = std::fs::read_dir(dir)
    {
        out = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "bib"))
            .collect();
        out.sort();
    }
    out
}

/// An entry of a .bib file.
#[derive(Debug, Clone, PartialEq)]
pub struct BibEntry {
    pub key: String,
    pub kind: String,
    pub title: String,
    pub author: String,
    pub year: String,
}

/// The entries of a .bib file's text.
pub fn bib_entries(bib: &str) -> Vec<BibEntry> {
    let mut out = vec![];
    let mut from = 0;
    while let Some(at) = bib[from..].find('@').map(|i| i + from) {
        from = at + 1;
        let Some(open) = bib[at..].find(['{', '(']).map(|i| at + i) else {
            break;
        };
        let kind = bib[at + 1..open].trim().to_ascii_lowercase();
        if kind.is_empty()
            || !kind.chars().all(|c| c.is_ascii_alphabetic())
            || ["comment", "string", "preamble"].contains(&kind.as_str())
        {
            continue;
        }
        // @article{…} or @article(…)
        let body_end = if bib[open..].starts_with('(') {
            bib[open..].find(")\n").map_or(bib.len(), |i| open + i)
        } else {
            group_close(bib, open).unwrap_or(bib.len())
        };
        let body = &bib[open + 1..body_end];
        let Some((key, fields)) = body.split_once(',') else {
            continue;
        };
        let field = |name: &str| {
            field_of(fields, name)
                .map(|v| plain(&v))
                .unwrap_or_default()
        };
        out.push(BibEntry {
            key: key.trim().to_string(),
            kind,
            title: field("title"),
            author: field("author"),
            year: field("year"),
        });
        from = body_end;
    }
    out
}

/// The value of field `name` in a .bib entry's fields: `{…}`, `"…"` or a bare word.
fn field_of(fields: &str, name: &str) -> Option<String> {
    let lower = fields.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name).map(|i| i + from) {
        from = i + name.len();
        let ok_before = i == 0 || !lower.as_bytes()[i - 1].is_ascii_alphanumeric();
        let rest = fields[from..].trim_start();
        if !ok_before || !rest.starts_with('=') {
            continue;
        }
        let v = rest[1..].trim_start();
        let start = fields.len() - v.len();
        return Some(match v.chars().next()? {
            '{' => fields[start + 1..group_close(fields, start)?].to_string(),
            '"' => v[1..].split('"').next()?.to_string(),
            _ => v.split([',', '\n']).next()?.trim().to_string(),
        });
    }
    None
}

/// What a picker row shows for an entry: author, year and title.
pub fn bib_line(e: &BibEntry) -> String {
    let who = e
        .author
        .split(" and ")
        .next()
        .map(|a| a.split(',').next().unwrap_or(a).trim())
        .unwrap_or("");
    let etal = if e.author.contains(" and ") {
        " et al."
    } else {
        ""
    };
    let mut s = String::new();
    if !who.is_empty() {
        s.push_str(&format!("{who}{etal}"));
    }
    if !e.year.is_empty() {
        s.push_str(&format!(" ({})", e.year));
    }
    if !e.title.is_empty() {
        if !s.is_empty() {
            s.push_str(" — ");
        }
        s.push_str(&e.title);
    }
    s
}

/// The image formats LuaLaTeX's graphicx reads.
pub const IMAGE_TYPES: &[&str] = &["png", "jpg", "jpeg", "pdf"];

/// `bytes` saved as `figures/<name>` beside the document in `dir` (a name taken by another
/// file gets a number; the same picture again is the file already there), as the path the
/// document names it by.
pub fn save_into_figures(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<String> {
    let figures = dir.join("figures");
    std::fs::create_dir_all(&figures)?;
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_whitespace() || "%#{}\\~$&^".contains(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    let (stem, ext) = name.rsplit_once('.').unwrap_or((&name, "png"));
    // one dot only: graphicx takes the first one for the extension's
    let stem = stem.replace('.', "-");
    let stem = if stem.is_empty() {
        "image".into()
    } else {
        stem
    };
    let mut file = format!("{stem}.{ext}");
    let mut n = 2;
    loop {
        let p = figures.join(&file);
        match std::fs::read(&p) {
            Ok(old) if old == bytes => break,
            Ok(_) => {
                file = format!("{stem}-{n}.{ext}");
                n += 1;
            }
            Err(_) => {
                std::fs::write(&p, bytes)?;
                break;
            }
        }
    }
    Ok(format!("figures/{file}"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_tikz_templates_are_whole_pictures_with_a_caret() {
        for (name, tex) in super::TIKZ {
            assert_eq!(tex.matches(super::MARK).count(), 1, "{name}");
            assert!(crate::document::caret::balanced(tex), "{name}");
            assert!(tex.contains("\\begin{tikzpicture}") && tex.contains("\\end{tikzpicture}"));
        }
    }

    use super::*;

    #[test]
    fn labels_know_what_they_name() {
        let src = "\\section{Intro}\\label{sec:intro}\n\nText.\n\n\\begin{figure}\n  \\includegraphics{a}\n  \\caption{A \\emph{nice} plot}\n  \\label{fig:a}\n\\end{figure}\n\n\\begin{equation}\n  x\\label{eq:x}\n\\end{equation}\n% \\label{gone}\n";
        let l = labels(src);
        let got: Vec<_> = l
            .iter()
            .map(|l| (l.key.as_str(), l.kind, l.title.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("sec:intro", "Section", "Intro"),
                ("fig:a", "Figure", "A nice plot"),
                ("eq:x", "Equation", ""),
            ]
        );
        assert_eq!(l[1].reference(), "Figure~\\ref{fig:a}");
        assert_eq!(l[2].reference(), "(\\ref{eq:x})");
        // a commented-out figure is not one; a figure's own caption, not its subfigure's
        let s2 = "% \\begin{figure}\n\\section{A}\\label{a}\n\\begin{figure}\n\\begin{subfigure}{1in}\\caption{Sub}\\label{s}\\end{subfigure}\n\\caption{Whole}\\label{w}\n\\end{figure}";
        let l2 = labels(s2);
        let got: Vec<_> = l2
            .iter()
            .map(|l| (l.key.as_str(), l.kind, l.title.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("a", "Section", "A"),
                ("s", "Figure", "Sub"),
                ("w", "Figure", "Whole")
            ]
        );
        assert_eq!(new_label(src, "fig", "a"), "fig:a-2");
        assert_eq!(new_label(src, "fig", "My Plot.png"), "fig:my-plot-png");
    }

    #[test]
    fn bib_entries_are_read() {
        let bib = "@comment{x}\n@Article{knuth84,\n  author = {Knuth, Donald E. and Other, A.},\n  title = \"Literate {P}rogramming\",\n  year = 1984,\n}\n@book{lamport, title={\\LaTeX: A Document Preparation System}, author={Leslie Lamport}, year={1994}}";
        let e = bib_entries(bib);
        assert_eq!(e.len(), 2);
        let p = bib_entries("@book(lamport,\n  title = {A Book},\n  year = 1994\n)\n");
        assert_eq!(
            (p[0].key.as_str(), p[0].title.as_str()),
            ("lamport", "A Book")
        );
        assert_eq!(e[0].key, "knuth84");
        assert_eq!(e[0].title, "Literate Programming");
        assert_eq!(e[0].year, "1984");
        assert_eq!(
            bib_line(&e[0]),
            "Knuth et al. (1984) — Literate Programming"
        );
        assert_eq!(e[1].key, "lamport");
        assert_eq!(e[1].title, "LaTeX: A Document Preparation System");
    }

    #[test]
    fn pictures_go_into_figures() {
        let dir = std::env::temp_dir().join(format!("rtd-fig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            save_into_figures(&dir, "my plot.png", b"a").unwrap(),
            "figures/my-plot.png"
        );
        assert_eq!(
            save_into_figures(&dir, "my plot.png", b"a").unwrap(),
            "figures/my-plot.png"
        );
        assert_eq!(
            save_into_figures(&dir, "my plot.png", b"b").unwrap(),
            "figures/my-plot-2.png"
        );
        assert_eq!(
            save_into_figures(&dir, "a.b.c.jpg", b"c").unwrap(),
            "figures/a-b-c.jpg"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snippets() {
        assert_eq!(
            link("https://x.org/#a", "site"),
            "\\href{https://x.org/\\#a}{site}"
        );
        assert_eq!(link("https://x.org", ""), "\\url{https://x.org}");
        assert_eq!(
            link("example.com/a", "t"),
            "\\href{https://example.com/a}{t}"
        );
        assert_eq!(link("mailto:a@b.org", "t"), "\\href{mailto:a@b.org}{t}");
        assert_eq!(
            link(" https://x.org/a b{c} ", "t"),
            "\\href{https://x.org/a\\%20b\\%7Bc\\%7D}{t}"
        );
        let t = table(2, 2, "tab:1");
        assert!(t.contains("{|l|l|}"));
        assert!(t.contains(&format!("Column 1{MARK} & Column 2 \\\\")));
        assert!(t.contains("     &  \\\\"));
    }
}
