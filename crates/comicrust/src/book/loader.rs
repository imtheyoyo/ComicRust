//! Chargement des pages en arrière-plan.
//!
//! Un fil dédié par livre ouvre l'archive (certains fournisseurs, comme
//! PDFium, ne sont pas transférables entre fils), puis décode les pages et les
//! vignettes à la demande. Les pages demandées pour l'affichage passent avant
//! les vignettes. L'interface ne bloque jamais sur une archive lente.
//!
//! Le fil ne garde **aucune poignée** sur le fichier entre deux lectures (les fournisseurs
//! le referment après chaque page) : le livre affiché peut être réécrit (infos écrites dans
//! le CBZ), y compris sous Windows. Si le fichier a changé depuis son ouverture (taille ou
//! date), le livre est rouvert avant la lecture suivante.

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

use comicrust_core::library::PageInfo;
use comicrust_core::pages::{self, ReadingPage};
use comicrust_core::providers::on_demand::file_stamp;
use comicrust_core::{open_book_patiently, BookFormat, ComicInfo, PageImage, PageProvider};
use eframe::egui::{self, ColorImage};

pub const THUMB_HEIGHT: u32 = 180;

pub enum Request {
    Page(usize),
    Thumb(usize),
    /// Vignette à refaire (« Rafraîchir »).
    RefreshThumb(usize),
}

/// Ordre des pages servies par le chargeur.
#[derive(Debug, Clone)]
pub enum Order {
    /// Images du fichier, dans l'ordre naturel des noms, sans rotation.
    Raw,
    /// Ordre de lecture : liste de pages de la bibliothèque, ou `None` pour celle du
    /// ComicInfo.xml du fichier.
    Reading(Option<Vec<PageInfo>>),
}

pub enum Event {
    /// `pages` : noms des images dans l'ordre servi ; `numbers` : numéro de page du livre
    /// (ComicRack) de chacune ; `total` : nombre d'images du fichier.
    Opened {
        format: BookFormat,
        pages: Vec<String>,
        numbers: Vec<usize>,
        total: usize,
        info: Option<Box<ComicInfo>>,
    },
    Page {
        index: usize,
        result: Result<(ColorImage, [u32; 2]), String>,
    },
    Thumb {
        index: usize,
        image: Option<ColorImage>,
    },
    Failed(String),
}

pub struct BookLoader {
    tx: Sender<Request>,
    rx: Receiver<Event>,
}

fn to_color_image(img: &PageImage) -> ColorImage {
    ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw())
}

fn fit_within(img: PageImage, max_side: u32) -> PageImage {
    let (w, h) = img.dimensions();
    if w <= max_side && h <= max_side {
        return img;
    }
    let s = max_side as f32 / w.max(h) as f32;
    image::imageops::resize(
        &img,
        ((w as f32 * s) as u32).max(1),
        ((h as f32 * s) as u32).max(1),
        image::imageops::FilterType::Triangle,
    )
}

/// Applique la rotation de la page (degrés).
fn rotated(img: PageImage, degrees: u16) -> PageImage {
    match degrees {
        90 => image::imageops::rotate90(&img),
        180 => image::imageops::rotate180(&img),
        270 => image::imageops::rotate270(&img),
        _ => img,
    }
}

/// Pages servies, dans l'ordre (voir [`Order`]).
pub fn plan(order: &Order, count: usize, info: Option<&ComicInfo>) -> Vec<ReadingPage> {
    match order {
        Order::Raw => (0..count).map(|i| ReadingPage { image: i, page: i, rotation: 0 }).collect(),
        Order::Reading(Some(list)) => pages::reading_order(list, count),
        Order::Reading(None) => pages::reading_order(&info.map(pages::from_comic_info).unwrap_or_default(), count),
    }
}

fn thumbnail(img: &PageImage) -> ColorImage {
    let (w, h) = img.dimensions();
    let th = THUMB_HEIGHT.min(h.max(1));
    let tw = ((w as f32 * th as f32 / h.max(1) as f32) as u32).max(1);
    to_color_image(&image::imageops::thumbnail(img, tw, th))
}

