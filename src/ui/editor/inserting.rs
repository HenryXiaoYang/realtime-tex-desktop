// The Insert tab's edits at the caret: a block (a figure, a table, a page break, space, the
// contents) splits the paragraph there; a reference, a citation or a link goes in as text; a
// footnote or a display formula opens the source box to be written in. Pictures (chosen,
// pasted or dropped) are copied into figures/ beside the document.
use super::*;
use document::caret::{self, Edit, MARK};
use document::insert::{self, BibEntry, IMAGE_TYPES, Label};

impl Doc {
    /// The folder the document is in.
    pub fn dir(&self) -> PathBuf {
        let p = self.root.join(&self.path);
        p.parent().map_or(self.root.clone(), Path::to_path_buf)
    }

    /// `tex` (its caret at MARK, else after it) as a block of its own at the caret, or after
    /// the selected blocks, its packages added.
    pub fn insert_block(&mut self, tex: &str, packages: &[&str], cx: &mut Context<Self>) {
        let caret = self.caret.as_ref().map(|c| c.off);
        let e = match (caret, self.selection()) {
            (Some(off), _) => match self.block_of(off) {
                Some(bi) => {
                    caret::insert_block(&self.source, self.blocks[bi].range.clone(), off, tex)
                }
                None => Edit::marked(off..off, &format!("\n\n{tex}\n\n")),
            },
            (None, Some((_, last))) => {
                let end = self.blocks[last].range.end;
                caret::insert_block(&self.source, self.blocks[last].range.clone(), end, tex)
            }
            // nowhere chosen (a picture dropped beside the text): at the document's end
            (None, None) => {
                let end = self
                    .source
                    .rfind("\\end{document}")
                    .unwrap_or(self.source.len());
                let pre = self.source[..end].trim_end().len();
                Edit::marked(pre..end, &format!("\n\n{}\n\n", tex.replace(MARK, "")))
            }
        };
        self.selected = None;
        if self.caret.is_none() {
            self.caret = Some(Caret {
                off: e.caret,
                anchor: None,
                goal_x: None,
                marked: None,
            });
        }
        if let Some(c) = self.caret.as_mut() {
            c.anchor = None;
        }
        self.apply(e, cx);
        self.add_packages(packages, cx);
    }

    /// `tex` at the caret, over the selection, the caret after it.
    pub fn insert_inline(&mut self, tex: &str, packages: &[&str], cx: &mut Context<Self>) {
        if self.caret.is_none() {
            return;
        }
        // one undo step with the selection it replaces
        if !self.delete_selection(cx) {
            self.checkpoint();
        }
        self.typing = None;
        self.overlay = None;
        let Some(off) = self.caret.as_ref().map(|c| c.off) else {
            return;
        };
        if self.replace(off..off, tex, cx)
            && let Some(c) = self.caret.as_mut()
        {
            c.off = off + tex.len();
            c.goal_x = None;
        }
        self.add_packages(packages, cx);
        cx.notify();
    }

    /// The caret moved to where a footnote or a display formula can go (out of a caption, a
    /// heading, a table's cell: after the block). `heading_ok`: a footnote may stay in a
    /// heading, protected from the contents' moving argument; whether it did.
    fn caret_for_box(&mut self, heading_ok: bool) -> bool {
        let Some(off) = self.caret.as_ref().map(|c| c.off) else {
            return false;
        };
        let Some(b) = self.block_of(off).map(|bi| self.blocks[bi].range.clone()) else {
            return false;
        };
        let heading = caret::heading_title(&self.source[b.clone()], b.start)
            .is_some_and(|t| t.start <= off && off <= t.end);
        if heading_ok && heading {
            return true;
        }
        if let Some(c) = self.caret.as_mut() {
            c.off = caret::box_spot(&self.source, b, off);
            c.anchor = None;
        }
        false
    }

    /// A footnote at the caret, written in the source box.
    pub fn insert_footnote(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let p = if self.caret_for_box(true) {
            "\\protect"
        } else {
            ""
        };
        self.edit_tex_at_caret(&format!("{p}\\footnote{{{}}}", complete::MARK), window, cx);
    }

