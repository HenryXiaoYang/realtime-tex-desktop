// Find and replace over the text a document prints, apart from any UI: matches are found in
// the printed characters (markup between them does not count, formulas and pictures are
// skipped) and come back as source ranges; a replacement keeps markup a match runs through.
use super::caret::{self, atom_regions, delete_range};
use super::glyphmap::visible_chars;
use std::ops::Range;

/// The source ranges where `query` is printed in `blocks` of `src`, in order. `case`: match
/// case exactly.
pub fn find(src: &str, blocks: &[Range<usize>], query: &str, case: bool) -> Vec<Range<usize>> {
    let fold = |c: char| {
        if case {
            c
        } else {
            c.to_lowercase().next().unwrap_or(c)
        }
    };
    let q: Vec<char> = query.chars().map(fold).collect();
    if q.is_empty() {
        return vec![];
    }
    let mut out = vec![];
    for b in blocks {
        let Some(text) = src.get(b.clone()) else {
            continue;
        };
        let atoms = atom_regions(text);
        let chars: Vec<(char, Range<usize>)> = visible_chars(text)
            .into_iter()
            .filter(|(_, r)| !atoms.iter().any(|a| a.start <= r.start && r.end <= a.end))
            .map(|(c, r)| (fold(c), r.start + b.start..r.end + b.start))
            .collect();
        let mut i = 0;
        while i + q.len() <= chars.len() {
            if chars[i..i + q.len()]
                .iter()
                .map(|c| c.0)
                .eq(q.iter().copied())
            {
                out.push(chars[i].1.start..chars[i + q.len() - 1].1.end);
                i += q.len();
            } else {
                i += 1;
            }
        }
    }
    out
}

/// Replacing match `m` of `src` with `with` (plain text, escaped): the range that changes and
/// its new text. Markup the match runs through stays whole around the new text.
pub fn replace(src: &str, m: Range<usize>, with: &str) -> (Range<usize>, String) {
    let esc = caret::escape(with);
    let printed: String = visible_chars(&src[m.clone()]).iter().map(|c| c.0).collect();
    let plain = src[m.clone()].chars().all(|c| {
        !matches!(
            c,
            '\\' | '{' | '}' | '$' | '%' | '&' | '~' | '^' | '_' | '#'
        )
    });
    if plain && printed.chars().count() == src[m.clone()].chars().count() {
        return (m, esc);
    }
    let e = delete_range(src, m.start, m.end);
    let at = e.caret - e.range.start;
    let mut text = e.text;
    text.insert_str(at.min(text.len()), &esc);
    (e.range, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All of `src` as the one block.
    fn whole(src: &str) -> Vec<Range<usize>> {
        std::iter::once(0..src.len()).collect()
    }

    #[test]
    fn finds_printed_text_through_markup() {
        let src = "Some \\textbf{bo}ld text, then $bold$ math and Bold.";
        let all = whole(src);
        let m = find(src, &all, "bold", false);
        // through the \textbf, and the capital one; not in the formula
        assert_eq!(m.len(), 2);
        assert_eq!(&src[m[0].clone()], "bo}ld");
        assert_eq!(&src[m[1].clone()], "Bold");
        assert_eq!(find(src, &all, "bold", true).len(), 1);
        assert!(find(src, &all, "", false).is_empty());
    }

    #[test]
    fn replacing_keeps_the_markup() {
        let src = "Some bold here";
        let m = find(src, &whole(src), "bold", false)[0].clone();
        assert_eq!(replace(src, m, "50%"), (5..9, "50\\%".to_string()));
        let src = "Some \\textbf{bo}ld text";
        let m = find(src, &whole(src), "bold", false)[0].clone();
        let (r, t) = replace(src, m, "strong");
        let mut out = src.to_string();
        out.replace_range(r, &t);
        assert_eq!(out, "Some \\textbf{strong} text");
    }
}
