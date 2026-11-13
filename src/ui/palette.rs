// The editor's colours. The marks it draws on the typeset pages (the selected block, the insert
// line, the text selection, find matches, the source box) and the dark pages' paper and ink are
// the theme's, as Settings › Colors › Pages changes them; unchanged, the marks take their hue
// from the theme's roles, made to stand out from the paper they sit on, white or dark. And the
// surfaces the pages lie in.
use crate::render::{DARK, DarkPaper};
use crate::ui::settings::AppSettings;
use crate::ui::themes;
use gpui_kit::component::{ActiveTheme, Colorize, Theme, ThemeConfig};
use gpui_kit::{App, Hsla, SharedString, rgb, rgba};

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

/// Pages on dark paper: the dark theme, unless the settings keep pages white.
pub fn dark_pages(cx: &App) -> bool {
    cx.theme().mode.is_dark() && AppSettings::get(cx).dark_pages
}

/// The dark pages' paper and ink, as the renderer draws them: the theme's own, else its default.
pub fn dark_paper(cx: &App) -> DarkPaper {
    let color = |key, or| themes::page_color(key, cx).map_or(or, rgb8);
    DarkPaper {
        paper: color("pages.paper", DARK.paper),
        ink: color("pages.ink", DARK.ink),
    }
}

/// Paper, as the renderer draws it.
pub fn paper(dark: bool, cx: &App) -> Hsla {
    from_rgb8(if dark { dark_paper(cx).paper } else { [255; 3] })
}

/// Ink on that paper, as the renderer draws text.
pub fn ink(dark: bool, cx: &App) -> Hsla {
    from_rgb8(if dark { dark_paper(cx).ink } else { [0; 3] })
}