impl BookLoader {
    pub fn open(path: PathBuf, ctx: egui::Context, max_texture_side: u32, order: Order) -> Self {
        let (req_tx, req_rx) = mpsc::channel::<Request>();
        let (ev_tx, ev_rx) = mpsc::channel::<Event>();
        thread::Builder::new()
            .name("comicrust-loader".into())
            .spawn(move || {
                let send = |ev: Event| {
                    let _ = ev_tx.send(ev);
                    ctx.request_repaint();
                };
                // ComicRack peut être en train de réécrire le fichier (quelques secondes).
                let mut stamp = file_stamp(&path);
                let mut book = match open_book_patiently(&path, std::time::Duration::from_secs(20)) {
                    Ok(b) => b,
                    Err(e) => return send(Event::Failed(e.to_string())),
                };
                let raw_names = book.pages().to_vec();
                let info = book.comic_info().ok().flatten().map(Box::new);
                let total = book.pages().len();
                let plan = plan(&order, total, info.as_deref());
                let names = plan.iter().map(|p| book.pages().get(p.image).cloned().unwrap_or_default()).collect();
                let numbers = plan.iter().map(|p| p.page).collect();
                send(Event::Opened { format: book.format(), pages: names, numbers, total, info });
                // Rang servi → (image du fichier, rotation).
                let source = |i: usize| plan.get(i).map(|p| (p.image, p.rotation)).unwrap_or((i, 0));

                let mut pages: VecDeque<usize> = VecDeque::new();
                let mut thumbs: VecDeque<usize> = VecDeque::new();
                let mut thumbs_done: HashSet<usize> = HashSet::new();
                let debug = std::env::var_os("COMICRUST_DEBUG").is_some();
                loop {
                    // Attente bloquante seulement quand il n'y a plus rien à faire.
                    if pages.is_empty() && thumbs.is_empty() {
                        match req_rx.recv() {
                            Ok(r) => push(r, &mut pages, &mut thumbs, &mut thumbs_done),
                            Err(_) => return,
                        }
                    }
                    loop {
                        match req_rx.try_recv() {
                            Ok(r) => push(r, &mut pages, &mut thumbs, &mut thumbs_done),
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => return,
                        }
                    }
                    if debug {
                        eprintln!("[loader] file pages={pages:?} vignettes={thumbs:?}");
                    }
                    // Fichier réécrit depuis l'ouverture (infos écrites dans le CBZ…) : rouvert.
                    if let Err(e) = refresh(&path, &mut stamp, &mut book, &raw_names) {
                        return send(Event::Failed(e));
                    }
                    if let Some(index) = pages.pop_front() {
                        let (image, degrees) = source(index);
                        match book.render_page(image).map(|img| rotated(img, degrees)) {
                            Ok(img) => {
                                let size = [img.width(), img.height()];
                                if thumbs_done.insert(index) {
                                    send(Event::Thumb { index, image: Some(thumbnail(&img)) });
                                }
                                let img = fit_within(img, max_texture_side);
                                send(Event::Page { index, result: Ok((to_color_image(&img), size)) });
                            }
                            Err(e) => send(Event::Page { index, result: Err(e.to_string()) }),
                        }
                    } else if let Some(index) = thumbs.pop_front() {
                        if thumbs_done.insert(index) {
                            let (image, degrees) = source(index);
                            let image = book.render_page(image).ok().map(|img| thumbnail(&rotated(img, degrees)));
                            send(Event::Thumb { index, image });
                        }
                    }
                }
            })
            .expect("création du fil de chargement");
        Self { tx: req_tx, rx: ev_rx }
    }

    pub fn request(&self, r: Request) {
        let _ = self.tx.send(r);
    }

    pub fn poll(&self) -> Vec<Event> {
        self.rx.try_iter().collect()
    }
}

/// Rouvre le livre si le fichier a changé depuis la dernière ouverture (empreinte
/// `stamp`). Les images doivent rester les mêmes (`raw_names`) : sinon l'ordre servi n'a
/// plus de sens et le livre est à rouvrir.
fn refresh(
    path: &std::path::Path,
    stamp: &mut Option<(u64, Option<std::time::SystemTime>)>,
    book: &mut Box<dyn PageProvider>,
    raw_names: &[String],
) -> Result<(), String> {
    let now = file_stamp(path);
    if now == *stamp {
        return Ok(());
    }
    let fresh = open_book_patiently(path, std::time::Duration::from_secs(20)).map_err(|e| e.to_string())?;
    if fresh.pages() != raw_names {
        return Err("le fichier a changé (pages différentes) : rouvrir le livre".into());
    }
    *book = fresh;
    // Empreinte relevée avant la réouverture (une réécriture entre-temps sera vue) ; fichier
    // absent à ce moment-là (ComicRack le réécrivait) : celle du fichier rouvert.
    *stamp = if now.is_some() { now } else { file_stamp(path) };
    Ok(())
}

