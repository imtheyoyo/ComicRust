// MODE SIMULÉ (lot L2) : faux backend de la fenêtre et du lancement (docs/tauri/ipc-fenetre.md),
// utilisé seulement hors de Tauri pour développer et tester l'interface.
//
// Paramètres d'adresse :
// - `?ouvrir=<chemin>` : chemin « passé en argument » au lancement (rendu une seule fois) ;
// - `?plantage` : un rapport de plantage de la session précédente existe (rendu jusqu'à son acquittement).
// Un dépôt de fichiers est simulé par `window.dispatchEvent(new CustomEvent('comicrust-simule-depot',
// { detail: ['D:\\BD\\x.cbz'] }))` (survol : événement `comicrust-simule-survol`, détail booléen).
// `window.__comicrustSimule.fenetre` expose les traces pour les tests.
import type { GenreChemin, LancementInitial, RapportPlantage } from './api';

/** Même règle que `fenetre::classer_chemin`, sur le seul texte du chemin (rien n'existe vraiment). */
export function genreSimule(chemin: string): GenreChemin {
  const c = chemin.trim().replace(/^"|"$/g, '');
  if (/absent|introuvable/i.test(c)) return 'absent';
  if (/\.cbl$/i.test(c)) return 'liste';
  const dernier = c.split(/[\\/]/).filter(Boolean).pop() ?? '';
  if (/[\\/]$/.test(c) || !dernier.includes('.')) return 'dossier';
  return 'livre';
}

export const COMMANDES_FENETRE = ['definir_titre_fenetre', 'lancement_initial', 'classer_chemin', 'lire_plantage', 'acquitter_plantage'];

export interface TracesFenetre {
  /** Titres de fenêtre demandés, dans l'ordre. */
  titres: string[];
  plantageAcquitte: boolean;
  /** Chemins d'argument déjà rendus. */
  lancements: string[];
}

export function creerFenetreSimulee() {
  const url = new URLSearchParams(location.search);
  let initial = url.get('ouvrir');
  const traces: TracesFenetre = { titres: [], plantageAcquitte: false, lancements: [] };
  const rapport: RapportPlantage = {
    texte: [
      'ComicRust-Tauri v0.0.0-simule — 2026-11-21 10:00:00',
      'Fil : main',
      "panicked at crates/comicrust-tauri/src/exemple.rs:12:5:",
      'plantage simulé',
      '',
      '   0: exemple::fonction\n   1: exemple::main',
    ].join('\n'),
    resume: ['ComicRust-Tauri v0.0.0-simule — 2026-11-21 10:00:00', 'Fil : main', 'panicked at crates/comicrust-tauri/src/exemple.rs:12:5:', 'plantage simulé'].join('\n'),
    fichier: '(mode simulé) data/plantage.txt',
  };
  const g = globalThis as unknown as { __comicrustSimule?: Record<string, unknown> };
  g.__comicrustSimule ??= {};
  g.__comicrustSimule.fenetre = traces;

  return {
    appeler(commande: string, args: Record<string, unknown>): unknown {
      switch (commande) {
        case 'definir_titre_fenetre': {
          const t = typeof args.titre === 'string' ? args.titre.trim() : '';
          const titre = t ? `${t} — ComicRust` : 'ComicRust';
          traces.titres.push(titre);
          document.title = titre;
          return null;
        }
        case 'lancement_initial': {
          if (!initial) return null;
          const l: LancementInitial = { chemin: initial, genre: genreSimule(initial) };
          traces.lancements.push(initial);
          initial = null;
          return l;
        }
        case 'classer_chemin':
          return genreSimule(String(args.chemin ?? ''));
        case 'lire_plantage':
          return url.has('plantage') && !traces.plantageAcquitte ? rapport : null;
        case 'acquitter_plantage':
          traces.plantageAcquitte = true;
          return null;
        default:
          return undefined;
      }
    },
  };
}
