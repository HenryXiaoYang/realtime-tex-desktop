// A `tabular` in its source: where each cell's TeX is, and the edits that change a cell or add and
// remove rows and columns. Cells are kept as the TeX they are (a `\multicolumn`, a nested
// environment), so nothing is lost by editing the one beside it.
use super::caret::comment_start;
use std::ops::Range;

pub struct Table {
    /// The column spec, inside its braces.
    pub spec: Range<usize>,
    /// From after the spec's `}` to the `\end{tabular}`.
    pub body: Range<usize>,
    pub rows: Vec<Row>,
}

pub struct Row {
    /// Each cell's TeX, without the space around it (an empty cell: where text goes).
    pub cells: Vec<Range<usize>>,
    /// Where its first cell's text starts, after the rules (`\hline`) above it.
    pub start: usize,
    /// Its `\\` (and `[2pt]`), if it has one.
    pub end: Option<Range<usize>>,
}

/// A source change: `range` becomes `text`.
pub struct Change {
    pub range: Range<usize>,
    pub text: String,
}

const RULES: &[&str] = &[
    "\\hline",
    "\\toprule",
    "\\midrule",
    "\\bottomrule",
    "\\cline",
    "\\cmidrule",
];

/// The first `\begin{tabular}{…}` in `block` (a range of `src`), parsed.
pub fn find(src: &str, block: Range<usize>) -> Option<Table> {
    let text = src.get(block.clone())?;
    let begin = block.start + text.find("\\begin{tabular}")? + "\\begin{tabular}".len();
    let open = begin + src[begin..].find(|c: char| !c.is_whitespace())?;
    if !src[open..].starts_with('{') {
        return None;
    }
    let close = group_end(src, open)?;
    let end = close + 1 + src.get(close + 1..block.end)?.find("\\end{tabular}")?;
    parse(src, open + 1..close, close + 1..end)
}

