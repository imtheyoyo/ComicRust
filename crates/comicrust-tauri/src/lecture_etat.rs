//! « Marquer comme lu / non lu » (menu contextuel d'un livre). Contrat : `docs/tauri/ipc-lecture-etat.md`.
//!
//! Réutilise le mécanisme d'enregistrement de la lecture du lecteur (`lecteur.rs`) :
//! - mode autonome : la progression (`current_page`, `last_page_read`) est écrite dans la base de
//!   ComicRust ;
//! - base MySQL de ComicRack avec « Lecture et écriture » : seuls `CurrentPage` et `LastPageRead`
//!   sont réécrits (protocole des compteurs de `db/write.rs`) ;
//! - **tout autre cas (ComicRack en lecture seule, ComicDb.xml, aucune bibliothèque) : refusé avec
//!   la raison habituelle, rien n'est écrit** (règle du projet). La vue n'afficherait d'ailleurs pas
//!   un avancement qui ne serait gardé que dans `avancement.json` (les pastilles lisent la fiche).
//!
//! L'avancement local (`avancement.json`) est aligné : fixé à « lu » ou oublié pour « non lu », sinon
//! le détail du livre (qui garde le plus avancé des deux) contredirait la fiche.
//! Ni `Opened` ni `OpenCount` ne changent : ce n'est pas une ouverture du livre.

use crate::avancement::Avancement;
use crate::biblio::Biblio;
use crate::edition::{Cible, Edition};
use crate::lecteur::Lecteur;
use comicrust_core::db::{self, BookEdit, Value};
use comicrust_core::library::LibraryBook;
use comicrust_core::store::Store;
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

/// Livre non modifié, avec la raison.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LivreIgnore {
    pub id: String,
    pub raison: String,
}

/// Bilan de « Marquer comme lu / non lu ».
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BilanLecture {
    /// État demandé : vrai = lu, faux = non lu.
    pub lu: bool,
    /// Livres dont la fiche a été écrite.
    pub modifies: Vec<String>,
    /// Livres déjà dans l'état demandé.
    pub inchanges: usize,
    /// Livres non modifiés (pages inconnues, conflit, absent de la base…).
    pub ignores: Vec<LivreIgnore>,
}

/// Page courante et dernière page lue à écrire pour l'état demandé, comme `ComicBook.MarkAsRead`
/// et `MarkAsNotRead` de ComicRack : `(0, 0)` pour « non lu » ; pour « lu », la dernière page
/// (`n - 1`), sauf pour un livre d'une seule page : `1` (0 voudrait dire « non lu »), voir
/// `comicrust_core::library::last_page_read_when_read`. Dans les deux cas 100 % selon
/// `LibraryBook::read_percent`.
pub fn pages_cibles(lu: bool, nb_pages: u32) -> (u32, u32) {
    if lu {
        let derniere = comicrust_core::library::last_page_read_when_read(nb_pages);
        (derniere, derniere)
    } else {
        (0, 0)
    }
}

/// La fiche est-elle déjà dans l'état demandé ?
fn deja_dans_l_etat(b: &LibraryBook, lu: bool) -> bool {
    if lu {
        b.read_percent() == Some(100)
    } else {
        b.last_page_read.unwrap_or(0) == 0 && b.current_page.unwrap_or(0) == 0
    }
}

/// Nombre de pages connu : fiche, à défaut avancement local.
fn nombre_pages(b: &LibraryBook, local: Option<Avancement>) -> u32 {
    b.page_count.filter(|n| *n > 0).or(local.map(|a| a.pages as u32).filter(|n| *n > 0)).unwrap_or(0)
}

/// Reporte dans le bilan ce que le rapport d'écriture ComicRack n'a pas écrit (absents, conflits,
/// erreurs) et les livres déjà à jour. Fonction pure, testée sans base.
pub fn integrer_rapport(bilan: &mut BilanLecture, rapport: &db::WriteReport) {
    for id in &rapport.missing {
        bilan.ignores.push(LivreIgnore { id: id.clone(), raison: "absent de la base ComicRack".into() });
    }
    for id in rapport.conflicts.iter().chain(&rapport.stale) {
        bilan.ignores.push(LivreIgnore { id: id.clone(), raison: "modifié par ailleurs".into() });
    }
    for (id, m) in &rapport.failed {
        bilan.ignores.push(LivreIgnore { id: id.clone(), raison: m.clone() });
    }
    bilan.inchanges += rapport.unchanged.len();
}

