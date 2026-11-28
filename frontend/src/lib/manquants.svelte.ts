// Livres manquants (lot L3, docs/tauri/ipc-manquants.md) : état partagé par la barre latérale, la
// vue, le Détail et les fenêtres. Aucune logique métier : détection, garde-fous (volume injoignable,
// disparition massive), règles d'écriture et confirmations viennent du backend.
import {
  appliquerRelocalisations, arreterManquants, chercherManquants, ecouter, etatManquants, messageErreur, preparerRetrait,
  proposerFichier, retirerLivres, sonderLivre, verifierManquants,
  type BibliothequeModifiee, type EtatManquants, type NoeudListe, type PreparationRetrait, type PropositionManquant,
  type SondeLivre,
} from './api';

/** Identifiant de la liste virtuelle « Livres manquants » (même valeur que le backend). */
export const ID_MANQUANTS = '@manquants';

/**
 * Point d'extension : sélecteurs natifs (fichier, dossier). Aucun plugin dialog aujourd'hui : les
 * chemins sont saisis. Un lot futur renseigne ces fonctions (elles reçoivent le chemin courant et
 * rendent le chemin choisi, ou null) : les fenêtres affichent alors un bouton « Parcourir… ».
 */
export const selecteurs: {
  fichier?: (initial: string) => Promise<string | null>;
  dossier?: (initial: string) => Promise<string | null>;
} = {};

/** Nœud de la liste virtuelle, tel que la vue l'attend (liste intelligente : ni dépôt ni ordre). */
export const NOEUD_MANQUANTS: NoeudListe = {
  id: ID_MANQUANTS,
  nom: 'Livres manquants',
  type: 'intelligente',
  description: 'Livres dont le fichier est introuvable (déplacé, supprimé, renommé)',
  nbComicrack: 0,
  accepteLivres: false,
  peutMonter: false,
  peutDescendre: false,
};

export interface FenetreRetrouver {
  id: string;
  legende: string;
  chemin: string;
  erreur: string;
  occupe: boolean;
}
export interface FenetreDossier {
  ids: string[];
  dossier: string;
  erreur: string;
  occupe: boolean;
}
export interface FenetreRelocalisation {
  source: string;
  propositions: Array<PropositionManquant & { coche: boolean }>;
  nonRetrouves: number;
  erreur: string;
  occupe: boolean;
}
export interface FenetreRetrait {
  ids: string[];
  prep: PreparationRetrait | null;
  confirme: boolean;
  erreur: string;
  occupe: boolean;
}

class Manquants {
  etat = $state<EtatManquants | null>(null);
  /** La liste « Livres manquants » est affichée dans la vue. */
  affiche = $state(false);
  /** Fenêtre « Fichier introuvable » (ouverture d'un livre manquant ou volume injoignable). */
  introuvable = $state<{ sonde: SondeLivre; attente: boolean } | null>(null);
  retrouver = $state<FenetreRetrouver | null>(null);
  dossier = $state<FenetreDossier | null>(null);
  relocalisation = $state<FenetreRelocalisation | null>(null);
  retrait = $state<FenetreRetrait | null>(null);
  /** Message d'information (résultat d'une opération). */
  info = $state('');
  /** Branché par l'application : relire les listes (comptes) après une modification de fiches. */
  surModification: () => void = () => {};
  /** Branché par l'application : quitte la vue des manquants (retour à toute la bibliothèque). */
  surRetour: () => void = () => {};
  /** Branché par la vue : rouvre le livre `id` (bouton « Réessayer » du volume injoignable). */
  surRouvrir: (id: string) => void = () => {};

  private minuterie: ReturnType<typeof setInterval> | undefined;
  private desabonner: (() => void) | undefined;
  /** Jeton des démarrages / arrêts (voir `demarrer`). */
  private demarrages = 0;