/// The `}` closing the `{` at `open`.
fn group_end(src: &str, open: usize) -> Option<usize> {
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

/// Where the rules at the start of `s` (`\\hline`…, white space between them) end; 0 with none.
fn rules_len(s: &str) -> usize {
    let mut at = 0;
    loop {
        let rest = &s[at..];
        let trimmed = rest.trim_start();
        let skipped = rest.len() - trimmed.len();
        let Some(rule) = RULES.iter().find(|r| trimmed.starts_with(**r)) else {
            return at;
        };
        at += skipped + rule.len();
        // a `\cline{2-3}` or `\cmidrule(lr){2-3}` has its arguments
        while let Some(c) = s[at..].chars().next().filter(|c| matches!(c, '(' | '{')) {
            let close = if c == '(' {
                s[at..].find(')').map(|i| at + i)
            } else {
                group_end(s, at)
            };
            match close {
                Some(i) => at = i + 1,
                None => return at,
            }
        }
    }
}

fn parse(src: &str, spec: Range<usize>, body: Range<usize>) -> Option<Table> {
    let b = src.as_bytes();
    let (mut i, mut depth, mut envs) = (body.start, 0i32, 0i32);
    let (mut amps, mut breaks): (Vec<usize>, Vec<Range<usize>>) = (vec![], vec![]);
    while i < body.end {
        match b[i] {
            b'\\' if b.get(i + 1) == Some(&b'\\') => {
                if depth == 0 && envs == 0 {
                    let mut e = i + 2;
                    if b.get(e) == Some(&b'*') {
                        e += 1;
                    }
                    if b.get(e) == Some(&b'[')
                        && let Some(k) = src[e..body.end].find(']')
                    {
                        e += k + 1;
                    }
                    breaks.push(i..e);
                    i = e;
                    continue;
                }
                i += 2;
            }
            b'\\' if src[i..].starts_with("\\begin{") => {
                envs += 1;
                i += 7;
            }
            b'\\' if src[i..].starts_with("\\end{") => {
                envs -= 1;
                i += 5;
            }
            b'\\' => i += 2,
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                i += 1;
            }
            b'&' if depth == 0 && envs == 0 => {
                amps.push(i);
                i += 1;
            }
            b'%' => {
                while i < body.end && b[i] != b'\n' {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    if depth != 0 || envs != 0 {
        return None;
    }
    let mut rows = vec![];
    let mut from = body.start;
    for k in 0..=breaks.len() {
        let to = breaks.get(k).map_or(body.end, |r| r.start);
        let after_rules = from + rules_len(&src[from..to]);
        let rest = &src[after_rules..to];
        let start = after_rules + rest.len() - rest.trim_start().len();
        // what follows the last `\\` is only the rules under the table
        if k == breaks.len() && src[start..to].trim().is_empty() {
            break;
        }
        let mut cells = vec![];
        let mut cell_from = start;
        let seps = amps.iter().copied().filter(|a| (start..to).contains(a));
        for end in seps.chain([to]) {
            cells.push(trimmed(src, cell_from..end));
            cell_from = end + 1;
        }
        rows.push(Row {
            cells,
            start,
            end: breaks.get(k).cloned(),
        });
        from = breaks.get(k).map_or(to, |r| r.end);
    }
    Some(Table { spec, body, rows })
}

/// `r` without the white space at its ends; an empty cell is a spot inside it.
fn trimmed(src: &str, r: Range<usize>) -> Range<usize> {
    let t = &src[r.clone()];
    let start = r.start + (t.len() - t.trim_start().len());
    let end = start + t.trim().len();
    if start == end {
        let at = r.start + usize::from(r.len() >= 2);
        return at..at;
    }
    start..end
}

impl Table {
    pub fn cols(&self) -> usize {
        self.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0)
    }

    /// Cell (`row`, `col`) read `text`.
    pub fn set_cell(&self, src: &str, row: usize, col: usize, text: &str) -> Option<Change> {
        let r = self.rows.get(row)?.cells.get(col)?.clone();
        let mut text = text.trim().to_string();
        // into an empty cell, with the space the cell had room for
        if r.is_empty() {
            let space_before = src[..r.start].ends_with(char::is_whitespace);
            let space_after = src[r.end..].starts_with(char::is_whitespace);
            if !space_before {
                text.insert(0, ' ');
            }
            if !space_after {
                text.push(' ');
            }
        }
        Some(Change { range: r, text })
    }

    /// A row of empty cells after row `row`, after the rules under it.
    pub fn add_row(&self, src: &str, row: usize) -> Option<Change> {
        let r = self.rows.get(row)?;
        let cols = self.cols();
        let empty = vec![""; cols].join(" & ");
        let indent = indent_of(src, r.start);
        match &r.end {
            Some(end) => {
                // after the rules under the row, with a copy of them under the new one
                let at = end.end + rules_len(&src[end.end..self.body.end]);
                let under = src[end.end..at].trim();
                let mut text = format!("\n{indent}{empty} \\\\");
                if !under.is_empty() {
                    text.push_str(&format!("\n{indent}{under}"));
                }
                Some(Change {
                    range: at..at,
                    text,
                })
            }
            // the last row has no `\\`: it gets one, and the new row follows it
            None => {
                let at = past_comment(src, r.cells.last()?.end);
                Some(Change {
                    range: at..at,
                    text: format!(" \\\\\n{indent}{empty} \\\\"),
                })
            }
        }
    }

    /// Row `row` removed (never the only one).
    pub fn del_row(&self, src: &str, row: usize) -> Option<Change> {
        if self.rows.len() < 2 {
            return None;
        }
        let r = self.rows.get(row)?;
        let last = r.cells.last()?.end;
        let to = match &r.end {
            Some(e) => e.end,
            None => last,
        };
        // the row and the line break after it; the last row takes the `\\` before it
        if r.end.is_none() {
            let before = &self.rows[row - 1];
            let prev = before.end.clone()?;
            // (a blank row keeps its `\\`: a last row with nothing in it is no row at all)
            let blank = before
                .cells
                .iter()
                .all(|c| src[c.clone()].trim().is_empty());
            return Some(Change {
                range: if blank { prev.end } else { prev.start }..to,
                text: String::new(),
            });
        }
        // the whole line when the row has it to itself
        let ls = line_start(src, r.start);
        let from = if src[ls..r.start].trim().is_empty() {
            ls
        } else {
            r.start
        };
        let spaces = src[to..self.body.end]
            .find(|c: char| c != ' ' && c != '\t')
            .unwrap_or(0);
        let after = if src[to + spaces..].starts_with('\n') {
            to + spaces + 1
        } else {
            to
        };
        Some(Change {
            range: from..after,
            text: String::new(),
        })
    }

    /// A column after column `col`, in the spec and every row.
    pub fn add_col(&self, src: &str, col: usize) -> Option<Vec<Change>> {
        let mut out = vec![];
        for r in &self.rows {
            if r.cells.len() != self.cols() {
                continue; // a `\multicolumn` row: left as it is
            }
            let at = past_comment(src, r.cells.get(col)?.end);
            out.push(Change {
                range: at..at,
                text: " &".into(),
            });
        }
        let tokens = spec_tokens(&src[self.spec.clone()]);
        let at = tokens
            .get(col)
            .map_or(self.spec.end, |t| self.spec.start + t.end);
        let bar = src[at..self.spec.end].starts_with('|');
        out.push(Change {
            range: at + usize::from(bar)..at + usize::from(bar),
            text: if bar { "l|" } else { "l" }.into(),
        });
        Some(out)
    }

    /// Column `col` removed from the spec and every row (never the only one).
    pub fn del_col(&self, src: &str, col: usize) -> Option<Vec<Change>> {
        if self.cols() < 2 || col >= self.cols() {
            return None;
        }
        let mut out = vec![];
        for r in &self.rows {
            if r.cells.len() != self.cols() {
                continue;
            }
            // from this cell to the next one's start, or (the last) from the one before's end
            let range = if col + 1 < r.cells.len() {
                r.cells[col].start..r.cells[col + 1].start
            } else {
                // a comment ending the one before would swallow what follows it: the `&` goes too
                let prev = r.cells[col - 1].end;
                let gap = &src[prev..r.cells[col].start];
                let from = match gap.rfind('&') {
                    Some(amp) if comment_start(&src[line_start(src, prev)..prev]).is_some() => {
                        prev + amp
                    }
                    _ => prev,
                };
                from..r.cells[col].end
            };
            out.push(Change {
                range,
                text: String::new(),
            });
        }
        let tokens = spec_tokens(&src[self.spec.clone()]);
        let t = tokens.get(col)?.clone();
        let s = &src[self.spec.clone()];
        let end = t.end + usize::from(s[t.end..].starts_with('|'));
        out.push(Change {
            range: self.spec.start + t.start..self.spec.start + end,
            text: String::new(),
        });
        Some(out)
    }
}

/// How a table is ruled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Borders {
    /// No rules and no bars.
    None,
    /// A rule above and below every row (`\hline`).
    Lines,
    /// Those, and a bar between the columns and at both sides.
    Grid,
    /// `booktabs`: above the table, under the first row, below the table.
    Booktabs,
}

impl Table {
    /// Column `col`'s alignment: `l`, `c` or `r` (a plain `p{3cm}` column has none; one set in
    /// `>{\\centering\\arraybackslash}p{3cm}` has it).
    pub fn align(&self, src: &str, col: usize) -> Option<char> {
        let (prefix, t) = spec_cols(&src[self.spec.clone()]).into_iter().nth(col)?;
        let text = &src[self.spec.start + t.start..self.spec.start + t.end];
        if text.len() == 1 {
            return text.chars().next().filter(|c| "lcr".contains(*c));
        }
        let prefix = prefix?;
        prefix_align(&src[self.spec.start + prefix.start..self.spec.start + prefix.end])
    }

    /// Column `col` made `l`, `c` or `r`; none for a column that is not one of those.
    pub fn set_align(&self, src: &str, col: usize, to: char) -> Option<Change> {
        let (prefix, t) = spec_cols(&src[self.spec.clone()]).into_iter().nth(col)?;
        let at = |r: &Range<usize>| self.spec.start + r.start..self.spec.start + r.end;
        let text = &src[at(&t)];
        if text.len() == 1 {
            self.align(src, col)?;
            return Some(Change {
                range: at(&t),
                text: to.to_string(),
            });
        }
        // a fixed width keeps it: only the prefix changes
        if !text.starts_with(['p', 'm', 'b']) {
            return None;
        }
        let start = prefix.as_ref().map_or(t.start, |p| p.start);
        Some(Change {
            range: self.spec.start + start..self.spec.start + t.start,
            text: align_prefix(to).to_string(),
        })
    }

    /// Column `col`'s fixed width in points, when it has one in a unit TeX counts in.
    pub fn col_width(&self, src: &str, col: usize) -> Option<f64> {
        let (_, t) = spec_cols(&src[self.spec.clone()]).into_iter().nth(col)?;
        let text = &src[self.spec.start + t.start..self.spec.start + t.end];
        if !text.starts_with(['p', 'm', 'b']) || text.len() < 3 {
            return None;
        }
        length_pt(&text[2..text.len() - 1])
    }

    /// Column `col` made `pt` wide (a fixed width keeps its alignment); the packages it needs.
    pub fn set_col_width(
        &self,
        src: &str,
        col: usize,
        pt: f64,
    ) -> Option<(Change, Vec<&'static str>)> {
        let (prefix, t) = spec_cols(&src[self.spec.clone()]).into_iter().nth(col)?;
        let abs = |r: &Range<usize>| self.spec.start + r.start..self.spec.start + r.end;
        let text = &src[abs(&t)];
        let w = show_pt(pt.max(10.0));
        if text.starts_with(['p', 'm', 'b']) && text.len() >= 3 {
            return Some((
                Change {
                    range: abs(&t),
                    text: format!("{}{{{w}}}", &text[..1]),
                },
                vec![],
            ));
        }
        let letter = text.chars().next().filter(|c| "lcr".contains(*c))?;
        let from = prefix.as_ref().map_or(t.start, |p| p.start);
        Some((
            Change {
                range: self.spec.start + from..self.spec.start + t.end,
                text: format!("{}p{{{w}}}", align_prefix(letter)),
            },
            vec!["array"],
        ))
    }

    /// Column `col` back to the width its text asks for, its alignment kept.
    pub fn auto_col(&self, src: &str, col: usize) -> Option<Change> {
        let letter = self.align(src, col)?;
        let (prefix, t) = spec_cols(&src[self.spec.clone()]).into_iter().nth(col)?;
        if src[self.spec.start + t.start..self.spec.start + t.end].len() == 1 {
            return None;
        }
        let from = prefix.as_ref().map_or(t.start, |p| p.start);
        Some(Change {
            range: self.spec.start + from..self.spec.start + t.end,
            text: letter.to_string(),
        })
    }

    /// Room kept under row `row` beyond its text, in points (`\\[6pt]`).
    pub fn row_space(&self, src: &str, row: usize) -> f64 {
        let Some(e) = self.rows.get(row).and_then(|r| r.end.clone()) else {
            return 0.0;
        };
        let t = &src[e];
        t.find('[')
            .and_then(|i| length_pt(t[i + 1..].trim_end_matches(']')))
            .unwrap_or(0.0)
    }

    /// Row `row` given `pt` of room under it; none for the last row when it has no `\\`.
    pub fn set_row_space(&self, src: &str, row: usize, pt: f64) -> Option<Change> {
        let e = self.rows.get(row)?.end.clone()?;
        let star = if src[e.clone()].starts_with("\\\\*") {
            "*"
        } else {
            ""
        };
        let text = if pt < 0.5 {
            format!("\\\\{star}")
        } else {
            format!("\\\\{star}[{}]", show_pt(pt))
        };
        Some(Change { range: e, text })
    }

    /// The ruling the table has now, as near as the four styles tell it.
    pub fn borders(&self, src: &str) -> Borders {
        let rules: String = self.rule_slots(src).into_iter().map(|r| &src[r]).collect();
        if ["\\toprule", "\\midrule", "\\bottomrule"]
            .iter()
            .any(|r| rules.contains(r))
        {
            Borders::Booktabs
        } else if rules.contains("\\hline") {
            if src[self.spec.clone()].contains('|') {
                Borders::Grid
            } else {
                Borders::Lines
            }
        } else {
            Borders::None
        }
    }

    /// Where the rules go: above each row, then under the last (when it has a `\\`).
    fn rule_slots(&self, src: &str) -> Vec<Range<usize>> {
        let mut out = vec![];
        let mut from = self.body.start;
        for r in &self.rows {
            out.push(from..r.start);
            match &r.end {
                Some(e) => from = e.end,
                None => return out,
            }
        }
        out.push(from..self.body.end.max(from).min(src.len()));
        out
    }

    /// The table ruled as `style`: its rules and the bars of its spec rewritten.
    pub fn set_borders(&self, src: &str, style: Borders) -> Option<Vec<Change>> {
        let n = self.rows.len();
        let rule = |slot: usize| match style {
            Borders::None => None,
            Borders::Lines | Borders::Grid => Some("\\hline"),
            Borders::Booktabs if slot == 0 => Some("\\toprule"),
            Borders::Booktabs if slot == n => Some("\\bottomrule"),
            Borders::Booktabs if slot == 1 => Some("\\midrule"),
            Borders::Booktabs => None,
        };
        let mut out = vec![];
        for (slot, range) in self.rule_slots(src).into_iter().enumerate() {
            let indent = match self.rows.get(slot) {
                Some(r) => indent_of(src, r.start),
                None => indent_of(src, self.rows.last()?.start),
            };
            let text = if slot == n {
                // before `\end{tabular}`, at its own indent
                let close = indent_of(src, self.body.end);
                match rule(slot) {
                    Some(r) => format!("\n{indent}{r}\n{close}"),
                    None => format!("\n{close}"),
                }
            } else {
                match rule(slot) {
                    Some(r) => format!("\n{indent}{r}\n{indent}"),
                    None => format!("\n{indent}"),
                }
            };
            out.push(Change { range, text });
        }
        // the last row has no `\\` to put a rule under: it gets one
        if out.len() == n
            && let Some(r) = rule(n)
        {
            let last = self.rows.last()?;
            let at = past_comment(src, last.cells.last()?.end);
            out.push(Change {
                range: at..at,
                text: format!(" \\\\\n{}{r}", indent_of(src, last.start)),
            });
        }
        out.push(Change {
            range: self.spec.clone(),
            text: ruled_spec(&src[self.spec.clone()], style == Borders::Grid),
        });
        Some(out)
    }

    /// Whether the first row is all bold (`\textbf`).
    pub fn header(&self, src: &str) -> bool {
        let cells: Vec<&str> = self.rows.first().map_or(vec![], |r| {
            r.cells
                .iter()
                .map(|c| split_comment(&src[c.clone()]).0)
                .filter(|c| !c.trim().is_empty())
                .collect()
        });
        !cells.is_empty() && cells.iter().all(|c| bold_inner(c).is_some())
    }

    /// The first row made bold (each cell in `\textbf`), or plain again; none when a cell of it is
    /// a `\multicolumn`, which is left as it is.
    pub fn set_header(&self, src: &str, on: bool) -> Option<Vec<Change>> {
        let row = self.rows.first()?;
        let mut out = vec![];
        for c in &row.cells {
            let (text, comment) = split_comment(&src[c.clone()]);
            if text.contains("\\multicolumn") {
                return None;
            }
            if text.trim().is_empty() {
                continue;
            }
            match (on, bold_inner(text)) {
                (true, None) => out.push(Change {
                    range: c.clone(),
                    text: with_comment(format!("\\textbf{{{text}}}"), comment),
                }),
                (false, Some(inner)) => out.push(Change {
                    range: c.clone(),
                    text: with_comment(inner.to_string(), comment),
                }),
                _ => {}
            }
        }
        Some(out)
    }
}

/// `cell` with its whole text in `\\cmd{…}` (a bold or italic cell), or out of it again.
pub fn toggle_cell(cell: &str, cmd: &str) -> String {
    let (cell, comment) = split_comment(cell);
    let (shade, text) = split_shade(cell);
    let open = format!("\\{cmd}{{");
    let text = text.trim();
    let inner = text
        .strip_prefix(&open)
        .and_then(|t| t.strip_suffix('}'))
        .filter(|i| crate::document::caret::balanced(i));
    let body = match inner {
        Some(i) => i.to_string(),
        None => format!("{open}{text}}}"),
    };
    with_comment(format!("{shade}{body}"), comment)
}

/// `cell` shaded `color` (`yellow!30`), or not shaded with none.
pub fn shade_cell(cell: &str, color: Option<&str>) -> String {
    let (_, text) = split_shade(cell);
    match color {
        Some(c) => format!("\\cellcolor{{{c}}}{}", text.trim_start()),
        None => text.trim_start().to_string(),
    }
}

/// `cell`'s text in `\\textcolor{color}{…}`, or plain again with none.
pub fn colour_cell(cell: &str, color: Option<&str>) -> String {
    let (cell, comment) = split_comment(cell);
    let (shade, text) = split_shade(cell);
    let text = text.trim();
    let plain = text
        .strip_prefix("\\textcolor{")
        .and_then(|t| t.split_once("}{").map(|(_, r)| r))
        .and_then(|r| r.strip_suffix('}'))
        .filter(|i| crate::document::caret::balanced(i))
        .unwrap_or(text);
    let out = match color {
        Some(c) => format!("{shade}\\textcolor{{{c}}}{{{plain}}}"),
        None => format!("{shade}{plain}"),
    };
    with_comment(out, comment)
}

/// A cell's `\\cellcolor{…}` (with the space after it) and the rest.
fn split_shade(cell: &str) -> (&str, &str) {
    if let Some(rest) = cell.strip_prefix("\\cellcolor{")
        && let Some(close) = rest.find('}')
    {
        let at = "\\cellcolor{".len() + close + 1;
        let tail = &cell[at..];
        return (
            &cell[..at + (tail.len() - tail.trim_start().len())],
            tail.trim_start(),
        );
    }
    ("", cell)
}

/// What a cell is shaded, and whether its text is bold, italic or coloured.
pub fn cell_look(cell: &str) -> (Option<String>, bool, bool, Option<String>) {
    let cell = split_comment(cell).0;
    let shade = cell
        .strip_prefix("\\cellcolor{")
        .and_then(|r| r.split_once('}'))
        .map(|(c, _)| c.to_string());
    let (_, text) = split_shade(cell);
    let text = text.trim();
    let is = |cmd: &str| {
        text.strip_prefix(&format!("\\{cmd}{{"))
            .is_some_and(|t| t.ends_with('}'))
    };
    let colour = text
        .strip_prefix("\\textcolor{")
        .and_then(|r| r.split_once('}'))
        .map(|(c, _)| c.to_string());
    (shade, is("textbf"), is("textit"), colour)
}

/// `x` of `\textbf{x}` when the whole cell is that.
fn bold_inner(cell: &str) -> Option<&str> {
    let inner = cell.strip_prefix("\\textbf{")?.strip_suffix('}')?;
    crate::document::caret::balanced(inner).then_some(inner)
}

/// A column spec without its bars, or with one between every column and at both sides.
fn ruled_spec(spec: &str, bars: bool) -> String {
    let (mut plain, mut depth) = (String::new(), 0i32);
    for c in spec.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if !(c == '|' && depth == 0) {
            plain.push(c);
        }
    }
    if !bars {
        return plain;
    }
    let mut out = String::new();
    let mut from = 0;
    for (i, t) in spec_tokens(&plain).into_iter().enumerate() {
        out.push_str(&plain[from..t.start]);
        if i == 0 {
            out.push('|');
        }
        out.push_str(&plain[t.clone()]);
        out.push('|');
        from = t.end;
    }
    out.push_str(&plain[from..]);
    out
}

/// The column letters of a spec (`|l|p{3cm}|c|`: three), each with its arguments.
fn spec_tokens(spec: &str) -> Vec<Range<usize>> {
    spec_cols(spec).into_iter().map(|(_, t)| t).collect()
}

/// Each column of a spec: the `>{…}` before it, if it has one, and its letter with arguments.
fn spec_cols(spec: &str) -> Vec<(Option<Range<usize>>, Range<usize>)> {
    let mut out = vec![];
    let mut prefix = None;
    let mut i = 0;
    while i < spec.len() {
        let c = spec[i..].chars().next().unwrap_or(' ');
        let mut end = i + c.len_utf8();
        if matches!(c, '|' | ' ') {
            i = end;
            continue;
        }
        // `p{3cm}`, `@{}`: the group belongs to the letter
        if spec[end..].starts_with('{')
            && let Some(close) = group_end(spec, end)
        {
            end = close + 1;
        }
        match c {
            '>' => prefix = Some(i..end),
            // `@{…}` and `!{…}` are not columns
            '@' | '!' | '<' => {}
            _ => out.push((prefix.take(), i..end)),
        }
        i = end;
    }
    out
}

/// The alignment a `>{…}` prefix sets: `l`, `c` or `r`.
fn prefix_align(prefix: &str) -> Option<char> {
    [
        ("\\centering", 'c'),
        ("\\raggedleft", 'r'),
        ("\\raggedright", 'l'),
    ]
    .into_iter()
    .find(|(cmd, _)| prefix.contains(cmd))
    .map(|(_, a)| a)
}

/// The `>{…}` that sets alignment `a` (and ends the cell's lines properly).
fn align_prefix(a: char) -> &'static str {
    match a {
        'c' => ">{\\centering\\arraybackslash}",
        'r' => ">{\\raggedleft\\arraybackslash}",
        _ => ">{\\raggedright\\arraybackslash}",
    }
}

