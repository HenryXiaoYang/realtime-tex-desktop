// The sidebar's page setup, shown with the document's other settings when nothing is selected: the
// paper with its margins drawn to scale, and the packages the preamble loads, to remove or add.
// Custom macros stay in the preamble's source.
use super::*;
use crate::document::element::Field;
use crate::document::preamble::{self, PageShape};
use crate::ui::Tip;

/// The box the paper is drawn in (px).
const DRAW_W: f32 = 132.0;
const DRAW_H: f32 = 150.0;

/// The margin fields the page setup shows itself, so the sidebar's field list leaves them out.
pub(super) const MARGIN_KEYS: [&str; 5] = ["margin", "top", "bottom", "left", "right"];

impl Workspace {
    /// The paper drawn to scale with its text area, then the margin fields.
    pub(super) fn page_setup(&self, fields: &[Field], cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let PageShape {
            width,
            height,
            margins: [top, bottom, left, right],
        } = preamble::page_shape(fields);
        let scale = (DRAW_W / width).min(DRAW_H / height);
        let mm = |v: f32| px(v * scale);
        let field = |key: &str| fields.iter().find(|f| f.key == key);
        let cell = |key: &str, cx: &mut Context<Self>| {
            div()
                .flex_1()
                .min_w_0()
                .children(field(key).map(|f| self.inspector_field(f, cx)))
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Margins"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .id("page-shape")
                            .relative()
                            .w(mm(width))
                            .h(mm(height))
                            .bg(gpui::white())
                            .border_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .absolute()
                                    .left(mm(left))
                                    .right(mm(right))
                                    .top(mm(top))
                                    .bottom(mm(bottom))
                                    .border_1()
                                    .border_dashed()
                                    .border_color(theme.muted_foreground)
                                    .bg(theme.muted.opacity(0.4)),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("{width:.0} × {height:.0} mm")),
                    ),
            )
            .children(field("margin").map(|f| self.inspector_field(f, cx)))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(cell("top", cx))
                    .child(cell("bottom", cx)),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(cell("left", cx))
                    .child(cell("right", cx)),
            )
            .into_any_element()
    }

    /// The text field that adds a package on Enter, made the first time.
    fn package_input(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
        if let Some(i) = &self.inspector.new_package {
            return i.clone();
        }
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Add a package, then Enter"));
        let sub = cx.subscribe_in(
            &input,
            window,
            |this, input, ev: &InputEvent, window, cx| match ev {
                InputEvent::Change => cx.notify(),
                InputEvent::PressEnter { .. } => {
                    let name = input.read(cx).value().trim().to_string();
                    if !preamble::is_package_name(&name) {
                        return;
                    }
                    this.with_doc(cx, |d, cx| d.add_package(&name, cx));
                    input.update(cx, |s, cx| s.set_value(String::new(), window, cx));
                }
                _ => {}
            },
        );
        self.inspector._subs.push(sub);
        self.inspector.new_package = Some(input.clone());
        input
    }

    /// The packages the preamble loads, each with a button to remove it, and the field to add one.
    pub(super) fn package_list(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let names = self
            .doc()
            .map(|d| d.read(cx).packages())
            .unwrap_or_default();
        let input = self.package_input(window, cx);
        let typed = input.read(cx).value().trim().to_string();
        let bad = !typed.is_empty() && !preamble::is_package_name(&typed);
        let rows = names.into_iter().enumerate().map(|(i, name)| {
            let tip = format!("Stop loading {name}");
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(div().text_sm().truncate().child(name.clone()))
                .child(
                    Button::new(("insp-package-remove", i))
                        .icon(Icon::default().path("icons/x.svg"))
                        .ghost()
                        .xsmall()
                        .tip(tip)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.with_doc(cx, |d, cx| d.remove_package(&name, cx));
                        })),
                )
        });
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Packages"),
            )
            .children(rows)
            .child(Input::new(&input).small())
            .when(bad, |d| {
                d.child(
                    div()
                        .text_xs()
                        .text_color(theme.danger)
                        .child("Letters, digits, - and _ only"),
                )
            })
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Custom macros stay in the preamble: click the Preamble strip above the first page to edit its source."),
            )
            .into_any_element()
    }
}
