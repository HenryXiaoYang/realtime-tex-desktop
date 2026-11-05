// Editing typeset text with a caret, apart from any UI: where the caret can stand among a block's
// glyphs (and the source offset each spot is), and the source edits that typing, Enter, Tab and
// Backspace make: escaped text, list items, paragraph breaks that close and reopen formatting.
use super::glyphmap::{self, GlyphText};
use crate::engine::FontDesc;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// Where the caret goes in an edit's text (removed before it is applied).
pub const MARK: char = '\u{E001}';

/// A char of a block's typeset text.
pub struct Char {
    pub ch: char,
    /// The source bytes it shows (absolute); None for text a macro printed, or inline math.
    pub src: Option<Range<usize>>,
    /// x, baseline y, width, font size (sp); a space TeX set as glue gets the gap it fills.
    pub bbox: [f64; 4],
    /// It is a glyph, not such a gap.
    pub glyph: bool,
    /// The font the glyph is set in.
    pub font: Option<FontDesc>,
}

/// A spot the caret can stand at: between chars `k - 1` and `k`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    pub k: usize,
    pub off: usize,
    pub x: f64,
    pub y: f64,
    pub size: f64,
}

pub struct CharMap {
    pub chars: Vec<Char>,
    pub stops: Vec<Stop>,
}

/// The caret map of a block's rows on one page: `gt` their glyphs, `src` the block's source,
/// which starts at byte `base` of the file.
pub fn char_map(gt: &GlyphText, src: &str, base: usize) -> CharMap {
    let boxes = glyphmap::char_boxes(gt);
    let fonts = glyphmap::char_fonts(gt);
    let srcs = glyphmap::char_sources(gt, src);
    let math = math_regions(src);
    let n = gt.text.len();
    let mut chars = Vec::with_capacity(n);
    for i in 0..n {
        let prev = boxes[..i].iter().rev().find_map(|b| *b);
        let next = boxes[i + 1..].iter().find_map(|b| *b);
        let bbox = match (boxes[i], prev, next) {
            (Some(b), ..) => b,
            // a gap on one line spans to the next glyph; at a line end it has no width
            (None, Some(p), Some(q)) if (p[1] - q[1]).abs() < 0.3 * p[3] => {
                [p[0] + p[2], p[1], (q[0] - p[0] - p[2]).max(0.0), p[3]]
            }
            (None, Some(p), _) => [p[0] + p[2], p[1], 0.0, p[3]],
            (None, None, Some(q)) => [q[0], q[1], 0.0, q[3]],
            (None, None, None) => continue,
        };
        // math is edited as source: its chars stand for the whole formula, like a macro's
        let src = srcs[i]
            .clone()
            .filter(|r| !math.iter().any(|m| m.start < r.start && r.end <= m.end))
            .map(|r| r.start + base..r.end + base);
        chars.push(Char {
            ch: gt.text[i],
            src,
            bbox,
            glyph: boxes[i].is_some(),
            font: fonts[i].clone(),
        });
    }
    let mut stops: Vec<Stop> = vec![];
    for k in 0..=chars.len() {
        let (l, r) = (k.checked_sub(1).map(|i| &chars[i]), chars.get(k));
        // inside a macro's text or a formula there is no spot; after one, only before a glyph
        if l.is_some_and(|c| c.src.is_none()) && r.is_none_or(|c| !c.glyph) {
            continue;
        }
        // after a char (typing there takes its formatting), but a space TeX set as glue maps
        // loosely: before a glyph it is that glyph's start
        let after = l
            .filter(|c| c.glyph || r.is_none_or(|r| r.src.is_none()))
            .and_then(|c| c.src.as_ref().map(|s| s.end));
        let Some(off) = after.or_else(|| r.and_then(|c| c.src.as_ref().map(|s| s.start))) else {
            continue;
        };
        // before a glyph at its left edge (the start of a wrapped line), else after the char
        let (x, y, size) = match (l, r) {
            (_, Some(c)) if c.glyph => (c.bbox[0], c.bbox[1], c.bbox[3]),
            (Some(c), _) => (c.bbox[0] + c.bbox[2], c.bbox[1], c.bbox[3]),
            (None, Some(c)) => (c.bbox[0], c.bbox[1], c.bbox[3]),
            (None, None) => continue,
        };
        if stops.iter().all(|s| s.off != off) {
            stops.push(Stop { k, off, x, y, size });
        }
    }
    CharMap { chars, stops }
}

