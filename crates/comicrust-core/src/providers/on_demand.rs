//! Fichier ouvert **à la demande** et refermé après chaque lecture ponctuelle.
//!
//! Un livre affiché (lecteur, onglet « Pages » de la fenêtre « Infos du livre »…) reste
//! « ouvert » longtemps : si son fournisseur gardait le fichier ouvert, Windows refuserait
//! de le remplacer (écriture du `ComicInfo.xml`, renommage, déplacement : « Accès refusé »,
//! os error 5), surtout sur un partage réseau (SMB). [`OnDemandFile`] se comporte comme un
//! `File` (lecture + déplacement) mais n'ouvre le fichier qu'au moment d'y lire et le
//! referme quand le fournisseur appelle [`OnDemandFile::close`] (à la fin de chaque
//! lecture de page ou de `ComicInfo.xml`). La position courante est gardée entre deux
//! ouvertures, si bien que les index déjà lus (répertoire central d'un ZIP, positions des
//! entrées d'un TAR, structure d'un PDF) restent valables.
//!
//! **Fichier remplacé entre-temps** (taille ou date de modification différentes de celles
//! de la première ouverture) : la lecture échoue (erreur « le fichier a changé ») au lieu
//! de renvoyer des octets d'un autre fichier ; il faut rouvrir le livre.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// Empreinte du fichier à la première ouverture : (taille, date de modification).
type Stamp = (u64, Option<SystemTime>);

fn stamp_of(file: &File) -> io::Result<Stamp> {
    let m = file.metadata()?;
    Ok((m.len(), m.modified().ok()))
}

/// Empreinte actuelle d'un fichier sur le disque (sans l'ouvrir).
pub fn file_stamp(path: &Path) -> Option<(u64, Option<SystemTime>)> {
    std::fs::metadata(path).ok().map(|m| (m.len(), m.modified().ok()))
}

struct Inner {
    path: PathBuf,
    file: Option<File>,
    pos: u64,
    stamp: Stamp,
}

impl Inner {
    /// Fichier ouvert (rouvert au besoin, à la position courante).
    fn file(&mut self) -> io::Result<&mut File> {
        if self.file.is_none() {
            let mut f = File::open(&self.path)?;
            if stamp_of(&f)? != self.stamp {
                return Err(io::Error::other(format!(
                    "le fichier a changé depuis son ouverture (réécrit ailleurs ?) : {}",
                    self.path.display()
                )));
            }
            f.seek(SeekFrom::Start(self.pos))?;
            self.file = Some(f);
        }
        Ok(self.file.as_mut().expect("fichier ouvert"))
    }
}

/// Fichier en lecture ouvert à la demande (voir le module). Les copies (`clone`) partagent
/// le même fichier et la même position : le fournisseur en garde une pour pouvoir le
/// refermer quand la bibliothèque de lecture (zip, PDFium…) détient l'autre.
#[derive(Clone)]
pub struct OnDemandFile {
    inner: Arc<Mutex<Inner>>,
}

impl OnDemandFile {
    /// Ouvre `path` (le fichier doit exister et être lisible), relève son empreinte et le
    /// laisse ouvert jusqu'au premier [`close`](Self::close).
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let stamp = stamp_of(&file)?;
        let inner = Inner { path: path.to_path_buf(), file: Some(file), pos: 0, stamp };
        Ok(Self { inner: Arc::new(Mutex::new(inner)) })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // Un fil qui a paniqué en lisant ne laisse pas d'état incohérent (position seule).
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Referme le fichier (il sera rouvert à la prochaine lecture).
    pub fn close(&self) {
        self.lock().file = None;
    }

    /// Le fichier est-il ouvert en ce moment ?
    pub fn is_open(&self) -> bool {
        self.lock().file.is_some()
    }
}

impl Read for OnDemandFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut inner = self.lock();
        let n = inner.file()?.read(buf)?;
        inner.pos += n as u64;
        Ok(n)
    }
}

impl Seek for OnDemandFile {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let mut inner = self.lock();
        let target = match to {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::Current(d) => inner.pos.checked_add_signed(d),
            SeekFrom::End(d) => inner.stamp.0.checked_add_signed(d),
        };
        let target = target.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "position invalide"))?;
        if let Some(f) = inner.file.as_mut() {
            f.seek(SeekFrom::Start(target))?;
        }
        inner.pos = target;
        Ok(target)
    }
}

/// Nombre de descripteurs ouverts par ce processus sur `path` (Linux, `/proc/self/fd`) :
/// sert aux tests qui vérifient qu'aucun livre ne garde son fichier ouvert.
#[cfg(target_os = "linux")]
pub fn open_descriptors(path: &Path) -> usize {
    let Ok(want) = path.canonicalize() else { return 0 };
    let Ok(dir) = std::fs::read_dir("/proc/self/fd") else { return 0 };
    dir.filter_map(Result::ok).filter(|e| std::fs::read_link(e.path()).is_ok_and(|t| t == want)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_like_a_file_and_closes_between_reads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.bin");
        std::fs::write(&path, b"0123456789").unwrap();
        let mut f = OnDemandFile::open(&path).unwrap();
        let mut buf = [0u8; 3];
        f.seek(SeekFrom::Start(2)).unwrap();
        f.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"234");
        f.close();
        assert!(!f.is_open());
        #[cfg(target_os = "linux")]
        assert_eq!(open_descriptors(&path), 0);
        // Rouvert à la même position.
        f.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"567");
        assert_eq!(f.seek(SeekFrom::End(-1)).unwrap(), 9);
        assert_eq!(f.seek(SeekFrom::Current(-4)).unwrap(), 5);
        f.close();
        // Fichier remplacé : erreur plutôt que les octets d'un autre fichier.
        std::fs::write(&path, b"autre contenu, plus long").unwrap();
        let err = f.read_exact(&mut buf).unwrap_err().to_string();
        assert!(err.contains("a changé"), "{err}");
    }
}
