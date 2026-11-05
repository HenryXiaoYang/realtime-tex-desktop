// Character-exact mapping from page glyphs to the source text of their block; a port of
// realtime-tex-vsc-plugin src/glyphMap.ts and webview/textLayer.ts. TeX keeps no column
// positions, so the glyph text is aligned with the source text with the markup that prints
// nothing taken out (longest common subsequence). Glyphs a macro printed (\ref, \cite) match
// nothing and map to the source just after the matched text before them.
use crate::engine::FontDesc;
use crate::render::Row;
use std::collections::HashMap;
use std::ops::Range;

/// A glyph that has text, in sp.
struct Glyph {
    x: f64,
    y: f64,
    adv: f64,
    size: f64,
    font: Option<FontDesc>,
    /// Where its text starts in `GlyphText::text`, in chars.
    start: usize,
    len: usize,
}

pub struct GlyphText {
    pub text: Vec<char>,
    glyphs: Vec<Glyph>,
}

// OT1/T1 slots of TFM fonts that are not their ASCII character (ligatures, quotes, dashes)
fn tfm_char(c: u32) -> Option<&'static str> {
    Some(match c {
        11 | 27 => "ff",
        12 | 28 => "fi",
        13 | 29 => "fl",
        14 | 30 => "ffi",
        15 | 31 => "ffl",
        16 | 92 => "“",
        17 | 34 => "”",
        21 | 123 => "–",
        22 | 124 => "—",
        96 => "‘",
        39 => "’",
        _ => return None,
    })
}

/// The text of a glyph: TFM fonts (no glyph index) number their slots by encoding, OpenType
/// fonts by Unicode; private-use codes give nothing.
fn glyph_text(c: u32, has_index: bool) -> String {
    if !has_index {
        return tfm_char(c).map(String::from).unwrap_or_else(|| {
            if (33..127).contains(&c) {
                char::from(c as u8).to_string()
            } else {
                String::new()
            }
        });
    }
    if c <= 32 || (0xe000..=0xf8ff).contains(&c) || c >= 0xf0000 {
        return String::new();
    }
    char::from_u32(c)
        .map(|ch| match ch {
            'ﬀ' => "ff".into(),
            'ﬁ' => "fi".into(),
            'ﬂ' => "fl".into(),
            'ﬃ' => "ffi".into(),
            'ﬄ' => "ffl".into(),
            _ => ch.to_string(),
        })
        .unwrap_or_default()
}

/// The font most of the glyphs of `rows` are set in: what their source is shown in.
pub fn main_font(rows: &[&Row]) -> Option<FontDesc> {
    let mut count: HashMap<String, (usize, FontDesc)> = HashMap::new();
    for r in rows {
        for it in &r.dl.lines[r.line].items {
            let Some(a) = it.as_array().filter(|a| a.len() >= 7 && a[0] == "g") else {
                continue;
            };
            let Some(id) = a[1].as_i64().map(|f| f.to_string()) else {
                continue;
            };
            if let Some(d) = r.dl.fonts.get(&id) {
                count.entry(id).or_insert_with(|| (0, d.clone())).0 += 1;
            }
        }
    }
    count.into_values().max_by_key(|(n, _)| *n).map(|(_, d)| d)
}

/// The text of `rows`: glyphs in order, a space where rows change or a gap is wider than a
/// sixth of the font size.
pub fn glyph_text_of(rows: &[&Row]) -> GlyphText {
    let mut gt = GlyphText {
        text: vec![],
        glyphs: vec![],
    };
    for (ri, r) in rows.iter().enumerate() {
        let mut prev: Option<(f64, f64)> = None; // y, end x
        let line = &r.dl.lines[r.line];
        for it in &line.items {
            let Some(a) = it.as_array().filter(|a| a.len() >= 7 && a[0] == "g") else {
                continue;
            };
            let text = glyph_text(a[2].as_u64().unwrap_or(0) as u32, !a[3].is_null());
            if text.is_empty() {
                continue;
            }
            let font = a[1]
                .as_i64()
                .and_then(|f| r.dl.fonts.get(&f.to_string()))
                .cloned();
            let size = font.as_ref().and_then(|d| d.size).unwrap_or(655360.0);
            let (x, y, adv) = (
                a[4].as_f64().unwrap_or(0.0) + r.dx,
                a[5].as_f64().unwrap_or(0.0) + r.dy,
                a[6].as_f64().unwrap_or(0.0),
            );
            // a list's label (bullet, number) hangs left of its line: LaTeX's, not the text's
            if x + adv <= line.x + r.dx + 32768.0 {
                continue;
            }
            let gap = match prev {
                None => ri > 0 && !gt.text.is_empty(),
                Some((py, end)) => {
                    (y - py).abs() > size * 0.3 || x - end > size / 6.0 || x < end - size
                }
            };
            if gap && !gt.text.is_empty() {
                gt.text.push(' ');
            }
            let start = gt.text.len();
            gt.text.extend(text.chars());
            gt.glyphs.push(Glyph {
                x,
                y,
                adv,
                size,
                font,
                start,
                len: gt.text.len() - start,
            });
            prev = Some((y, x + adv));
        }
    }
    gt
}

