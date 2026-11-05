// PDFs and images opened from the project panel: shown page by page in their tab, zoomable
// like the typeset pages. PDF pages are rasterized with hayro (render::pdf_page), images decoded
// with the image crate, both off the main thread.
use crate::render;
use std::path::{Path, PathBuf};

/// Files the viewer shows.
pub const EXTENSIONS: &[&str] = &["pdf", "png", "jpg", "jpeg"];

pub fn can_view(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

pub struct View {
    pub file: PathBuf,
    pub pdf: bool,
    /// Page sizes in points (an image: its pixels, one point each).
    pub pages: Vec<(f32, f32)>,
}

pub fn open(file: &Path) -> Result<View, String> {
    let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
    let pdf = file
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    let pages = if pdf {
        let doc =
            hayro::hayro_syntax::Pdf::new(bytes).map_err(|_| "not a readable PDF".to_string())?;
        doc.pages().iter().map(|p| p.render_dimensions()).collect()
    } else {
        let (w, h) = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())?
            .into_dimensions()
            .map_err(|e| e.to_string())?;
        vec![(w as f32, h as f32)]
    };
    if pages.is_empty() {
        return Err("the PDF has no pages".into());
    }
    Ok(View {
        file: file.to_path_buf(),
        pdf,
        pages,
    })
}

/// Page `ix` at `w`×`h` device pixels, as BGRA for a RenderImage.
pub fn raster(file: &Path, pdf: bool, ix: usize, w: u32, h: u32) -> Option<(u32, u32, Vec<u8>)> {
    let (w, h, mut data) = if pdf {
        let p = render::pdf_page(file, ix, w, h, true)?;
        (p.width(), p.height(), p.take())
    } else {
        let img = image::open(file).ok()?;
        let img = img
            .resize_exact(w, h, image::imageops::FilterType::Triangle)
            .to_rgba8();
        (img.width(), img.height(), img.into_raw())
    };
    for px in data.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    Some((w, h, data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_and_rasterizes_an_image() {
        let dir = std::env::temp_dir().join("rtd-viewer-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("dot.png");
        image::RgbaImage::from_pixel(4, 2, image::Rgba([255, 0, 0, 255]))
            .save(&file)
            .unwrap();
        let v = open(&file).unwrap();
        assert!(!v.pdf);
        assert_eq!(v.pages, vec![(4.0, 2.0)]);
        let (w, h, data) = raster(&file, false, 0, 8, 4).unwrap();
        assert_eq!((w, h), (8, 4));
        // red comes out as BGRA
        assert_eq!(&data[..4], &[0, 0, 255, 255]);
    }

    #[test]
    fn opens_and_rasterizes_a_pdf() {
        // any one-page PDF from the TeX tree
        crate::engine::configure("", "/Library/TeX/texbin");
        let Some(file) = crate::engine::kpsewhich("example-image-a.pdf") else {
            return;
        };
        let v = open(&file).unwrap();
        assert!(v.pdf && !v.pages.is_empty());
        let (w, h) = v.pages[0];
        let (pw, ph, data) = raster(&file, true, 0, w as u32, h as u32).unwrap();
        assert_eq!(data.len(), (pw * ph * 4) as usize);
    }
}