fn push(r: Request, pages: &mut VecDeque<usize>, thumbs: &mut VecDeque<usize>, done: &mut HashSet<usize>) {
    match r {
        Request::RefreshThumb(i) => {
            done.remove(&i);
            thumbs.retain(|&t| t != i);
            thumbs.push_front(i);
        }
        Request::Page(i) => {
            // La dernière demande est la plus urgente (page affichée).
            pages.retain(|&p| p != i);
            pages.push_front(i);
        }
        Request::Thumb(i) => {
            if !thumbs.contains(&i) {
                thumbs.push_back(i);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lecteur : ordre de la fiche (bibliothèque), sinon du ComicInfo.xml ; pages
    /// supprimées masquées, rotation transmise ; onglet « Pages » : ordre brut.
    #[test]
    fn plan_follows_the_page_list() {
        let mut list = pages::full_list(&[], 4);
        pages::move_to_start(&mut list, &[3]);
        pages::set_type(&mut list, &[1], "Deleted");
        pages::set_rotation(&mut list, &[2], "Rotate180");
        let p = plan(&Order::Reading(Some(list)), 4, None);
        assert_eq!(
            p.iter().map(|r| (r.image, r.page, r.rotation)).collect::<Vec<_>>(),
            [(3, 0, 0), (0, 1, 0), (2, 3, 180)]
        );
        let raw = plan(&Order::Raw, 3, None);
        assert_eq!(raw.iter().map(|r| r.image).collect::<Vec<_>>(), [0, 1, 2]);
        let ci = ComicInfo::from_xml(
            r#"<ComicInfo><Pages><Page Image="1" Type="FrontCover"/><Page Image="0" Type="Deleted"/></Pages></ComicInfo>"#,
        )
        .unwrap();
        let file = plan(&Order::Reading(None), 3, Some(&ci));
        assert_eq!(file.iter().map(|r| r.image).collect::<Vec<_>>(), [1, 2], "ComicInfo.xml du fichier");
        let img = image::RgbaImage::new(4, 2);
        assert_eq!(rotated(img, 90).dimensions(), (2, 4));
    }

    /// Livre affiché dans le lecteur pendant l'écriture de ses infos : le fil ne garde pas
    /// le fichier ouvert, puis rouvre le fichier réécrit au lieu d'échouer.
    #[test]
    fn reopens_the_book_after_it_was_rewritten() {
        use std::io::Write;
        use std::time::{Duration, Instant};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("livre.cbz");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for n in ["a.png", "b.png"] {
            w.start_file(n, zip::write::SimpleFileOptions::default()).unwrap();
            let mut png = Vec::new();
            image::RgbaImage::new(6, 9).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
            w.write_all(&png).unwrap();
        }
        w.finish().unwrap();
        let loader = BookLoader::open(path.clone(), egui::Context::default(), 4096, Order::Reading(None));
        let next_page = |loader: &BookLoader, want: usize| {
            let start = Instant::now();
            loop {
                assert!(start.elapsed() < Duration::from_secs(20), "pas de réponse");
                for ev in loader.poll() {
                    match ev {
                        Event::Page { index, result } if index == want => return result.map(|(_, s)| s),
                        Event::Failed(e) => return Err(e),
                        _ => {}
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        loader.request(Request::Page(0));
        assert_eq!(next_page(&loader, 0), Ok([6, 9]));
        #[cfg(target_os = "linux")]
        assert_eq!(comicrust_core::providers::on_demand::open_descriptors(&path), 0);
        let record = r#"<ComicBook Id="x"><Series>S</Series></ComicBook>"#;
        comicrust_core::writer::write_comic_info(&path, record).unwrap();
        loader.request(Request::Page(1));
        assert_eq!(next_page(&loader, 1), Ok([6, 9]), "fichier réécrit : rouvert");
        #[cfg(target_os = "linux")]
        assert_eq!(comicrust_core::providers::on_demand::open_descriptors(&path), 0);
    }
}