impl CharMap {
    /// The stop for source offset `off`: the one at it, else the last one before it.
    pub fn stop_for(&self, off: usize) -> Option<usize> {
        self.stops.iter().position(|s| s.off == off).or_else(|| {
            self.stops
                .iter()
                .enumerate()
                .filter(|(_, s)| s.off <= off)
                .max_by_key(|(_, s)| s.off)
                .map(|(i, _)| i)
        })
    }

    /// The stop nearest (`x`, `y`) sp: on the nearest line, then the nearest across.
    pub fn stop_near(&self, x: f64, y: f64) -> Option<usize> {
        let dist = |s: &Stop| {
            let (t, b) = (s.y - 0.75 * s.size, s.y + 0.25 * s.size);
            let v = if y < t {
                t - y
            } else if y > b {
                y - b
            } else {
                0.0
            };
            (v, (s.x - x).abs())
        };
        self.stops
            .iter()
            .enumerate()
            .min_by(|a, b| dist(a.1).partial_cmp(&dist(b.1)).unwrap())
            .map(|(i, _)| i)
    }

    /// The source a Backspace (`back`) or Delete at stop `i` removes: the char beside it, or the
    /// whole macro or formula it is part of. None at the block's edge.
    pub fn deletion(&self, i: usize, back: bool, src: &str) -> Option<Range<usize>> {
        let Stop { k, off, .. } = *self.stops.get(i)?;
        let at = if back { k.checked_sub(1)? } else { k };
        let c = self.chars.get(at)?;
        // a space TeX set as glue maps loosely: the source's whitespace beside the caret
        if !c.glyph {
            let ws = if back {
                off - src.get(..off)?.trim_end().len()
            } else {
                src.get(off..)?.len() - src.get(off..)?.trim_start().len()
            };
            if ws > 0 {
                return Some(if back { off - ws..off } else { off..off + ws });
            }
        }
        if let Some(r) = &c.src {
            return Some(r.clone());
        }
        // an atom: from the end of the glyph before it to the start of the glyph after it
        let mapped = |c: &Char| c.src.clone().filter(|_| c.glyph);
        let lo = self.chars[..at]
            .iter()
            .rev()
            .find_map(|c| mapped(c).map(|s| s.end));
        let hi = self.chars[at..]
            .iter()
            .find_map(|c| mapped(c).map(|s| s.start));
        let (lo, hi) = match (lo, hi) {
            (Some(a), Some(b)) if a < b => (a, b),
            (Some(a), None) => (a, a + src.get(a..)?.trim_end().len()),
            (None, Some(b)) => (b - src.get(..b)?.trim_start().len(), b),
            _ => return None,
        };
        let t = src.get(lo..hi)?;
        let start = lo + (t.len() - t.trim_start().len());
        Some(start..(start + t.trim().len()).max(start))
    }
}

