// One open file: for a .tex file, its typeset pages from `rtex serve`, where the block under the
// cursor (a paragraph, a heading, a display formula) turns into its TeX source in place and every
// keystroke goes to rtex, which re-typesets it in milliseconds. Other text files open as source.
mod complete;
#[cfg(test)]
mod e2e;
mod editing;
mod element_edit;
mod file_view;
mod find_bar;
mod formatting;
mod inserting;
mod menu;
mod navigation;
mod pages;
mod selection;
mod session;
mod source_box;
mod text_selection;
mod typing;

use crate::document::{self, glyphmap};
use crate::engine;
use crate::render::{self, viewer};
use crate::ui::settings::AppSettings;
pub use formatting::{FormatOp, RibbonState};
use futures::StreamExt;
use gpui_kit::component::input::{
    Copy, Cut, Editor, EditorState, InputEvent, Paste, SelectAll, highlight_characters,
};
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_kit::component::scroll::{Scrollbar, ScrollbarAxis, ScrollbarMode};
use gpui_kit::component::{ActiveTheme, Icon, Sizable, Size, spinner::Spinner};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use source_box::*;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use typing::*;

const SP_PER_BP: f64 = 65781.76;
pub const ZOOMS: [f32; 9] = [0.5, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];
const PAGE_GAP: f32 = 24.0;
/// Above the first page: padding, then the collapsed preamble strip and its gap.
const PAGES_TOP: f32 = 32.0 + FRAME_H + 8.0;
const FRAME_H: f32 = 26.0;
pub const MONO: &str = "Menlo";

struct PageImage {
    img: Arc<RenderImage>,
    key: (u64, u32),
}

actions!(
    doc,
    [
        EditBlock,
        DeleteBlock,
        DuplicateBlock,
        MoveBlockUp,
        MoveBlockDown,
        SelectPrevBlock,
        SelectNextBlock,
        ExtendSelectionUp,
        ExtendSelectionDown,
        ClearSelection,
        CaretLeft,
        CaretRight,
        CaretUp,
        CaretDown,
        CaretWordLeft,
        CaretWordRight,
        CaretLineStart,
        CaretLineEnd,
        CaretDocStart,
        CaretDocEnd,
        Backspace,
        DeleteForward,
        NewParagraph,
        LineBreak,
        Indent,
        Outdent,
        SelectCaretBlock,
        InsertMath,
        MoveParagraphUp,
        MoveParagraphDown,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectWordLeft,
        SelectWordRight,
        SelectLineStart,
        SelectLineEnd,
        SelectDocStart,
        SelectDocEnd,
        FormatBold,
        FormatItalic,
        FormatUnderline,
        FontBigger,
        FontSmaller,
        Heading1,
        Heading2,
        Heading3,
        AlignLeft,
        AlignCenter,
        AlignRight,
        AlignJustify,
        PastePlain,
        OpenFind,
        OpenReplace,
        FindNext,
        FindPrev,
        CloseFind
    ]
);