  get nombre(): number {
    return this.etat?.nbManquants ?? 0;
  }
  get verification() {
    return this.etat?.verification;
  }
  /** Raison pour laquelle les fiches ne sont pas modifiables (undefined : possibles). */
  get raison(): string | undefined {
    return this.etat?.raisonLectureSeule;
  }
  /** Raison pour laquelle les fichiers des livres ne sont pas modifiables (D1) : texte du backend
   *  propre aux fichiers, sinon la raison des fiches. */
  get raisonFichiers(): string | undefined {
    return this.etat?.raisonFichiers ?? this.etat?.raisonLectureSeule;
  }
  /** Volumes injoignables ou blocages de la dernière vérification (avertissement discret). */
  get avertissements(): string[] {
    const d = this.etat?.derniere;
    if (!d) return [];
    return [
      ...d.injoignables.map((u) => `⚠ ${u.racine} inaccessible : ${u.livres} livre${u.livres > 1 ? 's' : ''} non vérifié${u.livres > 1 ? 's' : ''}.`),
      ...d.bloques.map((b) => `⚠ ${b.racine} : ${b.disparus} livres sur ${b.livres} auraient disparu d'un coup ; non marqués (partage partiellement monté ?).`),
    ];
  }

  async rafraichir(): Promise<void> {
    try {
      this.etat = await etatManquants();
    } catch {
      /* état indisponible un instant : on réessaiera */
    }
  }

  /** Démarre le suivi (comptes) ; `verifier` : vérification automatique (bibliothèque ComicRack, comme
   *  la version egui ; la base de ComicRust est tenue à jour par l'analyse des dossiers). */
  async demarrer(verifier: boolean): Promise<void> {
    this.arreter();
    const n = this.demarrages;
    const desabonner = await ecouter<BibliothequeModifiee>('bibliotheque-modifiee', () => {
      void this.rafraichir();
    });
    // Deux démarrages rapprochés (ou un arrêt pendant l'abonnement) : seul le dernier garde son
    // écoute, l'autre est retirée aussitôt (sinon jamais désabonnée, relecture B8).
    if (n !== this.demarrages) {
      desabonner();
      return;
    }
    this.desabonner = desabonner;
    await this.rafraichir();
    if (verifier) void this.verifier();
  }

  arreter(): void {
    this.demarrages++;
    this.desabonner?.();
    this.desabonner = undefined;
    clearInterval(this.minuterie);
  }

  afficher(): void {
    this.affiche = true;
  }

  masquer(): void {
    this.affiche = false;
  }

  /** « Toute la bibliothèque » : l'application revient à la vue de la bibliothèque. */
  retourBibliotheque(): void {
    this.surRetour();
  }

  /** « Vérifier les fichiers » : avancement relu toutes les 400 ms, bilan dans l'état. */
  async verifier(): Promise<void> {
    if (this.etat?.verification) return;
    clearInterval(this.minuterie);
    this.minuterie = setInterval(() => void this.rafraichir(), 400);
    // L'avancement apparaît tout de suite, sans attendre le premier relevé.
    if (this.etat) this.etat = { ...this.etat, verification: { fait: 0, total: 0 } };
    try {
      await verifierManquants();
    } catch (e) {
      this.info = `Vérification des fichiers : ${messageErreur(e)}`;
    } finally {
      clearInterval(this.minuterie);
      await this.rafraichir();
      this.surModification();
    }
  }

  arreterTravail(): void {
    void arreterManquants();
  }

  // ───── Ouverture d'un livre ─────

  /** Examine le fichier avant l'ouverture ; faux si la fenêtre « Fichier introuvable » est affichée. */
  async peutOuvrir(id: string): Promise<boolean> {
    let sonde: SondeLivre;
    try {
      sonde = await sonderLivre(id);
    } catch {
      return true; // l'ouverture dira elle-même ce qui ne va pas
    }
    if (sonde.etat === 'present') {
      if (this.introuvable?.sonde.id === id) this.introuvable = null;
      return true;
    }
    this.introuvable = { sonde, attente: false };
    void this.rafraichir();
    if (sonde.etat === 'introuvable') this.surModification();
    return false;
  }

  /** « Réessayer » (volume injoignable) : le livre s'ouvre si le disque répond, sinon la fenêtre le redit. */
  async reessayer(): Promise<void> {
    const f = this.introuvable;
    if (!f) return;
    this.introuvable = { ...f, attente: true };
    try {
      const s = await sonderLivre(f.sonde.id);
      if (s.etat === 'present') {
        this.introuvable = null;
        this.surRouvrir(s.id);
      } else {
        this.introuvable = { sonde: s, attente: false };
        void this.rafraichir();
      }
    } catch (e) {
      this.introuvable = null;
      this.info = messageErreur(e);
    }
  }

