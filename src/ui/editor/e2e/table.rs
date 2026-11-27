// The table editor on a real document: `cargo test --release the_table_editor -- --ignored
// --nocapture`.
use super::{calm, open, scratch, wait};
use crate::document::table::{self, Borders};
use crate::document::table_lines;
use crate::ui::editor::table_panel::Op;
use gpui_kit::{EntityInputHandler, Keystroke, TestAppContext};

const DOC: &str = "\\documentclass{article}\n\\begin{document}\nBefore.\n\n\
\\begin{tabular}{|l|l|}\n\\hline\na & b \\\\\n\\hline\nc & d \\\\\n\\hline\n\\end{tabular}\n\nAfter.\n\\end{document}\n";

#[gpui_kit::test]
#[ignore]
fn the_table_editor(cx: &mut TestAppContext) {
    let (doc, cx) = open(cx, scratch("table", DOC), "main.tex", 2);
    let src = |cx: &mut gpui_kit::VisualTestContext| cx.read(|cx| doc.read(cx).source.clone());
    let cells = |cx: &mut gpui_kit::VisualTestContext| {
        let s = src(cx);
        let t = table::find(&s, 0..s.len()).unwrap();
        t.rows
            .iter()
            .map(|r| {
                r.cells
                    .iter()
                    .map(|c| s[c.clone()].to_string())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    let bi = cx.read(|cx| {
        let d = doc.read(cx);
        d.blocks
            .iter()
            .position(|b| d.source[b.range.clone()].contains("tabular"))
            .unwrap()
    });
    doc.update_in(cx, |d, window, cx| assert!(d.open_table(bi, window, cx)));
    cx.run_until_parked();
    cx.read(|cx| {
        let d = doc.read(cx);
        assert!(
            d.active
                .as_ref()
                .is_some_and(|a| a.panel == Some(crate::ui::editor::Panel::Table))
                && d.box_place().is_none()
        );
        assert_eq!(d.table.as_ref().map(|t| t.cell_count()), Some(4));
    });
    // a cell typed in goes into the source; one holding an `&` waits
    let set = |cx: &mut gpui_kit::VisualTestContext, r: usize, c: usize, v: &str| {
        let v = v.to_string();
        doc.update_in(cx, |d, window, cx| {
            let cell = d.table.as_ref().unwrap().cell(r, c);
            cell.update(cx, |s, cx| {
                s.set_value(String::new(), window, cx);
                s.replace_text_in_range(None, &v, window, cx);
            });
        });
        cx.run_until_parked();
    };
    set(cx, 1, 1, "x & y");
    assert_eq!(cells(cx)[1], vec!["c", "d"]);
    set(cx, 1, 1, "dee");
    assert_eq!(cells(cx)[1], vec!["c", "dee"]);
    // rows and columns next to the focused cell (the first)
    doc.update_in(cx, |d, window, cx| d.table_op(Op::RowBelow, window, cx));
    assert_eq!(cells(cx).len(), 3);
    assert_eq!(cells(cx)[1], vec!["", ""]);
    doc.update_in(cx, |d, window, cx| d.table_op(Op::ColRight, window, cx));
    assert!(src(cx).contains("{|l|l|l|}"), "{}", src(cx));
    assert_eq!(cells(cx)[0].len(), 3);
    wait(cx, &doc, "the table typeset", |d| d.spans_pending == 0);
    // the focused column's alignment, a bold header, and each way of ruling: all typeset
    doc.update_in(cx, |d, window, cx| d.table_align('r', window, cx));
    assert!(src(cx).contains("{|l|r|l|}"), "{}", src(cx));
    doc.update(cx, |d, cx| d.table_header(cx));
    assert!(
        src(cx).contains("\\textbf{a} & & \\textbf{b}"),
        "{}",
        src(cx)
    );
    for style in [
        Borders::Booktabs,
        Borders::None,
        Borders::Lines,
        Borders::Grid,
    ] {
        doc.update(cx, |d, cx| d.table_borders(style, cx));
        wait(cx, &doc, "the table typeset", |d| d.spans_pending == 0);
        calm(cx, &doc);
        let status = cx.read(|cx| doc.read(cx).status.clone());
        assert!(!status.contains("LaTeX"), "{style:?}: {status}");
        let s = src(cx);
        let t = table::find(&s, 0..s.len()).unwrap();
        assert_eq!(t.borders(&s), style, "{s}");
        if style == Borders::Booktabs {
            assert!(s.contains("\\usepackage{booktabs}"), "{s}");
        }
    }
    // Escape closes it; one undo takes every change back
    doc.update_in(cx, |d, window, cx| {
        d.on_table_key(&Keystroke::parse("escape").unwrap(), window, cx)
    });
    cx.read(|cx| {
        let d = doc.read(cx);
        assert!(d.active.is_none() && d.table.is_none());
    });
    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    assert_eq!(src(cx), DOC);
}

/// A column's width and a row's room are dragged by their edges, a cell is made bold and shaded;
/// the table typesets each time, and the whole drag is one undo step. `cargo test --release
/// the_table_sizes -- --ignored --nocapture`.
#[gpui_kit::test]
#[ignore]
fn the_table_sizes(cx: &mut TestAppContext) {
    use gpui_kit::{point, px};
    let (doc, cx) = open(cx, scratch("table-sizes", DOC), "main.tex", 2);
    let src = |cx: &mut gpui_kit::VisualTestContext| cx.read(|cx| doc.read(cx).source.clone());
    let before = src(cx);
    let bi = cx.read(|cx| {
        let d = doc.read(cx);
        d.blocks
            .iter()
            .position(|b| d.source[b.range.clone()].contains("tabular"))
            .unwrap()
    });
    doc.update_in(cx, |d, window, cx| assert!(d.open_table(bi, window, cx)));
    cx.run_until_parked();
    // the second column's edge pulled 60 px to the right of where it starts (112 px)
    doc.update_in(cx, |d, _, cx| {
        d.table_drag_start(Some(1), None, point(px(300.0), px(50.0)), cx);
        d.table_drag_to(point(px(360.0), px(50.0)), cx);
        d.table_drag_end(cx);
    });
    cx.run_until_parked();
    let s = src(cx);
    assert!(
        s.contains("{|l|>{\\raggedright\\arraybackslash}p{") && s.contains("\\usepackage{array}"),
        "{s}"
    );
    calm(cx, &doc);
    // the first row given room under it
    doc.update_in(cx, |d, _, cx| {
        d.table_drag_start(None, Some(0), point(px(300.0), px(100.0)), cx);
        d.table_drag_to(point(px(300.0), px(112.0)), cx);
        d.table_drag_end(cx);
    });
    cx.run_until_parked();
    assert!(
        src(cx).contains("a & b \\\\[") || src(cx).contains("pt]"),
        "{}",
        src(cx)
    );
    calm(cx, &doc);
    // a cell bold and shaded
    doc.update_in(cx, |d, window, cx| {
        d.focus_cell((1, 0), window, cx);
        d.table_cell_format(|t| table::toggle_cell(t, "textbf"), &[], window, cx);
        d.table_cell_format(
            |t| table::shade_cell(t, Some("yellow!35")),
            &["colortbl", "xcolor"],
            window,
            cx,
        );
    });
    cx.run_until_parked();
    let s = src(cx);
    assert!(s.contains("\\cellcolor{yellow!35}\\textbf{c}"), "{s}");
    assert!(
        s.contains("\\usepackage{colortbl}") && s.contains("\\usepackage{xcolor}"),
        "{s}"
    );
    calm(cx, &doc);
    let status = cx.read(|cx| doc.read(cx).status.clone());
    assert!(!status.contains("LaTeX"), "{status}");
    let _ = before;
}

/// A selected table's lines are dragged on the page: a column's line sets its width, a row's the
/// room under it; the table typesets each time and each drag is one undo step. `cargo test
/// --release the_table_lines -- --ignored --nocapture`.
#[gpui_kit::test]
#[ignore]
fn the_table_lines(cx: &mut TestAppContext) {
    use crate::ui::editor::table_handles::Edge;
    use gpui_kit::{point, px};
    let (doc, cx) = open(cx, scratch("table-lines", DOC), "main.tex", 2);
    let src = |cx: &mut gpui_kit::VisualTestContext| cx.read(|cx| doc.read(cx).source.clone());
    let select = |cx: &mut gpui_kit::VisualTestContext| {
        let off = cx.read(|cx| {
            let d = doc.read(cx);
            d.blocks
                .iter()
                .position(|b| d.source[b.range.clone()].contains("tabular"))
                .unwrap()
        });
        doc.update(cx, |d, cx| d.set_selection(off, off, cx));
    };
    // the pointer position (window coordinates) of a spot on the page, sp
    let at = |cx: &mut gpui_kit::VisualTestContext, page: u32, x: f64, y: f64| {
        cx.read(|cx| {
            let d = doc.read(cx);
            let bounds = d.page_bounds.take();
            let o = bounds
                .iter()
                .find(|(n, _)| *n == page)
                .map(|(_, b)| b.origin)
                .unwrap();
            d.page_bounds.set(bounds);
            let px_of = |v: f64| px(crate::ui::editor::page_px(v, d.zoom));
            point(o.x + px_of(x), o.y + px_of(y))
        })
    };
    calm(cx, &doc);
    select(cx);
    let lines = |cx: &mut gpui_kit::VisualTestContext| {
        cx.read(|cx| doc.read(cx).table_lines())
            .expect("the table's lines")
    };
    let (page, first) = lines(cx);
    assert_eq!(first.cols.len(), 2, "{first:?}");
    assert_eq!(first.rows.len(), 2, "{first:?}");
    // the first column's line 40 pt to the right
    let (x, y) = (first.cols[0].x, (first.bbox[1] + first.bbox[3]) / 2.0);
    let grab = at(cx, page, x, y);
    let to = at(cx, page, x + 40.0 * 65536.0, y);
    doc.update(cx, |d, cx| {
        let from = d.hit(page, grab).unwrap().0;
        d.hold_line(page, Edge::Col(0), from, first.cols[0].width, cx);
        d.line_drag_to(page, to, cx);
        d.line_drag_end(cx);
    });
    let s = src(cx);
    assert!(
        s.contains("{|>{\\raggedright\\arraybackslash}p{") && s.contains("\\usepackage{array}"),
        "{s}"
    );
    calm(cx, &doc);
    let (_, wider) = lines(cx);
    let grew = wider.cols[0].width - first.cols[0].width;
    assert!((grew - 40.0).abs() < 2.0, "{grew} pt: {wider:?}");
    // the drag was one undo step
    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    assert_eq!(src(cx), DOC);
    calm(cx, &doc);
    select(cx);
    // the first row's line 10 pt down: room under it
    let (page, first) = lines(cx);
    let (x, y) = ((first.bbox[0] + first.bbox[2]) / 2.0, first.rows[0].y);
    let grab = at(cx, page, x, y);
    let to = at(cx, page, x, y + 10.0 * 65536.0);
    doc.update(cx, |d, cx| {
        let from = d.hit(page, grab).unwrap().1;
        d.hold_line(page, Edge::Row(0), from, 0.0, cx);
        d.line_drag_to(page, to, cx);
        d.line_drag_end(cx);
    });
    assert!(src(cx).contains("a & b \\\\[10pt]"), "{}", src(cx));
    calm(cx, &doc);
    let (_, taller) = lines(cx);
    // the second row's baseline is 10 pt further from the first's (a table in a line is centred
    // on it, so both rows move)
    let gap = |l: &table_lines::Lines| {
        (l.rows[1].y - (l.rows[0].y - l.rows[0].space * 65536.0)) / 65536.0
    };
    let grew = gap(&taller) - gap(&first);
    assert!((grew - 10.0).abs() < 2.0, "{grew} pt: {taller:?}");
    let status = cx.read(|cx| doc.read(cx).status.clone());
    assert!(!status.contains("LaTeX"), "{status}");
    // a click that moves nothing writes nothing and leaves no undo step
    let before = src(cx);
    let steps = cx.read(|cx| doc.read(cx).undo.len());
    doc.update(cx, |d, cx| {
        d.hold_line(page, Edge::Col(1), 0.0, 10.0, cx);
        d.line_drag_end(cx);
    });
    assert_eq!(src(cx), before);
    assert_eq!(cx.read(|cx| doc.read(cx).undo.len()), steps);
}
