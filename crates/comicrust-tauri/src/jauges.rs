//! Jauges des listes (comme ComicRack) : pour chaque liste, trois comptes affichés à droite de son
//! nom dans le panneau des listes. Contrat : `docs/tauri/ipc-jauges.md`.
//!
//! - **nouveaux** (rouge) : livres jamais ouverts et ajoutés il y a **moins de** [`JOURS_NOUVEAU`] jours (un livre ajouté il y a 7 jours
//!   n'est plus nouveau ; un `Added` dans le futur — horloge, `ComicDb.xml` — n'est pas nouveau) ;
//! - **non lus** (jaune) : livres lus à moins de 100 % (même règle que la pastille « lu » de la
//!   vue : `LibraryBook::read_percent`) ; les nouveaux en font partie ;
//! - **total** (vert) : livres de la liste présents dans la bibliothèque, **exactement** le
//!   `nbLivres` de `arbre_listes` (livres manquants compris : leur fiche reste dans la base).
//!
//! Une seule passe : l'état « nouveau / non lu » de chaque livre est calculé une fois (5 000 livres
//! = un parcours), puis chaque liste ne fait que sommer les états de ses membres, déjà résolus par
//! `membres_toutes` (le contenu des listes intelligentes n'est donc jamais réévalué ici). Le résultat
//! est gardé tant que la base, ses fiches (lecture notée, édition, scan), les listes et le jour ne
//! changent pas : même clé que le contenu des listes.

use crate::biblio::Biblio;
use chrono::{Duration, NaiveDate};
use comicrust_core::comic_lists::Members;
use comicrust_core::db::ComicDatabase;
use comicrust_core::library::LibraryBook;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

/// Un livre jamais ouvert est « nouveau » tant qu'il a été ajouté il y a moins de ce nombre de jours.
pub const JOURS_NOUVEAU: i64 = 7;

/// Les trois comptes d'une liste.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Jauge {
    pub nouveaux: usize,
    pub non_lus: usize,
    pub total: usize,
}

/// Jauges gardées avec la clé `(génération, révision des fiches, version des listes, jour)`.
pub(crate) type JaugesEnCache = ((u64, u64, u64, NaiveDate), Arc<JaugesListes>);

/// Jauges de la bibliothèque et de chaque liste dont le contenu est connu.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JaugesListes {
    /// Toute la bibliothèque (entrée « Bibliothèque » du panneau).
    pub racine: Jauge,
    /// Par identifiant de liste (casse d'origine). Absent : contenu inconnu (critère non pris en
    /// charge) ; le front n'affiche alors pas de jauge.
    pub listes: HashMap<String, Jauge>,
}

/// Date ISO (`2019-10-01T12:58:00+02:00`) → jour ; `0001-01-01` (ComicRack : « jamais ») = absent.
fn jour(v: &Option<String>) -> Option<NaiveDate> {
    let t = v.as_deref()?.trim();
    if t.len() < 10 || t.starts_with("0001-01-01") {
        return None;
    }
    NaiveDate::parse_from_str(t.get(..10)?, "%Y-%m-%d").ok()
}

/// `(nouveau, non lu)` d'un livre au jour `aujourdhui` : nouveau = ajouté dans ]aujourdhui - 7 j ; aujourdhui].
pub fn etat_livre(b: &LibraryBook, aujourdhui: NaiveDate) -> (bool, bool) {
    let non_lu = b.read_percent() != Some(100);
    let jamais_ouvert = jour(&b.opened).is_none()
        && b.open_count.unwrap_or(0) == 0
        && b.last_page_read.unwrap_or(0) == 0
        && b.current_page.unwrap_or(0) == 0;
    let seuil = aujourdhui - Duration::days(JOURS_NOUVEAU);
    let recent = jour(&b.added).is_some_and(|a| a > seuil && a <= aujourdhui);
    (non_lu && jamais_ouvert && recent, non_lu)
}

/// Calcule les jauges de toutes les listes en une passe sur la base. Fonction pure.
pub fn calculer(base: &ComicDatabase, membres: &HashMap<String, Members>, aujourdhui: NaiveDate) -> JaugesListes {
    let etats: Vec<(bool, bool)> = base.books.iter().map(|b| etat_livre(b, aujourdhui)).collect();
    let somme = |indices: &mut dyn Iterator<Item = usize>| {
        let mut j = Jauge::default();
        for i in indices {
            let (n, nl) = etats[i];
            j.total += 1;
            j.nouveaux += usize::from(n);
            j.non_lus += usize::from(nl);
        }
        j
    };
    let racine = somme(&mut (0..etats.len()));
    let mut listes = HashMap::with_capacity(membres.len());
    for (id, m) in membres {
        let j = match m {
            Members::All => racine,
            // Même règle que `nbLivres` : les identifiants absents de la base ne comptent pas.
            Members::Ids(ids) => somme(&mut ids.iter().filter_map(|i| base.index_of(i))),
            Members::Unknown(_) => continue,
        };
        listes.insert(id.clone(), j);
    }
    JaugesListes { racine, listes }
}

impl Biblio {
    /// Jauges de toutes les listes (en cache jusqu'au prochain changement de la base, de ses
    /// fiches, des listes ou du jour).
    pub fn jauges_listes(&self) -> Result<Arc<JaugesListes>, String> {
        self.rafraichir_listes(false);
        let (_, (generation, contenu)) = self.base_et_contenu().ok_or("aucune bibliothèque ouverte")?;
        let (_, version) = self.listes_lues();
        let jour = chrono::Local::now().date_naive();
        let cle = (generation, contenu, version, jour);
        if let Some((k, j)) = self.cache().jauges.as_ref() {
            if *k == cle {
                return Ok(j.clone());
            }
        }
        let (membres, base) = self.membres_toutes()?;
        let calcul = Arc::new(calculer(&base, &membres, jour));
        let mut c = self.cache();
        if c.version == version {
            c.jauges = Some((cle, calcul.clone()));
        }
        Ok(calcul)
    }
}
