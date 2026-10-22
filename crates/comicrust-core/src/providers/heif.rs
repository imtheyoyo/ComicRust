//! Pages HEIF / HEIC / AVIF (conteneur ISO BMFF « ftyp », comme les photos d'iPhone).
//!
//! - **AVIF** (image AV1) : décodeur pur Rust `oxideav-avif`, sur toutes les plateformes.
//! - **HEIC / HEIF** (image HEVC) : sous Windows, le décodeur du système (WIC) quand les
//!   extensions « HEIF » et « HEVC » de Windows sont installées ; sinon le décodeur pur
//!   Rust `heif-oxide`, plus lent et encore imparfait (petits défauts possibles sur les
//!   zones très détaillées).
//!
//! ComicRack CE utilise `libheif` pour les mêmes formats (`HeifAvifImage`).

use super::PageImage;
use crate::Error;

/// Marques (`ftyp`) des images HEIF et AVIF.
const BRANDS: &[&[u8; 4]] =
    &[b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx", b"mif1", b"msf1", b"avif", b"avis", b"miaf"];

/// Image HEIF ou AVIF (d'après l'en-tête « ftyp ») ?
pub fn is_heif_family(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[4..8] == b"ftyp" && BRANDS.iter().any(|b| &bytes[8..12] == *b)
}

/// AVIF déclaré (marque principale) ?
fn is_avif(bytes: &[u8]) -> bool {
    matches!(&bytes[8..12], b"avif" | b"avis")
}

/// Décode une image HEIF, HEIC ou AVIF.
pub fn decode(bytes: &[u8]) -> Result<PageImage, Error> {
    // Une image « mif1 » peut contenir de l'AV1 comme du HEVC : l'AVIF est essayé d'abord
    // (il refuse tout de suite un contenu HEVC).
    let avif_err = match decode_avif(bytes) {
        Ok(img) => return Ok(img),
        Err(e) => e,
    };
    if is_avif(bytes) {
        return Err(Error::Decode(format!("AVIF : {avif_err}")));
    }
    #[cfg(windows)]
    let wic_err = match wic::decode(bytes) {
        Ok(img) => return Ok(img),
        Err(e) => Some(e),
    };
    #[cfg(not(windows))]
    let wic_err: Option<String> = None;
    match decode_heic(bytes) {
        Ok(img) => Ok(img),
        Err(e) => Err(Error::Decode(match wic_err {
            Some(w) => format!(
                "HEIF : {e} ; décodeur de Windows : {w} (installer les extensions « HEIF » et « HEVC » \
                 depuis le Microsoft Store)"
            ),
            None => format!("HEIF : {e}"),
        })),
    }
}

fn decode_avif(bytes: &[u8]) -> Result<PageImage, String> {
    let img = std::panic::catch_unwind(|| oxideav_avif::decode_rgba8(bytes))
        .map_err(|_| "décodeur interrompu".to_string())?
        .map_err(|e| e.to_string())?;
    PageImage::from_raw(img.width, img.height, img.data).ok_or_else(|| "taille d'image incohérente".into())
}

fn decode_heic(bytes: &[u8]) -> Result<PageImage, String> {
    let img = std::panic::catch_unwind(|| heif_oxide::decode_bytes(bytes))
        .map_err(|_| "décodeur interrompu".to_string())?
        .map_err(|e| e.to_string())?;
    PageImage::from_raw(img.width, img.height, img.to_rgba8()).ok_or_else(|| "taille d'image incohérente".into())
}

/// Décodeur d'images de Windows (Windows Imaging Component).
#[cfg(windows)]
mod wic {
    use windows::core::GUID;
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_WICPixelFormat32bppRGBA, IWICImagingFactory, IWICPalette,
        WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};

    use super::PageImage;

    pub fn decode(bytes: &[u8]) -> Result<PageImage, String> {
        // SAFETY : appels COM documentés ; `bytes` reste valide jusqu'à la fin de la
        // fonction, donc pendant toute la vie du flux WIC qui le lit.
        unsafe {
            // Déjà initialisé (éventuellement en STA) : sans conséquence pour WIC.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let factory: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).map_err(|e| e.message())?;
            let stream = factory.CreateStream().map_err(|e| e.message())?;
            stream.InitializeFromMemory(bytes).map_err(|e| e.message())?;
            let decoder = factory
                .CreateDecoderFromStream(&stream, std::ptr::null::<GUID>(), WICDecodeMetadataCacheOnDemand)
                .map_err(|e| e.message())?;
            let frame = decoder.GetFrame(0).map_err(|e| e.message())?;
            let converter = factory.CreateFormatConverter().map_err(|e| e.message())?;
            converter
                .Initialize(
                    &frame,
                    &GUID_WICPixelFormat32bppRGBA,
                    WICBitmapDitherTypeNone,
                    None::<&IWICPalette>,
                    0.0,
                    WICBitmapPaletteTypeCustom,
                )
                .map_err(|e| e.message())?;
            let (mut w, mut h) = (0u32, 0u32);
            converter.GetSize(&mut w, &mut h).map_err(|e| e.message())?;
            let mut buf = vec![0u8; w as usize * h as usize * 4];
            converter.CopyPixels(std::ptr::null(), w * 4, &mut buf).map_err(|e| e.message())?;
            PageImage::from_raw(w, h, buf).ok_or_else(|| "taille d'image incohérente".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Images de test produites par ffmpeg (AVIF) et libheif (HEIC), 64 × 48, dégradé.
    const AVIF: &[u8] = include_bytes!("../../tests/fixtures/degrade.avif");
    const HEIC: &[u8] = include_bytes!("../../tests/fixtures/degrade.heic");

    fn close_to_reference(img: &PageImage) {
        assert_eq!(img.dimensions(), (64, 48));
        // Dégradé horizontal rouge, vert constant 128, bleu vertical (voir le générateur).
        for (x, y) in [(4, 4), (32, 24), (60, 44)] {
            let p = img.get_pixel(x, y).0;
            let want = [(x * 4) as i32, 128, (y * 5) as i32];
            for c in 0..3 {
                assert!((p[c] as i32 - want[c]).abs() <= 12, "pixel ({x},{y}) {p:?} ≠ {want:?}");
            }
        }
    }

    #[test]
    fn avif_page() {
        assert!(is_heif_family(AVIF));
        let img = super::super::decode_image(AVIF).unwrap();
        close_to_reference(&img);
    }

    #[test]
    fn heic_page() {
        assert!(is_heif_family(HEIC));
        assert!(!is_avif(HEIC));
        let img = super::super::decode_image(HEIC).unwrap();
        close_to_reference(&img);
    }

    #[test]
    fn not_heif() {
        assert!(!is_heif_family(b"\x89PNG\r\n\x1a\n0000"));
        assert!(decode(b"\0\0\0\x18ftypheic\0\0\0\0garbage").is_err());
    }
}