/// `$…$` and `\(…\)` in `src`, delimiters included.
pub fn math_regions(src: &str) -> Vec<Range<usize>> {
    let s = src.as_bytes();
    let mut out = vec![];
    let mut open: Option<(usize, &str)> = None;
    let mut i = 0;
    while i < s.len() {
        match s[i] {
            b'\\' if s.get(i + 1) == Some(&b'(') && open.is_none() => {
                open = Some((i, "\\)"));
                i += 2;
            }
            b'\\' if s.get(i + 1) == Some(&b')') && open.is_some_and(|o| o.1 == "\\)") => {
                out.push(open.take().unwrap().0..i + 2);
                i += 2;
            }
            b'\\' => i += 2,
            b'%' => {
                while i < s.len() && s[i] != b'\n' {
                    i += 1;
                }
            }
            b'$' if s.get(i + 1) == Some(&b'$') => i += 2,
            b'$' => {
                match open {
                    Some((a, "$")) => {
                        out.push(a..i + 1);
                        open = None;
                    }
                    None => open = Some((i, "$")),
                    _ => {}
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// Text as TeX source that prints it: the special characters escaped.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\textbackslash{}"),
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            '{' | '}' | '$' | '&' | '#' | '%' | '_' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Scripts written without spaces between words: each input-method commit is a word.
pub fn spaceless(c: char) -> bool {
    matches!(c as u32,
        0x0E00..=0x0EFF   // Thai, Lao
        | 0x1000..=0x109F // Myanmar
        | 0x1780..=0x17FF // Khmer
        | 0x3040..=0x30FF // Hiragana, Katakana
        | 0x3400..=0x4DBF // CJK Extension A
        | 0x4E00..=0x9FFF // CJK Unified Ideographs
        | 0xF900..=0xFAFF
        | 0x20000..=0x3FFFF)
}

/// Whether typing `new` right after `before` (what was typed so far) begins a new undo step:
/// a new word by Unicode word boundaries (UAX #29), or any text in a script without spaces.
pub fn new_step(before: &str, new: &str) -> bool {
    let Some(last) = before.chars().last() else {
        return true;
    };
    if new.chars().chain([last]).any(spaceless) {
        return true;
    }
    let tail: String = before
        .chars()
        .rev()
        .take(24)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let joined = format!("{tail}{new}");
    let words: Vec<(usize, &str)> = joined.split_word_bound_indices().collect();
    let Some(ix) = words.iter().position(|(i, _)| *i == tail.len()) else {
        return false; // the new text continues the word before it ("can" + "'t")
    };
    let wordy = |s: &str| s.chars().any(char::is_alphanumeric);
    wordy(words[ix].1) && (ix == 0 || !wordy(words[ix - 1].1))
}

/// Byte at `i` starts a command `\x` (not an escaped backslash `\\x`).
fn is_cmd(s: &[u8], i: usize) -> bool {
    s[i] == b'\\' && s[..i].iter().rev().take_while(|&&b| b == b'\\').count() % 2 == 0
}

const LISTS: &[&str] = &["itemize", "enumerate", "description"];

/// Inline formatting a paragraph break closes and reopens.
const FORMAT: &[&str] = &[
    "textbf",
    "textit",
    "textsl",
    "textsc",
    "textsf",
    "texttt",
    "textrm",
    "textup",
    "textmd",
    "emph",
    "underline",
    "textcolor",
    "colorbox",
    "uline",
    "sout",
    "hl",
];

#[derive(Debug, PartialEq)]
enum Tok {
    Begin(String),
    End(String),
    Item,
}

/// `\begin{x}`, `\end{x}` and `\item` in `src`, outside comments: (start, end, token).
fn tokens(src: &str) -> Vec<(usize, usize, Tok)> {
    let s = src.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'%' && (i == 0 || !is_cmd(s, i - 1)) {
            while i < s.len() && s[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if !is_cmd(s, i) {
            i += 1;
            continue;
        }
        let rest = &src[i..];
        let env = |p: &str| {
            let name = rest[p.len()..].split('}').next()?;
            Some((name.to_string(), p.len() + name.len() + 1))
        };
        if rest.starts_with("\\begin{")
            && let Some((name, len)) = env("\\begin{")
        {
            out.push((i, i + len, Tok::Begin(name)));
            i += len;
        } else if rest.starts_with("\\end{")
            && let Some((name, len)) = env("\\end{")
        {
            out.push((i, i + len, Tok::End(name)));
            i += len;
        } else if rest.starts_with("\\item")
            && !rest[5..].starts_with(|c: char| c.is_ascii_alphabetic())
        {
            out.push((i, i + 5, Tok::Item));
            i += 5;
        } else {
            i += 2;
        }
    }
    out
}

/// The list item the caret at `off` is in.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub env: String,
    /// The list: `\begin{env}` to the end of `\end{env}`.
    pub list: Range<usize>,
    /// Start of the item's line (its indent), `\item`, and its text (after `\item` and any
    /// `[label]`) up to the next item or the list's end, trailing space trimmed.
    pub line: usize,
    pub item: usize,
    pub text: Range<usize>,
    pub first: bool,
    pub last: bool,
    pub indent: String,
    /// The list it is nested in, if any.
    pub parent: Option<Range<usize>>,
}

/// The innermost list item holding `off`.
pub fn item_at(src: &str, off: usize) -> Option<Item> {
    let toks = tokens(src);
    // open lists: env, \begin start, current item start, items so far
    let mut stack: Vec<(String, usize, Option<usize>, usize)> = vec![];
    let mut k = 0;
    while k < toks.len() && toks[k].0 < off {
        match &toks[k].2 {
            Tok::Begin(n) if LISTS.contains(&n.as_str()) => {
                stack.push((n.clone(), toks[k].0, None, 0))
            }
            Tok::End(n) if stack.last().is_some_and(|t| &t.0 == n) => {
                stack.pop();
            }
            Tok::Item => {
                if let Some(t) = stack.last_mut() {
                    t.2 = Some(toks[k].0);
                    t.3 += 1;
                }
            }
            _ => {}
        }
        k += 1;
    }
    let (env, begin, item, count) = stack.last().cloned()?;
    let item = item?;
    // the item's end and the list's: the next item or \end at this depth
    let (mut depth, mut next, mut end) = (0, None, None);
    for (a, b, t) in &toks[k..] {
        match t {
            Tok::Begin(n) if LISTS.contains(&n.as_str()) => depth += 1,
            Tok::End(n) if LISTS.contains(&n.as_str()) => {
                if depth == 0 {
                    end = Some((*a, *b));
                    break;
                }
                depth -= 1;
            }
            Tok::Item if depth == 0 && next.is_none() => next = Some(*a),
            _ => {}
        }
    }
    let (end_start, end) = end?;
    let s = src.as_bytes();
    let mut t = item + 5;
    if s.get(t) == Some(&b'[') {
        t = src[t..].find(']').map_or(t, |j| t + j + 1);
    }
    while s.get(t).is_some_and(|c| *c == b' ' || *c == b'\t') {
        t += 1;
    }
    let stop = next.unwrap_or(end_start);
    let text_end = (t + src[t..stop].trim_end().len()).max(t);
    let line = src[..item].rfind('\n').map_or(0, |i| i + 1);
    let indent: String = src[line..item]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect();
    let parent = (stack.len() > 1).then(|| {
        let (_, pb, ..) = &stack[stack.len() - 2];
        *pb..list_end(&toks, *pb).unwrap_or(src.len())
    });
    Some(Item {
        env,
        list: begin..end,
        line,
        item,
        text: t..text_end,
        first: count == 1,
        last: next.is_none(),
        indent,
        parent,
    })
}

/// The end of the list whose `\\begin` is at `begin`.
fn list_end(toks: &[(usize, usize, Tok)], begin: usize) -> Option<usize> {
    let mut depth = 0;
    for (_, b, t) in toks.iter().filter(|t| t.0 >= begin) {
        match t {
            Tok::Begin(n) if LISTS.contains(&n.as_str()) => depth += 1,
            Tok::End(n) if LISTS.contains(&n.as_str()) => {
                depth -= 1;
                if depth == 0 {
                    return Some(*b);
                }
            }
            _ => {}
        }
    }
    None
}

/// A change to the source: `range` becomes `text`, the caret goes to `caret`.
#[derive(Debug, PartialEq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
    pub caret: usize,
}

impl Edit {
    /// `range` becomes `text`, the caret at its MARK (or its end).
    fn marked(range: Range<usize>, text: &str) -> Self {
        let caret = range.start + text.find(MARK).unwrap_or(text.len());
        Self {
            text: text.replace(MARK, ""),
            range,
            caret,
        }
    }
}

/// Lists left with no items dropped, along with their line.
fn drop_empty_lists(text: &str) -> String {
    let mut t = text.to_string();
    'again: loop {
        for env in LISTS {
            let (b, e) = (format!("\\begin{{{env}}}"), format!("\\end{{{env}}}"));
            let mut from = 0;
            while let Some(i) = t[from..].find(&b).map(|i| i + from) {
                let after = i + b.len();
                let ws = t[after..].len() - t[after..].trim_start().len();
                if t[after + ws..].starts_with(&e) {
                    let line_start = t[..i].trim_end_matches([' ', '\t']).len();
                    let mut end = after + ws + e.len();
                    if t[end..].starts_with('\n') {
                        end += 1;
                    }
                    t.replace_range(line_start..end, "");
                    continue 'again;
                }
                from = after;
            }
        }
        return t;
    }
}

/// Enter at `off` in list item `it`: a new item; on an empty item, leave the list there.
pub fn enter_in_item(src: &str, it: &Item, off: usize) -> Edit {
    let empty = !glyphmap::has_visible(&src[it.text.clone()]);
    if !empty {
        let at = off.clamp(it.text.start, it.text.end.max(off));
        return Edit::marked(at..at, &format!("\n{}\\item {MARK}", it.indent));
    }
    // split the list around the empty item: what is left of either half stays a list
    let (start, li, head, tail) = list_parts(src, it);
    let env = &it.env;
    let text = format!("{head}\n{li}\\end{{{env}}}\n\n{MARK}\n\n{li}\\begin{{{env}}}{tail}");
    Edit::marked(start..it.list.end, &tidy(&text))
}

/// Item `it`'s list split around it: where its `\\begin` line starts, that line's indent, the
/// list before the item and after its text.
fn list_parts<'a>(src: &'a str, it: &Item) -> (usize, &'a str, &'a str, &'a str) {
    let start = src[..it.list.start].rfind('\n').map_or(0, |i| i + 1);
    let start = if src[start..it.list.start].trim().is_empty() {
        start
    } else {
        it.list.start
    };
    (
        start,
        &src[start..it.list.start],
        src[start..it.line].trim_end(),
        &src[it.text.end..it.list.end],
    )
}

/// Paragraph breaks and empty lists cleaned up after an edit that split or joined lists.
fn tidy(text: &str) -> String {
    let mut t = drop_empty_lists(text);
    while t.contains("\n\n\n") {
        t = t.replace("\n\n\n", "\n\n");
    }
    // a list that became empty at an end leaves its paragraph break dangling
    t.trim_start_matches('\n').trim_end().to_string()
}

/// Tab: item `it` becomes a sub-item of the one before it (joining that one's sub-list if it
/// ends with one). None on a list's first item.
pub fn indent_item(src: &str, it: &Item, off: usize) -> Option<Edit> {
    if it.first {
        return None;
    }
    let caret = off.saturating_sub(it.text.start);
    let body = format!(
        "{}\\item {}",
        it.indent.clone() + "  ",
        insert_mark(&src[it.text.clone()], caret)
    );
    let before = src[..it.line].trim_end();
    for env in LISTS {
        let end = format!("\\end{{{env}}}");
        if before.ends_with(&end) && before.len() >= it.list.start {
            let at = before.len() - end.len();
            let line = src[..at].rfind('\n').map_or(0, |i| i + 1);
            return Some(Edit::marked(
                line..it.text.end,
                &format!("{body}\n{}\\end{{{env}}}", &src[line..at]),
            ));
        }
    }
    let env = &it.env;
    Some(Edit::marked(
        it.line..it.text.end,
        &format!(
            "{}\\begin{{{env}}}\n{body}\n{}\\end{{{env}}}",
            it.indent, it.indent
        ),
    ))
}

/// Shift-Tab: sub-item `it` moves out to the list its list is nested in. None at the top level.
pub fn outdent_item(src: &str, it: &Item, off: usize) -> Option<Edit> {
    it.parent.as_ref()?;
    let env = &it.env;
    let caret = off.saturating_sub(it.text.start);
    // the sub-list's indent is its parent item's
    let (start, li, head, tail) = list_parts(src, it);
    let text = format!(
        "{head}\n{li}\\end{{{env}}}\n{li}\\item {}\n{li}\\begin{{{env}}}{tail}",
        insert_mark(&src[it.text.clone()], caret),
    );
    Some(Edit::marked(start..it.list.end, &tidy(&text)))
}

/// Backspace at the start of item `it`: join it to the item before; a sub-list's first item
/// moves out a level, a list's first item leaves the list.
pub fn backspace_item(src: &str, it: &Item, off: usize) -> Option<Edit> {
    if !it.first {
        let prev_end = src[..it.line].trim_end().len();
        let sep = if it.text.is_empty() { "" } else { " " };
        return Some(Edit::marked(
            prev_end..it.text.start,
            &format!("{MARK}{sep}"),
        ));
    }
    if it.parent.is_some() {
        return outdent_item(src, it, off);
    }
    let env = &it.env;
    let (start, li, _, tail) = list_parts(src, it);
    let text = format!(
        "{MARK}{}\n\n{li}\\begin{{{env}}}{tail}",
        &src[it.text.clone()]
    );
    Some(Edit::marked(start..it.list.end, &tidy(&text)))
}

fn insert_mark(s: &str, at: usize) -> String {
    let at = at.min(s.len());
    let at = (0..=at).rev().find(|&i| s.is_char_boundary(i)).unwrap_or(0);
    format!("{}{MARK}{}", &s[..at], &s[at..])
}

/// For each brace group open at `off` in `block` (source starting at `base`), outermost first:
/// the command it is the argument of, its start, and where its `{` is. A bare `{` has no name.
pub fn open_groups(block: &str, base: usize, off: usize) -> Vec<(String, usize, usize)> {
    let s = block.as_bytes();
    let end = off.saturating_sub(base).min(s.len());
    let mut stack: Vec<usize> = vec![];
    let mut i = 0;
    while i < end {
        match s[i] {
            b'\\' => i += 1,
            b'%' => {
                while i < end && s[i] != b'\n' {
                    i += 1;
                }
            }
            b'{' => stack.push(i),
            b'}' => {
                stack.pop();
            }
            _ => {}
        }
        i += 1;
    }
    stack
        .into_iter()
        .map(|open| {
            let (start, name) = group_command(block, open);
            (name, base + start, base + open)
        })
        .collect()
}

/// The command whose argument the group opening at `open` is: its start and name (past any
/// earlier arguments, `\textcolor{red}{`); the group itself and "" when there is none.
fn group_command(src: &str, open: usize) -> (usize, String) {
    let s = src.as_bytes();
    let mut i = open;
    // step back over earlier {…} and […] arguments
    loop {
        let j = src[..i].trim_end().len();
        let Some(&close) = j.checked_sub(1).map(|k| &s[k]) else {
            break;
        };
        let opener = match close {
            b'}' => b'{',
            b']' => b'[',
            _ => break,
        };
        let mut depth = 0;
        let mut k = j;
        let mut found = None;
        while k > 0 {
            k -= 1;
            if s[k] == close {
                depth += 1;
            } else if s[k] == opener {
                depth -= 1;
                if depth == 0 {
                    found = Some(k);
                    break;
                }
            }
        }
        match found {
            Some(k) => i = k,
            None => break,
        }
    }
    let name_end = i;
    let name_start = src[..name_end]
        .trim_end_matches(|c: char| c.is_ascii_alphabetic())
        .len();
    if name_start < name_end && name_start > 0 && s[name_start - 1] == b'\\' {
        (name_start - 1, src[name_start..name_end].to_string())
    } else {
        (open, String::new())
    }
}

/// Enter at `off` in a paragraph (`block` its source from `base`): a paragraph break, closing
/// the formatting open there and reopening it after. In a heading or other command's argument,
/// the new paragraph starts after the block. None when there is nothing to break (`at_start`
/// of a paragraph with no text before the caret is still a break: an empty line above).
pub fn enter_in_paragraph(block: &str, base: usize, off: usize) -> Edit {
    let groups = open_groups(block, base, off);
    if groups.iter().all(|(n, ..)| FORMAT.contains(&n.as_str())) {
        let close = "}".repeat(groups.len());
        let reopen: String = groups
            .iter()
            .map(|(_, start, open)| &block[start - base..=open - base])
            .collect();
        return Edit::marked(off..off, &format!("{close}\n\n{reopen}{MARK}"));
    }
    let end = base + block.len();
    Edit::marked(end..end, &format!("\n\n{MARK}"))
}

/// Shift-Enter: a line break, except inside a heading or a command's argument.
pub fn line_break(block: &str, base: usize, off: usize) -> Option<Edit> {
    let groups = open_groups(block, base, off);
    groups
        .iter()
        .all(|(n, ..)| FORMAT.contains(&n.as_str()))
        .then(|| Edit::marked(off..off, &format!("\\\\\n{MARK}")))
}

/// After `range` of `src` was deleted (now at `at`): an empty `\cmd{}` left around `at`.
pub fn empty_group(src: &str, at: usize) -> Option<Range<usize>> {
    if !src.get(at..)?.starts_with('}') || !src.get(..at)?.ends_with('{') {
        return None;
    }
    let (start, name) = group_command(src, at - 1);
    (!name.is_empty() && FORMAT.contains(&name.as_str())).then_some(start..at + 1)
}

/// A block that is running text (not a heading, environment or display).
pub fn is_prose(block: &str) -> bool {
    !block.starts_with('\\')
        || FORMAT.iter().any(|f| {
            block[1..].starts_with(f) && !block[1 + f.len()..].starts_with(char::is_alphabetic)
        })
        || block.starts_with("\\noindent")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(src: &str, e: &Edit) -> String {
        let mut s = src.to_string();
        s.replace_range(e.range.clone(), &e.text);
        s.insert(e.caret, '|');
        s
    }

    #[test]
    fn escapes_specials() {
        assert_eq!(
            escape("50% & $5 #1 a_b {x}"),
            "50\\% \\& \\$5 \\#1 a\\_b \\{x\\}"
        );
        assert_eq!(
            escape("\\~^"),
            "\\textbackslash{}\\textasciitilde{}\\textasciicircum{}"
        );
        assert!(!glyphmap::has_visible("\\label{x} % c\n"));
        assert!(glyphmap::has_visible(&escape("%")));
    }

    #[test]
    fn undo_steps_follow_words() {
        assert!(new_step("", "a"));
        assert!(!new_step("hel", "l"));
        assert!(!new_step("hello", " "));
        assert!(new_step("hello ", "w"));
        assert!(new_step("end.", "N") || new_step("end. ", "N"));
        assert!(!new_step("can", "'"));
        // every commit of an input method in a spaceless script is a step
        assert!(new_step("你好", "世界"));
        assert!(new_step("x", "日本"));
        // Cyrillic and Greek words follow spaces like Latin ones
        assert!(!new_step("приве", "т"));
        assert!(new_step("привет ", "м"));
    }

    #[test]
    fn finds_inline_math() {
        let s = "a $x^2$ b \\(y\\) c \\$ d $$z$$";
        let m: Vec<&str> = math_regions(s).into_iter().map(|r| &s[r]).collect();
        assert_eq!(m, vec!["$x^2$", "\\(y\\)"]);
    }

    const LIST: &str = "\\begin{itemize}\n  \\item one\n  \\item two\n\\end{itemize}";

    #[test]
    fn finds_the_item() {
        let off = LIST.find("two").unwrap() + 1;
        let it = item_at(LIST, off).unwrap();
        assert_eq!(&LIST[it.text.clone()], "two");
        assert!(!it.first && it.last);
        assert_eq!(it.indent, "  ");
        assert_eq!(it.list, 0..LIST.len());
        assert!(item_at("plain text", 3).is_none());
        // \itemsep is no item
        assert!(item_at("\\begin{itemize}\\itemsep0pt x", 20).is_none());
    }

    #[test]
    fn enter_adds_an_item_and_leaves_on_an_empty_one() {
        let off = LIST.find("one").unwrap() + 3;
        let it = item_at(LIST, off).unwrap();
        let e = enter_in_item(LIST, &it, off);
        assert_eq!(
            apply(LIST, &e),
            "\\begin{itemize}\n  \\item one\n  \\item |\n  \\item two\n\\end{itemize}"
        );
        // an empty last item ends the list
        let src = "\\begin{itemize}\n  \\item one\n  \\item \n\\end{itemize}";
        let off = src.rfind("\\item").unwrap() + 6;
        let it = item_at(src, off).unwrap();
        assert_eq!(
            apply(src, &enter_in_item(src, &it, off)),
            "\\begin{itemize}\n  \\item one\n\\end{itemize}\n\n|"
        );
    }

    #[test]
    fn tab_nests_and_shift_tab_unnests() {
        let off = LIST.find("two").unwrap();
        let it = item_at(LIST, off).unwrap();
        let e = indent_item(LIST, &it, off).unwrap();
        let nested = apply(LIST, &e);
        assert_eq!(
            nested,
            "\\begin{itemize}\n  \\item one\n  \\begin{itemize}\n    \\item |two\n  \\end{itemize}\n\\end{itemize}"
        );
        let src = nested.replace('|', "");
        let off = src.find("two").unwrap();
        let it = item_at(&src, off).unwrap();
        assert!(it.parent.is_some());
        let back = apply(&src, &outdent_item(&src, &it, off).unwrap());
        assert_eq!(
            back,
            "\\begin{itemize}\n  \\item one\n  \\item |two\n\\end{itemize}"
        );
        // the first item cannot nest
        let it = item_at(LIST, LIST.find("one").unwrap()).unwrap();
        assert!(indent_item(LIST, &it, 0).is_none());
    }

    #[test]
    fn backspace_joins_items_and_unlists_the_first() {
        let off = LIST.find("two").unwrap();
        let it = item_at(LIST, off).unwrap();
        assert_eq!(
            apply(LIST, &backspace_item(LIST, &it, off).unwrap()),
            "\\begin{itemize}\n  \\item one| two\n\\end{itemize}"
        );
        let off = LIST.find("one").unwrap();
        let it = item_at(LIST, off).unwrap();
        assert_eq!(
            apply(LIST, &backspace_item(LIST, &it, off).unwrap()),
            "|one\n\n\\begin{itemize}\n  \\item two\n\\end{itemize}"
        );
    }

    #[test]
    fn enter_closes_and_reopens_formatting() {
        let b = "Some \\textbf{bold \\emph{and} more} text";
        let off = 10 + b.find("more").unwrap() + 2;
        let e = enter_in_paragraph(b, 10, off);
        let mut s = format!("{}{b}", " ".repeat(10));
        s.replace_range(e.range.clone(), &e.text);
        assert_eq!(
            &s[10..],
            "Some \\textbf{bold \\emph{and} mo}\n\n\\textbf{re} text"
        );
        assert_eq!(
            e.caret,
            10 + "Some \\textbf{bold \\emph{and} mo}\n\n\\textbf{".len()
        );
        // in a heading the new paragraph follows the block
        let h = "\\section{Intro}";
        let e = enter_in_paragraph(h, 0, 10);
        assert_eq!((e.range, e.text.as_str(), e.caret), (15..15, "\n\n", 17));
        assert!(line_break(h, 0, 10).is_none());
        let e = enter_in_paragraph("\\textcolor{red}{ab}", 0, 17);
        assert_eq!(e.text, "}\n\n\\textcolor{red}{");
    }

    #[test]
    fn empty_formatting_is_dropped() {
        let s = "a \\textbf{} b";
        assert_eq!(empty_group(s, 10), Some(2..11));
        assert_eq!(empty_group("a {} b", 3), None);
        assert!(is_prose("\\textbf{Note:} text"));
        assert!(!is_prose("\\section{A}"));
        assert!(!is_prose("\\textbfx"));
    }
}
