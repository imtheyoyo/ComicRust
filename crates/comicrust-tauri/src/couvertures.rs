//! Couvertures : miniatures JPEG gardées dans `data\couvertures\<id>.jpg`, produites à la
//! demande (protocole de ressources `cr-couv`, voir `app.rs`) par un petit groupe de fils.
//!
//! Logique reprise de `crates/comicrust/src/library/covers.rs` (dupliquée : décision du plan),
//! sans egui : on rend des octets JPEG, pas des textures. Le fichier d'archive n'est lu (souvent
//! sur un NAS) qu'une seule fois ; ensuite le cache disque suffit, même si le livre disparaît.

use comicrust_core::open_book_patiently;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Dossier des couvertures dans `data\`.
pub const DOSSIER_COUVERTURES: &str = "couvertures";
/// Hauteur maximale des miniatures (pixels).
pub const HAUTEUR_MINIATURE: u32 = 360;
/// Fils de travail : un NAS ne gagne rien à plus de lectures simultanées.
pub const FILS: usize = 4;

/// `data\couvertures`.
pub fn dossier(data: &Path) -> PathBuf {
    data.join(DOSSIER_COUVERTURES)
}

/// Nom de fichier sûr pour un identifiant (GUID en pratique) ; l'image 0 garde le nom court.
/// Même convention que la version egui.
pub fn nom_cache(id: &str, image: usize) -> String {
    let sur: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    if image == 0 {
        format!("{sur}.jpg")
    } else {
        format!("{sur}-p{image}.jpg")
    }
}

static COMPTEUR_TMP: AtomicU64 = AtomicU64::new(0);

/// Octets JPEG de la couverture : depuis le cache disque, sinon lus dans l'archive `fichier`
/// (image n°`image`, réduite à [`HAUTEUR_MINIATURE`] pixels) puis mis en cache.
pub fn couverture_jpeg(cache: &Path, id: &str, fichier: Option<&Path>, image: usize) -> Result<Vec<u8>, String> {
    let chemin = cache.join(nom_cache(id, image));
    if let Ok(octets) = std::fs::read(&chemin) {
        if !octets.is_empty() {
            return Ok(octets);
        }
    }
    let fichier = fichier.ok_or("livre sans fichier")?;
    let mut livre = open_book_patiently(fichier, std::time::Duration::from_secs(20)).map_err(|e| e.to_string())?;
    let n = livre.pages().len();
    if n == 0 {
        return Err("aucune page".into());
    }
    let page = livre.render_page(image.min(n - 1)).map_err(|e| e.to_string())?;
    let (l, h) = page.dimensions();
    let hauteur = HAUTEUR_MINIATURE.min(h.max(1));
    let largeur = ((l as f32 * hauteur as f32 / h.max(1) as f32) as u32).max(1);
    let petite = image::imageops::thumbnail(&page, largeur, hauteur);
    let rgb = image::DynamicImage::ImageRgba8(petite).into_rgb8();
    let mut octets = Vec::new();
    rgb.write_to(&mut std::io::Cursor::new(&mut octets), image::ImageFormat::Jpeg).map_err(|e| e.to_string())?;
    // Cache : écrit sous un nom temporaire puis renommé (jamais de JPEG à moitié écrit).
    // Un échec d'écriture n'empêche pas de servir l'image.
    if std::fs::create_dir_all(cache).is_ok() {
        let tmp = cache.join(format!("{}.{}.tmp", nom_cache(id, image), COMPTEUR_TMP.fetch_add(1, Ordering::Relaxed)));
        if std::fs::write(&tmp, &octets).is_ok() && std::fs::rename(&tmp, &chemin).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }
    Ok(octets)
}

/// Une tâche reçoit `true` si elle a trop attendu (la requête a sans doute été abandonnée par
/// la WebView : elle répond sans travailler).
type Tache = Box<dyn FnOnce(bool) + Send>;

/// Attente au-delà de laquelle une tâche est jugée périmée.
pub const ATTENTE_MAX: Duration = Duration::from_secs(30);
/// Durée maximale de la production d'une couverture (archive sur un NAS qui ne répond plus).
pub const DELAI_PRODUCTION: Duration = Duration::from_secs(25);

#[derive(Default)]
struct File {
    taches: VecDeque<(Instant, Tache)>,
    fermee: bool,
}

/// Groupe de fils qui exécutent des tâches, **les plus récentes d'abord** (ce qui vient
/// d'apparaître à l'écran passe devant ; ce qui a défilé hors de vue attend, puis est périmé).
pub struct Pool {
    partage: Arc<(Mutex<File>, Condvar)>,
}

impl Pool {
    pub fn nouveau(fils: usize) -> Self {
        let partage: Arc<(Mutex<File>, Condvar)> = Arc::default();
        for n in 0..fils.max(1) {
            let partage = partage.clone();
            std::thread::Builder::new()
                .name(format!("comicrust-couv-{n}"))
                .spawn(move || loop {
                    let (depuis, tache) = {
                        let (verrou, cv) = &*partage;
                        let Ok(mut f) = verrou.lock() else { return };
                        loop {
                            if f.fermee {
                                return;
                            }
                            if let Some(t) = f.taches.pop_back() {
                                break t;
                            }
                            f = match cv.wait(f) {
                                Ok(f) => f,
                                Err(_) => return,
                            };
                        }
                    };
                    tache(depuis.elapsed() > ATTENTE_MAX);
                })
                .expect("fil des couvertures");
        }
        Pool { partage }
    }

    /// Met une tâche en file.
    pub fn executer(&self, tache: impl FnOnce(bool) + Send + 'static) {
        let (verrou, cv) = &*self.partage;
        if let Ok(mut f) = verrou.lock() {
            f.taches.push_back((Instant::now(), Box::new(tache)));
            cv.notify_one();
        }
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        let (verrou, cv) = &*self.partage;
        if let Ok(mut f) = verrou.lock() {
            f.fermee = true;
            cv.notify_all();
        }
    }
}

/// Comme [`couverture_jpeg`], mais rend la main après [`DELAI_PRODUCTION`] même si la lecture
/// est bloquée : le travail continue dans un fil détaché (il remplira le cache s'il aboutit).
pub fn couverture_avec_delai(
    cache: PathBuf,
    id: String,
    fichier: Option<PathBuf>,
    image: usize,
    delai: Duration,
) -> Result<Vec<u8>, String> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("comicrust-couv-lecture".into())
        .spawn(move || {
            let _ = tx.send(couverture_jpeg(&cache, &id, fichier.as_deref(), image));
        })
        .map_err(|e| e.to_string())?;
    rx.recv_timeout(delai).unwrap_or_else(|_| Err("délai dépassé".into()))
}

/// Cache disque seul (lecture rapide d'un petit fichier local) : `None` si absent.
pub fn depuis_le_cache(cache: &Path, id: &str, image: usize) -> Option<Vec<u8>> {
    std::fs::read(cache.join(nom_cache(id, image))).ok().filter(|o| !o.is_empty())
}

/// CBZ d'une image unie, pour les tests.
#[cfg(test)]
pub(crate) fn cbz_de_test(chemin: &Path, couleur: [u8; 4], pages: usize, info: Option<&str>) {
    use std::io::Write;
    let mut w = zip::ZipWriter::new(std::fs::File::create(chemin).unwrap());
    if let Some(xml) = info {
        w.start_file("ComicInfo.xml", zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(xml.as_bytes()).unwrap();
    }
    for i in 1..=pages {
        w.start_file(format!("{i:02}.png"), zip::write::SimpleFileOptions::default()).unwrap();
        let img = image::RgbaImage::from_pixel(600, 900, image::Rgba(couleur));
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        w.write_all(&png).unwrap();
    }
    w.finish().unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn couverture_faite_une_fois_puis_lue_du_cache() {
        let d = tempfile::tempdir().unwrap();
        let livre = d.path().join("Thanos 01.cbz");
        cbz_de_test(&livre, [200, 30, 30, 255], 2, None);
        let cache = dossier(d.path());
        let jpeg = couverture_jpeg(&cache, "id/1", Some(&livre), 0).unwrap();
        assert_eq!(&jpeg[..2], [0xFF, 0xD8], "JPEG");
        let img = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((img.width(), img.height()), (240, 360));
        assert!(cache.join("id_1.jpg").exists());
        // Le livre disparaît : la couverture vient du cache disque.
        std::fs::remove_file(&livre).unwrap();
        assert_eq!(couverture_jpeg(&cache, "id/1", Some(&livre), 0).unwrap(), jpeg);
        // Image au-delà du nombre de pages : la dernière sert (le livre est revenu).
        cbz_de_test(&livre, [10, 10, 200, 255], 2, None);
        assert!(couverture_jpeg(&cache, "id/1", Some(&livre), 7).is_ok());
        assert!(cache.join("id_1-p7.jpg").exists());
        // Ni cache ni fichier : erreur lisible.
        assert!(couverture_jpeg(&cache, "inconnu", Some(&d.path().join("absent.cbz")), 0).is_err());
        assert!(couverture_jpeg(&cache, "inconnu", None, 0).is_err());
    }

    #[test]
    fn le_pool_execute_les_taches() {
        let pool = Pool::nouveau(2);
        let (tx, rx) = mpsc::channel();
        for i in 0..10 {
            let tx = tx.clone();
            pool.executer(move |_perime| tx.send(i).unwrap());
        }
        let mut recus: Vec<i32> =
            (0..10).map(|_| rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap()).collect();
        recus.sort();
        assert_eq!(recus, (0..10).collect::<Vec<_>>());
    }
}
