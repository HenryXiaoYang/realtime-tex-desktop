// The element the sidebar edits, apart from any UI: what is at the caret or selected (a figure,
// an image, a table, a heading, an equation, a list, else the document), its fields as the
// sidebar shows them (common first, then Advanced), their values read from the source, and the
// edit that sets one. Edits touch only the part of the source a field lives in (an
// \includegraphics option, the \caption, a [placement]), never the rest of what the user wrote.
use super::caret::{group_close, item_at};
use super::format::{add_package, heading_levels};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Figure,
    Image,
    Table,
    Heading,
    Equation,
    List,
    Document,
}

impl Kind {
    pub fn title(self) -> &'static str {
        match self {
            Kind::Figure => "Figure",
            Kind::Image => "Image",
            Kind::Table => "Table",
            Kind::Heading => "Heading",
            Kind::Equation => "Equation",
            Kind::List => "List",
            Kind::Document => "Document",
        }
    }
}

/// How a field is edited.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Text,
    Number,
    Toggle,
    /// (value, label)
    Choice(Vec<(&'static str, &'static str)>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub input: Input,
    pub advanced: bool,
    pub value: String,
}

/// The element at hand: its kind and its source.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub kind: Kind,
    pub range: Range<usize>,
}

/// A change to the source, with the packages it needs.
#[derive(Debug, PartialEq)]
pub struct Change {
    pub range: Range<usize>,
    pub text: String,
    pub packages: Vec<&'static str>,
}

fn change(range: Range<usize>, text: impl Into<String>) -> Option<Change> {
    Some(Change {
        range,
        text: text.into(),
        packages: vec![],
    })
}

const HEADINGS: &[(&str, &str)] = &[
    ("part", "Part"),
    ("chapter", "Chapter"),
    ("section", "Section"),
    ("subsection", "Subsection"),
    ("subsubsection", "Subsubsection"),
    ("paragraph", "Paragraph"),
];

const EQUATIONS: &[(&str, &str)] = &[
    ("equation", "Equation"),
    ("align", "Aligned (align)"),
    ("gather", "Centred lines (gather)"),
    ("multline", "Long equation (multline)"),
];

/// The element for `block` (a block's source range), with the caret at `at`.
pub fn element_at(src: &str, block: Option<Range<usize>>, at: usize) -> Element {
    let doc = Element {
        kind: Kind::Document,
        range: 0..src.len(),
    };
    // in a list item: the list (the innermost one)
    if let Some(it) = item_at(src, at)
        && block
            .as_ref()
            .is_none_or(|b| b.start <= it.list.start || it.list.contains(&b.start))
    {
        return Element {
            kind: Kind::List,
            range: it.list,
        };
    }
    let Some(b) = block else { return doc };
    let t = &src[b.clone()];
    let begins = |env: &str| t.starts_with(&format!("\\begin{{{env}"));
    let kind = if begins("figure") || begins("wrapfigure") {
        Kind::Figure
    } else if begins("table") {
        Kind::Table
    } else if t.starts_with("\\[")
        || t.starts_with("$$")
        || [
            "equation",
            "align",
            "gather",
            "multline",
            "eqnarray",
            "displaymath",
        ]
        .iter()
        .any(|e| begins(e))
    {
        Kind::Equation
    } else if HEADINGS.iter().any(|(h, _)| {
        t.strip_prefix('\\')
            .and_then(|r| r.strip_prefix(h))
            .is_some_and(|r| r.starts_with(['{', '*', '[']))
    }) {
        Kind::Heading
    } else if begins("itemize") || begins("enumerate") || begins("description") {
        Kind::List
    } else if t.contains("\\includegraphics") && !t.contains("\\begin{tabular") {
        Kind::Image
    } else {
        return doc;
    };
    Element { kind, range: b }
}

// --- reading the source ---

/// `key=value` options, split at top-level commas.
pub fn parse_opts(s: &str) -> Vec<(String, Option<String>)> {
    let mut out = vec![];
    let mut depth = 0;
    let mut cur = String::new();
    let mut push = |cur: &mut String| {
        let t = cur.trim();
        if !t.is_empty() {
            match t.split_once('=') {
                Some((k, v)) => out.push((k.trim().to_string(), Some(v.trim().to_string()))),
                None => out.push((t.to_string(), None)),
            }
        }
        cur.clear();
    };
    for c in s.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                push(&mut cur);
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    push(&mut cur);
    out
}

