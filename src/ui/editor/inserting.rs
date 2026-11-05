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
