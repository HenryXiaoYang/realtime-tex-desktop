// Rasterizing rtex display lists (realtime-tex docs/DISPLAY_LIST.md, "Rendering rules"); a port of
// realtime-tex-vsc-plugin webview/render.ts onto skrifa outlines and tiny-skia.
mod resources;
pub mod viewer;

pub use resources::*;

use crate::engine::{DisplayList, FontDesc, ImageInfo, kpsewhich};
use serde_json::Value;
use skrifa::font::Kind;
use skrifa::{DrawMemory, Font, GlyphId, PathSink, ScalerFont};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, PixmapPaint, Rect, Transform};

/// Affine map [a, b, c, d, e, f]: x' = a·x + c·y + e, y' = b·x + d·y + f.
type Mat = [f64; 6];
const IDENTITY: Mat = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn mul(m: Mat, n: Mat) -> Mat {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}

fn transform(m: Mat) -> Transform {
    Transform::from_row(
        m[0] as f32,
        m[1] as f32,
        m[2] as f32,
        m[3] as f32,
        m[4] as f32,
        m[5] as f32,
    )
}

/// One display-list line drawn translated by (dx, dy) sp.
#[derive(Clone)]
pub struct Row {
    pub dl: Arc<DisplayList>,
    pub line: usize,
    pub dx: f64,
    pub dy: f64,
}

/// What a page (or a crop of one) is made of.
pub struct Job {
    pub rows: Vec<Row>,
    /// Items outside lines (page number, headers), with their display list.
    pub other: Option<Arc<DisplayList>>,
    /// The region drawn, in sp: x, y, w, h.
    pub region: [f64; 4],
    /// Device pixels per sp.
    pub scale: f64,
    /// Draw this PDF page (from 0) first, for pages the display list cannot draw; then whiten
    /// `clear` (sp boxes of lines replaced by live rows) and draw `rows` over it.
    pub pdf: Option<(PathBuf, usize)>,
    pub clear: Vec<[f64; 4]>,
    /// Dark paper with light ink, for the dark theme; included graphics keep their colours.
    /// None: white paper, black ink.
    pub dark: Option<DarkPaper>,
}

/// Dark pages' paper, and the ink black text turns into on it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DarkPaper {
    pub paper: [u8; 3],
    pub ink: [u8; 3],
}

/// The dark pages unless the settings name others.
pub const DARK: DarkPaper = DarkPaper {
    paper: [0, 0, 0],
    ink: [224, 224, 224],
};

/// The ink for a colour on dark paper: black and near-black text turn light, colours stay.
fn on_paper(c: [u8; 3], dark: Option<DarkPaper>) -> [u8; 3] {
    match dark {
        Some(d) if c.iter().all(|&v| v < 40) => d.ink,
        _ => c,
    }
}

/// RGBA pixels of a white page turned onto dark paper: white to the paper, black to the ink,
/// each channel in between on the line from one to the other.
fn onto_dark(px: &mut [u8], d: DarkPaper) {
    for px in px.as_chunks_mut::<4>().0 {
        for (c, (p, i)) in px[..3].iter_mut().zip(d.paper.into_iter().zip(d.ink)) {
            let (p, i) = (i32::from(p), i32::from(i));
            *c = (p + (i - p) * (255 - i32::from(*c)) / 255) as u8;
        }
    }
}