/// The character under a click at (`x`, `y`) sp: the glyph whose box holds it or is nearest,
/// and whether the click is on its right half (the caret goes after it).
pub fn char_at(gt: &GlyphText, x: f64, y: f64) -> Option<(usize, bool)> {
    // by each glyph's own baseline; the nearest across among those nearest the baseline
    let mut best: Option<(&Glyph, f64, f64)> = None;
    for g in &gt.glyphs {
        let (t, b) = (g.y - 0.75 * g.size, g.y + 0.25 * g.size);
        let v = if y < t {
            t - y
        } else if y > b {
            y - b
        } else {
            0.0
        };
        let dx = if x < g.x {
            g.x - x
        } else if x > g.x + g.adv {
            x - g.x - g.adv
        } else {
            0.0
        };
        if best.is_none_or(|(_, bv, bx)| v < bv || (v == bv && dx < bx)) {
            best = Some((g, v, dx));
        }
    }
    let (g, ..) = best?;
    // a ligature's glyph holds several characters: split its box evenly between them
    let f = if g.adv > 0.0 {
        ((x - g.x) / g.adv).clamp(0.0, 0.999) * g.len as f64
    } else {
        0.0
    };
    Some((g.start + f as usize, f.fract() >= 0.5))
}

/// Each char of `gt.text`: its box (x, baseline y, width, font size) in sp, None for a space
/// TeX drew as glue. A ligature's box is split evenly between its chars.
pub fn char_boxes(gt: &GlyphText) -> Vec<Option<[f64; 4]>> {
    let mut out = vec![None; gt.text.len()];
    for g in &gt.glyphs {
        let w = g.adv / g.len.max(1) as f64;
        for k in 0..g.len {
            out[g.start + k] = Some([g.x + w * k as f64, g.y, w, g.size]);
        }
    }
    out
}

/// Each char of `gt.text`: the font its glyph is set in (None for a space TeX set as glue).
pub fn char_fonts(gt: &GlyphText) -> Vec<Option<FontDesc>> {
    let mut out = vec![None; gt.text.len()];
    for g in &gt.glyphs {
        for slot in &mut out[g.start..g.start + g.len] {
            *slot = g.font.clone();
        }
    }
    out
}

/// Each char of `gt.text`: the bytes of `src` it shows, None for text a macro printed.
pub fn char_sources(gt: &GlyphText, src: &str) -> Vec<Option<Range<usize>>> {
    let vis = visible_source(src);
    let (at, ..) = char_map(gt, &vis, gt.text.len() / 2);
    at.iter()
        .map(|&v| usize::try_from(v).ok().map(|v| vis.start[v]..vis.end[v]))
        .collect()
}

/// Whether `src` prints any text (markup and spaces do not).
pub fn has_visible(src: &str) -> bool {
    visible_source(src).text.iter().any(|c| *c != ' ')
}

/// Source text with the markup that prints nothing removed; `start`/`end` give each character's
/// byte range in the source (a `--` is one character there, like the dash it prints).
struct Visible {
    pub text: Vec<char>,
    start: Vec<usize>,
    end: Vec<usize>,
}

// commands whose first argument is not printed (a label, a key, a file, an environment name)
const HIDDEN_ARG: &[&str] = &[
    "label",
    "ref",
    "eqref",
    "pageref",
    "autoref",
    "nameref",
    "cref",
    "Cref",
    "cite",
    "citep",
    "citet",
    "parencite",
    "textcite",
    "autocite",
    "footcite",
    "nocite",
    "begin",
    "end",
    "includegraphics",
    "input",
    "include",
    "hspace",
    "vspace",
    "color",
    "textcolor",
    "href",
    "index",
    "hyperref",
    "cline",
    "multicolumn",
    "multirow",
    "setlength",
    "addtolength",
];

