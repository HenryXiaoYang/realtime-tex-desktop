// The tour: a scratch project whose pages explain the app, opened in a window of its own, and the
// window dimmed but for the controls a step is about, a card beside them saying what they are for.
// A step that asks for something moves on once it is done since the step began; Escape ends the
// tour, Return goes on. The copy is deleted when its window closes.
use super::*;
use crate::ui::keys_in;
use std::cell::RefCell;
use std::collections::HashMap;

const MAIN: &str = include_str!("tour/main.tex");
const CHAPTER: &str = include_str!("tour/chapter.tex");
/// What the TeX-breaks step has the reader break (its closing brace) and mend.
const FRACTION: &str = "\\frac{1}{2}";

/// The scratch copy's folder, the app's own: one, so that a second tour brings the first one's
/// window back (a test's is its own, clear of the app's).
pub fn dir() -> PathBuf {
    if cfg!(test) {
        return std::env::temp_dir().join(format!("rtd-tour-{}", std::process::id()));
    }
    settings::support_dir().join("Tour")
}

/// A fresh copy of the tour project, in place of any older one.
pub fn create() -> std::io::Result<PathBuf> {
    let d = dir();
    remove();
    std::fs::create_dir_all(&d)?;
    std::fs::write(d.join("main.tex"), MAIN)?;
    std::fs::write(d.join("chapter.tex"), CHAPTER)?;
    Ok(d)
}

/// The copy gone (its window closed, or a crash left it).
pub fn remove() {
    let _ = std::fs::remove_dir_all(dir());
}

/// Where the tour's PDF is exported: the scratch folder goes when its window closes.
pub(super) fn export_dir() -> PathBuf {
    let home = std::env::home_dir().unwrap_or_default();
    Some(home.join("Documents"))
        .filter(|d| d.is_dir())
        .unwrap_or(home)
}

/// What a step needs on screen: the control it points at may be folded or closed.
#[derive(Clone, Copy, PartialEq)]
enum Show {
    Nothing,
    Ribbon(ribbon::RibbonTab),
    Files,
    Sidebar,
}

/// What a step that asks for something waits for (done since the step began).
#[derive(Clone, Copy, PartialEq)]
enum Done {
    TypedInMain,
    BoxOpened,
    MadeBold,
    TypedInChapter,
    BrokeAndFixed,
    Recompiled,
    Exported,
}

struct Step {
    /// The controls shown through the dim (their marks' names); the card goes by the first.
    marks: &'static [&'static str],
    title: &'static str,
    body: &'static str,
    show: Show,
    /// None: it only shows something, and Next goes on.
    done: Option<Done>,
}

const fn see(
    marks: &'static [&'static str],
    show: Show,
    title: &'static str,
    body: &'static str,
) -> Step {
    Step {
        marks,
        title,
        body,
        show,
        done: None,
    }
}

const fn act(
    marks: &'static [&'static str],
    show: Show,
    done: Done,
    title: &'static str,
    body: &'static str,
) -> Step {
    Step {
        marks,
        title,
        body,
        show,
        done: Some(done),
    }
}

const HOME: Show = Show::Ribbon(ribbon::RibbonTab::Home);
const INSERT: Show = Show::Ribbon(ribbon::RibbonTab::Insert);

const STEPS: &[Step] = &[
    see(
        &["pages"],
        Show::Nothing,
        "Your document, typeset",
        "There is no source pane: you write on these pages, and LaTeX sets what you type as it will print.",
    ),
    act(
        &["pages"],
        Show::Nothing,
        Done::TypedInMain,
        "Write on the page",
        "Click in the sentence under “1 Write on the pages” and type a few words.",
    ),
    act(
        &["pages"],
        Show::Nothing,
        Done::BoxOpened,
        "Open the source box",
        "Click the formula in section 2. Its TeX opens in a box on the page, with the result under it as you type.",
    ),
    see(
        &["ribbon"],
        HOME,
        "The Home tab",
        "Bold, italic, headings, lists and alignment, as in a word processor. Each puts in the LaTeX for it.",
    ),
    act(
        &["bold", "pages"],
        HOME,
        Done::MadeBold,
        "Make a word bold",
        "Double-click a word in section 3, then click B (or press ⌘B).",
    ),
    see(
        &["ribbon"],
        INSERT,
        "The Insert tab",
        "Formulas, pictures, tables, footnotes and references: what you add to the document.",
    ),
    act(
        &["files", "pages"],
        Show::Files,
        Done::TypedInChapter,
        "Your files",
        "Open chapter.tex here and type in it. A file the main one brings in with \\input is edited on the same pages.",
    ),
    see(
        &["status-left"],
        Show::Nothing,
        "Pages and panels",
        "Where you are in the document, and the button that shows or hides the project panel.",
    ),
    act(
        &["problems", "pages"],
        Show::Nothing,
        Done::BrokeAndFixed,
        "When TeX breaks",
        "In main.tex, click the formula in section 5, delete the } after the 2 and click outside the box. LaTeX can’t typeset that, so Problems turns red: open it, then Undo (⌘Z).",
    ),
    see(
        &["logs"],
        Show::Nothing,
        "rtex’s log",
        "What rtex, the engine that typesets your pages, says. Look here when the pages look wrong.",
    ),
    see(
        &["sidebar"],
        Show::Sidebar,
        "The sidebar",
        "Choices for what the caret is in, and for the document itself: its title, author, paper and fonts.",
    ),
    act(
        &["recompile"],
        Show::Nothing,
        Done::Recompiled,
        "Run LaTeX in full",
        "Paragraphs update as you type; a full LaTeX run catches up with page breaks, references and the contents. Recompile (⌘⇧R) runs one now.",
    ),
    see(
        &["settings"],
        Show::Nothing,
        "Settings",
        "The theme, fonts, autosave and shortcuts, and where rtex and TeX are.",
    ),
    act(
        &["export"],
        Show::Nothing,
        Done::Exported,
        "Export the PDF",
        "Export PDF typesets the whole document into a PDF file.",
    ),
];

