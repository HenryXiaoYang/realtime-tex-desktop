// A look at how a document splits into blocks, without the window: `RTD_INSPECT=file.tex
// cargo test inspect -- --ignored --nocapture` prints each block's source and typeset text.
#![cfg(test)]
use crate::document::{self, glyphmap};
use crate::{engine, render};
use futures::StreamExt;
use std::path::Path;
use std::time::{Duration, Instant};

#[test]
#[ignore]
fn inspect() {
    let file = std::env::var("RTD_INSPECT").expect("RTD_INSPECT=path/to/main.tex");
    let path = Path::new(&file);
    let (root, main) = (
        path.parent().unwrap(),
        path.file_name().unwrap().to_str().unwrap(),
    );
    let source = std::fs::read_to_string(path).unwrap();
    engine::configure("", "/Library/TeX/texbin");
    let (tx, mut rx) = futures::channel::mpsc::unbounded();
    let mut r = engine::Rtex::start(root, main, tx).unwrap();
    let (mut preview, mut spans) = (document::Preview::default(), None);
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut laid_out = false;
    while Instant::now() < deadline && !(laid_out && spans.is_some()) {
        match futures::executor::block_on(rx.next()) {
            Some(engine::Msg::Layout {
                pages,
                pages_total,
                placements,
                stale_since,
                ..
            }) => {
                preview.apply_layout(pages, pages_total, placements, stale_since);
                laid_out = pages_total > 0;
                r.request_spans();
            }
            Some(engine::Msg::Spans(s)) => spans = Some(s),
            Some(engine::Msg::Exited(t)) => panic!("rtex exited: {t}"),
            Some(_) => {}
            None => break,
        }
    }
    let mut spans = spans.expect("no spans");
    let mut source = source;
    // RTD_INSPECT_EDIT=find|replace makes one edit first and shows the live (fast path) result
    if let Some((find, with)) = std::env::var("RTD_INSPECT_EDIT")
        .ok()
        .as_deref()
        .and_then(|e| e.split_once('|'))
    {
        let at = source.find(find).expect("edit text not found");
        r.edit(at, at + find.len(), with);
        source.replace_range(at..at + find.len(), with);
        r.request_spans();
        let until = Instant::now() + Duration::from_secs(3);
        while Instant::now() < until {
            let Some(Some(m)) = futures::executor::block_on(within_half_second(rx.next())) else {
                continue;
            };
            match m {
                engine::Msg::Paragraph {
                    par_id,
                    removed,
                    dl,
                    fragments,
                    source_revision,
                } => {
                    println!("paragraph {par_id} fragments {fragments:?}");
                    preview.apply_paragraph(
                        par_id,
                        dl,
                        if removed { vec![] } else { fragments },
                        source_revision,
                    );
                }
                engine::Msg::Spans(s) => spans = s,
                _ => {}
            }
        }
    }
    if std::env::var("RTD_INSPECT_FONTS").is_ok() {
        for dl in preview.pages.values().take(1) {
            for (id, f) in &dl.fonts {
                println!(
                    "font {id}: {:?} {:?} {:?}",
                    f.name,
                    f.filename,
                    f.size.map(|s| s / 65536.0)
                );
            }
        }
    }
    for sp in &spans {
        println!(
            "span {} {:?} {:?}",
            sp.id,
            sp.kind,
            &source[sp.range.start..sp.range.end.min(source.len())]
        );
    }
    for (i, b) in document::blocks(&spans, &source, &preview)
        .iter()
        .enumerate()
    {
        let rows: Vec<_> = b.rows.iter().map(|r| &r.row).collect();
        let text: String = glyphmap::glyph_text_of(&rows).text.iter().collect();
        let pages: Vec<u32> = b.rows.iter().map(|r| r.page).collect();
        println!(
            "\nblock {i} (span {}) pages {pages:?}\n  source: {:?}\n  typeset: {text:?}",
            b.span,
            &source[b.range.clone()]
        );
        // RTD_INSPECT_CARET=1: where the caret can stand, as ¦ in the source
        if std::env::var("RTD_INSPECT_CARET").is_ok() {
            let mut seen = std::collections::BTreeSet::new();
            for page in b.rows.iter().map(|r| r.page) {
                if !seen.insert(page) {
                    continue;
                }
                let rows: Vec<_> = b
                    .rows
                    .iter()
                    .filter(|r| r.page == page)
                    .map(|r| &r.row)
                    .collect();
                let gt = glyphmap::glyph_text_of(&rows);
                let m = document::caret::char_map(&gt, &source[b.range.clone()], b.range.start);
                let mut marked = String::new();
                for (o, ch) in source[b.range.clone()].char_indices() {
                    if m.stops.iter().any(|s| s.off == b.range.start + o) {
                        marked.push('¦');
                    }
                    marked.push(ch);
                }
                if m.stops.iter().any(|s| s.off == b.range.end) {
                    marked.push('¦');
                }
                println!("  caret p{page} ({} stops): {marked:?}", m.stops.len());
                if std::env::var("RTD_INSPECT_CHARS")
                    .is_ok_and(|f| source[b.range.clone()].contains(&f))
                {
                    for c in &m.chars {
                        let s = c.src.clone().map(|r| &source[r]);
                        println!("    {:?} {s:?} glyph={}", c.ch, c.glyph);
                    }
                }
            }
        }
    }
    // RTD_INSPECT_PNG=dir also writes each page as dir/page-<n>.png
    let Ok(out) = std::env::var("RTD_INSPECT_PNG") else {
        return;
    };
    let mut res = render::Resources::default();
    res.root = root.to_path_buf();
    for (&n, dl) in &preview.pages {
        let (own, live, _) = preview.page_parts(n);
        let job = render::Job {
            rows: own.into_iter().chain(live).collect(),
            other: Some(dl.clone()),
            region: [
                0.0,
                0.0,
                dl.page_width.unwrap_or(40258437.0),
                dl.page_height.unwrap_or(52099153.0),
            ],
            scale: 1.5 / 65781.76,
            pdf: None,
            clear: vec![],
            dark: None,
        };
        let (w, h, mut px) = render::render(&mut res, &job);
        px.chunks_mut(4).for_each(|c| c.swap(0, 2));
        image::RgbaImage::from_raw(w, h, px)
            .unwrap()
            .save(format!("{out}/page-{n}.png"))
            .unwrap();
    }
}

/// `f`, or None after half a second.
async fn within_half_second<T>(f: impl std::future::Future<Output = T>) -> Option<T> {
    let (tx, rx) = futures::channel::oneshot::channel::<()>();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        let _ = tx.send(());
    });
    futures::pin_mut!(f);
    match futures::future::select(f, rx).await {
        futures::future::Either::Left((v, _)) => Some(v),
        _ => None,
    }
}
