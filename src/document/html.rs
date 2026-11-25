// Rich text pasted from another app (a web page, Word, Pages) as TeX: the basics carry over —
// bold, italic, underline, links, headings, paragraphs, line breaks, lists, code, super- and
// subscripts, and the inline styles editors write them with — and everything else becomes
// plain text. No HTML library: the clipboard's HTML is small and only these tags matter.
use super::caret::escape;

/// `html` as TeX, with the packages it needs (hyperref for links).
pub fn html_to_tex(html: &str) -> (String, Vec<&'static str>) {
    let mut out = String::new();
    let mut packages = vec![];
    // open tags: their name and what closes them in the TeX
    let mut open: Vec<(String, String)> = vec![];
    let mut pre = 0;
    let mut i = 0;
    let b = html.as_bytes();
    while i < html.len() {
        if html[i..].starts_with("<!--") {
            i = html[i..].find("-->").map_or(html.len(), |j| i + j + 3);
            continue;
        }
        if b[i] == b'<' {
            let Some(end) = html[i..].find('>').map(|j| i + j) else {
                break;
            };
            let tag = &html[i + 1..end];
            i = end + 1;
            let closing = tag.starts_with('/');
            let tag = tag.trim_start_matches('/').trim_end_matches('/');
            let (name, attrs) = tag.split_once(char::is_whitespace).unwrap_or((tag, ""));
            let name = name.to_ascii_lowercase();
            // preformatted text is shown as it is: a tag in it is not markup (`<pre><code>`), but
            // a line break, and `</pre>` ends it
            if pre > 0 && !(closing && name == "pre") {
                if name == "br" && !closing {
                    out.push('\n');
                }
                continue;
            }
            // what these hold is not text
            if !closing && ["style", "script", "head", "title", "xml"].contains(&name.as_str()) {
                let close = format!("</{name}");
                i = html[i..]
                    .to_ascii_lowercase()
                    .find(&close)
                    .map_or(html.len(), |j| {
                        let k = i + j;
                        html[k..].find('>').map_or(html.len(), |e| k + e + 1)
                    });
                continue;
            }
            if closing {
                if let Some(k) = open.iter().rposition(|(n, _)| *n == name) {
                    while open.len() > k {
                        let (n, close) = open.pop().unwrap();
                        if n == "pre" && close.contains("verbatim") {
                            pre -= 1;
                        }
                        out.push_str(&close);
                    }
                }
                continue;
            }
            let style = attr(attrs, "style")
                .unwrap_or_default()
                .to_ascii_lowercase();
            let (opener, closer) = match name.as_str() {
                "b" | "strong" => ("\\textbf{".to_string(), "}".to_string()),
                "i" | "em" | "cite" => ("\\textit{".into(), "}".into()),
                "u" | "ins" => ("\\underline{".into(), "}".into()),
                "code" | "tt" | "kbd" | "samp" => ("\\texttt{".into(), "}".into()),
                // (verbatim cannot stand in another command's argument: there, plain monospace)
                "pre" if open.iter().any(|(_, c)| c.contains('}')) => {
                    ("\\texttt{".into(), "}".into())
                }
                "pre" => {
                    pre += 1;
                    (
                        "\n\n\\begin{verbatim}\n".into(),
                        "\n\\end{verbatim}\n\n".into(),
                    )
                }
                "sup" => ("\\textsuperscript{".into(), "}".into()),
                "sub" => ("\\textsubscript{".into(), "}".into()),
                "a" => match attr(attrs, "href").filter(|h| !h.starts_with('#')) {
                    Some(h) => {
                        if !packages.contains(&"hyperref") {
                            packages.push("hyperref");
                        }
                        (
                            format!("\\href{{{}}}{{", h.replace('%', "\\%").replace('#', "\\#")),
                            "}".into(),
                        )
                    }
                    None => (String::new(), String::new()),
                },
                "h1" => ("\n\n\\section{".into(), "}\n\n".into()),
                "h2" => ("\n\n\\subsection{".into(), "}\n\n".into()),
                "h3" => ("\n\n\\subsubsection{".into(), "}\n\n".into()),
                "h4" | "h5" | "h6" => ("\n\n\\paragraph{".into(), "}\n\n".into()),
                "p" | "div" | "blockquote" | "tr" => ("\n\n".into(), "\n\n".into()),
                "ul" => ("\n\n\\begin{itemize}".into(), "\n\\end{itemize}\n\n".into()),
                "ol" => (
                    "\n\n\\begin{enumerate}".into(),
                    "\n\\end{enumerate}\n\n".into(),
                ),
                "li" => ("\n  \\item ".into(), String::new()),
                "td" | "th" => (" ".into(), " ".into()),
                "br" => {
                    out.push_str("\\\\\n");
                    continue;
                }
                _ => (String::new(), String::new()),
            };
            // bold, italic and underline written as styles (Word, Google Docs, browsers)
            let (mut o, mut c) = (opener, closer);
            let has = |k: &str, vs: &[&str]| {
                style.split(';').any(|d| {
                    d.split_once(':').is_some_and(|(p, v)| {
                        p.trim() == k && vs.iter().any(|x| v.trim().starts_with(x))
                    })
                })
            };
            if !matches!(
                name.as_str(),
                "b" | "strong" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
            ) && has("font-weight", &["bold", "600", "700", "800", "900"])
            {
                o.push_str("\\textbf{");
                c.insert(0, '}');
            }
            if !matches!(name.as_str(), "i" | "em") && has("font-style", &["italic"]) {
                o.push_str("\\textit{");
                c.insert(0, '}');
            }
            if name != "u" && has("text-decoration", &["underline"]) {
                o.push_str("\\underline{");
                c.insert(0, '}');
            }
            out.push_str(&o);
            // void elements close at once
            if ["img", "hr", "meta", "link", "input", "col", "wbr"].contains(&name.as_str()) {
                out.push_str(&c);
            } else {
                open.push((name, c));
            }
            continue;
        }
        // text up to the next tag
        let next = html[i..].find('<').map_or(html.len(), |j| i + j);
        let text = decode(&html[i..next]);
        if pre > 0 {
            // (the one line verbatim cannot hold)
            out.push_str(&text.replace("\\end{verbatim}", "\\end{verbatim }"));
        } else {
            let squashed: String = text
                .split(|c: char| c.is_whitespace() && c != '\u{a0}')
                .collect::<Vec<_>>()
                .join(" ");
            out.push_str(&escape(&squashed));
        }
        i = next;
    }
    while let Some((_, close)) = open.pop() {
        out.push_str(&close);
    }
    (tidy(&out), packages)
}

