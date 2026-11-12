// Mise à jour de ComicRust (tauri-v0.8.0) : état partagé par le bandeau et la fenêtre « Mise à
// jour ». Alimenté par `maj_etat` puis par les événements `maj://statut` et `maj://progression`
// (docs/tauri/ipc-maj.md). Aucune logique métier : le backend vérifie, télécharge, contrôle le
// SHA-256 et installe ; ici on affiche et on transmet les demandes. Interface de référence : le
// bandeau et la fenêtre de la version egui (crates/comicrust/src/ui/update_ui.rs).
import {
  ecouterMaj, majEtat, majInstaller, majRedemarrer, majRegler, majVerifier, messageErreur,
  type EtatMaj, type ProgressionMaj, type ReleaseMaj, type StatutMaj,
} from './api';

export class EtatMiseAJour {
  /** Réglages et version (null tant que `maj_etat` n'a pas répondu). */
  etat = $state<EtatMaj | null>(null);
  statut = $state<StatutMaj>({ etat: 'inactif', manuel: false });
  progression = $state<ProgressionMaj | null>(null);
  /** « Plus tard » : bandeau masqué jusqu'au prochain lancement (non mémorisé). */
  plusTard = $state(false);
  /** Fenêtre « Mise à jour » ouverte. */
  fenetre = $state(false);
  /** Échec d'une commande elle-même (refus immédiat du backend). */
  erreurCommande = $state('');
  private desabonner?: () => void;

  /** Release proposée par le dernier statut (disponible, installation, installée, échec d'installation). */
  get release(): ReleaseMaj | undefined {
    return this.statut.release;
  }

  /** Bandeau : version disponible, ni ignorée ni repoussée à plus tard. */
  get bandeau(): boolean {
    const r = this.statut.release;
    return this.statut.etat === 'disponible' && !!r && !this.plusTard && this.etat?.versionIgnoree !== r.tag;
  }

  /** Statut à montrer dans la fenêtre : un résultat « à jour » ou « échec » d'une vérification
   *  AUTOMATIQUE n'est jamais affiché (règle du contrat) ; la fenêtre montre alors l'état neutre. */
  get statutAffiche(): StatutMaj {
    const s = this.statut;
    if (!s.manuel && (s.etat === 'aJour' || (s.etat === 'echec' && !s.release))) return { etat: 'inactif', manuel: false };
    return s;
  }

  /** Vérification ou installation en cours. */
  get occupe(): boolean {
    return this.statut.etat === 'verification' || this.statut.etat === 'installation';
  }

  private appliquer(s: StatutMaj): void {
    if (s.etat === 'installation' && this.statut.etat !== 'installation') {
      this.progression = { recu: 0, total: s.release?.taille ?? 0 };
    }
    // Une nouvelle version trouvée par une vérification manuelle réaffiche le bandeau.
    if (s.etat === 'disponible' && s.manuel) this.plusTard = false;
    this.statut = s;
    if (this.etat) this.etat = { ...this.etat, statut: s };
  }

  /** Abonnement aux événements, état initial et, si demandé, vérification automatique. */
  async demarrer(): Promise<void> {
    this.arreter();
    try {
      this.desabonner = await ecouterMaj((s) => this.appliquer(s), (p) => (this.progression = p));
      const e = await majEtat();
      this.etat = e;
      // Statut déjà connu (vérification lancée par le backend avant l'abonnement) : on le garde.
      if (this.statut.etat === 'inactif') this.appliquer(e.statut);
      // Rien de lancé côté backend : vérification automatique (résultat silencieux sauf
      // « disponible », qui n'affiche que le bandeau).
      if (e.verifierAuDemarrage && this.statut.etat === 'inactif') await majVerifier(false);
    } catch (err) {
      // Mise à jour indisponible : jamais bloquant pour le reste de l'application.
      console.warn('mise à jour indisponible', err);
    }
  }

  arreter(): void {
    this.desabonner?.();
    this.desabonner = undefined;
  }

  ouvrir(): void {
    this.erreurCommande = '';
    this.fenetre = true;
    if (!this.etat) void majEtat().then((e) => (this.etat = e)).catch((err) => (this.erreurCommande = messageErreur(err)));
  }

  fermer(): void {
    this.fenetre = false;
  }

  private async commande(f: () => Promise<unknown>): Promise<void> {
    this.erreurCommande = '';
    try {
      await f();
    } catch (err) {
      this.erreurCommande = messageErreur(err);
    }
  }

  /** « Rechercher maintenant » (vérification manuelle : le résultat est toujours affiché). */
  verifier(): Promise<void> {
    return this.commande(() => majVerifier(true));
  }

  installer(): Promise<void> {
    const r = this.statut.release;
    if (!r) return Promise.resolve();
    return this.commande(() => majInstaller(r.tag));
  }

  redemarrer(): Promise<void> {
    return this.commande(() => majRedemarrer());
  }

  /** Case « Vérifier les mises à jour au démarrage ». */
  reglerDemarrage(actif: boolean): Promise<void> {
    return this.commande(async () => (this.etat = await majRegler({ verifierAuDemarrage: actif })));
  }

  /** « Ignorer cette version » : plus de bandeau pour cette version, même après relance. */
  ignorer(): Promise<void> {
    const r = this.statut.release;
    if (!r) return Promise.resolve();
    return this.commande(async () => (this.etat = await majRegler({ versionIgnoree: r.tag })));
  }

  /** Ne plus ignorer de version (lien de la fenêtre). */
  neplusIgnorer(): Promise<void> {
    return this.commande(async () => (this.etat = await majRegler({ versionIgnoree: null })));
  }
}

/** Une seule instance pour l'application (démarrée par App.svelte). */
export const miseAJour = new EtatMiseAJour();

/** Octets en mégaoctets lisibles (« 12,3 Mo »), comme la barre de la version egui. */
export const enMo = (o: number): string =>
  `${(o / 1e6).toLocaleString('fr-FR', { minimumFractionDigits: 1, maximumFractionDigits: 1 })} Mo`;
