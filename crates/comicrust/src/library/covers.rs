//! Couvertures de la vue Bibliothèque.
//!
//! Chaque couverture est la page choisie comme couverture dans ComicRack (la première
//! par défaut), réduite à `COVER_HEIGHT` pixels et gardée en JPEG dans
//! `data\couvertures\<identifiant>.jpg` (`<identifiant>-p<image>.jpg` si ce n'est pas la
//! première image) : elle n'est lue dans l'archive (souvent sur le NAS) qu'une seule fois. Trois fils de travail traitent les
//! demandes, les plus récentes d'abord (ce que l'on voit à l'écran passe devant).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};

use comicrust_core::open_book_patiently;
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

/// Hauteur des couvertures en cache (pixels).
pub const COVER_HEIGHT: u32 = 360;
const WORKERS: usize = 3;
/// Textures gardées en mémoire graphique au maximum.
const MAX_TEXTURES: usize = 600;

struct Job {
    /// Clé du cache (identifiant + image).
    key: String,
    id: String,
    file: PathBuf,
    image: usize,
}

fn cover_key(id: &str, image: usize) -> String {
    format!("{id}#{image}")
}

#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    closed: bool,
}

type Shared = Arc<(Mutex<Queue>, Condvar)>;

enum Done {
    Ok(String, ColorImage),
    Failed(String),
}

pub enum Cover<'a> {
    Ready(&'a TextureHandle),
    Pending,
    Failed,
}

pub struct CoverCache {
    shared: Shared,
    rx: Receiver<Done>,
    textures: HashMap<String, TextureHandle>,
    /// Ordre d'utilisation pour libérer les plus anciennes textures.
    lru: VecDeque<String>,
    requested: HashSet<String>,
    failed: HashSet<String>,
}

/// Nom de fichier sûr pour un identifiant (GUID en pratique) ; l'image 0 garde le nom
/// des versions précédentes, donc le cache existant reste valable.
fn cache_name(id: &str, image: usize) -> String {
    let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    if image == 0 {
        format!("{safe}.jpg")
    } else {
        format!("{safe}-p{image}.jpg")
    }
}

fn to_color(img: &image::RgbaImage) -> ColorImage {
    ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw())
}

/// Produit la couverture (depuis le cache disque ou l'archive).
fn make_cover(dir: Option<&Path>, job: &Job) -> Result<ColorImage, String> {
    let cached = dir.map(|d| d.join(cache_name(&job.id, job.image)));
    if let Some(p) = &cached {
        if let Ok(img) = image::open(p) {
            return Ok(to_color(&img.into_rgba8()));
        }
    }
    let mut book = open_book_patiently(&job.file, std::time::Duration::from_secs(20)).map_err(|e| e.to_string())?;
    let n = book.pages().len();
    if n == 0 {
        return Err("aucune page".into());
    }
    let page = book.render_page(job.image.min(n - 1)).map_err(|e| e.to_string())?;
    let (w, h) = page.dimensions();
    let th = COVER_HEIGHT.min(h.max(1));
    let tw = ((w as f32 * th as f32 / h.max(1) as f32) as u32).max(1);
    let small = image::imageops::thumbnail(&page, tw, th);
    if let Some(p) = &cached {
        let rgb = image::DynamicImage::ImageRgba8(small.clone()).into_rgb8();
        let _ = rgb.save_with_format(p, image::ImageFormat::Jpeg);
    }
    Ok(to_color(&small))
}

impl CoverCache {
    pub fn new(dir: Option<PathBuf>, ctx: &egui::Context) -> Self {
        if let Some(d) = &dir {
            let _ = std::fs::create_dir_all(d);
        }
        let shared: Shared = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let (tx, rx) = mpsc::channel::<Done>();
        for n in 0..WORKERS {
            let shared = shared.clone();
            let tx: Sender<Done> = tx.clone();
            let ctx = ctx.clone();
            let dir = dir.clone();
            std::thread::Builder::new()
                .name(format!("comicrust-couv-{n}"))
                .spawn(move || loop {
                    let job = {
                        let (lock, cv) = &*shared;
                        let mut q = lock.lock().unwrap();
                        loop {
                            if q.closed {
                                return;
                            }
                            // Le plus récent d'abord : ce qui vient d'apparaître à l'écran.
                            if let Some(j) = q.jobs.pop_back() {
                                break j;
                            }
                            q = cv.wait(q).unwrap();
                        }
                    };
                    let done = match make_cover(dir.as_deref(), &job) {
                        Ok(img) => Done::Ok(job.key, img),
                        Err(_) => Done::Failed(job.key),
                    };
                    if tx.send(done).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                })
                .expect("fil des couvertures");
        }
        Self {
            shared,
            rx,
            textures: HashMap::new(),
            lru: VecDeque::new(),
            requested: HashSet::new(),
            failed: HashSet::new(),
        }
    }

