//! Part de la bibliothèque dédiée aux livres manquants (module `manquants`) : marques « fichier
//! introuvable » posées en mémoire et retrait de fiches de la bibliothèque affichée. Sous-module
//! de `biblio` : accès à son état interne (même schéma que `biblio_surveillance`).

use super::*;

/// Résultat de vérification d'un livre : identifiant, chemin de la fiche **vérifié** (le résultat
/// ne vaut que pour lui), fichier absent (`true`) ou présent (`false`).
pub(crate) type Marque = (String, String, bool);

impl Biblio {
    /// Pose les marques en mémoire, seulement pour les fiches qui ont encore le chemin vérifié
    /// (fiche déplacée ou renommée pendant une longue vérification : jamais marquée). Un livre
    /// retrouvé n'est « présent » que s'il était marqué manquant (`Some(false)`) : autrement la
    /// valeur de la base est gardée. Renvoie les identifiants dont la fiche a changé.
    ///
    /// La base n'est pas écrite ici (voir `Manquants::appliquer_resultat`) ; la vue et le contenu
    /// calculé des listes sont invalidés si quelque chose a changé.
    pub(crate) fn marquer_manquants(&self, marques: &[Marque]) -> Vec<String> {
        let mut g = self.verrou();
        // Pendant une analyse, la base est partagée avec elle (`Arc::ptr_eq` à la fin) : ne pas la cloner ;
        // l'analyse recalcule de toute façon les marques « manquant ».
        if g.scan_en_cours {
            return Vec::new();
        }
        let Some(base) = g.base.as_mut() else { return Vec::new() };
        let a_changer: Vec<(usize, Option<bool>)> = marques
            .iter()
            .filter_map(|(id, fichier, manquant)| {
                let i = base.index_of(id)?;
                let b = &base.books[i];
                if b.file.as_deref() != Some(fichier.as_str()) {
                    return None;
                }
                let voulu = match (*manquant, b.missing) {
                    (true, _) => Some(true),
                    (false, Some(true)) => Some(false),
                    (false, autre) => autre,
                };
                (b.missing != voulu).then_some((i, voulu))
            })
            .collect();
        if a_changer.is_empty() {
            return Vec::new();
        }
        let db = Arc::make_mut(base);
        let mut ids = Vec::with_capacity(a_changer.len());
        for (i, voulu) in a_changer {
            db.books[i].missing = voulu;
            ids.push(db.books[i].id.clone());
        }
        g.contenu += 1;
        g.vue = None;
        ids
    }

    /// Retire des fiches de la bibliothèque affichée (déjà supprimées de la base : « Retirer de
    /// la bibliothèque »). Renvoie le nombre de fiches retirées. Le contenu calculé des listes
    /// est invalidé : un livre retiré disparaît des listes de lecture (identifiant inconnu).
    pub(crate) fn retirer_fiches_memoire(&self, ids: &[String]) -> usize {
        let mut g = self.verrou();
        let Some(base) = g.base.as_mut() else { return 0 };
        let n = Arc::make_mut(base).remove(ids);
        if n > 0 {
            g.contenu += 1;
            g.vue = None;
        }
        n
    }
}