/// The fixed editing keys on the pages, while no source box is open: for the caret ("Caret"), or
/// for the selected blocks ("Pages"); commands' keys are in ui::keymap, which you can change. Inside a box the editor's own bindings win, so ⌘A, ⇧↑ and the rest act on
/// its text.
pub fn bind_keys(cx: &mut App) {
    let caret = Some("Caret");
    cx.bind_keys([
        KeyBinding::new("left", CaretLeft, caret),
        KeyBinding::new("right", CaretRight, caret),
        KeyBinding::new("up", CaretUp, caret),
        KeyBinding::new("down", CaretDown, caret),
        KeyBinding::new("alt-left", CaretWordLeft, caret),
        KeyBinding::new("alt-right", CaretWordRight, caret),
        KeyBinding::new("cmd-left", CaretLineStart, caret),
        KeyBinding::new("cmd-right", CaretLineEnd, caret),
        KeyBinding::new("home", CaretLineStart, caret),
        KeyBinding::new("end", CaretLineEnd, caret),
        KeyBinding::new("cmd-up", CaretDocStart, caret),
        KeyBinding::new("cmd-down", CaretDocEnd, caret),
        KeyBinding::new("backspace", Backspace, caret),
        KeyBinding::new("delete", DeleteForward, caret),
        KeyBinding::new("enter", NewParagraph, caret),
        KeyBinding::new("shift-enter", LineBreak, caret),
        KeyBinding::new("tab", Indent, caret),
        KeyBinding::new("shift-tab", Outdent, caret),
        KeyBinding::new("escape", SelectCaretBlock, caret),
        KeyBinding::new("cmd-v", Paste, caret),
        KeyBinding::new("cmd-c", Copy, caret),
        KeyBinding::new("cmd-x", Cut, caret),
        KeyBinding::new("cmd-a", SelectAll, caret),
        KeyBinding::new("shift-left", SelectLeft, caret),
        KeyBinding::new("shift-right", SelectRight, caret),
        KeyBinding::new("shift-up", SelectUp, caret),
        KeyBinding::new("shift-down", SelectDown, caret),
        KeyBinding::new("alt-shift-left", SelectWordLeft, caret),
        KeyBinding::new("alt-shift-right", SelectWordRight, caret),
        KeyBinding::new("cmd-shift-left", SelectLineStart, caret),
        KeyBinding::new("cmd-shift-right", SelectLineEnd, caret),
        KeyBinding::new("shift-home", SelectLineStart, caret),
        KeyBinding::new("shift-end", SelectLineEnd, caret),
        KeyBinding::new("cmd-shift-up", SelectDocStart, caret),
        KeyBinding::new("cmd-shift-down", SelectDocEnd, caret),
    ]);
    cx.bind_keys([
        KeyBinding::new("escape", CloseFind, Some("FindBar")),
        KeyBinding::new("cmd-g", FindNext, Some("FindBar")),
        KeyBinding::new("cmd-shift-g", FindPrev, Some("FindBar")),
    ]);
    let pages = Some("Pages");
    cx.bind_keys([
        KeyBinding::new("enter", EditBlock, pages),
        KeyBinding::new("backspace", DeleteBlock, pages),
        KeyBinding::new("delete", DeleteBlock, pages),
        KeyBinding::new("up", SelectPrevBlock, pages),
        KeyBinding::new("down", SelectNextBlock, pages),
        KeyBinding::new("shift-up", ExtendSelectionUp, pages),
        KeyBinding::new("shift-down", ExtendSelectionDown, pages),
        KeyBinding::new("cmd-a", SelectAll, pages),
        KeyBinding::new("escape", ClearSelection, pages),
        KeyBinding::new("cmd-c", Copy, pages),
        KeyBinding::new("cmd-x", Cut, pages),
        KeyBinding::new("cmd-v", Paste, pages),
    ]);
}

/// A page, a box on it in sp, and the font of the text there.
type BoxSpot = (u32, [f64; 4], Option<engine::FontDesc>);
/// A page and where it was last drawn (window coordinates).
type PageBounds = (u32, Bounds<Pixels>);
/// A right-click: page, y (sp) and the start of the block there.
type MenuTarget = (u32, f64, Option<usize>);

/// The block being edited as source.
struct Active {
    span: u64,
    /// Its bytes in `source`, kept up to date as the user types.
    range: Range<usize>,
    /// The preamble or the trailer, which are edited in a strip off the page.
    frame: bool,
    /// New text typed into empty space on a page, until it typesets as a block of its own.
    gap: Option<Gap>,
    /// Where the box was last drawn (page, box in sp): it stays there while the text has no
    /// typeset lines (a lone `\`, an error). A box that is not drawn loses focus and closes.
    last: Cell<Option<BoxSpot>>,
}

impl Active {
    fn new(span: u64, range: Range<usize>, frame: bool, gap: Option<Gap>) -> Self {
        Self {
            span,
            range,
            frame,
            gap,
            last: Cell::new(None),
        }
    }
}

