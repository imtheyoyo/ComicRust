// Logique d'affichage du lecteur (sans état Svelte) : pages visibles, planches (double page),
// navigation et calcul des ajustements. Portage de crates/comicrust/src/book/navigation.rs et
// ui/viewer.rs (version egui). Les « positions » sont des rangs dans la liste des pages visibles
// (pages supprimées exclues) ; les « index » sont les numéros de page du fichier.
import type { Ajustement, PageInfo } from './api';

export interface Dimensions {
  largeur: number;
  hauteur: number;
}

/** Dimensions supposées d'une page tant que l'image n'est pas chargée (format BD 2:3). */
export const DIMENSIONS_DEFAUT: Dimensions = { largeur: 1000, hauteur: 1500 };
export const ZOOM_MIN = 0.1;
export const ZOOM_MAX = 8;

/** Index des pages affichées par le lecteur (les pages supprimées sont masquées, sauf si elles le
 *  sont toutes). */
export function pagesVisibles(pages: PageInfo[], nbPages: number): number[] {
  const liste = pages.length > 0 ? pages : Array.from({ length: nbPages }, (_, index) => ({ index, supprimee: false }));
  const gardees = liste.filter((p) => !p.supprimee).map((p) => p.index);
  return gardees.length > 0 ? gardees : liste.map((p) => p.index);
}

/** Position de la page `index` (ou de la première page visible qui la suit). */
export function positionDe(visibles: number[], index: number): number {
  const k = visibles.findIndex((i) => i >= index);
  return k < 0 ? Math.max(0, visibles.length - 1) : k;
}

/** Une rotation d'un quart de tour échange largeur et hauteur. */
export function dimensionsAffichees(d: Dimensions, rotation: number): Dimensions {
  return ((rotation % 180) + 180) % 180 === 90 ? { largeur: d.hauteur, hauteur: d.largeur } : d;
}

export interface ContexteNavigation {
  nb: number;
  double: boolean;
  couvertureSeule: boolean;
  /** Vrai si la page à cette position doit être seule (page double ou paysage). */
  large: (position: number) => boolean;
}

/** Positions affichées ensemble à partir de `debut` (ordre de lecture). */
export function planche(ctx: ContexteNavigation, debut: number): number[] {
  if (ctx.nb === 0) return [];
  const d = Math.min(Math.max(0, debut), ctx.nb - 1);
  if (!ctx.double || d + 1 >= ctx.nb || (ctx.couvertureSeule && d === 0) || ctx.large(d) || ctx.large(d + 1)) {
    return [d];
  }
  return [d, d + 1];
}

/** Position de la planche suivante, ou `null` en fin de livre. */
export function suivante(ctx: ContexteNavigation, courante: number): number | null {
  const cible = courante + Math.max(1, planche(ctx, courante).length);
  return cible < ctx.nb ? cible : null;
}

/** Position de la planche précédente, ou `null` au début du livre. En double page, c'est le début
 *  de la planche (découpage depuis la couverture) qui contient la page précédente : on retrouve
 *  exactement les planches vues en avançant (2-3, 4-5, 6, 7…), sans page sautée ni décalage. */
export function precedente(ctx: ContexteNavigation, courante: number): number | null {
  if (courante <= 0) return null;
  return ctx.double ? debutPlanche(ctx, courante - 1) : courante - 1;
}

/** Début de la planche qui contient `position` (curseur, miniatures, reprise en double page). */
export function debutPlanche(ctx: ContexteNavigation, position: number): number {
  let p = 0;
  for (;;) {
    const s = suivante(ctx, p);
    if (s === null || s > position) return p;
    p = s;
  }
}

/** Facteur d'échelle de l'ajustement pour un contenu `contenu` dans une zone `zone`. */
export function echelleAjustement(a: Ajustement, contenu: Dimensions, zone: Dimensions): number {
  const sx = zone.largeur / Math.max(1, contenu.largeur);
  const sy = zone.hauteur / Math.max(1, contenu.hauteur);
  switch (a) {
    case 'meilleur': return Math.min(sx, sy);
    case 'largeur': return sx;
    case 'hauteur': return sy;
    case 'reelle': return 1;
  }
}

/** Largeur demandée au backend (`?l=`) : arrondie au multiple de 512 supérieur pour que la page
 *  préchargée et la page affichée aient la même URL (cache). 0 = taille d'origine. */
export function largeurDemandee(largeurZone: number, zoom: number, ajustement: Ajustement, ratioPixels: number): number {
  if (ajustement === 'reelle') return 0;
  const brut = Math.max(1, largeurZone) * Math.max(1, zoom) * Math.max(1, ratioPixels);
  const arrondi = Math.ceil(brut / 512) * 512;
  return arrondi > 8192 ? 0 : arrondi;
}

/** Livre voisin dans la vue affichée (règle de `next_book` / `ListState::neighbour` de la version
 *  egui) : le premier livre après (ou avant) `depart` dont le fichier est présent ; les manquants
 *  sont sautés. `null` : dernier (ou premier) livre, la lecture reste où elle est. `lire(k)` rend le
 *  livre de rang `k` (ou `undefined` s'il n'est pas disponible : on s'arrête). */
export async function trouverVoisin<T extends { manquant?: boolean }>(
  total: number,
  depart: number,
  arriere: boolean,
  lire: (rang: number) => Promise<T | undefined>,
): Promise<{ rang: number; livre: T } | null> {
  const pas = arriere ? -1 : 1;
  for (let k = depart + pas; k >= 0 && k < total; k += pas) {
    const livre = await lire(k);
    if (!livre) return null;
    if (!livre.manquant) return { rang: k, livre };
  }
  return null;
}

/** Taille d'une image pour la barre d'état (« 1000 × 1500 »). */
export const libelleTaille = (d: Dimensions): string => `${Math.round(d.largeur)} × ${Math.round(d.hauteur)}`;

/** Raccourcis du lecteur, affichés par l'aide (`?` ou F1). `L` : voir la décision ci-dessous. */
export const RACCOURCIS_LECTEUR: Array<{ touches: string; effet: string }> = [
  { touches: '← → · Espace · Retour arrière · PgUp PgDn', effet: 'Page suivante / précédente (inversé en sens manga) ; en fin de livre, livre suivant de la liste affichée' },
  { touches: 'Début · Fin', effet: 'Première / dernière page' },
  { touches: '↑ ↓', effet: 'Défiler la page' },
  { touches: '1 · 2 · 3 · 4', effet: 'Ajustement : page entière, largeur, hauteur, taille réelle' },
  { touches: '+ · − · 0 · Ctrl+molette', effet: 'Zoom avant, arrière, 100 %' },
  { touches: 'D · V · M', effet: 'Double page, défilement continu, sens manga' },
  { touches: 'T', effet: 'Miniatures' },
  { touches: 'L', effet: 'Loupe. Dans la version egui, L affichait le panneau de la liste de lecture : ici la liste est la bibliothèque, la touche reste donc à la loupe' },
  { touches: 'I', effet: 'Infos du livre' },
  { touches: 'F · F11', effet: 'Plein écran (Échap pour en sortir)' },
  { touches: 'Échap · Ctrl+W', effet: 'Fermer le lecteur' },
  { touches: '? · F1', effet: 'Cette aide' },
];

export const borneZoom =(z: number) => Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, z));
