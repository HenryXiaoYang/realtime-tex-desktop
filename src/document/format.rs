// Formatting as source edits, apart from any UI: what the ribbon's buttons do to the TeX.
// Inline formatting wraps the selection in commands (split where a paragraph, an item or an
// environment begins or ends, so the TeX stays whole) and toggles off by splitting the group it
// is in; sizes, colours and fonts rewrite a group that holds exactly the selection instead of
// nesting another; lists, alignment and paragraph styles rewrite whole blocks. Each edit says
// which packages it needs.
use super::caret::{self, group_close, group_command, item_at, open_groups};
use super::glyphmap::has_visible;
use std::ops::Range;

/// A formatting edit: `range` becomes `text`; the selection after it, and the packages it needs.
#[derive(Debug, PartialEq)]
pub struct Fmt {
    pub range: Range<usize>,
    pub text: String,
    pub select: Range<usize>,
    pub packages: Vec<&'static str>,
}

/// LaTeX's named sizes, smallest first.
pub const SIZES: &[&str] = &[
    "tiny",
    "scriptsize",
    "footnotesize",
    "small",
    "normalsize",
    "large",
    "Large",
    "LARGE",
    "huge",
    "Huge",
];

/// Inline commands, with the package each needs.
pub fn inline_package(cmd: &str) -> Option<&'static str> {
    match cmd {
        "textcolor" => Some("xcolor"),
        "sout" | "uline" => Some("ulem"),
        _ => None,
    }
}

/// The formatting open at `off` in `block`: each enclosing group's command (`textbf`), and for a
/// bare group the declaration it starts with (`large`, `fontspec`), outermost first.
pub fn formats_at(src: &str, block: Range<usize>, off: usize) -> Vec<String> {
    let text = &src[block.clone()];
    open_groups(text, block.start, off)
        .into_iter()
        .map(|(name, _, open)| {
            if !name.is_empty() {
                return name;
            }
            declaration(src, open).map_or(String::new(), |(d, _)| d.to_string())
        })
        .collect()
}

/// The declaration a bare group opening at `open` starts with (`{\large …`), and where it ends.
fn declaration(src: &str, open: usize) -> Option<(&str, usize)> {
    let rest = src.get(open + 1..)?;
    let name = rest.strip_prefix('\\')?;
    let len = name
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(name.len());
    let mut end = open + 2 + len;
    // \fontsize{13}{15.6}\selectfont, \fontspec{Name}: their arguments belong to it
    let d = &name[..len];
    if d == "fontsize" || d == "fontspec" || d == "color" {
        let s = src.as_bytes();
        while s.get(end) == Some(&b'{') {
            end = group_close(src, end)? + 1;
        }
        if d == "fontsize" && src[end..].starts_with("\\selectfont") {
            end += "\\selectfont".len();
        }
    }
    let end = end + src[end..].len() - src[end..].trim_start_matches(' ').len();
    Some((d, end))
}

/// The innermost group enclosing all of `sel` in `block` whose command (or declaration) `is`
/// accepts: its command's start, its `{`, and its `}`.
fn enclosing(
    src: &str,
    block: Range<usize>,
    sel: Range<usize>,
    is: impl Fn(&str) -> bool,
) -> Option<(usize, usize, usize)> {
    let text = &src[block.clone()];
    open_groups(text, block.start, sel.start)
        .into_iter()
        .rev()
        .find_map(|(name, start, open)| {
            let close = group_close(src, open)?;
            let named = if name.is_empty() {
                declaration(src, open).map(|(d, _)| d.to_string())
            } else {
                Some(name)
            };
            (named.is_some_and(|n| is(&n)) && close >= sel.end).then_some((start, open, close))
        })
}

