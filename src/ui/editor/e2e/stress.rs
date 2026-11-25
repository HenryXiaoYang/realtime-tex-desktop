// A document edited at random, the way a person who does everything at once would: typing every
// character TeX cares about, Enter and Backspace anywhere, selections, formats, undo and redo.
// What must hold throughout: the source stays TeX that balances, the blocks stay inside it, the
// caret stays on a character, nothing panics, and rtex typesets what is left without an error.
// At the end every step is undone and the document is the one it began as.
//
// `cargo test --release stress_editing -- --ignored --nocapture`; `FUZZ_N` steps, `FUZZ_SEED`.
use super::super::FormatOp;
use super::*;
use crate::document::caret::balanced;
use crate::document::format::{Align, Font, Size, Style};

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as usize % n.max(1)
    }
}

const DOC: &str = "\\documentclass{article}\n\\usepackage{amsmath}\n\\usepackage{xcolor}\n\\begin{document}\n\\section{Intro}\nFirst paragraph with \\textbf{bold} and \\emph{emphasis}, plus a long tail of words that wraps onto a second line of the page.\n\nSecond paragraph, short.\n\n\\begin{itemize}\n\\item One\n\\item Two with $x^2$ in it\n\\end{itemize}\n\n\\begin{center}\nCentred text.\n\\end{center}\n\nLast paragraph, with \\textcolor{red}{colour} and \u{e9}\u{65e5}\u{672c}.\n\\end{document}\n";

const TYPED: &[&str] = &[
    "a", "Z", " ", "  ", "5", "%", "&", "#", "_", "^", "~", "{", "}", "\\", "\u{e9}", "\u{65e5}",
    "word ", "50% ", "a_b ", "x & y", "$",
];

fn formats() -> Vec<(&'static str, FormatOp)> {
    vec![
        ("bold", FormatOp::Inline("textbf")),
        ("italic", FormatOp::Inline("textit")),
        ("underline", FormatOp::Inline("underline")),
        ("large", FormatOp::Size(Size::Named("large"))),
        ("grow", FormatOp::Grow(1)),
        ("shrink", FormatOp::Grow(-1)),
        ("sans", FormatOp::Font(Font::Sans)),
        ("mono", FormatOp::Font(Font::Mono)),
        ("red", FormatOp::Color("red".into())),
        ("bullets", FormatOp::List("itemize")),
        ("numbers", FormatOp::List("enumerate")),
        ("indent", FormatOp::Indent(true)),
        ("outdent", FormatOp::Indent(false)),
        ("centre", FormatOp::Align(Align::Center)),
        ("right", FormatOp::Align(Align::Right)),
        ("left", FormatOp::Align(Align::Left)),
        ("justify", FormatOp::Align(Align::Justify)),
        ("heading", FormatOp::Style(Style::Heading("section"))),
        ("quote", FormatOp::Style(Style::Quote)),
        ("normal", FormatOp::Style(Style::Normal)),
    ]
}

/// Every place the caret can go on the pages now.
fn stops(d: &Doc) -> Vec<usize> {
    let mut out = vec![];
    for bi in 0..d.blocks.len() {
        if !d.is_text_block(bi) {
            continue;
        }
        let Some(page) = d.blocks[bi].rows.first().map(|r| r.page) else {
            continue;
        };
        if let Some(m) = d.map_of(bi, page) {
            out.extend(m.stops.iter().map(|s| s.off));
        }
    }
    out
}

