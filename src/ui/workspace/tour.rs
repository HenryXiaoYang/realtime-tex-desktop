// The tour: a scratch project whose pages explain the app, opened in a window of its own, and the
// window dimmed but for one control at a time, a card beside it saying what it is for. A step
// that asks for something moves on once it is done; the copy is deleted when its window closes.
use super::*;
use crate::ui::keys_in;
use std::cell::RefCell;
use std::collections::HashMap;

const MAIN: &str = include_str!("tour/main.tex");
const CHAPTER: &str = include_str!("tour/chapter.tex");

/// The scratch copy's folder: one, so that a second tour brings the first one's window back.
pub fn dir() -> PathBuf {
    std::env::temp_dir().join("Realtime Tex Tour")
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

/// What a step needs on screen: the control it points at may be folded or closed.
#[derive(Clone, Copy, PartialEq)]
enum Show {
    Nothing,
    Ribbon(ribbon::RibbonTab),
    Files,
    Sidebar,
}

/// What a step that asks for something waits for.
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
    /// The control pointed at (its mark's name).
    mark: &'static str,
    title: &'static str,
    body: &'static str,
    show: Show,
    /// None: it only shows something, and Next goes on.
    done: Option<Done>,
}

const fn see(mark: &'static str, show: Show, title: &'static str, body: &'static str) -> Step {
    Step {
        mark,
        title,
        body,
        show,
        done: None,
    }
}

