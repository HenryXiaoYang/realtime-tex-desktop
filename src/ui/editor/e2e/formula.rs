// The formula editor and Enter's line breaks on a real document: `cargo test --release
// the_formula_editor -- --ignored --nocapture`.
use super::{open, scratch, wait};
use crate::ui::settings::AppSettings;
use gpui_kit::{EntityInputHandler, TestAppContext};

const DOC: &str = "\\documentclass{article}\n\\begin{document}\n\
Euler says $e^{i\\pi}+1=0$ and it is famous.\n\n\\[\n  x^2\n\\]\n\nLast one here.\n\\end{document}\n";

#[gpui_kit::test]
#[ignore]
fn the_formula_editor(cx: &mut TestAppContext) {
    let (doc, cx) = open(cx, scratch("formula", DOC), "main.tex", 2);
    let src = |cx: &mut gpui_kit::VisualTestContext| cx.read(|cx| doc.read(cx).source.clone());
    // a dollar sign at the caret opens an empty formula, the page still typeset
    let off = src(cx).find(" and it").unwrap();
    doc.update_in(cx, |d, window, cx| {
        d.set_caret(off, cx);
        d.replace_text_in_range(None, "$", window, cx);
    });
    cx.run_until_parked();
    assert!(src(cx).contains("famous") && src(cx).contains("$e^{i\\pi}+1=0$ $ $ and"));
    cx.read(|cx| {
        let d = doc.read(cx);
        assert!(
            d.active
                .as_ref()
                .is_some_and(|a| a.panel == Some(crate::ui::editor::Panel::Formula)),
            "the formula editor is open"
        );
        assert!(d.box_place().is_none(), "no box replaces the lines");
        assert!(
            d.formula_anchor(
                d.active.as_ref().unwrap().range.start,
                d.active.as_ref().unwrap().panel
            )
            .is_some()
        );
    });
    // typing in it writes the formula; a symbol goes in at the cursor
    doc.update_in(cx, |d, window, cx| {
        d.input
            .update(cx, |s, cx| s.replace_text_in_range(None, "x^2", window, cx));
    });
    cx.run_until_parked();
    doc.update_in(cx, |d, window, cx| {
        d.insert_symbol(r"\frac{‸}{}", window, cx)
    });
    cx.run_until_parked();
    assert!(src(cx).contains("$x^2\\frac{}{}$"), "{}", src(cx));
    wait(cx, &doc, "the formula typeset", |d| d.spans_pending == 0);
    // a second dollar sign closes it, the caret after the formula
    doc.update_in(cx, |d, window, cx| {
        d.input
            .update(cx, |s, cx| s.replace_text_in_range(None, "$", window, cx));
    });
    cx.run_until_parked();
    let after = src(cx);
    assert!(after.contains("$ $x^2\\frac{}{}$ and"), "{after}");
    assert!(!after.contains("\\$"), "no escaped dollar");
    cx.read(|cx| {
        let d = doc.read(cx);
        assert!(d.active.is_none());
        let end = after.find("{}$ and").unwrap() + "{}$".len();
        assert_eq!(d.caret.as_ref().map(|c| c.off), Some(end));
    });
    // a formula left blank goes with its dollar signs
    let last = after.find("Last one").unwrap();
    doc.update_in(cx, |d, window, cx| {
        d.set_caret(last, cx);
        d.replace_text_in_range(None, "$", window, cx);
        d.finish_formula(window, cx);
    });
    assert_eq!(src(cx), after);
    // an existing display formula opens by its TeX alone
    let inside = src(cx).find("x^2\n").unwrap();
    doc.update_in(cx, |d, window, cx| {
        assert!(d.open_formula(inside, inside, false, window, cx));
        assert_eq!(d.input.read(cx).value().as_ref(), "\n  x^2\n");
        d.finish_formula(window, cx);
    });
    assert_eq!(src(cx), after);
    // Enter in running text: two line breaks, no blank line; Shift-Enter: one
    cx.update(|_, cx| {
        let mut s = AppSettings::get(cx).clone();
        s.enter_no_indent = true;
        cx.set_global(s);
    });
    let at = src(cx).find("Last one").unwrap() + "Last".len();
    doc.update(cx, |d, cx| {
        d.set_caret(at, cx);
        d.enter(false, cx);
    });
    assert!(src(cx).contains("Last \\\\\\\\\none here"), "{}", src(cx));
    wait(cx, &doc, "the break typeset", |d| d.spans_pending == 0);
    let here = src(cx).find("here.").unwrap();
    doc.update(cx, |d, cx| {
        d.set_caret(here, cx);
        d.enter(true, cx);
    });
    assert!(src(cx).contains("one \\\\\nhere."), "{}", src(cx));
}