/// How a step left what it opened, to put it back when the step ends.
#[derive(Clone, Copy)]
enum Restore {
    Nothing,
    Ribbon {
        collapsed: bool,
        tab: ribbon::RibbonTab,
    },
    Files {
        shown: bool,
        anyway: bool,
    },
    Sidebar {
        closed: bool,
        anyway: bool,
    },
}

/// What was so when a step began, and what happened since: a step is done by a change since,
/// never by what was so already (it came back by Back, or an earlier step did it).
#[derive(Default)]
pub(super) struct Since {
    /// The files' text then (a file opened later: when it opened).
    main: Option<String>,
    chapter: Option<String>,
    /// Where a source box was open then, until it closes.
    boxed: Option<usize>,
    /// The fraction was broken.
    broke: bool,
    pub(super) recompiled: bool,
    /// A PDF was made.
    pub(super) exported: bool,
}

pub(super) struct Tour {
    /// The step shown; STEPS.len() is the end.
    step: usize,
    restore: Restore,
    pub(super) since: Since,
    /// The controls' bounds (window coordinates) by name, as painted last frame: only those
    /// painted.
    marks: Rc<RefCell<HashMap<&'static str, Bounds<Pixels>>>>,
    /// The holes in the dim this frame.
    holes: Vec<Bounds<Pixels>>,
    /// Escape and Return, when nothing in the window took them.
    _keys: Subscription,
}

