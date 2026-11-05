// Fonts and images for drawing display lists: OpenType and Type1 fonts through skrifa (TFM fonts
// via pdftex.map and .enc files), PNG/JPEG through image, PDF pages through hayro.
use super::*;

pub(super) struct MapEntry {
    font_file: Option<String>,
    enc_file: Option<String>,
    pub(super) slant: f64,
    pub(super) extend: f64,
}

/// pdftex.map: which Type1 file (and encoding) draws a TFM font.
pub(super) fn parse_map(text: &str) -> HashMap<String, MapEntry> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('%') || line.starts_with('#') {
            continue;
        }
        let mut tokens = Vec::new();
        let mut rest = line;
        while let Some(t) = rest.trim_start().chars().next() {
            rest = rest.trim_start();
            if t == '"' {
                let end = rest[1..].find('"').map_or(rest.len(), |i| i + 2);
                tokens.push(&rest[..end]);
                rest = &rest[end..];
            } else {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                tokens.push(&rest[..end]);
                rest = &rest[end..];
            }
        }
        let Some(tfm) = tokens.first() else { continue };
        let mut e = MapEntry {
            font_file: None,
            enc_file: None,
            slant: 0.0,
            extend: 1.0,
        };
        for t in &tokens[1..] {
            if let Some(ps) = t.strip_prefix('"') {
                let w: Vec<&str> = ps.trim_end_matches('"').split_whitespace().collect();
                for i in 1..w.len() {
                    if w[i] == "SlantFont" {
                        e.slant = w[i - 1].parse().unwrap_or(0.0);
                    }
                    if w[i] == "ExtendFont" {
                        e.extend = w[i - 1].parse().unwrap_or(1.0);
                    }
                }
            } else if t.starts_with('<') {
                let f = t.trim_start_matches(['<', '[']);
                if f.to_ascii_lowercase().ends_with(".enc") {
                    e.enc_file = Some(f.to_string());
                } else if !f.is_empty() {
                    e.font_file = Some(f.to_string());
                }
            }
        }
        out.entry(tfm.to_string()).or_insert(e);
    }
    out
}

/// Glyph names of an .enc file (`/Name [ /a /b … ] def`), indexed by code.
pub(super) fn parse_enc(text: &str) -> Vec<String> {
    let clean: String = text
        .lines()
        .map(|l| l.split('%').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" ");
    let (Some(a), Some(b)) = (clean.find('['), clean.rfind(']')) else {
        return vec![];
    };
    clean[a + 1..b.max(a + 1)]
        .split_whitespace()
        .filter_map(|t| t.strip_prefix('/'))
        .map(String::from)
        .collect()
}

pub(super) struct Loaded {
    font: Font,
    scaler: ScalerFont,
    pub(super) upem: f64,
    pub(super) type1: bool,
    enc: Option<Vec<String>>,
    pub(super) slant: f64,
    pub(super) extend: f64,
    paths: HashMap<u32, Option<tiny_skia::Path>>,
}

/// Fonts and images a display list draws with, loaded once.
#[derive(Default)]
pub struct Resources {
    /// The project folder, which image paths are relative to.
    pub root: PathBuf,
    map: Option<HashMap<String, MapEntry>>,
    fonts: HashMap<String, Option<Loaded>>,
    /// By file, page and width in device pixels (0 for bitmaps, drawn at their own size).
    images: HashMap<(PathBuf, usize, u32), Option<Pixmap>>,
}

pub(super) struct Sink(PathBuilder);
impl PathSink for Sink {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y)
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y)
    }
    fn quad_to(&mut self, a: f32, b: f32, x: f32, y: f32) {
        self.0.quad_to(a, b, x, y)
    }
    fn curve_to(&mut self, a: f32, b: f32, c: f32, d: f32, x: f32, y: f32) {
        self.0.cubic_to(a, b, c, d, x, y)
    }
    fn close(&mut self) {
        self.0.close()
    }
}

