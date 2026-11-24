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

/// The code of `src` without its comments: same length, a comment's text blanked.
fn code(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.split_inclusive('\n') {
        match line.find('%') {
            Some(i) if !line[..i].ends_with('\\') => {
                out.push_str(&line[..i]);
                out.extend(
                    line[i..]
                        .chars()
                        .map(|c| if c == '\n' { '\n' } else { ' ' }),
                );
            }
            _ => out.push_str(line),
        }
    }
    out
}

/// The `}` closing the `{` at `open`.
fn close_brace(src: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    let mut chars = src.get(open..)?.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// A node's text: `{…}` after `node`.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    /// The text between the braces.
    pub text: Range<usize>,
    /// The point (by index in `points`) it sits at: its `at (x,y)`, else the one before it.
    pub anchor: Option<usize>,
}

/// The nodes of the picture that have text.
pub fn labels(src: &str) -> Vec<Label> {
    let code = code(src);
    let pts = points(src);
    let mut out = vec![];
    let mut from = 0;
    while let Some(i) = code[from..].find("node") {
        let at = from + i;
        from = at + 4;
        let word_before = at > 0 && code.as_bytes()[at - 1].is_ascii_alphabetic();
        let word_after = code
            .as_bytes()
            .get(at + 4)
            .is_some_and(|b| b.is_ascii_alphabetic());
        if word_before || word_after {
            continue;
        }
        // the header: options, a name, `at (x,y)`, up to the `{` of the text
        let mut j = at + 4;
        let mut ok = false;
        while j < code.len() {
            match code.as_bytes()[j] {
                b'{' => {
                    ok = true;
                    break;
                }
                b';' => break,
                b'[' => match code[j..].find(']') {
                    Some(k) => j += k,
                    None => break,
                },
                _ => {}
            }
            j += 1;
        }
        if !ok {
            continue;
        }
        let Some(close) = close_brace(&code, j) else {
            continue;
        };
        let anchor = pts
            .iter()
            .position(|p| p.x_at.start > at && p.x_at.start < j)
            .or_else(|| pts.iter().rposition(|p| p.y_at.end <= at));
        out.push(Label {
            text: j + 1..close,
            anchor,
        });
        from = close;
    }
    out
}

/// `src` with label `ix`'s text replaced; none when there is no such label.
pub fn set_label(src: &str, ix: usize, text: &str) -> Option<String> {
    let l = labels(src).into_iter().nth(ix)?;
    Some(format!(
        "{}{}{}",
        &src[..l.text.start],
        text,
        &src[l.text.end..]
    ))
}

/// A circle `(x,y) circle (r)` (or `circle [radius=r]`): its centre point and radius.
#[derive(Clone, Debug, PartialEq)]
pub struct Circle {
    pub center: usize,
    pub r_at: Range<usize>,
    pub r: f64,
}

pub fn circles(src: &str) -> Vec<Circle> {
    let code = code(src);
    let pts = points(src);
    let mut out = vec![];
    for (ix, p) in pts.iter().enumerate() {
        // the `)` after the point, then `circle`
        let after = p.y_at.end + code[p.y_at.end..].find(')').map_or(0, |k| k + 1);
        let rest = &code[after..];
        let t = rest.trim_start();
        let Some(t2) = t.strip_prefix("circle") else {
            continue;
        };
        let base = after + (rest.len() - t.len()) + "circle".len();
        let t3 = t2.trim_start();
        let open = base + (t2.len() - t3.len());
        let (num_from, close) = if t3.starts_with('(') {
            (open + 1, code[open..].find(')').map(|k| open + k))
        } else if t3.starts_with('[') {
            let end = code[open..].find(']').map(|k| open + k);
            let eq = code[open..]
                .find("radius=")
                .map(|k| open + k + "radius=".len());
            (eq.unwrap_or(open + 1), end)
        } else {
            continue;
        };
        let Some(close) = close else { continue };
        if num_from > close {
            continue;
        }
        let inner = &code[num_from..close];
        // `0.5`, `0.5cm`
        let digits = inner.trim_start();
        let lead = inner.len() - digits.len();
        let len = digits
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .unwrap_or(digits.len());
        if let Ok(r) = digits[..len].parse::<f64>() {
            out.push(Circle {
                center: ix,
                r_at: num_from + lead..num_from + lead + len,
                r,
            });
        }
    }
    out
}

