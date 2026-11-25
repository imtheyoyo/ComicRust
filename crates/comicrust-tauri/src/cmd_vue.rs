//! Rang de livres dans la vue affichée (tauri-v1.0.2, relecture du front B1).
//!
//! Quand la vue est relue (fiche modifiée, fin d'analyse, lecteur fermé…), l'ordre des livres peut
//! changer : le front garde la sélection **par identifiant** et doit retrouver le rang des livres
//! sélectionnés qui ne sont pas dans les pages déjà chargées. Lecture seule : aucune écriture, la
//! vue est celle de `lister_livres` (même cache, mêmes filtres : liste, recherche, pile ouverte).
//!
//! La logique est compilée et testée partout ; seule la commande IPC dépend de la feature `app`.

use crate::biblio::{Biblio, RequeteLivres};
use std::collections::HashMap;

/// Nombre maximal d'identifiants par demande (la sélection au-delà n'est pas suivie par le front).
pub const MAX_IDS_RANGS: usize = 10_000;

impl Biblio {
    /// Rang (à partir de 0) de chaque identifiant dans la vue décrite par `requete`, ou `None` s'il
    /// n'y figure pas (livre retiré, filtré par la recherche…). Même ordre que `ids`.
    pub fn rangs_dans_vue(&self, requete: &RequeteLivres, ids: &[String]) -> Result<Vec<Option<usize>>, String> {
        if ids.len() > MAX_IDS_RANGS {
            return Err(format!("trop d'identifiants ({} ; au plus {MAX_IDS_RANGS})", ids.len()));
        }
        let mut voulus: HashMap<&str, Option<usize>> = ids.iter().map(|i| (i.as_str(), None)).collect();
        if voulus.is_empty() {
            return Ok(Vec::new());
        }
        let (base, vue) = self.vue(requete)?;
        let mut restants = voulus.len();
        for (rang, &i) in vue.indices.iter().enumerate() {
            if let Some(place @ None) = voulus.get_mut(base.books[i].id.as_str()) {
                *place = Some(rang);
                restants -= 1;
                if restants == 0 {
                    break;
                }
            }
        }
        Ok(ids.iter().map(|i| voulus.get(i.as_str()).copied().flatten()).collect())
    }
}

/// Commande IPC `rangs_vue` : rangs des identifiants dans la vue (voir [`Biblio::rangs_dans_vue`]).
#[cfg(feature = "app")]
#[tauri::command(async)]
pub fn rangs_vue(
    biblio: tauri::State<'_, Biblio>,
    requete: RequeteLivres,
    ids: Vec<String>,
) -> Result<Vec<Option<usize>>, String> {
    biblio.rangs_dans_vue(&requete, &ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biblio::{Groupe, SensTri};
    use crate::reglages::Mode;
    use comicrust_core::db::ComicDatabase;
    use comicrust_core::library::LibraryBook;

    fn biblio(n: usize) -> (tempfile::TempDir, Biblio) {
        let d = tempfile::tempdir().unwrap();
        let b = Biblio::nouvelle(d.path().to_path_buf());
        let livres = (0..n)
            .map(|i| LibraryBook {
                id: format!("id-{i}"),
                series: Some(format!("Série {}", i % 7)),
                number: Some((i / 7 + 1).to_string()),
                ..Default::default()
            })
            .collect();
        b.installer(Mode::Autonome, ComicDatabase::from_books(livres), None);
        (d, b)
    }

    fn requete(tri: &str, recherche: Option<&str>) -> RequeteLivres {
        RequeteLivres {
            recherche: recherche.map(str::to_string),
            tri: tri.into(),
            sens_tri: SensTri::Asc,
            groupe: Some(Groupe::Serie),
            debut: 0,
            nombre: 0,
            liste: None,
            serie: None,
        }
    }

    /// Les rangs rendus sont ceux de `lister_livres` pour la même requête ; absent = `None`.
    #[test]
    fn rangs_identiques_a_la_liste() {
        let (_d, b) = biblio(300);
        let mut req = requete("serie", None);
        req.nombre = 300;
        let liste = b.lister(&req).unwrap();
        let ids: Vec<String> = [5usize, 0, 299, 150].iter().map(|&k| liste.livres[k].id.clone()).collect();
        let mut demande = ids.clone();
        demande.push("inconnu".into());
        let r = b.rangs_dans_vue(&req, &demande).unwrap();
        assert_eq!(r, vec![Some(5), Some(0), Some(299), Some(150), None]);
    }

    /// Recherche : un livre filtré n'a pas de rang ; liste vide ; trop d'identifiants refusés.
    #[test]
    fn filtre_vide_et_limite() {
        let (_d, b) = biblio(50);
        let req = requete("serie", Some("série 3"));
        let tous: Vec<String> = (0..50).map(|i| format!("id-{i}")).collect();
        let r = b.rangs_dans_vue(&req, &tous).unwrap();
        assert_eq!(r.iter().filter(|x| x.is_some()).count(), b.lister(&req).unwrap().total);
        assert!(r[3].is_some() && r[4].is_none(), "id-3 est de la série 3, id-4 non");
        assert!(b.rangs_dans_vue(&req, &[]).unwrap().is_empty());
        let trop: Vec<String> = (0..=MAX_IDS_RANGS).map(|i| i.to_string()).collect();
        assert!(b.rangs_dans_vue(&req, &trop).is_err());
    }
}