/// Where the source box is drawn: a page, its box in sp, and whether that is a block's lines
/// (else a spot without lines yet).
struct BoxPlace {
    page: u32,
    rect: [f64; 4],
    /// The font the text is typeset in: the source is shown in it, at its size.
    font: Option<engine::FontDesc>,
    /// How many typeset lines the box replaces on its page.
    count: usize,
    lines: bool,
    /// It holds several blocks' source (a multi-block selection being edited).
    multi: bool,
}

/// How far below the block it follows new text's box opens, in sp.
const GAP_PAD: f64 = 3.0 * 65536.0;

/// Where the right-click menu puts new text.
#[derive(Clone, Copy)]
enum Spot {
    Above(usize),
    Below(usize),
    /// Empty space at y (sp) on a page.
    Gap(u32, f64),
}

struct Gap {
    page: u32,
    /// Where the box opens, in sp from the page top.
    y: f64,
    /// The blank line that makes it a paragraph goes before it (after the previous block) or,
    /// above the first block, after it.
    sep_before: bool,
}

pub struct Doc {
    /// A PDF or image shown in the viewer instead of text.
    pub view: Option<viewer::View>,
    /// The viewer's rendered pages: page to (zoom key, image).
    view_images: HashMap<usize, (u64, Arc<RenderImage>)>,
    root: PathBuf,
    /// Relative to `root`.
    pub path: String,
    /// Typeset with rtex (a .tex file); otherwise edited as plain source.
    pub tex: bool,
    pub source: String,
    pub dirty: bool,
    pub status: String,
    /// A full compile is under way for an edit the live path could not take.
    pub compiling: bool,
    rtex: Option<engine::Rtex>,
    spans: Vec<engine::Span>,
    spans_pending: usize,
    preview: document::Preview,
    blocks: Vec<document::Block>,
    pub outline: Vec<document::Heading>,
    resources: Arc<Mutex<render::Resources>>,
    layout_version: u64,
    images: HashMap<u32, PageImage>,
    versions: HashMap<u32, u64>,
    inflight: HashSet<(u32, u64, u32)>,
    live: Option<(Arc<RenderImage>, [f64; 4])>,
    pub zoom: f32,
    /// Rasterizing waits until a pinch has paused; the stretched old images show meanwhile.
    zoomed_at: Instant,
    scroll: ScrollHandle,
    page_bounds: Rc<Cell<Vec<PageBounds>>>,
    /// The source box's bounds (window coordinates) and the page and height of the lines it
    /// replaces: the page below it is shifted down by the difference.
    box_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    box_cut: Cell<Option<(u32, f32)>>,
    /// What the source box was last sized with.
    box_fit: Rc<Cell<BoxFit>>,
    /// Document fonts registered for the source box: file to family (None: unusable).
    doc_fonts: std::cell::RefCell<HashMap<String, Option<Font>>>,
    /// The insert line under the pointer: its page, height (sp) and where a new block goes.
    hover_gap: Option<(u32, f64, Spot)>,
    /// The last single click's page, point (sp) and block: the first half of a double-click.
    first_click: Option<(u32, f64, f64, Option<usize>)>,
    /// The source before each edit made on the pages (a source box, a menu action), for ⌘Z
    /// outside the box; the box and Source mode have their own undo while focused.
    undo: Vec<String>,
    redo: Vec<String>,
    /// The pages', so actions reach the workspace while no editor is focused.
    focus: FocusHandle,
    /// What the last right-click hit: page, y (sp) and the start of the block there, for the
    /// context menu. Its actions find the block again by that offset when they run.
    menu_target: Rc<Cell<Option<MenuTarget>>>,
    /// The selected blocks (outlined) while no box is open: the anchor and head blocks, by the
    /// source offsets of their starts. Right-click selects one; Shift extends; ⌘A takes all.
    selected: Option<(usize, usize)>,
    input: Entity<EditorState>,
    /// The whole file as source in a code editor: a .tex file's Source mode, any other text file.
    code: Entity<EditorState>,
    /// A .tex file shown as its source instead of its pages.
    pub source_mode: bool,
    /// Line numbers and soft wrap as last applied to `code`.
    code_opts: (bool, bool),
    autosave: Option<Task<()>>,
    active: Option<Active>,
    /// Visual mode's text caret, while no source box is open.
    caret: Option<Caret>,
    /// Text typed since the pages last caught up, drawn at the caret until they do.
    overlay: Option<Overlay>,
    /// The undo step typing is in.
    typing: Option<Typing>,
    /// Caret maps by (block start, page), until the blocks or the source change.
    maps: RefCell<HashMap<(usize, u32), Rc<document::caret::CharMap>>>,
    /// The caret's blink: shown now, and the timer that flips it.
    caret_on: bool,
    blink: Option<Task<()>>,
    /// Where the caret was last drawn (window coordinates), for an input method's window.
    caret_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// A drag that began on text is under way: moving the pointer selects.
    dragging: bool,
    /// The Find and Replace bar, while open.
    find: Option<find_bar::Find>,
    /// What the last edit made: the bytes it wrote.
    last_edit: Range<usize>,
    _tasks: Vec<Task<()>>,
    _subs: [Subscription; 3],
}