/// Marque les livres `ids` comme lus (`lu`) ou non lus. Voir l'en-tête du module pour les règles
/// d'écriture. Les livres inconnus de la bibliothèque sont ignorés en silence (comme l'édition).
pub fn marquer_lecture(
    biblio: &Biblio,
    edition: &Edition,
    lecteur: &Lecteur,
    ids: &[String],
    lu: bool,
) -> Result<BilanLecture, String> {
    let cible = edition.cible();
    match &cible {
        Cible::LectureSeule(m) => return Err(m.clone()),
        Cible::Comicrack(cfg) if !cfg.is_complete() => {
            return Err(crate::surveillance::CONFIG_MYSQL_INCOMPLETE.into());
        }
        _ => {}
    }
    if let Some(m) = biblio.motif_hors_ligne() {
        return Err(m);
    }
    // Une opération à la fois, jamais pendant une analyse (garde gardée jusqu'à la fin).
    let _garde = biblio.prendre_edition()?;
    let base = biblio.base()?;
    let mut vus = HashSet::new();
    let livres: Vec<LibraryBook> =
        ids.iter().filter_map(|i| base.get(i)).filter(|b| vus.insert(b.id.to_ascii_lowercase())).cloned().collect();
    if livres.is_empty() {
        return Err("aucun livre connu dans la sélection".into());
    }

    let mut bilan = BilanLecture { lu, modifies: Vec::new(), inchanges: 0, ignores: Vec::new() };
    // (livre, nombre de pages) à écrire.
    let mut a_ecrire: Vec<(LibraryBook, u32)> = Vec::new();
    // Livres ouverts dans le lecteur : la fermeture réécrirait la page courante et annulerait « non lu ».
    let ouverts: HashSet<String> = lecteur.ids_ouverts().into_iter().map(|i| i.to_ascii_lowercase()).collect();
    for b in &livres {
        if ouverts.contains(&b.id.to_ascii_lowercase()) {
            bilan.ignores.push(LivreIgnore { id: b.id.clone(), raison: "ouvert dans le lecteur".into() });
            continue;
        }
        let local = b.file.as_deref().and_then(|f| lecteur.avancement_de(Path::new(f)));
        let n = nombre_pages(b, local);
        if n == 0 {
            bilan.ignores.push(LivreIgnore { id: b.id.clone(), raison: "nombre de pages inconnu".into() });
            continue;
        }
        if deja_dans_l_etat(b, lu) {
            bilan.inchanges += 1;
        } else {
            a_ecrire.push((b.clone(), n));
        }
    }

    if !a_ecrire.is_empty() {
        let fraiches: Vec<LibraryBook> = match &cible {
            Cible::Store(cfg) => {
                let ids_ecrits: Vec<String> = a_ecrire.iter().map(|(b, _)| b.id.clone()).collect();
                let mut st = Store::open_existing(cfg).map_err(|e| format!("base ComicRust inaccessible : {e}"))?;
                // Fiches relues dans la base : seules les colonnes de lecture changent.
                let mut relues = st.get(&ids_ecrits).map_err(|e| format!("lecture de la base ComicRust : {e}"))?;
                for (b, _) in &a_ecrire {
                    if !relues.iter().any(|r| r.id.eq_ignore_ascii_case(&b.id)) {
                        bilan.ignores.push(LivreIgnore { id: b.id.clone(), raison: "absent de la base".into() });
                    }
                }
                for r in &mut relues {
                    let n = a_ecrire.iter().find(|(b, _)| b.id.eq_ignore_ascii_case(&r.id)).map_or(0, |(_, n)| *n);
                    let (courante, derniere) = pages_cibles(lu, n);
                    r.current_page = Some(courante);
                    r.last_page_read = Some(derniere);
                    // Comme le lecteur : PageCount renseigné s'il manquait.
                    if r.page_count.unwrap_or(0) == 0 {
                        r.page_count = Some(n);
                    }
                }
                if relues.is_empty() {
                    Vec::new()
                } else {
                    let compteur = st
                        .upsert(&relues)
                        .map_err(|e| format!("lecture non enregistrée dans la base ComicRust : {e}"))?;
                    biblio.signaler_ecriture_autonome(relues.iter().map(|r| r.id.clone()), Some(compteur));
                    relues
                }
            }
            Cible::Comicrack(cfg) => {
                let edits: Vec<BookEdit> = a_ecrire
                    .iter()
                    .map(|(b, n)| {
                        let (courante, derniere) = pages_cibles(lu, *n);
                        let e = BookEdit::new(&b.id)
                            .set("CurrentPage", Value::Int(courante.into()))
                            .set("LastPageRead", Value::Int(derniere.into()));
                        // Comme le lecteur : PageCount renseigné s'il manquait.
                        if b.page_count.unwrap_or(0) == 0 {
                            e.set("PageCount", Value::Int((*n).into()))
                        } else {
                            e
                        }
                    })
                    .collect();
                let rapport = db::update_books(cfg, &edits)
                    .map_err(|e| format!("base ComicRack inaccessible, lecture non modifiée : {e}"))?;
                biblio.signaler_rapport_comicrack(&rapport);
                integrer_rapport(&mut bilan, &rapport);
                rapport.written.iter().map(|w| w.book.clone()).collect()
            }
            Cible::LectureSeule(m) => return Err(m.clone()),
        };
        bilan.modifies = biblio.remplacer_fiches(&fraiches, false);
    }

    // Avancement local aligné sur la fiche (livres écrits ou déjà dans l'état demandé).
    for b in &livres {
        let ecarte = bilan.ignores.iter().any(|i| i.id.eq_ignore_ascii_case(&b.id));
        let Some(f) = b.file.as_deref().filter(|_| !ecarte) else { continue };
        let valeur = if lu {
            let n = nombre_pages(b, lecteur.avancement_de(Path::new(f)));
            let (courante, derniere) = pages_cibles(true, n);
            // Position locale : une vraie page (un livre d'une page a `courante` = 1).
            Some(Avancement {
                page: courante.min(n.saturating_sub(1)) as usize,
                page_max: derniere as usize,
                pages: n as usize,
                relecture: false,
            })
        } else {
            None
        };
        if let Err(e) = lecteur.fixer_avancement(Path::new(f), valeur) {
            bilan
                .ignores
                .push(LivreIgnore { id: b.id.clone(), raison: format!("avancement local non mis à jour : {e}") });
        }
    }
    Ok(bilan)
}
