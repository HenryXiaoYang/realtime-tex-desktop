// The source box: the block's TeX in the font it typesets in, flat on the page in place of its
// lines, growing with its text.
use super::*;

impl Doc {
    /// The source box of the block being edited on page `n`, placed over the page (relative to
    /// its top left) without moving what follows: it hovers over the text below.
    pub(super) fn render_box(
        &self,
        at: &BoxPlace,
        dl: &engine::DisplayList,
        cx: &mut Context<Self>,
    ) -> (AnyElement, f32, f32) {
        // the source at the size of the text it typesets to (10pt when there is none yet), in
        // its font unless the settings name one
        let fs = page_px(
            at.font
                .as_ref()
                .and_then(|f| f.size)
                .unwrap_or(10.0 * 65536.0),
            self.zoom,
        );
        let family = self.box_family(at.font.as_ref(), cx);
        let (_, box_fg) = palette::source_box(self.dark_pages(cx), cx);
        let pw = dl.page_width.unwrap_or(40258437.0);
        let w = page_px(pw, self.zoom);
        let pad = 4.0 * self.zoom;
        // the editor's own inset at each side, taken back so the source starts where the text did
        let inset = 4.0;
        let mut rect = at.rect;
        if rect[0].is_nan() {
            // no lines yet: as wide as the text column
            (rect[0], rect[2]) = (pw * 0.2, pw * 0.8);
        }
        let top = if at.lines {
            // a display sits only a few points under the line above it: never cut into that line
            let above = dl
                .lines
                .iter()
                .map(|l| l.y + l.d)
                .filter(|&b| b <= rect[1])
                .fold(0.0, f64::max);
            page_px(rect[1], self.zoom).max(page_px(above, self.zoom) + 0.5 * self.zoom)
        } else {
            page_px(rect[1], self.zoom)
        };
        let bottom = page_px(rect[3], self.zoom);
        // as tall as the lines it replaces: their height shared out among them, so as many
        // source rows fit exactly; more rows (source is longer than its output) grow it. A tall
        // line (a table, a fraction) keeps ordinary row spacing: the box is just at least as
        // tall as it was.
        let lh = if at.lines {
            ((bottom - top) / at.count.max(1) as f32).clamp(fs, fs * 1.25)
        } else {
            fs * 1.2
        };
        let min_h = if at.lines { bottom - top } else { 0.0 };
        let left = page_px(
            dl.lines
                .iter()
                .map(|l| l.x)
                .fold(f64::MAX, f64::min)
                .min(rect[0]),
            self.zoom,
        );
        let right = page_px(
            rect[2].max(dl.lines.iter().map(|l| l.x + l.w).fold(0.0, f64::max)),
            self.zoom,
        );
        // at least a usable width, centred over a page narrower than that (a standalone picture)
        let box_w = (right - left + 2.0 * inset).max(320.0);
        let box_x = if left - inset + box_w > w {
            ((w - box_w) / 2.0).min(left - inset)
        } else {
            left - inset
        };
        // Typora-style: a formula shows its typeset result under the source, text does not
        let live = self
            .live
            .as_ref()
            .filter(|_| {
                AppSettings::get(cx).live_preview
                    && !self.compiling
                    && at.lines
                    && !at.multi
                    && self
                        .active_block()
                        .is_some_and(|b| b.rows.iter().all(|r| r.display))
            })
            .map(|(i, r, _)| {
                (
                    i.clone(),
                    page_px(r[2], self.zoom),
                    page_px(r[3], self.zoom),
                )
            });
        let border = cx.theme().border;
        let box_bounds = self.box_bounds.clone();
        // on the paper itself, set off by a thin outline laid over its edge (so it takes no room):
        // the lines below move only when the source needs more rows than the text had
        let editor = div()
            .id("source-box")
            .relative()
            .ml(px(box_x))
            .w(px(box_w))
            // on the paper unless the theme names the box a background of its own
            .when_some(
                crate::ui::themes::page_color("pages.source_box.background", cx),
                |d, c| d.bg(c),
            )
            .text_color(box_fg)
            .flex()
            .flex_col()
            .child(
                canvas(move |b, _, _| box_bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .border_1()
                    .border_color(palette::marks(self.dark_pages(cx), cx).accent.opacity(0.5)),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(self.box_editor(box_w, fs, lh, min_h, family, cx))
            .children(live.map(|(i, w, h)| {
                div()
                    .flex()
                    .justify_center()
                    .border_t_1()
                    .border_color(border)
                    .pt(px(pad))
                    .child(img(i).w(px(w)).h(px(h)))
            }));
        (editor.into_any_element(), top, bottom)
    }

    /// The family to show source in: the settings' choice, else the document's own font for
    /// that text (registered with the window's text system the first time), else Menlo. A
    /// Type1 font (TFM-based, as pdfTeX-era fonts are) cannot be registered and falls back too.
    pub(super) fn box_family(&self, desc: Option<&engine::FontDesc>, cx: &App) -> SharedString {
        let chosen = AppSettings::get(cx).source_box_font.trim().to_string();
        if !chosen.is_empty() {
            return chosen.into();
        }
        self.doc_family(desc, cx)
    }

    /// The family of the document's own font `desc`; Menlo when it cannot be loaded.
    pub(super) fn doc_family(&self, desc: Option<&engine::FontDesc>, cx: &App) -> SharedString {
        self.doc_font(desc, cx)
            .map_or_else(|| MONO.into(), |f| f.family)
    }

    /// The document's own font `desc` as a GPUI font (family, weight, slant), registered with
    /// the window's text system the first time. A TFM-based Latin Modern or Computer Modern font
    /// (`ec-lmr12`, `cmbx10`), which GPUI cannot load, is its OpenType Latin Modern twin.
    pub(super) fn doc_font(&self, desc: Option<&engine::FontDesc>, cx: &App) -> Option<Font> {
        let desc = desc?;
        let key = desc.filename.clone().or_else(|| desc.name.clone())?;
        self.doc_fonts
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                let file = desc.filename.as_ref().map(|f| {
                    (
                        std::path::PathBuf::from(f),
                        desc.subfont.unwrap_or(1).saturating_sub(1),
                    )
                });
                let twin = || {
                    let name = latin_modern(desc.name.as_deref()?)?;
                    Some((engine::kpsewhich(&name)?, 0))
                };
                file.and_then(|(p, i)| load_font(&p, i, cx))
                    .or_else(|| twin().and_then(|(p, i)| load_font(&p, i, cx)))
            })
            .clone()
    }

