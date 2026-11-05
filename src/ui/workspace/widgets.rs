// Small pieces of the ribbon and the sidebar: xcolor's colours as swatches, the grid that picks a
// table's size, and a segmented choice.
use super::*;
use gpui_kit::component::popover::Popover;

/// What a widget calls with the value picked.
pub(super) type OnPick<T> = Rc<dyn Fn(T, &mut Window, &mut App)>;

/// xcolor's base colours: name, label, and RGB as xcolor defines them.
pub(super) const XCOLORS: &[(&str, &str, u32)] = &[
    ("black", "Black", 0x000000),
    ("darkgray", "Dark gray", 0x404040),
    ("gray", "Gray", 0x808080),
    ("lightgray", "Light gray", 0xbfbfbf),
    ("white", "White", 0xffffff),
    ("brown", "Brown", 0xbf8040),
    ("red", "Red", 0xff0000),
    ("orange", "Orange", 0xff8000),
    ("yellow", "Yellow", 0xffff00),
    ("lime", "Lime", 0xbfff00),
    ("olive", "Olive", 0x808000),
    ("green", "Green", 0x00ff00),
    ("teal", "Teal", 0x008080),
    ("cyan", "Cyan", 0x00ffff),
    ("blue", "Blue", 0x0000ff),
    ("violet", "Violet", 0x800080),
    ("purple", "Purple", 0xbf0040),
    ("magenta", "Magenta", 0xff00ff),
    ("pink", "Pink", 0xffbfbf),
];

/// A colour as the TeX names it: an xcolor name when it is one, else HTML hex.
pub(super) fn color_spec(c: Hsla) -> String {
    let rgb = c.to_rgb();
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    let hex = (byte(rgb.r) << 16) | (byte(rgb.g) << 8) | byte(rgb.b);
    match XCOLORS.iter().find(|(_, _, x)| *x == hex) {
        Some((name, ..)) => name.to_string(),
        None => format!("{hex:06X}"),
    }
}

/// The colour a TeX spec names (an xcolor name, or HTML hex), to show it.
pub(super) fn color_of(spec: &str) -> Option<Hsla> {
    if let Some((_, _, x)) = XCOLORS.iter().find(|(n, ..)| *n == spec) {
        return Some(rgb(*x).into());
    }
    u32::from_str_radix(spec, 16)
        .ok()
        .filter(|_| spec.len() == 6)
        .map(|x| rgb(x).into())
}

/// A big ribbon button's face: its icon over its name, with a caret when it opens a menu.
pub(super) fn big_face(icon: &'static str, label: &'static str, caret: bool, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .px_1p5()
        .pt_1()
        // one height for all, menu or not, so their names line up
        .h(px(50.0))
        .line_height(px(16.0))
        .min_w(px(48.0))
        .rounded_md()
        .text_xs()
        .cursor_pointer()
        .hover(|d| d.bg(theme.secondary))
        .child(Icon::default().path(icon).size(px(20.0)))
        .child(
            div()
                .flex()
                .items_center()
                .gap_0p5()
                .child(label)
                .when(caret, |d| {
                    d.child(
                        Icon::default()
                            .path("icons/chevron-down.svg")
                            .size(px(10.0)),
                    )
                }),
        )
}

/// A swatch: a square of colour `c`, ringed when `on`.
fn swatch(id: impl Into<ElementId>, c: Hsla, on: bool, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    div()
        .id(id)
        .size(px(18.0))
        .rounded_sm()
        .bg(c)
        .border_1()
        .border_color(if on { theme.primary } else { theme.border })
        .when(on, |d| d.shadow_sm())
        .cursor_pointer()
}