/// `src` with circle `ix`'s radius set.
pub fn set_radius(src: &str, ix: usize, r: f64) -> Option<String> {
    let c = circles(src).into_iter().nth(ix)?;
    Some(format!(
        "{}{}{}",
        &src[..c.r_at.start],
        show(r),
        &src[c.r_at.end..]
    ))
}

/// A rectangle `(x,y) rectangle (x,y)`: its two corner points.
#[derive(Clone, Debug, PartialEq)]
pub struct Rect {
    pub a: usize,
    pub b: usize,
}

pub fn rects(src: &str) -> Vec<Rect> {
    let code = code(src);
    let pts = points(src);
    pts.windows(2)
        .enumerate()
        .filter(|(_, w)| {
            code.get(w[0].y_at.end..w[1].x_at.start)
                .is_some_and(|g| g.contains("rectangle") && !g.contains(';'))
        })
        .map(|(i, _)| Rect { a: i, b: i + 1 })
        .collect()
}

const COMMANDS: &[&str] = &["\\filldraw", "\\draw", "\\fill", "\\path", "\\node"];

/// One statement: `\draw … ;`.
#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub range: Range<usize>,
    /// Where its command ends (its options follow).
    pub head: usize,
}

pub fn statements(src: &str) -> Vec<Stmt> {
    let code = code(src);
    let mut out: Vec<Stmt> = vec![];
    let mut i = 0;
    while i < code.len() {
        let rest = &code[i..];
        let Some(cmd) = COMMANDS.iter().find(|c| {
            rest.starts_with(**c)
                && !rest
                    .as_bytes()
                    .get(c.len())
                    .is_some_and(|b| b.is_ascii_alphabetic())
        }) else {
            i += rest.chars().next().map_or(1, char::len_utf8);
            continue;
        };
        let (mut depth, mut j) = (0i32, i + cmd.len());
        while j < code.len() {
            match code.as_bytes()[j] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                b';' if depth <= 0 => break,
                _ => {}
            }
            j += 1;
        }
        out.push(Stmt {
            range: i..(j + 1).min(code.len()),
            head: i + cmd.len(),
        });
        i = j + 1;
    }
    out
}

/// The statement holding point `ix`.
pub fn stmt_of(src: &str, point: usize) -> Option<usize> {
    let at = points(src).get(point)?.x_at.start;
    statements(src).iter().position(|s| s.range.contains(&at))
}

/// What a statement's options say, for the style tools to show.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    pub stroke: Option<String>,
    pub fill: Option<String>,
    pub width: Option<String>,
    pub dashed: bool,
    pub arrow: bool,
}

const WIDTHS: &[&str] = &[
    "ultra thin",
    "very thin",
    "thin",
    "semithick",
    "thick",
    "very thick",
    "ultra thick",
];

/// The options of a statement: their text range (inside the `[ ]`) and the items.
fn options(src: &str, st: &Stmt) -> (Option<Range<usize>>, Vec<String>) {
    let rest = &src[st.head..st.range.end];
    let t = rest.trim_start();
    if !t.starts_with('[') {
        return (None, vec![]);
    }
    let open = st.head + (rest.len() - t.len());
    // the matching bracket (braces inside keep their commas)
    let (mut depth, mut close) = (0i32, None);
    for (k, c) in src[open..st.range.end].char_indices() {
        match c {
            '[' | '{' => depth += 1,
            ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + k);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return (None, vec![]);
    };
    let inner = &src[open + 1..close];
    let (mut items, mut depth, mut from) = (vec![], 0i32, 0);
    for (k, c) in inner.char_indices() {
        match c {
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            ',' if depth == 0 => {
                items.push(inner[from..k].trim().to_string());
                from = k + 1;
            }
            _ => {}
        }
    }
    items.push(inner[from..].trim().to_string());
    items.retain(|i| !i.is_empty());
    (Some(open..close + 1), items)
}

