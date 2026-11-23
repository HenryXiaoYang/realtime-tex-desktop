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

/// An empty inline formula, with a space to hold the editor open: removed again if it stays blank.
pub const EMPTY_INLINE: &str = "$ $";

#[cfg(test)]
mod tests {
    use super::*;

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