impl Workspace {
    /// The tour from its first step (again, if it is on: what its step opened put back first).
    pub fn tour_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tour_end(cx);
        let handle = window.window_handle();
        let keys = cx.observe_keystrokes(move |ws, ev, window, cx| {
            if window.window_handle() == handle {
                ws.tour_key(ev, window, cx);
            }
        });
        self.tour = Some(Tour {
            step: 0,
            restore: Restore::Nothing,
            since: Since::default(),
            marks: Default::default(),
            holes: vec![],
            _keys: keys,
        });
        self.tour_go(0, window, cx);
    }

    /// Escape ends the tour, Return goes on from a step that asks for nothing; not a key
    /// something else took (a menu's Escape, a new paragraph's Return), nor the source box's.
    /// The pages' Escape only selects: it ends the tour too.
    fn tour_key(&mut self, ev: &KeystrokeEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = &self.tour else { return };
        let ks = &ev.keystroke;
        if ks.modifiers.modified() || self.doc().is_some_and(|d| d.read(cx).box_open()) {
            return;
        }
        let pages = ev
            .context_stack
            .last()
            .is_some_and(|c| c.contains("Pages") || c.contains("Caret"));
        match ks.key.as_str() {
            "escape" if ev.action.is_none() || pages => self.tour_end(cx),
            "enter"
                if ev.action.is_none() && STEPS.get(t.step).is_some_and(|s| s.done.is_none()) =>
            {
                self.tour_go(t.step + 1, window, cx)
            }
            _ => {}
        }
    }

    /// A canvas over a control the tour points at (the last child of a relative box around
    /// it), keeping its bounds for the next frame.
    pub(super) fn tour_mark(&self, name: &'static str) -> Option<impl IntoElement + use<>> {
        let marks = self.tour.as_ref()?.marks.clone();
        Some(
            canvas(
                move |b, _, _| {
                    marks.borrow_mut().insert(name, b);
                },
                |_, _, _, _| {},
            )
            // pinned to the box's corner: left to itself, after the control in a block box
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    }

    fn tour_source(&self, path: &str, cx: &App) -> Option<String> {
        self.tabs
            .iter()
            .map(|d| d.read(cx))
            .find(|d| d.path == path)
            .map(|d| d.source.clone())
    }

    fn tour_box(&self, cx: &App) -> Option<usize> {
        self.doc().and_then(|d| d.read(cx).box_at())
    }

    /// The step done since it began: on to the next. Called each frame.
    pub(super) fn tour_check(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = &self.tour else { return };
        let step = t.step;
        let Some(done) = STEPS.get(step).and_then(|s| s.done) else {
            return;
        };
        let main = self.tour_source("main.tex", cx);
        let chapter = self.tour_source("chapter.tex", cx);
        let boxed = self.tour_box(cx);
        let s = &mut self.tour.as_mut().unwrap().since;
        if s.main.is_none() {
            s.main.clone_from(&main);
        }
        if s.chapter.is_none() {
            s.chapter.clone_from(&chapter);
        }
        if boxed.is_none() {
            s.boxed = None;
        }
        if main.as_ref().is_some_and(|m| !m.contains(FRACTION)) {
            s.broke = true;
        }
        let bold = |s: &str| s.matches("\\textbf").count();
        let is_done = match done {
            Done::TypedInMain => main != s.main,
            Done::BoxOpened => boxed.is_some() && boxed != s.boxed,
            Done::MadeBold => main
                .zip(s.main.as_deref())
                .is_some_and(|(now, then)| bold(&now) > bold(then)),
            Done::TypedInChapter => chapter != s.chapter,
            Done::BrokeAndFixed => s.broke && main.is_some_and(|m| m.contains(FRACTION)),
            Done::Recompiled => s.recompiled,
            Done::Exported => s.exported,
        };
        if is_done {
            self.tour_go(step + 1, window, cx);
        }
    }

    /// Show step `to`, putting back what the step before opened.
    fn tour_go(&mut self, to: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = &self.tour else { return };
        let before = t.restore;
        let show = STEPS.get(to).map_or(Show::Nothing, |s| s.show);
        // what stays open for the next step stays as it is
        let keep = matches!(
            (before, show),
            (Restore::Ribbon { .. }, Show::Ribbon(_))
                | (Restore::Files { .. }, Show::Files)
                | (Restore::Sidebar { .. }, Show::Sidebar)
        );
        let restore = if keep {
            before
        } else {
            self.tour_put_back(before, cx);
            self.tour_open(show, cx)
        };
        if let Show::Ribbon(tab) = show {
            self.ribbon_tab = tab;
        }
        // the pages, the ribbon and the buttons a step points at are main.tex's, as pages (the
        // files step leaves the tab to the reader)
        if to < STEPS.len() && show != Show::Files {
            self.open("main.tex".into(), window, cx);
            if let Some(d) = self.doc().cloned() {
                d.update(cx, |d, cx| d.set_source_mode(false, window, cx));
            }
        }
        let since = Since {
            main: self.tour_source("main.tex", cx),
            chapter: self.tour_source("chapter.tex", cx),
            boxed: self.tour_box(cx),
            ..Default::default()
        };
        if let Some(t) = &mut self.tour {
            t.step = to;
            t.restore = restore;
            t.since = since;
        }
        cx.notify();
    }

    fn tour_open(&mut self, show: Show, cx: &mut Context<Self>) -> Restore {
        match show {
            Show::Nothing => Restore::Nothing,
            Show::Ribbon(_) => {
                let collapsed = settings::AppSettings::get(cx).ribbon_collapsed;
                if collapsed {
                    settings::AppSettings::set(cx, |s| s.ribbon_collapsed = false);
                }
                Restore::Ribbon {
                    collapsed,
                    tab: self.ribbon_tab,
                }
            }
            Show::Files => {
                let r = Restore::Files {
                    shown: self.sidebar,
                    anyway: self.fit.left_anyway,
                };
                self.sidebar = true;
                self.fit.left_anyway = true;
                r
            }
            Show::Sidebar => {
                let closed = settings::AppSettings::get(cx).inspector_closed;
                if closed {
                    settings::AppSettings::set(cx, |s| s.inspector_closed = false);
                }
                let r = Restore::Sidebar {
                    closed,
                    anyway: self.fit.right_anyway,
                };
                self.fit.right_anyway = true;
                r
            }
        }
    }

    fn tour_put_back(&mut self, r: Restore, cx: &mut Context<Self>) {
        match r {
            Restore::Nothing => {}
            Restore::Ribbon { collapsed, tab } => {
                self.ribbon_tab = tab;
                if collapsed {
                    settings::AppSettings::set(cx, |s| s.ribbon_collapsed = true);
                }
            }
            Restore::Files { shown, anyway } => {
                self.sidebar = shown;
                self.fit.left_anyway = anyway;
            }
            Restore::Sidebar { closed, anyway } => {
                if closed {
                    settings::AppSettings::set(cx, |s| s.inspector_closed = true);
                }
                self.fit.right_anyway = anyway;
            }
        }
    }

    /// The tour over (the window stays: the scratch copy is there to try things in), what its
    /// step opened put back.
    pub fn tour_end(&mut self, cx: &mut Context<Self>) {
        if let Some(t) = self.tour.take() {
            self.tour_put_back(t.restore, cx);
        }
        cx.notify();
    }

    /// The window dimmed but for the step's controls, and the step's card beside the first.
    pub(super) fn render_tour(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let ring = cx.theme().primary;
        let t = self.tour.as_mut()?;
        // the marks painted last frame: a control not painted again is gone, and its hole too
        let seen = std::mem::take(&mut *t.marks.borrow_mut());
        t.holes = STEPS.get(t.step).map_or(vec![], |s| {
            s.marks
                .iter()
                .filter_map(|m| seen.get(m))
                .map(|b| b.dilate(px(4.0)))
                .collect()
        });
        let (marks, holes) = (t.marks.clone(), t.holes.clone());
        let dim = hsla(0.0, 0.0, 0.0, 0.45);
        // in window coordinates, as the marks are (a window drawing its own frame on Linux pads
        // the workspace in it)
        let layer = canvas(
            // a control moved, came or went since this frame was laid out: one frame more, so
            // the holes follow it without waiting for input
            move |_, window, _| {
                if *marks.borrow() != seen {
                    window.request_animation_frame();
                }
            },
            // no hitbox: clicks go through the dim, what a step asks for is done in the window
            move |area, _, window, _| {
                for r in around(area, &holes) {
                    window.paint_quad(fill(r, dim));
                }
                for h in &holes {
                    window.paint_quad(
                        outline(*h, ring, BorderStyle::Solid)
                            .border_widths(px(2.0))
                            .corner_radii(px(6.0)),
                    );
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        let t = self.tour.as_ref()?;
        let card = self.tour_card(t, t.holes.first().copied(), window.viewport_size(), cx);
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(layer)
                .child(card)
                .into_any_element(),
        )
    }

    fn tour_card(
        &self,
        t: &Tour,
        hole: Option<Bounds<Pixels>>,
        view: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        const W: f32 = 320.0;
        // room the card needs above or below its control (it is about this tall)
        const H: f32 = 190.0;
        const GAP: f32 = 12.0;
        let (vw, vh) = (f32::from(view.width), f32::from(view.height));
        let card = div()
            .id("tour-card")
            .occlude()
            .w(px(W))
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .shadow_lg()
            .role(Role::Dialog);
        let clamp_x = |x: f32| x.clamp(GAP, (vw - W - GAP).max(GAP));
        // below the control, else above it, else beside it, else inside it (the pages)
        let (x, y, anchor) = match hole.map(|b| {
            let (x, y) = (f32::from(b.origin.x), f32::from(b.origin.y));
            (x, y, f32::from(b.size.width), f32::from(b.size.height))
        }) {
            None => (clamp_x((vw - W) / 2.0), vh / 3.0, Anchor::TopLeft),
            Some((x, y, w, h)) => {
                let mid = clamp_x(x + w / 2.0 - W / 2.0);
                if y + h + GAP + H < vh {
                    (mid, y + h + GAP, Anchor::TopLeft)
                } else if y - GAP - H > 0.0 {
                    (mid, y - GAP, Anchor::BottomLeft)
                } else if x + w + GAP + W < vw {
                    (x + w + GAP, y + 2.0 * GAP, Anchor::TopLeft)
                } else if x - GAP - W > 0.0 {
                    (x - GAP - W, y + 2.0 * GAP, Anchor::TopLeft)
                } else {
                    (
                        clamp_x(x + w - W - 2.0 * GAP),
                        y + 2.0 * GAP,
                        Anchor::TopLeft,
                    )
                }
            }
        };
        let place = |card| {
            anchored()
                .position(point(px(x), px(y)))
                .anchor(anchor)
                .snap_to_window_with_margin(px(GAP))
                .child(card)
        };
        let Some(step) = STEPS.get(t.step) else {
            return place(card
                .aria_label("That’s the tour")
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child("That’s the tour"))
                .child(div().text_sm().child(
                    "This copy is deleted when you close its window. Start a project of your own to keep your work.",
                ))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .pt_1()
                        .child(
                            Button::new("tour-close")
                                .label("Close the Tour")
                                .ghost()
                                .small()
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(crate::ui::projects::CloseProject), cx)
                                }),
                        )
                        .child(
                            Button::new("tour-new")
                                .label("Start Your Own Project")
                                .primary()
                                .small()
                                .on_click(|_, _, cx| crate::ui::projects::new_project(cx)),
                        ),
                ));
        };
        let n = t.step;
        place(
            card.aria_label(step.title)
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("{} of {}", n + 1, STEPS.len())),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(step.title),
                )
                .child(div().text_sm().child(keys_in(step.body)))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .pt_1()
                        .child(
                            Button::new("tour-skip")
                                .label("Skip Tour")
                                .ghost()
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| this.tour_end(cx))),
                        )
                        .child(div().flex_1())
                        .when(n > 0, |d| {
                            d.child(
                                Button::new("tour-back")
                                    .label("Back")
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.tour_go(n - 1, window, cx)
                                    })),
                            )
                        })
                        // a step that waits can still be passed, should what it waits for not come
                        .child(
                            if step.done.is_some() {
                                Button::new("tour-next")
                                    .label("Skip Step")
                                    .outline()
                                    .small()
                            } else {
                                Button::new("tour-next").label("Next").primary().small()
                            }
                            .on_click(cx.listener(
                                move |this, _, window, cx| this.tour_go(n + 1, window, cx),
                            )),
                        ),
                ),
        )
    }
}

