// The sidebar on the right: the settings of the element at the caret or selected (a figure, a
// table, a heading, an equation, a list; the document when nothing is), its common ones first
// and the rest under Advanced. Changes apply as they are made (a text field's a moment after
// typing stops). It shows unless closed (⌘⌥\ or its ×), which is remembered.
use super::*;
use crate::document::element::{self, Field, Input as FieldInput, Kind};
use crate::ui::Tip;
use gpui_kit::component::switch::Switch;
use std::collections::HashMap;
use std::time::Duration;

/// The sidebar's own state: its text fields, for the element they were filled for.
#[derive(Default)]
pub struct Inspector {
    inputs: HashMap<&'static str, Entity<InputState>>,
    /// The element the fields were last filled from: its kind and where it starts.
    shown: Option<(Kind, usize)>,
    advanced: bool,
    /// A field's change waiting for typing to stop, by field: one per field, so typing in a
    /// second field does not drop the first one's.
    pending: HashMap<&'static str, Task<()>>,
    /// What is wrong with what a field holds, by field, shown under it.
    hints: HashMap<&'static str, &'static str>,
    _subs: Vec<Subscription>,
}

/// The icon of an element's kind, beside its name.
fn kind_icon(kind: Kind) -> &'static str {
    match kind {
        Kind::Figure | Kind::Image => "icons/image.svg",
        Kind::Table => "icons/table.svg",
        Kind::Heading => "icons/heading.svg",
        Kind::Equation => "icons/sigma.svg",
        Kind::List => "icons/list.svg",
        Kind::Document => "icons/file-text.svg",
    }
}

impl Workspace {
    pub(super) fn inspector_visible(&self, cx: &App) -> bool {
        !settings::AppSettings::get(cx).inspector_closed && self.visual(cx)
    }