fn render_opts(opts: &[(String, Option<String>)]) -> String {
    opts.iter()
        .map(|(k, v)| match v {
            Some(v) => format!("{k}={v}"),
            None => k.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A command `\name` in `r` of `src` (at depth 0 of its own text): where it starts, its
/// `[options]` (inside the brackets, if any) and its first `{argument}` (inside the braces).
struct Cmd {
    start: usize,
    opts: Option<Range<usize>>,
    /// Where options would go: just after the name.
    after_name: usize,
    arg: Option<Range<usize>>,
    end: usize,
}

fn find_cmd(src: &str, r: Range<usize>, name: &str) -> Option<Cmd> {
    let pat = format!("\\{name}");
    let mut from = r.start;
    loop {
        let i = src[from..r.end].find(&pat)? + from;
        let after = i + pat.len();
        if src[after..].starts_with(|c: char| c.is_ascii_alphabetic()) {
            from = after;
            continue;
        }
        let mut j = after;
        if src[j..].starts_with('*') {
            j += 1;
        }
        let after_name = j;
        let mut opts = None;
        let ws = |k: usize| k + src[k..].len() - src[k..].trim_start_matches([' ', '\t']).len();
        let k = ws(j);
        if src[k..].starts_with('[') {
            let close = src[k..].find(']').map(|e| k + e)?;
            opts = Some(k + 1..close);
            j = close + 1;
        }
        let k = ws(j);
        let mut arg = None;
        let mut end = j;
        if src[k..].starts_with('{') {
            let close = group_close(src, k)?;
            arg = Some(k + 1..close);
            end = close + 1;
        }
        return Some(Cmd {
            start: i,
            opts,
            after_name,
            arg,
            end,
        });
    }
}

/// A figure's or table's `\begin{env}` line: its end, and its [placement].
fn env_head(src: &str, el: &Element) -> Option<(String, Option<Range<usize>>, usize)> {
    let t = &src[el.range.clone()];
    let rest = t.strip_prefix("\\begin{")?;
    let name = rest.split('}').next()?.to_string();
    let after = el.range.start + "\\begin{".len() + name.len() + 1;
    let (opts, end) = if src[after..].starts_with('[') {
        let close = src[after..].find(']').map(|e| after + e)?;
        (Some(after + 1..close), close + 1)
    } else {
        (None, after)
    };
    Some((name, opts, end))
}

fn graphics(src: &str, el: &Element) -> Option<Cmd> {
    find_cmd(src, el.range.clone(), "includegraphics")
}

fn opt_of(src: &str, cmd: &Cmd, key: &str) -> Option<Option<String>> {
    let opts = parse_opts(&src[cmd.opts.clone()?]);
    opts.into_iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// A width option as a percentage of the line: `0.8\linewidth` is 80.
fn width_percent(v: &str) -> Option<String> {
    for unit in ["\\linewidth", "\\textwidth", "\\columnwidth"] {
        if let Some(f) = v.strip_suffix(unit) {
            let f: f64 = if f.trim().is_empty() {
                1.0
            } else {
                f.trim().parse().ok()?
            };
            return Some(format!("{}", (f * 100.0).round()));
        }
    }
    None
}

/// The fields of `el`, with their values now.
pub fn fields(src: &str, el: &Element) -> Vec<Field> {
    let f = |key, label, input, advanced, value: String| Field {
        key,
        label,
        input,
        advanced,
        value,
    };
    let text_of = |name: &str| {
        find_cmd(src, el.range.clone(), name)
            .and_then(|c| c.arg)
            .map(|a| src[a].to_string())
            .unwrap_or_default()
    };
    match el.kind {
        Kind::Figure | Kind::Image => {
            let g = graphics(src, el);
            let opt = |k: &str| {
                g.as_ref()
                    .and_then(|g| opt_of(src, g, k))
                    .map(|v| v.unwrap_or_else(|| "true".into()))
                    .unwrap_or_default()
            };
            let width = opt("width");
            let width = width_percent(&width).unwrap_or(width);
            let file = g
                .as_ref()
                .and_then(|g| g.arg.clone())
                .map(|a| src[a].to_string())
                .unwrap_or_default();
            let align = if src[el.range.clone()].contains("\\raggedleft") {
                "right"
            } else if src[el.range.clone()].contains("\\raggedright") {
                "left"
            } else if src[el.range.clone()].contains("\\centering") {
                "center"
            } else {
                "left"
            };
            let mut out = vec![f("width", "Width (% of line)", Input::Number, false, width)];
            if el.kind == Kind::Figure {
                out.push(f(
                    "align",
                    "Alignment",
                    Input::Choice(vec![
                        ("left", "Left"),
                        ("center", "Center"),
                        ("right", "Right"),
                    ]),
                    false,
                    align.into(),
                ));
                out.push(f(
                    "caption",
                    "Caption",
                    Input::Text,
                    false,
                    text_of("caption"),
                ));
                out.push(f("label", "Label", Input::Text, false, text_of("label")));
            }
            out.extend([
                f("file", "File", Input::Text, true, file),
                f("height", "Height", Input::Text, true, opt("height")),
                f("scale", "Scale", Input::Number, true, opt("scale")),
                f(
                    "angle",
                    "Rotation (degrees)",
                    Input::Number,
                    true,
                    opt("angle"),
                ),
                f(
                    "keepaspectratio",
                    "Keep aspect ratio",
                    Input::Toggle,
                    true,
                    opt("keepaspectratio"),
                ),
                f(
                    "trim",
                    "Trim (left bottom right top)",
                    Input::Text,
                    true,
                    opt("trim"),
                ),
                f("clip", "Clip to the trim", Input::Toggle, true, opt("clip")),
                f("page", "PDF page", Input::Number, true, opt("page")),
            ]);
            if el.kind == Kind::Figure {
                let placement = env_head(src, el)
                    .and_then(|(_, o, _)| o)
                    .map(|o| src[o].to_string())
                    .unwrap_or_default();
                out.push(f(
                    "placement",
                    "Placement (h t b p !)",
                    Input::Text,
                    true,
                    placement,
                ));
            }
            out
        }
        Kind::Table => {
            let placement = env_head(src, el)
                .and_then(|(_, o, _)| o)
                .map(|o| src[o].to_string())
                .unwrap_or_default();
            vec![
                f("caption", "Caption", Input::Text, false, text_of("caption")),
                f("label", "Label", Input::Text, false, text_of("label")),
                f(
                    "placement",
                    "Placement (h t b p !)",
                    Input::Text,
                    true,
                    placement,
                ),
            ]
        }
        Kind::Heading => {
            let t = &src[el.range.clone()];
            let level = HEADINGS
                .iter()
                .find(|(h, _)| {
                    t[1..].starts_with(h)
                        && !t[1 + h.len()..].starts_with(|c: char| c.is_ascii_alphabetic())
                })
                .map_or("section", |(h, _)| *h);
            let cmd = find_cmd(src, el.range.clone(), level);
            let starred = t[1 + level.len()..].starts_with('*');
            let short = cmd
                .as_ref()
                .and_then(|c| c.opts.clone())
                .map(|o| src[o].to_string())
                .unwrap_or_default();
            vec![
                f(
                    "level",
                    "Level",
                    Input::Choice({
                        let levels = heading_levels(src);
                        HEADINGS
                            .iter()
                            .filter(|(h, _)| *h == level || levels.contains(h))
                            .copied()
                            .collect()
                    }),
                    false,
                    level.into(),
                ),
                f(
                    "numbered",
                    "Numbered",
                    Input::Toggle,
                    false,
                    (!starred).to_string(),
                ),
                f(
                    "short",
                    "Short title (for the contents)",
                    Input::Text,
                    true,
                    short,
                ),
                f(
                    "label",
                    "Label",
                    Input::Text,
                    true,
                    label_after(src, el).map(|(_, l)| l).unwrap_or_default(),
                ),
            ]
        }
        Kind::Equation => {
            let (env, starred) = equation_env(src, el);
            vec![
                f(
                    "numbered",
                    "Numbered",
                    Input::Toggle,
                    false,
                    (!starred).to_string(),
                ),
                f("label", "Label", Input::Text, false, text_of("label")),
                f(
                    "env",
                    "Environment",
                    Input::Choice(EQUATIONS.to_vec()),
                    true,
                    env,
                ),
            ]
        }
        Kind::List => {
            let (env, opts, _) = env_head(src, el).unwrap_or(("itemize".into(), None, 0));
            let opts = opts.map(|o| parse_opts(&src[o])).unwrap_or_default();
            let get = |k: &str| {
                opts.iter()
                    .find(|(o, _)| o == k)
                    .map(|(_, v)| v.clone().unwrap_or_else(|| "true".into()))
                    .unwrap_or_default()
            };
            vec![
                f(
                    "env",
                    "Kind",
                    Input::Choice(vec![
                        ("itemize", "Bullets"),
                        ("enumerate", "Numbers"),
                        ("description", "Descriptions"),
                    ]),
                    false,
                    env,
                ),
                f(
                    "label",
                    "Item label (enumitem)",
                    Input::Text,
                    true,
                    get("label"),
                ),
                f("start", "Start at", Input::Number, true, get("start")),
                f("nosep", "Compact", Input::Toggle, true, get("nosep")),
            ]
        }
        Kind::Document => {
            let class = find_cmd(src, 0..src.len(), "documentclass");
            let opts = class
                .as_ref()
                .and_then(|c| c.opts.clone())
                .map(|o| parse_opts(&src[o]))
                .unwrap_or_default();
            let has = |k: &str| opts.iter().any(|(o, _)| o == k);
            let size = ["10pt", "11pt", "12pt"]
                .into_iter()
                .find(|s| has(s))
                .unwrap_or("10pt");
            let paper = ["a4paper", "letterpaper", "a5paper", "legalpaper"]
                .into_iter()
                .find(|s| has(s))
                .unwrap_or("letterpaper");
            let name = class
                .and_then(|c| c.arg)
                .map(|a| src[a].to_string())
                .unwrap_or_default();
            vec![
                f(
                    "class",
                    "Class",
                    Input::Choice(vec![
                        ("article", "Article"),
                        ("report", "Report"),
                        ("book", "Book"),
                        ("letter", "Letter"),
                        ("beamer", "Slides (beamer)"),
                    ]),
                    false,
                    name,
                ),
                f(
                    "fontsize",
                    "Font size",
                    Input::Choice(vec![
                        ("10pt", "10 pt"),
                        ("11pt", "11 pt"),
                        ("12pt", "12 pt"),
                    ]),
                    false,
                    size.into(),
                ),
                f(
                    "paper",
                    "Paper",
                    Input::Choice(vec![
                        ("a4paper", "A4"),
                        ("letterpaper", "Letter"),
                        ("a5paper", "A5"),
                        ("legalpaper", "Legal"),
                    ]),
                    false,
                    paper.into(),
                ),
                f(
                    "margin",
                    "Margins (geometry)",
                    Input::Text,
                    false,
                    geometry_margin(src).unwrap_or_default(),
                ),
                f(
                    "twoside",
                    "Two-sided",
                    Input::Toggle,
                    true,
                    has("twoside").to_string(),
                ),
                f(
                    "twocolumn",
                    "Two columns",
                    Input::Toggle,
                    true,
                    has("twocolumn").to_string(),
                ),
            ]
        }
    }
}

fn equation_env(src: &str, el: &Element) -> (String, bool) {
    let t = &src[el.range.clone()];
    if t.starts_with("\\[") || t.starts_with("$$") {
        return ("equation".into(), true);
    }
    let name = t
        .strip_prefix("\\begin{")
        .and_then(|r| r.split('}').next())
        .unwrap_or("equation");
    (name.trim_end_matches('*').to_string(), name.ends_with('*'))
}

/// A heading's `\label{…}` right after it: its range and text.
fn label_after(src: &str, el: &Element) -> Option<(Range<usize>, String)> {
    let after = &src[el.range.end..];
    let ws = after.len() - after.trim_start().len();
    let at = el.range.end + ws;
    let c = find_cmd(src, at..(at + 200).min(src.len()), "label").filter(|c| c.start == at)?;
    Some((c.start..c.end, src[c.arg?].to_string()))
}

fn geometry_margin(src: &str) -> Option<String> {
    let begin = src.find("\\begin{document}").unwrap_or(src.len());
    let mut from = 0;
    while let Some(c) = find_cmd(src, from..begin, "usepackage") {
        if c.arg
            .as_ref()
            .is_some_and(|a| src[a.clone()].trim() == "geometry")
        {
            let opts = parse_opts(&src[c.opts?]);
            return opts
                .into_iter()
                .find(|(k, _)| k == "margin")
                .and_then(|(_, v)| v);
        }
        from = c.end;
    }
    None
}

// --- setting a field ---

/// The edit that gives field `key` of `el` the value `value` (empty: none / off).
pub fn set(src: &str, el: &Element, key: &str, value: &str) -> Option<Change> {
    let v = value.trim();
    match (el.kind, key) {
        (Kind::Figure | Kind::Image, "file") => {
            let g = graphics(src, el)?;
            change(g.arg?, v)
        }
        (Kind::Figure | Kind::Image, "width") => {
            let val = if v.is_empty() {
                None
            } else if let Ok(p) = v.trim_end_matches('%').parse::<f64>() {
                Some(format!("{}\\linewidth", p / 100.0))
            } else {
                Some(v.to_string())
            };
            set_graphics_opt(src, el, "width", val)
        }
        (Kind::Figure | Kind::Image, "keepaspectratio" | "clip") => {
            set_graphics_opt(src, el, key, (v == "true").then(String::new))
        }
        (Kind::Figure | Kind::Image, "height" | "scale" | "angle" | "trim" | "page") => {
            set_graphics_opt(src, el, key, (!v.is_empty()).then(|| v.to_string()))
        }
        (Kind::Figure, "align") => {
            let decl = match v {
                "center" => "\\centering",
                "right" => "\\raggedleft",
                _ => "\\raggedright",
            };
            let r = el.range.clone();
            for old in ["\\centering", "\\raggedleft", "\\raggedright"] {
                if let Some(i) = src[r.clone()].find(old) {
                    let at = r.start + i;
                    return change(at..at + old.len(), decl);
                }
            }
            let (_, _, end) = env_head(src, el)?;
            change(end..end, format!("\n  {decl}"))
        }
        (Kind::Figure | Kind::Table, "caption") => set_cmd_arg(src, el, "caption", v),
        (Kind::Figure | Kind::Table | Kind::Equation, "label") => set_cmd_arg(src, el, "label", v),
        (Kind::Figure | Kind::Table, "placement") => {
            let (_, opts, end) = env_head(src, el)?;
            match (opts, v.is_empty()) {
                (Some(o), true) => change(o.start - 1..o.end + 1, ""),
                (Some(o), false) => change(o, v),
                (None, false) => {
                    let name_end = end;
                    change(name_end..name_end, format!("[{v}]"))
                }
                (None, true) => None,
            }
        }
        (Kind::Heading, "level") => {
            let t = &src[el.range.clone()];
            let (old, _) = HEADINGS.iter().find(|(h, _)| {
                t[1..].starts_with(h)
                    && !t[1 + h.len()..].starts_with(|c: char| c.is_ascii_alphabetic())
            })?;
            let at = el.range.start + 1;
            change(at..at + old.len(), v)
        }
        (Kind::Heading, "numbered") => {
            let t = &src[el.range.clone()];
            let (h, _) = HEADINGS.iter().find(|(h, _)| t[1..].starts_with(h))?;
            let at = el.range.start + 1 + h.len();
            let starred = src[at..].starts_with('*');
            match (v == "true", starred) {
                (true, true) => change(at..at + 1, ""),
                (false, false) => change(at..at, "*"),
                _ => None,
            }
        }
        (Kind::Heading, "short") => {
            let t = &src[el.range.clone()];
            let (h, _) = HEADINGS.iter().find(|(h, _)| t[1..].starts_with(h))?;
            let cmd = find_cmd(src, el.range.clone(), h)?;
            match (cmd.opts, v.is_empty()) {
                (Some(o), true) => change(o.start - 1..o.end + 1, ""),
                (Some(o), false) => change(o, v),
                (None, false) => change(cmd.after_name..cmd.after_name, format!("[{v}]")),
                (None, true) => None,
            }
        }
        (Kind::Heading, "label") => match (label_after(src, el), v.is_empty()) {
            (Some((r, _)), true) => {
                let start = src[..r.start].trim_end().len();
                change(start..r.end, "")
            }
            (Some((r, _)), false) => change(r, format!("\\label{{{v}}}")),
            (None, false) => change(el.range.end..el.range.end, format!("\n\\label{{{v}}}")),
            (None, true) => None,
        },
        (Kind::Equation, "numbered") => {
            let (env, starred) = equation_env(src, el);
            set_equation_env(src, el, &env, v != "true" || (v.is_empty() && starred))
        }
        (Kind::Equation, "env") => {
            let (_, starred) = equation_env(src, el);
            set_equation_env(src, el, v, starred)
        }
        (Kind::List, "env") => {
            let (env, _, _) = env_head(src, el)?;
            let t = &src[el.range.clone()];
            let body_start = "\\begin{".len() + env.len() + 1;
            let body_end = t.len() - format!("\\end{{{env}}}").len();
            let text = format!("\\begin{{{v}}}{}\\end{{{v}}}", &t[body_start..body_end]);
            change(el.range.clone(), text)
        }
        (Kind::List, "label" | "start" | "nosep") => {
            let (_, opts, end) = env_head(src, el)?;
            let mut list = opts
                .clone()
                .map(|o| parse_opts(&src[o]))
                .unwrap_or_default();
            list.retain(|(k, _)| k != key);
            match key {
                "nosep" if v == "true" => list.push(("nosep".into(), None)),
                "nosep" => {}
                _ if !v.is_empty() => list.push((key.to_string(), Some(v.to_string()))),
                _ => {}
            }
            let text = if list.is_empty() {
                String::new()
            } else {
                format!("[{}]", render_opts(&list))
            };
            let range = match opts {
                Some(o) => o.start - 1..o.end + 1,
                None => end..end,
            };
            Some(Change {
                range,
                text,
                packages: vec!["enumitem"],
            })
        }
        (Kind::Document, "class") => {
            let c = find_cmd(src, 0..src.len(), "documentclass")?;
            change(c.arg?, v)
        }
        (Kind::Document, "fontsize" | "paper" | "twoside" | "twocolumn") => {
            let c = find_cmd(src, 0..src.len(), "documentclass")?;
            let mut opts = c
                .opts
                .clone()
                .map(|o| parse_opts(&src[o]))
                .unwrap_or_default();
            let group: &[&str] = match key {
                "fontsize" => &["10pt", "11pt", "12pt"],
                "paper" => &["a4paper", "letterpaper", "a5paper", "legalpaper"],
                "twoside" => &["twoside", "oneside"],
                _ => &["twocolumn", "onecolumn"],
            };
            opts.retain(|(k, _)| !group.contains(&k.as_str()));
            match key {
                "twoside" | "twocolumn" if v == "true" => opts.push((key.to_string(), None)),
                "twoside" | "twocolumn" => {}
                _ => opts.push((v.to_string(), None)),
            }
            let text = if opts.is_empty() {
                String::new()
            } else {
                format!("[{}]", render_opts(&opts))
            };
            let range = match c.opts {
                Some(o) => o.start - 1..o.end + 1,
                None => c.after_name..c.after_name,
            };
            change(range, text)
        }
        (Kind::Document, "margin") => set_margin(src, v),
        _ => None,
    }
}

/// `\name{…}` in `el` made to hold `v`: inserted after the figure's graphics (or the
/// environment's first line) when missing; removed when `v` is empty.
fn set_cmd_arg(src: &str, el: &Element, name: &str, v: &str) -> Option<Change> {
    match (find_cmd(src, el.range.clone(), name), v.is_empty()) {
        (Some(c), true) => {
            // the whole line it is on, when it is alone there
            let line = src[..c.start].rfind('\n').map_or(0, |i| i + 1);
            let start = if src[line..c.start].trim().is_empty() {
                line.saturating_sub(1)
            } else {
                c.start
            };
            change(start..c.end, "")
        }
        (Some(c), false) => change(c.arg?, v),
        (None, false) => {
            // after the caption for a label; else after the graphics or the first line
            let after = find_cmd(
                src,
                el.range.clone(),
                if name == "label" {
                    "caption"
                } else {
                    "includegraphics"
                },
            )
            .map(|c| c.end)
            .or_else(|| env_head(src, el).map(|(_, _, e)| e))
            .or_else(|| src[el.range.clone()].find('\n').map(|i| el.range.start + i))?;
            change(after..after, format!("\n  \\{name}{{{v}}}"))
        }
        (None, true) => None,
    }
}

fn set_graphics_opt(src: &str, el: &Element, key: &str, value: Option<String>) -> Option<Change> {
    let g = graphics(src, el)?;
    let mut opts = g
        .opts
        .clone()
        .map(|o| parse_opts(&src[o]))
        .unwrap_or_default();
    let at = opts.iter().position(|(k, _)| k == key);
    match (at, value) {
        (Some(i), Some(v)) => opts[i].1 = (!v.is_empty()).then_some(v),
        (None, Some(v)) => opts.push((key.to_string(), (!v.is_empty()).then_some(v))),
        (Some(i), None) => {
            opts.remove(i);
        }
        (None, None) => return None,
    }
    let text = if opts.is_empty() {
        String::new()
    } else {
        format!("[{}]", render_opts(&opts))
    };
    let range = match g.opts {
        Some(o) => o.start - 1..o.end + 1,
        None => g.after_name..g.after_name,
    };
    Some(Change {
        range,
        text,
        packages: vec!["graphicx"],
    })
}

/// An equation as `env` (numbered unless `starred`), keeping its body; `\[…\]` and `$$…$$` turn
/// into an environment when numbered or given a name.
fn set_equation_env(src: &str, el: &Element, env: &str, starred: bool) -> Option<Change> {
    let t = &src[el.range.clone()];
    let body = if let Some(b) = t.strip_prefix("\\[").and_then(|r| r.strip_suffix("\\]")) {
        b
    } else if let Some(b) = t.strip_prefix("$$").and_then(|r| r.strip_suffix("$$")) {
        b
    } else {
        let name = t.strip_prefix("\\begin{")?.split('}').next()?;
        let inner = &t["\\begin{".len() + name.len() + 1..];
        inner.strip_suffix(&format!("\\end{{{name}}}"))?
    };
    let name = if starred {
        format!("{env}*")
    } else {
        env.to_string()
    };
    let packages = if env == "equation" {
        vec![]
    } else {
        vec!["amsmath"]
    };
    let text = if env == "equation" && starred && (t.starts_with("\\[") || t.starts_with("$$")) {
        // unnumbered and already a display: keep it
        return None;
    } else {
        format!("\\begin{{{name}}}{body}\\end{{{name}}}")
    };
    Some(Change {
        range: el.range.clone(),
        text,
        packages,
    })
}

/// The margins through geometry's `margin=`: changing its option, or adding the package.
fn set_margin(src: &str, v: &str) -> Option<Change> {
    let begin = src.find("\\begin{document}")?;
    let mut from = 0;
    while let Some(c) = find_cmd(src, from..begin, "usepackage") {
        if c.arg
            .as_ref()
            .is_some_and(|a| src[a.clone()].trim() == "geometry")
        {
            let mut opts = c
                .opts
                .clone()
                .map(|o| parse_opts(&src[o]))
                .unwrap_or_default();
            opts.retain(|(k, _)| k != "margin");
            if !v.is_empty() {
                opts.push(("margin".into(), Some(v.to_string())));
            }
            let text = if opts.is_empty() {
                String::new()
            } else {
                format!("[{}]", render_opts(&opts))
            };
            let range = match c.opts {
                Some(o) => o.start - 1..o.end + 1,
                None => c.after_name..c.after_name,
            };
            return change(range, text);
        }
        from = c.end;
    }
    if v.is_empty() {
        return None;
    }
    let (at, _) = add_package(src, "geometry")?;
    change(at..at, format!("\\usepackage[margin={v}]{{geometry}}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(src: &str, c: Change) -> String {
        let mut s = src.to_string();
        s.replace_range(c.range, &c.text);
        s
    }

    fn value(src: &str, el: &Element, key: &str) -> String {
        fields(src, el)
            .into_iter()
            .find(|f| f.key == key)
            .unwrap()
            .value
    }

    const FIG: &str = "\\begin{figure}[h]\n  \\centering\n  \\includegraphics[width=0.8\\linewidth]{cat.png}\n  \\caption{A cat}\n\\end{figure}";

    #[test]
    fn a_figures_fields() {
        let el = element_at(FIG, Some(0..FIG.len()), 5);
        assert_eq!(el.kind, Kind::Figure);
        assert_eq!(value(FIG, &el, "width"), "80");
        assert_eq!(value(FIG, &el, "caption"), "A cat");
        assert_eq!(value(FIG, &el, "align"), "center");
        assert_eq!(value(FIG, &el, "placement"), "h");
        assert_eq!(value(FIG, &el, "file"), "cat.png");
        assert_eq!(value(FIG, &el, "label"), "");
    }

    #[test]
    fn setting_a_figures_fields_touches_only_their_part() {
        let el = element_at(FIG, Some(0..FIG.len()), 5);
        let s = apply(FIG, set(FIG, &el, "width", "50").unwrap());
        assert!(
            s.contains("\\includegraphics[width=0.5\\linewidth]{cat.png}"),
            "{s}"
        );
        let s = apply(FIG, set(FIG, &el, "angle", "90").unwrap());
        assert!(s.contains("[width=0.8\\linewidth, angle=90]"), "{s}");
        let s = apply(FIG, set(FIG, &el, "caption", "A dog").unwrap());
        assert!(s.contains("\\caption{A dog}"));
        let s = apply(FIG, set(FIG, &el, "label", "fig:cat").unwrap());
        assert!(s.contains("\\caption{A cat}\n  \\label{fig:cat}"), "{s}");
        let s = apply(FIG, set(FIG, &el, "align", "right").unwrap());
        assert!(s.contains("\\raggedleft") && !s.contains("\\centering"));
        let s = apply(FIG, set(FIG, &el, "placement", "tbp").unwrap());
        assert!(s.starts_with("\\begin{figure}[tbp]"));
        let s = apply(FIG, set(FIG, &el, "caption", "").unwrap());
        assert!(
            !s.contains("\\caption") && s.contains("cat.png}\n\\end{figure}"),
            "{s:?}"
        );
        let s = apply(FIG, set(FIG, &el, "keepaspectratio", "true").unwrap());
        assert!(s.contains("keepaspectratio]"));
    }

    #[test]
    fn headings_equations_lists() {
        let h = "\\section{Intro}";
        let el = element_at(h, Some(0..h.len()), 3);
        assert_eq!(el.kind, Kind::Heading);
        assert_eq!(value(h, &el, "level"), "section");
        assert_eq!(
            apply(h, set(h, &el, "level", "subsection").unwrap()),
            "\\subsection{Intro}"
        );
        assert_eq!(
            apply(h, set(h, &el, "numbered", "false").unwrap()),
            "\\section*{Intro}"
        );
        assert_eq!(
            apply(h, set(h, &el, "short", "In").unwrap()),
            "\\section[In]{Intro}"
        );
        assert_eq!(
            apply(h, set(h, &el, "label", "sec:intro").unwrap()),
            "\\section{Intro}\n\\label{sec:intro}"
        );
        let eq = "\\[ E = mc^2 \\]";
        let el = element_at(eq, Some(0..eq.len()), 0);
        assert_eq!(el.kind, Kind::Equation);
        assert_eq!(value(eq, &el, "numbered"), "false");
        assert_eq!(
            apply(eq, set(eq, &el, "numbered", "true").unwrap()),
            "\\begin{equation} E = mc^2 \\end{equation}"
        );
        let al = "\\begin{equation}\nx\n\\end{equation}";
        let el = element_at(al, Some(0..al.len()), 0);
        let c = set(al, &el, "env", "align").unwrap();
        assert_eq!(c.packages, vec!["amsmath"]);
        assert_eq!(apply(al, c), "\\begin{align}\nx\n\\end{align}");
        let l = "\\begin{itemize}\n  \\item a\n\\end{itemize}";
        let el = element_at(l, Some(0..l.len()), l.find('a').unwrap());
        assert_eq!(el.kind, Kind::List);
        assert_eq!(
            apply(l, set(l, &el, "env", "enumerate").unwrap()),
            "\\begin{enumerate}\n  \\item a\n\\end{enumerate}"
        );
        let c = set(l, &el, "nosep", "true").unwrap();
        assert_eq!(c.packages, vec!["enumitem"]);
        assert_eq!(
            apply(l, c),
            "\\begin{itemize}[nosep]\n  \\item a\n\\end{itemize}"
        );
    }

    #[test]
    fn the_document() {
        let d = "\\documentclass[12pt,a4paper]{article}\n\\usepackage{amsmath}\n\\begin{document}\nx\n\\end{document}";
        let el = element_at(d, None, 0);
        assert_eq!(el.kind, Kind::Document);
        assert_eq!(value(d, &el, "fontsize"), "12pt");
        assert_eq!(value(d, &el, "paper"), "a4paper");
        assert_eq!(value(d, &el, "class"), "article");
        let s = apply(d, set(d, &el, "fontsize", "11pt").unwrap());
        assert!(
            s.starts_with("\\documentclass[a4paper, 11pt]{article}"),
            "{s}"
        );
        let s = apply(d, set(d, &el, "margin", "2cm").unwrap());
        assert!(
            s.contains("\\usepackage{amsmath}\n\\usepackage[margin=2cm]{geometry}\n"),
            "{s}"
        );
        let s2 = apply(
            &s,
            set(&s, &element_at(&s, None, 0), "margin", "1in").unwrap(),
        );
        assert!(s2.contains("\\usepackage[margin=1in]{geometry}"));
        assert_eq!(value(&s2, &element_at(&s2, None, 0), "margin"), "1in");
    }
}
