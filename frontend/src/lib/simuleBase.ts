// MODE SIMULÉ : surveillance de la base (étape 6, docs/tauri/ipc-surveillance.md). Imite les
// commandes `etat_base` / `reessayer_base` et l'événement `base-etat`.
//  - `?horsLigne` : la base est injoignable dès le départ ;
//  - `window.__comicrustBase.perdre()` / `.rendre()` : bascule en cours de route (tests) ;
//    `reessayer_base` revient en ligne seulement si `rendre()` a été appelé (la base « répond »).
import type { EtatBase, ModeBiblio } from './api';

export const MOTIF_HORS_LIGNE_SIMULE =
  "Base injoignable : modifications impossibles pour le moment. ComicRust réessaie automatiquement ; rien n'est perdu (la lecture reste mémorisée sur ce poste).";

export function creerBaseSimulee(emettre: (nom: string, charge: unknown) => void, mode: () => ModeBiblio) {
  let perdue = new URLSearchParams(location.search).has('horsLigne');
  let repondra = !perdue;
  let essais = perdue ? 1 : 0;

  const etat = (): EtatBase => ({
    source: mode() === 'comicrack' ? 'comicrack-mysql' : 'comicrust',
    surveillee: true,
    enLigne: !perdue,
    message: perdue ? 'Impossible de se connecter au serveur MySQL (délai dépassé).' : undefined,
    essais: perdue ? essais : 0,
  });
  const signaler = () => emettre('base-etat', etat());

  const controle = {
    perdre() { perdue = true; repondra = false; essais = 1; signaler(); },
    rendre() { repondra = true; },
    /** Le sondage constate le retour de la base (ou un nouvel échec). */
    sonder() {
      if (perdue && repondra) { perdue = false; essais = 0; signaler(); }
      else if (perdue) { essais++; }
    },
  };
  (globalThis as { __comicrustBase?: typeof controle }).__comicrustBase = controle;

  return {
    /** Raison d'impossibilité d'écrire, hors ligne. */
    motif: (): string | undefined => (perdue ? MOTIF_HORS_LIGNE_SIMULE : undefined),
    appeler(commande: string): unknown {
      switch (commande) {
        case 'etat_base': return etat();
        case 'reessayer_base': controle.sonder(); return etat();
        default: return undefined;
      }
    },
  };
}
