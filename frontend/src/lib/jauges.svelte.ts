// Jauges des listes (docs/tauri/ipc-jauges.md) : nouveaux / non lus / total de chaque liste, comme
// ComicRack. Aucune logique métier : les comptes viennent du backend (`jauges_listes`), ici seulement
// l'abonnement aux changements (événements, lecture finie, listes relues), le regroupement des
// demandes rapprochées et la préférence « Afficher les jauges des listes » (activée par défaut).
import { ecouter, jaugesListes, type BibliothequeModifiee, type Jauge, type JaugesListes } from './api';
import { enregistrerPreference, preferences } from './prefs';

/** Demandes rapprochées (rafale d'événements, relecture de l'arbre) regroupées en une seule. */
export const DELAI_JAUGES_MS = 300;

/** Intervalle du contrôle du changement de jour local (minuit passé fenêtre ouverte). */
export const INTERVALLE_JOUR_MS = 60_000;

/** Date locale `AAAA-MM-JJ` (la fenêtre « nouveau » glisse avec le jour). */
export function jourLocal(d = new Date()): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

const pluriel = (n: number, singulier: string, plurielTexte: string) => (n > 1 ? plurielTexte : singulier);

/** Texte alternatif d'une jauge : « 12 nouveaux, 40 non lus, 120 livres » (accord au singulier). */
export function libelleJauge(j: Jauge): string {
  return [
    `${j.nouveaux} ${pluriel(j.nouveaux, 'nouveau', 'nouveaux')}`,
    `${j.nonLus} ${pluriel(j.nonLus, 'non lu', 'non lus')}`,
    `${j.total} ${pluriel(j.total, 'livre', 'livres')}`,
  ].join(', ');
}

class EtatJauges {
  /** Dernières jauges reçues (null : pas encore calculées ou option désactivée). */
  donnees = $state<JaugesListes | null>(null);
  /** Option « Afficher les jauges des listes » (mémorisée dans `vue.jaugesListes`, activée par défaut). */
  actif = $state(true);

  private minuterie: ReturnType<typeof setTimeout> | undefined;
  private numero = 0;
  private abonne = false;
  /** Jour local du dernier calcul reçu. */
  private jourCalcul = '';
  private veilleJour: ReturnType<typeof setInterval> | undefined;
  private surVisibilite = () => {
    if (document.visibilityState === 'visible') this.verifierJour();
  };
  private desabonnements: Array<() => void> = [];

  /** Demande des jauges à jour (regroupée) ; sans effet si l'option est désactivée. Le premier appel
   *  lit la préférence mémorisée (déjà chargée au démarrage) et s'abonne aux événements. */
  rafraichir(): void {
    if (!this.abonne) this.demarrer();
    if (!this.actif) return;
    clearTimeout(this.minuterie);
    this.minuterie = setTimeout(() => void this.charger(), DELAI_JAUGES_MS);
  }

  /** Active ou désactive l'affichage (case des Réglages) et mémorise le choix. */
  activer(oui: boolean): void {
    this.actif = oui;
    enregistrerPreference({ jaugesListes: oui });
    clearTimeout(this.minuterie);
    if (oui) {
      void this.charger();
    } else {
      this.numero++; // une réponse en vol ne doit plus s'afficher
      this.donnees = null;
    }
  }

  /** Jauge d'une liste (undefined : option désactivée, jauges pas encore reçues ou contenu inconnu). */
  de(id: string): Jauge | undefined {
    return this.actif ? this.donnees?.listes[id] : undefined;
  }

  /** Jauge de la bibliothèque entière. */
  get racine(): Jauge | undefined {
    return this.actif ? (this.donnees?.racine ?? undefined) : undefined;
  }

  private demarrer(): void {
    this.abonne = true;
    this.actif = preferences().jaugesListes !== false;
    // Lecture, marquer lu / non lu, édition, scripts, suivi externe, manquants : tout passe par là.
    void ecouter<BibliothequeModifiee>('bibliotheque-modifiee', () => this.rafraichir()).then((f) => this.desabonnements.push(f));
    // Changement de jour local (minuit) ou retour de la fenêtre : « nouveau » dépend de la date.
    this.veilleJour = setInterval(() => this.verifierJour(), INTERVALLE_JOUR_MS);
    document.addEventListener('visibilitychange', this.surVisibilite);
    // Analyse terminée : livres ajoutés ou retirés.
    void ecouter<unknown>('scan-termine', () => this.rafraichir()).then((f) => this.desabonnements.push(f));
  }

  /** Redemande les jauges si la date locale a changé depuis le dernier calcul. */
  verifierJour(): void {
    if (this.actif && this.jourCalcul && this.jourCalcul !== jourLocal()) this.rafraichir();
  }

  /** Arrêt : retire les abonnements, la minuterie et l'écoute de visibilité. */
  arreter(): void {
    clearTimeout(this.minuterie);
    clearInterval(this.veilleJour);
    document.removeEventListener('visibilitychange', this.surVisibilite);
    this.desabonnements.splice(0).forEach((f) => f());
    this.abonne = false;
  }

  private async charger(): Promise<void> {
    const n = ++this.numero;
    const jour = jourLocal(); // pris avant l'appel : un minuit pendant le calcul sera revu
    try {
      const j = await jaugesListes();
      if (n === this.numero && this.actif) {
        this.donnees = j;
        this.jourCalcul = jour;
      }
    } catch (e) {
      // Pas de jauges (aucune bibliothèque ouverte, hors ligne…) : on garde les dernières, sans bruit.
      console.warn('jauges des listes indisponibles', e);
    }
  }
}

export const jauges = new EtatJauges();