/// A TeX length in points: `3cm`, `72pt`, `1.5 in`.
fn length_pt(s: &str) -> Option<f64> {
    let s = s.trim();
    let split = s.find(|c: char| c.is_alphabetic())?;
    let (num, unit) = s.split_at(split);
    let k = match unit.trim() {
        "pt" => 1.0,
        "bp" => 1.00375,
        "cm" => 28.452_756,
        "mm" => 2.845_275_6,
        "in" => 72.27,
        _ => return None,
    };
    Some(num.trim().parse::<f64>().ok()? * k)
}

/// A length in points as TeX writes it: no zero decimals.
fn show_pt(v: f64) -> String {
    let v = (v * 10.0).round() / 10.0;
    if v.fract() == 0.0 {
        format!("{v:.0}pt")
    } else {
        format!("{v:.1}pt")
    }
}

fn line_start(src: &str, at: usize) -> usize {
    src[..at].rfind('\n').map_or(0, |i| i + 1)
}

/// Where text may go after a cell that ends at `at`: there, or, when a comment ends its line, past
/// the line break (anything put inside the comment is lost with it).
fn past_comment(src: &str, at: usize) -> usize {
    if comment_start(&src[line_start(src, at)..at]).is_some() {
        return src[at..].find('\n').map_or(at, |i| at + i + 1);
    }
    at
}

