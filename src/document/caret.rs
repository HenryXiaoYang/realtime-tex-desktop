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
    /// The block's source bytes.
    pub range: Range<usize>,
}

/// The caret map of a block's rows on one page: `gt` their glyphs, `src` the block's source,
/// which starts at byte `base` of the file.
pub fn char_map(gt: &GlyphText, src: &str, base: usize) -> CharMap {
    let boxes = glyphmap::char_boxes(gt);
    let fonts = glyphmap::char_fonts(gt);
    let srcs = glyphmap::char_sources(gt, src);
    let math = atom_regions(src);
    // a numbered heading starts with LaTeX's number, which could match a digit of its title
    let numbered = HEADINGS.iter().any(|h| {
        src.strip_prefix('\\')
            .and_then(|r| r.strip_prefix(h))
            .is_some_and(|r| r.starts_with(['{', '[']))
    });
    let number = if numbered {
        gt.text.iter().position(|c| *c == ' ').unwrap_or(0)
    } else {
        0
    };
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
        // math and TikZ are edited as source: their chars stand for the whole, like a macro's
        let src = srcs[i]
            .clone()
            .filter(|_| i >= number)
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
    CharMap {
        chars,
        stops,
        range: base..base + src.len(),
    }
}

impl CharMap {
    /// The stop for source offset `off` of `src`: the one at it, else the last one before it
    /// (text typed there, not typeset yet), but the next one when only markup comes before that
    /// and a line ends before the caret (`\\item |\\textbf{text}`, a new paragraph).
    pub fn stop_for(&self, off: usize, src: &str) -> Option<usize> {
        if let Some(i) = self.stops.iter().position(|s| s.off == off) {
            return Some(i);
        }
        let at = |pick: Option<(usize, &Stop)>| pick.map(|(i, _)| i);
        let prev = at(self
            .stops
            .iter()
            .enumerate()
            .filter(|(_, s)| s.off <= off)
            .max_by_key(|(_, s)| s.off));
        let next = at(self
            .stops
            .iter()
            .enumerate()
            .filter(|(_, s)| s.off > off)
            .min_by_key(|(_, s)| s.off));
        let breaks = |t: &str| t.contains("\\item") || t.contains("\n\n") || t.contains("\\\\");
        if let Some(n) = next
            && src
                .get(off..self.stops[n].off)
                .is_some_and(|t| !glyphmap::has_visible(t))
            && prev.is_none_or(|p| src.get(self.stops[p].off..off).is_some_and(breaks))
        {
            return Some(n);
        }
        prev
    }

    /// The stop nearest (`x`, `y`) sp: on the nearest line, then the nearest across.
    pub fn stop_near(&self, x: f64, y: f64) -> Option<usize> {
        nearest_stop(&self.stops, x, y)
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
        // at the block's edge, up to its edge: never past it
        let (lo, hi) = match (lo, hi) {
            (Some(a), Some(b)) if a < b => (a, b),
            (Some(a), None) => (a, self.range.end),
            (None, Some(b)) => (self.range.start, b),
            _ => return None,
        };
        let t = src.get(lo..hi)?;
        let mut start = lo + (t.len() - t.trim_start().len());
        let mut end = (start + t.trim().len()).max(start);
        // the closing braces of the groups it sits in (a heading's) are not its own
        while depth(&src[start..end]) < 0 && src[..end].ends_with('}') {
            end = src[start..end - 1].trim_end().len() + start;
        }
        while depth(&src[start..end]) > 0 && src[start..].starts_with('{') {
            start += 1;
        }
        // only a whole piece of TeX goes: no half group, environment or item
        let piece = &src[start..end];
        let whole = depth(piece) == 0
            && min_depth(piece) >= 0
            && !piece.contains("\\begin")
            && !piece.contains("\\end")
            && !piece.contains("\\item")
            && !piece.is_empty();
        whole.then_some(start..end)
    }
}

/// TeX still being typed, which would only make LaTeX choke (or, `\\` alone, print a space): a
/// command name at `at` (the caret, in bytes), or a brace left open.
pub fn unfinished(tex: &str, at: usize) -> bool {
    let at = (0..=at.min(tex.len()))
        .rev()
        .find(|&i| tex.is_char_boundary(i))
        .unwrap_or(0);
    let name = tex[..at].trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '@');
    let slashes = name.len() - name.trim_end_matches('\\').len();
    slashes % 2 == 1 || depth(tex) > 0
}

