//! Convertit une image en document DjVu d'une page (pour les essais).
//!   cargo run --release -p comicrust-core --example make_djvu -- page.jpg sortie.djvu
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let img = image::open(&args[1]).expect("image").into_rgba8();
    let mut pm = djvu_rs::Pixmap::try_new(img.width(), img.height(), 255, 255, 255, 255).unwrap();
    for (x, y, p) in img.enumerate_pixels() {
        pm.set_rgb(x, y, p[0], p[1], p[2]);
    }
    let bytes = djvu_rs::djvu_encode::PageEncoder::from_pixmap(&pm).with_dpi(150).encode().unwrap();
    std::fs::write(&args[2], bytes).unwrap();
}