const fn act(
    mark: &'static str,
    show: Show,
    done: Done,
    title: &'static str,
    body: &'static str,
) -> Step {
    Step {
        mark,
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
        "pages",
        Show::Nothing,
        "Your document, typeset",
        "There is no source pane: you write on these pages, and LaTeX sets what you type as it will print.",
    ),
    act(
        "pages",
        Show::Nothing,
        Done::TypedInMain,
        "Write on the page",
        "Click in the sentence under “1 Write on the pages” and type a few words.",
    ),
    act(
        "pages",
        Show::Nothing,
        Done::BoxOpened,
        "Open the source box",
        "Click the formula in section 2. Its TeX opens in a box on the page, with the result under it as you type.",
    ),
    see(
        "ribbon",
        HOME,
        "The Home tab",
        "Bold, italic, headings, lists and alignment, as in a word processor. Each puts in the LaTeX for it.",
    ),
    act(
        "bold",
        HOME,
        Done::MadeBold,
        "Make a word bold",
        "Double-click a word in section 3, then click B (or press ⌘B).",
    ),
    see(
        "ribbon",
        INSERT,
        "The Insert tab",
        "Formulas, pictures, tables, footnotes and references: what you add to the document.",
    ),
    act(
        "files",
        Show::Files,
        Done::TypedInChapter,
        "Your files",
        "Open chapter.tex here and type in it. A file the main one brings in with \\input is edited on the same pages.",
    ),
    see(
        "status-left",
        Show::Nothing,
        "Pages and panels",
        "Where you are in the document, and the button that shows or hides the project panel.",
    ),
    act(
        "problems",
        Show::Nothing,
        Done::BrokeAndFixed,
        "When TeX breaks",
        "In main.tex, delete the } after the 2 in section 5. LaTeX can’t typeset that, so Problems turns red: open it, then Undo (⌘Z).",
    ),
    see(
        "logs",
        Show::Nothing,
        "rtex’s log",
        "What rtex, the engine that typesets your pages, says. Look here when the pages look wrong.",
    ),
    see(
        "sidebar",
        Show::Sidebar,
        "The sidebar",
        "Choices for what the caret is in, and for the document itself: its title, author, paper and fonts.",
    ),
    act(
        "recompile",
        Show::Nothing,
        Done::Recompiled,
        "Run LaTeX in full",
        "Paragraphs update as you type; a full LaTeX run catches up with page breaks, references and the contents. Recompile (⌘⇧R) runs one now.",
    ),
    see(
        "settings",
        Show::Nothing,
        "Settings",
        "The theme, fonts, autosave and shortcuts, and where rtex and TeX are.",
    ),
    act(
        "export",
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

pub(super) struct Tour {
    /// The step shown; STEPS.len() is the end.
    step: usize,
    restore: Restore,
    /// The controls' bounds last frame, by name (shortcut: a control gone keeps its last
    /// bounds; each step shows its own first).
    marks: Rc<RefCell<HashMap<&'static str, Bounds<Pixels>>>>,
    /// Problems listed an error while the TeX-breaks step was shown.
    saw_error: bool,
    pub(super) recompiled: bool,
    pub(super) exported: bool,
}

impl Tour {
    pub(super) fn new() -> Self {
        Self {
            step: 0,
            restore: Restore::Nothing,
            marks: Default::default(),
            saw_error: false,
            recompiled: false,
            exported: false,
        }
    }
}

impl Workspace {
    /// A canvas over a control the tour points at, keeping its bounds for the next frame.
    pub(super) fn tour_mark(&self, name: &'static str) -> Option<impl IntoElement + use<>> {
        let marks = self.tour.as_ref()?.marks.clone();
        Some(
            canvas(
                move |b, window, _| {
                    if marks.borrow_mut().insert(name, b) != Some(b) {
                        window.refresh();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
    }

    fn tour_doc<'a>(&self, path: &str, cx: &'a App) -> Option<&'a Doc> {
        self.tabs
            .iter()
            .map(|d| d.read(cx))
            .find(|d| d.path == path)
    }

    /// The step done: on to the next. Called each frame.
    pub(super) fn tour_check(&mut self, cx: &mut Context<Self>) {
        let Some(t) = &self.tour else { return };
        let (step, saw_error, recompiled, exported) =
            (t.step, t.saw_error, t.recompiled, t.exported);
        let Some(done) = STEPS.get(step).and_then(|s| s.done) else {
            return;
        };
        let errors = |ws: &Self, cx: &App| {
            ws.tabs
                .iter()
                .any(|d| d.read(cx).problems.iter().any(|p| p.severity == "error"))
        };
        let bold = |s: &str| s.matches("\\textbf").count();
        let is_done = match done {
            Done::TypedInMain => self
                .tour_doc("main.tex", cx)
                .is_some_and(|d| d.source != MAIN),
            Done::BoxOpened => self.doc().is_some_and(|d| d.read(cx).box_open()),
            Done::MadeBold => self
                .tour_doc("main.tex", cx)
                .is_some_and(|d| bold(&d.source) > bold(MAIN)),
            Done::TypedInChapter => self
                .tour_doc("chapter.tex", cx)
                .is_some_and(|d| d.source != CHAPTER),
            Done::BrokeAndFixed => {
                if errors(self, cx) {
                    self.tour.as_mut().unwrap().saw_error = true;
                    false
                } else {
                    saw_error
                }
            }
            Done::Recompiled => recompiled,
            Done::Exported => exported,
        };
        if is_done {
            self.tour_go(step + 1, cx);
        }
    }

    /// Show step `to`, putting back what the step before opened.
    fn tour_go(&mut self, to: usize, cx: &mut Context<Self>) {
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
        if let Some(t) = &mut self.tour {
            t.step = to;
            t.restore = restore;
            t.saw_error = false;
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

    /// The window dimmed but for the step's control, and the step's card beside it.
    pub(super) fn render_tour(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let t = self.tour.as_ref()?;
        let theme = cx.theme().clone();
        let view = window.viewport_size();
        let (vw, vh) = (f32::from(view.width), f32::from(view.height));
        let step = STEPS.get(t.step);
        let hole = step
            .and_then(|s| t.marks.borrow().get(s.mark).copied())
            .map(|b| {
                let pad = 4.0;
                (
                    f32::from(b.origin.x) - pad,
                    f32::from(b.origin.y) - pad,
                    f32::from(b.size.width) + 2.0 * pad,
                    f32::from(b.size.height) + 2.0 * pad,
                )
            });
        // the dim lets clicks through (no handlers): what a step asks for is done in the window
        let dim = hsla(0.0, 0.0, 0.0, 0.45);
        let rect = |x: f32, y: f32, w: f32, h: f32| {
            div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(w.max(0.0)))
                .h(px(h.max(0.0)))
                .bg(dim)
        };
        let mut layer = div().absolute().top_0().left_0().size_full();
        if let Some((x, y, w, h)) = hole {
            layer = layer
                .child(rect(0.0, 0.0, vw, y))
                .child(rect(0.0, y + h, vw, vh - y - h))
                .child(rect(0.0, y, x, h))
                .child(rect(x + w, y, vw - x - w, h))
                .child(
                    div()
                        .absolute()
                        .left(px(x))
                        .top(px(y))
                        .w(px(w))
                        .h(px(h))
                        .rounded_md()
                        .border_2()
                        .border_color(theme.primary),
                );
        }
        Some(
            layer
                .child(self.tour_card(t, hole, (vw, vh), cx))
                .into_any_element(),
        )
    }

    fn tour_card(
        &self,
        t: &Tour,
        hole: Option<(f32, f32, f32, f32)>,
        (vw, vh): (f32, f32),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        const W: f32 = 320.0;
        // room the card needs above or below its control (it is about this tall)
        const H: f32 = 190.0;
        const GAP: f32 = 12.0;
        let card = div()
            .id("tour-card")
            .occlude()
            .absolute()
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
        let card = match hole {
            None => card.left(px(clamp_x((vw - W) / 2.0))).top(px(vh / 3.0)),
            Some((x, y, w, h)) => {
                let mid = clamp_x(x + w / 2.0 - W / 2.0);
                if y + h + GAP + H < vh {
                    card.left(px(mid)).top(px(y + h + GAP))
                } else if y - GAP - H > 0.0 {
                    card.left(px(mid)).bottom(px(vh - y + GAP))
                } else if x + w + GAP + W < vw {
                    card.left(px(x + w + GAP)).top(px(y + 2.0 * GAP))
                } else if x - GAP - W > 0.0 {
                    card.left(px(x - GAP - W)).top(px(y + 2.0 * GAP))
                } else {
                    card.left(px(clamp_x(x + w - W - 2.0 * GAP)))
                        .top(px(y + 2.0 * GAP))
                }
            }
        };
        let Some(step) = STEPS.get(t.step) else {
            return card
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
                );
        };
        let n = t.step;
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
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.tour_go(n - 1, cx)),
                                ),
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
                        .on_click(cx.listener(move |this, _, _, cx| this.tour_go(n + 1, cx))),
                    ),
            )
    }
}