    /// The source box's editor, as tall as its text: an editor does not grow by itself.
    /// The box's editor, as tall as its text (never shorter than `min_h`), in `family` at `fs`
    /// with rows `lh` apart. It never scrolls: after each layout it checks that it got the rows
    /// and row height it was sized for, and draws again when it did not.
    pub(super) fn box_editor(
        &self,
        width: f32,
        fs: f32,
        lh: f32,
        min_h: f32,
        family: impl Into<SharedString>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let family = family.into();
        let ts = cx.text_system();
        let id = ts.resolve_font(&font(family.clone()));
        // each character's real width, so a proportional document font wraps as the editor does
        // (its insets are 4 each side, and it keeps a margin on the right)
        let estimate = || {
            let mut widths: HashMap<char, f32> = HashMap::new();
            let mut advance = |c: char| {
                *widths.entry(c).or_insert_with(|| {
                    ts.advance(id, px(fs), c)
                        .map_or(fs * 0.6, |a| f32::from(a.width))
                })
            };
            wrapped_rows(
                &self.input.read(cx).value(),
                width - 8.0 - 20.0,
                &mut advance,
            )
        };
        let fit = fit_box(self.input.read(cx), lh, self.box_fit.get(), estimate);
        self.box_fit.set(fit);
        let (input, used, weak) = (
            self.input.clone(),
            self.box_fit.clone(),
            cx.entity().downgrade(),
        );
        let check = move |cx: &mut App| {
            let state = input.read(cx);
            let scrolled = state.scroll_offset().y != px(0.0);
            if refit(state, used.get()) {
                let _ = weak.update(cx, |_, cx| cx.notify());
            }
            // a frame where the text outgrew the box scrolled it to the caret: undo that once
            // the box has grown
            if scrolled {
                input.update(cx, |s, cx| s.set_scroll_offset(point(px(0.0), px(0.0)), cx));
            }
        };
        div()
            .relative()
            .child(sized_editor(&self.input, family, fs, fit, min_h))
            .child(
                canvas(|_, _, _| {}, move |_, _, _, cx| check(cx))
                    .absolute()
                    .size_full(),
            )
            .into_any_element()
    }

