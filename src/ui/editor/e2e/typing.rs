// Typing, editing keys, commands and the ribbon's formatting on a real document.
use super::{calm, open, rtd_type, scratch, wait};
use crate::document::{self, caret, glyphmap};
use crate::engine;
use crate::ui::editor::Doc;
use gpui_kit::{EntityInputHandler, TestAppContext};
use std::time::{Duration, Instant};

fn typed(doc: &Doc, bi: usize) -> String {
    let b = &doc.blocks[bi];
    let rows: Vec<_> = b.rows.iter().map(|r| &r.row).collect();
    glyphmap::glyph_text_of(&rows).text.iter().collect()
}

/// Typing on the pages of a real document through rtex: `RTD_TYPE=/abs/main.tex cargo test
/// --release type_on_pages -- --ignored --nocapture` (use a copy: rtex builds next to it).
#[gpui_kit::test]
#[ignore]
fn type_on_pages(cx: &mut TestAppContext) {
    let (root, main) = rtd_type();
    let (doc, cx) = open(cx, root, &main, 3);
    // a plain paragraph: the caret before its second word
    let (bi, off) = cx.read(|cx| {
        let d = doc.read(cx);
        let bi = (0..d.blocks.len())
            .find(|&b| {
                d.is_text_block(b)
                    && d.source[d.blocks[b].range.clone()].split(' ').count() > 6
                    && document::caret::is_prose(&d.source[d.blocks[b].range.clone()])
                    && !d.source[d.blocks[b].range.clone()].starts_with('\\')
            })
            .expect("a plain paragraph");
        let b = &d.blocks[bi];
        let src = &d.source[b.range.clone()];
        (bi, b.range.start + src.find(' ').unwrap() + 1)
    });
    let before = cx.read(|cx| doc.read(cx).source.clone());
    let page = cx.read(|cx| doc.read(cx).blocks[bi].rows[0].page);
    doc.update(cx, |d, cx| {
        let m = d.map_of(bi, page).unwrap();
        assert!(
            m.stops.iter().any(|s| s.off == off),
            "a stop before the second word"
        );
        d.set_caret(off, cx);
    });
    // the caret is drawn, with what was typed until the page catches up
    doc.update_in(cx, |d, window, cx| d.focus.focus(window, cx));
    let t0 = Instant::now();
    doc.update_in(cx, |d, window, cx| {
        for ch in "Quick 50% ".chars() {
            d.replace_text_in_range(None, &ch.to_string(), window, cx);
        }
    });
    let src = cx.read(|cx| doc.read(cx).source.clone());
    assert!(src.contains("Quick 50\\% "), "typed text is escaped");
    assert_eq!(
        cx.read(|cx| doc.read(cx).caret.as_ref().unwrap().off),
        off + 11
    );
    wait(cx, &doc, "the typeset paragraph", |d| {
        d.spans_pending == 0 && typed(d, bi).contains("Quick 50% ")
    });
    println!("typed → typeset in {:?}", t0.elapsed());
    wait(cx, &doc, "the overlay to go", |d| d.overlay.is_none());
    // the caret is drawn on its page, at its line
    doc.update(cx, |d, cx| {
        d.caret_bounds.set(None);
        cx.notify();
    });
    wait(cx, &doc, "the caret drawn", |d| {
        d.caret_bounds.get().is_some()
    });
    // typed text shows in the document's font, as the text before the caret is set: its
    // OpenType twin here (the test text system has no fonts to register it with)
    let twin = |d: &Doc| {
        let name = d.caret_font().and_then(|f| f.name).unwrap_or_default();
        let file = super::super::source_box::latin_modern(&name).expect("a Latin Modern twin");
        assert!(
            engine::kpsewhich(&file).is_some(),
            "{file} is in the TeX tree"
        );
        file
    };
    assert!(cx.read(|cx| twin(doc.read(cx))).ends_with("-regular.otf"));
    doc.update(cx, |d, cx| {
        // in the body (the preamble's \\textbf is not on the pages)
        let at = d.source.find("\\textbf{Mistakes").unwrap() + "\\textbf{".len() + 2;
        d.set_caret(at, cx);
    });
    assert!(
        cx.read(|cx| twin(doc.read(cx))).ends_with("-bold.otf"),
        "bold inside \\textbf"
    );
    // it blinks, and a move shows it again at once
    doc.update(cx, |d, cx| d.set_caret(off + 11, cx));
    assert!(cx.read(|cx| doc.read(cx).caret_on));
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    assert!(!cx.read(|cx| doc.read(cx).caret_on), "off after a blink");
    doc.update(cx, |d, cx| d.set_caret(off + 11, cx));
    assert!(cx.read(|cx| doc.read(cx).caret_on), "on again after a move");
    cx.read(|cx| {
        let d = doc.read(cx);
        let b = d.caret_bounds.get().unwrap();
        let (page, _, y, _) = d.caret_at().unwrap();
        let pages = d.page_bounds.take();
        d.page_bounds.set(pages.clone());
        let (_, pb) = pages.iter().find(|(n, _)| *n == page).unwrap();
        let want = pb.origin.y + gpui_kit::px(crate::ui::editor::page_px(y, d.zoom));
        assert!(
            (b.origin.y - want).abs() < gpui_kit::px(30.0) && b.origin.x > pb.origin.x,
            "caret at {b:?}, its line at y {want:?} on page {pb:?}"
        );
    });
    // the caret is on its text again, after what was typed
    cx.read(|cx| {
        let d = doc.read(cx);
        let (_, _, m, i) = d.caret_stop().unwrap();
        assert_eq!(m.stops[i].off, off + 11);
    });
    // one undo step per word: "Quick ", "50% "
    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    assert!(cx.read(|cx| doc.read(cx).source.contains("Quick ")));
    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    assert_eq!(cx.read(|cx| doc.read(cx).source.clone()), before);
    // Enter splits the paragraph; Backspace joins it again
    doc.update(cx, |d, cx| d.set_caret(off, cx));
    doc.update(cx, |d, cx| d.enter(false, cx));
    let split = cx.read(|cx| doc.read(cx).source.clone());
    assert_eq!(split.len(), before.len() + 2);
    assert_eq!(&split[off..off + 2], "\n\n");
    wait(cx, &doc, "two paragraphs", |d| {
        d.spans_pending == 0 && d.block_of(off + 2) == Some(bi + 1)
    });
    doc.update(cx, |d, cx| d.delete(true, cx));
    let joined = cx.read(|cx| doc.read(cx).source.clone());
    assert_eq!(joined.replace("  ", " "), before.replace("  ", " "));
    // an input method composes at the caret without touching the source, then commits
    let base = cx.read(|cx| doc.read(cx).source.clone());
    doc.update_in(cx, |d, window, cx| {
        d.replace_and_mark_text_in_range(None, "ni", None, window, cx);
        assert_eq!(d.source, base);
        assert!(d.marked_text_range(window, cx).is_some());
        d.replace_text_in_range(None, "你", window, cx);
        assert!(d.marked_text_range(window, cx).is_none());
    });
    assert_eq!(
        cx.read(|cx| doc.read(cx).source.len()),
        base.len() + "你".len()
    );
    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    assert_eq!(cx.read(|cx| doc.read(cx).source.clone()), base);
    wait(cx, &doc, "the undo typeset", |d| d.spans_pending == 0);
    // a backslash opens the paragraph's source with the command begun at the caret
    let base = cx.read(|cx| doc.read(cx).source.clone());
    doc.update_in(cx, |d, window, cx| {
        d.set_caret(off, cx);
        d.replace_text_in_range(None, "\\", window, cx);
    });
    cx.run_until_parked();
    doc.update_in(cx, |d, window, cx| {
        assert!(d.active.is_some(), "the source box is open");
        let v = d.input.read(cx).value().to_string();
        let cur = d.input.read(cx).cursor();
        assert!(
            !v.contains(crate::ui::editor::complete::MARK),
            "the caret mark is gone"
        );
        assert_eq!(&v[cur - 1..cur], "\\", "the caret after the backslash");
        assert!(!d.source.contains("textbackslash"));
        // a lone backslash waits in the box: it is half a command
        assert_eq!(d.source, base, "nothing in the source yet");
        d.leave(window, cx);
    });
    doc.update_in(cx, |d, window, cx| {
        while d.source != base {
            d.undo(window, cx);
        }
    });
    wait(cx, &doc, "the undo", |d| {
        d.spans_pending == 0 && !d.compiling
    });
    // Enter at the end of a list item adds one; Tab nests it; undo takes both back
    let item = cx.read(|cx| {
        let d = doc.read(cx);
        // the end of a list's last item, which has a line of its own for a new item
        d.source.find("\n\\end{itemize}").unwrap()
    });
    let base = cx.read(|cx| doc.read(cx).source.clone());
    // where the caret was, at the end of the item before
    let (_, x0, y0, _) = doc.update(cx, |d, cx| {
        d.set_caret(item, cx);
        let at = d.caret_at().unwrap();
        d.enter(false, cx);
        at
    });
    let one = cx.read(|cx| doc.read(cx).source.clone());
    assert!(
        one[item..].starts_with("\n    \\item "),
        "{:?}",
        &one[item..item + 20]
    );
    // the caret stands on the new item's line, not after the item before, at once and
    // once it is typeset
    let (_, _, y_now, _) = cx.read(|cx| doc.read(cx).caret_at().unwrap());
    assert!(y_now > y0, "below the item before at once: {y0} → {y_now}");
    calm(cx, &doc);
    let (_, x1, y1, size) = cx.read(|cx| doc.read(cx).caret_at().unwrap());
    assert!(y1 > y0 + size, "a line lower: {y0} → {y1}");
    assert!(x1 < x0, "at the line's start: {x0} → {x1}");
    // a click on the empty item's line puts the caret back in it
    let (bi, page) = cx.read(|cx| {
        let d = doc.read(cx);
        let bi = d.block_of(item).unwrap();
        (bi, d.blocks[bi].rows[0].page)
    });
    let want = cx.read(|cx| doc.read(cx).caret.as_ref().unwrap().off);
    doc.update(cx, |d, cx| {
        d.set_caret(item, cx);
        assert!(d.caret_click(bi, page, x1 + 20.0 * 65536.0, y1 - 3.0 * 65536.0, cx));
        assert_eq!(d.caret.as_ref().unwrap().off, want, "in the empty item");
    });
    // and the arrows stop there, from the item before and back
    doc.update(cx, |d, cx| {
        d.set_caret(item, cx);
        d.move_v(1, cx);
        assert_eq!(d.caret.as_ref().unwrap().off, want, "↓ into the empty item");
        d.move_v(-1, cx);
        assert!(d.caret.as_ref().unwrap().off < item + 1, "↑ out of it");
        d.set_caret(item, cx);
        d.move_h(1, cx);
        assert_eq!(d.caret.as_ref().unwrap().off, want, "→ into the empty item");
        d.move_h(-1, cx);
        assert_eq!(d.caret.as_ref().unwrap().off, item, "← back");
        d.set_caret(want, cx);
    });
    doc.update(cx, |d, cx| d.type_text("x", false, cx));
    doc.update(cx, |d, cx| d.tab(false, cx));
    assert!(cx.read(|cx| doc.read(cx).source[item..].contains("\\begin{itemize}\n      \\item x")));
    for _ in 0..3 {
        doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    }
    assert_eq!(cx.read(|cx| doc.read(cx).source.clone()), base);
    // Enter at the start of a heading: a new paragraph before it, the caret there
    let heading = cx.read(|cx| doc.read(cx).source.find("\\subsection*{Which").unwrap());
    let start = heading + "\\subsection*{".len();
    let (hp, _, hy, _) = doc.update(cx, |d, cx| {
        d.set_caret(start, cx);
        let at = d.caret_at().unwrap();
        d.enter(false, cx);
        at
    });
    calm(cx, &doc);
    let (p, _, y, _) = cx.read(|cx| doc.read(cx).caret_at().unwrap());
    assert!((p, y) < (hp, hy), "the caret is above the heading");
    doc.update(cx, |d, cx| d.type_text("New", false, cx));
    let s = cx.read(|cx| doc.read(cx).source.clone());
    assert!(s[heading..].starts_with("New\n\n\\subsection*{Which"));
    doc.update_in(cx, |d, window, cx| {
        d.undo(window, cx);
        d.undo(window, cx);
    });
    wait(cx, &doc, "the undo", |d| {
        d.spans_pending == 0 && !d.compiling
    });
    // a selected word: highlighted, typed over in one undo step, copied and pasted as TeX
    let before = cx.read(|cx| doc.read(cx).source.clone());
    doc.update(cx, |d, cx| {
        d.set_caret(off, cx);
        d.extending(cx, |d, cx| d.move_word(1, cx));
        let r = d.selection_range().expect("a word selected");
        assert!(
            !d.source[r.clone()].contains(' '),
            "{:?}",
            &d.source[r.clone()]
        );
        let on = d.blocks[d.block_of(off).unwrap()].rows[0].page;
        assert!(!d.render_selection(on, cx).is_empty(), "it is highlighted");
        assert!(d.copy_selection(cx));
        d.type_text("W", false, cx);
        assert!(d.selection_range().is_none());
    });
    let word = cx.read(|cx| cx.read_from_clipboard().and_then(|c| c.text()).unwrap());
    assert!(
        cx.read(|cx| doc.read(cx).source[off..].starts_with(&format!(
            "W{}",
            &before[off + word.len()..off + word.len() + 3]
        )))
    );
    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    assert_eq!(
        cx.read(|cx| doc.read(cx).source.clone()),
        before,
        "one undo step"
    );
    calm(cx, &doc);
    doc.update(cx, |d, cx| {
        d.set_caret(off, cx);
        d.paste_at_caret(false, cx);
    });
    assert!(cx.read(|cx| doc.read(cx).source[off..].starts_with(&format!("{word}{word}"))));
    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    assert_eq!(cx.read(|cx| doc.read(cx).source.clone()), before);
    calm(cx, &doc);
    // the caret moves through the text and back
    wait(cx, &doc, "one paragraph", |d| d.spans_pending == 0);
    doc.update(cx, |d, cx| {
        d.set_caret(off, cx);
        let start = off;
        d.move_h(1, cx);
        assert_eq!(d.caret.as_ref().unwrap().off, start + 1);
        d.move_h(-1, cx);
        assert_eq!(d.caret.as_ref().unwrap().off, start);
        d.move_v(1, cx);
        d.move_v(-1, cx);
    });
}

