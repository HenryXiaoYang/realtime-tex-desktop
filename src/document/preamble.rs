// The preamble as the page-setup sidebar shows it: the packages it loads, and the paper with its
// margins to scale. Custom macros (`\newcommand` and the like) are not part of it: they stay in
// the preamble's source.
use super::element::{Change, Field, change, find_cmd};

/// What a margin is drawn as when the preamble sets none (or one that is not a plain length):
/// about what the standard classes leave.
const DEFAULT_MARGIN_MM: f32 = 25.4;

/// Each `\usepackage` of the preamble that is not commented out, with the packages it names.
fn uses(src: &str) -> Vec<(super::element::Cmd, Vec<String>)> {
    let Some(begin) = src.find("\\begin{document}") else {
        return vec![];
    };
    let (mut out, mut from) = (vec![], 0);
    while let Some(c) = find_cmd(src, from..begin, "usepackage") {
        from = c.end;
        let Some(arg) = c.arg.clone().filter(|_| !commented(src, c.start)) else {
            continue;
        };
        let names = src[arg]
            .split(',')
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .collect();
        out.push((c, names));
    }
    out
}

/// Whether `at` is after a `%` on its line.
fn commented(src: &str, at: usize) -> bool {
    let line = src[..at].rsplit('\n').next().unwrap_or("");
    let mut prev = ' ';
    for c in line.chars() {
        if c == '%' && prev != '\\' {
            return true;
        }
        prev = c;
    }
    false
}

/// The packages the preamble loads, in order.
pub fn packages(src: &str) -> Vec<String> {
    uses(src).into_iter().flat_map(|(_, names)| names).collect()
}

/// The edit that stops the preamble loading `name`: its `\usepackage` line, or just its name out
/// of a list.
pub fn remove_package(src: &str, name: &str) -> Option<Change> {
    let (c, names) = uses(src)
        .into_iter()
        .find(|(_, names)| names.iter().any(|n| n == name))?;
    if names.len() > 1 {
        let rest: Vec<&str> = names
            .iter()
            .map(String::as_str)
            .filter(|n| *n != name)
            .collect();
        return change(c.arg?, rest.join(","));
    }
    let line_start = src[..c.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = src[c.end..].find('\n').map_or(src.len(), |i| c.end + i + 1);
    let alone =
        src[line_start..c.start].trim().is_empty() && src[c.end..line_end].trim().is_empty();
    change(
        if alone {
            line_start..line_end
        } else {
            c.start..c.end
        },
        "",
    )
}

/// A package name as LaTeX has them: letters, digits, `-` and `_`.
pub fn is_package_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The paper and its margins, in millimetres, for drawing the page to scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageShape {
    pub width: f32,
    pub height: f32,
    /// Top, bottom, left, right.
    pub margins: [f32; 4],
}

/// A length in millimetres, when it is in a unit the page is measured in.
pub fn length_mm(s: &str) -> Option<f32> {
    let s = s.trim();
    let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))?;
    let n: f32 = s[..split].parse().ok()?;
    let per = match s[split..].trim() {
        "mm" => 1.0,
        "cm" => 10.0,
        "in" => 25.4,
        "pt" => 25.4 / 72.27,
        "bp" => 25.4 / 72.0,
        _ => return None,
    };
    Some(n * per)
}

/// The page the document-element `fields` describe: its paper (turned for landscape) and the
/// margin each side has (its own, else the one for all sides).
pub fn page_shape(fields: &[Field]) -> PageShape {
    let get = |k: &str| {
        fields
            .iter()
            .find(|f| f.key == k)
            .map_or("", |f| f.value.as_str())
    };
    let (w, h) = match get("paper") {
        "a4paper" => (210.0, 297.0),
        "a5paper" => (148.0, 210.0),
        "legalpaper" => (215.9, 355.6),
        _ => (215.9, 279.4),
    };
    let (width, height) = if get("landscape") == "true" {
        (h, w)
    } else {
        (w, h)
    };
    let side = |k: &str| {
        [get(k), get("margin")]
            .into_iter()
            .find(|v| !v.is_empty())
            .and_then(length_mm)
            .unwrap_or(DEFAULT_MARGIN_MM)
    };
    PageShape {
        width,
        height,
        margins: ["top", "bottom", "left", "right"].map(side),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::element::{Kind, element_at, fields, set};

    const DOC: &str = "\\documentclass[a4paper]{article}\n% \\usepackage{old}\n\\usepackage{amsmath}\n\\usepackage[utf8]{inputenc, fontenc}\n\\newcommand{\\R}{\\mathbb{R}}\n\\begin{document}\nx\n\\end{document}";

    fn apply(src: &str, c: Change) -> String {
        let mut s = src.to_string();
        s.replace_range(c.range, &c.text);
        s
    }

    #[test]
    fn lists_the_packages_the_preamble_loads() {
        assert_eq!(packages(DOC), ["amsmath", "inputenc", "fontenc"]);
        assert!(packages("no document yet").is_empty());
    }

    #[test]
    fn removes_a_package_alone_or_from_a_list() {
        let s = apply(DOC, remove_package(DOC, "amsmath").unwrap());
        assert!(!s.contains("amsmath") && s.contains("% \\usepackage{old}\n\\usepackage[utf8]"));
        let s = apply(DOC, remove_package(DOC, "inputenc").unwrap());
        assert!(s.contains("\\usepackage[utf8]{fontenc}\n"), "{s}");
        assert!(s.contains("\\newcommand{\\R}"), "macros stay");
        assert_eq!(
            remove_package(DOC, "old"),
            None,
            "a commented one is not loaded"
        );
        assert_eq!(remove_package(DOC, "tikz"), None);
    }

    #[test]
    fn measures_lengths_the_page_is_drawn_in() {
        assert_eq!(length_mm("2cm"), Some(20.0));
        assert_eq!(length_mm(" 1in "), Some(25.4));
        assert_eq!(length_mm("15mm"), Some(15.0));
        assert_eq!(length_mm("0.5\\textheight"), None);
        assert_eq!(length_mm("2em"), None);
        assert_eq!(length_mm(""), None);
    }

    #[test]
    fn draws_the_page_from_its_fields() {
        let page = |src: &str| page_shape(&fields(src, &element_at(src, None, 0)));
        let p = page(DOC);
        assert_eq!((p.width, p.height), (210.0, 297.0));
        assert_eq!(p.margins, [DEFAULT_MARGIN_MM; 4]);
        // all sides, then one side of its own, and landscape
        let el = |s: &str| element_at(s, None, 0);
        let s = apply(DOC, set(DOC, &el(DOC), "margin", "2cm").unwrap());
        let s = apply(&s, set(&s, &el(&s), "left", "3cm").unwrap());
        let s = apply(&s, set(&s, &el(&s), "landscape", "true").unwrap());
        assert_eq!(el(&s).kind, Kind::Document);
        let p = page(&s);
        assert_eq!((p.width, p.height), (297.0, 210.0));
        assert_eq!(p.margins, [20.0, 20.0, 30.0, 20.0]);
        assert!(
            s.contains("\\documentclass[a4paper, landscape]{article}"),
            "{s}"
        );
    }

    #[test]
    fn package_names_are_plain() {
        assert!(is_package_name("amsmath") && is_package_name("pgf-plots_2"));
        assert!(!is_package_name("") && !is_package_name("a b") && !is_package_name("a}{b"));
    }
}
