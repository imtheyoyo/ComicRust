//! TAR / CBT. Les entrées sont indexées une fois (position + taille) puis lues
//! directement par `seek`, sans relire l'archive. L'index ne lit que les en-têtes : les
//! données sont sautées par `seek` (`entries_with_seek`), une analyse ne lit donc pas tout
//! le fichier sur le NAS.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use super::on_demand::OnDemandFile;
use super::{index_entries, parse_comic_info, BookFormat, PageProvider};
use crate::{ComicInfo, Error};

pub struct TarProvider {
    /// Ouvert le temps d'une lecture seulement (voir `on_demand`).
    file: OnDemandFile,
    entries: HashMap<String, (u64, u64)>,
    pages: Vec<String>,
    info_entry: Option<String>,
}

impl TarProvider {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let mut entries = HashMap::new();
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        let mut archive = ::tar::Archive::new(file);
        for entry in archive.entries_with_seek()? {
            let entry = entry?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let name = entry.path()?.to_string_lossy().replace('\\', "/");
            let (pos, size) = (entry.raw_file_position(), entry.size());
            // Les données ne sont plus lues : une taille déclarée qui dépasse la fin du
            // fichier (archive tronquée ou piégée) est refusée ici.
            if pos.checked_add(size).is_none_or(|end| end > len) {
                return Err(Error::Archive(format!("TAR : entrée au-delà de la fin du fichier : {name}")));
            }
            entries.insert(name, (pos, size));
        }
        let (pages, info_entry) = index_entries(entries.keys().cloned());
        let file = OnDemandFile::open(path)?;
        file.close();
        Ok(Self { file, entries, pages, info_entry })
    }

    fn read_entry(&mut self, name: &str) -> Result<Vec<u8>, Error> {
        let (pos, size) = *self.entries.get(name).ok_or_else(|| Error::Archive(format!("entrée absente : {name}")))?;
        let max = super::entry_limit(name);
        if size > max {
            return Err(Error::Io(super::too_big(name, max)));
        }
        let mut buf = vec![0; size as usize];
        let read = self.file.seek(SeekFrom::Start(pos)).and_then(|_| self.file.read_exact(&mut buf));
        self.file.close();
        read?;
        Ok(buf)
    }
}

impl PageProvider for TarProvider {
    fn format(&self) -> BookFormat {
        BookFormat::Tar
    }
    fn pages(&self) -> &[String] {
        &self.pages
    }
    fn read_page(&mut self, index: usize) -> Result<Vec<u8>, Error> {
        let name = self.pages.get(index).ok_or(Error::PageOutOfRange(index))?.clone();
        self.read_entry(&name)
    }
    fn comic_info(&mut self) -> Result<Option<ComicInfo>, Error> {
        match self.info_entry.clone() {
            Some(name) => Ok(Some(parse_comic_info(&self.read_entry(&name)?)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::testutil::*;

    #[test]
    fn cbt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.cbt");
        let mut b = ::tar::Builder::new(File::create(&path).unwrap());
        let mut add = |name: &str, data: &[u8]| {
            let mut h = ::tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, name, data).unwrap();
        };
        for n in NAMES {
            add(n, &png(page_number(n)));
        }
        add("ComicInfo.xml", INFO.as_bytes());
        b.finish().unwrap();
        drop(b);
        assert_eq!(BookFormat::detect(&path), Some(BookFormat::Tar));
        let mut book = TarProvider::open(&path).unwrap();
        check_book(&mut book);
        // Aucune poignée gardée sur le fichier entre deux lectures.
        #[cfg(target_os = "linux")]
        assert_eq!(crate::providers::on_demand::open_descriptors(&path), 0);
    }

    /// En-tête TAR GNU pour `name` de `size` octets.
    fn header(name: &str, size: u64) -> [u8; 512] {
        let mut h = ::tar::Header::new_gnu();
        h.set_path(name).unwrap();
        h.set_size(size);
        h.set_mode(0o644);
        h.set_cksum();
        *h.as_bytes()
    }

    /// Une entrée qui déclare dépasser la fin du fichier est refusée à l'ouverture.
    #[test]
    fn entry_past_end_of_file_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("piege.cbt");
        let mut bytes = header("001.jpg", 1 << 40).to_vec();
        bytes.extend_from_slice(&png(1));
        bytes.resize(bytes.len().div_ceil(512) * 512 + 1024, 0);
        std::fs::write(&path, bytes).unwrap();
        let err = TarProvider::open(&path).err().expect("refusé").to_string();
        assert!(err.contains("au-delà de la fin"), "{err}");
    }

    /// L'index saute les données par `seek` : ouvrir un CBT dont la première page fait
    /// 1 Gio (fichier creux) ne lit que les en-têtes. Avant : tout le fichier était lu.
    #[cfg(target_os = "linux")]
    #[test]
    fn index_reads_headers_only() {
        use std::io::Write;
        let rchar = || -> u64 {
            let s = std::fs::read_to_string("/proc/thread-self/io").unwrap();
            s.lines().find_map(|l| l.strip_prefix("rchar:")).unwrap().trim().parse().unwrap()
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gros.cbt");
        let big: u64 = 1 << 30;
        let mut f = File::create(&path).unwrap();
        f.write_all(&header("001.jpg", big)).unwrap();
        f.seek(SeekFrom::Current(big as i64)).unwrap();
        f.write_all(&header("ComicInfo.xml", INFO.len() as u64)).unwrap();
        let mut data = INFO.as_bytes().to_vec();
        data.resize(512 + 1024, 0);
        f.write_all(&data).unwrap();
        drop(f);
        let before = rchar();
        let mut book = TarProvider::open(&path).unwrap();
        assert_eq!(book.comic_info().unwrap().unwrap().series.as_deref(), Some("Thanos"));
        let read = rchar() - before;
        assert!(read < 1 << 20, "{read} octets lus pour indexer");
        // Page trop grande pour être chargée en mémoire : erreur claire.
        assert!(book.read_page(0).unwrap_err().to_string().contains("trop volumineuse"));
    }
}
