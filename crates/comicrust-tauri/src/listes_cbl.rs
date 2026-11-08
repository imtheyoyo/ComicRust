//! Import / export de listes de lecture `.cbl` (« ComicRack Reading List ») : contrat
//! `docs/tauri/ipc-listes.md`, « Import / export `.cbl` ».
//!
//! Import : chaque entrée est rattachée à un livre de la bibliothèque ouverte, d'abord par son
//! `<Id>` (GUID ComicRack), sinon par série + numéro normalisés (`library::normalize_series`,
//! `normalize_number` de comicrust-core, mêmes règles que `Library::resolve` et que le panneau
//! `.cbl` de la version egui) et le volume quand les deux le connaissent. Une nouvelle liste de
//! lecture reçoit les livres trouvés, dans l'ordre du fichier. Export : les livres d'une liste,
//! dans l'ordre de la liste, au format de ComicRack (`cbl::ReadingList`).

use crate::biblio::Biblio;
use crate::listes::{Issue, ResultatListes};
use comicrust_core::cbl::{CblBook, CblBooks};
use comicrust_core::comic_lists::Members;
use comicrust_core::db::ComicDatabase;
use comicrust_core::library::{normalize_number, normalize_series};
use comicrust_core::lists_edit::NewList;
use comicrust_core::ReadingList;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManquantCbl {
    pub rang: usize,
    pub serie: String,
    pub numero: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annee: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RapportCbl {
    pub nom: String,
    pub total: usize,
    pub trouves: usize,
    pub par_identifiant: usize,
    pub par_serie: usize,
    pub doublons: usize,
    pub manquants: Vec<ManquantCbl>,
    pub texte: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCbl {
    pub resultat: ResultatListes,
    pub rapport: RapportCbl,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportCbl {
    pub chemin: String,
    pub livres: usize,
}

/// Lit un `.cbl`.
pub fn lire_cbl(chemin: &Path) -> Result<ReadingList, String> {
    let texte = std::fs::read(chemin).map_err(|e| format!("{} : {e}", chemin.display()))?;
    let texte = String::from_utf8_lossy(&texte);
    ReadingList::from_xml(&texte).map_err(|e| format!("{} : liste .cbl illisible ({e})", chemin.display()))
}

fn volume_connu(v: Option<i32>) -> Option<i32> {
    v.filter(|v| *v >= 0)
}

/// Rattache les entrées d'une liste `.cbl` aux livres de la base : (identifiants des livres
/// trouvés dans l'ordre, sans doublon ; rapport).
pub fn rattacher(liste: &ReadingList, nom: &str, db: &ComicDatabase) -> (Vec<String>, RapportCbl) {
    // Index série + numéro normalisés → livres (dans l'ordre de la base).
    let mut index: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (i, b) in db.books.iter().enumerate() {
        if let (Some(s), Some(n)) = (b.series.as_deref(), b.number.as_deref()) {
            index.entry((normalize_series(s), normalize_number(n))).or_default().push(i);
        }
    }
    let mut ids = Vec::new();
    let mut vus = HashSet::new();
    let (mut par_id, mut par_serie, mut doublons) = (0, 0, 0);
    let mut manquants = Vec::new();
    for (rang, e) in liste.books.book.iter().enumerate() {
        let par_identifiant = e.id.as_deref().and_then(|id| db.get(id.trim()));
        let livre = match par_identifiant {
            Some(b) => {
                par_id += 1;
                Some(b)
            }
            None => {
                let cle = (normalize_series(&e.series), normalize_number(&e.number));
                let v = volume_connu(e.volume);
                let trouve = index.get(&cle).and_then(|c| {
                    c.iter().map(|&i| &db.books[i]).find(|b| {
                        let bv = volume_connu(b.volume);
                        v.is_none() || bv.is_none() || v == bv
                    })
                });
                if trouve.is_some() {
                    par_serie += 1;
                }
                trouve
            }
        };
        match livre {
            Some(b) => {
                if vus.insert(b.id.to_ascii_lowercase()) {
                    ids.push(b.id.clone());
                } else {
                    doublons += 1;
                }
            }
            None => manquants.push(ManquantCbl {
                rang: rang + 1,
                serie: e.series.clone(),
                numero: e.number.clone(),
                volume: e.volume,
                annee: e.year,
            }),
        }
    }
    let total = liste.books.book.len();
    let trouves = total - manquants.len();
    let mut lignes = vec![
        format!("Liste « {nom} » : {trouves} / {total} numéros trouvés"),
        format!("Par identifiant : {par_id} ; par série et numéro : {par_serie} ; en double : {doublons}"),
        String::new(),
        format!("Manquants ({}) :", manquants.len()),
    ];
    for m in &manquants {
        let vol = m.volume.map(|v| format!(", volume {v}")).unwrap_or_default();
        let an = m.annee.map(|a| format!(" ({a})")).unwrap_or_default();
        lignes.push(format!("{:4}. {} #{}{an}{vol}", m.rang, m.serie, m.numero));
    }
    let rapport = RapportCbl {
        nom: nom.to_string(),
        total,
        trouves,
        par_identifiant: par_id,
        par_serie,
        doublons,
        manquants,
        texte: lignes.join("\n"),
    };
    (ids, rapport)
}

/// Nom de la liste : `<Name>`, sinon le nom du fichier.
fn nom_liste(liste: &ReadingList, chemin: &Path) -> String {
    let n = liste.name.trim();
    if n.is_empty() {
        chemin.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Liste importée".into())
    } else {
        n.to_string()
    }
}

/// Écrit un fichier dans un temporaire du même dossier puis le renomme.
fn ecrire_atomique(chemin: &Path, octets: &[u8]) -> Result<(), String> {
    let dossier = chemin.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let nom = chemin.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dossier.join(format!(".{nom}.comicrust-tmp"));
    std::fs::write(&tmp, octets).map_err(|e| format!("{} : {e}", tmp.display()))?;
    std::fs::rename(&tmp, chemin).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{} : {e}", chemin.display())
    })
}

impl Biblio {
    /// Aperçu de l'import (rien n'est créé).
    pub fn analyser_cbl(&self, chemin: &str) -> Result<RapportCbl, String> {
        let chemin = PathBuf::from(chemin.trim().trim_matches('"'));
        let liste = lire_cbl(&chemin)?;
        let base = self.base()?;
        Ok(rattacher(&liste, &nom_liste(&liste, &chemin), &base).1)
    }

    /// Nouvelle liste de lecture d'après un `.cbl`.
    pub fn importer_cbl(
        &self,
        chemin: &str,
        nom: Option<String>,
        parent: Option<String>,
        position: Option<usize>,
        apres_liste: Option<&str>,
    ) -> Result<ImportCbl, String> {
        let chemin = PathBuf::from(chemin.trim().trim_matches('"'));
        let liste = lire_cbl(&chemin)?;
        let base = self.base()?;
        let nom = nom.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| nom_liste(&liste, &chemin));
        let (ids, rapport) = rattacher(&liste, &nom, &base);
        let (parent, position) = self.emplacement(parent, position, apres_liste);
        let (n, total) = (ids.len(), rapport.total);
        let nom_op = nom.trim().to_string();
        let resultat = self.executer(format!("Import de « {nom_op} »"), move |ed| {
            let id = ed.create(parent.as_deref(), position, &nom, NewList::Reading { books: ids })?;
            Ok(Issue {
                selection: Some(Some(id.clone())),
                creee: Some(id),
                info: Some(format!(
                    "Liste « {nom_op} » importée : {n} livre{} sur {total}",
                    if n > 1 { "s" } else { "" }
                )),
                ..Default::default()
            })
        })?;
        Ok(ImportCbl { resultat, rapport })
    }

    /// Écrit le `.cbl` des livres d'une liste (dans l'ordre de la liste).
    pub fn exporter_cbl(&self, id: &str, chemin: &str) -> Result<ExportCbl, String> {
        let mut chemin = PathBuf::from(chemin.trim().trim_matches('"'));
        if chemin.as_os_str().is_empty() {
            return Err("chemin du fichier .cbl vide".into());
        }
        if !chemin.extension().is_some_and(|e| e.eq_ignore_ascii_case("cbl")) {
            let mut s = chemin.into_os_string();
            s.push(".cbl");
            chemin = PathBuf::from(s);
        }
        self.rafraichir_listes(false);
        let (items, _) = self.listes_lues();
        let nom = comicrust_core::comic_lists::ComicList::find(&items, id)
            .map(|l| l.name.clone())
            .ok_or_else(|| format!("liste introuvable : {id}"))?;
        let (membres, _) = self.membres_liste(id)?;
        let base = self.base()?;
        let indices: Vec<usize> = match membres {
            Members::All => (0..base.books.len()).collect(),
            Members::Ids(ids) => ids.iter().filter_map(|i| base.index_of(i)).collect(),
            Members::Unknown(why) => return Err(format!("contenu de « {nom} » inconnu ({why}) : export impossible")),
        };
        let mut vus = HashSet::new();
        let livres: Vec<CblBook> = indices
            .into_iter()
            .filter(|&i| vus.insert(i))
            .map(|i| {
                let b = &base.books[i];
                let texte = |v: &Option<String>| v.as_deref().map(str::trim).unwrap_or("").to_string();
                CblBook {
                    series: texte(&b.series),
                    number: texte(&b.number),
                    volume: volume_connu(b.volume),
                    year: b.year.filter(|y| *y > 0),
                    format: b.format.clone().filter(|f| !f.trim().is_empty()),
                    id: Some(b.id.clone()),
                }
            })
            .collect();
        let n = livres.len();
        let rl = ReadingList { name: nom, books: CblBooks { book: livres } };
        let xml = rl.to_xml().map_err(|e| format!("liste .cbl impossible à écrire : {e}"))?;
        ecrire_atomique(&chemin, xml.as_bytes())?;
        Ok(ExportCbl { chemin: chemin.display().to_string(), livres: n })
    }
}