  fermerIntrouvable(): void {
    this.introuvable = null;
  }

  // ───── Retrouver le fichier ─────

  demanderRetrouver(id: string, legende: string): void {
    this.introuvable = null;
    this.retrouver = { id, legende, chemin: '', erreur: '', occupe: false };
  }

  /** Examine le fichier saisi (taille, doublon) puis ouvre la confirmation. */
  async validerRetrouver(): Promise<void> {
    const f = this.retrouver;
    if (!f || f.occupe || !f.chemin.trim()) return;
    f.occupe = true;
    f.erreur = '';
    try {
      const p = await proposerFichier(f.id, f.chemin);
      this.retrouver = null;
      this.relocalisation = { source: 'fichier choisi', propositions: [{ ...p }], nonRetrouves: 0, erreur: '', occupe: false };
    } catch (e) {
      f.erreur = messageErreur(e);
      f.occupe = false;
    }
  }

  // ───── Chercher dans un dossier ─────

  demanderDossier(ids: string[]): void {
    this.introuvable = null;
    this.dossier = { ids, dossier: '', erreur: '', occupe: false };
  }

  async validerDossier(): Promise<void> {
    const f = this.dossier;
    if (!f || f.occupe || !f.dossier.trim()) return;
    f.occupe = true;
    f.erreur = '';
    this.minuterie = setInterval(() => void this.rafraichir(), 400);
    try {
      const r = await chercherManquants(f.ids, f.dossier);
      this.dossier = null;
      if (r.interrompu) return;
      if (r.propositions.length === 0) {
        this.info = r.info ?? `Aucun fichier retrouvé dans ${r.dossier}.`;
      } else {
        this.relocalisation = {
          source: `${r.dossier} (${r.fichiersExamines.toLocaleString('fr-FR')} fichiers examinés)`,
          propositions: r.propositions.map((p) => ({ ...p })),
          nonRetrouves: r.nonRetrouves,
          erreur: '',
          occupe: false,
        };
      }
    } catch (e) {
      f.erreur = messageErreur(e);
      f.occupe = false;
    } finally {
      clearInterval(this.minuterie);
      void this.rafraichir();
    }
  }

  /** Confirmation : les fiches cochées pointent vers leur nouveau fichier. */
  async validerRelocalisation(): Promise<void> {
    const f = this.relocalisation;
    if (!f || f.occupe) return;
    const demandes = f.propositions.filter((p) => p.coche).map((p) => ({ id: p.id, ancien: p.ancien, nouveau: p.nouveau }));
    if (demandes.length === 0) return;
    f.occupe = true;
    f.erreur = '';
    try {
      const r = await appliquerRelocalisations(demandes);
      this.relocalisation = null;
      this.info = r.info;
      await this.rafraichir();
      this.surModification();
    } catch (e) {
      f.erreur = messageErreur(e);
      f.occupe = false;
    }
  }

  // ───── Retirer de la bibliothèque ─────

  /** Ouvre la fenêtre de confirmation : le backend vérifie les fichiers (un NAS éteint fait attendre). */
  async demanderRetrait(ids: string[]): Promise<void> {
    this.introuvable = null;
    if (ids.length === 0) return;
    const f: FenetreRetrait = $state({ ids, prep: null, confirme: false, erreur: '', occupe: true });
    this.retrait = f;
    try {
      const prep = await preparerRetrait(ids);
      if (this.retrait !== f) return;
      f.prep = prep;
      f.ids = prep.ids;
    } catch (e) {
      if (this.retrait !== f) return;
      // Refus (lecture seule, base injoignable…) : message simple, aucune fenêtre de confirmation.
      this.retrait = null;
      this.info = messageErreur(e);
      return;
    }
    f.occupe = false;
  }

  async validerRetrait(): Promise<void> {
    const f = this.retrait;
    if (!f || f.occupe || !f.prep) return;
    f.occupe = true;
    f.erreur = '';
    try {
      const r = await retirerLivres(f.ids, f.confirme);
      this.retrait = null;
      this.info = r.info;
      await this.rafraichir();
      this.surModification();
    } catch (e) {
      f.erreur = messageErreur(e);
      f.occupe = false;
    }
  }
}

export const manquants = new Manquants();