    /// A numbered display formula at the caret, written in the source box.
    pub fn insert_display_math(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let m = complete::MARK;
        self.caret_for_box(false);
        self.edit_tex_at_caret(
            &format!("\n\\begin{{equation}}\n  {m}\n\\end{{equation}}\n"),
            window,
            cx,
        );
    }

    /// A `rows` × `cols` table at the caret.
    pub fn insert_table(&mut self, rows: usize, cols: usize, cx: &mut Context<Self>) {
        let label = insert::new_label(&self.source, "tab", &(self.labels().len() + 1).to_string());
        self.insert_block(&insert::table(rows, cols, &label), &[], cx);
    }

    /// A link to `url` at the caret: over the selection, the selection becomes its text; else
    /// `text` (or the URL itself).
    pub fn insert_link(&mut self, url: &str, text: &str, cx: &mut Context<Self>) {
        if url.trim().is_empty() {
            return;
        }
        let sel = self.selection_range();
        // the selection becomes the link's text when it is still what the text field holds
        let wrap = sel.clone().filter(|r| {
            let t = &self.source[r.clone()];
            caret::balanced(t)
                && !t.contains("\n\n")
                && !t.contains("\\href")
                && !t.contains("\\url")
                && (text.trim().is_empty() || text.trim() == self.selected_text().trim())
        });
        if sel.is_some() && wrap.is_none() && text.trim().is_empty() {
            self.status = "A link's text is one stretch of a paragraph: select less".into();
            cx.notify();
            return;
        }
        match wrap {
            Some(r) => {
                let tex = insert::link(url, &self.source[r.clone()]);
                let end = r.start + tex.len();
                self.apply(
                    Edit {
                        range: r,
                        text: tex,
                        caret: end,
                    },
                    cx,
                );
                if let Some(c) = self.caret.as_mut() {
                    c.anchor = None;
                }
                self.add_packages(&["hyperref"], cx);
            }
            _ => self.insert_inline(&insert::link(url, &caret::escape(text)), &["hyperref"], cx),
        }
    }

    /// The selection's text, for a link's.
    pub fn selected_text(&self) -> String {
        self.selection_range()
            .map(|r| glyphmap::plain_text(&self.source[r]))
            .unwrap_or_default()
    }

    /// The labels in the document, for a cross-reference.
    // shortcut: this file's labels only, not those of files it \input's; read those too when
    // projects split into files need it.
    pub fn labels(&self) -> Vec<Label> {
        insert::labels(&self.source)
    }

    /// The entries of the .bib files the document cites from.
    pub fn bib_entries(&self) -> Vec<BibEntry> {
        insert::bib_files(&self.source, &self.dir())
            .iter()
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .flat_map(|t| insert::bib_entries(&t))
            .collect()
    }

    /// A citation of `key` at the caret, after a citation just before it.
    pub fn insert_citation(&mut self, key: &str, cx: &mut Context<Self>) {
        // right after `\cite{a}`: one citation of both
        if let Some(off) = self.caret.as_ref().map(|c| c.off)
            && self.selection_range().is_none()
            && self.source[..off].ends_with('}')
            && let Some(open) = self.source[..off].rfind("\\cite{")
            && !self.source[open..off - 1].contains('}')
        {
            let text = format!(",{key}");
            let caret = off + text.len();
            self.apply(
                Edit {
                    range: off - 1..off - 1,
                    text,
                    caret,
                },
                cx,
            );
            return;
        }
        self.insert_inline(&format!("\\cite{{{key}}}"), &[], cx);
    }

    /// Pictures as figures at the caret (or where they were dropped), copied into figures/.
    pub fn insert_pictures(&mut self, files: &[PathBuf], cx: &mut Context<Self>) {
        for f in files {
            let name = f
                .file_name()
                .map_or("image.png".into(), |n| n.to_string_lossy().to_string());
            match self.picture_path(f, &name) {
                Ok(rel) => self.insert_figure(&rel, cx),
                Err(e) => self.status = format!("Could not insert {name}: {e}"),
            }
            cx.notify();
        }
    }

