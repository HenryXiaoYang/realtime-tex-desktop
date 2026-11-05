// The ribbon's side of a document: what the text at the caret is set in (for the buttons'
// state) and the formatting operations, applied to the selection or the caret's word or blocks
// as one undo step, with the packages they need added to the preamble.
use super::*;
use document::format::{self, Align, Fmt, Font, Size, Style};

/// A formatting operation from the ribbon or a shortcut.
#[derive(Clone, Debug, PartialEq)]
pub enum FormatOp {
    /// An inline command toggled: `textbf`, `textit`, `underline`…
    Inline(&'static str),
    Size(Size),
    /// One named size up (1) or down (-1).
    Grow(i32),
    Font(Font),
    Color(String),
    /// `itemize` or `enumerate`.
    List(&'static str),
    /// A list item one level in (true) or out.
    Indent(bool),
    Align(Align),
    Style(Style),
}

/// What the ribbon shows as on at the caret.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RibbonState {
    /// The inline commands open at the caret (`textbf`, `emph`…).
    pub formats: Vec<String>,
    pub size: &'static str,
    pub align: Option<Align>,
    /// The list the caret is in: `itemize`, `enumerate`.
    pub list: Option<String>,
    pub style: Option<Style>,
    /// The heading levels the document class has.
    pub headings: Vec<&'static str>,
    /// There is text to format: a caret or a selection on the pages.
    pub enabled: bool,
    /// The named font (fontspec) and the colour of the text at the caret.
    pub font: Option<String>,
    pub color: Option<String>,
}

impl RibbonState {
    pub fn has(&self, cmd: &str) -> bool {
        self.formats
            .iter()
            .any(|f| f == cmd || (cmd == "textit" && f == "emph"))
    }
}

impl Doc {
    /// The selection, or the caret as an empty one.
    fn format_target(&self) -> Option<Range<usize>> {
        let c = self.caret.as_ref()?;
        Some(self.selection_range().unwrap_or(c.off..c.off))
    }

    /// The blocks the target touches, from the first's start to the last's end.
    fn target_blocks(&self, t: &Range<usize>) -> Option<Range<usize>> {
        let hit: Vec<&document::Block> = self
            .blocks
            .iter()
            .filter(|b| b.range.start <= t.end && b.range.end >= t.start)
            .collect();
        Some(hit.first()?.range.start..hit.last()?.range.end)
    }

    /// What the ribbon shows for the caret.
    pub fn ribbon_state(&self) -> RibbonState {
        let headings = format::heading_levels(&self.source);
        let Some(t) = self.format_target().filter(|_| self.active.is_none()) else {
            return RibbonState {
                size: "normalsize",
                headings,
                ..Default::default()
            };
        };
        let Some(blocks) = self.target_blocks(&t) else {
            return RibbonState {
                size: "normalsize",
                headings,
                enabled: true,
                ..Default::default()
            };
        };
        let item = document::caret::item_at(&self.source, t.start);
        let (font, color) = format::font_and_color_at(&self.source, blocks.clone(), t.start);
        RibbonState {
            font,
            color,
            formats: format::formats_at(&self.source, blocks.clone(), t.start),
            size: format::size_at(&self.source, blocks.clone(), t.start),
            align: Some(format::alignment_of(&self.source, blocks.clone())),
            list: item.map(|it| it.env),
            style: Some(format::style_of(&self.source, blocks.clone())),
            headings,
            enabled: true,
        }
    }

    /// Apply `op` to the selection (or the caret's word, or the caret's blocks), as one undo
    /// step; the packages it needs go in the preamble, with a note in the status bar.
    pub fn format(&mut self, op: FormatOp, window: &mut Window, cx: &mut Context<Self>) {
        self.leave(window, cx);
        self.focus.focus(window, cx);
        let Some(t) = self.format_target() else {
            return;
        };
        let blocks = self.target_blocks(&t);
        let src = &self.source;
        let inline_block = blocks.clone().unwrap_or(t.clone());
        let fmt: Option<Fmt> = match &op {
            FormatOp::Inline(cmd) => format::toggle_inline(src, inline_block, t.clone(), cmd),
            FormatOp::Size(s) => format::set_size(src, inline_block, t.clone(), *s),
            FormatOp::Grow(step) => format::step_size(src, inline_block, t.clone(), *step),
            FormatOp::Font(f) => format::set_font(src, inline_block, t.clone(), f),
            FormatOp::Color(c) => format::set_color(src, inline_block, t.clone(), c),
            FormatOp::List(env) => blocks.and_then(|b| format::toggle_list(src, b, t.start, env)),
            FormatOp::Align(a) => blocks.and_then(|b| format::align(src, b, *a)),
            FormatOp::Style(s) => blocks.and_then(|b| format::set_style(src, b, *s)),
            FormatOp::Indent(inward) => {
                return self.tab(!*inward, cx);
            }
        };
        let Some(fmt) = fmt else {
            self.status = "Nothing to format there".into();
            cx.notify();
            return;
        };
        self.checkpoint();
        self.typing = None;
        self.overlay = None;
        self.replace(fmt.range.clone(), &fmt.text, cx);
        // the selection over what was formatted (the caret inside an empty group)
        if let Some(c) = self.caret.as_mut() {
            c.off = fmt.select.end;
            c.anchor = (!fmt.select.is_empty()).then_some(fmt.select.start);
            c.goal_x = None;
        }
        self.add_packages(&fmt.packages, cx);
        cx.notify();
    }

    /// The packages `packages` before `\begin{document}` when the preamble lacks them, with a
    /// note in the status bar (the caret and selection follow); fontspec also names LuaLaTeX.
    pub(super) fn add_packages(&mut self, packages: &[&str], cx: &mut Context<Self>) {
        let mut added = vec![];
        for p in packages {
            if let Some((at, text)) = format::add_package(&self.source, p) {
                self.replace(at..at, &text, cx);
                added.push(*p);
            }
        }
        if packages.contains(&"fontspec")
            && let Some((at, text)) = format::add_lualatex(&self.source)
        {
            self.replace(at..at, &text, cx);
        }
        if !added.is_empty() {
            self.status = format!(
                "Added {} to the preamble",
                added
                    .iter()
                    .map(|p| format!("\\usepackage{{{p}}}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
}