/// Rasterize `job` on its paper: (width, height, BGRA pixels).
pub fn render(fonts: &mut Resources, job: &Job) -> (u32, u32, Vec<u8>) {
    let [x0, y0, w, h] = job.region;
    let (pw, ph) = (
        ((w * job.scale).ceil() as u32).max(1),
        ((h * job.scale).ceil() as u32).max(1),
    );
    let mut pix = Pixmap::new(pw, ph).unwrap();
    let paper = job.dark.map_or([255, 255, 255], |d| d.paper);
    pix.fill(tiny_skia::Color::from_rgba8(
        paper[0], paper[1], paper[2], 255,
    ));
    let base = [
        job.scale,
        0.0,
        0.0,
        job.scale,
        -x0 * job.scale,
        -y0 * job.scale,
    ];
    // the PDF can be caught mid-write by the next pass; a parser panic then falls back to the
    // display list instead of leaving the page blank
    let from_pdf = job.pdf.as_ref().and_then(|(f, n)| {
        std::panic::catch_unwind(|| pdf_page(f, *n, pw, ph, true))
            .ok()
            .flatten()
    });
    if let Some(pdf) = &from_pdf {
        pix.draw_pixmap(
            0,
            0,
            pdf.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
        if let Some(d) = job.dark {
            // shortcut: a PDF-drawn page (TikZ) is inverted onto dark paper, so its colours flip
            // hue too; draw such pages from their display list when that can render pictures
            onto_dark(pix.data_mut(), d);
        }
        let mut white = Paint::default();
        white.set_color_rgba8(paper[0], paper[1], paper[2], 255);
        for b in &job.clear {
            let pt = 65536.0;
            let r = Rect::from_ltrb(
                ((b[0] - x0 - pt) * job.scale) as f32,
                ((b[1] - y0 - pt) * job.scale) as f32,
                ((b[2] - x0 + pt) * job.scale) as f32,
                ((b[3] - y0 + pt) * job.scale) as f32,
            );
            if let Some(r) = r {
                pix.fill_rect(r, &white, Transform::identity(), None);
            }
        }
    }
    let mut p = Painter {
        fonts,
        pix: &mut pix,
        base,
        cur: IDENTITY,
        mstack: vec![],
        stacks: HashMap::new(),
        color: on_paper([0, 0, 0], job.dark),
        dark: job.dark,
    };
    if let Some(dl) = job.other.as_ref().filter(|_| from_pdf.is_none()) {
        p.draw(dl, &dl.other, 0.0, 0.0);
    }
    for r in &job.rows {
        p.draw(&r.dl, &r.dl.lines[r.line].items, r.dx, r.dy);
    }
    let mut data = pix.take();
    for px in data.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    (pw, ph, data)
}

struct Painter<'a> {
    fonts: &'a mut Resources,
    pix: &'a mut Pixmap,
    base: Mat,
    cur: Mat,
    mstack: Vec<Mat>,
    stacks: HashMap<i64, Vec<String>>,
    color: [u8; 3],
    dark: Option<DarkPaper>,
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

impl Painter<'_> {
    fn paint(&self) -> Paint<'static> {
        let mut paint = Paint::default();
        paint.set_color_rgba8(self.color[0], self.color[1], self.color[2], 255);
        paint.anti_alias = true;
        paint
    }

    fn draw(&mut self, dl: &DisplayList, items: &[Value], dx: f64, dy: f64) {
        for it in items {
            let Some(a) = it.as_array() else { continue };
            match a.first().and_then(Value::as_str) {
                Some("g") if a.len() >= 6 => {
                    let Some(desc) = a[1].as_i64().and_then(|id| dl.fonts.get(&id.to_string()))
                    else {
                        continue;
                    };
                    let size = desc.size.unwrap_or(655360.0);
                    let char = num(&a[2]) as u32;
                    let index = a[3].as_u64().map(|i| i as u32);
                    let ef = a.get(7).map(num).unwrap_or(0.0);
                    let (desc_slant, desc_extend) =
                        (desc.slant.unwrap_or(0.0), desc.extend.unwrap_or(0.0));
                    let paint = self.paint();
                    let m0 = mul(self.base, self.cur);
                    let Some(f) = self.fonts.load(desc) else {
                        continue;
                    };
                    let s = size / f.upem;
                    let mut hx = (1.0 + ef / 1_000_000.0)
                        * if desc_extend != 0.0 {
                            desc_extend / 1000.0
                        } else {
                            1.0
                        };
                    let mut slant = desc_slant / 1000.0;
                    if f.type1 {
                        hx *= f.extend;
                        if slant == 0.0 {
                            slant = f.slant;
                        }
                    }
                    let m = mul(
                        m0,
                        [s * hx, 0.0, s * slant, -s, num(&a[4]) + dx, num(&a[5]) + dy],
                    );
                    if let Some(path) = f.path(char, index) {
                        self.pix
                            .fill_path(path, &paint, FillRule::Winding, transform(m), None);
                    }
                }
                Some("r") if a.len() >= 5 => {
                    let m = mul(self.base, self.cur);
                    // keep hairlines visible: at least 0.75 device px
                    let min_w = 0.75 / m[0].hypot(m[1]);
                    let min_h = 0.75 / m[2].hypot(m[3]);
                    let (x, y, w, h) = (
                        num(&a[1]) + dx,
                        num(&a[2]) + dy,
                        num(&a[3]).max(min_w),
                        num(&a[4]).max(min_h),
                    );
                    // device-space rect for axis-aligned transforms keeps f32 precision
                    let t = |px: f64, py: f64| {
                        (m[0] * px + m[2] * py + m[4], m[1] * px + m[3] * py + m[5])
                    };
                    let (ax, ay) = t(x, y);
                    let (bx, by) = t(x + w, y + h);
                    if let Some(r) = Rect::from_ltrb(
                        ax.min(bx) as f32,
                        ay.min(by) as f32,
                        ax.max(bx) as f32,
                        ay.max(by) as f32,
                    ) {
                        self.pix
                            .fill_rect(r, &self.paint(), Transform::identity(), None);
                    }
                }
                Some("i") if a.len() >= 6 => {
                    let m = mul(self.base, self.cur);
                    let (x, y) = (num(&a[2]) + dx, num(&a[3]) + dy);
                    let (ax, ay, aw, ah) = (
                        m[0] * x + m[4],
                        m[3] * y + m[5],
                        num(&a[4]) * m[0],
                        num(&a[5]) * m[3],
                    );
                    let (iw, ih) = (aw.round().max(1.0) as u32, ah.round().max(1.0) as u32);
                    let img = image_info(dl, a[1].as_i64().unwrap_or(0))
                        .filter(|i| !i.cached_picture)
                        .and_then(|i| self.fonts.image(i, iw, ih));
                    if let Some(img) = img {
                        let t = Transform::from_row(
                            (aw / img.width() as f64) as f32,
                            0.0,
                            0.0,
                            (ah / img.height() as f64) as f32,
                            ax as f32,
                            ay as f32,
                        );
                        let paint = PixmapPaint {
                            quality: tiny_skia::FilterQuality::Bilinear,
                            ..Default::default()
                        };
                        self.pix.draw_pixmap(0, 0, img.as_ref(), &paint, t, None);
                    } else if let Some(r) =
                        Rect::from_xywh(ax as f32, ay as f32, aw as f32, ah as f32)
                    {
                        // missing or unreadable file: a grey box where it goes
                        let mut paint = Paint::default();
                        paint.set_color_rgba8(128, 128, 128, 40);
                        self.pix.fill_rect(r, &paint, Transform::identity(), None);
                    }
                }
                Some("c") if a.len() >= 4 => {
                    let data = a[3].as_str().unwrap_or("").to_string();
                    let st = self.stacks.entry(a[1].as_i64().unwrap_or(0)).or_default();
                    // LuaTeX's colorstack command: 0 set, 1 push, 2 pop, 3 current; when absent, a
                    // non-empty string is a push and an empty one a pop
                    let op = a[2]
                        .as_i64()
                        .filter(|c| *c <= 3)
                        .unwrap_or(if data.trim().is_empty() { 2 } else { 1 });
                    match op {
                        0 => match st.last_mut() {
                            Some(top) => *top = data,
                            None => st.push(data),
                        },
                        1 => st.push(data),
                        2 => {
                            st.pop();
                        }
                        _ => continue,
                    }
                    self.color = on_paper(
                        st.last().and_then(|t| pdf_color(t)).unwrap_or([0, 0, 0]),
                        self.dark,
                    );
                }
                Some("M") if a.len() >= 4 => match a[1].as_str() {
                    Some("save") => self.mstack.push(self.cur),
                    Some("restore") => self.cur = self.mstack.pop().unwrap_or(IDENTITY),
                    _ => {
                        let v: Vec<f64> = a
                            .get(4)
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .split_whitespace()
                            .filter_map(|t| t.parse().ok())
                            .collect();
                        if v.len() == 4 {
                            // PDF matrices are y-up; display lists are y-down: conjugate by the flip
                            let (ma, mb, mc, md) = (v[0], -v[1], -v[2], v[3]);
                            let (px, py) = (num(&a[2]) + dx, num(&a[3]) + dy);
                            self.cur = mul(
                                self.cur,
                                [
                                    ma,
                                    mb,
                                    mc,
                                    md,
                                    px - (ma * px + mc * py),
                                    py - (mb * px + md * py),
                                ],
                            );
                        }
                    }
                },
                _ => {}
            }
        }
    }
}