    /// A pasted picture (`bytes` in format `ext`) as a figure at the caret.
    pub fn insert_picture_bytes(&mut self, bytes: &[u8], ext: &str, cx: &mut Context<Self>) {
        let tmp = std::env::temp_dir().join(format!("realtime-tex-paste.{ext}"));
        let res = std::fs::write(&tmp, bytes)
            .map_err(|e| e.to_string())
            .and_then(|_| self.picture_path(&tmp, &format!("pasted.{ext}")));
        let _ = std::fs::remove_file(&tmp);
        match res {
            Ok(rel) => self.insert_figure(&rel, cx),
            Err(e) => self.status = format!("Could not paste the picture: {e}"),
        }
        cx.notify();
    }

    /// How the document names picture `file` (called `name`): where it is when it is in the
    /// document's folder already, else its copy in figures/ (as PNG when LaTeX cannot read it).
    fn picture_path(&self, file: &Path, name: &str) -> Result<String, String> {
        let dir = self.dir();
        let ext = file
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let readable = IMAGE_TYPES.contains(&ext.as_str());
        if readable
            && let (Ok(f), Ok(d)) = (file.canonicalize(), dir.canonicalize())
            && let Ok(rel) = f.strip_prefix(&d)
            && let rel = rel.to_string_lossy().replace('\\', "/")
            // a name \includegraphics cannot take as it is gets a copy that it can
            && !rel.contains(['%', '#', '{', '}', '~', '$', '&', '^', '\\'])
            && Path::new(&rel).file_stem().is_some_and(|s| !s.to_string_lossy().contains('.'))
        {
            return Ok(rel);
        }
        let (bytes, name) = if readable {
            (
                std::fs::read(file).map_err(|e| e.to_string())?,
                name.to_string(),
            )
        } else {
            let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
            (to_png(file)?, format!("{stem}.png"))
        };
        insert::save_into_figures(&dir, &name, &bytes).map_err(|e| e.to_string())
    }

    /// A figure of picture `rel`, its caption from the file's name, the caret in the caption.
    fn insert_figure(&mut self, rel: &str, cx: &mut Context<Self>) {
        let stem = Path::new(rel)
            .file_stem()
            .map_or(String::new(), |s| s.to_string_lossy().to_string());
        let label = insert::new_label(&self.source, "fig", &stem);
        let caption = caret::escape(&stem.replace(['-', '_'], " "));
        // `rel` has no TeX specials (picture_path saw to it)
        let tex = insert::figure(rel, &label, &caption);
        debug_assert!(tex.contains(MARK));
        self.insert_block(&tex, &["graphicx"], cx);
    }
}

/// Whether `file` is a picture that can go in (as it is, or converted).
pub(super) fn is_picture(file: &Path) -> bool {
    let ext = file
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    IMAGE_TYPES.contains(&ext.as_str())
        || ["gif", "tif", "tiff", "heic", "webp", "bmp"].contains(&ext.as_str())
}

