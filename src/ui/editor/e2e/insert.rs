// The Insert tab's edits typeset: at one place each, and everywhere in a document.
use super::{DOC, PNG, calm, open, rtd_type, scratch, src, typesets_and_undoes};
use crate::document::element::Kind;
use crate::document::insert;
use crate::ui::editor::Doc;
use gpui_kit::{EntityInputHandler, TestAppContext, Window};

/// Everything the Insert tab puts in, typeset by rtex: `RTD_TYPE=/abs/main.tex cargo test
/// --release insert_on_pages -- --ignored --nocapture` (a copy: it writes figures/ there).
#[gpui_kit::test]
#[ignore]
fn insert_on_pages(cx: &mut TestAppContext) {
    let (root, main) = rtd_type();
    let (doc, cx) = open(cx, root.clone(), &main, 3);
    calm(cx, &doc);
    let base = src(cx, &doc);
    let off = base.find("Used together").expect("the paragraph") + "Used ".len();
    let at = |d: &mut Doc, cx: &mut gpui_kit::Context<Doc>| d.set_caret(off, cx);

    // a page break splits the paragraph at the caret
    doc.update(cx, |d, cx| {
        at(d, cx);
        d.insert_block("\\newpage", &[], cx);
    });
    assert!(src(cx, &doc).contains("Used \n\n\\newpage\n\ntogether"));
    typesets_and_undoes(cx, &doc, &base, "page break");

    // a picture: copied into figures/, a figure with graphicx, the sidebar on it
    let pic = std::env::temp_dir().join("rtd test pic.png");
    std::fs::write(&pic, PNG).unwrap();
    doc.update(cx, |d, cx| {
        at(d, cx);
        d.insert_pictures(std::slice::from_ref(&pic), cx);
    });
    let s = src(cx, &doc);
    assert!(
        s.contains("\\includegraphics[width=0.8\\linewidth]{figures/rtd-test-pic.png}"),
        "{s}"
    );
    assert!(s.contains("\\label{fig:rtd-test-pic}"));
    assert!(s.contains("graphicx"));
    assert!(root.join("figures/rtd-test-pic.png").exists());
    calm(cx, &doc);
    assert_eq!(
        cx.read(|cx| doc.read(cx).element().kind),
        Kind::Figure,
        "the caret is in the new figure"
    );
    // a cross-reference to it
    doc.update(cx, |d, cx| {
        let l = d.labels().into_iter().find(|l| l.key == "fig:rtd-test-pic");
        let l = l.expect("its label");
        assert_eq!(l.kind, "Figure");
        // (graphicx went into the preamble, above it)
        let off = d
            .source
            .find("Used \n\n\\begin{figure}")
            .expect("the split")
            + 5;
        d.set_caret(off, cx);
        d.insert_inline(&l.reference(), &[], cx);
    });
    assert!(src(cx, &doc).contains("Used Figure~\\ref{fig:rtd-test-pic}\n\n\\begin{figure}"));
    calm(cx, &doc);
    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    typesets_and_undoes(cx, &doc, &base, "picture");
    let _ = std::fs::remove_dir_all(root.join("figures"));

    // a table
    doc.update(cx, |d, cx| {
        at(d, cx);
        d.insert_table(3, 3, cx);
    });
    assert!(src(cx, &doc).contains("\\begin{tabular}{|l|l|l|}"));
    typesets_and_undoes(cx, &doc, &base, "table");

    // a link over the selected word
    doc.update(cx, |d, cx| {
        at(d, cx);
        d.extending(cx, |d, cx| d.move_word(1, cx));
        assert_eq!(d.selected_text(), "together");
        // the dialog's text field starts as the selection's text
        d.insert_link("https://example.com/#x", "together", cx);
    });
    let s = src(cx, &doc);
    assert!(
        s.contains("\\href{https://example.com/\\#x}{together}"),
        "link"
    );
    assert!(s.contains("hyperref"));
    typesets_and_undoes(cx, &doc, &base, "link");
    // text typed over the selection's: the link shows it instead
    doc.update(cx, |d, cx| {
        at(d, cx);
        d.extending(cx, |d, cx| d.move_word(1, cx));
        d.insert_link("https://example.com", "a site", cx);
    });
    assert!(src(cx, &doc).contains("Used \\href{https://example.com}{a site}"));
    typesets_and_undoes(cx, &doc, &base, "link with its own text");

    // a footnote and a display formula open the source box at the caret
    doc.update_in(cx, |d, window, cx| {
        at(d, cx);
        d.insert_footnote(window, cx);
    });
    cx.run_until_parked();
    doc.update_in(cx, |d, window, cx| {
        assert!(d.active.is_some(), "footnote box");
        let v = d.input.read(cx).value().to_string();
        let cur = d.input.read(cx).cursor();
        assert!(
            v[..cur].ends_with("\\footnote{"),
            "the caret in the footnote"
        );
        d.leave(window, cx);
    });
    calm(cx, &doc);
    let s = src(cx, &doc);
    let i = s.find("Used ").unwrap();
    assert!(
        s.contains("Used \\footnote{}together"),
        "{:?}",
        &s[i..i + 40]
    );
    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    calm(cx, &doc);
    assert_eq!(src(cx, &doc), base, "footnote undone");
    // a display formula: an equation in the box, the caret in it
    doc.update_in(cx, |d, window, cx| {
        at(d, cx);
        d.insert_display_math(window, cx);
    });
    cx.run_until_parked();
    doc.update_in(cx, |d, window, cx| {
        let v = d.input.read(cx).value().to_string();
        let cur = d.input.read(cx).cursor();
        assert!(v[..cur].ends_with("\\begin{equation}\n  "), "{v:?}");
        d.input.update(cx, |s, cx| {
            s.replace_text_in_range(None, "E=mc^2", window, cx)
        });
    });
    cx.run_until_parked();
    doc.update_in(cx, |d, window, cx| d.leave(window, cx));
    calm(cx, &doc);
    let s = src(cx, &doc);
    assert!(
        s.contains("\\begin{equation}\n  E=mc^2\n\\end{equation}"),
        "equation"
    );
    let status = cx.read(|cx| doc.read(cx).status.clone());
    assert!(!status.contains("LaTeX"), "equation: {status}");
}