/// Fill color (else stroke color) of a PDF color string such as "1 0 0 rg 1 0 0 RG".
fn pdf_color(data: &str) -> Option<[u8; 3]> {
    let t: Vec<&str> = data.split_whitespace().collect();
    let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let (mut fill, mut stroke) = (None, None);
    for (i, op) in t.iter().enumerate() {
        let n = |k: usize| -> Option<Vec<f64>> {
            (i >= k).then(|| {
                t[i - k..i]
                    .iter()
                    .map(|s| s.parse().unwrap_or(0.0))
                    .collect()
            })
        };
        let rgb = match *op {
            "g" | "G" => n(1).map(|v| [c(v[0]), c(v[0]), c(v[0])]),
            "rg" | "RG" => n(3).map(|v| [c(v[0]), c(v[1]), c(v[2])]),
            "k" | "K" => n(4).map(|v| {
                [
                    c((1.0 - v[0]) * (1.0 - v[3])),
                    c((1.0 - v[1]) * (1.0 - v[3])),
                    c((1.0 - v[2]) * (1.0 - v[3])),
                ]
            }),
            _ => None,
        };
        let Some(rgb) = rgb else { continue };
        if op.chars().all(|ch| ch.is_lowercase()) {
            fill.get_or_insert(rgb);
        } else {
            stroke.get_or_insert(rgb);
        }
    }
    fill.or(stroke)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_colors() {
        assert_eq!(pdf_color("1 0 0 rg 0 0 1 RG"), Some([255, 0, 0]));
    }

    #[test]
    fn dark_paper_draws_light_ink() {
        let pt = 65536.0;
        let dl: DisplayList = serde_json::from_value(serde_json::json!({
            "lines": [{"x": 0, "y": 10.0 * pt, "w": 10.0 * pt, "h": 10.0 * pt, "d": 0, "items": [["r", 0, 0, 10.0 * pt, 10.0 * pt]]}],
        }))
        .unwrap();
        let dl = Arc::new(dl);
        let job = |dark| Job {
            rows: vec![Row {
                dl: dl.clone(),
                line: 0,
                dx: 0.0,
                dy: 0.0,
            }],
            other: None,
            region: [0.0, 0.0, 20.0 * pt, 20.0 * pt],
            scale: 1.0 / pt,
            pdf: None,
            clear: vec![],
            dark,
        };
        // BGRA
        let at = |dark, x: usize, y: usize| {
            let (w, _, px) = render(&mut Resources::default(), &job(dark));
            let i = (y * w as usize + x) * 4;
            [px[i + 2], px[i + 1], px[i]]
        };
        assert_eq!(at(Some(DARK), 15, 15), [0, 0, 0]);
        assert_eq!(at(Some(DARK), 5, 5), [224, 224, 224]);
        assert_eq!(at(None, 15, 15), [255, 255, 255]);
        assert_eq!(at(None, 5, 5), [0, 0, 0]);
        // paper and ink of one's own
        let navy = DarkPaper {
            paper: [16, 24, 48],
            ink: [250, 240, 200],
        };
        assert_eq!(at(Some(navy), 15, 15), navy.paper);
        assert_eq!(at(Some(navy), 5, 5), navy.ink);
    }

    #[test]
    fn a_pdf_drawn_page_turns_onto_the_dark_paper() {
        let navy = DarkPaper {
            paper: [16, 24, 48],
            ink: [250, 240, 200],
        };
        // white, black, mid gray, as RGBA
        let mut px = [255, 255, 255, 255, 0, 0, 0, 255, 128, 128, 128, 255];
        onto_dark(&mut px, navy);
        assert_eq!(px[..3], navy.paper);
        assert_eq!(px[4..7], navy.ink);
        assert_eq!(px[8..11], [132, 131, 123]);
        // the default dark paper as before: black paper, ink 224
        let mut px = [255, 255, 255, 255, 0, 0, 0, 255, 128, 128, 128, 255];
        onto_dark(&mut px, DARK);
        assert_eq!(px[..11], [0, 0, 0, 255, 224, 224, 224, 255, 111, 111, 111]);
    }
}
