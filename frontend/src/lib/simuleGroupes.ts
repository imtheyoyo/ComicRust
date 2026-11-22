// MODE SIMULÉ (lot L5) : titres de groupe des neuf regroupements, comme `Regroupement::titre` de
// crates/comicrust-tauri/src/tri.rs (lui-même copié de `GroupKey::label` d'egui). Hors Tauri
// seulement : le backend réel renvoie `groupe` dans chaque livre et chaque pile.
import type { Groupe, LivreResume } from './api';

/** Champs du simulé qui servent aux regroupements (en plus de ceux de `LivreResume`). */
export interface ChampsGroupes {
  genre?: string;
  scenariste?: string;
  arc?: string;
  format?: string;
  /** Avancement de lecture en % (0 ou absent : non lu). */
  avancement?: number;
}

const texte = (v: string | undefined, vide: string) => (v?.trim() ? v.trim() : vide);

/** Titre du groupe d'un livre, `''` sans regroupement. */
export function titreGroupe(l: LivreResume & ChampsGroupes, g: Groupe | undefined): string {
  switch (g) {
    case 'serie': return texte(l.serie, '(sans série)');
    case 'editeur': return texte(l.editeur, '(sans éditeur)');
    case 'scenariste': return texte(l.scenariste, '(sans scénariste)');
    case 'annee': return l.annee && l.annee > 0 ? String(l.annee) : '(sans année)';
    case 'arc': return texte(l.arc, '(sans arc)');
    case 'genre': return texte(l.genre, '(sans genre)');
    case 'format': return texte(l.format, '(sans format)');
    case 'etat': return l.lu || l.avancement === 100 ? 'Lu' : l.avancement ? 'En cours' : 'Non lu';
    default: return '';
  }
}

/** Comparaison des titres de groupe : minuscules, ordre des unités de code (comme `cmp` de Rust). */
export const comparerTitres = (a: string, b: string) => {
  const x = a.toLowerCase();
  const y = b.toLowerCase();
  return x < y ? -1 : x > y ? 1 : 0;
};
