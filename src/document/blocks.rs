// The document as editable blocks: rtex spans, with display formulas split out of paragraphs so
// that only the formula under the cursor turns into source.
use super::{PlacedRow, Preview};
use crate::engine::Span;
use std::ops::Range;

pub struct Block {
    pub span: u64,
    /// Source bytes, without surrounding whitespace.
    pub range: Range<usize>,
    pub rows: Vec<PlacedRow>,
}

const MATH_ENVS: &[&str] = &[
    "equation",
    "equation*",
    "align",
    "align*",
    "gather",
    "gather*",
    "multline",
    "multline*",
    "eqnarray",
    "eqnarray*",
    "displaymath",
];

/// A block that is a display formula (`\[…\]`, `$$…$$`, an equation environment).
pub fn is_display_math(block: &str) -> bool {
    block.starts_with("\\[")
        || block.starts_with("$$")
        || block
            .strip_prefix("\\begin{")
            .and_then(|r| r.split('}').next())
            .is_some_and(|name| MATH_ENVS.contains(&name))
}

/// Byte at `i` starts a command `\x` (not an escaped backslash `\\x`).
fn is_cmd(s: &[u8], i: usize) -> bool {
    s[i] == b'\\' && s[..i].iter().rev().take_while(|&&b| b == b'\\').count() % 2 == 0
}

/// Split `r` of `src` into text and display-math pieces: (range, is_math), blank pieces dropped.
pub fn split_display_math(src: &str, r: Range<usize>) -> Vec<(Range<usize>, bool)> {
    let s = src.as_bytes();
    let mut out = vec![];
    let mut text_start = r.start;
    let mut i = r.start;
    let push = |out: &mut Vec<(Range<usize>, bool)>, a: usize, b: usize, math: bool| {
        let t = trim(src, a..b);
        if !t.is_empty() {
            out.push((t, math));
        }
    };
    while i < r.end {
        let close: Option<String> =
            if s[i] == b'$' && s.get(i + 1) == Some(&b'$') && (i == 0 || !is_cmd(s, i - 1)) {
                Some("$$".into())
            } else if is_cmd(s, i) && s.get(i + 1) == Some(&b'[') {
                Some("\\]".into())
            } else if is_cmd(s, i) && src[i..].starts_with("\\begin{") {
                let name = src[i + 7..].split('}').next().unwrap_or("");
                MATH_ENVS
                    .contains(&name)
                    .then(|| format!("\\end{{{name}}}"))
            } else {
                None
            };
        let Some(close) = close else {
            i += 1;
            continue;
        };
        let open_len = if close == "$$" || close == "\\]" {
            2
        } else {
            close.len() + 2
        };
        let Some(end) = src[i + open_len..r.end]
            .find(&close)
            .map(|k| i + open_len + k + close.len())
        else {
            break;
        };
        push(&mut out, text_start, i, false);
        push(&mut out, i, end, true);
        i = end;
        text_start = end;
    }
    push(&mut out, text_start, r.end, false);
    out
}

pub fn trim(src: &str, r: Range<usize>) -> Range<usize> {
    // a stale range (an edit not yet re-spanned) can end inside a character: an empty block
    let Some(t) = src.get(r.clone()) else {
        let at = r.start.min(src.len());
        return at..at;
    };
    let start = r.start + (t.len() - t.trim_start().len());
    start..(start + t.trim().len()).max(start)
}

pub fn blocks(spans: &[Span], src: &str, preview: &Preview) -> Vec<Block> {
    let spans: Vec<&Span> = spans
        .iter()
        .filter(|sp| {
            !matches!(sp.kind.as_str(), "Preamble" | "Trailer")
                && src.get(sp.range.start..sp.range.end).is_some()
        })
        .collect();
    let mut rows: Vec<Vec<PlacedRow>> = spans.iter().map(|sp| preview.unit_rows(sp.id)).collect();
    give_unplaced_rows(&mut rows, preview.unplaced_rows());
    let mut out = vec![];
    for (sp, rows) in spans.into_iter().zip(rows) {
        let whole = trim(src, sp.range.start..sp.range.end);
        if whole.is_empty() {
            continue;
        }
        let segs = if sp.kind == "Body" {
            split_display_math(src, whole.clone())
        } else {
            vec![]
        };
        // runs of rows with the same display flag, matched to the pieces in order
        let mut runs: Vec<(bool, Vec<PlacedRow>)> = vec![];
        for r in rows.iter().cloned() {
            match runs.last_mut() {
                Some((d, v)) if *d == r.display => v.push(r),
                _ => runs.push((r.display, vec![r])),
            }
        }
        if segs.len() > 1
            && runs.len() == segs.len()
            && runs.iter().zip(&segs).all(|((d, _), (_, m))| d == m)
        {
            for ((_, rows), (range, _)) in runs.into_iter().zip(segs) {
                out.push(Block {
                    span: sp.id,
                    range,
                    rows,
                });
            }
        } else {
            out.push(Block {
                span: sp.id,
                range: whole,
                rows,
            });
        }
    }
    out
}

