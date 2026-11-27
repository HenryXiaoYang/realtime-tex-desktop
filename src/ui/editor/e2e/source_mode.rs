// Switching to Source mode opens the code editor where the pages were being edited: `cargo test
// --release source_mode_opens_where_you_were -- --ignored --nocapture`.
use super::{open, scratch};
use gpui_kit::{TestAppContext, point, px};

#[gpui_kit::test]
#[ignore]
fn source_mode_opens_where_you_were(cx: &mut TestAppContext) {
    let mut tex = String::from("\\documentclass{article}\n\\begin{document}\n");
    for i in 0..60 {
        tex.push_str(&format!(
            "Paragraph {i}: the quick brown fox jumps over the lazy dog.\n\n"
        ));
    }
    tex.push_str("\\end{document}\n");
    let (doc, cx) = open(cx, scratch("source-mode", &tex), "main.tex", 50);
    let code_at = |cx: &mut gpui_kit::VisualTestContext| {
        cx.read(|cx| {
            let d = doc.read(cx);
            let (r, scroll) = d
                .code
                .read_with(cx, |s, _| (s.selected_range(), s.scroll_offset().y));
            (r.start, scroll)
        })
    };
    let settle = |cx: &mut gpui_kit::VisualTestContext| {
        for _ in 0..5 {
            cx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
    };

    // a block being edited: the code editor's caret is where the box's was, and is in view
    let start = cx.read(|cx| doc.read(cx).blocks[45].range.start);
    doc.update_in(cx, |d, window, cx| d.activate(45, 7, window, cx));
    doc.update_in(cx, |d, window, cx| d.set_source_mode(true, window, cx));
    settle(cx);
    let (at, scroll) = code_at(cx);
    assert_eq!(at, start + 7);
    assert!(scroll < px(0.0), "the editor scrolled to it ({scroll:?})");

    // back on the pages, scrolled with nothing being edited: the first block in view
    doc.update_in(cx, |d, window, cx| d.set_source_mode(false, window, cx));
    settle(cx);
    doc.update_in(cx, |d, _, cx| {
        d.caret = None;
        d.scroll.set_offset(point(px(0.0), px(-4000.0)));
        cx.notify();
    });
    settle(cx);
    doc.update_in(cx, |d, window, cx| d.set_source_mode(true, window, cx));
    settle(cx);
    let (at, scroll) = code_at(cx);
    let first = cx.read(|cx| {
        let d = doc.read(cx);
        d.blocks.iter().position(|b| b.range.start == at)
    });
    assert!(
        first.is_some_and(|b| b > 5),
        "a block far down, not the top: {at}"
    );
    assert!(scroll < px(0.0), "the editor scrolled to it ({scroll:?})");
}

// Source mode shows the pages beside the source: the caret on them follows the code editor's
// cursor, and a click on them puts that cursor on the text clicked: `cargo test --release
// source_mode_previews_beside_the_source -- --ignored --nocapture`.
#[gpui_kit::test]
#[ignore]
fn source_mode_previews_beside_the_source(cx: &mut TestAppContext) {
    let mut tex = String::from("\\documentclass{article}\n\\begin{document}\n");
    for i in 0..60 {
        tex.push_str(&format!(
            "Paragraph {i}: the quick brown fox jumps over the lazy dog.\n\n"
        ));
    }
    tex.push_str("\\end{document}\n");
    let (doc, cx) = open(cx, scratch("source-preview", &tex), "main.tex", 50);
    let settle = |cx: &mut gpui_kit::VisualTestContext| {
        for _ in 0..5 {
            cx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
    };
    let cursor = |cx: &mut gpui_kit::VisualTestContext| {
        cx.read(|cx| doc.read(cx).code.read_with(cx, |s, _| s.cursor()))
    };
    doc.update_in(cx, |d, window, cx| d.set_source_mode(true, window, cx));
    settle(cx);

    // the pages are drawn, right of the editor
    let first = cx.read(|cx| {
        let cell = &doc.read(cx).page_bounds;
        let v = cell.take();
        cell.set(v.clone());
        v
    });
    let width = cx.update(|window, _| window.viewport_size().width);
    assert!(
        first
            .iter()
            .any(|(_, b)| b.origin.x > width / 2.0 - px(40.0)),
        "pages in the right half: {first:?}"
    );

    // a click on a word of paragraph 3 puts the code cursor in it
    let (page, bbox, range) = cx.read(|cx| {
        let d = doc.read(cx);
        let b = d
            .blocks
            .iter()
            .find(|b| d.source[b.range.clone()].starts_with("Paragraph 3:"))
            .unwrap();
        (b.rows[0].page, b.rows[0].bbox, b.range.clone())
    });
    let origin = first.iter().find(|(n, _)| *n == page).unwrap().1.origin;
    let zoom = cx.read(|cx| doc.read(cx).zoom);
    let at = |sp: f64| px(super::super::page_px(sp, zoom));
    let x = origin.x + at((bbox[0] + bbox[2]) / 2.0);
    let y = origin.y + at((bbox[1] + bbox[3]) / 2.0);
    cx.simulate_click(point(x, y), Default::default());
    settle(cx);
    let c = cursor(cx);
    assert!(
        range.start < c && c < range.end,
        "cursor {c} in paragraph 3 ({range:?})"
    );
    assert_eq!(
        cx.read(|cx| doc.read(cx).caret.as_ref().map(|c| c.off)),
        Some(c)
    );

    // the cursor moved far down in the editor: the caret follows and the pages scroll to it
    let far = cx.read(|cx| {
        let d = doc.read(cx);
        d.source.find("Paragraph 55:").unwrap() + 4
    });
    doc.update_in(cx, |d, _, cx| {
        d.code
            .update(cx, |s, cx| s.set_selected_range(far..far, cx))
    });
    settle(cx);
    let (off, scroll) = cx.read(|cx| {
        let d = doc.read(cx);
        (d.caret.as_ref().map(|c| c.off), d.scroll.offset().y)
    });
    assert_eq!(off, Some(far));
    let (top, view) = cx.read(|cx| {
        let d = doc.read(cx);
        let b = d.blocks.iter().find(|b| b.range.contains(&far)).unwrap();
        let r = &b.rows[0];
        (
            d.page_y(r.page) + super::super::page_px(r.bbox[3], d.zoom),
            d.scroll.bounds().size.height,
        )
    });
    assert!(
        px(top) + scroll <= view && px(top) + scroll > px(0.0),
        "the pages scrolled to it: line at {top:?}, scroll {scroll:?}, view {view:?}"
    );
    assert!(
        cx.read(|cx| doc.read(cx).caret_bounds.get()).is_some(),
        "the caret is drawn"
    );
}