impl Resources {
    pub(super) fn load(&mut self, desc: &FontDesc) -> Option<&mut Loaded> {
        let key = match &desc.filename {
            Some(f) => format!("file:{f}#{}", desc.subfont.unwrap_or(0)),
            None => format!("tfm:{}", desc.name.clone().unwrap_or_default()),
        };
        if !self.fonts.contains_key(&key) {
            let loaded = self.open(desc);
            self.fonts.insert(key.clone(), loaded);
        }
        self.fonts.get_mut(&key).and_then(Option::as_mut)
    }

    fn open(&mut self, desc: &FontDesc) -> Option<Loaded> {
        let (file, index, entry) = match &desc.filename {
            Some(f) => (f.into(), desc.subfont.unwrap_or(1).saturating_sub(1), None),
            None => {
                let map = self.map.get_or_insert_with(|| {
                    kpsewhich("pdftex.map")
                        .and_then(|p| std::fs::read_to_string(p).ok())
                        .map(|t| parse_map(&t))
                        .unwrap_or_default()
                });
                // font 1 is preloaded in the LaTeX format, and LuaTeX reports nothing about it: it
                // is cmex10, the first font latex.ltx loads (display-size integrals and sums)
                let name = desc
                    .name
                    .as_deref()
                    .or((desc.id == Some(1)).then_some("cmex10"))?;
                let e = map.get(name)?;
                let file = kpsewhich(e.font_file.as_deref()?)?;
                (file, 0, Some(e))
            }
        };
        let enc = entry
            .and_then(|e| e.enc_file.as_deref())
            .and_then(kpsewhich)
            .and_then(|p| std::fs::read(p).ok())
            .map(|b| parse_enc(&String::from_utf8_lossy(&b)));
        let (slant, extend) = entry.map_or((0.0, 1.0), |e| (e.slant, e.extend));
        let font = Font::new(std::fs::read(file).ok()?, index)?;
        let type1 = matches!(font.kind(), Kind::Type1(_));
        let upem = if type1 {
            1000.0
        } else {
            font.units_per_em() as f64
        };
        Some(Loaded {
            scaler: ScalerFont::unhinted(font.clone(), None),
            font,
            upem,
            type1,
            enc,
            slant,
            extend,
            paths: HashMap::new(),
        })
    }

    /// An included graphic, `w` device pixels wide: PDF pages are rasterized at that size.
    pub(super) fn image(&mut self, info: &ImageInfo, w: u32, h: u32) -> Option<&Pixmap> {
        let file = self.root.join(&info.file);
        let pdf = file
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
        let page = info.page.unwrap_or(1).max(1) - 1;
        let key = (file.clone(), page, if pdf { w } else { 0 });
        if !self.images.contains_key(&key) {
            // only one size of a PDF figure is kept
            self.images.retain(|k, _| k.0 != file || k.1 != page);
            let img = if pdf {
                pdf_page(&file, page, w, h, false)
            } else {
                bitmap(&file)
            };
            self.images.insert(key.clone(), img);
        }
        self.images.get(&key).and_then(Option::as_ref)
    }
}

/// Page `page` (from 0) of a PDF at `w`×`h` device pixels, on white or transparent.
pub fn pdf_page(file: &Path, page: usize, w: u32, h: u32, white: bool) -> Option<Pixmap> {
    use hayro::vello_cpu::color::palette::css::{TRANSPARENT, WHITE};
    let pdf = hayro::hayro_syntax::Pdf::new(std::fs::read(file).ok()?).ok()?;
    let p = pdf.pages().get(page)?;
    let (pw, ph) = p.render_dimensions();
    let settings = hayro::PixmapSettings {
        x_scale: w as f32 / pw,
        y_scale: h as f32 / ph,
        bg_color: if white { WHITE } else { TRANSPARENT },
    };
    let out = hayro::render(
        p,
        &hayro::RenderCache::new(),
        &Default::default(),
        &Default::default(),
        &settings,
    );
    let size = tiny_skia::IntSize::from_wh(out.width() as u32, out.height() as u32)?;
    Pixmap::from_vec(
        out.take_rgba8(hayro::vello_cpu::peniko::ImageAlphaType::AlphaPremultiplied),
        size,
    )
}

