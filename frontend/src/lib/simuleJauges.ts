// MODE SIMULÉ (lot jauges) : faux calcul des jauges des listes (docs/tauri/ipc-jauges.md), utilisé
// seulement hors de Tauri. Mêmes règles que le backend : total = livres de la liste (manquants
// compris), non lus = pas « lu », nouveaux = non lus jamais ouverts parmi les derniers ajoutés
// (ici : les 12 derniers de la bibliothèque simulée, l'ajout simulé étant un simple rang).
// `window.__comicrustSimule.jauges` expose aux tests le nombre d'appels reçus.
import type { Jauge, JaugesListes } from './api';
import type { LivreListe } from './simuleListes';

/** Nombre de livres les plus récemment ajoutés qui comptent comme « nouveaux » (simulation). */
export const DERNIERS_AJOUTS = 12;

/** Contenu d'une liste simulée : identifiants des livres, ou contenu inconnu. */
export type MembresSimules = { ids: string[] } | { inconnu: string };

const exposition = { appels: 0 };

function jaugeDe(livres: LivreListe[], seuil: number): Jauge {
  const j: Jauge = { nouveaux: 0, nonLus: 0, total: livres.length };
  for (const l of livres) {
    if (l.lu) continue;
    j.nonLus++;
    if (l.ajout > seuil && l.pageCourante === 0) j.nouveaux++;
  }
  return j;
}

/** Jauges de la bibliothèque et de chaque liste dont le contenu est connu. */
export function jaugesSimulees(livres: LivreListe[], listes: Array<{ id: string; membres: MembresSimules }>): JaugesListes {
  exposition.appels++;
  // Exposé à chaque appel : le simulé principal peut remplacer l'objet global après le chargement du module.
  (globalThis as { __comicrustSimule?: Record<string, unknown> }).__comicrustSimule ??= {};
  (globalThis as unknown as { __comicrustSimule: Record<string, unknown> }).__comicrustSimule.jauges = exposition;
  const dernier = livres.reduce((m, l) => Math.max(m, l.ajout), 0);
  const seuil = dernier - DERNIERS_AJOUTS;
  const parId = new Map(livres.map((l) => [l.id, l]));
  const resultat: JaugesListes = { racine: jaugeDe(livres, seuil), listes: {} };
  for (const { id, membres } of listes) {
    if ('inconnu' in membres) continue;
    const presents = membres.ids.map((i) => parId.get(i)).filter((l): l is LivreListe => !!l);
    resultat.listes[id] = jaugeDe(presents, seuil);
  }
  return resultat;
}
