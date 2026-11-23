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