/// The runs of `sel` a command can wrap: split where a paragraph, an item, an environment or
/// a group from outside the selection begins or ends; blank runs dropped.
fn segments(src: &str, sel: Range<usize>) -> Vec<Range<usize>> {
    let s = src.as_bytes();
    let mut out = vec![];
    let mut start = sel.start;
    let cut = |out: &mut Vec<Range<usize>>, a: usize, b: usize| {
        let t = &src[a..b.max(a)];
        let lead = t.len() - t.trim_start().len();
        let r = a + lead..(a + lead + t.trim().len()).max(a + lead);
        if has_visible(&src[r.clone()]) {
            out.push(r);
        }
    };
    let mut i = sel.start;
    while i < sel.end {
        // a blank line
        if s[i] == b'\n' {
            let rest = &src[i + 1..sel.end];
            let ws = rest.len() - rest.trim_start_matches([' ', '\t']).len();
            if rest[ws..].starts_with('\n') {
                cut(&mut out, start, i);
                let after = &src[i..sel.end];
                i += after.len() - after.trim_start().len();
                start = i;
                continue;
            }
        }
        if s[i] == b'\\' && (i == 0 || s[i - 1] != b'\\') {
            let rest = &src[i..sel.end];
            let token = ["\\item", "\\begin{", "\\end{", "\\\\", "\\par"]
                .into_iter()
                .find(|t| rest.starts_with(t));
            if let Some(t) = token {
                cut(&mut out, start, i);
                let mut j = i + t.len();
                if t.ends_with('{') {
                    j = src[j..].find('}').map_or(sel.end, |k| j + k + 1);
                    // an environment's arguments
                    while let Some(&c) = s.get(j).filter(|_| t == "\\begin{") {
                        let k = match c {
                            b'[' => src[j..].find(']').map(|k| j + k + 1),
                            b'{' => group_close(src, j).map(|k| k + 1),
                            _ => None,
                        };
                        match k {
                            Some(k) if k <= sel.end => j = k,
                            _ => break,
                        }
                    }
                }
                i = j.min(sel.end);
                start = i;
                continue;
            }
            i += 2;
            continue;
        }
        match s[i] {
            b'{' => {
                if group_close(src, i).is_none_or(|c| c >= sel.end) {
                    // a group that goes on past the selection: wrap up to it, then inside it
                    let (cmd, _) = group_command(src, i);
                    cut(&mut out, start, cmd.max(start));
                    start = i + 1;
                } else {
                    i = group_close(src, i).unwrap_or(i);
                }
            }
            // the end of a group that began before the selection
            b'}' => {
                cut(&mut out, start, i);
                start = i + 1;
            }
            b'%' => {
                while i < sel.end && s[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    cut(&mut out, start, sel.end);
    out
}

/// Wrap the runs of `sel` in `open`…`close`: the range replaced, its text, and the selection
/// after, on the text inside the new groups.
fn wrap(
    src: &str,
    sel: Range<usize>,
    open: &str,
    close: &str,
) -> Option<(Range<usize>, String, Range<usize>)> {
    let segs = segments(src, sel.clone());
    let (first, last) = (segs.first()?.start, segs.last()?.end);
    let mut text = String::new();
    let mut at = first;
    for r in segs {
        text.push_str(&src[at..r.start]);
        text.push_str(open);
        text.push_str(&src[r.clone()]);
        text.push_str(close);
        at = r.end;
    }
    let select = first + open.len()..first + text.len() - close.len();
    Some((first..last, text, select))
}

/// The word around `off` in `src` (letters and digits, no markup), if `off` is inside one.
fn word_at(src: &str, off: usize) -> Option<Range<usize>> {
    let wordy = |c: char| c.is_alphanumeric();
    let before = src[..off]
        .chars()
        .rev()
        .take_while(|c| wordy(*c))
        .map(char::len_utf8)
        .sum::<usize>();
    let after = src[off..]
        .chars()
        .take_while(|c| wordy(*c))
        .map(char::len_utf8)
        .sum::<usize>();
    // a command's name is no word
    let start = off - before;
    if before == 0 || after == 0 || src[..start].ends_with('\\') {
        return None;
    }
    Some(start..off + after)
}

/// Toggle inline command `cmd` (`textbf`…) over `sel` in `block`: off, by splitting the group of
/// it that holds the selection; else on, wrapping it. With no selection: the word at the caret,
/// else an empty group to type into (or, inside such a group, a split there to type out of it).
pub fn toggle_inline(src: &str, block: Range<usize>, sel: Range<usize>, cmd: &str) -> Option<Fmt> {
    let same = |n: &str| n == cmd || (cmd == "textit" && n == "emph");
    let packages: Vec<&'static str> = inline_package(cmd).into_iter().collect();
    if let Some((start, open, close)) = enclosing(src, block.clone(), sel.clone(), same) {
        let prefix = &src[start..=open];
        let (before, mid, after) = (
            &src[open + 1..sel.start],
            &src[sel.clone()],
            &src[sel.end..close],
        );
        let mut text = String::new();
        if has_visible(before) {
            text.push_str(prefix);
            text.push_str(before);
            text.push('}');
        } else {
            text.push_str(before);
        }
        let from = start + text.len();
        text.push_str(mid);
        let to = start + text.len();
        if has_visible(after) {
            text.push_str(prefix);
            text.push_str(after);
            text.push('}');
        } else {
            text.push_str(after);
        }
        return Some(Fmt {
            range: start..close + 1,
            text,
            select: from..to,
            packages: vec![],
        });
    }
    let sel = if sel.is_empty() {
        match word_at(src, sel.start) {
            Some(w) => w,
            None => {
                let open = format!("\\{cmd}{{");
                let at = sel.start;
                return Some(Fmt {
                    range: at..at,
                    text: format!("{open}}}"),
                    select: at + open.len()..at + open.len(),
                    packages,
                });
            }
        }
    } else {
        sel
    };
    let open = format!("\\{cmd}{{");
    let (range, text, select) = wrap(src, sel, &open, "}")?;
    Some(Fmt {
        range,
        text,
        select,
        packages,
    })
}

/// Set a group declaration over `sel` (a size `\large`, `\fontsize{13}{15.6}\selectfont`, a font
/// `\fontspec{Name}`): rewriting the declaration of a group of that kind holding exactly the
/// selection, else wrapping the selection in a new group.
fn declare(
    src: &str,
    block: Range<usize>,
    sel: Range<usize>,
    kind: impl Fn(&str) -> bool,
    decl: &str,
    packages: Vec<&'static str>,
) -> Option<Fmt> {
    let sel = if sel.is_empty() {
        word_at(src, sel.start)?
    } else {
        sel
    };
    if let Some((start, open, close)) = enclosing(src, block, sel.clone(), &kind)
        && start == open
        && let Some((_, end)) = declaration(src, open)
        && !has_visible(&src[end..sel.start])
        && !has_visible(&src[sel.end..close])
    {
        let text = format!("{{{decl} {}}}", &src[end..close]);
        let select = start + decl.len() + 2..start + text.len() - 1;
        return Some(Fmt {
            range: start..close + 1,
            text,
            select,
            packages,
        });
    }
    let (range, text, select) = wrap(src, sel, &format!("{{{decl} "), "}")?;
    Some(Fmt {
        range,
        text,
        select,
        packages,
    })
}

/// A size for the selection: one of SIZES, or points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Size {
    Named(&'static str),
    Pt(f32),
}

fn is_size(d: &str) -> bool {
    SIZES.contains(&d) || d == "fontsize"
}

pub fn set_size(src: &str, block: Range<usize>, sel: Range<usize>, size: Size) -> Option<Fmt> {
    let decl = match size {
        Size::Named(n) => format!("\\{n}"),
        Size::Pt(p) => format!(
            "\\fontsize{{{p}}}{{{}}}\\selectfont",
            (p * 1.2 * 10.0).round() / 10.0
        ),
    };
    declare(src, block, sel, is_size, &decl, vec![])
}

/// The named size at `off` (normalsize when none is set there).
pub fn size_at(src: &str, block: Range<usize>, off: usize) -> &'static str {
    formats_at(src, block, off)
        .iter()
        .rev()
        .find_map(|f| SIZES.iter().find(|s| **s == f.as_str()).copied())
        .unwrap_or("normalsize")
}

/// One named size up (`step` 1) or down (-1) from the one at the selection.
pub fn step_size(src: &str, block: Range<usize>, sel: Range<usize>, step: i32) -> Option<Fmt> {
    let now = size_at(src, block.clone(), sel.start);
    let i = SIZES.iter().position(|s| *s == now)? as i32 + step;
    let next = SIZES.get(usize::try_from(i).ok()?)?;
    set_size(src, block, sel, Size::Named(next))
}

/// A font for the selection: the document's serif, sans or monospaced family, or an installed
/// font by name (fontspec, so LuaLaTeX).
#[derive(Clone, Debug, PartialEq)]
pub enum Font {
    Serif,
    Sans,
    Mono,
    Named(String),
}

pub fn set_font(src: &str, block: Range<usize>, sel: Range<usize>, font: &Font) -> Option<Fmt> {
    let cmd = match font {
        Font::Serif => "textrm",
        Font::Sans => "textsf",
        Font::Mono => "texttt",
        Font::Named(n) => {
            return declare(
                src,
                block,
                sel,
                |d| d == "fontspec",
                &format!("\\fontspec{{{n}}}"),
                vec!["fontspec"],
            );
        }
    };
    // on, never toggled off: choosing a family sets it
    let families = |n: &str| ["textrm", "textsf", "texttt"].contains(&n);
    let sel = if sel.is_empty() {
        word_at(src, sel.start)?
    } else {
        sel
    };
    if let Some((start, open, close)) = enclosing(src, block.clone(), sel.clone(), families)
        && !has_visible(&src[open + 1..sel.start])
        && !has_visible(&src[sel.end..close])
    {
        let text = format!("\\{cmd}{{{}}}", &src[open + 1..close]);
        let select = start + cmd.len() + 2..start + text.len() - 1;
        return Some(Fmt {
            range: start..close + 1,
            text,
            select,
            packages: vec![],
        });
    }
    let (range, text, select) = wrap(src, sel, &format!("\\{cmd}{{"), "}")?;
    Some(Fmt {
        range,
        text,
        select,
        packages: vec![],
    })
}

/// Colour the selection (an xcolor name or `HTML` hex): rewriting the colour of a
/// `\textcolor` holding exactly it.
pub fn set_color(src: &str, block: Range<usize>, sel: Range<usize>, color: &str) -> Option<Fmt> {
    let spec = if color.len() == 6 && color.chars().all(|c| c.is_ascii_hexdigit()) {
        format!("[HTML]{{{}}}", color.to_ascii_uppercase())
    } else {
        format!("{{{color}}}")
    };
    let sel = if sel.is_empty() {
        word_at(src, sel.start)?
    } else {
        sel
    };
    if let Some((start, open, close)) =
        enclosing(src, block.clone(), sel.clone(), |n| n == "textcolor")
        && !has_visible(&src[open + 1..sel.start])
        && !has_visible(&src[sel.end..close])
    {
        let text = format!("\\textcolor{spec}{{{}}}", &src[open + 1..close]);
        let select = start + "\\textcolor".len() + spec.len() + 1..start + text.len() - 1;
        return Some(Fmt {
            range: start..close + 1,
            text,
            select,
            packages: vec!["xcolor"],
        });
    }
    let (range, text, select) = wrap(src, sel, &format!("\\textcolor{spec}{{"), "}")?;
    Some(Fmt {
        range,
        text,
        select,
        packages: vec!["xcolor"],
    })
}

/// The paragraphs of `blocks` (their source), split at blank lines.
fn paragraphs(src: &str, blocks: Range<usize>) -> Vec<&str> {
    let mut out = vec![];
    let mut para_start = None;
    let mut last_end = blocks.start;
    let text = &src[blocks.clone()];
    let mut off = 0;
    for line in text.split_inclusive('\n') {
        if line.trim().is_empty() {
            if let Some(s) = para_start.take() {
                out.push(src[blocks.start + s..blocks.start + last_end].trim());
            }
        } else {
            para_start.get_or_insert(off);
            last_end = off + line.len();
        }
        off += line.len();
    }
    if let Some(s) = para_start {
        out.push(src[blocks.start + s..blocks.end].trim());
    }
    out
}

/// The bulleted (`itemize`) or numbered (`enumerate`) list button on blocks `blocks`, the caret
/// at `at`: in a list of that kind, its items become paragraphs again; in the other kind, it
/// becomes this kind; on paragraphs, each becomes an item of a new list.
pub fn toggle_list(src: &str, blocks: Range<usize>, at: usize, env: &str) -> Option<Fmt> {
    if let Some(it) = item_at(src, at) {
        let list = it.list.clone();
        let (b, e) = (
            format!("\\begin{{{}}}", it.env),
            format!("\\end{{{}}}", it.env),
        );
        let body = &src[list.start + b.len()..list.end - e.len()];
        if it.env != env {
            let text = format!("\\begin{{{env}}}{body}\\end{{{env}}}");
            let select = list.start..list.start + text.len();
            return Some(Fmt {
                range: list,
                text,
                select,
                packages: vec![],
            });
        }
        // the items, each a paragraph (a [label] dropped)
        let body = body.trim_start();
        let body = if body.starts_with('[') {
            body.find(']').map_or(body, |i| &body[i + 1..])
        } else {
            body
        };
        let items: Vec<String> = split_items(body)
            .into_iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        let text = items.join("\n\n");
        let select = list.start..list.start + text.len();
        return Some(Fmt {
            range: list,
            text,
            select,
            packages: vec![],
        });
    }
    let paras = paragraphs(src, blocks.clone());
    if paras.is_empty() {
        return None;
    }
    let items: String = paras.iter().map(|p| format!("  \\item {p}\n")).collect();
    let text = format!("\\begin{{{env}}}\n{items}\\end{{{env}}}");
    let select = blocks.start..blocks.start + text.len();
    Some(Fmt {
        range: blocks,
        text,
        select,
        packages: vec![],
    })
}

/// A list's body split at its own `\item`s (not those of lists nested in it).
fn split_items(body: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut depth = 0;
    let mut start = None;
    let mut i = 0;
    let b = body.as_bytes();
    while i < body.len() {
        let rest = &body[i..];
        if rest.starts_with("\\begin{") {
            depth += 1;
        } else if rest.starts_with("\\end{") {
            depth -= 1;
        } else if depth == 0
            && rest.starts_with("\\item")
            && !rest[5..].starts_with(|c: char| c.is_ascii_alphabetic())
        {
            if let Some(s) = start {
                out.push(&body[s..i]);
            }
            let mut j = i + 5;
            if b.get(j) == Some(&b'[') {
                j = body[j..].find(']').map_or(j, |k| j + k + 1);
            }
            start = Some(j);
            i = j;
            continue;
        }
        i += rest.chars().next().map_or(1, char::len_utf8);
    }
    if let Some(s) = start {
        out.push(&body[s..]);
    }
    out
}

/// Paragraph alignment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
    Justify,
}

const ALIGN_ENVS: &[&str] = &["flushleft", "center", "flushright"];

/// The alignment of blocks `blocks`: the alignment environment holding exactly them changes (or,
/// to justify, goes); else one wraps them.
pub fn align(src: &str, blocks: Range<usize>, a: Align) -> Option<Fmt> {
    let env = match a {
        Align::Left => Some("flushleft"),
        Align::Center => Some("center"),
        Align::Right => Some("flushright"),
        Align::Justify => None,
    };
    // the blocks are an alignment environment's whole body, or the environment itself
    let around = alignment_env(src, blocks.clone());
    if let Some((outer, _, inner)) = around {
        let text = match env {
            Some(e) => format!("\\begin{{{e}}}{}\\end{{{e}}}", &src[inner.clone()]),
            None => src[inner.clone()].trim().to_string(),
        };
        let select = outer.start..outer.start + text.len();
        return Some(Fmt {
            range: outer,
            text,
            select,
            packages: vec![],
        });
    }
    let e = env?;
    let text = format!("\\begin{{{e}}}\n{}\n\\end{{{e}}}", &src[blocks.clone()]);
    let select = blocks.start..blocks.start + text.len();
    Some(Fmt {
        range: blocks,
        text,
        select,
        packages: vec![],
    })
}

/// The alignment environment `blocks` are: itself (when they begin with it) or its body: the
/// whole environment, its name, its body.
fn alignment_env(src: &str, blocks: Range<usize>) -> Option<(Range<usize>, String, Range<usize>)> {
    let t = &src[blocks.clone()];
    for env in ALIGN_ENVS {
        let (b, e) = (format!("\\begin{{{env}}}"), format!("\\end{{{env}}}"));
        if t.starts_with(&b) && t.ends_with(&e) {
            return Some((
                blocks.clone(),
                env.to_string(),
                blocks.start + b.len()..blocks.end - e.len(),
            ));
        }
        let before = src[..blocks.start].trim_end();
        let after = src[blocks.end..].trim_start();
        if before.ends_with(&b) && after.starts_with(&e) {
            let start = before.len() - b.len();
            let end = blocks.end + (src[blocks.end..].len() - after.len()) + e.len();
            return Some((start..end, env.to_string(), start + b.len()..end - e.len()));
        }
    }
    None
}

/// The current alignment of blocks `blocks`.
pub fn alignment_of(src: &str, blocks: Range<usize>) -> Align {
    match alignment_env(src, blocks).map(|(_, n, _)| n).as_deref() {
        Some("flushleft") => Align::Left,
        Some("center") => Align::Center,
        Some("flushright") => Align::Right,
        _ => Align::Justify,
    }
}

/// Paragraph styles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Style {
    Normal,
    Heading(&'static str),
    Quote,
    Title,
}

/// The heading levels the document class has, largest first.
pub fn heading_levels(src: &str) -> Vec<&'static str> {
    let class = src
        .find("\\documentclass")
        .and_then(|i| src[i..].find('{').map(|j| i + j + 1))
        .and_then(|i| src[i..].find('}').map(|j| &src[i..i + j]))
        .unwrap_or("article");
    let chapters = ["book", "report", "memoir", "scrbook", "scrreprt"].contains(&class);
    let mut out = vec!["part"];
    if chapters {
        out.push("chapter");
    }
    out.extend(["section", "subsection", "subsubsection", "paragraph"]);
    out
}

/// The style of block `block`.
pub fn style_of(src: &str, block: Range<usize>) -> Style {
    let t = &src[block.clone()];
    if t.starts_with("\\begin{quote}") || t.starts_with("\\begin{quotation}") {
        return Style::Quote;
    }
    if t.starts_with("\\maketitle") {
        return Style::Title;
    }
    heading_levels(src)
        .into_iter()
        .chain(["subparagraph"])
        .find(|h| {
            t.strip_prefix('\\')
                .and_then(|r| r.strip_prefix(h))
                .is_some_and(|r| r.starts_with(['{', '*', '[']))
        })
        .map_or(Style::Normal, Style::Heading)
}

/// The text of block `block` without its style: a heading's title, a quote's body, or itself.
fn bare_text(src: &str, block: Range<usize>) -> String {
    let t = &src[block.clone()];
    if let Some(title) = caret::heading_title(t, block.start) {
        return src[title].to_string();
    }
    for env in ["quote", "quotation"] {
        let (b, e) = (format!("\\begin{{{env}}}"), format!("\\end{{{env}}}"));
        if let Some(inner) = t.strip_prefix(&b).and_then(|r| r.strip_suffix(&e)) {
            return inner.trim().to_string();
        }
    }
    t.to_string()
}

/// Give block `block` style `style`. A title goes into the preamble (`\title{…}`) and the block
/// becomes `\maketitle` (or goes, when the document already makes its title).
pub fn set_style(src: &str, block: Range<usize>, style: Style) -> Option<Fmt> {
    let text = bare_text(src, block.clone());
    // a heading's title is one paragraph
    let one_line = |t: &str| t.split("\n\n").map(str::trim).collect::<Vec<_>>().join(" ");
    let star = src[block.clone()]
        .split('{')
        .next()
        .is_some_and(|h| h.ends_with('*'));
    let new = match style {
        Style::Normal => text,
        Style::Heading(h) => format!(
            "\\{h}{}{{{}}}",
            if star { "*" } else { "" },
            one_line(&text)
        ),
        Style::Quote => format!("\\begin{{quote}}\n{text}\n\\end{{quote}}"),
        Style::Title => return set_title(src, block, &one_line(&text)),
    };
    let select = block.start..block.start + new.len();
    Some(Fmt {
        range: block,
        text: new,
        select,
        packages: vec![],
    })
}

/// `\title{text}` in the preamble (replacing one there), and the block made `\maketitle`.
fn set_title(src: &str, block: Range<usize>, text: &str) -> Option<Fmt> {
    let begin = src.find("\\begin{document}")?;
    let has_make = src[begin..].contains("\\maketitle");
    let title = format!("\\title{{{text}}}");
    // one edit from the preamble's \title (or \begin{document}) to the block's end
    let (from, old_title) = match src[..begin].find("\\title{") {
        Some(i) => (i, group_close(src, i + 6).map(|c| i..c + 1)?),
        None => (begin, begin..begin),
    };
    let mut text_out = String::new();
    text_out.push_str(&title);
    if old_title.is_empty() {
        text_out.push('\n');
    }
    text_out.push_str(&src[old_title.end..block.start]);
    if !has_make {
        text_out.push_str("\\maketitle");
    }
    let select = from..from + text_out.len();
    let range = from..if has_make {
        // the block goes, with the blank line after it
        block.end + src[block.end..].len() - src[block.end..].trim_start().len()
    } else {
        block.end
    };
    Some(Fmt {
        range,
        text: text_out,
        select,
        packages: vec![],
    })
}

/// Where `\usepackage{name}` goes when the preamble does not load `name` yet: after its last
/// `\usepackage`, else before `\begin{document}`. None when it is loaded already.
pub fn add_package(src: &str, name: &str) -> Option<(usize, String)> {
    let begin = src.find("\\begin{document}")?;
    let pre = &src[..begin];
    let mut last = None;
    let mut from = 0;
    while let Some(i) = pre[from..].find("\\usepackage").map(|i| i + from) {
        let line_end = pre[i..].find('\n').map_or(begin, |j| i + j + 1);
        // \usepackage[opts]{a,b,name}
        let args = &pre[i..line_end];
        let list = args
            .rsplit_once('{')
            .map_or("", |(_, l)| l.split('}').next().unwrap_or(""));
        if list.split(',').any(|p| p.trim() == name) {
            return None;
        }
        last = Some(line_end);
        from = line_end;
    }
    let at = last.unwrap_or(begin);
    Some((at, format!("\\usepackage{{{name}}}\n")))
}

/// LuaLaTeX named for the document (`% !TEX program = lualatex` on its first line), as fontspec
/// needs, unless it names an engine already.
pub fn add_lualatex(src: &str) -> Option<(usize, String)> {
    (!src.lines().take(5).any(|l| l.contains("!TEX program")))
        .then(|| (0, "% !TEX program = lualatex\n".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run `f` on `src` with the selection between its `[` `]` (or the caret at `|`), as the
    /// source after, the selection marked again.
    fn run(src: &str, f: impl Fn(&str, Range<usize>, Range<usize>) -> Option<Fmt>) -> String {
        let (s, sel) = if let Some(a) = src.find('[').filter(|_| src.contains(']')) {
            let b = src.find(']').unwrap() - 1;
            (src.replacen('[', "", 1).replacen(']', "", 1), a..b)
        } else {
            let a = src.find('|').unwrap();
            (src.replacen('|', "", 1), a..a)
        };
        let e = f(&s, 0..s.len(), sel).expect("an edit");
        let mut out = s.clone();
        out.replace_range(e.range.clone(), &e.text);
        out.insert(e.select.end, ']');
        out.insert(e.select.start, '[');
        out
    }

    #[test]
    fn bold_wraps_and_unwraps() {
        let bold = |s: &str, b, r| toggle_inline(s, b, r, "textbf");
        assert_eq!(
            run("some [plain] text", bold),
            "some \\textbf{[plain]} text"
        );
        // across paragraphs: each wrapped on its own
        assert_eq!(
            run("one [two\n\nthree] four", bold),
            "one \\textbf{[two}\n\n\\textbf{three]} four"
        );
        // inside bold: off, the bold split around it
        assert_eq!(
            run("\\textbf{ab[cd]ef}", bold),
            "\\textbf{ab}[cd]\\textbf{ef}"
        );
        assert_eq!(run("\\textbf{[abcd]}", bold), "[abcd]");
        // the caret in a word: the word
        assert_eq!(run("a wo|rd here", bold), "a \\textbf{[word]} here");
        // the caret between words: an empty group to type into
        assert_eq!(run("a | b", bold), "a \\textbf{[]} b");
        // emphasis counts as italic
        assert_eq!(
            run("\\emph{[x]}", |s, b, r| toggle_inline(s, b, r, "textit")),
            "[x]"
        );
    }

    #[test]
    fn wrapping_never_breaks_a_group_or_an_item() {
        let it = |s: &str, b, r| toggle_inline(s, b, r, "textit");
        // a group from outside: its end stays out of the new one
        assert_eq!(
            run("\\textbf{a[b} c]d", it),
            "\\textbf{a\\textit{[b}} \\textit{c]}d"
        );
        assert_eq!(
            run(
                "\\begin{itemize}\n  \\item [one\n  \\item two]\n\\end{itemize}",
                it
            ),
            "\\begin{itemize}\n  \\item \\textit{[one}\n  \\item \\textit{two]}\n\\end{itemize}"
        );
    }

    #[test]
    fn sizes_and_colours_rewrite_rather_than_nest() {
        let large = |s: &str, b, r| set_size(s, b, r, Size::Named("large"));
        assert_eq!(run("a [big] b", large), "a {\\large [big]} b");
        assert_eq!(
            run("a {\\large [big]} b", |s, b, r| set_size(
                s,
                b,
                r,
                Size::Named("Huge")
            )),
            "a {\\Huge [big]} b"
        );
        assert_eq!(
            run("a [big] b", |s, b, r| set_size(s, b, r, Size::Pt(13.0))),
            "a {\\fontsize{13}{15.6}\\selectfont [big]} b"
        );
        assert_eq!(size_at("x {\\large ab} y", 0..15, 10), "large");
        assert_eq!(
            run("{\\large [big]}", |s, b, r| step_size(s, b, r, 1)),
            "{\\Large [big]}"
        );
        let red = |s: &str, b, r| set_color(s, b, r, "red");
        assert_eq!(run("a [hot] b", red), "a \\textcolor{red}{[hot]} b");
        assert_eq!(
            run("a \\textcolor{red}{[hot]} b", |s, b, r| set_color(
                s, b, r, "1f77b4"
            )),
            "a \\textcolor[HTML]{1F77B4}{[hot]} b"
        );
        assert_eq!(
            set_color("a b", 0..3, 0..1, "red").unwrap().packages,
            vec!["xcolor"]
        );
        // a family replaces a family
        assert_eq!(
            run("\\textsf{[code]}", |s, b, r| set_font(s, b, r, &Font::Mono)),
            "\\texttt{[code]}"
        );
        let named = set_font("a word b", 0..8, 2..6, &Font::Named("Avenir".into())).unwrap();
        assert_eq!(named.text, "{\\fontspec{Avenir} word}");
        assert_eq!(named.packages, vec!["fontspec"]);
    }

    #[test]
    fn lists_toggle() {
        let src = "First para.\n\nSecond para.";
        let e = toggle_list(src, 0..src.len(), 3, "itemize").unwrap();
        assert_eq!(
            e.text,
            "\\begin{itemize}\n  \\item First para.\n  \\item Second para.\n\\end{itemize}"
        );
        let list = e.text.clone();
        let mid = list.find("Second").unwrap();
        let back = toggle_list(&list, 0..list.len(), mid, "itemize").unwrap();
        assert_eq!(back.text, "First para.\n\nSecond para.");
        let num = toggle_list(&list, 0..list.len(), mid, "enumerate").unwrap();
        assert!(
            num.text.starts_with("\\begin{enumerate}") && num.text.ends_with("\\end{enumerate}")
        );
    }

    #[test]
    fn alignment_wraps_changes_and_removes() {
        let src = "Some text.";
        let c = align(src, 0..src.len(), Align::Center).unwrap();
        assert_eq!(c.text, "\\begin{center}\nSome text.\n\\end{center}");
        let centred = format!("x\n\n{}\n\ny", c.text);
        let body = centred.find("Some").unwrap();
        let blk = body..body + "Some text.".len();
        assert_eq!(alignment_of(&centred, blk.clone()), Align::Center);
        let r = align(&centred, blk.clone(), Align::Right).unwrap();
        assert_eq!(r.text, "\\begin{flushright}\nSome text.\n\\end{flushright}");
        let j = align(&centred, blk, Align::Justify).unwrap();
        assert_eq!(j.text, "Some text.");
    }

    #[test]
    fn styles() {
        let src = "\\documentclass{article}\n\\begin{document}\nIntro text\n\\end{document}";
        let b = src.find("Intro").unwrap()..src.find("text").unwrap() + 4;
        let h = set_style(src, b.clone(), Style::Heading("section")).unwrap();
        assert_eq!(h.text, "\\section{Intro text}");
        let mut s = src.to_string();
        s.replace_range(h.range.clone(), &h.text);
        let hb = s.find("\\section").unwrap()..s.find("\\end{document}").unwrap() - 1;
        assert_eq!(style_of(&s, hb.clone()), Style::Heading("section"));
        assert_eq!(
            set_style(&s, hb.clone(), Style::Normal).unwrap().text,
            "Intro text"
        );
        assert_eq!(
            set_style(&s, hb.clone(), Style::Heading("subsection"))
                .unwrap()
                .text,
            "\\subsection{Intro text}"
        );
        assert!(!heading_levels(src).contains(&"chapter"));
        assert!(heading_levels("\\documentclass[12pt]{report}").contains(&"chapter"));
        // a title goes to the preamble; the block becomes \maketitle
        let t = set_style(src, b, Style::Title).unwrap();
        let mut out = src.to_string();
        out.replace_range(t.range.clone(), &t.text);
        assert_eq!(
            out,
            "\\documentclass{article}\n\\title{Intro text}\n\\begin{document}\n\\maketitle\n\\end{document}"
        );
    }

    #[test]
    fn packages_are_added_once() {
        let src = "\\documentclass{article}\n\\usepackage[T1]{fontenc}\n\\usepackage{amsmath,xcolor}\n\\begin{document}\nx\n\\end{document}";
        assert_eq!(add_package(src, "xcolor"), None);
        let (at, text) = add_package(src, "graphicx").unwrap();
        assert_eq!(
            &src[..at],
            "\\documentclass{article}\n\\usepackage[T1]{fontenc}\n\\usepackage{amsmath,xcolor}\n"
        );
        assert_eq!(text, "\\usepackage{graphicx}\n");
        assert!(add_lualatex(src).is_some());
        assert!(add_lualatex("% !TEX program = xelatex\nx").is_none());
    }
}
