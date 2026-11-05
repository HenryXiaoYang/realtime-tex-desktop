// The sidebar's fields set on every element of a document, typeset.
use super::{DOC, calm, open, scratch};
use crate::document::element::{self, Element, Input, Kind};
use gpui_kit::TestAppContext;
use std::time::Instant;

/// What to set a field to: every other choice, the toggle flipped, and for text an
/// ordinary value, TeX's specials, and nothing.
fn values(key: &str, input: &Input, now: &str) -> Vec<String> {
    match input {
        Input::Toggle => vec![if now == "true" { "false" } else { "true" }.into()],
        Input::Choice(opts) => opts
            .iter()
            .filter(|(v, _)| *v != now)
            .map(|(v, _)| v.to_string())
            .collect(),
        Input::Number => vec!["5".into(), "".into(), "abc".into()],
        Input::Text if key == "label" => vec!["sweep:x".into(), "".into()],
        Input::Text if key == "file" => vec!["pic".into()],
        Input::Text => vec!["Sweep text".into(), "50% & #1 {x} $y_2$".into(), "".into()],
    }
}

/// Every field of every element of a document set to several values through the sidebar's
/// edit, typeset by rtex: each typesets without a LaTeX error, reads back as set, and one
/// undo restores the source. `cargo test --release sidebar_everywhere -- --ignored
/// --nocapture` (RTD_SWEEP_ONLY=Figure:width runs those).
#[gpui_kit::test]
#[ignore]
fn sidebar_everywhere(cx: &mut TestAppContext) {
    let dir = scratch("sidebar", DOC);
    let (doc, cx) = open(cx, dir.clone(), "main.tex", 8);
    calm(cx, &doc);
    assert!(
        !cx.read(|cx| doc.read(cx).status.contains("LaTeX")),
        "the fixture typesets"
    );
    let base = cx.read(|cx| doc.read(cx).source.clone());
    // the elements: each block's, and an \includegraphics inside one
    let mut els: Vec<Element> = cx.read(|cx| {
        let d = doc.read(cx);
        let mut v = vec![element::element_at(&d.source, None, 0)];
        for b in &d.blocks {
            let r = b.range.clone();
            v.push(element::element_at(&d.source, Some(r.clone()), r.start));
            if let Some(i) = d.source[r.clone()].find("\\includegraphics") {
                v.push(element::element_at(
                    &d.source,
                    Some(r.clone()),
                    r.start + i + 1,
                ));
            }
        }
        v
    });
    els.dedup();
    let only = std::env::var("RTD_SWEEP_ONLY").ok();
    let (mut failures, mut n, t0) = (vec![], 0, Instant::now());
    for el in &els {
        let fields = element::fields(&base, el);
        for f in &fields {
            for v in values(f.key, &f.input, &f.value) {
                let what = format!(
                    "{}:{} = {v:?} (at {})",
                    el.kind.title(),
                    f.key,
                    el.range.start
                );
                if only.as_ref().is_some_and(|o| !what.starts_with(o.as_str())) {
                    continue;
                }
                n += 1;
                // refused (not a number, unbalanced): nothing to check but that
                let Some(change) = element::set(&base, el, f.key, &v) else {
                    continue;
                };
                // what the field shows after: the TeX it became
                let want = f.key.ne("width").then(|| {
                    let mut s = base.clone();
                    s.replace_range(change.range.clone(), &change.text);
                    s
                });
                doc.update(cx, |d, cx| {
                    d.status.clear();
                    d.set_field_at(el.kind, el.range.start, f.key, &v, cx);
                });
                calm(cx, &doc);
                let (src, status) = cx.read(|cx| {
                    let d = doc.read(cx);
                    (d.source.clone(), d.status.clone())
                });
                if status.contains("LaTeX") {
                    let r = change.range.clone();
                    failures.push(format!(
                        "{what} → {status}\n    {:?} → {:?}",
                        &base[r.clone()],
                        change.text
                    ));
                }
                // read back: the element again where it now is
                let grown = src.len() as isize - base.len() as isize;
                let start = if change.range.start < el.range.start {
                    (el.range.start as isize + grown).max(0) as usize
                } else {
                    el.range.start
                };
                let end = (el.range.end as isize + grown).max(start as isize) as usize;
                let el2 = Element {
                    kind: el.kind,
                    range: start..end.min(src.len()),
                };
                let back = element::fields(&src, &el2)
                    .into_iter()
                    .find(|g| g.key == f.key)
                    .map(|g| g.value);
                let shown = |s: &str| s.trim().replace('\\', "");
                if want.is_some_and(|w| w == src)
                    && el.kind != Kind::Document
                    && !matches!(f.input, Input::Number)
                    && back.as_deref().map(shown) != Some(shown(&v))
                {
                    failures.push(format!("{what} → reads back {back:?}"));
                }
                if src != base {
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
    }
    println!(
        "{n} sidebar edits on {} elements in {:?}",
        els.len(),
        t0.elapsed()
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