    /// The sidebar for the open document.
    pub(super) fn render_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let Some(doc) = self.doc().cloned() else {
            return div().into_any_element();
        };
        let (el, fields) = doc.read(cx).element_fields();
        // other element: its values into the text fields (not while one is being typed in)
        let id = (el.kind, el.range.start);
        let refill = self.inspector.shown != Some(id);
        if refill {
            self.inspector.hints.clear();
        }
        self.inspector.shown = Some(id);
        for f in fields
            .iter()
            .filter(|f| matches!(f.input, FieldInput::Text | FieldInput::Number))
        {
            let input = self.inspector_input(f.key, window, cx);
            let focused = input.focus_handle(cx).is_focused(window);
            if refill || (!focused && input.read(cx).value().as_ref() != f.value) {
                let v = f.value.clone();
                input.update(cx, |s, cx| s.set_value(v, window, cx));
            }
        }
        let advanced = self.inspector.advanced;
        let (common, more): (Vec<&Field>, Vec<&Field>) = fields.iter().partition(|f| !f.advanced);
        let mut body = div().flex().flex_col().gap_3().p_3();
        for f in common {
            body = body.child(self.inspector_field(f, cx));
        }
        if !more.is_empty() {
            body = body.child(
                Button::new("inspector-advanced")
                    .ghost()
                    .small()
                    // flush with the fields' labels
                    .self_start()
                    .px_0()
                    .icon(Icon::default().path(if advanced {
                        "icons/chevron-down.svg"
                    } else {
                        "icons/chevron-right.svg"
                    }))
                    .label("Advanced")
                    .accessibility_label(if advanced {
                        "Advanced, shown"
                    } else {
                        "Advanced, hidden"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.inspector.advanced = !this.inspector.advanced;
                        cx.notify();
                    })),
            );
            if advanced {
                for f in more {
                    body = body.child(self.inspector_field(f, cx));
                }
            }
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h_9()
                    .px_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(
                                Icon::default()
                                    .path(kind_icon(el.kind))
                                    .size(px(14.0))
                                    .text_color(theme.muted_foreground),
                            )
                            .child(el.kind.title()),
                    )
                    .child(
                        Button::new("inspector-close")
                            .icon(Icon::default().path("icons/x.svg"))
                            .ghost()
                            .xsmall()
                            .tip("Hide the sidebar (⌘⌥\\)")
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_right(cx))),
                    ),
            )
            .child(
                div()
                    .id("inspector-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            )
            .into_any_element()
    }

    /// The text field for `key`, made the first time, applying what is typed a moment after
    /// typing stops.
    fn inspector_input(
        &mut self,
        key: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(i) = self.inspector.inputs.get(key) {
            return i.clone();
        }
        let input = cx.new(|cx| InputState::new(window, cx));
        let sub = cx.subscribe_in(
            &input,
            window,
            move |this, input, ev: &InputEvent, window, cx| {
                if !matches!(ev, InputEvent::Change | InputEvent::PressEnter { .. }) {
                    return;
                }
                // only what the user typed: a refill is not focused
                if !input.focus_handle(cx).is_focused(window) {
                    return;
                }
                let value = input.read(cx).value().to_string();
                let kind_now = this.doc().map(|d| d.read(cx).element().kind);
                let hint = kind_now.and_then(|k| element::hint(k, key, &value));
                let had = match hint {
                    Some(h) => this.inspector.hints.insert(key, h).is_some(),
                    None => this.inspector.hints.remove(key).is_some(),
                };
                if hint.is_some() || had {
                    cx.notify();
                }
                if hint.is_some() {
                    this.inspector.pending.remove(key);
                    return;
                }
                // the element it was typed for (the caret may move on before it applies)
                let Some((kind, start)) = this.doc().map(|d| {
                    let el = d.read(cx).element();
                    (el.kind, el.range.start)
                }) else {
                    return;
                };
                let delay = if matches!(ev, InputEvent::PressEnter { .. }) {
                    0
                } else {
                    400
                };
                let task = cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(delay))
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.inspector.pending.remove(key);
                        this.with_doc(cx, |d, cx| d.set_field_at(kind, start, key, &value, cx));
                    });
                });
                this.inspector.pending.insert(key, task);
            },
        );
        self.inspector._subs.push(sub);
        self.inspector.inputs.insert(key, input.clone());
        input
    }

    /// One field: its label over its control.
    fn inspector_field(&self, f: &Field, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let key = f.key;
        let set = {
            let weak = cx.entity().downgrade();
            Rc::new(move |v: &'static str, _: &mut Window, cx: &mut App| {
                let _ = weak.update(cx, |this, cx| {
                    this.with_doc(cx, |d, cx| d.set_element_field(key, v, cx));
                });
            })
        };
        let control: AnyElement = match &f.input {
            FieldInput::Choice(_) if key == "align" => widgets::segmented(
                "insp-align",
                &[
                    ("left", Some("icons/text-align-start.svg"), "Left"),
                    ("center", Some("icons/text-align-center.svg"), "Center"),
                    ("right", Some("icons/text-align-end.svg"), "Right"),
                ],
                &f.value,
                set,
            ),
            // a float's placement: each letter on or off
            FieldInput::Text if key == "placement" => {
                let now = f.value.clone();
                let mut row = div().flex().gap_0p5();
                for (i, (c, tip)) in [
                    ('h', "Here"),
                    ('t', "Top of a page"),
                    ('b', "Bottom of a page"),
                    ('p', "A page of floats"),
                    ('!', "Even if it looks bad"),
                ]
                .into_iter()
                .enumerate()
                {
                    let on = now.contains(c);
                    let next: String = if on {
                        now.chars().filter(|x| *x != c).collect()
                    } else {
                        // in LaTeX's order
                        "htbp!"
                            .chars()
                            .filter(|x| *x == c || now.contains(*x))
                            .collect()
                    };
                    row = row.child(
                        Button::new(("insp-place", i))
                            .label(c.to_string())
                            .small()
                            .when(on, |b| b.primary())
                            .when(!on, |b| b.outline())
                            .tip(format!("{tip} ({c})"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.with_doc(cx, |d, cx| d.set_element_field(key, &next, cx));
                            })),
                    );
                }
                row.into_any_element()
            }
            FieldInput::Text | FieldInput::Number => match self.inspector.inputs.get(key) {
                Some(i) => Input::new(i).small().into_any_element(),
                None => div().into_any_element(),
            },
            FieldInput::Toggle => {
                let on = f.value == "true";
                return div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child(f.label))
                    .child(
                        Switch::new(SharedString::from(format!("insp-{key}")))
                            .checked(on)
                            .on_click(cx.listener(move |this, v: &bool, _, cx| {
                                let v = if *v { "true" } else { "false" };
                                this.with_doc(cx, |d, cx| d.set_element_field(key, v, cx));
                            })),
                    )
                    .into_any_element();
            }
            FieldInput::Choice(options) => {
                let label = options
                    .iter()
                    .find(|(v, _)| *v == f.value)
                    .map_or(f.value.clone(), |(_, l)| l.to_string());
                let options = options.clone();
                let now = f.value.clone();
                let weak = cx.entity().downgrade();
                Button::new(SharedString::from(format!("insp-{key}")))
                    .label(label)
                    .outline()
                    .small()
                    .w_full()
                    .dropdown_caret(true)
                    .dropdown_menu(move |mut menu, _, _| {
                        for (v, l) in &options {
                            let (weak, v) = (weak.clone(), *v);
                            menu = menu.item(PopupMenuItem::new(*l).checked(now == v).on_click(
                                move |_, _, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.with_doc(cx, |d, cx| d.set_element_field(key, v, cx));
                                    });
                                },
                            ));
                        }
                        menu
                    })
                    .into_any_element()
            }
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(f.label),
            )
            .child(control)
            .when_some(self.inspector.hints.get(key), |d, h| {
                d.child(div().text_xs().text_color(theme.danger).child(*h))
            })
            .into_any_element()
    }
}
