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
use gpui_kit::EntityInputHandler;

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

/// What a press removed was one whole `\begin{equation}…\end{equation}` and nothing else.
fn gone_whole_equation(before: &str, after: &str) -> bool {
    let p = before
        .bytes()
        .zip(after.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let end = before.len() - after.len() + p;
    before.get(p..end).is_some_and(|g| {
        g.starts_with("\\begin{equation}")
            && g.ends_with("\\end{equation}")
            && g.matches("\\begin").count() == 1
    })
}

const NOTES: &str = "\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n\\section{Kinematics}\nA particle of mass $m$ moves with velocity $v = \\frac{dx}{dt}$ and the kinetic energy is $E = \\frac{1}{2} m v^2$ for slow motion.\n\nNewton's second law reads\n\\begin{equation}\nF = m a = m \\frac{d^2 x}{dt^2}\n\\end{equation}\nwhich we use throughout.\n\n\\section{Energy}\nThe work done is $W = \\int F \\, dx$ and so $W = \\Delta E$ holds exactly.\n\n\\begin{itemize}\n\\item Conservation of $E$ in closed systems\n\\item Momentum $p = m v$ is also conserved\n\\end{itemize}\n\n\\section{Waves}\nA wave has $\\lambda f = c$ and the amplitude $A$ decays slowly in the medium we study here.\n\n\\section{Optics}\nSnell's law: $n_1 \\sin\\theta_1 = n_2 \\sin\\theta_2$ at the boundary between two media.\n\\end{document}\n";

/// Delete or Backspace held down: presses come faster than TeX answers. No press may take more
/// than what is beside the caret: a char, a macro, a formula.
///
/// `cargo test --release held_delete -- --ignored --nocapture`; HOLD_MS is the pause between
/// presses (default 0: only the queue is drained), HOLD_N the presses.
#[gpui_kit::test]
#[ignore]
fn held_delete(cx: &mut TestAppContext) {
    let pause: u64 = std::env::var("HOLD_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let n: usize = std::env::var("HOLD_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(150);
    let mut worst: Vec<String> = vec![];
    for (name, back, from) in [
        ("delete from the first paragraph", false, "A particle"),
        ("delete from a formula paragraph", false, "The work done"),
        ("delete from the list", false, "Conservation"),
        (
            "backspace from the end of the notes",
            true,
            "between two media.",
        ),
        (
            "backspace from the waves paragraph",
            true,
            "medium we study here.",
        ),
    ] {
        let (doc, cx) = open(cx, scratch("held", NOTES), "main.tex", 3);
        doc.update_in(cx, |d, window, cx| d.focus.focus(window, cx));
        calm(cx, &doc);
        let at = cx.read(|cx| {
            let d = doc.read(cx);
            let i = d.source.find(from).unwrap();
            if back { i + from.len() } else { i }
        });
        doc.update(cx, |d, cx| d.set_caret(at, cx));
        for press in 0..n {
            let before = cx.read(|cx| doc.read(cx).source.clone());
            doc.update(cx, |d, cx| d.delete(back, cx));
            cx.run_until_parked();
            if pause > 0 {
                std::thread::sleep(Duration::from_millis(pause));
                cx.run_until_parked();
            }
            let after = cx.read(|cx| doc.read(cx).source.clone());
            let lost = before.len().saturating_sub(after.len());
            if lost > 40 && !gone_whole_equation(&before, &after) {
                let cut = before
                    .char_indices()
                    .zip(
                        after
                            .char_indices()
                            .chain(std::iter::repeat((after.len(), ' '))),
                    )
                    .find(|((_, a), (_, b))| a != b)
                    .map_or(0, |((i, _), _)| i);
                worst.push(format!(
                    "{name}: press {press} took {lost} bytes at {cut}: {:?}",
                    &before[cut..(cut + lost).min(before.len())]
                ));
                break;
            }
        }
    }
    assert!(
        worst.is_empty(),
        "a key press took more than what is beside the caret:\n{}",
        worst.join("\n")
    );
}

/// `held_delete` from many places in one document: undone between runs.
#[gpui_kit::test]
#[ignore]
fn held_delete_everywhere(cx: &mut TestAppContext) {
    let pause: u64 = std::env::var("HOLD_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let presses: usize = std::env::var("HOLD_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(45);
    let stride: usize = std::env::var("HOLD_STRIDE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(37);
    let (doc, cx) = open(cx, scratch("held-all", NOTES), "main.tex", 3);
    doc.update_in(cx, |d, window, cx| d.focus.focus(window, cx));
    calm(cx, &doc);
    let (lo, hi) = (
        NOTES.find("\\section{Kinematics}").unwrap(),
        NOTES.find("\\end{document}").unwrap(),
    );
    let mut bad: Vec<String> = vec![];
    let mut tried = 0;
    for start in (lo..hi)
        .step_by(stride)
        .filter(|&i| NOTES.is_char_boundary(i))
    {
        for back in [false, true] {
            doc.update(cx, |d, cx| d.set_caret(start, cx));
            tried += 1;
            let mut trace: Vec<String> = vec![];
            for press in 0..presses {
                let before = cx.read(|cx| doc.read(cx).source.clone());
                doc.update(cx, |d, cx| d.delete(back, cx));
                cx.run_until_parked();
                if pause > 0 {
                    std::thread::sleep(Duration::from_millis(pause));
                    cx.run_until_parked();
                }
                let after = cx.read(|cx| doc.read(cx).source.clone());
                let lost = before.len().saturating_sub(after.len());
                let caret = cx.read(|cx| doc.read(cx).caret.as_ref().map(|c| c.off));
                trace.push(format!(
                    "press {press}: lost {lost}, caret {caret:?}, {} bytes",
                    after.len()
                ));
                if lost > 40 && !gone_whole_equation(&before, &after) {
                    let cut = before
                        .bytes()
                        .zip(after.bytes())
                        .position(|(a, b)| a != b)
                        .unwrap_or(after.len());
                    let cut = (0..=cut)
                        .rev()
                        .find(|&i| before.is_char_boundary(i))
                        .unwrap_or(0);
                    bad.push(format!(
                        "{} from {start} ({:?}), press {press}: {lost} bytes gone at {cut}: {:?}",
                        if back { "backspace" } else { "delete" },
                        &NOTES[start..(start + 12).min(NOTES.len())],
                        before
                            .get(cut..(cut + lost).min(before.len()))
                            .unwrap_or("")
                    ));
                    let from = trace.len().saturating_sub(6);
                    bad.push(format!("  last presses: {:?}", &trace[from..]));
                    break;
                }
            }
            // everything back as it was, then the next place
            for _ in 0..presses * 3 {
                doc.update_in(cx, |d, window, cx| d.undo(window, cx));
            }
            calm(cx, &doc);
            let now = cx.read(|cx| doc.read(cx).source.clone());
            if now != NOTES {
                bad.push(format!("undo did not return the notes after {start}"));
                doc.update(cx, |d, cx| d.replace(0..d.source.len(), NOTES, cx));
                calm(cx, &doc);
            }
        }
    }
    eprintln!("{tried} runs");
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// `text` typed into the formula editor, from outside the document's own update (the editor's
/// change reaches the document as an event, as it does from a key press).
fn type_in_editor(cx: &mut gpui_kit::VisualTestContext, doc: &Entity<Doc>, text: &str) {
    let input = cx.read(|cx| doc.read(cx).input.clone());
    cx.update(|window, cx| {
        input.update(cx, |s, cx| s.replace_text_in_range(None, text, window, cx))
    });
}

/// How many unescaped `$` the TeX has outside comments: odd means a formula left open.
fn dollars(s: &str) -> usize {
    let (mut n, mut chars) = (0, s.chars());
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '%' => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '$' => n += 1,
            _ => {}
        }
    }
    n
}

/// Dollar signs typed the way a person types them, on the pages and in the formula editor:
/// every `$` opens or closes a formula, nothing else is lost, the TeX stays typesettable.
///
/// `cargo test --release dollar_stress -- --ignored --nocapture`; `FUZZ_N` steps, `FUZZ_SEED`.
#[gpui_kit::test]
#[ignore]
fn dollar_stress(cx: &mut TestAppContext) {
    let n: usize = std::env::var("FUZZ_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(90);
    let seed: u64 = std::env::var("FUZZ_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let mut r = Rng(0x9E37_79B9_7F4A_7C15 ^ seed.wrapping_mul(0x2545_F491));
    let (doc, cx) = open(cx, scratch("dollars", NOTES), "main.tex", 3);
    doc.update_in(cx, |d, window, cx| d.focus.focus(window, cx));
    calm(cx, &doc);
    let math = [
        "x",
        "x^2",
        "\\alpha ",
        "\\frac{a}{b}",
        "a_1",
        "\\sum_i ",
        "(a+b)",
    ];
    let mut log: Vec<String> = vec![];
    for i in 0..n {
        let before = cx.read(|cx| doc.read(cx).source.clone());
        let editing = cx.read(|cx| {
            doc.read(cx)
                .active
                .as_ref()
                .is_some_and(|a| a.panel == Some(crate::ui::editor::Panel::Formula))
        });
        let (spots, choice) = (cx.read(|cx| stops(doc.read(cx))), r.below(100));
        let at = spots[r.below(spots.len())];
        let what = if editing {
            match choice {
                0..=34 => {
                    type_in_editor(cx, &doc, "$");
                    "in the editor: $".to_string()
                }
                35..=69 => {
                    let t = math[r.below(math.len())];
                    type_in_editor(cx, &doc, t);
                    format!("in the editor: {t:?}")
                }
                70..=84 => {
                    doc.update_in(cx, |d, window, cx| d.finish_formula(window, cx));
                    "finish the formula".into()
                }
                85..=92 => {
                    doc.update_in(cx, |d, window, cx| d.leave(window, cx));
                    "leave".into()
                }
                _ => {
                    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
                    "undo".into()
                }
            }
        } else {
            match choice {
                0..=39 => {
                    doc.update_in(cx, |d, window, cx| {
                        d.set_caret(at, cx);
                        d.replace_text_in_range(None, "$", window, cx);
                    });
                    format!("$ at {at}")
                }
                40..=54 => {
                    let t = ["a", "b ", "5", "x"][r.below(4)];
                    doc.update_in(cx, |d, window, cx| {
                        d.set_caret(at, cx);
                        d.replace_text_in_range(None, t, window, cx);
                    });
                    format!("type {t:?} at {at}")
                }
                55..=74 => {
                    let back = r.below(2) == 0;
                    let times = 1 + r.below(8);
                    doc.update(cx, |d, cx| {
                        d.set_caret(at, cx);
                        for _ in 0..times {
                            d.delete(back, cx);
                        }
                    });
                    format!(
                        "{} x{times} at {at}",
                        if back { "backspace" } else { "delete" }
                    )
                }
                75..=84 => {
                    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
                    "undo".into()
                }
                85..=92 => {
                    doc.update_in(cx, |d, window, cx| d.redo(window, cx));
                    "redo".into()
                }
                _ => {
                    doc.update_in(cx, |d, window, cx| d.leave(window, cx));
                    "leave".into()
                }
            }
        };
        log.push(what.clone());
        cx.run_until_parked();
        calm(cx, &doc);
        let (bad, status, src, active) = cx.read(|cx| {
            let d = doc.read(cx);
            (
                sound(d).err(),
                d.status.clone(),
                d.source.clone(),
                d.active.is_some(),
            )
        });
        let lost = before.len().saturating_sub(src.len());
        let undid = what.starts_with("undo") || what == "redo";
        // a formula still open in the editor may hold one `$` too many or few while it is typed
        let open_ok = active || what.starts_with("in the editor");
        // (while the formula editor is open its text is raw TeX, as unfinished as typed)
        let mut err = bad.filter(|_| !open_ok);
        if err.is_none() && status.contains("LaTeX") && !open_ok {
            err = Some(status.to_string());
        }
        if err.is_none() && !open_ok && dollars(&src) % 2 == 1 {
            err = Some("an odd number of dollar signs: a formula left open".into());
        }
        // (a delete press is a char, a macro or a formula: x8 presses at most)
        let allowed = if what.starts_with("backspace") || what.starts_with("delete") {
            8 * 45
        } else {
            60
        };
        if err.is_none() && !undid && lost > allowed {
            err = Some(format!("{lost} bytes lost by one step"));
        }
        if let Some(e) = err {
            panic!(
                "step {i} ({what}): {e}\n--- steps:\n{}\n--- before:\n{before}\n--- after:\n{src}",
                log.join("\n")
            );
        }
    }
}

/// A dollar sign typed inside a display formula opens nothing, and a second one (in the editor
/// of a formula opened beside it) cannot take the equation: the equation always stays.
#[gpui_kit::test]
#[ignore]
fn dollar_in_a_display_formula(cx: &mut TestAppContext) {
    let (doc, cx) = open(cx, scratch("dollar-display", NOTES), "main.tex", 3);
    doc.update_in(cx, |d, window, cx| d.focus.focus(window, cx));
    calm(cx, &doc);
    let src = |cx: &mut gpui_kit::VisualTestContext| cx.read(|cx| doc.read(cx).source.clone());
    let at = src(cx).find("\\frac{d^2 x}").unwrap() + "\\frac{d^2 ".len();
    doc.update_in(cx, |d, window, cx| {
        d.set_caret(at, cx);
        d.replace_text_in_range(None, "$", window, cx);
    });
    cx.run_until_parked();
    cx.read(|cx| {
        let d = doc.read(cx);
        let a = d.active.as_ref().map(|a| a.range.clone());
        eprintln!(
            "ACTIVE {a:?} = {:?}; input {:?}",
            a.clone().map(|r| d.source[r].to_string()),
            d.input.read(cx).value()
        );
    });
    type_in_editor(cx, &doc, "$");
    cx.run_until_parked();
    let after = src(cx);
    eprintln!("after the second:\n{after}");
    assert!(
        after.contains("\\begin{equation}") && after.contains("\\end{equation}"),
        "the equation is gone"
    );
    assert!(after.contains("d^2 x}"), "{after}");
    assert_eq!(dollars(&after) % 2, 0);
}

/// Which blocks the caret may enter, and what Backspace does with a caret put inside a display
/// equation (not where a person can click, but where a stale layout may leave it).
#[gpui_kit::test]
#[ignore]
fn backspace_inside_an_equation(cx: &mut TestAppContext) {
    let (doc, cx) = open(cx, scratch("eq-bs", NOTES), "main.tex", 3);
    doc.update_in(cx, |d, window, cx| d.focus.focus(window, cx));
    calm(cx, &doc);
    cx.read(|cx| {
        let d = doc.read(cx);
        for (i, b) in d.blocks.iter().enumerate() {
            eprintln!(
                "BLOCK {i} text={} {:?}",
                d.is_text_block(i),
                d.source[b.range.clone()]
                    .chars()
                    .take(34)
                    .collect::<String>()
            );
        }
        eprintln!("STOPS in the equation: {:?}", {
            let eq = d.source.find("\\begin{equation}").unwrap();
            let end = d.source.find("\\end{equation}").unwrap();
            stops(d)
                .into_iter()
                .filter(|s| *s > eq && *s < end)
                .collect::<Vec<_>>()
        });
    });
    let at = cx.read(|cx| doc.read(cx).source.find("d^2 x").unwrap() + 5);
    let before = cx.read(|cx| doc.read(cx).source.clone());
    doc.update(cx, |d, cx| {
        d.set_caret(at, cx);
        for _ in 0..5 {
            d.delete(true, cx);
        }
    });
    let after = cx.read(|cx| doc.read(cx).source.clone());
    eprintln!(
        "EQUATION AFTER: {:?}",
        after.lines().find(|l| l.starts_with("F ="))
    );
    // the equation goes whole or stays whole, never half; and nothing else goes with it
    assert!(
        after == before || !after.contains("\\frac{^") && !after.contains("d^}"),
        "Backspace left half an equation: {after:?}"
    );
    assert!(balanced(&after));
    assert!(
        before.len() - after.len() <= 70,
        "more than the equation went"
    );
}

/// Backspace held from the start of a paragraph's text, shown press by press.
#[gpui_kit::test]
#[ignore]
fn backspace_run_shown(cx: &mut TestAppContext) {
    let (doc, cx) = open(cx, scratch("bs-run", NOTES), "main.tex", 3);
    doc.update_in(cx, |d, window, cx| d.focus.focus(window, cx));
    calm(cx, &doc);
    let from: usize = std::env::var("FROM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(85);
    let presses: usize = std::env::var("PRESSES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    doc.update(cx, |d, cx| d.set_caret(from, cx));
    for p in 0..presses {
        let before = cx.read(|cx| doc.read(cx).source.clone());
        doc.update(cx, |d, cx| d.delete(true, cx));
        cx.run_until_parked();
        let (after, caret) = cx.read(|cx| {
            let d = doc.read(cx);
            (d.source.clone(), d.caret.as_ref().map(|c| c.off))
        });
        let line = after.lines().find(|l| l.contains("moves")).unwrap_or("");
        eprintln!(
            "PRESS {p}: lost {} caret {caret:?} | {line}",
            before.len().saturating_sub(after.len())
        );
    }
}
