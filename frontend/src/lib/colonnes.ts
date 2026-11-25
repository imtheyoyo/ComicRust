// Colonnes du tableau de la bibliothèque : mêmes 11 colonnes, même ordre et mêmes formats que la
// version egui (`COLS` de crates/comicrust/src/ui/library_table.rs). Les cellules sont calculées ici
// à partir du `LivreResume` ; le backend ne renvoie que les champs bruts.
import type { ClefTri, LivreResume } from './api';

export interface Colonne {
  /** Identifiant mémorisé dans `vue.tableau` (même liste que `COLONNES_TABLEAU` côté Rust). */
  id: string;
  titre: string;
  /** Clé de tri au clic sur l'en-tête (egui : seulement ces colonnes sont triables). */
  tri: ClefTri | null;
  /** Largeur initiale en px (egui). */
  largeur: number;
  /** Alignement à droite (nombres). */
  num?: boolean;
  cellule: (l: LivreResume) => string;
}

/** `2019-10-01T12:58:00…` → `01/10/2019` (vide si absente ou « 0001-01-01 »). */
export function dateCourte(iso?: string): string {
  if (!iso || iso.length < 10 || iso.startsWith('0001-01-01')) return '';
  return `${iso.slice(8, 10)}/${iso.slice(5, 7)}/${iso.slice(0, 4)}`;
}

/** Note de 0 à 5 en étoiles (`★★★☆☆`), vide si absente. */
export function etoiles(note?: number): string {
  if (!note || note <= 0) return '';
  const pleines = Math.min(5, Math.max(0, Math.round(note)));
  return '★'.repeat(pleines) + '☆'.repeat(5 - pleines);
}

/** Colonne « N° » : `v2 #3`, `v2`, `#3` ou vide. */
export function numeroVolume(l: Pick<LivreResume, 'numero' | 'volume'>): string {
  const n = l.numero ?? '';
  const v = l.volume;
  if (v !== undefined && v >= 0) return n ? `v${v} #${n}` : `v${v}`;
  return n ? `#${n}` : '';
}

/** Colonne « Parution » : `03/1991`, `1991` ou vide. */
export function parution(l: Pick<LivreResume, 'annee' | 'mois'>): string {
  if (!l.annee || l.annee <= 0) return '';
  // Mois hors de 1..12 (fiche abîmée) : seulement l'année.
  return l.mois && l.mois >= 1 && l.mois <= 12 ? `${String(l.mois).padStart(2, '0')}/${l.annee}` : String(l.annee);
}

/** Les 11 colonnes, dans l'ordre d'egui. */
export const COLONNES: Colonne[] = [
  { id: 'serie', titre: 'Série', tri: 'serie', largeur: 220, cellule: (l) => l.serie ?? '' },
  { id: 'numero', titre: 'N°', tri: null, largeur: 70, num: true, cellule: numeroVolume },
  { id: 'titre', titre: 'Titre', tri: 'titre', largeur: 220, cellule: (l) => l.titre },
  { id: 'parution', titre: 'Parution', tri: 'parution', largeur: 80, num: true, cellule: parution },
  { id: 'editeur', titre: 'Éditeur', tri: null, largeur: 110, cellule: (l) => l.editeur ?? '' },
  { id: 'scenariste', titre: 'Scénariste', tri: null, largeur: 140, cellule: (l) => l.scenariste ?? '' },
  { id: 'pages', titre: 'Pages', tri: 'pages', largeur: 56, num: true, cellule: (l) => (l.pages ? String(l.pages) : '') },
  { id: 'lu', titre: 'Lu', tri: 'avancement', largeur: 56, num: true, cellule: (l) => (l.avancement ? `${l.avancement} %` : '') },
  { id: 'note', titre: 'Note', tri: 'note', largeur: 76, cellule: (l) => etoiles(l.note) },
  { id: 'ajoute', titre: 'Ajouté', tri: 'ajout', largeur: 86, num: true, cellule: (l) => dateCourte(l.ajoute) },
  { id: 'fichier', titre: 'Fichier', tri: 'fichier', largeur: 300, cellule: (l) => l.fichier ?? '' },
];

export const LARGEUR_MIN = 30;
export const LARGEUR_MAX = 2000;
/** Largeur minimale de la colonne « Fichier » (egui : `at_least(80)`). */
export const LARGEUR_MIN_FICHIER = 80;

/** Colonnes visibles : celles de `ids` dans l'ordre canonique ; toutes si `ids` est absent ou vide. */
export function colonnesVisibles(ids?: string[]): Colonne[] {
  const v = ids?.length ? COLONNES.filter((c) => ids.includes(c.id)) : [];
  return v.length ? v : COLONNES;
}

/** Largeur d'une colonne : celle mémorisée (bornée), sinon celle d'egui. */
export function largeurColonne(c: Colonne, largeurs?: Record<string, number>): number {
  const m = c.id === 'fichier' ? LARGEUR_MIN_FICHIER : LARGEUR_MIN;
  const n = largeurs?.[c.id];
  return Math.min(LARGEUR_MAX, Math.max(m, Number.isFinite(n) ? (n as number) : c.largeur));
}
