// Suivi de la sélection par IDENTIFIANT quand la vue est relue (relecture tauri-v1.0.2, B1).
//
// La sélection (lib/selection.svelte.ts) est faite de rangs dans la vue. Quand la vue est relue
// (fiche modifiée, « Marquer comme lu », fin d'analyse, lecteur fermé, modification externe…),
// l'ordre peut changer : gardée telle quelle, la sélection désignait d'autres livres (Suppr,
// Ctrl+I, scripts agissaient sur eux). Ici : avant la relecture, les identifiants des livres
// sélectionnés sont relevés dans les pages chargées ; après, ils sont retrouvés dans les nouvelles
// pages, sinon demandés au backend (`rangs_vue`). Règles :
// - tout sélectionné (Ctrl+A) : reste « tout », le livre courant est suivi ;
// - sélection connue (≤ MAX_SUIVI livres, tous chargés) : chaque livre est suivi ; ceux sortis de
//   la vue (filtrés, retirés) quittent la sélection ;
// - sélection inconnue (trop grande ou en partie jamais chargée) : seul le livre courant est gardé,
//   jamais une sélection par rangs qui pourrait désigner d'autres livres.
import type { LivreResume } from './api';
import type { Instantane, Selection } from './selection.svelte';

/** Nombre maximal de livres sélectionnés suivis un à un (au-delà : seul le livre courant). */
export const MAX_SUIVI = 5000;

export interface MemoSelection {
  /** Toute la vue était sélectionnée (Ctrl+A). */
  tout: boolean;
  /** Livre courant (curseur), ou null. */
  curseurId: string | null;
  /** Livres sélectionnés ; null : inconnus (trop nombreux, ou pas tous chargés). */
  ids: string[] | null;
  /** État de la sélection au relevé : s'il a changé entre-temps (clic pendant la relecture), on
   *  ne touche à rien. */
  avant: string;
}

const signature = (i: Instantane) => JSON.stringify(i);

/** Relève la sélection avant une relecture (`livre` : livre chargé au rang i dans la vue actuelle). */
export function memoriser(sel: Selection, total: number, livre: (i: number) => LivreResume | undefined): MemoSelection {
  const inst = sel.instantane();
  const curseurId = sel.curseur >= 0 ? (livre(sel.curseur)?.id ?? null) : null;
  const tout = total > 0 && inst.intervalles.length === 1 && inst.intervalles[0][0] === 0 && inst.intervalles[0][1] >= total - 1;
  let ids: string[] | null = null;
  if (!tout && sel.nombre <= MAX_SUIVI) {
    ids = [];
    for (const i of sel.indices()) {
      const l = livre(i);
      if (!l) { ids = null; break; }
      ids.push(l.id);
    }
  }
  return { tout, curseurId, ids, avant: signature(inst) };
}

export interface SourceRangs {
  total: number;
  /** Index des identifiants présents dans les pages chargées. */
  indexDansPages: (ids: Iterable<string>) => Map<string, number>;
  /** Rangs dans la vue (backend) des identifiants absents des pages ; null = hors de la vue. */
  rangs: (ids: string[]) => Promise<Array<number | null>>;
}

/** Retrouve la sélection après la relecture. Renvoie faux si rien n'a été touché (sélection
 *  changée par l'utilisateur entre-temps). */
export async function retrouver(sel: Selection, memo: MemoSelection, source: SourceRangs): Promise<boolean> {
  if (signature(sel.instantane()) !== memo.avant) return false;
  const voulus = new Set<string>(memo.ids ?? []);
  if (memo.curseurId) voulus.add(memo.curseurId);
  const places = source.indexDansPages(voulus);
  const absents = [...voulus].filter((id) => !places.has(id));
  if (absents.length > 0) {
    try {
      const r = await source.rangs(absents);
      absents.forEach((id, k) => {
        const rang = r[k];
        if (typeof rang === 'number' && rang >= 0 && rang < source.total) places.set(id, rang);
      });
    } catch {
      /* rangs indisponibles : ces livres quittent la sélection (jamais de rang au hasard) */
    }
    // Une autre action a pu changer la sélection pendant la demande.
    if (signature(sel.instantane()) !== memo.avant) return false;
  }
  const curseur = memo.curseurId !== null ? (places.get(memo.curseurId) ?? -1) : -1;
  if (memo.tout) {
    if (source.total <= 0) sel.vider();
    else {
      sel.tout(source.total);
      sel.placerCurseur(curseur >= 0 ? curseur : Math.min(Math.max(sel.curseur, 0), source.total - 1));
    }
  } else if (memo.ids !== null) {
    const indices = memo.ids.map((id) => places.get(id)).filter((i): i is number => i !== undefined);
    if (indices.length === 0 && curseur < 0) sel.vider();
    else sel.definir(indices, curseur);
  } else if (curseur >= 0) sel.unique(curseur);
  else sel.vider();
  return true;
}
