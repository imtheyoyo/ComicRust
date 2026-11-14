// Rectangle de sélection (« lasso ») de la bibliothèque, comme ComicRack et la version egui
// (Gesture::Marquee de library_view.rs) : appui du bouton gauche dans le vide de la vue (ni sur un
// livre, ni sur un en-tête, ni sur la barre de défilement) puis glisser ; les livres touchés sont
// sélectionnés en direct. Sans modificateur : ils remplacent la sélection ; Maj : ils s'y ajoutent ;
// Ctrl : ils y sont basculés. Échap rétablit la sélection d'avant ; un simple clic dans le vide
// (sans glisser) la vide. Près du bord haut ou bas, la vue défile toute seule.
//
// La vue est virtualisée : les livres touchés sont calculés à partir de la géométrie de la grille
// (fonction `touches` du composant), pas des éléments du DOM, dont seule une partie existe.
// Coordonnées : celles de la « piste » (contenu défilant), qui suivent le défilement.

import type { Instantane, Intervalle, ModeRectangle, Selection } from './selection.svelte';

/** Rectangle normalisé, en coordonnées de la piste. */
export interface Boite {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

export interface ContexteRectangle {
  sel: () => Selection;
  /** Zone qui défile (conteneur de la piste). */
  zone: () => HTMLElement | null;
  /** Piste : élément de toute la hauteur du contenu, origine des coordonnées. */
  piste: () => HTMLElement | null;
  /** Livres touchés par la boîte (intervalles d'index) et le dernier touché : celui le plus proche
   *  du coin tenu par le pointeur (`px`, `py`), -1 si aucun. */
  touches: (b: Boite, px: number, py: number) => { intervalles: Intervalle[]; dernier: number };
  /** Hauteur masquée en haut de la zone par un en-tête collant (tableau). */
  margeHaut?: () => number;
}

/** Déplacement minimal (pixels) avant que l'appui devienne un rectangle. */
const SEUIL = 4;
/** Bande près des bords haut et bas qui fait défiler la vue. */
const BORD = 40;

export class RectangleSelection {
  /** Rectangle affiché (coordonnées de la piste), null hors geste. */
  boite = $state<Boite | null>(null);

  private depart: { x: number; y: number; cx: number; cy: number; mode: ModeRectangle; base: Instantane } | null = null;
  private actif = false;
  /** Dernière position du pointeur, en coordonnées de la fenêtre. */
  private px = 0;
  private py = 0;
  private defilement: ReturnType<typeof setInterval> | undefined;

  constructor(private ctx: ContexteRectangle) {}

  /** Appui sur la zone : démarre le geste si c'est dans le vide. */
  appui = (e: PointerEvent): void => {
    if (e.button !== 0 || this.depart) return;
    const z = this.ctx.zone();
    const p = this.ctx.piste();
    const cible = e.target as HTMLElement | null;
    if (!z || !p || !cible) return;
    // Pas sur un livre, une pile, un en-tête de groupe ni un contrôle (l'en-tête collant du tableau
    // est exclu plus bas par `margeHaut`).
    if (cible.closest('[data-index], [data-groupe], [data-pile], button, input, select, a')) return;
    // Pas sur la barre de défilement (à droite ou en bas de la zone).
    const zr = z.getBoundingClientRect();
    if (e.clientX - zr.left - z.clientLeft >= z.clientWidth || e.clientY - zr.top - z.clientTop >= z.clientHeight) return;
    if (e.clientY - zr.top < (this.ctx.margeHaut?.() ?? 0)) return;
    e.preventDefault(); // ni sélection de texte ni glisser natif
    z.focus({ preventScroll: true });
    const pr = p.getBoundingClientRect();
    const m = e.ctrlKey || e.metaKey ? 'basculer' : e.shiftKey ? 'ajouter' : 'remplacer';
    this.depart = { x: e.clientX - pr.left, y: e.clientY - pr.top, cx: e.clientX, cy: e.clientY, mode: m, base: this.ctx.sel().instantane() };
    this.px = e.clientX;
    this.py = e.clientY;
    window.addEventListener('pointermove', this.bouger);
    window.addEventListener('pointerup', this.lacher);
    window.addEventListener('pointercancel', this.interrompre);
    // Perte du focus (Alt+Tab, fenêtre d'une autre application) ou du capteur pendant le geste : on
    // termine proprement, sinon `depart` resterait posé et le prochain appui serait ignoré.
    window.addEventListener('blur', this.interrompre);
    window.addEventListener('lostpointercapture', this.interrompre);
    window.addEventListener('keydown', this.clavier, true);
  };

  private bouger = (e: PointerEvent) => {
    if (!this.depart) return;
    this.px = e.clientX;
    this.py = e.clientY;
    if (!this.actif) {
      if (Math.hypot(e.clientX - this.depart.cx, e.clientY - this.depart.cy) < SEUIL) return;
      this.actif = true;
      document.body.classList.add('rectangle-selection');
      this.defilement = setInterval(() => this.defiler(), 30);
    }
    this.calculer();
  };

  /** Défilement automatique quand le pointeur approche du bord haut ou bas (ou le dépasse). */
  private defiler() {
    const z = this.ctx.zone();
    if (!z || !this.actif) return;
    const r = z.getBoundingClientRect();
    const haut = r.top + (this.ctx.margeHaut?.() ?? 0);
    const bas = r.top + z.clientTop + z.clientHeight;
    let pas = 0;
    if (this.py < haut + BORD) pas = -Math.min(40, 4 + (haut + BORD - this.py) / 3);
    else if (this.py > bas - BORD) pas = Math.min(40, 4 + (this.py - (bas - BORD)) / 3);
    if (!pas) return;
    const avant = z.scrollTop;
    z.scrollTop += pas;
    if (z.scrollTop !== avant) this.calculer();
  }