/// `area` but for `holes`: cut across at each hole's top and bottom, each band then around the
/// holes that cross it.
fn around(area: Bounds<Pixels>, holes: &[Bounds<Pixels>]) -> Vec<Bounds<Pixels>> {
    let mut ys: Vec<Pixels> = holes
        .iter()
        .flat_map(|h| [h.top(), h.bottom()])
        .map(|y| y.clamp(area.top(), area.bottom()))
        .chain([area.top(), area.bottom()])
        .collect();
    ys.sort();
    ys.dedup();
    let mut out = vec![];
    for band in ys.windows(2) {
        let (top, bottom) = (band[0], band[1]);
        let mut cuts: Vec<(Pixels, Pixels)> = holes
            .iter()
            .filter(|h| h.top() < bottom && h.bottom() > top)
            .map(|h| (h.left(), h.right()))
            .collect();
        cuts.sort();
        let mut x = area.left();
        for (left, right) in cuts.into_iter().chain([(area.right(), area.right())]) {
            let left = left.min(area.right());
            if left > x {
                out.push(Bounds::from_corners(point(x, top), point(left, bottom)));
            }
            x = x.max(right);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::around;
    use gpui_kit::{Bounds, point, px, size};

    /// Every point of the area is dimmed once, or is in a hole and not dimmed: holes that
    /// overlap, touch or run past the area included.
    #[test]
    fn the_dim_covers_all_but_the_holes() {
        let b =
            |x: f32, y: f32, w: f32, h: f32| Bounds::new(point(px(x), px(y)), size(px(w), px(h)));
        let area = b(0.0, 0.0, 100.0, 80.0);
        let holes = [
            b(10.0, 10.0, 20.0, 20.0),
            b(20.0, 20.0, 30.0, 15.0),
            b(50.0, 20.0, 10.0, 10.0),
            b(90.0, 70.0, 30.0, 30.0),
            b(-5.0, 50.0, 10.0, 5.0),
        ];
        let dim = around(area, &holes);
        for x in 0..100 {
            for y in 0..80 {
                let p = point(px(x as f32 + 0.5), px(y as f32 + 0.5));
                let dims = dim.iter().filter(|r| r.contains(&p)).count();
                let holed = holes.iter().any(|h| h.contains(&p));
                assert_eq!(dims, usize::from(!holed), "at {x}, {y}");
            }
        }
        assert_eq!(around(area, &[]), vec![area]);
    }
}

#[cfg(test)]
mod e2e {
    use super::super::Workspace;
    use super::super::ribbon::RibbonTab;
    use super::{CHAPTER, FRACTION, MAIN, STEPS, export_dir};
    use crate::ui::editor::Doc;
    use crate::ui::settings::{AppSettings, TexView};
    use gpui_kit::InputEvent as _;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        App, AppContext, Bounds, Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent,
        Pixels, TestAppContext, Window, WindowBounds, WindowOptions, point, px, size,
    };
    use std::time::{Duration, Instant};

    /// The tour from its first step to its end, driven as a reader would: its card's buttons,
    /// Escape and Return, typing in source boxes, the ribbon's B, Recompile and Export. A step
    /// that asks for something goes on once it is done since the step began, and not on what
    /// was done before (an earlier step's work, or the step itself before Back); the holes
    /// follow their controls a frame later without input; what a step opened is put back when
    /// the tour ends (by Escape); it starts again after Skip; nothing is left after.
    /// `cargo test --release the_tour_walks_through -- --ignored` (opens the tour with rtex).
    #[gpui_kit::test]
    #[ignore]
    fn the_tour_walks_through(cx: &mut TestAppContext) {
        let dir = super::create().unwrap();
        std::fs::write(dir.join("main.tex"), "changed").unwrap();
        super::create().unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("main.tex")).unwrap(), MAIN);
        assert_eq!(
            std::fs::read_to_string(dir.join("chapter.tex")).unwrap(),
            CHAPTER
        );
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            // folded and closed, to be unfolded and opened for their steps; .tex files open as
            // source, which the tour's must not
            cx.set_global(AppSettings {
                ribbon_collapsed: true,
                inspector_closed: true,
                open_tex_in: TexView::Source,
                ..Default::default()
            });
            crate::ui::editor::bind_keys(cx);
            crate::ui::keymap::bind_all(cx);
        });
        crate::engine::configure("", "/Library/TeX/texbin");
        let options = |cx: &App| WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1460.0), px(900.0)), cx)),
            ..Default::default()
        };
        let (window, ws) = cx
            .update(|cx| {
                gpui_kit::open_window(options(cx), cx, |window, cx| {
                    let d = dir.clone();
                    cx.new(|cx| Workspace::new(d, Some("main.tex".into()), window, cx))
                })
            })
            .unwrap();
        let in_window = |cx: &mut TestAppContext, f: &mut dyn FnMut(&mut Window, &mut App)| {
            cx.update_window(window, |_, window, cx| f(window, cx))
                .unwrap()
        };
        let frame = |cx: &mut TestAppContext| {
            for _ in 0..3 {
                cx.run_until_parked();
                in_window(cx, &mut |window, cx| window.render_frame(cx));
            }
        };
        let step =
            |cx: &mut TestAppContext| cx.read(|cx| ws.read(cx).tour.as_ref().map(|t| t.step));
        let press = |cx: &mut TestAppContext, key: &str| {
            in_window(cx, &mut |window, cx| window.press(key, cx));
            frame(cx);
        };
        let click = |cx: &mut TestAppContext, id: &'static str| {
            in_window(cx, &mut |window, cx| window.click(id, cx));
            frame(cx);
        };
        let doc = |cx: &mut TestAppContext, path: &str| -> Entity<Doc> {
            cx.read(|cx| {
                ws.read(cx)
                    .tabs
                    .iter()
                    .find(|d| d.read(cx).path == path)
                    .unwrap()
                    .clone()
            })
        };
        let main = doc(cx, "main.tex");
        let source = |cx: &mut TestAppContext| cx.read(|cx| main.read(cx).source.clone());
        // rtex answers from its own process: real time passes
        let wait = |cx: &mut TestAppContext, what: &str, done: &dyn Fn(&App) -> bool| {
            let until = Instant::now() + Duration::from_secs(90);
            loop {
                frame(cx);
                if cx.read(|cx| done(cx)) {
                    return;
                }
                assert!(Instant::now() < until, "timed out waiting for {what}");
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        // typeset, and rtex quiet for a while (its log stays the same)
        let calm = |cx: &mut TestAppContext| {
            let (mut log, mut since) = (0, Instant::now());
            loop {
                frame(cx);
                let (busy, now) = cx.read(|cx| {
                    let d = main.read(cx);
                    (d.compiling || d.page_position().1 == 0, d.log.len())
                });
                if busy || now != log {
                    (log, since) = (now, Instant::now());
                } else if since.elapsed() > Duration::from_millis(600) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        // a source box on the paragraph at `at` in `d`, and `text` typed at its start
        let type_in = |cx: &mut TestAppContext, d: &Entity<Doc>, at: &str, text: &str| {
            let off = cx.read(|cx| d.read(cx).source.find(at).unwrap());
            in_window(cx, &mut |window, cx| {
                d.update(cx, |d, cx| d.edit_at(off, window, cx))
            });
            frame(cx);
            in_window(cx, &mut |window, cx| window.input(text, cx));
            frame(cx);
        };

        frame(cx);
        assert_eq!(step(cx), Some(0));
        assert!(
            !cx.read(|cx| main.read(cx).source_mode),
            "pages, not source"
        );
        // the card in the window
        in_window(cx, &mut |window, _| {
            let card = window.find("tour-next").bounds();
            let size = window.viewport_size();
            assert!(card.origin.x >= px(0.0) && card.origin.x + card.size.width <= size.width);
            assert!(card.origin.y >= px(0.0) && card.origin.y + card.size.height <= size.height);
        });
        calm(cx);

        // typed before the typing step: not its work
        type_in(cx, &main, "Click anywhere", "Hello. ");
        assert!(
            source(cx).contains("Hello. Click anywhere"),
            "typed in the box"
        );
        assert_eq!(step(cx), Some(0));
        // Escape is the open source box's; then Return goes on
        press(cx, "escape");
        assert_eq!(step(cx), Some(0), "the box closed, not the tour");
        assert!(!cx.read(|cx| main.read(cx).box_open()));
        press(cx, "enter");
        assert_eq!(step(cx), Some(1));
        frame(cx);
        assert_eq!(
            step(cx),
            Some(1),
            "what was typed before is not this step's"
        );
        type_in(cx, &main, "Hello. ", "Now. ");
        assert_eq!(step(cx), Some(2), "typed: on to the source box");
        frame(cx);
        assert_eq!(step(cx), Some(2), "the box open already is not this step's");
        // Back stays back: the step is not done again by what was done in it
        click(cx, "tour-back");
        assert_eq!(step(cx), Some(1));
        frame(cx);
        assert_eq!(step(cx), Some(1), "Back does not bounce forward");
        type_in(cx, &main, "Now. ", "Then. ");
        assert_eq!(step(cx), Some(2));
        // the formula's box: on to the Home tab, the ribbon unfolded
        let formula = source(cx).find("e^{i\\pi}").unwrap();
        in_window(cx, &mut |window, cx| {
            main.update(cx, |d, cx| d.edit_at(formula, window, cx))
        });
        frame(cx);
        assert_eq!(step(cx), Some(3));
        cx.read(|cx| {
            assert!(!AppSettings::get(cx).ribbon_collapsed, "unfolded");
            assert!(ws.read(cx).ribbon_tab == RibbonTab::Home);
        });

        // the holes follow their controls a frame later: back to the box step (the ribbon folds
        // again), then on, which unfolds it under the hole
        click(cx, "tour-back");
        assert_eq!(step(cx), Some(2));
        assert!(
            cx.read(|cx| AppSettings::get(cx).ribbon_collapsed),
            "folded again"
        );
        calm(cx);
        let cut = |cx: &mut TestAppContext| -> (Vec<Bounds<Pixels>>, Vec<Bounds<Pixels>>) {
            cx.read(|cx| {
                let t = ws.read(cx).tour.as_ref().unwrap();
                let marks = t.marks.borrow();
                let want = STEPS[t.step]
                    .marks
                    .iter()
                    .map(|m| marks[m].dilate(px(4.0)))
                    .collect();
                (t.holes.clone(), want)
            })
        };
        in_window(cx, &mut |window, cx| {
            ws.update(cx, |ws, cx| ws.tour_go(3, window, cx))
        });
        cx.run_until_parked();
        let (holes, ribbon) = cut(cx);
        assert_ne!(
            holes, ribbon,
            "the first frame is cut where the folded ribbon was"
        );
        // the platform's next frame (a window draws only when something asks for one)
        let next = |cx: &mut TestAppContext| {
            let n = cx
                .update_window(window, |_, window, cx| window.simulate_next_frame(cx))
                .unwrap();
            cx.run_until_parked();
            n
        };
        next(cx);
        let (holes, ribbon) = cut(cx);
        assert_eq!(holes, ribbon, "the hole on the ribbon one frame later");
        assert_eq!(next(cx), 0, "and no more: the holes stay");
        // the holes are around the controls themselves
        in_window(cx, &mut |window, cx| {
            let t = ws.read(cx).tour.as_ref().unwrap();
            let marks = t.marks.borrow();
            for (mark, id) in [
                ("bold", "rb-bold"),
                ("export", "export"),
                ("recompile", "recompile"),
                ("logs", "toggle-logs"),
                ("settings", "settings"),
                ("problems", "toggle-problems"),
            ] {
                assert_eq!(marks[mark], window.find(id).bounds(), "{mark}");
            }
        });

        // a word made bold before the bold step: not its work
        let bold = |cx: &mut TestAppContext| source(cx).matches("\\textbf").count();
        // a double-click down the middle of the pages until it selects a word not bold yet,
        // then B
        let make_bold = |cx: &mut TestAppContext| {
            let before = bold(cx);
            let pages = cx.read(|cx| ws.read(cx).tour.as_ref().unwrap().marks.borrow()["pages"]);
            let mut word = String::new();
            for i in 0..30 {
                let at = point(pages.center().x, pages.top() + px(60.0 + 20.0 * i as f32));
                in_window(cx, &mut |window, cx| {
                    for n in [1, 2] {
                        window.dispatch_event(
                            MouseDownEvent {
                                button: MouseButton::Left,
                                position: at,
                                modifiers: Modifiers::default(),
                                click_count: n,
                                first_mouse: false,
                            }
                            .to_platform_input(),
                            cx,
                        );
                        window.dispatch_event(
                            MouseUpEvent {
                                button: MouseButton::Left,
                                position: at,
                                modifiers: Modifiers::default(),
                                click_count: n,
                            }
                            .to_platform_input(),
                            cx,
                        );
                    }
                });
                frame(cx);
                word = cx
                    .read(|cx| {
                        let d = main.read(cx);
                        let plain = !d.ribbon_state().has("textbf");
                        Some(d.selected_text()).filter(|w| plain && !w.trim().is_empty())
                    })
                    .unwrap_or_default();
                if !word.is_empty() {
                    break;
                }
            }
            assert!(!word.is_empty(), "a word selected");
            click(cx, "rb-bold");
            assert_eq!(bold(cx), before + 1, "{word} bold");
        };
        press(cx, "escape");
        assert_eq!(step(cx), Some(3), "the formula's box closed, not the tour");
        make_bold(cx);
        click(cx, "tour-next");
        assert_eq!(step(cx), Some(4));
        frame(cx);
        assert_eq!(step(cx), Some(4), "the word bold before is not this step's");
        // the step's holes: B, and the pages where the word is
        let (holes, want) = cut(cx);
        assert_eq!(holes, want);
        assert_eq!(holes.len(), 2);
        make_bold(cx);
        assert_eq!(step(cx), Some(5), "bold: on to the Insert tab");
        assert!(cx.read(|cx| ws.read(cx).ribbon_tab == RibbonTab::Insert));
        click(cx, "tour-next");
        assert_eq!(step(cx), Some(6));
        cx.read(|cx| {
            assert!(AppSettings::get(cx).ribbon_collapsed, "folded again");
            assert!(ws.read(cx).ribbon_tab == RibbonTab::Home);
            assert!(ws.read(cx).fit.left, "the project panel shown");
        });

        // chapter.tex opened (as pages, though .tex files open as source) and typed in
        in_window(cx, &mut |window, cx| {
            ws.update(cx, |ws, cx| ws.open("chapter.tex".into(), window, cx))
        });
        frame(cx);
        let chapter = doc(cx, "chapter.tex");
        assert!(!cx.read(|cx| chapter.read(cx).source_mode), "pages");
        assert_eq!(step(cx), Some(6), "opening it is not typing in it");
        wait(cx, "chapter.tex typeset", &|cx| {
            chapter.read(cx).page_position().1 > 0
        });
        type_in(cx, &chapter, "This section", "Mine. ");
        assert!(cx.read(|cx| chapter.read(cx).source.contains("Mine. This section")));
        assert_eq!(step(cx), Some(7), "typed in chapter.tex");
        assert!(cx.read(|cx| ws.read(cx).shown_file(cx)).as_deref() == Some("main.tex"));
        press(cx, "escape");
        press(cx, "enter");
        assert_eq!(step(cx), Some(8));

        // TeX broken and mended: its brace deleted in the formula's box (which keeps a brace
        // left open from the pages until it closes), then undone on the pages
        let at = source(cx).find(FRACTION).unwrap();
        in_window(cx, &mut |window, cx| {
            main.update(cx, |d, cx| d.edit_at(at, window, cx))
        });
        frame(cx);
        let start = cx.read(|cx| main.read(cx).box_at()).unwrap();
        for _ in start..at + FRACTION.len() {
            in_window(cx, &mut |window, cx| window.press("right", cx));
        }
        press(cx, "backspace");
        press(cx, "escape");
        assert!(
            !source(cx).contains(FRACTION),
            "broken: {}",
            &source(cx)[at..at + 20]
        );
        assert_eq!(step(cx), Some(8));
        press(cx, "cmd-z");
        assert!(source(cx).contains(FRACTION), "mended");
        assert_eq!(step(cx), Some(9), "broken and mended");
        click(cx, "tour-next");
        assert_eq!(step(cx), Some(10));
        cx.read(|cx| {
            assert!(!AppSettings::get(cx).inspector_closed);
            assert!(ws.read(cx).fit.right, "the sidebar shown");
            let t = ws.read(cx).tour.as_ref().unwrap();
            let marks = t.marks.borrow();
            for name in STEPS.iter().flat_map(|s| s.marks).filter(|m| **m != "bold") {
                assert!(marks.contains_key(name), "{name} marked");
            }
        });

        // Escape ends the tour, the sidebar closed again
        press(cx, "escape");
        assert_eq!(step(cx), None);
        assert!(
            cx.read(|cx| AppSettings::get(cx).inspector_closed),
            "closed again"
        );
        // started again (Help > Show the Tour), and again after Skip
        in_window(cx, &mut |window, cx| {
            ws.update(cx, |ws, cx| ws.tour_start(window, cx))
        });
        frame(cx);
        assert_eq!(step(cx), Some(0));
        click(cx, "tour-skip");
        assert_eq!(step(cx), None);
        calm(cx);
        in_window(cx, &mut |window, cx| {
            ws.update(cx, |ws, cx| ws.tour_start(window, cx))
        });
        cx.run_until_parked();
        assert_eq!(step(cx), Some(0));
        // its first frame knows no control yet; the next cuts the hole
        next(cx);
        let (holes, pages) = cut(cx);
        assert_eq!(holes, pages, "the hole on the pages one frame later");
        assert_eq!(next(cx), 0, "and no more");

        // a recompile before the recompile step: not its work
        let recompile = STEPS.iter().position(|s| s.marks == ["recompile"]).unwrap();
        in_window(cx, &mut |window, cx| {
            ws.update(cx, |ws, cx| ws.tour_go(recompile - 1, window, cx))
        });
        frame(cx);
        click(cx, "recompile");
        click(cx, "tour-next");
        assert_eq!(step(cx), Some(recompile));
        frame(cx);
        assert_eq!(
            step(cx),
            Some(recompile),
            "the recompile before is not this step's"
        );
        click(cx, "recompile");
        assert_eq!(step(cx), Some(recompile + 1));
        click(cx, "tour-next");
        assert_eq!(step(cx), Some(STEPS.len() - 1));

        // export: the dialog in Documents, not the scratch folder; cancelled, the step stays
        calm(cx);
        click(cx, "export");
        cx.simulate_new_path_selection(|at| {
            assert_eq!(at, export_dir());
            None
        });
        frame(cx);
        assert_eq!(step(cx), Some(STEPS.len() - 1), "cancelled: not exported");
        // a PDF made: the end, with the way to a project of one's own
        let pdf = std::env::temp_dir().join(format!("rtd-tour-{}.pdf", std::process::id()));
        let _ = std::fs::remove_file(&pdf);
        click(cx, "export");
        cx.simulate_new_path_selection(|_| Some(pdf.clone()));
        wait(cx, "the PDF", &|cx| {
            ws.read(cx).tour.as_ref().map(|t| t.step) == Some(STEPS.len())
        });
        assert!(pdf.is_file());
        let _ = std::fs::remove_file(&pdf);
        in_window(cx, &mut |window, _| {
            window.find("tour-new");
        });
        cx.read(|cx| {
            let s = AppSettings::get(cx);
            assert!(s.ribbon_collapsed && s.inspector_closed, "all put back");
        });
        super::remove();
        assert!(!dir.exists());
    }
}