/// Environments whose arguments print nothing.
const SILENT_ARGS: &[&str] = &[
    "minipage",
    "tabular",
    "tabular*",
    "tabularx",
    "tabulary",
    "array",
    "longtable",
    "multicols",
    "wrapfigure",
    "figure",
    "table",
    "figure*",
    "table*",
    "center",
    "itemize",
    "enumerate",
    "description",
    "tikzpicture",
];

fn visible_source(src: &str) -> Visible {
    let s: Vec<(usize, char)> = src.char_indices().collect();
    let at = |k: usize| s.get(k).map_or('\0', |c| c.1);
    let pos = |k: usize| s.get(k).map_or(src.len(), |c| c.0);
    let mut v = Visible {
        text: vec![],
        start: vec![],
        end: vec![],
    };
    let emit = |v: &mut Visible, ch: char, a: usize, b: usize| {
        v.text.push(ch);
        v.start.push(pos(a));
        v.end.push(pos(b));
    };
    let letter = |c: char| c.is_ascii_alphabetic() || c == '@';
    // index after the balanced group opening at `i`, or `i` when none opens there
    let group = |i: usize, open: char, close: char| -> usize {
        if at(i) != open {
            return i;
        }
        let mut depth = 0;
        let mut k = i;
        while k < s.len() {
            match at(k) {
                '\\' => k += 1,
                c if c == open => depth += 1,
                c if c == close => {
                    depth -= 1;
                    if depth == 0 {
                        return k + 1;
                    }
                }
                _ => {}
            }
            k += 1;
        }
        s.len()
    };
    let mut i = 0;
    while i < s.len() {
        let c = at(i);
        let space = |v: &Visible| v.text.last().is_some_and(|&l| l != ' ');
        match c {
            '%' => {
                while i < s.len() && at(i) != '\n' {
                    i += 1;
                }
            }
            '\\' => {
                let n = at(i + 1);
                if letter(n) {
                    let mut k = i + 1;
                    while k < s.len() && letter(at(k)) {
                        k += 1;
                    }
                    let name: String = s[i + 1..k].iter().map(|c| c.1).collect();
                    if at(k) == '*' {
                        k += 1;
                    }
                    while at(k) == ' ' || at(k) == '\t' {
                        k += 1;
                    }
                    if at(k) == '\n' {
                        k += 1;
                    }
                    if HIDDEN_ARG.contains(&name.as_str()) {
                        loop {
                            let j = group(k, '[', ']');
                            if j == k {
                                break;
                            }
                            k = j;
                        }
                        let name_at = k;
                        k = group(k, '{', '}');
                        // the arguments of an environment that prints none of them (a
                        // minipage's width, a table's columns)
                        let env: String = s[name_at.min(k)..k]
                            .iter()
                            .map(|c| c.1)
                            .filter(|c| *c != '{' && *c != '}')
                            .collect();
                        if name == "begin" && SILENT_ARGS.contains(&env.as_str()) {
                            loop {
                                let j = group(group(k, '[', ']'), '{', '}');
                                if j == k {
                                    break;
                                }
                                k = j;
                            }
                        }
                        // a table cell's span and column spec: \multicolumn{2}{c}{text}
                        if name == "multicolumn" || name == "multirow" {
                            k = group(k, '{', '}');
                        }
                    }
                    i = k;
                } else {
                    if "&%$#_{}".contains(n) && n != '\0' {
                        emit(&mut v, n, i, i + 2);
                    } else if (n == ' ' || n == '\\') && space(&v) {
                        emit(&mut v, ' ', i, i + 2);
                    }
                    i += 2;
                }
            }
            '{' | '}' | '$' | '&' | '^' | '_' | '#' => i += 1,
            ' ' | '\t' | '\n' | '\r' | '~' => {
                if space(&v) {
                    emit(&mut v, ' ', i, i + 1);
                }
                i += 1;
            }
            '`' | '\'' if at(i + 1) == c => {
                emit(&mut v, '"', i, i + 2);
                i += 2;
            }
            '`' => {
                emit(&mut v, '\'', i, i + 1);
                i += 1;
            }
            '-' => {
                let n = if at(i + 1) == '-' {
                    if at(i + 2) == '-' { 3 } else { 2 }
                } else {
                    1
                };
                emit(&mut v, '-', i, i + n);
                i += n;
            }
            _ => {
                emit(&mut v, c, i, i + 1);
                i += 1;
            }
        }
    }
    v
}

