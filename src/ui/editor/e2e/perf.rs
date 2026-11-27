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

/// Where the caret is drawn at each step after an edit, until rtex has answered: it stays on its
/// line and moves by a few points at a time, never back to a word's or a line's start. `cargo test --release caret_follows_edits --
/// --ignored --nocapture`.
#[gpui_kit::test]
#[ignore]
fn caret_follows_edits(cx: &mut TestAppContext) {
    let words = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega ";
    let tex = format!(
        "\\documentclass{{article}}\n\\begin{{document}}\n{}\n\n\\end{{document}}\n",
        words.repeat(6)
    );
    let (doc, cx) = open(cx, scratch("caret", &tex), "main.tex", 0);
    let at = cx.read(|cx| doc.read(cx).source.find("kappa").unwrap() + 3);
    doc.update_in(cx, |d, window, cx| {
        d.set_caret(at, cx);
        d.focus.focus(window, cx);
    });
    // where the pages draw the caret: at the overlay holding it, else at its stop
    let spot = |cx: &mut gpui_kit::VisualTestContext| {
        cx.read(|cx| {
            let at = doc.read(cx).caret_spot();
            at.map(|(p, x, y, ..)| (p, (x / 65536.0).round(), (y / 65536.0).round()))
        })
    };
    let mut prev = spot(cx).unwrap();
    println!("start {prev:?}");
    for (what, edit) in [
        ("backspace", 0),
        ("backspace", 0),
        ("type a", 1),
        ("type b", 1),
        ("delete", 2),
    ] {
        doc.update_in(cx, |d, window, cx| match edit {
            0 => d.delete(true, cx),
            1 => {
                use gpui_kit::EntityInputHandler;
                d.replace_text_in_range(None, if what == "type a" { "a" } else { "b" }, window, cx)
            }
            _ => d.delete(false, cx),
        });
        let t = std::time::Instant::now();
        // (when, the caret's page and position then)
        type Seen = (u128, Option<(u32, f64, f64)>);
        let mut seen: Vec<Seen> = vec![];
        loop {
            let now = spot(cx);
            if seen.last().is_none_or(|l| l.1 != now) {
                seen.push((t.elapsed().as_millis(), now));
            }
            let settled = cx.read(|cx| doc.read(cx).spans_pending == 0);
            if (settled && t.elapsed().as_millis() > 150) || t.elapsed().as_secs() > 5 {
                break;
            }
            cx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        println!("{what}: {seen:?}");
        for (_, now) in seen {
            let now = now.unwrap();
            assert!(
                now.0 == prev.0 && now.2 == prev.2 && (now.1 - prev.1).abs() < 12.0,
                "{what}: the caret jumped from {prev:?} to {now:?}"
            );
            prev = now;
        }
    }
}