    /// The preamble or trailer strip: one muted line of its source, or its source box while edited.
    pub(super) fn render_frame(
        &self,
        kind: &'static str,
        w: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (box_bg, box_fg) = palette::source_box(self.dark_pages(cx), cx);
        let theme = cx.theme().clone();
        let range = self.spans.iter().find(|s| s.kind == kind).map(|s| {
            document::trim(
                &self.source,
                s.range.start..s.range.end.min(self.source.len()),
            )
        });
        let editing = self
            .active
            .as_ref()
            .is_some_and(|a| a.frame && range.as_ref().is_some_and(|r| r.start == a.range.start));
        let family = AppSettings::get(cx).source_family();
        let strip = div()
            .id(kind)
            .w(px(w))
            .flex_none()
            .font_family(family.clone());
        if editing {
            return strip
                .p(px(4.0 * self.zoom))
                .bg(box_bg)
                .text_color(box_fg)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child({
                    let fs = AppSettings::get(cx).source_font_size as f32 * self.zoom;
                    self.box_editor(w - 8.0 * self.zoom, fs, fs * 1.4, 0.0, family, cx)
                })
                .into_any_element();
        }
        let summary = range.map_or(String::new(), |r| {
            let lines: Vec<&str> = self.source[r]
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('%'))
                .collect();
            match lines.as_slice() {
                [] => String::new(),
                [one] => one.to_string(),
                [first, .., last] => format!("{first}  …  {last}"),
            }
        });
        strip
            .h(px(FRAME_H))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .text_xs()
            .text_color(theme.muted_foreground)
            .border_1()
            .border_dashed()
            .border_color(theme.border)
            .cursor_pointer()
            .hover(|s| s.bg(theme.secondary))
            .child(if kind == "Preamble" {
                "Preamble"
            } else {
                "End"
            })
            .child(div().truncate().child(summary))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.activate_frame(kind, 0, window, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
}

/// The source box's grey, a step lighter than the paper in either theme.
/// What the box was last sized with: the row height asked for, and the rows and row height used.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct BoxFit {
    asked: f32,
    rows: usize,
    lh: f32,
}

impl BoxFit {
    fn height(&self, min_h: f32) -> f32 {
        (self.rows.max(1) as f32 * self.lh).max(min_h)
    }
}

/// The rows and row height to size a box with rows `asked` apart: the editor's own from its
/// last layout when that was at the same row height (`last`), else the whole pixels GPUI
/// rounds a line height to and `estimate` rows.
pub(super) fn fit_box(
    state: &EditorState,
    asked: f32,
    last: BoxFit,
    estimate: impl FnOnce() -> usize,
) -> BoxFit {
    match state.line_height().filter(|_| last.asked == asked) {
        Some(lh) => BoxFit {
            asked,
            rows: state.display_map().wrap_row_count(),
            lh: f32::from(lh),
        },
        None => BoxFit {
            asked,
            rows: estimate(),
            lh: asked.round().max(1.0),
        },
    }
}

