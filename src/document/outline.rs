// The section outline of a TeX source, for the project panel.

pub struct Heading {
    pub level: usize,
    pub number: Option<String>,
    pub title: String,
    pub offset: usize,
}

/// \section, \subsection, \subsubsection and \paragraph commands, numbered like article.cls
/// (paragraphs are not).
pub fn outline(src: &str) -> Vec<Heading> {
    const CMDS: [&str; 4] = [
        "\\section",
        "\\subsection",
        "\\subsubsection",
        "\\paragraph",
    ];
    let s = src.as_bytes();
    let mut counters = [0usize; 3];
    let mut out = vec![];
    let mut i = 0;
    while let Some(k) = src[i..].find('\\') {
        i += k;
        let line_start = src[..i].rfind('\n').map_or(0, |n| n + 1);
        let commented = src[line_start..i].contains('%');
        let Some(level) = (0..CMDS.len()).rev().find(|&l| {
            src[i..].starts_with(CMDS[l])
                && !s
                    .get(i + CMDS[l].len())
                    .is_some_and(u8::is_ascii_alphabetic)
        }) else {
            i += 1;
            continue;
        };
        let offset = i;
        let mut j = i + CMDS[level].len();
        let starred = s.get(j) == Some(&b'*');
        j += starred as usize;
        if s.get(j) == Some(&b'[') {
            j += src[j..].find(']').map_or(0, |n| n + 1);
        }
        let Some(open) = src[j..]
            .find('{')
            .filter(|n| src[j..j + n].trim().is_empty())
            .map(|n| j + n)
        else {
            i = j;
            continue;
        };
        let mut depth = 0;
        let mut end = open;
        for (n, b) in s[open..].iter().enumerate() {
            match b {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + n;
                        break;
                    }
                }
                _ => {}
            }
        }
        i = end.max(j) + 1;
        if commented || end == open {
            continue;
        }
        let number = (!starred && level < counters.len()).then(|| {
            counters[level] += 1;
            counters[level + 1..].iter_mut().for_each(|c| *c = 0);
            counters[..=level]
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(".")
        });
        out.push(Heading {
            level,
            number,
            // as it prints: no \\textcolor or braces in the panel
            title: crate::document::glyphmap::plain_text(&src[open + 1..end]),
            offset,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_sections() {
        let o = outline(
            "\\section{A}\n%\\section{no}\n\\subsection*{B}\n\\subsection{C {x}}\n\\section[s]{D \\textcolor{red}{ab}}",
        );
        let got: Vec<(Option<&str>, &str)> = o
            .iter()
            .map(|h| (h.number.as_deref(), h.title.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                (Some("1"), "A"),
                (None, "B"),
                (Some("1.1"), "C x"),
                (Some("2"), "D ab")
            ]
        );
    }

    #[test]
    fn paragraphs_are_listed_unnumbered() {
        let o = outline("\\section{A}\n\\paragraph{P}\n\\subparagraph{no}\n\\subsection{B}");
        let got: Vec<(usize, Option<&str>, &str)> = o
            .iter()
            .map(|h| (h.level, h.number.as_deref(), h.title.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![(0, Some("1"), "A"), (3, None, "P"), (1, Some("1.1"), "B")]
        );
    }
}