/// The value of attribute `name` in a tag's attributes.
fn attr(attrs: &str, name: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name).map(|i| i + from) {
        let before_ok = i == 0 || !lower.as_bytes()[i - 1].is_ascii_alphanumeric();
        let rest = attrs[i + name.len()..].trim_start();
        if before_ok && let Some(v) = rest.strip_prefix('=') {
            let v = v.trim_start();
            let (q, body) = match v.chars().next() {
                Some(q @ ('"' | '\'')) => (Some(q), &v[1..]),
                _ => (None, v),
            };
            let end = match q {
                Some(q) => body.find(q).unwrap_or(body.len()),
                None => body.find(char::is_whitespace).unwrap_or(body.len()),
            };
            return Some(decode(&body[..end]));
        }
        from = i + name.len();
    }
    None
}

/// HTML entities decoded.
fn decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(semi) = tail.find(';').filter(|&j| j <= 10) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let ent = &tail[1..semi];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            "ndash" => Some('–'),
            "mdash" => Some('—'),
            "lsquo" => Some('‘'),
            "rsquo" => Some('’'),
            "ldquo" => Some('“'),
            "rdquo" => Some('”'),
            "hellip" => Some('…'),
            _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                u32::from_str_radix(&ent[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &tail[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Blank lines at most one in a row, no empty formatting, no spaces at line ends.
fn tidy(tex: &str) -> String {
    let mut t = tex.to_string();
    for empty in ["\\textbf{}", "\\textit{}", "\\underline{}", "\\texttt{}"] {
        while t.contains(empty) {
            t = t.replace(empty, "");
        }
    }
    let lines: Vec<&str> = t.lines().map(str::trim_end).collect();
    let mut out = String::new();
    let mut blank = 0;
    for l in lines {
        if l.trim().is_empty() {
            blank += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str(if blank > 0 { "\n\n" } else { "\n" });
        }
        blank = 0;
        out.push_str(l.trim_start_matches(' '));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::html_to_tex;

    fn tex(html: &str) -> String {
        html_to_tex(html).0
    }

    #[test]
    fn inline_formatting_carries_over() {
        assert_eq!(
            tex("Some <b>bold</b> and <em>italic</em> text"),
            "Some \\textbf{bold} and \\textit{italic} text"
        );
        assert_eq!(
            tex("<span style=\"font-weight:700\">x</span> y"),
            "\\textbf{x} y"
        );
        assert_eq!(
            tex("<span style='font-style: italic; color:red'>x</span>"),
            "\\textit{x}"
        );
        assert_eq!(
            tex("a &amp; b &lt;3 50% $5 &#233;"),
            "a \\& b <3 50\\% \\$5 é"
        );
        let (t, p) = html_to_tex("<a href=\"https://x.org/a#b\">site</a>");
        assert_eq!(t, "\\href{https://x.org/a\\#b}{site}");
        assert_eq!(p, vec!["hyperref"]);
    }

    #[test]
    fn blocks_become_paragraphs_headings_and_lists() {
        assert_eq!(tex("<p>One</p><p>Two</p>"), "One\n\nTwo");
        assert_eq!(
            tex("<h2>Title</h2><p>Body</p>"),
            "\\subsection{Title}\n\nBody"
        );
        assert_eq!(
            tex("<ul><li>a</li><li><b>b</b></li></ul>"),
            "\\begin{itemize}\n\\item a\n\\item \\textbf{b}\n\\end{itemize}"
        );
        assert_eq!(tex("line<br>next"), "line\\\\\nnext");
    }

    #[test]
    fn preformatted_text_is_verbatim_whatever_tags_it_holds() {
        // GitHub and most editors write code as <pre><code>: no \texttt in what is shown as is
        assert_eq!(
            tex("<pre><code>let x = 1;<br>y</code></pre>"),
            "\\begin{verbatim}\nlet x = 1;\ny\n\\end{verbatim}"
        );
        assert_eq!(
            tex("<pre><b>a</b> % # &amp; <i>b</i></pre>after"),
            "\\begin{verbatim}\na % # & b\n\\end{verbatim}\n\nafter"
        );
        // the one line verbatim cannot hold
        assert!(!tex("<pre>\\end{verbatim} x</pre>").contains("\\end{verbatim} x"));
        // not inside another command's argument, where verbatim does not typeset
        let t = tex("<b><pre>a_b</pre></b><h1><pre>c</pre></h1>");
        assert!(!t.contains("verbatim"), "{t}");
        assert!(crate::document::caret::balanced(&t), "{t}");
    }

    #[test]
    fn what_is_not_text_goes() {
        let word = "<html><head><style>p{color:red}</style></head><body><!--[if gte mso 9]><xml>junk</xml><![endif]--><p class=MsoNormal><b><span lang=EN>Bold</span></b> plain<o:p></o:p></p></body></html>";
        assert_eq!(tex(word), "\\textbf{Bold} plain");
        assert_eq!(tex("<meta charset='utf-8'><span>plain</span>"), "plain");
    }
}