/// The brace depth `s` ends at (escaped braces aside).
fn depth(s: &str) -> i64 {
    brace_depths(s).last().copied().unwrap_or(0)
}

/// The lowest brace depth `s` reaches.
fn min_depth(s: &str) -> i64 {
    brace_depths(s).into_iter().min().unwrap_or(0)
}

fn brace_depths(s: &str) -> Vec<i64> {
    let b = s.as_bytes();
    let (mut d, mut out, mut i) = (0, vec![], 0);
    while i < b.len() {
        match b[i] {
            b'\\' => i += 1,
            b'{' => d += 1,
            b'}' => d -= 1,
            _ => {}
        }
        out.push(d);
        i += 1;
    }
    out
}

/// What the caret does not go into, as it is edited as source: inline math, TikZ pictures
/// (`\begin{tikzpicture}…`, `\tikz…;`), delimiters included.
pub fn atom_regions(src: &str) -> Vec<Range<usize>> {
    let mut out = math_regions(src);
    out.extend(picture_regions(src));
    out
}

/// TikZ and other pictures in `src`: `\begin{tikzpicture}…`, `\tikz…;`.
fn picture_regions(src: &str) -> Vec<Range<usize>> {
    let mut out = vec![];
    let toks = tokens(src);
    for (k, (a, _, t)) in toks.iter().enumerate() {
        if let Tok::Begin(n) = t
            && PICTURES.contains(&n.as_str())
            && let Some((_, b, _)) = toks[k..].iter().find(|(_, _, e)| *e == Tok::End(n.clone()))
        {
            out.push(*a..*b);
        }
    }
    // \tikz… up to its ; outside braces
    let mut from = 0;
    while let Some(i) = src[from..].find("\\tikz").map(|i| i + from) {
        let rest = &src[i + 5..];
        if rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
            from = i + 5;
            continue;
        }
        let mut depth = 0i64;
        let mut end = src.len();
        for (j, c) in rest.char_indices() {
            match c {
                '{' | '[' => depth += 1,
                '}' | ']' => depth -= 1,
                ';' if depth <= 0 => {
                    end = i + 5 + j + 1;
                    break;
                }
                _ => {}
            }
        }
        out.push(i..end);
        from = end;
    }
    out
}

const PICTURES: &[&str] = &[
    "tikzpicture",
    "pgfpicture",
    "picture",
    "circuitikz",
    "forest",
];