fn sound(d: &Doc) -> Result<(), String> {
    let s = &d.source;
    if !balanced(s) {
        return Err("the source does not balance".into());
    }
    if s.matches("\\begin{document}").count() != 1 || s.matches("\\end{document}").count() != 1 {
        return Err("\\begin{document} / \\end{document} are not one each".into());
    }
    let mut end = 0;
    for b in &d.blocks {
        let r = &b.range;
        if r.start > r.end
            || r.end > s.len()
            || !s.is_char_boundary(r.start)
            || !s.is_char_boundary(r.end)
        {
            return Err(format!("a block range {r:?} is off the source"));
        }
        if r.start < end {
            return Err(format!("blocks overlap at {r:?}"));
        }
        end = r.end;
    }
    if let Some(c) = &d.caret {
        if c.off > s.len() || !s.is_char_boundary(c.off) {
            return Err(format!("the caret {} is off the source", c.off));
        }
        if let Some(a) = c.anchor
            && (a > s.len() || !s.is_char_boundary(a))
        {
            return Err(format!("the selection anchor {a} is off the source"));
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Step {
    choice: usize,
    pick: usize,
}

/// What a step did, as words.
fn do_step(d: &mut Doc, st: Step, window: &mut Window, cx: &mut gpui_kit::Context<Doc>) -> String {
    let fmts = formats();
    let (choice, pick) = (st.choice, st.pick);
    let spots = stops(d);
    if spots.is_empty() {
        return "no caret stops".into();
    }
    let at = spots[pick % spots.len()];
    let other = spots[(pick / 7) % spots.len()];
    let (lo, hi) = (at.min(other), at.max(other));
    match choice {
        0..=29 => {
            if d.caret.is_none() {
                d.set_caret(at, cx);
            }
            let t = TYPED[pick % TYPED.len()];
            d.type_text(t, false, cx);
            format!("type {t:?}")
        }
        30..=39 => {
            d.set_caret(at, cx);
            d.delete(pick % 2 == 0, cx);
            format!(
                "{} at {at}",
                if pick % 2 == 0 { "backspace" } else { "delete" }
            )
        }
        40..=47 => {
            d.set_caret(at, cx);
            d.enter(pick % 3 == 0, cx);
            format!(
                "{} at {at}",
                if pick % 3 == 0 {
                    "shift-enter"
                } else {
                    "enter"
                }
            )
        }
        48..=57 => {
            d.set_caret(at, cx);
            let m = ["left", "right", "word", "down"][pick % 4];
            match pick % 4 {
                0 => d.move_h(-1, cx),
                1 => d.move_h(1, cx),
                2 => d.move_word(1, cx),
                _ => d.move_v(1, cx),
            }
            format!("move {m} from {at}")
        }
        58..=69 => {
            d.set_caret(lo, cx);
            d.extending(cx, |d, cx| d.set_caret(hi, cx));
            let (name, op) = &fmts[pick % fmts.len()];
            d.format(op.clone(), window, cx);
            format!("select {lo}..{hi} then {name}")
        }
        70..=77 => {
            d.set_caret(at, cx);
            let (name, op) = &fmts[pick % fmts.len()];
            d.format(op.clone(), window, cx);
            format!("{name} at {at}")
        }
        78..=83 => {
            d.set_caret(lo, cx);
            d.extending(cx, |d, cx| d.set_caret(hi, cx));
            let t = TYPED[pick % TYPED.len()];
            d.type_text(t, false, cx);
            format!("select {lo}..{hi} then type {t:?}")
        }
        84..=87 => {
            d.select_all_text(cx);
            let (name, op) = &fmts[pick % fmts.len()];
            d.format(op.clone(), window, cx);
            format!("select all then {name}")
        }
        88..=93 => {
            d.undo(window, cx);
            "undo".into()
        }
        94..=97 => {
            d.redo(window, cx);
            "redo".into()
        }
        _ => {
            d.leave(window, cx);
            "leave".into()
        }
    }
}

/// A fresh document with `steps` done to it, each typeset before the next: the first thing that
/// went wrong (its step, and what), with the words of every step taken.
fn run(cx: &mut TestAppContext, steps: &[Step]) -> Result<(), (usize, String, Vec<String>)> {
    let (doc, cx) = open(cx, scratch("stress", DOC), "main.tex", 3);
    doc.update_in(cx, |d, window, cx| d.focus.focus(window, cx));
    let mut log: Vec<String> = vec![];
    for (i, st) in steps.iter().enumerate() {
        let what = doc.update_in(cx, |d, window, cx| do_step(d, *st, window, cx));
        log.push(what);
        cx.run_until_parked();
        calm(cx, &doc);
        let (bad, status, src) = cx.read(|cx| {
            let d = doc.read(cx);
            (sound(d).err(), d.status.clone(), d.source.clone())
        });
        if std::env::var("FUZZ_VERBOSE").is_ok() {
            let sel = cx.read(|cx| doc.read(cx).caret.as_ref().map(|c| (c.anchor, c.off)));
            eprintln!(
                "--- step {i}: {} (anchor, caret) = {sel:?}\n{src}",
                log.last().unwrap()
            );
        }
        if let Some(e) = bad {
            return Err((i, format!("{e}\n{src}"), log));
        }
        // (a document with nothing in it has no pages for TeX to make: not an editor's fault)
        let empty = src
            .split_once("\\begin{document}")
            .and_then(|(_, b)| b.split_once("\\end{document}"))
            .is_some_and(|(b, _)| b.trim().is_empty());
        if status.contains("LaTeX") && !empty {
            return Err((i, format!("{status}\n{src}"), log));
        }
    }
    // every step taken back: the document it began as
    let depth = cx.read(|cx| doc.read(cx).undo.len());
    if depth < 190 {
        for _ in 0..depth + 5 {
            doc.update_in(cx, |d, window, cx| d.undo(window, cx));
        }
        calm(cx, &doc);
        let back = cx.read(|cx| doc.read(cx).source.clone());
        if back != DOC {
            return Err((
                steps.len(),
                format!("not undone all the way ({depth} steps)\n{back}"),
                log,
            ));
        }
    }
    Ok(())
}

/// The first line of a failure: what kind it is, whatever the document looked like.
fn kind(msg: &str) -> String {
    msg.lines()
        .next()
        .unwrap_or("")
        .split(['{', '('])
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

#[gpui_kit::test]
#[ignore]
fn stress_editing(cx: &mut TestAppContext) {
    let n: usize = std::env::var("FUZZ_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    let seed: u64 = std::env::var("FUZZ_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let mut r = Rng(0x9E37_79B9_7F4A_7C15 ^ seed.wrapping_mul(0x2545_F491));
    let mut steps: Vec<Step> = (0..n)
        .map(|_| Step {
            choice: r.below(100),
            pick: r.below(1_000_000),
        })
        .collect();
    // the steps a failure shrank to ("choice:pick,..."), run as they are
    if let Ok(list) = std::env::var("FUZZ_REPLAY") {
        steps = list
            .split(',')
            .filter_map(|p| p.split_once(':'))
            .filter_map(|(c, k)| {
                Some(Step {
                    choice: c.parse().ok()?,
                    pick: k.parse().ok()?,
                })
            })
            .collect();
    }
    let Err((at, msg, _)) = run(cx, &steps) else {
        return;
    };
    // the fewest of those steps that still go wrong the same way
    let want = kind(&msg);
    eprintln!("FIRST FAILURE at step {at}: {msg}");
    // every try opens a document with its own rtex, so the tries are few
    let limit: usize = std::env::var("FUZZ_TRIALS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(40);
    let mut best: Vec<Step> = steps[..=at.min(steps.len() - 1)].to_vec();
    let mut chunk = best.len().div_ceil(2);
    let mut trials = 0;
    while chunk >= 1 && trials < limit {
        let mut i = 0;
        while i < best.len() && trials < limit {
            let mut cand = best.clone();
            cand.drain(i..(i + chunk).min(cand.len()));
            trials += 1;
            if matches!(run(cx, &cand), Err((_, m, _)) if kind(&m) == want) {
                best = cand;
            } else {
                i += chunk;
            }
        }
        chunk /= 2;
    }
    let replay: Vec<String> = best
        .iter()
        .map(|s| format!("{}:{}", s.choice, s.pick))
        .collect();
    let (_, msg, log) = run(cx, &best).err().unwrap_or((0, msg, vec![]));
    panic!(
        "{} steps are enough ({trials} tries):\n{}\n\n{msg}\nFUZZ_REPLAY={}",
        best.len(),
        log.join("\n"),
        replay.join(",")
    );
}