fn rgb8(c: Hsla) -> [u8; 3] {
    let c = c.to_rgb();
    [c.r, c.g, c.b].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn from_rgb8([r, g, b]: [u8; 3]) -> Hsla {
    rgb(u32::from_be_bytes([0, r, g, b])).into()
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

/// The marks for pages on `dark` paper: those the theme names in Settings › Colors › Pages, and
/// for the rest, from the theme: its selection hue, at a lightness that stands 3:1 from the paper
/// (WCAG's floor for marks and controls), and its warning hue for find, as a highlighter's.
pub fn marks(dark: bool, cx: &App) -> Marks {
    let theme = cx.theme();
    let own = |key| themes::page_color(key, cx);
    let paper = paper(dark, cx);
    let accent = own("pages.accent")
        .unwrap_or_else(|| readable_on(theme.selection.opacity(1.0), paper, 3.0));
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
        text_selection: over(own("pages.selection"), accent, selection_alpha(dark)),
        match_current: over(own("pages.find.current"), theme.warning, 0.6),
        match_other: over(own("pages.find.other"), theme.warning, 0.28),
    }
}

/// A mark over text: the page's image lies under it, its text too, so a colour chosen opaque
/// (else `base`) is drawn at the mark's `alpha`; one chosen with an alpha of its own, as chosen.
fn over(own: Option<Hsla>, base: Hsla, alpha: f32) -> Hsla {
    match own {
        Some(c) if c.a < 1.0 => c,
        c => c.unwrap_or(base).opacity(alpha),
    }
}

/// How much of the accent fills selected text: enough that the fill stands 1.5:1 from the paper
/// while the text over it still reads.
fn selection_alpha(dark: bool) -> f32 {
    if dark { 0.6 } else { 0.4 }
}

/// The source box's background and text over `dark` paper: the theme's own, else a step off the
/// paper.
pub fn source_box(dark: bool, cx: &App) -> (Hsla, Hsla) {
    let (bg, fg) = if dark {
        (0x262626, 0xe6e6e6)
    } else {
        (0xebebeb, 0x1f1f1f)
    };
    let own = |key, or| themes::page_color(key, cx).unwrap_or_else(|| rgb(or).into());
    (
        own("pages.source_box.background", bg),
        own("pages.source_box.foreground", fg),
    )
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

/// The colour "#RRGGBB" names; None for anything else (an empty accent setting among them).
pub fn hex_color(s: &str) -> Option<Hsla> {
    hex_rgba(s).filter(|_| s.len() == 7)
}

/// The colour "#RRGGBB" or "#RRGGBBAA" names; None for anything else.
pub fn hex_rgba(s: &str) -> Option<Hsla> {
    let digits = s.strip_prefix('#')?;
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(digits, 16).ok()?;
    match digits.len() {
        6 => Some(rgb(v).into()),
        8 => Some(rgba(v).into()),
        _ => None,
    }
}

/// `c` as theme.json writes it: #RRGGBB, or #RRGGBBAA when translucent.
pub fn hex(c: Hsla) -> String {
    let c = c.to_rgb();
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let (r, g, b, a) = (byte(c.r), byte(c.g), byte(c.b), byte(c.a));
    if a == 255 {
        format!("#{r:02X}{g:02X}{b:02X}")
    } else {
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    }
}

/// `base`, a theme, in `accent`, kept in the relations theme.json's own blue has: light
/// buttons in the colour, darker on hover and press; dark buttons in the colour, lighter on hover,
/// with marks and links in a lighter shade; the highlights, faint tints of the marks. Marks keep
/// 3:1 on the background, and a button's text is white or ink, whichever reads better.
pub fn accented(base: &ThemeConfig, accent: Hsla) -> ThemeConfig {
    let accent = Hsla { a: 1.0, ..accent };
    let dark = base.mode.is_dark();
    let bg = (base.colors.background.as_deref())
        .and_then(hex_color)
        .unwrap_or_else(|| rgb(if dark { 0x000000 } else { 0xffffff }).into());
    let shade = |c: Hsla, by: f32| Hsla {
        l: (c.l + by).clamp(0.0, 1.0),
        ..c
    };
    let mark = readable_on(
        if dark {
            Hsla {
                l: accent.l.max(0.76),
                ..accent
            }
        } else {
            accent
        },
        bg,
        3.0,
    );
    let (hover, active, link_hover) = if dark {
        (shade(accent, 0.05), shade(accent, -0.05), shade(mark, 0.06))
    } else {
        (shade(accent, -0.05), shade(accent, -0.1), shade(mark, -0.1))
    };
    let (tint, list_tint) = if dark { (0x1F, 0x26) } else { (0x14, 0x1F) };
    let (white, ink): (Hsla, Hsla) = (rgb(0xFFFFFF).into(), rgb(0x1F2328).into());
    let on_accent = if contrast(white, accent) >= contrast(ink, accent) {
        white
    } else {
        ink
    };
    let mut config = base.clone();
    let c = &mut config.colors;
    let set = |v: Hsla| Some(SharedString::from(hex(v)));
    let tinted = |a: u8| set(mark.opacity(f32::from(a) / 255.0));
    c.primary = set(accent);
    c.primary_hover = set(hover);
    c.primary_active = set(active);
    c.primary_foreground = set(on_accent);
    c.selection = set(accent);
    c.ring = set(mark);
    c.caret = set(mark);
    c.link = set(mark);
    c.list_active_border = set(mark);
    c.link_hover = set(link_hover);
    c.link_active = set(link_hover);
    c.accent = tinted(tint);
    c.list_active = tinted(list_tint);
    // the light sidebar's picked row is a tint of the accent (the dark one's is a plain gray)
    if !dark {
        c.sidebar_accent = tinted(0x1A);
    }
    config
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
    use super::{accented, contrast, hex, hex_color, hex_rgba, readable_on};
    use gpui_kit::component::{ThemeConfig, ThemeSet};
    use gpui_kit::{Hsla, SharedString, rgb};

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

    /// theme.json's two themes, as the registry reads them.
    fn themes() -> (ThemeConfig, ThemeConfig) {
        let set: ThemeSet = serde_json::from_str(include_str!("theme.json")).unwrap();
        let mut t = set.themes.into_iter();
        (t.next().unwrap(), t.next().unwrap())
    }

    fn color(c: &Option<SharedString>) -> Hsla {
        hex_color(c.as_deref().unwrap()).unwrap()
    }

    #[test]
    fn an_accent_setting_reads_as_a_colour_or_none() {
        assert_eq!(hex_color("#3557d4").map(hex).as_deref(), Some("#3557D4"));
        for bad in [
            "",
            "3557D4",
            "#3557D",
            "#3557D4FF",
            "#zzzzzz",
            "#aébbb",
            " #3557D4",
        ] {
            assert_eq!(hex_color(bad), None, "{bad:?}");
        }
        assert_eq!(hex(rgb(0x3557D4).into()), "#3557D4");
        // a theme colour may be translucent
        assert_eq!(hex_rgba("#3557D414").map(hex).as_deref(), Some("#3557D414"));
        assert_eq!(hex_rgba("#3557d4").map(hex).as_deref(), Some("#3557D4"));
        for bad in ["", "#3557D", "#3557D41", "#+557D4", "3557D414"] {
            assert_eq!(hex_rgba(bad), None, "{bad:?}");
        }
        let tint = Hsla::from(rgb(0x3557D4)).opacity(20.0 / 255.0);
        assert_eq!(hex(tint), "#3557D414");
    }

    #[test]
    fn an_accent_keeps_the_themes_relations() {
        let (light, dark) = themes();
        assert!(!light.mode.is_dark() && dark.mode.is_dark());
        let pink = hex_color("#C93A85").unwrap();
        let l = accented(&light, pink).colors;
        assert_eq!(hex(color(&l.primary)), "#C93A85");
        assert_eq!(hex(color(&l.ring)), "#C93A85");
        assert_eq!(hex(color(&l.caret)), "#C93A85");
        assert_eq!(hex(color(&l.primary_foreground)), "#FFFFFF");
        assert!(
            color(&l.primary_hover).l < pink.l
                && color(&l.primary_active).l < color(&l.primary_hover).l
        );
        assert_eq!(l.accent.as_deref(), Some("#C93A8514"));
        assert_eq!(l.list_active.as_deref(), Some("#C93A851F"));
        let d = accented(&dark, pink).colors;
        assert_eq!(hex(color(&d.primary)), "#C93A85");
        assert!(color(&d.primary_hover).l > pink.l);
        // marks and links in a lighter shade of the same hue
        let mark = color(&d.ring);
        assert!(mark.l > pink.l + 0.15 && (mark.h - pink.h).abs() < 0.01);
        assert_eq!(color(&d.link), mark);
        assert!(color(&d.link_hover).l > mark.l);
        // what the theme does not tint stays as theme.json has it
        assert_eq!(d.background, dark.colors.background);
        assert_eq!(d.foreground, dark.colors.foreground);
    }

    #[test]
    fn a_light_accent_gets_dark_button_text_and_readable_marks() {
        let (light, dark) = themes();
        let yellow = hex_color("#FFE066").unwrap();
        let bg = |t: &ThemeConfig| color(&t.colors.background);
        for t in [&light, &dark] {
            let c = accented(t, yellow).colors;
            assert_eq!(hex(color(&c.primary_foreground)), "#1F2328");
            assert!(contrast(color(&c.ring), bg(t)) >= 3.0, "{}", t.name);
        }
    }

    #[test]
    fn every_offered_accent_keeps_white_text_on_its_buttons() {
        let (light, dark) = themes();
        for (_, v) in crate::ui::settings::ACCENTS {
            let a = hex_color(v)
                .or(hex_color(crate::ui::settings::THEME_BLUE))
                .unwrap();
            assert!(contrast(a, rgb(0xFFFFFF).into()) >= 4.5, "{v}");
            for t in [&light, &dark] {
                let c = accented(t, a).colors;
                assert_eq!(hex(color(&c.primary_foreground)), "#FFFFFF", "{v}");
                assert!(
                    contrast(color(&c.ring), color(&t.colors.background)) >= 3.0,
                    "{v}"
                );
            }
        }
    }
}
