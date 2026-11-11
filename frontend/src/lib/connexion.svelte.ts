// État de la connexion à la base (étape 6) : alimenté par l'événement `base-etat` du backend, qui
// sonde la base toutes les 5 s (docs/tauri/ipc-surveillance.md). Aucune logique métier : le backend
// décide de ce qui est refusé hors ligne ; ici on affiche l'état et on désactive les boutons
// d'écriture avec la raison.
import { ecouter, etatBase, messageErreur, reessayerBase, type EtatBase } from './api';

/** Raison affichée quand une écriture est impossible parce que la base est injoignable. */
export const MOTIF_HORS_LIGNE =
  "Base injoignable : modifications impossibles pour le moment. ComicRust réessaie automatiquement ; rien n'est perdu.";
/** Durée d'affichage du message « de nouveau joignable ». */
const DUREE_RETOUR_MS = 4000;

export class EtatConnexion {
  enLigne = $state(true);
  message = $state<string | undefined>(undefined);
  essais = $state(0);
  /** « Réessayer » en cours. */
  reessai = $state(false);
  /** Vrai quelques secondes après le retour de la base. */
  revenue = $state(false);
  /** Échec du bouton « Réessayer » lui-même (commande en erreur). */
  erreur = $state('');
  private desabonner?: () => void;
  private minuterie?: ReturnType<typeof setTimeout>;

  /** Raison de l'impossibilité d'écrire (`undefined` : en ligne). */
  get motifEcriture(): string | undefined {
    return this.enLigne ? undefined : MOTIF_HORS_LIGNE;
  }

  private appliquer(e: EtatBase): void {
    const retour = !this.enLigne && e.enLigne;
    this.enLigne = e.enLigne;
    this.message = e.message;
    this.essais = e.essais;
    if (retour) {
      this.revenue = true;
      clearTimeout(this.minuterie);
      this.minuterie = setTimeout(() => (this.revenue = false), DUREE_RETOUR_MS);
    } else if (!e.enLigne) {
      this.revenue = false;
    }
  }

  async demarrer(): Promise<void> {
    this.arreter();
    this.desabonner = await ecouter<EtatBase>('base-etat', (e) => this.appliquer(e));
    try {
      this.appliquer(await etatBase());
    } catch {
      /* état initial indisponible : l'événement suivant le donnera */
    }
  }

  arreter(): void {
    this.desabonner?.();
    this.desabonner = undefined;
    clearTimeout(this.minuterie);
  }

  /** Sondage immédiat (bouton « Réessayer »). */
  async reessayer(): Promise<void> {
    if (this.reessai) return;
    this.reessai = true;
    this.erreur = '';
    try {
      this.appliquer(await reessayerBase());
    } catch (e) {
      this.erreur = messageErreur(e);
    } finally {
      this.reessai = false;
    }
  }
}

/** Une seule connexion pour toute l'application (démarrée par App.svelte). */
export const connexion = new EtatConnexion();
