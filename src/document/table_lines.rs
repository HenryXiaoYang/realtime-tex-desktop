// Where a typeset table's column and row lines are on its page, found from its glyphs: the text of
// each cell is where the cell is, so a column's line sits in the gap after its widest text and a
// row's under its lowest baseline. The editor draws a handle on each, to drag.
use super::caret::CharMap;
use super::table::Table;
use std::ops::Range;

/// TeX points in sp.
const PT: f64 = 65536.0;
/// LaTeX's `\tabcolsep`: the room on each side of a column's text.
const TABCOLSEP: f64 = 6.0 * PT;
/// How far below its baseline a row's text reaches, in font sizes.
const DESCENT: f64 = 0.3;

/// A line between column `col` and the next (or the table's right edge).
#[derive(Clone, Debug, PartialEq)]
pub struct ColLine {
    pub col: usize,
    /// Where it stands (sp).
    pub x: f64,
    /// The width the column has now (pt): its fixed one, else the width of its text.
    pub width: f64,
}

/// A line under row `row`.
#[derive(Clone, Debug, PartialEq)]
pub struct RowLine {
    pub row: usize,
    /// Where it stands (sp): under the row's text and the room the source gives it.
    pub y: f64,
    /// The room under the row now (pt).
    pub space: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Lines {
    /// The table's text box (sp): left, top, right, bottom.
    pub bbox: [f64; 4],
    pub cols: Vec<ColLine>,
    pub rows: Vec<RowLine>,
}

/// The extent of what one cell shows: left, right, baseline, size.
fn cell_extent(map: &CharMap, cell: &Range<usize>) -> Option<[f64; 4]> {
    map.chars
        .iter()
        .filter(|c| c.glyph)
        .filter(|c| {
            c.src
                .as_ref()
                .is_some_and(|s| s.start >= cell.start && s.end <= cell.end)
        })
        .map(|c| [c.bbox[0], c.bbox[0] + c.bbox[2], c.bbox[1], c.bbox[3]])
        .reduce(|a, b| {
            [
                a[0].min(b[0]),
                a[1].max(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]
        })
}

/// The lines of `table` as `map` (its page's caret map) has it set, `src` the file's text. Rows
/// with a `\multicolumn` are left out of the columns, a row with no `\\` has no line under it;
/// None when no cell has text on this page.
pub fn lines(table: &Table, src: &str, map: &CharMap) -> Option<Lines> {
    let cols = table.cols();
    // each cell's extent, the row's baseline and the size of its text
    let grid: Vec<Vec<Option<[f64; 4]>>> = table
        .rows
        .iter()
        .map(|r| r.cells.iter().map(|c| cell_extent(map, c)).collect())
        .collect();
    let bbox = grid.iter().flatten().flatten().copied().reduce(|a, b| {
        [
            a[0].min(b[0]),
            a[1].max(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]
    })?;
    let full = |ri: usize| table.rows[ri].cells.len() == cols;
    // a column's text: where it starts and ends in any of its cells
    let span = |c: usize| {
        (0..grid.len())
            .filter(|ri| full(*ri))
            .filter_map(|ri| grid[ri][c])
            .map(|e| (e[0], e[1]))
            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
    };
    let spans: Vec<_> = (0..cols).map(span).collect();
    let mut out = Lines {
        bbox: [0.0; 4],
        cols: vec![],
        rows: vec![],
    };
    for (c, s) in spans.iter().enumerate() {
        let Some((left, right)) = *s else {
            continue;
        };
        let fixed = table.col_width(src, c);
        let width = fixed.unwrap_or((right - left) / PT);
        // a fixed width is the column's, whatever its text fills; else the line is mid-gap
        let x = match (fixed, spans.get(c + 1).copied().flatten()) {
            (Some(w), _) => left + w * PT + TABCOLSEP,
            (None, Some((next, _))) => (right + next) / 2.0,
            (None, None) => right + TABCOLSEP,
        };
        out.cols.push(ColLine { col: c, x, width });
    }
    let (mut top, mut bottom) = (f64::MAX, f64::MIN);
    for (ri, row) in grid.iter().enumerate() {
        let Some(e) = row.iter().flatten().copied().reduce(|a, b| {
            [
                a[0].min(b[0]),
                a[1].max(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]
        }) else {
            continue;
        };
        top = top.min(e[2] - e[3] * 0.8);
        let space = table.row_space(src, ri);
        let y = e[2] + e[3] * DESCENT + space * PT;
        bottom = bottom.max(y);
        if table.rows[ri].end.is_some() {
            out.rows.push(RowLine { row: ri, y, space });
        }
    }
    let right = out.cols.iter().map(|c| c.x).fold(bbox[1], f64::max);
    out.bbox = [bbox[0] - TABCOLSEP, top, right, bottom];
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::caret::Char;
    use crate::document::table;

    /// A map with the glyphs of each cell's first source char, at the given x and baseline.
    fn map(src: &str, t: &Table, at: &[(usize, usize, f64, f64, f64)]) -> CharMap {
        let chars = at
            .iter()
            .map(|&(r, c, x, w, y)| {
                let cell = t.rows[r].cells[c].clone();
                assert!(cell.end > cell.start, "{}", &src[cell.clone()]);
                Char {
                    ch: 'x',
                    src: Some(cell.start..cell.start + 1),
                    bbox: [x * PT, y * PT, w * PT, 10.0 * PT],
                    glyph: true,
                    font: None,
                }
            })
            .collect();
        CharMap {
            chars,
            stops: vec![],
            range: 0..src.len(),
        }
    }

    const SRC: &str = "\\begin{tabular}{ll}\na & bb \\\\\ncc & d \\\\[4pt]\n\\end{tabular}";

    #[test]
    fn a_lines_stand_in_the_gap_after_a_columns_text() {
        let t = table::find(SRC, 0..SRC.len()).unwrap();
        let m = map(
            SRC,
            &t,
            &[
                (0, 0, 10.0, 5.0, 20.0),
                (0, 1, 40.0, 10.0, 20.0),
                (1, 0, 10.0, 10.0, 40.0),
                (1, 1, 40.0, 5.0, 40.0),
            ],
        );
        let l = lines(&t, SRC, &m).unwrap();
        // column 0's text ends at 20 pt and column 1's starts at 40: the line is between
        assert_eq!(l.cols[0].col, 0);
        assert!((l.cols[0].x / PT - 30.0).abs() < 1e-6);
        assert!((l.cols[0].width - 10.0).abs() < 1e-6);
        // the last column's line is a column gap after its text
        assert!((l.cols[1].x / PT - 56.0).abs() < 1e-6, "{:?}", l.cols[1]);
        // the rows' lines are under their baselines, the second further by its 4 pt
        assert_eq!(l.rows.len(), 2);
        assert!((l.rows[0].y / PT - 23.0).abs() < 1e-6);
        assert!((l.rows[1].y / PT - (43.0 + 4.0)).abs() < 1e-6);
        assert!((l.rows[1].space - 4.0).abs() < 1e-6);
    }

    #[test]
    fn a_fixed_width_column_ends_where_the_source_says() {
        let src = "\\begin{tabular}{p{50pt}l}\na & b \\\\\n\\end{tabular}";
        let t = table::find(src, 0..src.len()).unwrap();
        let m = map(src, &t, &[(0, 0, 10.0, 5.0, 20.0), (0, 1, 80.0, 5.0, 20.0)]);
        let l = lines(&t, src, &m).unwrap();
        assert!((l.cols[0].width - 50.0).abs() < 1e-6);
        assert!((l.cols[0].x / PT - (10.0 + 50.0 + 6.0)).abs() < 1e-6);
    }

    #[test]
    fn a_table_with_no_text_has_no_lines() {
        let t = table::find(SRC, 0..SRC.len()).unwrap();
        let m = map(SRC, &t, &[]);
        assert!(lines(&t, SRC, &m).is_none());
    }
}