/// How statement `st` is drawn now.
pub fn look(src: &str, st: &Stmt) -> Look {
    let (_, items) = options(src, st);
    let mut l = Look::default();
    for i in &items {
        if let Some(c) = i.strip_prefix("draw=") {
            l.stroke = Some(c.to_string());
        } else if let Some(c) = i.strip_prefix("fill=") {
            l.fill = Some(c.to_string());
        } else if WIDTHS.contains(&i.as_str()) {
            l.width = Some(i.clone());
        } else if i == "dashed" {
            l.dashed = true;
        } else if i.starts_with("->") || i.starts_with("<-") || i.starts_with("<->") {
            l.arrow = true;
        }
    }
    l
}

/// A change to how a statement is drawn.
#[derive(Clone, Debug)]
pub enum Style<'a> {
    Stroke(Option<&'a str>),
    Fill(Option<&'a str>),
    Width(Option<&'a str>),
    Dashed,
    Arrow,
}

/// `src` with statement `st` drawn as `edit` says: its options rewritten, the rest as it was.
pub fn styled(src: &str, st: &Stmt, edit: &Style) -> Option<String> {
    let (at, mut items) = options(src, st);
    let had = |items: &[String], f: &dyn Fn(&str) -> bool| items.iter().any(|i| f(i));
    match edit {
        Style::Stroke(c) => {
            items.retain(|i| !i.starts_with("draw="));
            if let Some(c) = c {
                items.insert(0, format!("draw={c}"));
            }
        }
        Style::Fill(c) => {
            items.retain(|i| !i.starts_with("fill="));
            if let Some(c) = c {
                items.insert(0, format!("fill={c}"));
            }
        }
        Style::Width(w) => {
            items.retain(|i| !WIDTHS.contains(&i.as_str()));
            if let Some(w) = w {
                items.push((*w).to_string());
            }
        }
        Style::Dashed => {
            if had(&items, &|i| i == "dashed") {
                items.retain(|i| i != "dashed");
            } else {
                items.push("dashed".into());
            }
        }
        Style::Arrow => {
            if had(&items, &|i| i == "->") {
                items.retain(|i| i != "->");
            } else {
                items.push("->".into());
            }
        }
    }
    let text = if items.is_empty() {
        String::new()
    } else {
        format!("[{}]", items.join(", "))
    };
    // `\fill` paints no outline: one asked for makes it `\filldraw`
    let wants_outline = matches!(edit, Style::Stroke(Some(_)));
    let mut out = String::from(src);
    match at {
        Some(r) if text.is_empty() => {
            // the options gone, and the space that set them off
            let from = if out[..r.start].ends_with(' ') {
                r.start - 1
            } else {
                r.start
            };
            out.replace_range(from..r.end, "");
        }
        Some(r) => out.replace_range(r, &text),
        None => out.insert_str(
            st.head,
            &if text.is_empty() {
                text
            } else {
                format!(" {text}")
            },
        ),
    }
    if wants_outline
        && src[st.range.clone()].starts_with("\\fill")
        && !src[st.range.clone()].starts_with("\\filldraw")
    {
        out.replace_range(
            st.range.start..st.range.start + "\\fill".len(),
            "\\filldraw",
        );
    }
    Some(out)
}

/// What a new shape is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Line,
    Rectangle,
    Circle,
    Text,
}

/// `src` with a new shape at (`x`, `y`) before the picture's end; none without an `\end`.
pub fn added(src: &str, shape: Shape, x: f64, y: f64) -> Option<String> {
    let end = src.rfind("\\end{tikzpicture}")?;
    let (x2, y2) = (x + 1.0, y + 0.5);
    let line = match shape {
        Shape::Line => format!(
            "\\draw ({},{}) -- ({},{});",
            show(x),
            show(y),
            show(x2),
            show(y)
        ),
        Shape::Rectangle => format!(
            "\\draw ({},{}) rectangle ({},{});",
            show(x),
            show(y),
            show(x2),
            show(y2)
        ),
        Shape::Circle => format!("\\draw ({},{}) circle (0.5);", show(x), show(y)),
        Shape::Text => format!("\\node at ({},{}) {{Text}};", show(x), show(y)),
    };
    let at = src[..end].rfind('\n').map_or(end, |i| i + 1);
    // the end on a line of its own: the new line goes above it, indented as the picture's body is
    let on_own_line = src[at..end].trim().is_empty();
    let (at, text) = if on_own_line {
        (at, format!("  {line}\n"))
    } else {
        (end, format!("\n  {line}\n"))
    };
    let mut out = String::from(src);
    out.insert_str(at, &text);
    Some(out)
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

    const SHAPES: &str = "\\begin{tikzpicture}\n  \\draw[thick] (0,0) rectangle (2,1);\n  \\draw (3,1) circle (0.5);\n  \\node[above] at (1,1.5) {Title};\n  \\draw (0,0) node {A} -- (1,1) node[right] {B};\n\\end{tikzpicture}";

    #[test]
    fn labels_are_found_with_the_points_they_sit_at() {
        let l = labels(SHAPES);
        let text = |i: usize| &SHAPES[l[i].text.clone()];
        assert_eq!((text(0), text(1), text(2)), ("Title", "A", "B"));
        let p = points(SHAPES);
        // `at (1,1.5)`; the points before `node {A}` and `node[right] {B}`
        assert_eq!(p[l[0].anchor.unwrap()].y, 1.5);
        assert_eq!(
            (p[l[1].anchor.unwrap()].x, p[l[1].anchor.unwrap()].y),
            (0.0, 0.0)
        );
        assert_eq!(
            (p[l[2].anchor.unwrap()].x, p[l[2].anchor.unwrap()].y),
            (1.0, 1.0)
        );
        let s = set_label(SHAPES, 0, "A new title").unwrap();
        assert!(s.contains("{A new title};") && s.contains("{A} --"), "{s}");
    }

    #[test]
    fn circles_and_rectangles_are_read_and_resized() {
        let c = circles(SHAPES);
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].center, c[0].r), (2, 0.5));
        let s = set_radius(SHAPES, 0, 1.25).unwrap();
        assert!(s.contains("circle (1.25);"), "{s}");
        assert_eq!(rects(SHAPES), vec![Rect { a: 0, b: 1 }]);
        let r = circles("\\draw (1,1) circle[radius=2cm];");
        assert_eq!(r[0].r, 2.0);
        assert_eq!(
            set_radius("\\draw (1,1) circle[radius=2cm];", 0, 1.0).unwrap(),
            "\\draw (1,1) circle[radius=1cm];"
        );
    }

    #[test]
    fn a_statements_style_is_read_and_rewritten() {
        let st = statements(SHAPES);
        assert_eq!(st.len(), 4);
        assert_eq!(stmt_of(SHAPES, 2), Some(1));
        let l = look(SHAPES, &st[0]);
        assert_eq!(l.width.as_deref(), Some("thick"));
        let s = styled(SHAPES, &st[0], &Style::Stroke(Some("red"))).unwrap();
        assert!(s.contains("\\draw[draw=red, thick] (0,0) rectangle"), "{s}");
        let s2 = styled(&s, &statements(&s)[0], &Style::Width(Some("ultra thick"))).unwrap();
        assert!(s2.contains("[draw=red, ultra thick]"), "{s2}");
        // a statement without options gets them; none left, the brackets go
        let s = styled(SHAPES, &st[1], &Style::Fill(Some("yellow!40"))).unwrap();
        assert!(s.contains("\\draw [fill=yellow!40] (3,1)"), "{s}");
        let back = styled(&s, &statements(&s)[1], &Style::Fill(None)).unwrap();
        assert!(back.contains("\\draw (3,1) circle"), "{back}");
        let a = styled(SHAPES, &st[1], &Style::Arrow).unwrap();
        assert!(look(&a, &statements(&a)[1]).arrow);
        let f = styled(
            "\\fill (0,0) circle (1);",
            &statements("\\fill (0,0) circle (1);")[0],
            &Style::Stroke(Some("red")),
        )
        .unwrap();
        assert!(f.starts_with("\\filldraw [draw=red]"), "{f}");
    }

    #[test]
    fn a_shape_is_added_above_the_end() {
        for (shape, want) in [
            (Shape::Line, "  \\draw (1,1) -- (2,1);\n\\end{tikzpicture}"),
            (Shape::Rectangle, "  \\draw (1,1) rectangle (2,1.5);\n\\end"),
            (Shape::Circle, "  \\draw (1,1) circle (0.5);\n\\end"),
            (Shape::Text, "  \\node at (1,1) {Text};\n\\end"),
        ] {
            let s = added(SHAPES, shape, 1.0, 1.0).unwrap();
            assert!(s.contains(want), "{s}");
        }
        assert!(added("no picture", Shape::Line, 0.0, 0.0).is_none());
    }
}
