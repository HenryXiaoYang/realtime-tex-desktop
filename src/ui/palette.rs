// The editor's colours. The marks it draws on the typeset pages (the selected block, the insert
// line, the text selection, find matches, the source box) take their hue from the theme's roles,
// made to stand out from the paper they sit on, white or dark; and the surfaces the pages lie in.
use gpui_kit::component::{ActiveTheme, Colorize, Theme};
use gpui_kit::{App, Hsla, Rgba, rgb};

/// The desk the pages lie on, from the theme: its muted surface; in a dark theme, a third of
/// the way from its background to that (near-black, between the panel and a black page).
pub fn desk(theme: &Theme) -> Hsla {
    desk_of(theme.mode.is_dark(), theme.background, theme.muted)
}

/// The project panel, from the theme: its sidebar, unless a dark theme's sidebar is as dark as
/// the desk (the default's is its background); then its muted surface, set off from the pages.
pub fn panel(theme: &Theme) -> Hsla {
    panel_of(theme.sidebar, theme.muted, desk(theme))
}

fn desk_of(dark: bool, background: Hsla, muted: Hsla) -> Hsla {
    if dark {
        background.mix(muted, 0.65)
    } else {
        muted
    }
}

fn panel_of(sidebar: Hsla, muted: Hsla, desk: Hsla) -> Hsla {
    if luminance(sidebar) < 0.5 && contrast(sidebar, desk) < 1.2 {
        muted
    } else {
        sidebar
    }
}

/// The tools' colours, each (light theme, dark theme) readable 3:1 on the ribbon in its theme.
const AMBER: (u32, u32) = (0xB25E00, 0xF2A93B);
const BLUE: (u32, u32) = (0x3557D4, 0x86A1FF);
const GREEN: (u32, u32) = (0x1B8250, 0x55C98F);
const TEAL: (u32, u32) = (0x0B7A8C, 0x43C5D6);
const VIOLET: (u32, u32) = (0x6A45D9, 0xAE95FF);
const SLATE: (u32, u32) = (0x55627E, 0xA3AECA);

/// An icon's colour by what its tool does (pasting warm, lists and links blue, tables green,
/// pictures teal, formulas violet), as an office suite's ribbon colours its tools; None: the
/// text's own (bold, italic, alignment and the like stay ink).
pub fn icon_tint(icon: &str, cx: &App) -> Option<Hsla> {
    let name = icon.trim_start_matches("icons/").trim_end_matches(".svg");
    let (light, dark) = match name {
        "clipboard-paste" | "bookmark" | "quote" => AMBER,
        "scissors" | "copy" | "separator-horizontal" | "move-vertical" => SLATE,
        "list"
        | "list-ordered"
        | "list-indent-increase"
        | "list-indent-decrease"
        | "list-tree"
        | "link"
        | "file-text" => BLUE,
        "table" => GREEN,
        "image" | "file-image" => TEAL,
        "radical" | "sigma" => VIOLET,
        _ => return None,
    };
    Some(
        rgb(if cx.theme().mode.is_dark() {
            dark
        } else {
            light
        })
        .into(),
    )
}

/// Paper, as the renderer draws it (render::DARK_PAPER when dark).
pub fn paper(dark: bool) -> Rgba {
    rgb(if dark { 0x000000 } else { 0xffffff })
}

/// Ink on that paper, as the renderer draws text.
pub fn ink(dark: bool) -> Rgba {
    rgb(if dark { 0xe0e0e0 } else { 0x000000 })
}

/// What the editor draws over the pages.
pub struct Marks {
    /// The selected block's outline, the insert line, the source box's frame.
    pub accent: Hsla,
    /// An icon on the accent.
    pub on_accent: Hsla,
    /// Under selected text.
    pub text_selection: Hsla,
    /// Under the find match at hand, and under the others.
    pub match_current: Hsla,
    pub match_other: Hsla,
}

/// The marks for pages on `dark` paper, from the theme: its selection hue, at a lightness that
/// stands 3:1 from the paper (WCAG's floor for marks and controls), and its warning hue for
/// find, as a highlighter's.
pub fn marks(dark: bool, cx: &App) -> Marks {
    let theme = cx.theme();
    let paper: Hsla = paper(dark).into();
    let accent = readable_on(theme.selection.opacity(1.0), paper, 3.0);
    let white: Hsla = rgb(0xffffff).into();
    let black: Hsla = rgb(0x000000).into();
    Marks {
        accent,
        on_accent: if contrast(white, accent) >= contrast(black, accent) {
            white
        } else {
            black
        },
        // the accent, strong enough to stand out from the paper (dark paper needs more of it)
        text_selection: accent.opacity(selection_alpha(dark)),
        match_current: theme.warning.opacity(0.6),
        match_other: theme.warning.opacity(0.28),
    }
}

/// How much of the accent fills selected text: enough that the fill stands 1.5:1 from the paper
/// while the text over it still reads.
fn selection_alpha(dark: bool) -> f32 {
    if dark { 0.6 } else { 0.4 }
}

