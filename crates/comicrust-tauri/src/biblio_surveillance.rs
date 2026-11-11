//! Part de la bibliothèque dédiée à la surveillance de la base (module `surveillance`) : état de
//! connexion, instantané pour le sondage, application des modifications extérieures, signalement
//! des écritures de ComicRust. Sous-module de `biblio` : accès à son état interne.

use super::*;
use crate::surveillance::{ecrits_du_bilan, Connexion, EtatBase, SourceBase, MOTIF_HORS_LIGNE};
use comicrust_core::db::{Applied, Changes, WriteReport};

/// Ce que le sondage doit savoir de la bibliothèque, pris sous un seul verrou.
pub(crate) struct Instantane {
    pub generation: u64,
    pub source: SourceBiblio,
    pub base: Arc<ComicDatabase>,
    /// Analyse ou opération d'édition en cours : pas de sondage maintenant.
    pub occupe: bool,
}

impl Biblio {
    /// État de la connexion et écritures à signaler (partagé avec le fil de sondage).
    pub fn connexion(&self) -> &Connexion {
        &self.connexion
    }

    /// Raison pour laquelle la base ne peut pas être écrite parce qu'elle est injoignable
    /// (`None` : en ligne).
    pub fn motif_hors_ligne(&self) -> Option<String> {
        self.connexion.message_hors_ligne().map(|_| MOTIF_HORS_LIGNE.to_string())
    }

    /// État de la connexion, tel que `etat_base` et `base-etat` le donnent.
    pub fn etat_base(&self) -> EtatBase {
        let source = match self.source() {
            SourceBiblio::ComicrackMysql(_) => SourceBase::ComicrackMysql,
            SourceBiblio::Autonome(_) => SourceBase::Comicrust,
            SourceBiblio::ComicrackXml => SourceBase::ComicrackXml,
            SourceBiblio::Aucune => SourceBase::Aucune,
        };
        let message = self.connexion.message_hors_ligne();
        EtatBase {
            source,
            surveillee: matches!(source, SourceBase::ComicrackMysql | SourceBase::Comicrust),
            en_ligne: message.is_none(),
            message,
            essais: self.connexion.essais(),
        }
    }

    pub(crate) fn instantane_surveillance(&self) -> Option<Instantane> {
        let source = self.source();
        let g = self.verrou();
        Some(Instantane {
            generation: g.generation,
            source,
            base: g.base.clone()?,
            occupe: g.scan_en_cours || g.edition_en_cours,
        })
    }

    /// Applique des modifications faites hors de ComicRust (ComicRack, autre poste) à la
    /// bibliothèque affichée. `None` si la bibliothèque a été rechargée entre-temps
    /// (`generation` périmée : ces modifications sont déjà dans la base rechargée ou le seront au
    /// sondage suivant). Renvoie le bilan et les identifiants ajoutés ou modifiés.
    pub(crate) fn appliquer_changements(&self, generation: u64, ch: Changes) -> Option<(Applied, Vec<String>)> {
        let ids: Vec<String> = ch.updated.iter().map(|b| b.id.clone()).collect();
        let (bilan, anciens) = {
            let mut g = self.verrou();
            if g.generation != generation || g.scan_en_cours || g.edition_en_cours {
                return None;
            }
            let base = Arc::make_mut(g.base.as_mut()?);
            // Livres déjà présents : leur couverture a pu changer (fichier remplacé dans ComicRack).
            let anciens: Vec<(String, usize)> = ch
                .updated
                .iter()
                .filter(|b| base.get(&b.id).is_some())
                .map(|b| (b.id.clone(), b.front_cover_image()))
                .collect();
            let bilan = base.apply(ch);
            if bilan.added + bilan.modified + bilan.removed > 0 {
                g.contenu += 1;
                g.vue = None;
            }
            (bilan, anciens)
        };
        for (id, image) in &anciens {
            self.oublier_couverture(id, *image);
        }
        Some((bilan, ids))
    }

    /// Signale à la surveillance les fiches écrites par ComicRust dans la base MySQL de ComicRack
    /// (elles ne seront pas prises pour des modifications faites dans ComicRack).
    pub fn signaler_rapport_comicrack(&self, rapport: &WriteReport) {
        self.connexion.ecrit_mysql(ecrits_du_bilan(rapport));
    }

    /// Signale les fiches écrites dans la base de ComicRust (`compteur` : valeur renvoyée par
    /// l'écriture ; sans compteur, rien n'est signalé).
    pub fn signaler_ecriture_autonome(&self, ids: impl IntoIterator<Item = String>, compteur: Option<i64>) {
        if let Some(c) = compteur {
            self.connexion.ecrit_autonome(ids, c);
        }
    }
}