/// A picture LaTeX cannot read (GIF, TIFF, HEIC, WebP…) as PNG.
// shortcut: converts with macOS's `sips`; other platforms need an image crate here.
fn to_png(file: &Path) -> Result<Vec<u8>, String> {
    let out = std::env::temp_dir().join(format!("realtime-tex-{}.png", std::process::id()));
    let ok = std::process::Command::new("sips")
        .args(["-s", "format", "png"])
        .arg(file)
        .arg("--out")
        .arg(&out)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let bytes = std::fs::read(&out).ok().filter(|_| ok);
    let _ = std::fs::remove_file(&out);
    bytes.ok_or_else(|| "LaTeX cannot read this kind of picture".into())
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::typing::tests::calm;
    use super::Doc;
    use crate::document::element::Kind;
    use crate::document::insert;
    use crate::engine;
    use crate::ui::settings::AppSettings;
    use gpui_kit::{Entity, EntityInputHandler, TestAppContext, VisualTestContext, Window};

    /// A 1×1 PNG.
    pub(in crate::ui::editor) const PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2,
        0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 0, 0,
        3, 1, 1, 0, 201, 254, 146, 239, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];

    fn src(cx: &mut VisualTestContext, doc: &Entity<Doc>) -> String {
        cx.read(|cx| doc.read(cx).source.clone())
    }

    /// The edit typesets without a LaTeX error, and one undo takes it back.
    fn typesets_and_undoes(cx: &mut VisualTestContext, doc: &Entity<Doc>, base: &str, what: &str) {
        calm(cx, doc);
        let status = cx.read(|cx| doc.read(cx).status.clone());
        assert!(!status.contains("LaTeX"), "{what}: {status}");
        doc.update_in(cx, |d, window, cx| d.undo(window, cx));
        assert_eq!(src(cx, doc), base, "{what}: one undo step");
        calm(cx, doc);
    }

    /// Everything the Insert tab puts in, typeset by rtex: `RTD_TYPE=/abs/main.tex cargo test
    /// --release insert_on_pages -- --ignored --nocapture` (a copy: it writes figures/ there).
    #[gpui_kit::test]
    #[ignore]
    fn insert_on_pages(cx: &mut TestAppContext) {
        let file = std::env::var("RTD_TYPE").expect("RTD_TYPE=/abs/path/main.tex");
        let path = std::path::PathBuf::from(&file);
        let root = path.parent().unwrap().to_path_buf();
        let main = path.file_name().unwrap().to_string_lossy().to_string();
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        engine::configure("", "/Library/TeX/texbin");
        let (doc, cx) = cx.add_window_view(|window: &mut Window, cx| {
            Doc::new(root.clone(), main, Default::default(), window, cx)
        });
        super::super::typing::tests::wait(cx, &doc, "the first layout", |d| {
            d.spans_pending == 0 && d.blocks.iter().filter(|b| !b.rows.is_empty()).count() > 3
        });
        calm(cx, &doc);
        let base = src(cx, &doc);
        let off = base.find("Used together").expect("the paragraph") + "Used ".len();
        let at = |d: &mut Doc, cx: &mut gpui_kit::Context<Doc>| d.set_caret(off, cx);

        // a page break splits the paragraph at the caret
        doc.update(cx, |d, cx| {
            at(d, cx);
            d.insert_block("\\newpage", &[], cx);
        });
        assert!(src(cx, &doc).contains("Used \n\n\\newpage\n\ntogether"));
        typesets_and_undoes(cx, &doc, &base, "page break");

        // a picture: copied into figures/, a figure with graphicx, the sidebar on it
        let pic = std::env::temp_dir().join("rtd test pic.png");
        std::fs::write(&pic, PNG).unwrap();
        doc.update(cx, |d, cx| {
            at(d, cx);
            d.insert_pictures(std::slice::from_ref(&pic), cx);
        });
        let s = src(cx, &doc);
        assert!(
            s.contains("\\includegraphics[width=0.8\\linewidth]{figures/rtd-test-pic.png}"),
            "{s}"
        );
        assert!(s.contains("\\label{fig:rtd-test-pic}"));
        assert!(s.contains("graphicx"));
        assert!(root.join("figures/rtd-test-pic.png").exists());
        calm(cx, &doc);
        assert_eq!(
            cx.read(|cx| doc.read(cx).element().kind),
            Kind::Figure,
            "the caret is in the new figure"
        );
        // a cross-reference to it
        doc.update(cx, |d, cx| {
            let l = d.labels().into_iter().find(|l| l.key == "fig:rtd-test-pic");
            let l = l.expect("its label");
            assert_eq!(l.kind, "Figure");
            // (graphicx went into the preamble, above it)
            let off = d
                .source
                .find("Used \n\n\\begin{figure}")
                .expect("the split")
                + 5;
            d.set_caret(off, cx);
            d.insert_inline(&l.reference(), &[], cx);
        });
        assert!(src(cx, &doc).contains("Used Figure~\\ref{fig:rtd-test-pic}\n\n\\begin{figure}"));
        calm(cx, &doc);
        doc.update_in(cx, |d, window, cx| d.undo(window, cx));
        typesets_and_undoes(cx, &doc, &base, "picture");
        let _ = std::fs::remove_dir_all(root.join("figures"));

        // a table
        doc.update(cx, |d, cx| {
            at(d, cx);
            d.insert_table(3, 3, cx);
        });
        assert!(src(cx, &doc).contains("\\begin{tabular}{|l|l|l|}"));
        typesets_and_undoes(cx, &doc, &base, "table");

        // a link over the selected word
        doc.update(cx, |d, cx| {
            at(d, cx);
            d.extending(cx, |d, cx| d.move_word(1, cx));
            assert_eq!(d.selected_text(), "together");
            // the dialog's text field starts as the selection's text
            d.insert_link("https://example.com/#x", "together", cx);
        });
        let s = src(cx, &doc);
        assert!(
            s.contains("\\href{https://example.com/\\#x}{together}"),
            "link"
        );
        assert!(s.contains("hyperref"));
        typesets_and_undoes(cx, &doc, &base, "link");
        // text typed over the selection's: the link shows it instead
        doc.update(cx, |d, cx| {
            at(d, cx);
            d.extending(cx, |d, cx| d.move_word(1, cx));
            d.insert_link("https://example.com", "a site", cx);
        });
        assert!(src(cx, &doc).contains("Used \\href{https://example.com}{a site}"));
        typesets_and_undoes(cx, &doc, &base, "link with its own text");

        // a footnote and a display formula open the source box at the caret
        doc.update_in(cx, |d, window, cx| {
            at(d, cx);
            d.insert_footnote(window, cx);
        });
        cx.run_until_parked();
        doc.update_in(cx, |d, window, cx| {
            assert!(d.active.is_some(), "footnote box");
            let v = d.input.read(cx).value().to_string();
            let cur = d.input.read(cx).cursor();
            assert!(
                v[..cur].ends_with("\\footnote{"),
                "the caret in the footnote"
            );
            d.leave(window, cx);
        });
        calm(cx, &doc);
        let s = src(cx, &doc);
        let i = s.find("Used ").unwrap();
        assert!(
            s.contains("Used \\footnote{}together"),
            "{:?}",
            &s[i..i + 40]
        );
        doc.update_in(cx, |d, window, cx| d.undo(window, cx));
        calm(cx, &doc);
        assert_eq!(src(cx, &doc), base, "footnote undone");
        // a display formula: an equation in the box, the caret in it
        doc.update_in(cx, |d, window, cx| {
            at(d, cx);
            d.insert_display_math(window, cx);
        });
        cx.run_until_parked();
        doc.update_in(cx, |d, window, cx| {
            let v = d.input.read(cx).value().to_string();
            let cur = d.input.read(cx).cursor();
            assert!(v[..cur].ends_with("\\begin{equation}\n  "), "{v:?}");
            d.input.update(cx, |s, cx| {
                s.replace_text_in_range(None, "E=mc^2", window, cx)
            });
        });
        cx.run_until_parked();
        doc.update_in(cx, |d, window, cx| d.leave(window, cx));
        calm(cx, &doc);
        let s = src(cx, &doc);
        assert!(
            s.contains("\\begin{equation}\n  E=mc^2\n\\end{equation}"),
            "equation"
        );
        let status = cx.read(|cx| doc.read(cx).status.clone());
        assert!(!status.contains("LaTeX"), "equation: {status}");
    }

    /// Every Insert-tab action at the start, middle and end of every text block of a document
    /// with lists, tables, a minipage, formulas (or RTD_TYPE's): each typesets without a LaTeX
    /// error and one undo restores the source. `cargo test --release insert_everywhere --
    /// --ignored --nocapture` (RTD_SWEEP_ONLY=3:figure runs that one).
    #[gpui_kit::test]
    #[ignore]
    fn insert_everywhere(cx: &mut TestAppContext) {
        let dir = std::env::temp_dir().join(format!("rtd-insert-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // RTD_TYPE=/abs/main.tex sweeps that document instead (copied: it writes beside it)
        let fixture = std::env::var("RTD_TYPE")
            .map(|p| std::fs::read_to_string(p).unwrap())
            .unwrap_or_else(|_| super::super::element_edit::tests::DOC.into());
        std::fs::write(dir.join("main.tex"), fixture).unwrap();
        std::fs::write(dir.join("pic.png"), PNG).unwrap();
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        engine::configure("", "/Library/TeX/texbin");
        let (doc, cx) = cx.add_window_view(|window: &mut Window, cx| {
            Doc::new(
                dir.clone(),
                "main.tex".into(),
                Default::default(),
                window,
                cx,
            )
        });
        super::super::typing::tests::wait(cx, &doc, "the first layout", |d| {
            d.spans_pending == 0 && d.blocks.iter().filter(|b| !b.rows.is_empty()).count() > 3
        });
        calm(cx, &doc);
        let base = src(cx, &doc);
        let spots: Vec<(usize, String, [usize; 3])> = cx.read(|cx| {
            let d = doc.read(cx);
            (0..d.blocks.len())
                .filter(|&b| d.is_text_block(b))
                .filter_map(|b| {
                    let page = d.blocks[b].rows.first()?.page;
                    let st: Vec<usize> = d.map_of(b, page)?.stops.iter().map(|s| s.off).collect();
                    let name: String = d.source[d.blocks[b].range.clone()]
                        .chars()
                        .take(30)
                        .collect();
                    Some((b, name, [st[0], st[st.len() / 2], *st.last()?]))
                })
                .collect()
        });
        type Op = (
            &'static str,
            usize,
            fn(&mut Doc, &mut Window, &mut gpui_kit::Context<Doc>),
        );
        let ops: [Op; 9] = [
            ("pagebreak", 0, |d, _, cx| {
                d.insert_block("\\newpage", &[], cx)
            }),
            ("figure", 1, |d, _, cx| {
                d.insert_block(
                    &insert::figure("pic.png", "fig:sweep", "Sweep"),
                    &["graphicx"],
                    cx,
                )
            }),
            ("table", 2, |d, _, cx| d.insert_table(2, 2, cx)),
            ("contents", 1, |d, _, cx| {
                d.insert_block("\\tableofcontents", &[], cx)
            }),
            ("ref", 1, |d, _, cx| {
                d.insert_inline("Figure~\\ref{fig:a}", &[], cx)
            }),
            ("cite-merge", 1, |d, _, cx| d.insert_citation("k", cx)),
            ("link", 1, |d, _, cx| {
                d.insert_link("https://example.com/a b", "site", cx)
            }),
            ("footnote", 1, |d, w, cx| d.insert_footnote(w, cx)),
            ("equation", 1, |d, w, cx| d.insert_display_math(w, cx)),
        ];
        let only = std::env::var("RTD_SWEEP_ONLY").ok();
        let (mut failures, mut n, t0) = (vec![], 0, std::time::Instant::now());
        for (bi, name, at) in &spots {
            for (op, pos, f) in &ops {
                if only
                    .as_ref()
                    .is_some_and(|o| !format!("{bi}:{op}").starts_with(o.as_str()))
                {
                    continue;
                }
                n += 1;
                let what = format!("block {bi} {name:?}: {op}");
                doc.update_in(cx, |d, window, cx| {
                    d.status.clear();
                    d.set_caret(at[*pos], cx);
                    f(d, window, cx);
                });
                cx.run_until_parked();
                // a box opened (footnote, equation): write in it and leave
                doc.update_in(cx, |d, window, cx| {
                    if d.active.is_some() {
                        d.input
                            .update(cx, |s, cx| s.replace_text_in_range(None, "x", window, cx));
                    }
                });
                cx.run_until_parked();
                doc.update_in(cx, |d, window, cx| {
                    if d.active.is_some() {
                        d.leave(window, cx);
                    }
                });
                calm(cx, &doc);
                let (s, status) = cx.read(|cx| {
                    let d = doc.read(cx);
                    (d.source.clone(), d.status.clone())
                });
                // a citation of a key no .bib has warns, it does not fail
                if status.contains("LaTeX") {
                    let r = cx.read(|cx| doc.read(cx).last_edit.clone());
                    let around =
                        &s[r.start.saturating_sub(40).min(s.len())..(r.end + 40).min(s.len())];
                    failures.push(format!("{what} → {status} (around {around:?})"));
                }
                if s == base {
                    failures.push(format!("{what} → nothing went in"));
                    continue;
                }
                doc.update_in(cx, |d, window, cx| d.undo(window, cx));
                calm(cx, &doc);
                if src(cx, &doc) != base {
                    failures.push(format!("{what} → undo did not restore the source"));
                    let b = base.clone();
                    doc.update(cx, |d, cx| {
                        d.replace(0..d.source.len(), &b, cx);
                    });
                    calm(cx, &doc);
                }
            }
        }
        println!(
            "{n} insertions at {} spots in {:?}",
            spots.len(),
            t0.elapsed()
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }
}
