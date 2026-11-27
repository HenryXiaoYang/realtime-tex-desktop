// Formulas as the editor treats them: which one a spot in a block is in, and the TeX inside its
// delimiters, which is all the formula editor shows.
use super::blocks::split_display_math;
use super::caret::math_regions;
use std::ops::Range;

/// The formula (delimiters included) that `off` of `src` is in or at the edge of: `$…$`, `\(…\)`,
/// `$$…$$`, `\[…\]` or a math environment.
pub fn formula_at(src: &str, off: usize) -> Option<Range<usize>> {
    let display = split_display_math(src, 0..src.len())
        .into_iter()
        .filter(|(_, math)| *math)
        .map(|(r, _)| r);
    math_regions(src)
        .into_iter()
        .chain(display)
        .find(|r| r.start <= off && off <= r.end)
}

/// The TeX inside `formula`'s delimiters (an environment's body, without its `\begin` and `\end`).
pub fn body(src: &str, formula: Range<usize>) -> Option<Range<usize>> {
    let text = src.get(formula.clone())?;
    let (open, close) = if let Some(rest) = text.strip_prefix("\\begin{") {
        let name = rest.split('}').next()?;
        (name.len() + 8, format!("\\end{{{name}}}").len())
    } else {
        let open = ["$$", "\\[", "\\("]
            .iter()
            .find(|d| text.starts_with(**d))
            .map_or(1, |d| d.len());
        if !text.starts_with(['$', '\\']) {
            return None;
        }
        (open, open)
    };
    (text.len() >= open + close).then(|| formula.start + open..formula.end - close)
}

/// What goes when the formula editor, open on `at` (a formula's inside), has nothing in it: the
/// formula with its delimiters, and only when it holds nothing. Never an enclosing formula (a
/// `$$` left by an emptied `$ $` inside an equation reads as a display delimiter, and the
/// equation around it is not blank).
pub fn blank_formula(src: &str, at: Range<usize>) -> Option<Range<usize>> {
    if !src.get(at.clone())?.trim().is_empty() {
        return None;
    }
    // the dollar signs on both sides
    // (not `$$…$$`, a display: one sign of each pair would leave a lone `$$`)
    let (before, after) = (&src[..at.start], &src[at.end..]);
    if before.ends_with('$')
        && after.starts_with('$')
        && !(before.ends_with("$$") && after.starts_with("$$"))
    {
        return Some(at.start - 1..at.end + 1);
    }
    // another delimiter (`\(`…`\)`, `\[`…`\]`, `$$`…`$$`): the formula found must be all delimiters
    let whole = formula_at(src, at.start)?;
    let inside = body(src, whole.clone())?;
    src[inside].trim().is_empty().then_some(whole)
}

/// An empty inline formula, with a space to hold the editor open: removed again if it stays blank.
pub const EMPTY_INLINE: &str = "$ $";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_blank_formula_goes_never_the_one_around_it() {
        let eq = "\\begin{equation}\nF = m \\frac{d^2 $$x}{dt^2}\n\\end{equation}";
        // the `$$` an emptied `$ $` left in an equation: the two signs go, the equation stays
        let at = eq.find("$$").unwrap() + 1;
        assert_eq!(blank_formula(eq, at..at), Some(at - 1..at + 1));
        // a blank inline formula goes with its signs, spaces and all
        let s = "a $ $ b";
        assert_eq!(blank_formula(s, 3..4).map(|r| &s[r]), Some("$ $"));
        // nothing blank, or no formula there: nothing goes
        assert_eq!(blank_formula("a $x$ b", 3..4), None);
        let e = "\\begin{equation}\nF = m\n\\end{equation}";
        assert_eq!(blank_formula(e, 20..20), None);
        // a display formula with nothing in it goes whole
        // an emptied `$$…$$` goes whole, never down to a lone `$$`
        let dd = "a $$$$ b";
        assert_eq!(blank_formula(dd, 4..4).map(|r| &dd[r]), Some("$$$$"));
        let dd = "a $$ $$ b";
        assert_eq!(blank_formula(dd, 4..5).map(|r| &dd[r]), Some("$$ $$"));
        let d = "a \\[ \\] b";
        assert_eq!(blank_formula(d, 4..5).map(|r| &d[r]), Some("\\[ \\]"));
    }

    #[test]
    fn an_inline_formula_inside_a_display_one_is_its_own() {
        let src = "\\begin{equation}\nF = m \\frac{d^2 $ $x}{dt^2}\n\\end{equation}";
        let off = src.find("$ $").unwrap() + 1;
        let r = formula_at(src, off).unwrap();
        assert_eq!(&src[r], "$ $");
    }

    fn at(src: &str, marker: &str) -> Option<String> {
        let off = src.find(marker)?;
        formula_at(src, off).map(|r| src[r].to_string())
    }

    #[test]
    fn finds_the_formula_a_spot_is_in() {
        let src =
            "Let $x^2$ and \\(y\\) be; then\n\\[\n  a = b\n\\]\nand \\begin{align}c\\end{align}.";
        assert_eq!(at(src, "x^2").unwrap(), "$x^2$");
        assert_eq!(at(src, "y\\)").unwrap(), "\\(y\\)");
        assert_eq!(at(src, "a = b").unwrap(), "\\[\n  a = b\n\\]");
        assert_eq!(at(src, "c\\end").unwrap(), "\\begin{align}c\\end{align}");
        assert_eq!(at(src, "be;"), None);
    }

    #[test]
    fn takes_the_tex_inside_the_delimiters() {
        let inner = |s: &str| body(s, 0..s.len()).map(|r| s[r].to_string());
        assert_eq!(inner("$x^2$").unwrap(), "x^2");
        assert_eq!(inner("\\(y\\)").unwrap(), "y");
        assert_eq!(inner("$$a$$").unwrap(), "a");
        assert_eq!(inner("\\[ a \\]").unwrap(), " a ");
        assert_eq!(
            inner("\\begin{align*}\na&=b\n\\end{align*}").unwrap(),
            "\na&=b\n"
        );
        assert_eq!(inner(EMPTY_INLINE).unwrap(), " ");
        assert_eq!(inner("plain"), None);
        assert_eq!(inner("$"), None);
    }
}