/// Glyph characters as the source spells them: curly quotes and dashes fold to ASCII.
fn fold(c: char) -> char {
    match c {
        '“' | '”' | '„' => '"',
        '‘' | '’' | '`' => '\'',
        '–' | '—' | '‐' | '−' => '-',
        '\u{a0}' => ' ',
        _ => c,
    }
}

/// Cells of the alignment table beyond which a narrower window is aligned instead.
const MAX_CELLS: usize = 8_000_000;

/// For each char of `a[a0..a1]`, the index of the char of `b[b0..b1]` it matches (LCS), or -1.
fn align(a: &[char], b: &[char], (a0, a1): (usize, usize), (b0, b1): (usize, usize)) -> Vec<i64> {
    let mut out = vec![-1; a.len()];
    let (n, m) = (a1 - a0, b1 - b0);
    if n == 0 || m == 0 {
        return out;
    }
    let w = m + 1;
    let mut dp = vec![0u16; (n + 1) * w];
    for i in 1..=n {
        let ca = fold(a[a0 + i - 1]);
        for j in 1..=m {
            dp[i * w + j] = if ca == b[b0 + j - 1] {
                dp[(i - 1) * w + j - 1] + 1
            } else {
                dp[(i - 1) * w + j].max(dp[i * w + j - 1])
            };
        }
    }
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        if fold(a[a0 + i - 1]) == b[b0 + j - 1] && dp[i * w + j] == dp[(i - 1) * w + j - 1] + 1 {
            out[a0 + i - 1] = (b0 + j - 1) as i64;
            i -= 1;
            j -= 1;
        } else if dp[(i - 1) * w + j] >= dp[i * w + j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    out
}

/// The visible-source char each char of `gt.text` shows (-1 for none), for the stretch
/// `[a0, a1)` around `around` (the whole text unless the block is long).
fn char_map(gt: &GlyphText, vis: &Visible, around: usize) -> (Vec<i64>, usize, usize) {
    let (n, m) = (gt.text.len(), vis.text.len());
    let (mut a0, mut a1, mut b0, mut b1) = (0, n, 0, m);
    if n * m > MAX_CELLS {
        // a long block: a few hundred chars around `around`, with the matching stretch of source
        a0 = around.saturating_sub(300);
        a1 = (around + 300).min(n);
        let width = m.min(MAX_CELLS / (a1 - a0).max(1));
        let mid = (around as f64 / n.max(1) as f64 * m as f64).round() as usize;
        b0 = mid.saturating_sub(width / 2).min(m - width);
        b1 = b0 + width;
    }
    let matched = align(&gt.text, &vis.text, (a0, a1), (b0, b1));
    // ties can put a letter on the same letter in nearby markup (an option, a key): each word as
    // a whole settles them, at its occurrence in the source nearest where the alignment points
    let mut at = matched.clone();
    let mut ws = a0;
    while ws < a1 {
        let mut we = ws;
        while we < a1 && gt.text[we] != ' ' {
            we += 1;
        }
        let word: Vec<char> = gt.text[ws..we].iter().map(|&c| fold(c)).collect();
        if !word.is_empty() && word.len() <= m {
            let near = nearest_match(&matched, ws, a0, a1);
            let mut occ: Option<usize> = None;
            for k in (0..=m - word.len())
                .filter(|&k| vis.text[k..k + word.len()] == word[..])
                .take(256)
            {
                if occ.is_none()
                    || near
                        .is_some_and(|nr| (k as i64 - nr).abs() < (occ.unwrap() as i64 - nr).abs())
                {
                    occ = Some(k);
                }
            }
            if let Some(o) = occ {
                for (k, slot) in at[ws..we].iter_mut().enumerate() {
                    *slot = (o + k) as i64;
                }
            }
        }
        ws = we + 1;
    }
    (at, a0, a1)
}

/// The source index the alignment gives the matched char nearest `index`, shifted to `index`.
fn nearest_match(m: &[i64], index: usize, a0: usize, a1: usize) -> Option<i64> {
    if m[index] >= 0 {
        return Some(m[index]);
    }
    for d in 1.. {
        if index < a0 + d && index + d >= a1 {
            return None;
        }
        if index >= a0 + d && m[index - d] >= 0 {
            return Some(m[index - d] + d as i64);
        }
        if index + d < a1 && m[index + d] >= 0 {
            return Some(m[index + d] - d as i64);
        }
    }
    None
}

/// Byte offset in `src` (the block's source) of char `index` of `gt.text`, or just after it
/// when `after`. None when nothing around it matches.
pub fn source_offset(gt: &GlyphText, index: usize, after: bool, src: &str) -> Option<usize> {
    let vis = visible_source(src);
    let (at, a0, a1) = char_map(gt, &vis, index);
    let hit = |k: usize| usize::try_from(at[k]).ok();
    if let Some(v) = hit(index) {
        return Some(if after { vis.end[v] } else { vis.start[v] });
    }
    // printed by a macro: just after the matched text before it, else before the text after it
    (a0..index)
        .rev()
        .find_map(|k| hit(k).map(|v| vis.end[v]))
        .or_else(|| (index + 1..a1).find_map(|k| hit(k).map(|v| vis.start[v])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{DisplayList, Line};
    use std::sync::Arc;

    const PT: f64 = 65536.0;

    /// A line of `text` at baseline `y` pt: a space is a 3.3pt gap, every glyph 5pt wide.
    fn row(text: &str, y: f64) -> Row {
        let mut items = vec![];
        let mut x = 0.0;
        for ch in text.chars() {
            if ch == ' ' {
                x += 3.3 * PT;
                continue;
            }
            items.push(serde_json::json!([
                "g",
                1,
                ch as u32,
                1,
                x,
                y * PT,
                5.0 * PT,
                0
            ]));
            x += 5.0 * PT;
        }
        let dl: DisplayList = serde_json::from_value(serde_json::json!({"fonts": {"1": {"id": 1, "size": 10.0 * PT}}, "lines": [], "other": []})).unwrap();
        let line = Line {
            unit: None,
            x: 0.0,
            y: y * PT,
            w: x,
            h: 7.0 * PT,
            d: 2.0 * PT,
            gsign: 1,
            items,
        };
        Row {
            dl: Arc::new(DisplayList {
                lines: vec![line],
                ..dl
            }),
            line: 0,
            dx: 0.0,
            dy: 0.0,
        }
    }

    fn text(gt: &GlyphText) -> String {
        gt.text.iter().collect()
    }

    #[test]
    fn main_font_is_the_body_font() {
        let r = row("ab cd", 10.0);
        assert_eq!(main_font(&[&r]).and_then(|f| f.size), Some(10.0 * PT));
        assert!(main_font(&[]).is_none());
    }

    #[test]
    fn environment_arguments_that_print_nothing_are_markup() {
        let v = visible_source(
            "\\begin{minipage}[t]{0.52\\textwidth}x \\begin{tabular}{@{}cc@{}}a & b\\end{tabular}",
        );
        assert_eq!(v.text.iter().collect::<String>(), "x a b");
        // a theorem-like environment's title prints
        let v = visible_source("\\begin{definition}{Flowchart} text");
        assert_eq!(v.text.iter().collect::<String>(), "Flowchart text");
    }

    #[test]
    fn visible_source_drops_markup() {
        let v = visible_source(
            "Hello \\textbf{world} see~\\ref{sec:x} % note\nnext \\& \\cite[p.~2]{k} ``q'' a--b",
        );
        assert_eq!(
            v.text.iter().collect::<String>(),
            "Hello world see next & \"q\" a-b"
        );
        let dash = v.text.iter().position(|&c| c == '-').unwrap();
        assert_eq!(v.end[dash] - v.start[dash], 2);
    }

    #[test]
    fn click_lands_on_the_source_character() {
        let src = "Hello \\textbf{world}\nsee  \\emph{next} line";
        let (a, b) = (row("Hello world", 100.0), row("see next line", 112.0));
        let gt = glyph_text_of(&[&a, &b]);
        assert_eq!(text(&gt), "Hello world see next line");
        let idx = |s: &str| text(&gt).find(s).unwrap();
        assert_eq!(source_offset(&gt, idx("w"), false, src), src.find("world"));
        assert_eq!(
            source_offset(&gt, idx("d"), true, src),
            Some(src.find("world").unwrap() + 5)
        );
        assert_eq!(
            source_offset(&gt, idx("next") + 1, false, src),
            Some(src.find("next").unwrap() + 1)
        );
        // a click inside the second glyph of "world"
        let (i, _) = char_at(&gt, 5.0 * 5.0 * PT + 3.3 * PT + 7.0 * PT, 98.0 * PT).unwrap();
        assert_eq!(gt.text[i], 'o');
    }

    #[test]
    fn the_caret_stands_beside_text_not_inside_macros_or_math() {
        use crate::document::caret::{CharMap, char_map};
        let src = "see $x$ and \\ref{s} more";
        let r = row("see x and 3 more", 100.0);
        let m = char_map(&glyph_text_of(&[&r]), src, 0);
        let offs: Vec<usize> = m.stops.iter().map(|s| s.off).collect();
        assert!(offs.contains(&src.find("and").unwrap()));
        assert!(offs.contains(&src.find("more").unwrap()));
        // nowhere inside the formula
        let x = src.find('x').unwrap();
        assert!(!offs.contains(&x) && !offs.contains(&(x + 1)));
        let stop = |m: &CharMap, off: usize| {
            m.stops
                .iter()
                .position(|s| s.off == off)
                .unwrap_or_else(|| {
                    panic!(
                        "no stop at {off}: {:?}",
                        m.stops.iter().map(|s| s.off).collect::<Vec<_>>()
                    )
                })
        };
        // a space goes as the source's whitespace beside the caret; a letter as itself
        let and = src.find("and").unwrap() + 3;
        assert_eq!(m.deletion(stop(&m, and), false, src), Some(and..and + 1));
        let more = src.find("more").unwrap();
        assert_eq!(m.deletion(stop(&m, more), true, src), Some(more - 1..more));
        assert_eq!(m.deletion(stop(&m, 3), true, src), Some(2..3));
        // a reference goes whole, from either side
        let src = "a\\ref{s}b";
        let r = row("a3b", 100.0);
        let m = char_map(&glyph_text_of(&[&r]), src, 0);
        assert_eq!(m.deletion(stop(&m, 1), false, src), Some(1..8));
        assert_eq!(m.deletion(stop(&m, 8), true, src), Some(1..8));
        // a click lands on the nearest stop
        let i = m.stop_near(0.0, 98.0 * PT).unwrap();
        assert_eq!(m.stops[i].off, 0);
        // at a block's edge a macro's text goes only as far as the block, and only whole
        let src = "\\section*{Title \\hl}";
        let r = row("Title [HL]", 100.0);
        let m = char_map(&glyph_text_of(&[&r]), src, 0);
        let end = src.find("\\hl").unwrap();
        assert_eq!(
            m.deletion(stop(&m, end - 1), false, src),
            Some(15..16),
            "the space first"
        );
        assert_eq!(
            m.deletion(stop(&m, end), false, src),
            Some(16..19),
            "then \\hl, not the brace"
        );
        // a numbered heading's number is LaTeX's: the first stop is before the title
        let src = "\\section{B1 Intro}";
        let r = row("1 B1 Intro", 100.0);
        let m = char_map(&glyph_text_of(&[&r]), src, 0);
        assert_eq!(m.stops[0].off, src.find('B').unwrap());
        // text an environment prints before the caret (its title) is no piece to delete
        let src = "\\begin{note}{Plan} text";
        let r = row("Note (Plan): text", 100.0);
        let m = char_map(&glyph_text_of(&[&r]), src, 0);
        let p = src.find("Plan").unwrap();
        assert_eq!(m.deletion(stop(&m, p), true, src), None);
    }

    #[test]
    fn macro_text_maps_after_the_text_before_it() {
        let src = "see Section~\\ref{sec:intro} for more";
        let r = row("see Section 3 for more", 100.0);
        let gt = glyph_text_of(&[&r]);
        assert_eq!(
            source_offset(&gt, text(&gt).find('3').unwrap(), false, src),
            src.find("~\\ref")
        );
        assert_eq!(
            source_offset(&gt, text(&gt).find("for").unwrap(), false, src),
            src.find("for")
        );
    }
}
