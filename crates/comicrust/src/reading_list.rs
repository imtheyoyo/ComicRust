//! Listes de lecture `.cbl` : chaque entrée est rattachée à un fichier, via la
//! base ComicRack (`ComicDB.xml`) si elle est configurée, sinon en parcourant
//! le dossier des BD (ou, à défaut, le dossier de la liste).

use std::path::{Path, PathBuf};

use comicrust_core::cbl::CblBook;
use comicrust_core::index::{FileIndex, ScanProgress};
use comicrust_core::{Library, ReadingList};

pub struct Entry {
    pub book: CblBook,
    pub file: Option<PathBuf>,
}

impl Entry {
    pub fn label(&self) -> String {
        let mut s = format!("{} #{}", self.book.series, self.book.number);
        if let Some(y) = self.book.year {
            s.push_str(&format!(" ({y})"));
        }
        s
    }
}

pub struct ListState {
    pub path: PathBuf,
    pub name: String,
    pub entries: Vec<Entry>,
    pub current: Option<usize>,
    /// D'où viennent les correspondances (affiché dans le panneau).
    pub source: String,
    /// Bilan de l'indexation du dossier (fichiers lus via ComicInfo, via le nom…).
    pub index_summary: Option<String>,
    /// Exemples de fichiers dont ni le ComicInfo.xml ni le nom n'ont donné série + numéro.
    pub unrecognized: Vec<PathBuf>,
    /// Le dossier parcouru est celui de la liste (aucun dossier des BD choisi).
    pub default_root: bool,
}

impl ListState {
    pub fn load(
        path: &Path,
        library_db: Option<&Path>,
        comics_root: Option<&Path>,
        cache: Option<&Path>,
        progress: Option<&ScanProgress>,
    ) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let list = ReadingList::from_xml(&text).map_err(|e| e.to_string())?;
        let mut entries: Vec<Entry> = list.books.book.iter().cloned().map(|book| Entry { book, file: None }).collect();
        let mut sources = Vec::new();
        let (mut index_summary, mut unrecognized) = (None, Vec::new());

        if let Some(db) = library_db {
            match std::fs::read_to_string(db).map_err(|e| e.to_string()).and_then(|t| Library::from_xml(&t).map_err(|e| e.to_string())) {
                Ok(lib) => {
                    for (entry, hit) in entries.iter_mut().zip(lib.resolve(&list)) {
                        entry.file = hit.and_then(|b| b.file.clone()).map(PathBuf::from).filter(|p| p.exists());
                    }
                    sources.push(format!("base {}", db.file_name().unwrap_or_default().to_string_lossy()));
                }
                Err(e) => sources.push(format!("base illisible ({e})")),
            }
        }

        if entries.iter().any(|e| e.file.is_none()) {
            let root = comics_root.map(Path::to_path_buf).or_else(|| path.parent().map(Path::to_path_buf));
            if let Some(root) = root {
                let index = FileIndex::scan_with(&root, cache, progress);
                for e in entries.iter_mut().filter(|e| e.file.is_none()) {
                    e.file = index.find(&e.book.series, &e.book.number, e.book.year, e.book.volume).cloned();
                }
                sources.push(format!("dossier {}", root.display()));
                index_summary = Some(index.summary());
                unrecognized = index.stats.unrecognized.clone();
            }
        }

        Ok(Self {
            path: path.to_path_buf(),
            name: if list.name.is_empty() {
                path.file_stem().unwrap_or_default().to_string_lossy().into_owned()
            } else {
                list.name
            },
            entries,
            current: None,
            source: sources.join(" + "),
            index_summary,
            unrecognized,
            default_root: comics_root.is_none() && library_db.is_none(),
        })
    }

    pub fn found(&self) -> usize {
        self.entries.iter().filter(|e| e.file.is_some()).count()
    }

    pub fn position_of(&self, file: &Path) -> Option<usize> {
        self.entries.iter().position(|e| e.file.as_deref() == Some(file))
    }

    /// Prochaine entrée disponible après `from` (ou avant si `backward`).
    pub fn neighbour(&self, from: usize, backward: bool) -> Option<usize> {
        if backward {
            (0..from).rev().find(|&i| self.entries[i].file.is_some())
        } else {
            (from + 1..self.entries.len()).find(|&i| self.entries[i].file.is_some())
        }
    }

    /// Diagnostic copiable : bilan, numéros manquants, fichiers non reconnus.
    pub fn diagnostic(&self) -> String {
        let mut out = vec![
            format!("Liste « {} » : {} / {} numéros trouvés", self.name, self.found(), self.entries.len()),
            format!("Recherche : {}", self.source),
        ];
        if let Some(s) = &self.index_summary {
            out.push(format!("Index : {s}"));
        }
        let missing: Vec<String> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.file.is_none())
            .map(|(i, e)| {
                let vol = e.book.volume.map(|v| format!(", volume {v}")).unwrap_or_default();
                format!("{:4}. {}{vol}", i + 1, e.label())
            })
            .collect();
        out.push(String::new());
        out.push(format!("Manquants ({}) :", missing.len()));
        out.extend(missing);
        if !self.unrecognized.is_empty() {
            out.push(String::new());
            out.push("Exemples de fichiers non reconnus (ni ComicInfo.xml, ni nom exploitable) :".into());
            out.extend(self.unrecognized.iter().map(|p| format!("  {}", p.display())));
        }
        out.join("\n")
    }
}
