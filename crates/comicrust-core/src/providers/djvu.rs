//! DjVu, décodé et rendu en pur Rust (`djvu-rs`). ComicRack CE passe par les
//! outils DjVuLibre externes ; ici aucun binaire supplémentaire n'est requis.

use std::path::Path;

use djvu_rs::djvu_render::{render_pixmap, RenderOptions};
use djvu_rs::DjVuDocument;

use super::{encode_png, BookFormat, PageImage, PageProvider};
use crate::{ComicInfo, Error};

/// Hauteur cible du rendu (comme pour les PDF).
pub const RENDER_HEIGHT: u32 = 2600;

pub struct DjvuProvider {
    doc: DjVuDocument,
    pages: Vec<String>,
}

fn err(e: impl std::fmt::Display) -> Error {
    Error::Djvu(e.to_string())
}

impl DjvuProvider {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let data = std::fs::read(path)?;
        let doc = DjVuDocument::parse(&data).map_err(err)?;
        let n = doc.page_count();
        let width = n.to_string().len().max(3);
        let pages = (1..=n).map(|i| format!("Page {i:0width$}")).collect();
        Ok(Self { doc, pages })
    }
}

impl PageProvider for DjvuProvider {
    fn format(&self) -> BookFormat {
        BookFormat::Djvu
    }
    fn pages(&self) -> &[String] {
        &self.pages
    }
    fn read_page(&mut self, index: usize) -> Result<Vec<u8>, Error> {
        encode_png(&self.render_page(index)?)
    }
    fn render_page(&mut self, index: usize) -> Result<PageImage, Error> {
        if index >= self.pages.len() {
            return Err(Error::PageOutOfRange(index));
        }
        let page = self.doc.page(index).map_err(err)?;
        let (w, h) = (page.width() as u32, page.height() as u32);
        // Pas d'agrandissement au-delà de la résolution native du scan.
        let scale = (RENDER_HEIGHT as f32 / h.max(1) as f32).min(1.0);
        let opts = RenderOptions {
            width: ((w as f32 * scale) as u32).max(1),
            height: ((h as f32 * scale) as u32).max(1),
            ..Default::default()
        };
        let pm = render_pixmap(page, &opts).map_err(err)?;
        PageImage::from_raw(pm.width, pm.height, pm.data)
            .ok_or_else(|| Error::Djvu("tampon de rendu invalide".into()))
    }
    fn comic_info(&mut self) -> Result<Option<ComicInfo>, Error> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use djvu_rs::djvu_encode::PageEncoder;
    use djvu_rs::Pixmap;

    #[test]
    fn djvu_roundtrip() {
        // Page couleur 300x400 encodée en DjVu (IW44) puis relue et rendue.
        let mut pm = Pixmap::try_new(300, 400, 255, 255, 255, 255).unwrap();
        for y in 0..400 {
            for x in 0..300 {
                if y < 200 {
                    pm.set_rgb(x, y, 220, 20, 20);
                }
            }
        }
        let bytes = PageEncoder::from_pixmap(&pm).with_dpi(100).encode().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.djvu");
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(BookFormat::detect(&path), Some(BookFormat::Djvu));

        let mut book = DjvuProvider::open(&path).unwrap();
        assert_eq!(book.pages(), ["Page 001"]);
        let img = book.render_page(0).unwrap();
        assert_eq!((img.width(), img.height()), (300, 400));
        let top = img.get_pixel(150, 50).0;
        let bottom = img.get_pixel(150, 350).0;
        assert!(top[0] > 150 && top[1] < 90, "haut : {top:?}");
        assert!(bottom.iter().take(3).all(|&c| c > 200), "bas : {bottom:?}");
    }
}
