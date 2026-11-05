// What each frame costs: the ribbon's state, the sidebar's fields and a redraw of the pages, on a
// real document and on one ten times its length.
use super::{open, rtd_type, scratch};
use gpui_kit::TestAppContext;
use std::time::Instant;

/// `RTD_TYPE=/abs/main.tex cargo test --release frame_costs -- --ignored --nocapture`.
#[gpui_kit::test]
#[ignore]
fn frame_costs(cx: &mut TestAppContext) {
    let (root, main) = rtd_type();
    let tex = std::fs::read_to_string(root.join(&main)).unwrap();
    // ten times the body
    let (head, rest) = tex.split_once("\\begin{document}").unwrap();
    let (body, _) = rest.rsplit_once("\\end{document}").unwrap();
    let big = format!(
        "{head}\\begin{{document}}{}\\end{{document}}\n",
        body.replace("\\label{", "\\label{x").repeat(10)
    );
    for (name, dir, file) in [
        ("document", root.clone(), main.clone()),
        (
            "10x document",
            scratch("perf", &big),
            "main.tex".to_string(),
        ),
    ] {
        let (doc, cx) = open(cx, dir, &file, 3);
        let off = cx.read(|cx| {
            let d = doc.read(cx);
            d.source
                .find("Used together")
                .or(d.source.find("\\section"))
                .unwrap()
                + 5
        });
        doc.update(cx, |d, cx| d.set_caret(off, cx));
        cx.run_until_parked();
        // Fit Width: the widest page as wide as the column, less the gap either side
        doc.update_in(cx, |d, window, cx| d.fit_width(window, cx));
        cx.run_until_parked();
        cx.read(|cx| {
            let d = doc.read(cx);
            let widest = d
                .preview
                .pages
                .values()
                .filter_map(|p| p.page_width)
                .fold(0.0, f64::max);
            let column =
                f32::from(d.scroll.bounds().size.width) - 2.0 * crate::ui::editor::PAGE_GAP;
            let page = crate::ui::editor::page_px(widest, d.zoom);
            assert!(
                (page - column).abs() < 2.0 || d.zoom <= 0.25 || d.zoom >= 4.0,
                "{page} vs {column}"
            );
        });
        let n = 200;
        let t = Instant::now();
        for _ in 0..n {
            cx.read(|cx| {
                let d = doc.read(cx);
                std::hint::black_box(d.ribbon_state());
                std::hint::black_box(d.element_fields());
            });
        }
        let state = t.elapsed() / n;
        // a redraw of the pages, as a caret blink asks for
        let t = Instant::now();
        for _ in 0..n {
            doc.update(cx, |_, cx| cx.notify());
            cx.run_until_parked();
        }
        let frame = t.elapsed() / n;
        let (pages, blocks, bytes) = cx.read(|cx| {
            let d = doc.read(cx);
            (d.preview.pages_total, d.blocks.len(), d.source.len())
        });
        println!(
            "{name}: {pages} pages, {blocks} blocks, {bytes} bytes: ribbon + sidebar {state:?}, redraw {frame:?}"
        );
    }
}