/// Every Insert-tab action at the start, middle and end of every text block of a document
/// with lists, tables, a minipage, formulas (or RTD_TYPE's): each typesets without a LaTeX
/// error and one undo restores the source. `cargo test --release insert_everywhere --
/// --ignored --nocapture` (RTD_SWEEP_ONLY=3:figure runs that one).
#[gpui_kit::test]
#[ignore]
fn insert_everywhere(cx: &mut TestAppContext) {
    // RTD_TYPE=/abs/main.tex sweeps that document instead (copied: it writes beside it)
    let fixture = std::env::var("RTD_TYPE")
        .map(|p| std::fs::read_to_string(p).unwrap())
        .unwrap_or_else(|_| DOC.into());
    let dir = scratch("insert", &fixture);
    let (doc, cx) = open(cx, dir.clone(), "main.tex", 3);
    calm(cx, &doc);
    let base = src(cx, &doc);
    let spots: Vec<(usize, String, [usize; 3])> = cx.read(|cx| {
        let d = doc.read(cx);
        (0..d.blocks.len())
            .filter(|&b| d.is_text_block(b))
            .filter_map(|b| {
                let page = d.blocks[b].rows.first()?.page;
                let st: Vec<usize> = d.map_of(b, page)?.stops.iter().map(|s| s.off).collect();
                let name: String = d.source[d.blocks[b].range.clone()]
                    .chars()
                    .take(30)
                    .collect();
                Some((b, name, [st[0], st[st.len() / 2], *st.last()?]))
            })
            .collect()
    });
    type Op = (
        &'static str,
        usize,
        fn(&mut Doc, &mut Window, &mut gpui_kit::Context<Doc>),
    );
    let ops: [Op; 9] = [
        ("pagebreak", 0, |d, _, cx| {
            d.insert_block("\\newpage", &[], cx)
        }),
        ("figure", 1, |d, _, cx| {
            d.insert_block(
                &insert::figure("pic.png", "fig:sweep", "Sweep"),
                &["graphicx"],
                cx,
            )
        }),
        ("table", 2, |d, _, cx| d.insert_table(2, 2, cx)),
        ("contents", 1, |d, _, cx| {
            d.insert_block("\\tableofcontents", &[], cx)
        }),
        ("ref", 1, |d, _, cx| {
            d.insert_inline("Figure~\\ref{fig:a}", &[], cx)
        }),
        ("cite-merge", 1, |d, _, cx| d.insert_citation("k", cx)),
        ("link", 1, |d, _, cx| {
            d.insert_link("https://example.com/a b", "site", cx)
        }),
        ("footnote", 1, |d, w, cx| d.insert_footnote(w, cx)),
        ("equation", 1, |d, w, cx| d.insert_display_math(w, cx)),
    ];
    let only = std::env::var("RTD_SWEEP_ONLY").ok();
    let (mut failures, mut n, t0) = (vec![], 0, std::time::Instant::now());
    for (bi, name, at) in &spots {
        for (op, pos, f) in &ops {
            if only
                .as_ref()
                .is_some_and(|o| !format!("{bi}:{op}").starts_with(o.as_str()))
            {
                continue;
            }
            n += 1;
            let what = format!("block {bi} {name:?}: {op}");
            doc.update_in(cx, |d, window, cx| {
                d.status.clear();
                d.set_caret(at[*pos], cx);
                f(d, window, cx);
            });
            cx.run_until_parked();
            // a box opened (footnote, equation): write in it and leave
            doc.update_in(cx, |d, window, cx| {
                if d.active.is_some() {
                    d.input
                        .update(cx, |s, cx| s.replace_text_in_range(None, "x", window, cx));
                }
            });
            cx.run_until_parked();
            doc.update_in(cx, |d, window, cx| {
                if d.active.is_some() {
                    d.leave(window, cx);
                }
            });
            calm(cx, &doc);
            let (s, status) = cx.read(|cx| {
                let d = doc.read(cx);
                (d.source.clone(), d.status.clone())
            });
            // a citation of a key no .bib has warns, it does not fail
            if status.contains("LaTeX") {
                let r = cx.read(|cx| doc.read(cx).last_edit.clone());
                let around = &s[r.start.saturating_sub(40).min(s.len())..(r.end + 40).min(s.len())];
                failures.push(format!("{what} → {status} (around {around:?})"));
            }
            if s == base {
                failures.push(format!("{what} → nothing went in"));
                continue;
            }
            doc.update_in(cx, |d, window, cx| d.undo(window, cx));
            calm(cx, &doc);
            if src(cx, &doc) != base {
                failures.push(format!("{what} → undo did not restore the source"));
                let b = base.clone();
                doc.update(cx, |d, cx| {
                    d.replace(0..d.source.len(), &b, cx);
                });
                calm(cx, &doc);
            }
        }
    }
    println!(
        "{n} insertions at {} spots in {:?}",
        spots.len(),
        t0.elapsed()
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