#[cfg(test)]
mod e2e {
    use super::super::Workspace;
    use super::{CHAPTER, MAIN, STEPS};
    use crate::ui::settings::AppSettings;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{App, AppContext, TestAppContext, WindowBounds, WindowOptions, px, size};

    /// The tour from its first step to its end: a fresh copy each time; the spotlight on the
    /// step's control, its card clickable over the dim; a step that asks for something going on
    /// once it is done; what a step unfolds or opens put back after it; nothing left after.
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
            // folded and closed, to be unfolded and opened for their steps
            cx.set_global(AppSettings {
                ribbon_collapsed: true,
                inspector_closed: true,
                ..Default::default()
            });
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
        let frame = |cx: &mut TestAppContext| {
            for _ in 0..3 {
                cx.run_until_parked();
                cx.update_window(window, |_, window, cx| window.render_frame(cx))
                    .unwrap();
            }
        };
        let step =
            |cx: &mut TestAppContext| cx.read(|cx| ws.read(cx).tour.as_ref().map(|t| t.step));
        frame(cx);
        assert_eq!(step(cx), Some(0));
        // the card in the window, and its Next taken by the card, not the dim
        cx.update_window(window, |_, window, cx| {
            let card = window.find("tour-next").bounds();
            let size = window.viewport_size();
            assert!(card.origin.x >= px(0.0) && card.origin.x + card.size.width <= size.width);
            assert!(card.origin.y >= px(0.0) && card.origin.y + card.size.height <= size.height);
            window.click("tour-next", cx);
        })
        .unwrap();
        frame(cx);
        assert_eq!(step(cx), Some(1));
        // typed on the page: on to the formula
        cx.update(|cx| {
            let doc = ws.read(cx).tabs[0].clone();
            doc.update(cx, |d, cx| {
                d.source
                    .insert_str(MAIN.find("Click anywhere").unwrap(), "Hello. ");
                cx.notify();
            });
        });
        frame(cx);
        assert_eq!(step(cx), Some(2));
        // the ribbon's steps unfold it, at their tabs; the files step folds it again
        let go = |to: usize, cx: &mut TestAppContext| {
            cx.update(|cx| ws.update(cx, |ws, cx| ws.tour_go(to, cx)));
            frame(cx);
        };
        go(3, cx);
        assert!(!cx.read(|cx| AppSettings::get(cx).ribbon_collapsed));
        go(5, cx);
        assert!(cx.read(|cx| ws.read(cx).ribbon_tab == super::super::ribbon::RibbonTab::Insert));
        go(6, cx);
        cx.read(|cx| {
            assert!(AppSettings::get(cx).ribbon_collapsed, "folded again");
            assert!(ws.read(cx).ribbon_tab == super::super::ribbon::RibbonTab::Home);
        });
        // the sidebar opened for its step, closed after it
        let sidebar = STEPS.iter().position(|s| s.mark == "sidebar").unwrap();
        go(sidebar, cx);
        cx.read(|cx| {
            assert!(!AppSettings::get(cx).inspector_closed);
            assert!(ws.read(cx).fit.right, "shown");
        });
        // each control a step points at was found
        cx.read(|cx| {
            let marks = ws.read(cx).tour.as_ref().unwrap().marks.clone();
            for name in [
                "pages",
                "ribbon",
                "files",
                "status-left",
                "problems",
                "logs",
                "sidebar",
                "recompile",
                "settings",
                "export",
            ] {
                assert!(marks.borrow().contains_key(name), "{name} marked");
            }
        });
        go(sidebar + 1, cx);
        assert!(
            cx.read(|cx| AppSettings::get(cx).inspector_closed),
            "closed again"
        );
        // exported: the end, with the way to a project of one's own
        go(STEPS.len() - 1, cx);
        cx.update(|cx| ws.update(cx, |ws, cx| ws.export(cx)));
        frame(cx);
        assert_eq!(step(cx), Some(STEPS.len()));
        cx.update_window(window, |_, window, _| {
            window.find("tour-new");
        })
        .unwrap();
        super::remove();
        assert!(!dir.exists());
    }
}