    /// Récupère les couvertures terminées (à appeler à chaque image).
    pub fn poll(&mut self, ctx: &egui::Context) {
        for done in self.rx.try_iter().collect::<Vec<_>>() {
            match done {
                Done::Ok(id, img) => {
                    let tex = ctx.load_texture(format!("couv-{id}"), img, TextureOptions::LINEAR);
                    self.requested.remove(&id);
                    self.lru.push_back(id.clone());
                    self.textures.insert(id, tex);
                }
                Done::Failed(id) => {
                    self.requested.remove(&id);
                    self.failed.insert(id);
                }
            }
        }
        while self.textures.len() > MAX_TEXTURES {
            match self.lru.pop_front() {
                Some(old) => {
                    self.textures.remove(&old);
                }
                None => break,
            }
        }
    }

    /// Couverture d'un livre visible (`image` : rang de l'image de couverture dans le
    /// fichier) ; la demande si elle n'est pas encore prête.
    pub fn get(&mut self, id: &str, file: Option<&str>, image: usize) -> Cover<'_> {
        let key = cover_key(id, image);
        if self.textures.contains_key(&key) {
            return Cover::Ready(&self.textures[&key]);
        }
        let Some(file) = file else { return Cover::Failed };
        if self.failed.contains(&key) {
            return Cover::Failed;
        }
        if self.requested.insert(key.clone()) {
            let (lock, cv) = &*self.shared;
            lock.lock().unwrap().jobs.push_back(Job { key, id: id.to_string(), file: PathBuf::from(file), image });
            cv.notify_one();
        }
        Cover::Pending
    }

    /// Oublie un échec (le fichier a pu réapparaître après une actualisation).
    pub fn forget_failure(&mut self, id: &str, image: usize) {
        self.failed.remove(&cover_key(id, image));
    }

    /// Oublie les demandes en attente (après un défilement rapide, par exemple).
    pub fn cancel_pending(&mut self) {
        let (lock, _) = &*self.shared;
        let mut q = lock.lock().unwrap();
        for j in q.jobs.drain(..) {
            self.requested.remove(&j.key);
        }
    }
}

impl Drop for CoverCache {
    fn drop(&mut self) {
        let (lock, cv) = &*self.shared;
        lock.lock().unwrap().closed = true;
        cv.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_is_made_once_then_read_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let book = dir.path().join("Thanos 01.cbz");
        {
            use std::io::Write;
            let mut w = zip::ZipWriter::new(std::fs::File::create(&book).unwrap());
            w.start_file("01.png", zip::write::SimpleFileOptions::default()).unwrap();
            let img = image::RgbaImage::from_pixel(600, 900, image::Rgba([200, 30, 30, 255]));
            let mut png = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
            w.write_all(&png).unwrap();
            w.finish().unwrap();
        }
        let covers = dir.path().join("couvertures");
        std::fs::create_dir_all(&covers).unwrap();
        let job = Job { key: "k".into(), id: "id/1".into(), file: book.clone(), image: 0 };
        let img = make_cover(Some(&covers), &job).unwrap();
        assert_eq!(img.size, [240, 360]);
        let cached = covers.join("id_1.jpg");
        assert!(cached.exists());
        // Couverture choisie au-delà du nombre d'images : la dernière image sert.
        let other = Job { key: "k2".into(), id: "id/1".into(), file: book.clone(), image: 3 };
        assert_eq!(make_cover(Some(&covers), &other).unwrap().size, [240, 360]);
        assert!(covers.join("id_1-p3.jpg").exists());
        // Le livre disparaît : la couverture vient du cache disque.
        std::fs::remove_file(&book).unwrap();
        assert_eq!(make_cover(Some(&covers), &job).unwrap().size, [240, 360]);
    }
}
