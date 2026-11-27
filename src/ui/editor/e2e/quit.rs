// What quitting does with "Compile when quitting": `cargo test --release quitting_compiles --
// --ignored --nocapture`.
use super::{calm, open, scratch, wait};
use gpui_kit::TestAppContext;

const DOC: &str = "\\documentclass{article}\n\\begin{document}\nHello.\n\\end{document}\n";

#[gpui_kit::test]
#[ignore]
fn quitting_compiles(cx: &mut TestAppContext) {
    let dir = scratch("quit", DOC);
    let (doc, cx) = open(cx, dir.clone(), "main.tex", 0);
    // an edit not yet on disk: saved, then the chosen number of full runs, each a new layout
    doc.update(cx, |d, cx| {
        let end = d.source.find("Hello.").unwrap();
        d.replace(end..end, "Unsaved. ", cx);
    });
    calm(cx, &doc);
    let before = cx.read(|cx| doc.read(cx).layout_version);
    assert!(doc.update(cx, |d, cx| d.compile_for_quit(3, cx)));
    assert!(cx.read(|cx| doc.read(cx).quit_compiling()));
    wait(cx, &doc, "the three runs", |d| !d.quit_compiling());
    let after = cx.read(|cx| doc.read(cx).layout_version);
    assert!(after >= before + 3, "three full runs: {before} → {after}");
    let disk = std::fs::read_to_string(dir.join("main.tex")).unwrap();
    assert!(disk.contains("Unsaved. Hello."), "saved first");

    // a run that fails ends them
    doc.update(cx, |d, cx| {
        let end = d.source.find("Hello.").unwrap();
        d.replace(end..end, "\\notamacro ", cx);
    });
    assert!(doc.update(cx, |d, cx| d.compile_for_quit(5, cx)));
    wait(cx, &doc, "the failed run to end them", |d| {
        !d.quit_compiling()
    });
}