/// A cell without the comment that ends it, and that comment (`A % note` is `A` and `% note`).
fn split_comment(cell: &str) -> (&str, &str) {
    let ls = cell.rfind('\n').map_or(0, |i| i + 1);
    match comment_start(&cell[ls..]) {
        Some(i) => (cell[..ls + i].trim_end(), &cell[ls + i..]),
        None => (cell, ""),
    }
}

/// `text` and, after a space, the comment that ended the cell.
fn with_comment(text: String, comment: &str) -> String {
    if comment.is_empty() {
        text
    } else {
        format!("{text} {comment}")
    }
}

/// The white space the line at `at` starts with.
fn indent_of(src: &str, at: usize) -> String {
    let s = &src[line_start(src, at)..at];
    s.chars().take_while(|c| c.is_whitespace()).collect()
}

/// `src` with `changes` made, last first so each range stays where it was.
pub fn apply(src: &str, mut changes: Vec<Change>) -> String {
    changes.sort_by_key(|c| std::cmp::Reverse(c.range.start));
    let mut out = src.to_string();
    for c in changes {
        out.replace_range(c.range, &c.text);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMENTED: &str =
        "\\begin{tabular}{ll}\n  A % one\n  & B % two\n  \\\\\n  C & D % last\n\\end{tabular}";

    #[test]
    fn a_comment_ending_a_cell_does_not_swallow_what_follows_it() {
        let t = table(COMMENTED);
        let n = t.rows.len();
        // a column: its `&` goes after the line break, not into the comment
        let out = apply(COMMENTED, t.add_col(COMMENTED, 1).unwrap());
        let t2 = table(&out);
        assert_eq!((t2.cols(), t2.rows.len()), (3, n), "{out}");
        // a row after the last, which has no `\\`
        let out = apply(COMMENTED, vec![t.add_row(COMMENTED, 1).unwrap()]);
        assert_eq!(table(&out).rows.len(), n + 1, "{out}");
        // the last column out: the `\\` after the comment stays out of it
        let out = apply(COMMENTED, t.del_col(COMMENTED, 1).unwrap());
        assert_eq!(
            (table(&out).cols(), table(&out).rows.len()),
            (1, n),
            "{out}"
        );
        // the header wraps the text, not the comment
        let out = apply(COMMENTED, t.set_header(COMMENTED, true).unwrap());
        assert!(out.contains("\\textbf{A} % one"), "{out}");
        assert!(table(&out).header(&out));
        // and so do the cell tools
        assert_eq!(toggle_cell("A % note", "textbf"), "\\textbf{A} % note");
        assert_eq!(toggle_cell("\\textbf{A} % note", "textbf"), "A % note");
        assert_eq!(
            colour_cell("A % note", Some("red")),
            "\\textcolor{red}{A} % note"
        );
        assert!(cell_look("\\textbf{A} % note").1);
    }

    #[test]
    fn a_column_or_row_that_is_not_there_is_left_alone() {
        let t = table(T);
        assert!(t.del_col(T, 2).is_none() && t.del_col(T, 9).is_none());
        assert!(t.add_col(T, 9).is_none() && t.del_row(T, 9).is_none());
    }

    const T: &str = "\\begin{tabular}{|l|c|}\n  \\hline\n  A & B \\\\\n  \\hline\n  c & {d \\& e} \\\\\n  \\hline\n\\end{tabular}";

    fn table(src: &str) -> Table {
        find(src, 0..src.len()).expect("a table")
    }

    fn cells(src: &str, t: &Table) -> Vec<Vec<String>> {
        t.rows
            .iter()
            .map(|r| r.cells.iter().map(|c| src[c.clone()].to_string()).collect())
            .collect()
    }

    #[test]
    fn reads_the_cells_between_the_rules() {
        let t = table(T);
        assert_eq!(
            cells(T, &t),
            vec![vec!["A", "B"], vec!["c", "{d \\& e}"]],
            "the last `\\hline` is no row"
        );
        assert_eq!(&T[t.spec.clone()], "|l|c|");
        assert_eq!(t.cols(), 2);
    }

    #[test]
    fn keeps_a_multicolumn_and_a_last_row_without_a_break_whole() {
        let s = "\\begin{tabular}{ll}\n\\multicolumn{2}{c}{Head} \\\\\na & b\n\\end{tabular}";
        let t = table(s);
        assert_eq!(
            cells(s, &t),
            vec![vec!["\\multicolumn{2}{c}{Head}"], vec!["a", "b"]]
        );
        assert!(t.rows[1].end.is_none());
    }

    #[test]
    fn sets_a_cell_and_fills_an_empty_one() {
        let s = "\\begin{tabular}{ll}\na & \\\\\n&b\n\\end{tabular}";
        let t = table(s);
        let at = |row, col, text: &str| apply(s, vec![t.set_cell(s, row, col, text).unwrap()]);
        assert!(at(0, 0, "x y").contains("\nx y & \\\\"));
        assert!(at(0, 1, "new").contains("a & new \\\\"));
        assert!(at(1, 0, "z").contains("\nz &b"));
    }

    #[test]
    fn adds_and_removes_rows() {
        let t = table(T);
        let more = apply(T, vec![t.add_row(T, 0).unwrap()]);
        let t2 = table(&more);
        assert_eq!(
            cells(&more, &t2),
            vec![vec!["A", "B"], vec!["", ""], vec!["c", "{d \\& e}"]]
        );
        assert_eq!(more.matches("\\hline").count(), 4, "{more}");
        let fewer = apply(T, vec![t.del_row(T, 0).unwrap()]);
        assert_eq!(cells(&fewer, &table(&fewer)), vec![vec!["c", "{d \\& e}"]]);
        // the last row without `\\` gives one up, the one before becomes the last
        let s = "\\begin{tabular}{l}\na \\\\\nb\n\\end{tabular}";
        let t = table(s);
        let more = apply(s, vec![t.add_row(s, 1).unwrap()]);
        assert_eq!(
            cells(&more, &table(&more)),
            vec![vec!["a"], vec!["b"], vec![""]]
        );
        let fewer = apply(s, vec![t.del_row(s, 1).unwrap()]);
        assert_eq!(cells(&fewer, &table(&fewer)), vec![vec!["a"]]);
        assert!(
            table(&fewer).del_row(&fewer, 0).is_none(),
            "never the only row"
        );
    }

    #[test]
    fn adds_and_removes_columns() {
        let t = table(T);
        let wide = apply(T, t.add_col(T, 0).unwrap());
        let t2 = table(&wide);
        assert_eq!(&wide[t2.spec.clone()], "|l|l|c|");
        assert_eq!(
            cells(&wide, &t2),
            vec![vec!["A", "", "B"], vec!["c", "", "{d \\& e}"]]
        );
        let narrow = apply(T, t.del_col(T, 1).unwrap());
        let t3 = table(&narrow);
        assert_eq!(&narrow[t3.spec.clone()], "|l|");
        assert_eq!(cells(&narrow, &t3), vec![vec!["A"], vec!["c"]]);
        let narrow = apply(T, t.del_col(T, 0).unwrap());
        assert_eq!(&narrow[table(&narrow).spec.clone()], "|c|");
        assert_eq!(
            cells(&narrow, &table(&narrow)),
            vec![vec!["B"], vec!["{d \\& e}"]]
        );
    }

    #[test]
    fn aligns_a_column_that_has_a_letter() {
        let s = "\\begin{tabular}{|l|p{3cm}|c|}\na & b & c\n\\end{tabular}";
        let t = table(s);
        assert_eq!(
            (t.align(s, 0), t.align(s, 1), t.align(s, 2)),
            (Some('l'), None, Some('c'))
        );
        let r = apply(s, vec![t.set_align(s, 2, 'r').unwrap()]);
        assert!(r.contains("{|l|p{3cm}|r|}"), "{r}");
        // a fixed width keeps it, the alignment goes in front of it
        let c = apply(s, vec![t.set_align(s, 1, 'c').unwrap()]);
        assert!(
            c.contains("{|l|>{\\centering\\arraybackslash}p{3cm}|c|}"),
            "{c}"
        );
    }

    #[test]
    fn rules_a_table_in_each_style_and_reads_it_back() {
        let ruled = |src: &str, style| {
            let t = table(src);
            let out = apply(src, t.set_borders(src, style).unwrap());
            assert_eq!(table(&out).borders(&out), style, "{out}");
            assert_eq!(
                cells(&out, &table(&out)),
                cells(src, &t),
                "the cells stay: {out}"
            );
            out
        };
        assert_eq!(table(T).borders(T), Borders::Grid);
        let none = ruled(T, Borders::None);
        assert_eq!(
            none.matches("\\hline").count() + none.matches('|').count(),
            0,
            "{none}"
        );
        let lines = ruled(&none, Borders::Lines);
        assert_eq!(lines.matches("\\hline").count(), 3, "{lines}");
        assert!(lines.contains("{lc}"));
        let grid = ruled(&lines, Borders::Grid);
        assert!(grid.contains("{|l|c|}"), "{grid}");
        let book = ruled(&grid, Borders::Booktabs);
        assert!(
            book.contains("\\toprule")
                && book.contains("\\midrule")
                && book.contains("\\bottomrule"),
            "{book}"
        );
        assert!(book.contains("{lc}") && !book.contains("hline"));
        // a last row without `\\` gets one for the rule under it
        let open = "\\begin{tabular}{ll}\na & b \\\\\nc & d\n\\end{tabular}";
        let out = ruled(open, Borders::Lines);
        assert_eq!(out.matches("\\hline").count(), 3, "{out}");
    }

    #[test]
    fn makes_the_first_row_a_header_and_back() {
        let t = table(T);
        assert!(!t.header(T));
        let bold = apply(T, t.set_header(T, true).unwrap());
        assert!(bold.contains("\\textbf{A} & \\textbf{B}"), "{bold}");
        assert!(table(&bold).header(&bold));
        assert!(
            table(&bold).set_header(&bold, true).unwrap().is_empty(),
            "already"
        );
        let plain = apply(&bold, table(&bold).set_header(&bold, false).unwrap());
        assert_eq!(plain, T);
        let m = "\\begin{tabular}{ll}\n\\multicolumn{2}{c}{H} \\\\\n\\end{tabular}";
        assert!(table(m).set_header(m, true).is_none());
    }

    #[test]
    fn spec_tokens_skip_bars_and_keep_arguments() {
        let s = "|l|p{3cm}|@{}c|";
        let got: Vec<&str> = spec_tokens(s).into_iter().map(|r| &s[r]).collect();
        assert_eq!(got, vec!["l", "p{3cm}", "c"]);
    }

    #[test]
    fn a_column_gets_a_width_and_keeps_its_alignment() {
        let s = "\\begin{tabular}{|l|c|}\nA & B \\\\\n\\end{tabular}";
        let t = table(s);
        let (c, pk) = t.set_col_width(s, 1, 72.0).unwrap();
        assert_eq!(pk, vec!["array"]);
        let w = apply(s, vec![c]);
        assert!(
            w.contains("{|l|>{\\centering\\arraybackslash}p{72pt}|}"),
            "{w}"
        );
        let t = table(&w);
        assert_eq!(t.col_width(&w, 1), Some(72.0));
        assert_eq!(t.align(&w, 1), Some('c'));
        // narrower, then right aligned, then back to automatic
        let (c, pk) = t.set_col_width(&w, 1, 40.5).unwrap();
        assert!(pk.is_empty());
        let w = apply(&w, vec![c]);
        assert!(w.contains("p{40.5pt}"), "{w}");
        let t = table(&w);
        let w = apply(&w, vec![t.set_align(&w, 1, 'r').unwrap()]);
        assert!(
            w.contains(">{\\raggedleft\\arraybackslash}p{40.5pt}"),
            "{w}"
        );
        let t = table(&w);
        let w = apply(&w, vec![t.auto_col(&w, 1).unwrap()]);
        assert!(w.contains("{|l|r|}"), "{w}");
    }

    #[test]
    fn rows_keep_room_under_them() {
        let t = table(T);
        assert_eq!(t.row_space(T, 0), 0.0);
        let w = apply(T, vec![t.set_row_space(T, 0, 6.0).unwrap()]);
        assert!(w.contains("A & B \\\\[6pt]"), "{w}");
        let t = table(&w);
        assert_eq!(t.row_space(&w, 0), 6.0);
        let w = apply(&w, vec![t.set_row_space(&w, 0, 0.0).unwrap()]);
        assert_eq!(w, T);
        // the last row has no `\\` to carry it
        assert!(table(T).set_row_space(T, 5, 3.0).is_none());
    }

    #[test]
    fn cells_are_bold_shaded_and_coloured_in_any_order() {
        let c = toggle_cell("Total", "textbf");
        assert_eq!(c, "\\textbf{Total}");
        let c = shade_cell(&c, Some("yellow!30"));
        assert_eq!(c, "\\cellcolor{yellow!30}\\textbf{Total}");
        let c = colour_cell(&c, Some("red"));
        assert_eq!(c, "\\cellcolor{yellow!30}\\textcolor{red}{\\textbf{Total}}");
        assert_eq!(
            cell_look(&c),
            (Some("yellow!30".into()), false, false, Some("red".into()))
        );
        let c = colour_cell(&c, None);
        let c = toggle_cell(&c, "textbf");
        assert_eq!(c, "\\cellcolor{yellow!30}Total");
        assert_eq!(shade_cell(&c, None), "Total");
    }
}