const TIKZ_DOC: &str = "\\documentclass{article}\n\\usepackage{tikz}\n\\begin{document}\nBefore.\n\n\
\\begin{tikzpicture}\n  \\draw (0,0) -- (1,1);\n\\end{tikzpicture}\n\nAfter.\n\\end{document}\n";

/// A TikZ picture opens in the source panel and the Insert menu's templates go in whole.
#[gpui_kit::test]
#[ignore]
fn the_tikz_panel(cx: &mut TestAppContext) {
    let (doc, cx) = open(cx, scratch("tikz", TIKZ_DOC), "main.tex", 2);
    let bi = cx.read(|cx| {
        let d = doc.read(cx);
        d.blocks
            .iter()
            .position(|b| d.source[b.range.clone()].contains("tikzpicture"))
            .unwrap()
    });
    doc.update_in(cx, |d, window, cx| {
        assert!(d.open_source_panel(bi, window, cx))
    });
    cx.run_until_parked();
    cx.read(|cx| {
        let d = doc.read(cx);
        let a = d.active.as_ref().unwrap();
        assert_eq!(a.panel, Some(crate::ui::editor::Panel::Source));
        assert!(d.box_place().is_none(), "the picture stays on the page");
        assert!(d.input.read(cx).value().contains("\\draw (0,0) -- (1,1);"));
        assert!(d.formula_anchor(a.range.start, a.panel).is_some());
    });
    // what is typed goes into the source; Escape closes the panel
    doc.update_in(cx, |d, window, cx| {
        let end = d.input.read(cx).value().len();
        d.input.update(cx, |s, cx| {
            s.set_selected_range(end..end, cx);
            s.replace_text_in_range(None, "\n% more", window, cx)
        });
    });
    cx.run_until_parked();
    assert!(cx.read(|cx| doc.read(cx).source.contains("\\end{tikzpicture}\n% more")));
    doc.update_in(cx, |d, window, cx| {
        d.on_box_key(&gpui_kit::Keystroke::parse("escape").unwrap(), window, cx)
    });
    cx.read(|cx| assert!(doc.read(cx).active.is_none()));
    // a template goes in with its package
    doc.update_in(cx, |d, window, cx| {
        let off = d.source.find("After.").unwrap();
        d.set_caret(off, cx);
        d.insert_block(crate::document::insert::TIKZ[1].1, &["tikz"], cx);
        let _ = window;
    });
    let s = cx.read(|cx| doc.read(cx).source.clone());
    assert!(s.contains("plot (\\x, {0.25*\\x*\\x})"), "{s}");
    assert_eq!(
        s.matches("\\usepackage{tikz}").count(),
        1,
        "no second package line"
    );
    wait(cx, &doc, "the picture typeset", |d| {
        d.spans_pending == 0 && !d.compiling
    });
}

/// A TikZ picture's points are dots on a canvas: dragging one moves its numbers in the source,
/// and the whole drag undoes in one step.
#[gpui_kit::test]
#[ignore]
fn the_tikz_points(cx: &mut TestAppContext) {
    use gpui_kit::{point, px};
    let (doc, cx) = open(cx, scratch("tikz-points", TIKZ_DOC), "main.tex", 2);
    let src = |cx: &mut gpui_kit::VisualTestContext| cx.read(|cx| doc.read(cx).source.clone());
    let bi = cx.read(|cx| {
        let d = doc.read(cx);
        d.blocks
            .iter()
            .position(|b| d.source[b.range.clone()].contains("tikzpicture"))
            .unwrap()
    });
    let before = src(cx);
    doc.update_in(cx, |d, window, cx| {
        assert!(d.open_source_panel(bi, window, cx))
    });
    cx.run_until_parked();
    // two points, a unit apart: 80 px to a unit, so 40 px right and 20 px up are 0.5 and 0.25
    doc.update_in(cx, |d, window, cx| {
        d.point_down(0, point(px(100.0), px(100.0)), 600.0, cx);
        d.point_to(point(px(140.0), px(80.0)), window, cx);
        d.point_end(cx);
    });
    cx.run_until_parked();
    assert!(
        src(cx).contains("\\draw (0.5,0.25) -- (1,1);"),
        "{}",
        src(cx)
    );
    cx.read(|cx| assert!(doc.read(cx).input.read(cx).value().contains("(0.5,0.25)")));
    // the panel closed: one undo takes the picture back
    doc.update_in(cx, |d, window, cx| {
        d.on_box_key(&gpui_kit::Keystroke::parse("escape").unwrap(), window, cx)
    });
    cx.run_until_parked();
    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    cx.run_until_parked();
    assert_eq!(src(cx), before);
}