/// Every edit at the start, middle and end of every block of a real document, through
/// rtex: none may break the TeX, lose the caret or fail to undo. `RTD_TYPE=/abs/copy.tex
/// cargo test --release edit_everywhere -- --ignored --nocapture`.
#[gpui_kit::test]
#[ignore]
fn edit_everywhere(cx: &mut TestAppContext) {
    let (root, main) = rtd_type();
    let (doc, cx) = open(cx, root, &main, 3);
    let base = cx.read(|cx| doc.read(cx).source.clone());
    // the spots to edit at: each text block's first, middle and last stop
    let spots: Vec<(usize, String, [usize; 3])> = cx.read(|cx| {
        let d = doc.read(cx);
        (0..d.blocks.len())
            .filter(|&b| d.is_text_block(b))
            .filter_map(|b| {
                let page = d.blocks[b].rows.first()?.page;
                let m = d.map_of(b, page)?;
                let st: Vec<usize> = m.stops.iter().map(|s| s.off).collect();
                let name: String = d.source[d.blocks[b].range.clone()]
                    .chars()
                    .take(40)
                    .collect();
                Some((b, name, [st[0], st[st.len() / 2], *st.last()?]))
            })
            .collect()
    });
    type Op = (
        &'static str,
        usize,
        fn(&mut Doc, &mut gpui_kit::Context<Doc>),
    );
    let ops: [Op; 9] = [
        ("select-down-delete", 1, |d, cx| {
            d.extending(cx, |d, cx| {
                d.move_v(1, cx);
                d.move_v(1, cx);
            });
            d.delete(true, cx);
        }),
        ("select-word-type", 1, |d, cx| {
            d.extending(cx, |d, cx| d.move_word(1, cx));
            d.type_text("Zq", false, cx);
        }),
        ("type", 1, |d, cx| d.type_text("Zq", false, cx)),
        ("enter@start", 0, |d, cx| d.enter(false, cx)),
        ("enter@mid", 1, |d, cx| d.enter(false, cx)),
        ("enter@end", 2, |d, cx| d.enter(false, cx)),
        ("shift-enter@mid", 1, |d, cx| d.enter(true, cx)),
        ("backspace@start", 0, |d, cx| d.delete(true, cx)),
        ("delete@end", 2, |d, cx| d.delete(false, cx)),
    ];
    let mut failures = vec![];
    let t0 = Instant::now();
    // RTD_SWEEP_ONLY=7:enter@end runs one edit and shows it
    let only = std::env::var("RTD_SWEEP_ONLY").ok();
    for (bi, name, at) in &spots {
        for (op, pos, f) in &ops {
            if only
                .as_ref()
                .is_some_and(|o| !format!("{bi}:{op}").starts_with(o.as_str()))
            {
                continue;
            }
            let off = at[*pos];
            let (before, in_item) = doc.update(cx, |d, cx| {
                d.status.clear();
                d.set_caret(off, cx);
                let at = d.caret_at();
                let item = caret::item_at(&d.source, off).is_some_and(|it| off >= it.text.start);
                f(d, cx);
                (at, item)
            });
            calm(cx, &doc);
            let (changed, status, caret) = cx.read(|cx| {
                let d = doc.read(cx);
                // a caret, or (Backspace reaching a picture) that block selected, on the page
                let shown = d.caret_at().is_some()
                    || d.caret.is_none()
                        && d.selection()
                            .is_some_and(|(lo, _)| !d.blocks[lo].rows.is_empty());
                (d.source != base, d.status.clone(), shown)
            });
            let what = format!("block {bi} {name:?}: {op}");
            if status.contains("LaTeX") {
                let edit = cx.read(|cx| {
                    let d = doc.read(cx);
                    let r = d.last_edit.clone();
                    d.source[r.start.saturating_sub(20)..(r.end + 20).min(d.source.len())]
                        .to_string()
                });
                failures.push(format!("{what} → {status} (around {edit:?})"));
            }
            // Enter moves the caret up into a new paragraph before the text (at its start),
            // or down to the new line (in its middle, at its end)
            let after = cx.read(|cx| doc.read(cx).caret_at());
            if only.is_some() {
                cx.read(|cx| {
                    let d = doc.read(cx);
                    let c = d.caret.as_ref().unwrap().off;
                    let r = d.last_edit.clone();
                    println!("edit {r:?} caret {c} before {before:?} after {after:?}");
                    println!(
                        "around: {:?}",
                        &d.source[r.start.saturating_sub(60)..(r.end + 60).min(d.source.len())]
                    );
                    let bi = d.block_of(c);
                    println!(
                        "caret block {bi:?} stop {:?}",
                        d.caret_stop().map(|(b, p, m, i)| (b, p, m.stops[i]))
                    );
                    if let Some(bi) = bi {
                        for r in &d.blocks[bi].rows {
                            println!(
                                "  row p{} top {:.1} bottom {:.1}",
                                r.page,
                                r.bbox[1] / 65536.0,
                                r.bbox[3] / 65536.0
                            );
                        }
                    }
                });
            }
            if changed
                && op.starts_with("enter")
                && let (Some(b), Some(a)) = (before, after)
            {
                let (b, a) = ((b.0, b.2), (a.0, a.2));
                // before an item's text, a new item opens above and the caret stays with
                // the text, which moves down
                let ok = if *pos == 0 && !in_item { a < b } else { a > b };
                if !ok {
                    failures.push(format!("{what} → caret went from {b:?} to {a:?}"));
                }
            }
            if !caret {
                failures.push(format!("{what} → no caret, nothing selected"));
            }
            if changed {
                doc.update_in(cx, |d, window, cx| d.undo(window, cx));
                calm(cx, &doc);
                if cx.read(|cx| doc.read(cx).source != base) {
                    failures.push(format!("{what} → undo did not restore the source"));
                    let b = base.clone();
                    doc.update(cx, |d, cx| {
                        d.replace(0..d.source.len(), &b, cx);
                    });
                    calm(cx, &doc);
                }
            }
        }
    }
    println!(
        "{} edits at {} spots in {:?}",
        spots.len() * ops.len(),
        spots.len(),
        t0.elapsed()
    );
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// Enter before a heading, then a command typed there: `\\title{AAA}` goes in as TeX, and
/// nothing reaches the source (or moves the pages) while it is half typed.
/// `RTD_TYPE=/abs/copy.tex cargo test --release command_in_a_new_paragraph -- --ignored`.
#[gpui_kit::test]
#[ignore]
fn command_in_a_new_paragraph(cx: &mut TestAppContext) {
    let (root, main) = rtd_type();
    let (doc, cx) = open(cx, root, &main, 3);
    calm(cx, &doc);
    let base = cx.read(|cx| doc.read(cx).source.clone());
    // a click before "Which one?", Enter: a new paragraph above it
    let (bi, page, x, y) = cx.read(|cx| {
        let d = doc.read(cx);
        let w = d.source.find("{Which one").unwrap() + 1;
        let bi = d.block_of(w).unwrap();
        let page = d.blocks[bi].rows[0].page;
        let m = d.map_of(bi, page).unwrap();
        let s = m.stops.iter().find(|s| s.off == w).copied().unwrap();
        (bi, page, s.x + 65536.0, s.y - 3.0 * 65536.0)
    });
    doc.update(cx, |d, cx| {
        assert!(d.caret_click(bi, page, x, y, cx));
        d.enter(false, cx);
    });
    calm(cx, &doc);
    let split = cx.read(|cx| doc.read(cx).source.clone());
    // a backslash: the source box, and nothing in the source yet
    doc.update_in(cx, |d, window, cx| {
        d.replace_text_in_range(None, "\\", window, cx)
    });
    cx.run_until_parked();
    for ch in "titl".chars() {
        let input = cx.read(|cx| doc.read(cx).input.clone());
        cx.update(|window, cx| {
            input.update(cx, |s, cx| {
                s.replace_text_in_range(None, &ch.to_string(), window, cx)
            })
        });
        cx.run_until_parked();
    }
    cx.read(|cx| {
        let d = doc.read(cx);
        assert!(
            d.active.as_ref().is_some_and(|a| a.gap.is_some()),
            "a box in the new paragraph"
        );
        assert_eq!(d.input.read(cx).value().to_string(), "\\titl");
        assert_eq!(
            d.source, split,
            "a half-typed command stays out of the source"
        );
        assert!(
            d.box_place().is_some_and(|p| !p.lines),
            "the box stays in the gap"
        );
    });
    for ch in "e{AAA}".chars() {
        let input = cx.read(|cx| doc.read(cx).input.clone());
        cx.update(|window, cx| {
            input.update(cx, |s, cx| {
                s.replace_text_in_range(None, &ch.to_string(), window, cx)
            })
        });
        cx.run_until_parked();
    }
    let s = cx.read(|cx| doc.read(cx).source.clone());
    assert!(
        s.contains("\\title{AAA}\n\n"),
        "the whole command goes in as TeX"
    );
    assert!(!s.contains("textbackslash"));
    calm(cx, &doc);
    assert!(cx.read(|cx| !doc.read(cx).status.contains("LaTeX")));
    // a command left half typed goes in when the box closes
    let input = cx.read(|cx| doc.read(cx).input.clone());
    cx.update(|window, cx| {
        input.update(cx, |s, cx| {
            s.replace_text_in_range(None, " \\date", window, cx)
        })
    });
    cx.run_until_parked();
    assert!(!cx.read(|cx| doc.read(cx).source.contains("AAA} \\date")));
    doc.update_in(cx, |d, window, cx| d.leave(window, cx));
    assert!(cx.read(|cx| doc.read(cx).source.contains("\\title{AAA} \\date")));
    doc.update_in(cx, |d, window, cx| {
        while d.source != base && !d.undo.is_empty() {
            d.undo(window, cx);
        }
    });
    assert_eq!(
        cx.read(|cx| doc.read(cx).source.clone()),
        base,
        "undo restores it all"
    );
    calm(cx, &doc);
    // an empty heading (just "5.1" on the page) takes the caret and goes with Backspace
    doc.update(cx, |d, cx| {
        assert!(d.caret_click(bi, page, x, y, cx));
        d.enter(false, cx);
    });
    calm(cx, &doc);
    doc.update_in(cx, |d, window, cx| {
        d.replace_text_in_range(None, "\\", window, cx)
    });
    cx.run_until_parked();
    let input = cx.read(|cx| doc.read(cx).input.clone());
    cx.update(|window, cx| {
        input.update(cx, |s, cx| {
            s.replace_text_in_range(None, "subsection{}", window, cx)
        })
    });
    cx.run_until_parked();
    doc.update_in(cx, |d, window, cx| d.leave(window, cx));
    calm(cx, &doc);
    let (hb, hp, hx, hy) = cx.read(|cx| {
        let d = doc.read(cx);
        let hb = (0..d.blocks.len())
            .find(|&b| &d.source[d.blocks[b].range.clone()] == "\\subsection{}")
            .expect("the empty heading is a block");
        let r = &d.blocks[hb].rows[0];
        (
            hb,
            r.page,
            r.bbox[0] + 40.0 * 65536.0,
            r.bbox[3] - 3.0 * 65536.0,
        )
    });
    doc.update(cx, |d, cx| {
        assert!(
            d.caret_click(hb, hp, hx, hy, cx),
            "the caret goes into the empty heading"
        );
        d.delete(true, cx);
    });
    assert!(
        !cx.read(|cx| doc.read(cx).source.contains("\\subsection{}")),
        "Backspace removes it"
    );
}

/// The ribbon's operations on a real document through rtex: each typesets without a LaTeX
/// error and undoes in one step. `RTD_TYPE=/abs/copy.tex cargo test --release
/// format_on_pages -- --ignored --nocapture`.
#[gpui_kit::test]
#[ignore]
fn format_on_pages(cx: &mut TestAppContext) {
    use super::super::FormatOp;
    use crate::document::format::{Align, Size, Style};
    let (root, main) = rtd_type();
    let (doc, cx) = open(cx, root, &main, 3);
    calm(cx, &doc);
    let base = cx.read(|cx| doc.read(cx).source.clone());
    // a plain paragraph's second word
    let off = cx.read(|cx| {
        let d = doc.read(cx);
        let b = d
            .blocks
            .iter()
            .find(|b| d.source[b.range.clone()].starts_with("Used together"))
            .expect("the paragraph");
        b.range.start + "Used ".len()
    });
    let select_word = |d: &mut Doc, cx: &mut gpui_kit::Context<Doc>| {
        d.set_caret(off, cx);
        d.extending(cx, |d, cx| d.move_word(1, cx));
    };
    let ops: Vec<(&str, FormatOp, &str)> = vec![
        ("bold", FormatOp::Inline("textbf"), "\\textbf{together}"),
        ("italic", FormatOp::Inline("textit"), "\\textit{together}"),
        (
            "size",
            FormatOp::Size(Size::Named("Large")),
            "{\\Large together}",
        ),
        (
            "colour",
            FormatOp::Color("red".into()),
            "\\textcolor{red}{together}",
        ),
        (
            "bullets",
            FormatOp::List("itemize"),
            "\\begin{itemize}\n  \\item Used together",
        ),
        (
            "heading",
            FormatOp::Style(Style::Heading("subsection")),
            "\\subsection{Used together",
        ),
        (
            "center",
            FormatOp::Align(Align::Center),
            "\\begin{center}\nUsed together",
        ),
    ];
    for (name, op, want) in ops {
        doc.update_in(cx, |d, window, cx| {
            d.status.clear();
            select_word(d, cx);
            d.format(op.clone(), window, cx);
        });
        let src = cx.read(|cx| doc.read(cx).source.clone());
        assert!(
            src.contains(want),
            "{name}: {:?}",
            &src[off.saturating_sub(30)..(off + 80).min(src.len())]
        );
        if name == "colour" {
            assert!(
                src.contains("\\usepackage{xcolor}") || base.contains("xcolor"),
                "xcolor added"
            );
        }
        calm(cx, &doc);
        let status = cx.read(|cx| doc.read(cx).status.clone());
        assert!(!status.contains("LaTeX"), "{name}: {status}");
        if name == "bold" {
            // the ribbon shows it, and the button again takes it off
            cx.read(|cx| assert!(doc.read(cx).ribbon_state().has("textbf"), "bold lit"));
            doc.update_in(cx, |d, window, cx| {
                d.format(FormatOp::Inline("textbf"), window, cx)
            });
            let src = cx.read(|cx| doc.read(cx).source.clone());
            assert!(!src.contains("\\textbf{together}"), "bold off again");
            doc.update_in(cx, |d, window, cx| d.undo(window, cx));
        }
        doc.update_in(cx, |d, window, cx| d.undo(window, cx));
        assert_eq!(
            cx.read(|cx| doc.read(cx).source.clone()),
            base,
            "{name}: one undo step"
        );
        calm(cx, &doc);
    }
    // a colour at a caret between words: what is typed next takes it, as in Word
    doc.update_in(cx, |d, window, cx| {
        d.set_caret(off - 1, cx);
        d.replace_text_in_range(None, " ", window, cx);
        d.format(FormatOp::Color("red".into()), window, cx);
        for ch in "xy".chars() {
            d.replace_text_in_range(None, &ch.to_string(), window, cx);
        }
    });
    let src = cx.read(|cx| doc.read(cx).source.clone());
    assert!(
        src.contains("Used \\textcolor{red}{xy} together"),
        "typed in the colour: {:?}",
        &src[off.saturating_sub(30)..(off + 60).min(src.len())]
    );
    calm(cx, &doc);
    for _ in 0..4 {
        if cx.read(|cx| doc.read(cx).source == base) {
            break;
        }
        doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    }
    assert_eq!(cx.read(|cx| doc.read(cx).source.clone()), base, "undone");
    calm(cx, &doc);
    // Find: every printed "together"; ⌘G steps through them; Replace All in one undo step
    doc.update_in(cx, |d, window, cx| {
        d.set_caret(0, cx);
        d.open_find(true, window, cx);
        let q = d.find.as_ref().unwrap().query.clone();
        q.update(cx, |s, cx| s.set_value("together", window, cx));
        d.refind(cx);
        let n = d.find.as_ref().unwrap().matches.len();
        assert!(n >= 2, "{n} matches");
        d.find_step(1, window, cx);
        let first = d.selection_range().unwrap();
        assert!(d.source[first.clone()].eq_ignore_ascii_case("together"));
        d.find_step(1, window, cx);
        assert!(
            d.selection_range().unwrap().start > first.start,
            "the next one"
        );
        let w = d.find.as_ref().unwrap().with.clone();
        w.update(cx, |s, cx| s.set_value("in step", window, cx));
        d.replace_all(cx);
        assert!(!d.source.to_lowercase().contains("used together"));
        assert!(d.find.as_ref().unwrap().matches.is_empty(), "none left");
    });
    calm(cx, &doc);
    assert!(cx.read(|cx| !doc.read(cx).status.contains("LaTeX")));
    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    assert_eq!(
        cx.read(|cx| doc.read(cx).source.clone()),
        base,
        "Replace All: one undo step"
    );
}

#[gpui_kit::test]
#[ignore]
fn type_in_a_colour(cx: &mut TestAppContext) {
    use super::super::FormatOp;
    let tex = "\\documentclass{article}\n\\begin{document}\n\\section{Intro}\n\n\\subsection{i type}\n\nsome words here\n\n\\subsection{}\n\nlast words\n\n\\end{document}\n";
    let dir = scratch("colour", tex);
    let (doc, cx) = open(cx, dir, "main.tex", 2);
    // the caret at the end of a heading's title, then at the end of a paragraph: a colour
    // picked there, what is typed next takes it
    for (after, want) in [
        ("i type", "\\subsection{i type\\textcolor{red}{ab}}"),
        ("words here", "words here\\textcolor{red}{ab}"),
        // an empty heading, as the Subsection style leaves one
        (
            "here\\textcolor{red}{ab}\n\n\\subsection{",
            "\\subsection{\\textcolor{red}{ab}}\n\nlast",
        ),
    ] {
        doc.update_in(cx, |d, window, cx| {
            let at = d.source.find(after).unwrap() + after.len();
            d.set_caret(at, cx);
            d.format(FormatOp::Color("red".into()), window, cx);
        });
        // as a person does: the pages laid out again before the first key
        calm(cx, &doc);
        doc.update_in(cx, |d, window, cx| {
            for ch in "ab".chars() {
                d.replace_text_in_range(None, &ch.to_string(), window, cx);
            }
        });
        let src = cx.read(|cx| doc.read(cx).source.clone());
        assert!(src.contains(want), "{want}: {src}");
        calm(cx, &doc);
    }
    // through an input method: composed, then committed, it lands in the colour too
    doc.update_in(cx, |d, window, cx| {
        let at = d.source.find("last words").unwrap() + "last words".len();
        d.set_caret(at, cx);
        d.format(FormatOp::Color("blue".into()), window, cx);
    });
    calm(cx, &doc);
    doc.update_in(cx, |d, window, cx| {
        d.replace_and_mark_text_in_range(None, "ni", None, window, cx);
        d.replace_text_in_range(None, "你", window, cx);
    });
    let src = cx.read(|cx| doc.read(cx).source.clone());
    assert!(src.contains("last words\\textcolor{blue}{你}"), "{src}");
}