/// After a layout: whether the editor wrapped into other rows, or drew them at another height,
/// than the box was sized for (`used`), so it needs drawing again.
pub(super) fn refit(state: &EditorState, used: BoxFit) -> bool {
    state.line_height().is_some_and(|lh| {
        state.display_map().wrap_row_count() != used.rows || f32::from(lh) != used.lh
    })
}

/// The source box's editor state: soft-wrapped, no gutter, no scrollbars, nothing past the
/// last line, so the box can be exactly as tall as its text.
pub(super) fn box_state(
    tex: bool,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) -> EditorState {
    let mut s = EditorState::new(window, cx)
        .language(if tex { "tex" } else { "text" })
        .line_number(false)
        .folding(false)
        .soft_wrap(true)
        .scroll_beyond_last_line(Some(0));
    let none: gpui_kit::base::input::EditorScrollbarRenderer =
        Rc::new(|_, _, _| Empty.into_any_element());
    s.set_scrollbar_renderer(Some(none));
    s
}

/// The editor of a box sized by `fit`, at least `min_h` tall.
fn sized_editor(
    state: &Entity<EditorState>,
    family: SharedString,
    fs: f32,
    fit: BoxFit,
    min_h: f32,
) -> Editor {
    Editor::new(state)
        .appearance(false)
        .bordered(false)
        .font_family(family)
        .text_size(px(fs))
        .line_height(px(fit.asked))
        .with_size(Size::XSmall)
        .h(px(fit.height(min_h)))
}

/// The font in file `path` (face `index`) as GPUI selects it: its family, registered unless it
/// is installed already, and its weight and slant.
fn load_font(path: &Path, index: u32, cx: &App) -> Option<Font> {
    use skrifa::MetadataProvider;
    let bytes = std::fs::read(path).ok()?;
    let names = family_names(&bytes, index);
    let attrs = skrifa::FontRef::from_index(&bytes, index)
        .ok()?
        .attributes();
    let ts = cx.text_system();
    // an installed font (Times New Roman, say) is known already; a TeX tree's is not
    let known = |ts: &TextSystem| {
        let all: HashSet<String> = ts.all_font_names().into_iter().collect();
        names.iter().find(|n| all.contains(*n)).cloned()
    };
    let family = known(ts).or_else(|| {
        ts.add_fonts(vec![bytes.clone().into()]).ok()?;
        known(ts)
    })?;
    let mut f = font(family);
    f.weight = FontWeight(attrs.weight.value());
    if attrs.style != skrifa::attribute::Style::Normal {
        f.style = FontStyle::Italic;
    }
    Some(f)
}

/// The OpenType Latin Modern file standing in for a TFM-based font of the Latin Modern or
/// Computer Modern families (`ec-lmr12`, `cmbx10`): the same design, in a format GPUI loads.
/// The design size nearest the one asked for.
pub(super) fn latin_modern(name: &str) -> Option<String> {
    let base = name.rsplit('-').next()?;
    let rest = base
        .strip_prefix("lm")
        .or_else(|| base.strip_prefix("cm"))?;
    let digits = rest.trim_start_matches(|c: char| c.is_ascii_alphabetic());
    let shape = &rest[..rest.len() - digits.len()];
    let size: i64 = digits.parse().unwrap_or(10);
    let (family, style, sizes): (&str, &str, &[i64]) = match shape {
        "r" => ("roman", "regular", &[5, 6, 7, 8, 9, 10, 12, 17]),
        "bx" => ("roman", "bold", &[5, 6, 7, 8, 9, 10, 12]),
        "ri" | "ti" | "it" => ("roman", "italic", &[7, 8, 9, 10, 12]),
        "bxi" | "bxti" => ("roman", "bolditalic", &[10]),
        "sl" => ("romanslant", "regular", &[8, 9, 10, 12, 17]),
        "csc" => ("romancaps", "regular", &[10]),
        "tt" => ("mono", "regular", &[8, 9, 10, 12]),
        "ss" => ("sans", "regular", &[8, 9, 10, 12, 17]),
        "ssbx" => ("sans", "bold", &[10]),
        _ => return None,
    };
    let size = sizes.iter().min_by_key(|s| (**s - size).abs())?;
    Some(format!("lm{family}{size}-{style}.otf"))
}

