//! Chargement des pages en arrière-plan.
//!
//! Un fil dédié par livre ouvre l'archive (certains fournisseurs, comme
//! PDFium, ne sont pas transférables entre fils), puis décode les pages et les
//! vignettes à la demande. Les pages demandées pour l'affichage passent avant
//! les vignettes. L'interface ne bloque jamais sur une archive lente.

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

use comicrust_core::{open_book_patiently, BookFormat, ComicInfo, PageImage};
use eframe::egui::{self, ColorImage};

pub const THUMB_HEIGHT: u32 = 180;

pub enum Request {
    Page(usize),
    Thumb(usize),
}

pub enum Event {
    Opened { format: BookFormat, pages: Vec<String>, info: Option<Box<ComicInfo>> },
    Page { index: usize, result: Result<(ColorImage, [u32; 2]), String> },
    Thumb { index: usize, image: Option<ColorImage> },
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

fn thumbnail(img: &PageImage) -> ColorImage {
    let (w, h) = img.dimensions();
    let th = THUMB_HEIGHT.min(h.max(1));
    let tw = ((w as f32 * th as f32 / h.max(1) as f32) as u32).max(1);
    to_color_image(&image::imageops::thumbnail(img, tw, th))
}

impl BookLoader {
    pub fn open(path: PathBuf, ctx: egui::Context, max_texture_side: u32) -> Self {
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
                let mut book = match open_book_patiently(&path, std::time::Duration::from_secs(20)) {
                    Ok(b) => b,
                    Err(e) => return send(Event::Failed(e.to_string())),
                };
                let info = book.comic_info().ok().flatten().map(Box::new);
                send(Event::Opened { format: book.format(), pages: book.pages().to_vec(), info });

                let mut pages: VecDeque<usize> = VecDeque::new();
                let mut thumbs: VecDeque<usize> = VecDeque::new();
                let mut thumbs_done: HashSet<usize> = HashSet::new();
                let debug = std::env::var_os("COMICRUST_DEBUG").is_some();
                loop {
                    // Attente bloquante seulement quand il n'y a plus rien à faire.
                    if pages.is_empty() && thumbs.is_empty() {
                        match req_rx.recv() {
                            Ok(r) => push(r, &mut pages, &mut thumbs),
                            Err(_) => return,
                        }
                    }
                    loop {
                        match req_rx.try_recv() {
                            Ok(r) => push(r, &mut pages, &mut thumbs),
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => return,
                        }
                    }
                    if debug {
                        eprintln!("[loader] file pages={pages:?} vignettes={thumbs:?}");
                    }
                    if let Some(index) = pages.pop_front() {
                        match book.render_page(index) {
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
                            let image = book.render_page(index).ok().map(|img| thumbnail(&img));
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

fn push(r: Request, pages: &mut VecDeque<usize>, thumbs: &mut VecDeque<usize>) {
    match r {
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