/// The stop of `stops` nearest (`x`, `y`) sp: on the nearest line, then the nearest across.
pub fn nearest_stop(stops: &[Stop], x: f64, y: f64) -> Option<usize> {
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
    stops
        .iter()
        .enumerate()
        .min_by(|a, b| dist(a.1).partial_cmp(&dist(b.1)).unwrap())
        .map(|(i, _)| i)
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
    pub fn marked(range: Range<usize>, text: &str) -> Self {
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
pub fn enter_in_item(src: &str, it: &Item, off: usize) -> Option<Edit> {
    let indent = &it.indent;
    if glyphmap::has_visible(&src[it.text.clone()]) {
        let off = off.clamp(it.text.start, it.text.end);
        return Some(match split_at(src, it.text.clone(), off)? {
            // before its text: a new empty item above, the caret staying with the text
            Split::Start => {
                let text = format!("{indent}\\item\n");
                Edit {
                    range: it.line..it.line,
                    caret: off + text.len(),
                    text,
                }
            }
            Split::End => {
                let at = it.text.end;
                Edit::marked(at..at, &format!("\n{indent}\\item {MARK}"))
            }
            Split::Mid(at, close, reopen) => {
                Edit::marked(at..at, &format!("{close}\n{indent}\\item {reopen}{MARK}"))
            }
        });
    }
    // split the list around the empty item: what is left of either half stays a list
    let (start, li, head, tail) = list_parts(src, it);
    let env = &it.env;
    let text = format!("{head}\n{li}\\end{{{env}}}\n\n{MARK}\n\n{li}\\begin{{{env}}}{tail}");
    Some(Edit::marked(start..it.list.end, &tidy(&text)))
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
pub fn group_command(src: &str, open: usize) -> (usize, String) {
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
    // a starred command: \section*{
    let name_end = if i > 0 && s[i - 1] == b'*' { i - 1 } else { i };
    let name_start = src[..name_end]
        .trim_end_matches(|c: char| c.is_ascii_alphabetic())
        .len();
    if name_start < name_end && name_start > 0 && s[name_start - 1] == b'\\' {
        (name_start - 1, src[name_start..name_end].to_string())
    } else {
        (open, String::new())
    }
}

/// Deleting the selection `lo..hi` of `src` (offsets of caret stops): everything in it goes
/// except the markup it only partly holds, which stays whole — the `}` of a group opened
/// before it, the `\\textbf{` of one closed after it, an environment's `\\begin` or `\\end` whose
/// partner is outside. Paragraphs and items it spans join. The caret stays where it began.
pub fn delete_range(src: &str, lo: usize, hi: usize) -> Edit {
    let (lo, hi) = (lo.min(hi), lo.max(hi).min(src.len()));
    let s = src.as_bytes();
    // what stays: byte ranges in lo..hi
    let mut keep: Vec<Range<usize>> = vec![];
    // groups and environments opened inside, not closed yet: where their opening markup is
    let mut open: Vec<(usize, usize, Option<String>)> = vec![];
    let mut i = lo;
    while i < hi {
        if s[i] == b'%' && (i == 0 || !is_cmd(s, i - 1)) {
            while i < hi && s[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if is_cmd(s, i) {
            let rest = &src[i..hi];
            let env = |p: &str| {
                rest.strip_prefix(p)
                    .and_then(|r| r.split_once('}'))
                    .map(|(n, _)| n.to_string())
            };
            if let Some(name) = env("\\begin{") {
                let mut j = i + "\\begin{".len() + name.len() + 1;
                // its arguments belong to its opening
                loop {
                    let k = match s.get(j) {
                        Some(b'[') => src[j..hi].find(']').map(|e| j + e + 1),
                        Some(b'{') => group_close(src, j).filter(|&e| e < hi).map(|e| e + 1),
                        _ => None,
                    };
                    match k {
                        Some(k) => j = k,
                        None => break,
                    }
                }
                open.push((i, j, Some(name)));
                i = j;
                continue;
            }
            if let Some(name) = env("\\end{") {
                let j = i + "\\end{".len() + name.len() + 1;
                match open
                    .iter()
                    .rposition(|o| o.2.as_deref() == Some(name.as_str()))
                {
                    // its \begin is inside too: the whole environment goes
                    Some(k) => open.truncate(k),
                    None => keep.push(i..j),
                }
                i = j;
                continue;
            }
            i += 2;
            while i < hi && (s[i].is_ascii_alphabetic() || s[i] == b'@') {
                i += 1;
            }
            continue;
        }
        match s[i] {
            b'{' => {
                let (start, _) = group_command(src, i);
                open.push((start.max(lo), i + 1, None));
            }
            b'}' => match open.iter().rposition(|o| o.2.is_none()) {
                Some(k) => open.truncate(k),
                // closes a group opened before the selection
                None => keep.push(i..i + 1),
            },
            _ => {}
        }
        i += 1;
    }
    keep.extend(open.into_iter().map(|(a, b, _)| a..b));
    keep.sort_by_key(|r| r.start);
    let text: String = keep.iter().map(|r| &src[r.clone()]).collect();
    Edit {
        range: lo..hi,
        // where the selection began: typing goes on in the formatting there, as in Word
        caret: lo,
        text,
    }
}

/// Whether `tex` stands on its own: its groups and environments all close in it.
pub fn balanced(tex: &str) -> bool {
    if depth(tex) != 0 || min_depth(tex) < 0 {
        return false;
    }
    let mut envs: Vec<String> = vec![];
    for (_, _, t) in tokens(tex) {
        match t {
            Tok::Begin(n) => envs.push(n),
            Tok::End(n) if envs.last() == Some(&n) => {
                envs.pop();
            }
            Tok::End(_) => return false,
            Tok::Item => {}
        }
    }
    envs.is_empty()
}

/// Headings: Enter inside one splits it in two, as it splits formatting.
const HEADINGS: &[&str] = &[
    "part",
    "chapter",
    "section",
    "subsection",
    "subsubsection",
    "paragraph",
    "subparagraph",
];

/// Environments whose lines are rows (`&`, `\\`): Enter waits for table editing there.
const TABLES: &[&str] = &[
    "tabular",
    "tabular*",
    "tabularx",
    "tabulary",
    "longtable",
    "array",
    "tabu",
];

/// The environments open at `off`, outermost first.
pub fn envs_at(src: &str, off: usize) -> Vec<String> {
    let mut open: Vec<String> = vec![];
    for (a, _, t) in tokens(src) {
        if a >= off {
            break;
        }
        match t {
            Tok::Begin(n) => open.push(n),
            Tok::End(n) => {
                if let Some(i) = open.iter().rposition(|o| *o == n) {
                    open.truncate(i);
                }
            }
            Tok::Item => {}
        }
    }
    open
}

/// Whether `off` is in a table's cells.
pub fn in_table(src: &str, off: usize) -> bool {
    envs_at(src, off)
        .iter()
        .any(|e| TABLES.contains(&e.as_str()))
}

/// Where a break at `off` in `text` (a paragraph's or an item's) falls.
#[derive(Debug, PartialEq)]
enum Split {
    /// No text before it, or none after.
    Start,
    End,
    /// Between text: at this offset, closing and reopening the groups open there.
    Mid(usize, String, String),
}

/// Where a break at `off` in `text` falls; None inside a group that cannot be split (a
/// command's argument other than formatting or a heading). A caret at the edge of a group
/// (`\textbf{word|} more`) breaks outside it rather than leaving it empty on one side.
fn split_at(src: &str, text: Range<usize>, off: usize) -> Option<Split> {
    if !shows_text(src, text.start..off) {
        return Some(Split::Start);
    }
    if !shows_text(src, off..text.end) {
        return Some(Split::End);
    }
    let mut off = off;
    let mut groups = open_groups(&src[text.clone()], text.start, off);
    while let Some(&(_, start, open)) = groups.last() {
        let close = group_close(src, open).unwrap_or(text.end);
        if !glyphmap::has_visible(&src[off..close]) {
            off = close + 1;
        } else if !glyphmap::has_visible(&src[open + 1..off]) {
            off = start;
        } else {
            break;
        }
        groups.pop();
    }
    // the new line starts at its text, not with the space after the break
    off = src[..text.end]
        .len()
        .min(off + src[off..].len() - src[off..].trim_start_matches([' ', '\t']).len());
    let split = |n: &str| FORMAT.contains(&n) || HEADINGS.contains(&n);
    if !groups.iter().all(|(n, ..)| split(n)) {
        return None;
    }
    let close = "}".repeat(groups.len());
    let reopen = groups.iter().map(|(_, s, o)| &src[*s..=*o]).collect();
    Some(Split::Mid(off, close, reopen))
}

/// A heading's title, inside its braces, when `block` is a heading (`\\section*[short]{…}`).
pub fn heading_title(block: &str, base: usize) -> Option<Range<usize>> {
    let rest = block.strip_prefix('\\')?;
    let name = HEADINGS.iter().find(|h| {
        rest.strip_prefix(**h)
            .is_some_and(|r| !r.starts_with(|c: char| c.is_ascii_alphabetic()))
    })?;
    let mut i = 1 + name.len();
    if block[i..].starts_with('*') {
        i += 1;
    }
    if block[i..].starts_with('[') {
        i += block[i..].find(']')? + 1;
    }
    if !block[i..].starts_with('{') {
        return None;
    }
    let close = group_close(block, i)?;
    Some(base + i + 1..base + close)
}

/// Whether `src[r]` prints text, a picture's labels aside (a TikZ flowchart beside the text
/// is not text the caret is after).
fn shows_text(src: &str, r: Range<usize>) -> bool {
    let mut t = String::new();
    let mut at = r.start;
    let mut pics = picture_regions(src);
    pics.sort_by_key(|p| p.start);
    for p in pics
        .into_iter()
        .filter(|p| p.end > r.start && p.start < r.end)
    {
        t.push_str(&src[at..p.start.max(at)]);
        at = p.end.min(r.end).max(at);
    }
    t.push_str(&src[at..r.end]);
    glyphmap::has_visible(&t)
}

/// Where the group opening at `open` closes.
pub fn group_close(src: &str, open: usize) -> Option<usize> {
    let s = src.as_bytes();
    let mut depth = 0;
    let mut i = open;
    while i < s.len() {
        match s[i] {
            b'\\' => i += 1,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Enter at `off` in a paragraph, `block` its range: before its text, a new paragraph above
/// it (the caret in it); after its text, one below; between, a paragraph break that closes the
/// formatting or heading open there and reopens it after. None where a break cannot go.
pub fn enter_in_paragraph(src: &str, block: Range<usize>, off: usize) -> Option<Edit> {
    // a format picked for what is typed next (its group still empty): the new paragraph has it,
    // as Word's does, and this one is left without the empty group
    if let Some((group, open, close)) = pending_format(src, block.clone(), off)
        && matches!(split_at(src, block.clone(), off), Some(Split::End))
    {
        let rest = &src[group.end..block.end];
        return Some(Edit::marked(
            group.start..block.end,
            &format!("{rest}\n\n{open}{MARK}{close}"),
        ));
    }
    Some(match split_at(src, block.clone(), off)? {
        Split::Start => Edit::marked(block.start..block.start, &format!("{MARK}\n\n")),
        Split::End => Edit::marked(block.end..block.end, &format!("\n\n{MARK}")),
        Split::Mid(at, close, reopen) => {
            Edit::marked(at..at, &format!("{close}\n\n{reopen}{MARK}"))
        }
    })
}

/// The format groups around `off` in `block` with nothing in them yet (`\\textcolor{red}{|}`,
/// nested ones too): their range, their opening and their closing.
fn pending_format(
    src: &str,
    block: Range<usize>,
    off: usize,
) -> Option<(Range<usize>, String, String)> {
    let groups = open_groups(&src[block.clone()], block.start, off);
    let (mut from, mut to, mut n) = (off, off, 0);
    for (name, start, open) in groups.iter().rev() {
        // empty: its `{` right before what is inside, its `}` right after
        if !FORMAT.contains(&name.as_str()) || open + 1 != from || !src[to..].starts_with('}') {
            break;
        }
        (from, to, n) = (*start, to + 1, n + 1);
    }
    (n > 0).then(|| {
        let inner_open = groups[groups.len() - 1].2;
        (from..to, src[from..=inner_open].to_string(), "}".repeat(n))
    })
}

/// Environments a block that is not a float may go inside: plain alignment, quotes, lists.
const BLOCKS_INSIDE: &[&str] = &[
    "document",
    "center",
    "flushleft",
    "flushright",
    "quote",
    "quotation",
    "itemize",
    "enumerate",
    "description",
];

/// Whether a block (`float`: a figure or table) can go in at `off`: not inside a minipage, a
/// table, a float, a box or an environment of the document's own; a float only at the top
/// (LaTeX takes one in a list or a center, but rtex's fast path for that paragraph does not).
fn block_fits(src: &str, off: usize, float: bool) -> bool {
    envs_at(src, off)
        .iter()
        .all(|e| e == "document" || !float && BLOCKS_INSIDE.contains(&e.as_str()))
}

/// Where a footnote or a display formula typed at `off` in `block` can go: there in running
/// text (formatting around it is fine), else after the block — out of a caption, a heading, a
/// table's cell, a float.
pub fn box_spot(src: &str, block: Range<usize>, off: usize) -> usize {
    let prose = heading_title(&src[block.clone()], block.start).is_none()
        && block_fits(src, off, false)
        && open_groups(&src[block.clone()], block.start, off)
            .iter()
            .all(|(n, ..)| FORMAT.contains(&n.as_str()));
    if prose { off } else { block.end }
}

/// `tex` (its caret at MARK, else after it) as a block of its own at `off` in `block`: before
/// the block with the caret at its start, after it at its end, in a heading, in an environment
/// it cannot go into or an argument that cannot be split; else splitting the paragraph (or
/// item) there.
pub fn insert_block(src: &str, block: Range<usize>, off: usize, tex: &str) -> Edit {
    let after = || Edit::marked(block.end..block.end, &format!("\n\n{}", mark_end(tex)));
    let float = tex.starts_with("\\begin{figure") || tex.starts_with("\\begin{table");
    if !block_fits(src, off, float) {
        return after();
    }
    let text = item_at(src, off)
        .filter(|it| off >= it.text.start)
        .map_or(block.clone(), |it| it.text);
    let heading = heading_title(&src[block.clone()], block.start).is_some();
    match split_at(src, text.clone(), off) {
        Some(Split::Start) if text == block => {
            Edit::marked(block.start..block.start, &format!("{}\n\n", mark_end(tex)))
        }
        Some(Split::Mid(at, close, reopen)) if !heading => {
            Edit::marked(at..at, &format!("{close}\n\n{}\n\n{reopen}", mark_end(tex)))
        }
        Some(Split::Start) => {
            Edit::marked(text.start..text.start, &format!("{}\n\n", mark_end(tex)))
        }
        Some(Split::End) if text != block => {
            Edit::marked(text.end..text.end, &format!("\n\n{}", mark_end(tex)))
        }
        _ => after(),
    }
}

/// `tex` with a MARK at its end when it has none.
fn mark_end(tex: &str) -> String {
    if tex.contains(MARK) {
        tex.to_string()
    } else {
        format!("{tex}{MARK}")
    }
}

/// Shift-Enter: a line break between text, in formatting but not in a heading or a command's
/// argument (where `\\` breaks the TeX).
pub fn line_break(src: &str, block: Range<usize>, off: usize) -> Option<Edit> {
    match split_at(src, block.clone(), off)? {
        Split::Mid(at, _, _) => {
            let groups = open_groups(&src[block.clone()], block.start, at);
            groups
                .iter()
                .all(|(n, ..)| FORMAT.contains(&n.as_str()))
                .then(|| Edit::marked(at..at, &format!("\\\\\n{MARK}")))
        }
        _ => None,
    }
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
        let e = enter_in_item(LIST, &it, off).unwrap();
        assert_eq!(
            apply(LIST, &e),
            "\\begin{itemize}\n  \\item one\n  \\item |\n  \\item two\n\\end{itemize}"
        );
        // an empty last item ends the list
        let src = "\\begin{itemize}\n  \\item one\n  \\item \n\\end{itemize}";
        let off = src.rfind("\\item").unwrap() + 6;
        let it = item_at(src, off).unwrap();
        assert_eq!(
            apply(src, &enter_in_item(src, &it, off).unwrap()),
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

    /// Enter at the `|` of `src` (a whole paragraph), as the source after it with the caret `|`.
    fn enter(src: &str) -> Option<String> {
        let off = src.find('|').unwrap();
        let s = src.replace('|', "");
        let e = enter_in_paragraph(&s, 0..s.len(), off)?;
        Some(apply(&s, &e))
    }

    #[test]
    fn enter_carries_a_format_picked_for_what_comes_next() {
        assert_eq!(
            enter("some words\\textcolor{red}{|}").unwrap(),
            "some words\n\n\\textcolor{red}{|}"
        );
        assert_eq!(
            enter("end \\textbf{\\textcolor{red}{|}}").unwrap(),
            "end \n\n\\textbf{\\textcolor{red}{|}}"
        );
        assert_eq!(
            enter("\\subsection{Title\\textcolor{red}{|}}").unwrap(),
            "\\subsection{Title}\n\n\\textcolor{red}{|}"
        );
    }

    #[test]
    fn enter_closes_and_reopens_formatting() {
        assert_eq!(
            enter("Some \\textbf{bold \\emph{and} mo|re} text").unwrap(),
            "Some \\textbf{bold \\emph{and} mo}\n\n\\textbf{|re} text"
        );
        assert_eq!(
            enter("\\textcolor{red}{a|b}").unwrap(),
            "\\textcolor{red}{a}\n\n\\textcolor{red}{|b}"
        );
        // at a group's edge the break goes outside it, leaving no empty \textbf{}
        assert_eq!(
            enter("\\textbf{Note:|} more").unwrap(),
            "\\textbf{Note:} \n\n|more"
        );
        assert_eq!(
            enter("some \\textbf{|bold} text").unwrap(),
            "some \n\n|\\textbf{bold} text"
        );
    }

    #[test]
    fn a_block_goes_in_at_the_caret() {
        let ins = |src: &str| {
            let off = src.find('|').unwrap();
            let s = src.replace('|', "");
            apply(&s, &insert_block(&s, 0..s.len(), off, "\\newpage"))
        };
        assert_eq!(ins("|Text."), "\\newpage|\n\nText.");
        assert_eq!(ins("Text.|"), "Text.\n\n\\newpage|");
        assert_eq!(
            ins("Some \\textbf{bold |text} here"),
            "Some \\textbf{bold }\n\n\\newpage|\n\n\\textbf{text} here"
        );
        // a heading is not split: after it
        assert_eq!(
            ins("\\section{A |title}"),
            "\\section{A title}\n\n\\newpage|"
        );
        // in an item: a paragraph of the item
        let s = "\\begin{itemize}\n\\item one two\n\\end{itemize}";
        let off = s.find("two").unwrap();
        let e = insert_block(s, 0..s.len(), off, "X");
        let mut t = s.to_string();
        t.replace_range(e.range, &e.text);
        assert_eq!(
            t,
            "\\begin{itemize}\n\\item one \n\nX\n\ntwo\n\\end{itemize}"
        );
        // in a minipage, a table or an environment of the document's own: after the block
        for s in [
            "\\begin{minipage}{3cm}\none |two\n\\end{minipage}",
            "\\begin{tabular}{ll}\na |b & c \\\\\n\\end{tabular}",
            "\\begin{definition}{x}\none |two\n\\end{definition}",
        ] {
            let got = ins(s);
            assert!(got.ends_with("\n\n\\newpage|"), "{got}");
        }
        // a float leaves a list; a footnote leaves a caption, not formatting
        let l = "\\begin{itemize}\n\\item one two\n\\end{itemize}";
        let e = insert_block(
            l,
            0..l.len(),
            l.find("two").unwrap(),
            "\\begin{figure}x\\end{figure}",
        );
        assert_eq!(e.range, l.len()..l.len());
        let f = "\\begin{figure}\n\\caption{A pic}\n\\end{figure}";
        assert_eq!(box_spot(f, 0..f.len(), f.find("pic").unwrap()), f.len());
        let p = "Some \\textbf{bold} text";
        let at = p.find("old").unwrap();
        assert_eq!(box_spot(p, 0..p.len(), at), at);
    }

    #[test]
    fn enter_at_a_paragraphs_edges_adds_one_there() {
        // before the text (a heading's too): a new paragraph above, the caret in it
        assert_eq!(
            enter("\\subsection*{|Which one?}").unwrap(),
            "|\n\n\\subsection*{Which one?}"
        );
        assert_eq!(enter("|Plain text.").unwrap(), "|\n\nPlain text.");
        // after it: one below
        assert_eq!(enter("\\section{Intro|}").unwrap(), "\\section{Intro}\n\n|");
        assert_eq!(enter("Text \\emph{end|}").unwrap(), "Text \\emph{end}\n\n|");
        // in a heading: two headings
        assert_eq!(
            enter("\\subsection*{Which |one?}").unwrap(),
            "\\subsection*{Which }\n\n\\subsection*{|one?}"
        );
        // a command's argument that cannot split: nothing
        assert_eq!(enter("\\fbox{in a |box} after"), None);
        // a line break only between text, and not in a heading
        let lb = |src: &str| {
            let off = src.find('|').unwrap();
            let s = src.replace('|', "");
            line_break(&s, 0..s.len(), off).map(|e| apply(&s, &e))
        };
        assert_eq!(lb("one |two").unwrap(), "one \\\\\n|two");
        assert_eq!(lb("|two"), None);
        assert_eq!(lb("\\section{A |B}"), None);
    }

    #[test]
    fn enter_in_an_item_adds_one_above_below_or_splits() {
        let src = "\\begin{itemize}\n  \\item \\textbf{Rapid dev} text\n\\end{itemize}";
        let at = |s: &str| src.find(s).unwrap();
        let go = |off: usize| {
            let it = item_at(src, off).unwrap();
            apply(src, &enter_in_item(src, &it, off).unwrap())
        };
        // before its text: an empty item above, the caret staying with the text
        assert_eq!(
            go(at("Rapid")),
            "\\begin{itemize}\n  \\item\n  \\item \\textbf{|Rapid dev} text\n\\end{itemize}"
        );
        // inside bold: the bold closes and reopens around the new item
        assert_eq!(
            go(at("dev")),
            "\\begin{itemize}\n  \\item \\textbf{Rapid }\n  \\item \\textbf{|dev} text\n\\end{itemize}"
        );
        assert_eq!(
            go(at(" text") + 5),
            "\\begin{itemize}\n  \\item \\textbf{Rapid dev} text\n  \\item |\n\\end{itemize}"
        );
    }

    #[test]
    fn a_break_skips_the_space_after_it() {
        let src = "\\begin{itemize}\n  \\item \\textbf{Speed:} fast\n\\end{itemize}";
        let off = src.find(':').unwrap() + 2;
        let it = item_at(src, off).unwrap();
        assert_eq!(
            apply(src, &enter_in_item(src, &it, off).unwrap()),
            "\\begin{itemize}\n  \\item \\textbf{Speed:} \n  \\item |fast\n\\end{itemize}"
        );
    }

    #[test]
    fn tikz_is_an_atom() {
        let s = "a \\tikz\\node[x]{Start}; b \\begin{tikzpicture}\\draw (0,0)--(1,1);\\end{tikzpicture} c";
        let r: Vec<&str> = atom_regions(s).into_iter().map(|r| &s[r]).collect();
        assert_eq!(
            r,
            vec![
                "\\begin{tikzpicture}\\draw (0,0)--(1,1);\\end{tikzpicture}",
                "\\tikz\\node[x]{Start};"
            ]
        );
    }

    #[test]
    fn a_picture_beside_text_is_not_text_before_it() {
        let s = "\\begin{minipage}{.5\\textwidth}\\begin{tikzpicture}\\node{Start};\\end{tikzpicture}\\end{minipage}\nlimit = 7";
        let e = enter_in_paragraph(s, 0..s.len(), s.find("limit").unwrap()).unwrap();
        assert_eq!(
            (e.range.start, e.caret),
            (0, 0),
            "a new paragraph before the block"
        );
    }

    #[test]
    fn half_typed_tex_waits() {
        let at_end = |s: &str| unfinished(s, s.len());
        assert!(at_end("\\"));
        assert!(at_end("text \\tit"));
        assert!(at_end("\\title{AA"));
        assert!(!at_end("\\title{AAA}"));
        assert!(!at_end("\\maketitle "));
        assert!(!at_end("a line\\\\"), "a line break is no command");
        assert!(!at_end("50\\% off"));
        // a command name being typed in the middle of the text
        assert!(unfinished("see \\re and more", 7));
    }

    #[test]
    fn finds_a_headings_title() {
        let s = "\\subsection*[short]{Which one?} more";
        assert_eq!(heading_title(s, 10), Some(10 + 20..10 + 30));
        assert_eq!(heading_title("\\subsection{}", 0), Some(12..12));
        assert_eq!(heading_title("\\sectionmark{x}", 0), None);
        assert_eq!(heading_title("plain", 0), None);
    }

    /// Delete between the two `|` of `src`, as the source after with the caret `|`.
    fn del(src: &str) -> String {
        let lo = src.find('|').unwrap();
        let hi = src.rfind('|').unwrap() - 1;
        let s = src.replacen('|', "", 2);
        apply(&s, &delete_range(&s, lo, hi))
    }

    #[test]
    fn deleting_a_selection_keeps_markup_it_only_partly_holds() {
        assert_eq!(del("plain te|xt and mo|re"), "plain te|re");
        // all of a command inside goes; a group's end outside stays
        assert_eq!(del("a |\\textbf{b} c|d"), "a |d");
        assert_eq!(del("\\textbf{bo|ld} te|xt"), "\\textbf{bo|}xt");
        assert_eq!(del("te|xt \\textbf{bo|ld}"), "te|\\textbf{ld}");
        // paragraphs and items join
        assert_eq!(del("one tw|o\n\nthree fo|ur"), "one tw|ur");
        assert_eq!(
            del("\\begin{itemize}\n  \\item on|e\n  \\item tw|o\n\\end{itemize}"),
            "\\begin{itemize}\n  \\item on|o\n\\end{itemize}"
        );
        // out of a list into the text after: the list's end stays
        assert_eq!(
            del("\\begin{itemize}\n  \\item on|e\n\\end{itemize}\n\nAft|er"),
            "\\begin{itemize}\n  \\item on|\\end{itemize}er"
        );
        // into a list from before: its beginning stays, with its options
        assert_eq!(
            del("Bef|ore\n\\begin{itemize}[nosep]\n  \\item on|e\n\\end{itemize}"),
            "Bef|\\begin{itemize}[nosep]e\n\\end{itemize}"
        );
        // a whole environment inside goes
        assert_eq!(del("a |b \\begin{center}x\\end{center} c| d"), "a | d");
        assert!(balanced("\\textbf{a} \\begin{center}b\\end{center}"));
        assert!(!balanced("bo} x \\textbf{y"));
    }

    #[test]
    fn tables_are_known() {
        let s = "a \\begin{tabular}{ll} x & y \\end{tabular} b";
        assert!(in_table(s, s.find('x').unwrap()));
        assert!(!in_table(s, 1));
        assert!(!in_table(s, s.len() - 1));
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
