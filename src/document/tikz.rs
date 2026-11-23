// The points of a TikZ picture: the plain `(x,y)` coordinates in its source, for dragging in the
// TikZ panel. Only coordinates written out as two numbers count: a named node `(a)`, a polar
// `(30:2)`, a relative `+(1,0)` or a computed `(\x,{..})` is not something a dot stands for.
use std::ops::Range;

/// One `(x,y)` in the source: where its two numbers are, and what they say.
#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    pub x_at: Range<usize>,
    pub y_at: Range<usize>,
    pub x: f64,
    pub y: f64,
}

/// The coordinates in `src` (the picture's TeX), in the order written. Comments are skipped.
pub fn points(src: &str) -> Vec<Point> {
    let mut out = Vec::new();
    let mut line_start = 0;
    for line in src.split_inclusive('\n') {
        let code = match line.find('%') {
            // `\%` is a percent sign, not a comment
            Some(i) if !line[..i].ends_with('\\') => &line[..i],
            _ => line,
        };
        scan(code, line_start, &mut out);
        line_start += line.len();
    }
    out
}

fn scan(code: &str, base: usize, out: &mut Vec<Point>) {
    let b = code.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'(' {
            i += 1;
            continue;
        }
        let Some(len) = code[i + 1..].find(')') else {
            return;
        };
        let inner = &code[i + 1..i + 1 + len];
        // relative moves (`+(1,0)`, `++(1,0)`) are offsets, not places
        let relative = i > 0 && b[i - 1] == b'+';
        if !relative
            && !inner.contains('(')
            && let Some(p) = pair(inner, base + i + 1)
        {
            out.push(p);
        }
        i += len + 2;
    }
}

/// `x, y` as two plain numbers; `at` is where `inner` starts in the source.
fn pair(inner: &str, at: usize) -> Option<Point> {
    let comma = inner.find(',')?;
    let (a, b) = (&inner[..comma], &inner[comma + 1..]);
    if b.contains(',') {
        // three numbers: a point in space, not on this canvas
        return None;
    }
    let num = |s: &str, from: usize| -> Option<(Range<usize>, f64)> {
        let t = s.trim();
        let v: f64 = t.parse().ok().filter(|v: &f64| v.is_finite())?;
        let lead = s.len() - s.trim_start().len();
        let start = at + from + lead;
        Some((start..start + t.len(), v))
    };
    let (x_at, x) = num(a, 0)?;
    let (y_at, y) = num(b, comma + 1)?;
    Some(Point { x_at, y_at, x, y })
}

/// A coordinate as TikZ reads it: two decimals at most, no trailing zeros.
pub fn show(v: f64) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s.into()
    }
}

/// `src` with point `ix` moved to (`x`, `y`), the rest of its text kept. `None` when there is no
/// such point.
pub fn moved(src: &str, ix: usize, x: f64, y: f64) -> Option<String> {
    let p = points(src).into_iter().nth(ix)?;
    let mut out = String::with_capacity(src.len() + 4);
    // the two numbers are in order in the source: x, then y
    out.push_str(&src[..p.x_at.start]);
    out.push_str(&show(x));
    out.push_str(&src[p.x_at.end..p.y_at.start]);
    out.push_str(&show(y));
    out.push_str(&src[p.y_at.end..]);
    Some(out)
}

/// Round to the nearest `step` (a quarter of a unit: the numbers come out round).
pub fn snap(v: f64, step: f64) -> f64 {
    (v / step).round() * step
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIC: &str = "\\begin{tikzpicture}\n  \\draw (0,0) -- (1.5, -2);\n  \\node (a) at (3,1) {x};\n  \\draw (a) -- ++(1,0) -- +(0,1) -- (30:2); % (9,9)\n  \\draw (1,2,3) -- ({\\x},1);\n\\end{tikzpicture}";

    #[test]
    fn plain_pairs_are_points_and_nothing_else_is() {
        let p = points(PIC);
        let got: Vec<(f64, f64)> = p.iter().map(|p| (p.x, p.y)).collect();
        // named, polar, relative, three-number, computed and commented ones are left out
        assert_eq!(got, vec![(0.0, 0.0), (1.5, -2.0), (3.0, 1.0)]);
    }

    #[test]
    fn a_point_moves_with_the_text_around_it_kept() {
        let s = moved(PIC, 1, 2.0, 0.25).unwrap();
        assert!(s.contains("-- (2, 0.25);"), "{s}");
        assert_eq!(s.replace("(2, 0.25)", "(1.5, -2)"), PIC);
        let s = moved(PIC, 2, -1.0, 0.0).unwrap();
        assert!(s.contains("at (-1,0) {x}"), "{s}");
        assert!(moved(PIC, 3, 0.0, 0.0).is_none());
    }

    #[test]
    fn numbers_are_short_and_never_minus_zero() {
        assert_eq!(show(1.0), "1");
        assert_eq!(show(0.25), "0.25");
        assert_eq!(show(-0.001), "0");
        assert_eq!(show(2.3333), "2.33");
        assert_eq!(snap(1.37, 0.25), 1.25);
    }

    #[test]
    fn a_percent_sign_is_not_a_comment_when_escaped() {
        let p = points("\\node {50\\%} at (1,2);");
        assert_eq!(p.len(), 1);
    }
}