/// The family names a font file gives itself: typographic family first, then the legacy one
/// ("Latin Modern Roman", "LM Roman 10"). A Type1 font has none.
pub(super) fn family_names(bytes: &[u8], index: u32) -> Vec<String> {
    use skrifa::{MetadataProvider, string::StringId};
    let Ok(font) = skrifa::FontRef::from_index(bytes, index) else {
        return vec![];
    };
    [StringId::TYPOGRAPHIC_FAMILY_NAME, StringId::FAMILY_NAME]
        .into_iter()
        .filter_map(|id| font.localized_strings(id).english_or_first())
        .map(|s| s.to_string())
        .collect()
}

/// Rows `text` takes when word-wrapped at `width`, with `advance` giving each character's width.
pub(super) fn wrapped_rows(text: &str, width: f32, advance: &mut impl FnMut(char) -> f32) -> usize {
    let width = width.max(1.0);
    text.split('\n')
        .map(|line| {
            let (mut rows, mut x) = (1, 0.0);
            for word in line.split_inclusive(' ') {
                let w: f32 = word.trim_end().chars().map(&mut *advance).sum();
                if x > 0.0 && x + w > width {
                    rows += 1;
                    x = 0.0;
                }
                x += word.chars().map(&mut *advance).sum::<f32>();
                // a word longer than a row breaks across rows
                while x > width {
                    rows += 1;
                    x -= width;
                }
            }
            rows
        })
        .sum()
}

/// A completion just inserted text with the caret mark: drop the mark, leaving the caret there.
/// True when it did; the edit that removes it reports the change again.
pub(super) fn place_caret(state: &Entity<EditorState>, window: &mut Window, cx: &mut App) -> bool {
    let value = state.read(cx).value().to_string();
    let Some(i) = value.find(complete::MARK) else {
        return false;
    };
    let at = value[..i].encode_utf16().count();
    let r = at..at + complete::MARK.len_utf16();
    state.update(cx, |s, cx| s.replace_text_in_range(Some(r), "", window, cx));
    true
}