  /** Recalcule le rectangle et la sélection. */
  private calculer() {
    const p = this.ctx.piste();
    const d = this.depart;
    if (!p || !d) return;
    const pr = p.getBoundingClientRect();
    // Le pointeur est borné à la piste : sortir de la vue ne sélectionne rien de plus loin.
    const x = Math.max(0, Math.min(pr.width, this.px - pr.left));
    const y = Math.max(0, Math.min(pr.height, this.py - pr.top));
    const b = { x1: Math.min(d.x, x), y1: Math.min(d.y, y), x2: Math.max(d.x, x), y2: Math.max(d.y, y) };
    this.boite = b;
    const t = this.ctx.touches(b, x, y);
    this.ctx.sel().rectangle(t.intervalles, d.base, d.mode, t.dernier);
  }

  private lacher = () => {
    const d = this.depart;
    const avaitGlisse = this.actif;
    this.terminer();
    // Simple clic dans le vide (sans Ctrl ni Maj) : la sélection est vidée.
    if (d && !avaitGlisse && d.mode === 'remplacer') this.ctx.sel().vider();
  };

  /** Geste interrompu (pointercancel, blur, lostpointercapture) : fin sans « simple clic » ; la
   *  sélection obtenue jusque-là est gardée. */
  private interrompre = () => {
    if (this.depart) this.terminer();
  };

  private clavier = (e: KeyboardEvent) => {
    if (e.key !== 'Escape' || !this.depart) return;
    // Échap : retour à la sélection d'avant le geste (et rien d'autre : pas de fermeture de pile).
    e.preventDefault();
    e.stopImmediatePropagation();
    this.ctx.sel().restaurer(this.depart.base);
    this.terminer();
  };

  private terminer() {
    this.depart = null;
    this.actif = false;
    this.boite = null;
    clearInterval(this.defilement);
    document.body.classList.remove('rectangle-selection');
    window.removeEventListener('pointermove', this.bouger);
    window.removeEventListener('pointerup', this.lacher);
    window.removeEventListener('pointercancel', this.interrompre);
    window.removeEventListener('blur', this.interrompre);
    window.removeEventListener('lostpointercapture', this.interrompre);
    window.removeEventListener('keydown', this.clavier, true);
  }

  /** Composant démonté pendant le geste. */
  detruire(): void {
    this.terminer();
  }
}

/** Livres touchés dans une grille de cellules régulières (couvertures). */
export function touchesGrille(
  b: Boite,
  px: number,
  py: number,
  g: { total: number; colonnes: number; marge: number; largeur: number; espace: number; hautCellule: number; hautLigne: number },
): { intervalles: Intervalle[]; dernier: number } {
  const vide = { intervalles: [], dernier: -1 };
  if (g.total <= 0) return vide;
  const pasX = g.largeur + g.espace;
  // Colonnes dont la cellule [marge + c·pasX, + largeur] chevauche [x1, x2].
  let c0 = Math.max(0, Math.ceil((b.x1 - g.marge - g.largeur) / pasX));
  if (g.marge + c0 * pasX + g.largeur <= b.x1) c0++;
  const c1 = Math.min(g.colonnes - 1, Math.floor((b.x2 - g.marge) / pasX));
  // Lignes dont la cellule [marge + r·hautLigne, + hautCellule] chevauche [y1, y2].
  const nbLignes = Math.ceil(g.total / g.colonnes);
  let r0 = Math.max(0, Math.floor((b.y1 - g.marge) / g.hautLigne));
  if (g.marge + r0 * g.hautLigne + g.hautCellule <= b.y1) r0++;
  const r1 = Math.min(nbLignes - 1, Math.floor((b.y2 - g.marge) / g.hautLigne));
  if (c0 > c1 || r0 > r1 || b.x2 < g.marge || b.y2 < g.marge) return vide;
  const intervalles: Intervalle[] = [];
  for (let r = r0; r <= r1; r++) {
    const a = r * g.colonnes + c0;
    const z = Math.min(g.total - 1, r * g.colonnes + c1);
    if (a <= z) intervalles.push([a, z]);
  }
  if (!intervalles.length) return vide;
  // Dernier touché : la cellule touchée la plus proche du pointeur.
  const r = Math.max(r0, Math.min(r1, Math.floor((py - g.marge) / g.hautLigne)));
  const c = Math.max(c0, Math.min(c1, Math.floor((px - g.marge) / pasX)));
  let dernier = Math.min(r * g.colonnes + c, g.total - 1);
  if (dernier < r * g.colonnes + c0) dernier = intervalles[intervalles.length - 1][1];
  return { intervalles, dernier };
}

/** Livres touchés dans une liste de lignes de hauteur fixe (tableau). */
export function touchesLignes(b: Boite, py: number, total: number, hautLigne: number): { intervalles: Intervalle[]; dernier: number } {
  if (total <= 0) return { intervalles: [], dernier: -1 };
  const a = Math.max(0, Math.floor(b.y1 / hautLigne));
  const z = Math.min(total - 1, Math.floor(Math.max(0, b.y2 - 0.01) / hautLigne));
  if (a > z) return { intervalles: [], dernier: -1 };
  const dernier = Math.max(a, Math.min(z, Math.floor(py / hautLigne)));
  return { intervalles: [[a, z]], dernier };
}