pub(super) fn bitmap(file: &Path) -> Option<Pixmap> {
    let img = image::open(file).ok()?.to_rgba8();
    let size = tiny_skia::IntSize::from_wh(img.width(), img.height())?;
    let mut data = img.into_raw();
    for px in data.as_chunks_mut::<4>().0 {
        let a = px[3] as u16;
        for c in &mut px[..3] {
            *c = ((*c as u16 * a + 127) / 255) as u8;
        }
    }
    Pixmap::from_vec(data, size)
}

/// The image an IMAGE item refers to. Items carry LuaTeX's image index (1, 2, … in creation
/// order) while rtex keys `images_info` by object number; when the key does not match, map by
/// rank, both grow in creation order (plugin webview/render.ts `imageInfo`).
pub(super) fn image_info(dl: &DisplayList, index: i64) -> Option<&ImageInfo> {
    if let Some(i) = dl.images_info.get(&index.to_string()) {
        return Some(i);
    }
    let mut keys: Vec<i64> = dl
        .images_info
        .iter()
        .filter(|(_, i)| !i.cached_picture)
        .filter_map(|(k, _)| k.parse().ok())
        .collect();
    keys.sort();
    dl.images_info
        .get(&keys.get(usize::try_from(index - 1).ok()?)?.to_string())
}

impl Loaded {
    /// The glyph an item draws: by glyph index in OpenType fonts, by character code through the
    /// encoding in Type1 (TFM) fonts.
    pub(super) fn glyph(&self, char: u32, index: Option<u32>) -> Option<GlyphId> {
        if !self.type1 {
            return index
                .map(GlyphId::new)
                .or_else(|| self.font.charmap().map_unicode(char::from_u32(char)?));
        }
        if let Some(name) = self.enc.as_ref().and_then(|e| e.get(char as usize)) {
            return self.font.glyph_by_name(name);
        }
        match self.font.kind() {
            Kind::Type1(t) => t.encoding()?.map(u8::try_from(char).ok()?),
            _ => None,
        }
    }

    pub(super) fn path(&mut self, char: u32, index: Option<u32>) -> Option<&tiny_skia::Path> {
        let gid = self.glyph(char, index)?;
        let key = gid.to_u32();
        if !self.paths.contains_key(&key) {
            let mut sink = Sink(PathBuilder::new());
            let p = self
                .scaler
                .outline(gid)
                .and_then(|o| o.draw(DrawMemory::Scratch, &mut sink).ok())
                .and_then(|_| sink.0.finish());
            self.paths.insert(key, p);
        }
        self.paths.get(&key).and_then(Option::as_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_map_and_encodings() {
        let m = parse_map(
            "cmr10 CMR10 <cmr10.pfb\nptmro8r NimbusRoman-Regular \" .167 SlantFont TeXBase1Encoding ReEncodeFont \" <8r.enc <utmr8a.pfb\n",
        );
        assert_eq!(m["cmr10"].font_file.as_deref(), Some("cmr10.pfb"));
        assert_eq!(m["ptmro8r"].enc_file.as_deref(), Some("8r.enc"));
        assert_eq!(m["ptmro8r"].slant, 0.167);
        assert_eq!(parse_enc("/X [ /a /b % c\n /c ] def"), vec!["a", "b", "c"]);
    }

    /// Needs a TeX installation; skipped without one.
    #[test]
    fn draws_type1_math_glyphs() {
        if kpsewhich("cmex10.pfb").is_none() {
            return;
        }
        let mut res = Resources::default();
        for (name, code) in [(None, 90), (Some("cmmi10"), 69), (Some("cmsy10"), 0)] {
            let desc = FontDesc {
                id: Some(1),
                name: name.map(String::from),
                ..Default::default()
            };
            let name = name.unwrap_or("font 1");
            let f = res
                .load(&desc)
                .unwrap_or_else(|| panic!("{name} not loaded"));
            assert!(
                f.path(code, None).is_some(),
                "{name} char {code} has no outline"
            );
        }
    }
}