#[cfg(test)]
mod tests {
    use super::{BoxFit, box_state, fit_box, refit, sized_editor, wrapped_rows};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, Context, Entity, IntoElement, ParentElement, Render, Styled,
        TestAppContext, Window, WindowBounds, WindowOptions, div, point, px, size,
    };
    use std::cell::Cell;
    use std::rc::Rc;

    /// A source box alone in a window, sized the way the pages size it.
    struct BoxView {
        state: Entity<gpui_kit::component::input::EditorState>,
        fit: Rc<Cell<BoxFit>>,
        width: f32,
        fs: f32,
        lh: f32,
    }

    impl Render for BoxView {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            // a poor first estimate on purpose: the box must still settle on the editor's rows
            let fit = fit_box(self.state.read(cx), self.lh, self.fit.get(), || 1);
            self.fit.set(fit);
            div().w(px(self.width)).child(sized_editor(
                &self.state,
                "Menlo".into(),
                self.fs,
                fit,
                0.0,
            ))
        }
    }

    #[gpui_kit::test]
    fn the_box_grows_to_its_text_and_never_scrolls(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let text = "\\textbf{Success criteria:}\n\\begin{enumerate}[nosep]\n".to_string()
            + &"    \\item No teacher has back-to-back lessons in far-apart buildings\n".repeat(14)
            + "\\end{enumerate}";
        // fractional row heights (GPUI rounds them), narrow and wide boxes, small and large text
        for (width, fs, lh) in [
            (420.0, 13.7, 16.9),
            (600.0, 19.6, 24.5),
            (900.0, 14.0, 17.5),
            (300.0, 9.3, 11.6),
        ] {
            let (window, view) = cx
                .update(|cx| {
                    gpui_kit::open_window(
                        WindowOptions {
                            window_bounds: Some(WindowBounds::Windowed(Bounds {
                                origin: point(px(0.0), px(0.0)),
                                size: size(px(1000.0), px(4000.0)),
                            })),
                            ..Default::default()
                        },
                        cx,
                        |window, cx| {
                            let state = cx.new(|cx| box_state(true, window, cx));
                            state.update(cx, |s, cx| s.set_value(text.clone(), window, cx));
                            cx.new(|_| BoxView {
                                state,
                                fit: Default::default(),
                                width,
                                fs,
                                lh,
                            })
                        },
                    )
                })
                .unwrap();
            let (state, fit) = cx.read(|cx| {
                let v = view.read(cx);
                (v.state.clone(), v.fit.clone())
            });
            let mut frames = 0;
            loop {
                cx.update_window(window, |_, window, cx| window.render_frame(cx))
                    .unwrap();
                frames += 1;
                if !cx.read(|cx| refit(state.read(cx), fit.get())) {
                    break;
                }
                assert!(frames < 4, "the box never settled ({width}, {fs}, {lh})");
            }
            cx.read(|cx| {
                let s = state.read(cx);
                let rows = s.display_map().wrap_row_count();
                let row_h = f32::from(s.line_height().unwrap());
                assert!(rows > 16, "long lines wrap at {width}");
                assert_eq!(row_h, lh.round(), "GPUI draws rows at whole pixels");
                assert_eq!(
                    fit.get().height(0.0),
                    rows as f32 * row_h,
                    "({width}, {fs}, {lh})"
                );
                // the editor got the whole height: every row shows, nothing to scroll
                let shown = f32::from(s.input_bounds().size.height);
                assert!(
                    shown >= rows as f32 * row_h - 0.01,
                    "{shown} < {rows} × {row_h}"
                );
                assert_eq!(s.scroll_offset().y, px(0.0));
            });
        }
    }

    #[test]
    fn tfm_latin_modern_has_an_opentype_twin() {
        use super::latin_modern;
        assert_eq!(
            latin_modern("ec-lmr12").as_deref(),
            Some("lmroman12-regular.otf")
        );
        assert_eq!(
            latin_modern("ec-lmbx12").as_deref(),
            Some("lmroman12-bold.otf")
        );
        assert_eq!(
            latin_modern("ec-lmri12").as_deref(),
            Some("lmroman12-italic.otf")
        );
        assert_eq!(
            latin_modern("rm-lmr17").as_deref(),
            Some("lmroman17-regular.otf")
        );
        assert_eq!(
            latin_modern("ec-lmbx8").as_deref(),
            Some("lmroman8-bold.otf")
        );
        assert_eq!(
            latin_modern("cmr10").as_deref(),
            Some("lmroman10-regular.otf")
        );
        assert_eq!(
            latin_modern("cmtt11").as_deref(),
            Some("lmmono10-regular.otf")
        );
        // math fonts and other families have none
        assert_eq!(latin_modern("lmsy10"), None);
        assert_eq!(latin_modern("ptmr8r"), None);
    }

    #[test]
    fn counts_wrapped_rows() {
        let mut mono = |_| 1.0;
        assert_eq!(wrapped_rows("short", 10.0, &mut mono), 1);
        assert_eq!(wrapped_rows("a\n\nb", 10.0, &mut mono), 3);
        // "aaaa bbbb " fills a row, "cccc" goes to the next
        assert_eq!(wrapped_rows("aaaa bbbb cccc", 10.0, &mut mono), 2);
        assert_eq!(wrapped_rows("aaaaaaaaaaaaaaaaaaaaa", 10.0, &mut mono), 3);
        // proportional: wide letters wrap sooner
        let mut prop = |c: char| if c == 'm' { 3.0 } else { 1.0 };
        assert_eq!(wrapped_rows("mmm iii", 10.0, &mut prop), 2);
    }
}
