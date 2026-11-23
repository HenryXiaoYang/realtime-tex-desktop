// The sidebar's page setup on a real document: `cargo test --release the_page_setup -- --ignored
// --nocapture`.
use super::{open, scratch, wait};
use gpui_kit::TestAppContext;

const DOC: &str = "\\documentclass[a4paper]{article}\n\\usepackage{amsmath}\n\\newcommand{\\R}{\\mathbb{R}}\n\\begin{document}\nHello $\\R$.\n\\end{document}\n";

#[gpui_kit::test]
#[ignore]
fn the_page_setup(cx: &mut TestAppContext) {
    let (doc, cx) = open(cx, scratch("page-setup", DOC), "main.tex", 0);
    let src = |cx: &mut gpui_kit::VisualTestContext| cx.read(|cx| doc.read(cx).source.clone());
    let status = |cx: &mut gpui_kit::VisualTestContext| cx.read(|cx| doc.read(cx).status.clone());
    wait(cx, &doc, "the page typeset", |d| d.spans_pending == 0);
    // a package added and one removed; each typesets, one undo takes each back
    doc.update(cx, |d, cx| d.add_package("booktabs", cx));
    assert_eq!(
        cx.read(|cx| doc.read(cx).packages()),
        ["amsmath", "booktabs"]
    );
    doc.update(cx, |d, cx| d.remove_package("amsmath", cx));
    assert_eq!(cx.read(|cx| doc.read(cx).packages()), ["booktabs"]);
    assert!(src(cx).contains("\\newcommand{\\R}"), "macros stay");
    // margins and orientation through the document's fields
    doc.update(cx, |d, cx| {
        d.set_element_field("margin", "3cm", cx);
        d.set_element_field("top", "1in", cx);
        d.set_element_field("landscape", "true", cx);
    });
    wait(cx, &doc, "the page typeset", |d| d.spans_pending == 0);
    let s = src(cx);
    assert!(s.contains("[margin=3cm, top=1in]{geometry}"), "{s}");
    assert!(
        s.contains("\\documentclass[a4paper, landscape]{article}"),
        "{s}"
    );
    assert!(!status(cx).contains("LaTeX"), "{}", status(cx));
    for _ in 0..5 {
        doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    }
    assert_eq!(src(cx), DOC);
}
