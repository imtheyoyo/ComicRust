// MODE SIMULÉ (lot menu du livre) : faux backend de « Marquer comme lu / non lu »
// (docs/tauri/ipc-lecture-etat.md), utilisé seulement hors de Tauri. Même contrat que le backend :
// refus en lecture seule, livres déjà dans l'état demandé comptés à part, événement
// `bibliotheque-modifiee`. `?lecture=lectureSeule` force la lecture seule.
// `window.__comicrustSimule.lectureEtat` expose aux tests les appels reçus.
import type { BilanLecture } from './api';

interface LivreLecture {
  id: string;
  pages: number;
  lu: boolean;
  pageCourante: number;
  pageMax?: number;
  enRelecture?: boolean;
}

export interface ContexteLectureEtat {
  livres: LivreLecture[];
  /** Motif de lecture seule (undefined : fiches modifiables). */
  lectureSeule: () => string | undefined;
  emettre: (nom: string, charge: unknown) => void;
  /** Le contenu des vues a changé (cache de l'ordre à recalculer). */
  invalider: () => void;
}

export function creerLectureEtatSimulee(ctx: ContexteLectureEtat) {
  const option = new URLSearchParams(location.search).get('lecture');
  const appels: Array<{ ids: string[]; lu: boolean }> = [];
  const exposition = { appels };
  (globalThis as { __comicrustSimule?: Record<string, unknown> }).__comicrustSimule ??= {};
  (globalThis as unknown as { __comicrustSimule: Record<string, unknown> }).__comicrustSimule.lectureEtat = exposition;

  function marquer(ids: string[], lu: boolean): BilanLecture {
    const raison = ctx.lectureSeule() ?? (option === 'lectureSeule' ? 'Lecture seule : bibliothèque ComicRack non modifiable (simulation).' : undefined);
    if (raison) throw new Error(raison);
    appels.push({ ids, lu });
    const cibles = [...new Set(ids)].map((i) => ctx.livres.find((l) => l.id === i)).filter((l): l is LivreLecture => !!l);
    if (!cibles.length) throw new Error('aucun livre connu dans la sélection');
    const bilan: BilanLecture = { lu, modifies: [], inchanges: 0, ignores: [] };
    for (const l of cibles) {
      const dejaLu = l.lu;
      if (lu ? dejaLu : !dejaLu && l.pageCourante === 0 && !l.pageMax) {
        bilan.inchanges++;
        continue;
      }
      l.lu = lu;
      l.pageCourante = lu ? Math.max(0, l.pages - 1) : 0;
      l.pageMax = lu ? l.pageCourante : 0;
      l.enRelecture = false;
      bilan.modifies.push(l.id);
    }
    if (bilan.modifies.length) {
      ctx.invalider();
      ctx.emettre('bibliotheque-modifiee', { ids: bilan.modifies });
    }
    return bilan;
  }

  function appeler(commande: string, args: Record<string, unknown>): unknown {
    if (commande !== 'marquer_lecture') return undefined;
    return marquer((args.ids as string[]) ?? [], args.lu === true);
  }
  return { appeler };
}