/// Spans without rows (not placed by the layout) get the unplaced page lines between their placed
/// neighbours: one capture unit each when the counts agree, else all to the first of them.
fn give_unplaced_rows(rows: &mut [Vec<PlacedRow>], free: Vec<PlacedRow>) {
    let key = |r: &PlacedRow| (r.page, r.bbox[1]);
    let mut i = 0;
    while i < rows.len() {
        if !rows[i].is_empty() {
            i += 1;
            continue;
        }
        let end = (i..rows.len())
            .find(|&k| !rows[k].is_empty())
            .unwrap_or(rows.len());
        let after = rows[..i].iter().rev().find_map(|r| r.last()).map(key);
        let before = rows.get(end).and_then(|r| r.first()).map(key);
        let gap: Vec<&PlacedRow> = free
            .iter()
            .filter(|r| after.is_none_or(|a| key(r) > a) && before.is_none_or(|b| key(r) < b))
            .collect();
        let mut units: Vec<Vec<PlacedRow>> = vec![];
        for r in gap {
            match units.last_mut() {
                Some(u)
                    if u[0].row.dl.lines[u[0].row.line].unit == r.row.dl.lines[r.row.line].unit =>
                {
                    u.push(r.clone())
                }
                _ => units.push(vec![r.clone()]),
            }
        }
        if units.len() == end - i {
            for (k, u) in units.into_iter().enumerate() {
                rows[i + k] = u;
            }
        } else {
            rows[i] = units.concat();
        }
        i = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_survives_a_stale_range() {
        assert_eq!(trim("  ab  ", 0..6), 2..4);
        // "é" is two bytes: a range ending inside it gives an empty block, not a panic
        assert_eq!(trim("xé", 0..2), 0..0);
        assert_eq!(trim("ab", 1..9), 1..1);
    }

    #[test]
    fn splits_display_math_out_of_text() {
        let src = "text text text\n\\[\n  E = mc^2\n\\]\nafter \\\\[2pt] more";
        let segs = split_display_math(src, 0..src.len());
        let got: Vec<(&str, bool)> = segs.iter().map(|(r, m)| (&src[r.clone()], *m)).collect();
        assert_eq!(
            got,
            vec![
                ("text text text", false),
                ("\\[\n  E = mc^2\n\\]", true),
                ("after \\\\[2pt] more", false)
            ]
        );
        let src = "a \\begin{align}x\\end{align}";
        assert_eq!(split_display_math(src, 0..src.len()).len(), 2);
    }

    #[test]
    fn unplaced_spans_get_the_lines_between_their_neighbours() {
        use crate::render::Row;
        use std::sync::Arc;
        let line = |y: f64, unit: i64| serde_json::json!({"unit": unit, "x": 0, "y": y, "w": 10, "h": 1, "d": 0, "items": []});
        let dl: crate::engine::DisplayList =
            serde_json::from_value(serde_json::json!({"lines": [line(10.0, 1), line(20.0, 2), line(30.0, 3), line(40.0, 4)]})).unwrap();
        let dl = Arc::new(dl);
        let row = |i: usize| {
            let l = &dl.lines[i];
            PlacedRow {
                page: 1,
                bbox: [l.x, l.y - l.h, l.x + l.w, l.y],
                display: false,
                row: Row {
                    dl: dl.clone(),
                    line: i,
                    dx: 0.0,
                    dy: 0.0,
                },
            }
        };
        // spans: placed (line 0), unplaced, placed (line 3); lines 1 and 2 are free, two units for one span
        let mut rows = vec![vec![row(0)], vec![], vec![row(3)]];
        give_unplaced_rows(&mut rows, vec![row(1), row(2)]);
        assert_eq!(
            rows[1].iter().map(|r| r.row.line).collect::<Vec<_>>(),
            vec![1, 2]
        );
        let mut rows = vec![vec![row(0)], vec![], vec![], vec![row(3)]];
        give_unplaced_rows(&mut rows, vec![row(1), row(2)]);
        assert_eq!((rows[1][0].row.line, rows[2][0].row.line), (1, 2));
    }
}