/// The text-colour palette: xcolor's named colours (which read well in the TeX), shades of
/// each hue, and any colour by its hex code (`hex`, a field the caller keeps).
pub(super) fn color_palette(
    now: Option<String>,
    hex: Entity<InputState>,
    on_pick: OnPick<String>,
) -> Popover {
    let shown = now.as_deref().and_then(color_of);
    Popover::new("rb-color-palette")
        .trigger(
            Button::new("rb-color")
                .ghost()
                .small()
                .dropdown_caret(true)
                .tooltip("Text colour")
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .child(Icon::default().path("icons/baseline.svg").size(px(14.0)))
                        .child(
                            div()
                                .w(px(14.0))
                                .h(px(3.0))
                                .rounded_sm()
                                .bg(shown.unwrap_or(hsla(0.0, 0.0, 0.0, 1.0))),
                        ),
                ),
        )
        .content(move |_, _, cx| {
            let popover = cx.entity();
            let pick = |spec: String| {
                let (on_pick, popover) = (on_pick.clone(), popover.clone());
                move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                    on_pick(spec.clone(), window, cx);
                    popover.update(cx, |p, cx| p.dismiss(window, cx));
                }
            };
            let mut named = div().flex().flex_wrap().gap_1().w(px(10.0 * 22.0));
            for (i, (name, label, x)) in XCOLORS.iter().enumerate() {
                let on = now.as_deref() == Some(*name);
                named = named.child(
                    swatch(("xc", i), rgb(*x).into(), on, cx)
                        .tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(*label).build(window, cx)
                        })
                        .on_click(pick(name.to_string())),
                );
            }
            // ten hues, from light to dark
            let mut shades = div().flex().flex_col().gap_1();
            for l in [0.85, 0.7, 0.5, 0.35, 0.2] {
                let mut row = div().flex().gap_1();
                for h in 0..10 {
                    let c = hsla(h as f32 / 10.0, 0.8, l, 1.0);
                    let spec = color_spec(c);
                    let on = now.as_deref() == Some(spec.as_str());
                    row = row.child(
                        swatch(("sh", (l * 100.0) as usize * 16 + h), c, on, cx)
                            .on_click(pick(spec)),
                    );
                }
                shades = shades.child(row);
            }
            let theme = cx.theme().clone();
            let label =
                |t: &'static str| div().text_xs().text_color(theme.muted_foreground).child(t);
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_1()
                .child(label("Named colours"))
                .child(named)
                .child(label("More colours"))
                .child(shades)
                .child(label("Hex (Enter to apply)"))
                .child(Input::new(&hex).small())
        })
}

/// The grid that sizes a new table: hover over it to choose rows × columns, click to insert.
pub(super) fn table_grid(face: Div, on_pick: OnPick<(usize, usize)>) -> Popover {
    const ROWS: usize = 8;
    const COLS: usize = 8;
    let hover = Rc::new(std::cell::Cell::new((0usize, 0usize)));
    Popover::new("ins-table-grid")
        .trigger(Button::new("ins-table").ghost().h_auto().p_0().child(face))
        .content(move |_, _, cx| {
            let theme = cx.theme().clone();
            let (hr, hc) = hover.get();
            let popover = cx.entity();
            let mut grid = div().flex().flex_col().gap_0p5();
            for r in 1..=ROWS {
                let mut row = div().flex().gap_0p5();
                for c in 1..=COLS {
                    let on = r <= hr && c <= hc;
                    let (hover, popover, on_pick) =
                        (hover.clone(), popover.clone(), on_pick.clone());
                    let popover2 = popover.clone();
                    row = row.child(
                        div()
                            .id(("cell", r * 16 + c))
                            .size(px(16.0))
                            .rounded_sm()
                            .border_1()
                            .border_color(if on { theme.primary } else { theme.border })
                            .bg(if on {
                                theme.primary.opacity(0.25)
                            } else {
                                theme.background
                            })
                            .on_hover(move |entered, _, cx| {
                                if *entered && hover.get() != (r, c) {
                                    hover.set((r, c));
                                    popover.update(cx, |_, cx| cx.notify());
                                }
                            })
                            .on_click(move |_, window, cx| {
                                on_pick((r, c), window, cx);
                                popover2.update(cx, |p, cx| p.dismiss(window, cx));
                            }),
                    );
                }
                grid = grid.child(row);
            }
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_1()
                .child(div().text_sm().child(if hr == 0 {
                    "Insert a table".to_string()
                } else {
                    format!("{hr} × {hc} table")
                }))
                .child(grid)
        })
}

/// A choice among a few options as one row of buttons (an icon or a label each), the current
/// one pressed.
pub(super) fn segmented(
    id: &'static str,
    options: &[(&'static str, Option<&'static str>, &'static str)],
    now: &str,
    on_pick: OnPick<&'static str>,
) -> AnyElement {
    let mut row = div().flex().gap_0p5();
    for (i, (value, icon, label)) in options.iter().copied().enumerate() {
        let on_pick = on_pick.clone();
        let mut b = Button::new((id, i))
            .small()
            .selected(value == now)
            .tooltip(label)
            .on_click(move |_, window, cx| on_pick(value, window, cx));
        b = match icon {
            Some(i) => b.icon(Icon::default().path(i)).ghost(),
            None => b.label(label).ghost(),
        };
        row = row.child(b);
    }
    row.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{color_of, color_spec};

    #[test]
    fn colours_name_themselves() {
        assert_eq!(color_spec(color_of("red").unwrap()), "red");
        assert_eq!(color_spec(color_of("teal").unwrap()), "teal");
        assert_eq!(color_spec(color_of("1A2B3C").unwrap()), "1A2B3C");
        assert_eq!(color_of("nope"), None);
    }
}