/// The source box's background and text over `dark` paper: a step off the paper.
pub fn source_box(dark: bool) -> (Rgba, Rgba) {
    if dark {
        (rgb(0x262626), rgb(0xe6e6e6))
    } else {
        (rgb(0xebebeb), rgb(0x1f1f1f))
    }
}

/// WCAG relative luminance.
fn luminance(c: Hsla) -> f32 {
    let c = c.to_rgb();
    let f = |v: f32| {
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
}

/// The WCAG contrast ratio of two colours.
fn contrast(a: Hsla, b: Hsla) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// `c` darkened (on light `bg`) or lightened (on dark) until it stands `ratio` from `bg`.
fn readable_on(c: Hsla, bg: Hsla, ratio: f32) -> Hsla {
    let darken = luminance(bg) > 0.5;
    let mut c = c;
    while contrast(c, bg) < ratio && (darken && c.l > 0.0 || !darken && c.l < 1.0) {
        c.l = (c.l + if darken { -0.02 } else { 0.02 }).clamp(0.0, 1.0);
    }
    c
}

#[cfg(test)]
mod surface_tests {
    use super::*;

    #[test]
    fn the_tools_colours_read_on_the_ribbon() {
        // the theme's ribbon (title bar), light and dark
        let (light, dark) = (hex(0xF3F4F7), hex(0x26282D));
        for (l, d) in [AMBER, BLUE, GREEN, TEAL, VIOLET, SLATE] {
            assert!(contrast(hex(l), light) >= 3.0, "{l:06X} on light");
            assert!(contrast(hex(d), dark) >= 3.0, "{d:06X} on dark");
        }
    }

    fn hex(c: u32) -> Hsla {
        rgb(c).into()
    }

    #[test]
    fn the_desk_and_panel_follow_the_theme() {
        // the kit's default dark theme: background, muted, sidebar (as dark as the background)
        let (bg, muted, sidebar, fg) = (hex(0x0a0a0a), hex(0x262626), hex(0x0a0a0a), hex(0xfafafa));
        let desk = desk_of(true, bg, muted);
        let panel = panel_of(sidebar, muted, desk);
        let page = hex(0x000000);
        // a black page, a near-black desk, a gray panel, each set off from the next
        assert!(luminance(page) < luminance(desk) && luminance(desk) < luminance(panel));
        assert!(
            contrast(panel, desk) >= 1.2,
            "the panel stands off the desk"
        );
        assert!(contrast(fg, panel) >= 4.5, "the panel's text reads");
        // a theme's own sidebar, set apart already, is kept; so is a light theme's
        assert_eq!(panel_of(hex(0x1e2a3a), muted, desk), hex(0x1e2a3a));
        assert_eq!(
            panel_of(hex(0xfafafa), hex(0xf5f5f5), hex(0xf5f5f5)),
            hex(0xfafafa)
        );
        assert_eq!(desk_of(false, hex(0xffffff), hex(0xf5f5f5)), hex(0xf5f5f5));
    }
}

#[cfg(test)]
mod tests {
    use super::{contrast, readable_on};
    use gpui_kit::{Hsla, rgb};

    #[test]
    fn marks_read_on_either_paper() {
        let (white, black): (Hsla, Hsla) = (rgb(0xffffff).into(), rgb(0x000000).into());
        assert!((contrast(white, black) - 21.0).abs() < 0.1);
        // the light theme's selection blue is too pale on white; the dark theme's too deep on black
        for c in [0x55a0fc, 0x1d4ed8, 0xfde047] {
            let c: Hsla = rgb(c).into();
            assert!(contrast(readable_on(c, white, 3.0), white) >= 3.0);
            assert!(contrast(readable_on(c, black, 3.0), black) >= 3.0);
        }
        // a selection fill shows on either paper: the accent blended over it
        let blend = |c: Hsla, a: f32, bg: Hsla| {
            let (c, bg) = (c.to_rgb(), bg.to_rgb());
            let mix = |x: f32, y: f32| x * a + y * (1.0 - a);
            let r = gpui_kit::Rgba {
                r: mix(c.r, bg.r),
                g: mix(c.g, bg.g),
                b: mix(c.b, bg.b),
                a: 1.0,
            };
            Hsla::from(r)
        };
        for (sel, paper, dark) in [(0x55a0fc, white, false), (0x1d4ed8, black, true)] {
            let accent = readable_on(rgb(sel).into(), paper, 3.0);
            let fill = blend(accent, super::selection_alpha(dark), paper);
            assert!(
                contrast(fill, paper) >= 1.5,
                "{dark}: {}",
                contrast(fill, paper)
            );
        }
        // a colour that already reads stays as it is
        let ok: Hsla = rgb(0x1d4ed8).into();
        assert_eq!(readable_on(ok, white, 3.0), ok);
    }
}
