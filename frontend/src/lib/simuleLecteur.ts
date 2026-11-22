// MODE SIMULÉ (lot L6) : faux backend des fichiers récents (docs/tauri/ipc-lecteur.md), utilisé
// seulement hors de Tauri pour développer et tester l'interface. Même règle que
// `reglages::pousser_recent` : le plus récent d'abord, sans doublon, 15 au plus.
//
// Paramètre d'adresse : `?recents=<chemin>;<chemin>` : liste de départ (le plus récent d'abord).
// Un chemin contenant « absent », « introuvable » ou « fantome » est un fichier disparu (`existe`
// faux). `window.__comicrustSimule.recents` expose la liste pour les tests.
import type { RecentFichier } from './api';

export const COMMANDES_RECENTS = ['lire_recents', 'ajouter_recent', 'retirer_recent', 'vider_recents'];

const MAX = 15;

const nomDe = (chemin: string) => chemin.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || chemin;
const existe = (chemin: string) => !/absent|introuvable|fantome/i.test(chemin);

export function creerRecentsSimules() {
  const depart = (new URLSearchParams(location.search).get('recents') ?? '')
    .split(';')
    .map((c) => c.trim())
    .filter(Boolean);
  let liste: string[] = [...new Set(depart)].slice(0, MAX);
  const g = globalThis as unknown as { __comicrustSimule?: Record<string, unknown> };
  g.__comicrustSimule ??= {};
  const vue = (): RecentFichier[] => liste.map((chemin) => ({ chemin, nom: nomDe(chemin), existe: existe(chemin) }));
  const publier = () => {
    g.__comicrustSimule!.recents = liste.slice();
  };
  publier();

  return {
    appeler(commande: string, args: Record<string, unknown>): unknown {
      const chemin = typeof args.chemin === 'string' ? args.chemin.trim() : '';
      switch (commande) {
        case 'lire_recents':
          break;
        case 'ajouter_recent':
          if (chemin && liste[0] !== chemin) liste = [chemin, ...liste.filter((c) => c !== chemin)].slice(0, MAX);
          break;
        case 'retirer_recent':
          liste = liste.filter((c) => c !== chemin);
          break;
        case 'vider_recents':
          liste = [];
          break;
        default:
          return undefined;
      }
      publier();
      return vue();
    },
  };
}
