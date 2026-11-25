// Changes made to the file by someone else while it is open: `cargo test --release
// the_file_changes_under_us -- --ignored --nocapture`.
use super::{calm, open, scratch};
use gpui_kit::TestAppContext;

const DOC: &str = "\\documentclass{article}\n\\begin{document}\nFirst paragraph.\n\nSecond paragraph.\n\\end{document}\n";

#[gpui_kit::test]
#[ignore]
fn the_file_changes_under_us(cx: &mut TestAppContext) {
    let dir = scratch("disk", DOC);
    let file = dir.join("main.tex");
    let (doc, cx) = open(cx, dir, "main.tex", 1);
    let src = |cx: &mut gpui_kit::VisualTestContext| cx.read(|cx| doc.read(cx).source.clone());
    let look = |cx: &mut gpui_kit::VisualTestContext| {
        doc.update_in(cx, |d, window, cx| d.check_disk(window, cx));
        cx.run_until_parked();
    };
    // an agent edits the file; nothing is unsaved here: the pages follow, and rtex typesets it
    let theirs = DOC.replace(
        "Second paragraph.",
        "Second paragraph, as the agent wrote it.",
    );
    std::fs::write(&file, &theirs).unwrap();
    // not asked: the watch looks by itself
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(600));
    cx.run_until_parked();
    assert_eq!(src(cx), theirs, "the watch took the file in");
    cx.read(|cx| {
        let d = doc.read(cx);
        assert!(!d.dirty && d.conflict.is_none());
    });
    calm(cx, &doc);
    assert!(cx.read(|cx| {
        let d = doc.read(cx);
        d.blocks.iter().any(|b| {
            d.source[b.range.clone()].contains("as the agent wrote it") && !b.rows.is_empty()
        })
    }));
    // one undo takes the page back to what it was
    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    assert_eq!(src(cx), DOC);
    // an edit here, unsaved, and then the agent writes again: both are kept, nothing is lost
    let (mine, agent) = (
        DOC.replace("First paragraph.", "First paragraph, mine."),
        DOC.replace("Second paragraph.", "Second paragraph, agent's second go."),
    );
    doc.update_in(cx, |d, _, cx| {
        let at = d.source.find("First").unwrap();
        d.replace(
            at..at + "First paragraph.".len(),
            "First paragraph, mine.",
            cx,
        );
    });
    assert_eq!(src(cx), mine);
    std::fs::write(&file, &agent).unwrap();
    look(cx);
    cx.read(|cx| {
        let d = doc.read(cx);
        assert_eq!(d.conflict.as_deref(), Some(agent.as_str()));
        assert!(d.dirty);
        assert!(d.conflict_summary().unwrap().contains("differ"));
    });
    assert_eq!(src(cx), mine, "the page keeps the user's text");
    // a save while it is undecided writes nothing
    doc.update(cx, |d, cx| d.save(cx));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), agent);
    // keep mine: the next save replaces the file
    doc.update_in(cx, |d, window, cx| d.resolve_conflict(true, window, cx));
    doc.update(cx, |d, cx| d.save(cx));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), mine);
    // the agent again: this time the file's text is taken
    std::fs::write(&file, &agent).unwrap();
    look(cx);
    assert_eq!(src(cx), agent, "nothing unsaved: the file is taken at once");
    doc.update_in(cx, |d, _, cx| {
        let at = d.source.find("First").unwrap();
        d.replace(
            at..at + "First paragraph.".len(),
            "First, edited again.",
            cx,
        );
    });
    let third = agent.replace("Second paragraph, agent's second go.", "Third.");
    std::fs::write(&file, &third).unwrap();
    look(cx);
    assert!(cx.read(|cx| doc.read(cx).conflict.is_some()));
    doc.update_in(cx, |d, window, cx| d.resolve_conflict(false, window, cx));
    assert_eq!(src(cx), third);
    cx.read(|cx| assert!(!doc.read(cx).dirty && doc.read(cx).conflict.is_none()));
    // one undo brings the discarded edit back
    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    assert!(src(cx).contains("First, edited again."), "{}", src(cx));
}