impl Doc {
    pub fn new(
        root: PathBuf,
        path: String,
        resources: Arc<Mutex<render::Resources>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let tex = path.ends_with(".tex");
        let source = std::fs::read_to_string(root.join(&path));
        // an editor (not a textarea) for its completion menu; it never scrolls: the box grows to
        // its text and the page below moves
        let input = cx.new(|cx| box_state(tex, window, cx));
        // syntect names LaTeX "tex" and BibTeX "bib"
        let language = Path::new(&path)
            .extension()
            .map_or("text".into(), |e| e.to_string_lossy().to_string());
        let code = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(language)
                .line_number(true)
                .soft_wrap(true)
        });
        let latex = Rc::new(complete::Latex {
            root: root.clone(),
            doc: cx.entity().downgrade(),
        });
        if tex {
            for state in [&input, &code] {
                state.update(cx, |s, _| {
                    let lsp = s.lsp_mut();
                    lsp.completion_provider = Some(latex.clone());
                    lsp.set_completion_highlighter(Rc::new(highlight_characters));
                });
            }
        }
        let code_sub = cx.subscribe_in(&code, window, |this, code, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::Change) && !place_caret(code, window, cx) {
                this.on_code(cx);
            }
        });
        let sub = cx.subscribe_in(
            &input,
            window,
            |this, input, ev: &InputEvent, window, cx| match ev {
                InputEvent::Change if !place_caret(input, window, cx) => this.on_input(window, cx),
                // a click anywhere else (project panel, tabs) closes the source box; switching apps does not
                InputEvent::Blur if this.tex && window.is_window_active() => this.leave(window, cx),
                _ => {}
            },
        );
        // key bindings run before key listeners, and the textarea binds the arrows: intercept first
        let weak = cx.entity().downgrade();
        let keys = cx.intercept_keystrokes(move |ev, window, cx| {
            let _ = weak.update(cx, |this, cx| this.on_box_key(&ev.keystroke, window, cx));
        });
        let mut this = Self {
            view: None,
            view_images: HashMap::new(),
            root,
            path,
            tex,
            source: String::new(),
            dirty: false,
            status: String::new(),
            compiling: false,
            rtex: None,
            spans: vec![],
            spans_pending: 0,
            preview: Default::default(),
            blocks: vec![],
            outline: vec![],
            resources,
            layout_version: 0,
            images: HashMap::new(),
            versions: HashMap::new(),
            inflight: HashSet::new(),
            live: None,
            zoom: (AppSettings::get(cx).default_zoom / 100.0) as f32,
            zoomed_at: Instant::now(),
            scroll: ScrollHandle::new(),
            page_bounds: Default::default(),
            box_bounds: Default::default(),
            box_cut: Cell::new(None),
            first_click: None,
            hover_gap: None,
            doc_fonts: Default::default(),
            box_fit: Default::default(),
            menu_target: Default::default(),
            selected: None,
            undo: vec![],
            redo: vec![],
            focus: cx.focus_handle(),
            input,
            code,
            source_mode: false,
            code_opts: (true, true),
            autosave: None,
            active: None,
            caret: None,
            overlay: None,
            typing: None,
            maps: Default::default(),
            caret_on: true,
            blink: None,
            caret_bounds: Default::default(),
            dragging: false,
            find: None,
            last_edit: 0..0,
            _tasks: vec![],
            _subs: [sub, keys, code_sub],
        };
        if viewer::can_view(&this.path) {
            match viewer::open(&this.root.join(&this.path)) {
                Ok(v) => this.view = Some(v),
                Err(e) => this.status = format!("Cannot show {}: {e}", this.path),
            }
            return this;
        }
        match source {
            Ok(s) => this.source = s,
            Err(e) => {
                this.status = format!("Cannot read {}: {e}", this.path);
                return this;
            }
        }
        this.outline = document::outline(&this.source);
        let text = this.source.clone();
        this.code.update(cx, |s, cx| s.set_value(text, window, cx));
        if !tex {
            return this;
        }
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        match engine::Rtex::start(&this.root, &this.path, tx) {
            Ok(mut r) => {
                r.request_spans();
                this.spans_pending = 1;
                this.rtex = Some(r);
                this.status = "Typesetting…".into();
            }
            Err(e) => this.status = e,
        }
        this._tasks.push(cx.spawn_in(window, async move |this, cx| {
            while let Some(msg) = rx.next().await {
                if this
                    .update_in(cx, |this, window, cx| this.on_msg(msg, window, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
        this
    }

    /// Free the page images in the window's texture atlas (when the tab closes).
    pub fn release_images(&mut self, window: &mut Window) {
        for (_, im) in self.images.drain() {
            let _ = window.drop_image(im.img);
        }
        for (_, (_, img)) in self.view_images.drain() {
            let _ = window.drop_image(img);
        }
        if let Some((img, _)) = self.live.take() {
            let _ = window.drop_image(img);
        }
    }

    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    pub fn save(&mut self, cx: &mut Context<Self>) {
        if !self.dirty {
            return;
        }
        match std::fs::write(self.root.join(&self.path), &self.source) {
            Ok(()) => self.dirty = false,
            Err(e) => self.status = format!("Could not save {}: {e}", self.path),
        }
        cx.notify();
    }

    /// A full LaTeX run now (references, a changed .bib or included file).
    pub fn recompile(&mut self, cx: &mut Context<Self>) {
        if let Some(r) = self.rtex.as_mut() {
            r.request_layout();
            self.compiling = true;
            cx.notify();
        }
    }

    /// Ask where, then have rtex typeset the whole document into a PDF there.
    pub fn export(&mut self, cx: &mut Context<Self>) {
        if self.rtex.is_none() {
            return;
        }
        let stem = Path::new(&self.path)
            .file_stem()
            .map_or("document".into(), |s| s.to_string_lossy().to_string());
        let dir = self
            .root
            .join(&self.path)
            .parent()
            .map_or(self.root.clone(), Path::to_path_buf);
        let rx = cx.prompt_for_new_path(&dir, Some(&format!("{stem}.pdf")));
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(out))) = rx.await {
                let _ = this.update(cx, |this, cx| {
                    if let Some(r) = this.rtex.as_mut() {
                        r.export_pdf(&out);
                        this.status = format!("Exporting {}…", out.display());
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }
}

impl Render for Doc {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.view.is_some() {
            return self.render_view(window, cx);
        }
        let theme = cx.theme().clone();
        let st = AppSettings::get(cx).clone();
        let fs = st.source_font_size as f32;
        if !self.tex || self.source_mode {
            // follow the line-number and wrap settings (set only on change: setting them redraws)
            if self.code_opts != (st.line_numbers, st.soft_wrap) {
                self.code_opts = (st.line_numbers, st.soft_wrap);
                self.code.update(cx, |s, cx| {
                    s.set_line_number(st.line_numbers, window, cx);
                    s.set_soft_wrap(st.soft_wrap, window, cx);
                });
            }
            return div()
                .size_full()
                .bg(theme.background)
                .font_family(MONO)
                .child(
                    Editor::new(&self.code)
                        .h_full()
                        .appearance(false)
                        .bordered(false)
                        .font_family(MONO)
                        .text_size(px(fs)),
                )
                .into_any_element();
        }
        self.ensure_images(window, cx);
        self.settle_overlay();
        // the source box's page, or the last one if the document got shorter
        let at = self.box_place();
        let box_page = at.as_ref().map(|at| {
            let last = self
                .preview
                .pages
                .keys()
                .next_back()
                .copied()
                .unwrap_or(at.page);
            if self.preview.pages.contains_key(&at.page) {
                at.page
            } else {
                last
            }
        });
        let mut pages: Vec<AnyElement> = self
            .preview
            .pages
            .iter()
            .map(|(&n, dl)| {
                let here = at.as_ref().filter(|_| box_page == Some(n));
                self.render_page(n, dl, here, window, cx)
            })
            .collect();
        let width = |dl: Option<&Arc<engine::DisplayList>>| {
            page_px(
                dl.and_then(|d| d.page_width).unwrap_or(40258437.0),
                self.zoom,
            )
        };
        // the open source box, over its page: the column's top padding, the preamble strip and a
        // gap come before the first page
        if !pages.is_empty() {
            let (w0, w1) = (
                width(self.preview.pages.values().next()),
                width(self.preview.pages.values().last()),
            );
            pages.insert(0, self.render_frame("Preamble", w0, cx));
            pages.push(self.render_frame("Trailer", w1, cx));
        }
        // the selected blocks' keys; with no selection (or a box open) the key goes on
        type SelAct = fn(&mut Doc, usize, usize, &mut Window, &mut Context<Doc>);
        let on_sel = |f: SelAct| {
            move |this: &mut Doc, window: &mut Window, cx: &mut Context<Doc>| match this
                .selection()
                .filter(|_| this.active.is_none())
            {
                Some((lo, hi)) => f(this, lo, hi, window, cx),
                None => cx.propagate(),
            }
        };
        let typing = self.caret.is_some() && self.active.is_none();
        let view = caret_actions(div().id("pages"), cx)
            .on_action(cx.listener(|d, _: &OpenFind, w, cx| d.open_find(false, w, cx)))
            .on_action(cx.listener(|d, _: &PastePlain, _, cx| {
                if d.caret.is_some() && d.active.is_none() {
                    d.paste_at_caret(true, cx)
                }
            }))
            .on_action(cx.listener(|d, _: &OpenReplace, w, cx| d.open_find(true, w, cx)))
            .on_action(cx.listener(|d, _: &FindNext, w, cx| d.find_step(1, w, cx)))
            .on_action(cx.listener(|d, _: &FindPrev, w, cx| d.find_step(-1, w, cx)));
        let view = view
            .key_context(if typing { "Caret" } else { "Pages" })
            .track_focus(&self.focus)
            .on_action(cx.listener({
                let f = on_sel(|d, lo, hi, w, cx| d.activate_range(lo, hi, w, cx));
                move |this, _: &EditBlock, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, lo, hi, w, cx| d.delete_blocks(lo, hi, w, cx));
                move |this, _: &DeleteBlock, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, lo, hi, _, cx| d.duplicate_blocks(lo, hi, cx));
                move |this, _: &DuplicateBlock, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, lo, hi, _, cx| d.move_blocks(lo, hi, true, cx));
                move |this, _: &MoveBlockUp, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, lo, hi, _, cx| d.move_blocks(lo, hi, false, cx));
                move |this, _: &MoveBlockDown, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, lo, _, _, cx| {
                    if lo > 0 {
                        d.set_selection(lo - 1, lo - 1, cx)
                    }
                });
                move |this, _: &SelectPrevBlock, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, _, hi, _, cx| d.set_selection(hi + 1, hi + 1, cx));
                move |this, _: &SelectNextBlock, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, _, _, _, cx| d.extend_selection(-1, cx));
                move |this, _: &ExtendSelectionUp, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, _, _, _, cx| d.extend_selection(1, cx));
                move |this, _: &ExtendSelectionDown, w, cx| f(this, w, cx)
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, _, _, _, cx| d.clear_selection(cx));
                move |this, _: &ClearSelection, w, cx| f(this, w, cx)
            }))
            // ⌘A on the pages: every block (in a box, the box's text)
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                if this.caret.is_some() && this.active.is_none() {
                    return this.select_all_text(cx);
                }
                if this.active.is_some() || this.blocks.is_empty() {
                    cx.propagate();
                    return;
                }
                let last = this.blocks.len() - 1;
                this.selected = Some((this.blocks[0].range.start, this.blocks[last].range.start));
                cx.notify();
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, lo, hi, _, cx| d.copy_blocks(lo, hi, cx));
                move |this, _: &Copy, w, cx| {
                    if !(this.active.is_none() && this.copy_selection(cx)) {
                        f(this, w, cx)
                    }
                }
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, lo, hi, w, cx| {
                    d.copy_blocks(lo, hi, cx);
                    d.delete_blocks(lo, hi, w, cx);
                });
                move |this, _: &Cut, w, cx| {
                    if this.active.is_none() && this.copy_selection(cx) {
                        this.delete_selection(cx);
                    } else {
                        f(this, w, cx)
                    }
                }
            }))
            .on_action(cx.listener({
                let f = on_sel(|d, _, hi, w, cx| {
                    if let Some(t) = cx.read_from_clipboard().and_then(|c| c.text()) {
                        d.insert_at(Spot::Below(hi), &t, w, cx);
                    }
                });
                move |this, _: &Paste, w, cx| {
                    if this.caret.is_some() && this.active.is_none() {
                        this.paste_at_caret(false, cx)
                    } else {
                        f(this, w, cx)
                    }
                }
            }))
            .size_full()
            // typing and input methods reach the caret through the pages' input handler
            .when(typing, |v| {
                let (focus, doc) = (self.focus.clone(), cx.entity());
                v.child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, cx| {
                            window.handle_input(
                                &focus,
                                ElementInputHandler::new(bounds, doc.clone()),
                                cx,
                            )
                        },
                    )
                    .absolute()
                    .size_full(),
                )
            })
            // dark theme: a near-black desk, between the gray sidebar and the black page
            .bg(if theme.mode.is_dark() {
                rgb(0x141414).into()
            } else {
                theme.muted
            })
            .overflow_scroll()
            .track_scroll(&self.scroll)
            .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
            // a click beside the pages closes the source box
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.leave(window, cx)),
            )
            .on_pinch(cx.listener(|this, ev: &PinchEvent, window, cx| {
                this.set_zoom(this.zoom * (1.0 + ev.delta), Some(ev.position), window, cx)
            }))
            .child(
                div()
                    .id("page-column")
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(PAGE_GAP))
                    .pt(px(PAGES_TOP - FRAME_H - PAGE_GAP))
                    .pb(px(48.0))
                    .px_8()
                    .relative()
                    .children(pages)
                    .context_menu(self.page_menu(cx)),
            );
        // the scrollbars sit over the scroll area as a sibling, so they do not scroll with the
        // pages; always shown, like an editor's
        div()
            .relative()
            .size_full()
            .child(view)
            .child(
                div().absolute().inset_0().child(
                    Scrollbar::new(&self.scroll)
                        .axis(ScrollbarAxis::Both)
                        .viewport_from_layout()
                        .mode(ScrollbarMode::Always),
                ),
            )
            .children(self.render_find_bar(cx))
            .into_any_element()
    }
}

fn page_px(sp: f64, zoom: f32) -> f32 {
    (sp / SP_PER_BP) as f32 * zoom
}

fn union(boxes: impl Iterator<Item = [f64; 4]>) -> Option<[f64; 4]> {
    boxes.reduce(|a, b| {
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]
    })
}

fn to_image(w: u32, h: u32, bgra: Vec<u8>) -> Arc<RenderImage> {
    let buf = image::RgbaImage::from_raw(w, h, bgra).expect("buffer size matches");
    Arc::new(RenderImage::new(vec![image::Frame::new(buf)]))
}
