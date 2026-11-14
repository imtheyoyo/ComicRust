// Sélection multiple de la bibliothèque (grille et tableau), par index dans la vue courante.
// Stockée en intervalles fermés [début, fin] triés et disjoints : Ctrl+A sur 100 000 livres ne
// coûte qu'un intervalle. Les identifiants sont résolus à la demande (Bibliotheque.idsDe), les
// livres hors écran n'étant pas forcément chargés.

export type Intervalle = [number, number];

/** Normalise : tri, fusion des intervalles qui se touchent ou se chevauchent. */
function normaliser(liste: Intervalle[]): Intervalle[] {
  const tries = liste.filter(([a, b]) => a <= b).sort((x, y) => x[0] - y[0]);
  const res: Intervalle[] = [];
  for (const [a, b] of tries) {
    const d = res[res.length - 1];
    if (d && a <= d[1] + 1) d[1] = Math.max(d[1], b);
    else res.push([a, b]);
  }
  return res;
}

/** Retire l'index `i` des intervalles. */
function retirer(liste: Intervalle[], i: number): Intervalle[] {
  const res: Intervalle[] = [];
  for (const [a, b] of liste) {
    if (i < a || i > b) res.push([a, b]);
    else {
      if (a <= i - 1) res.push([a, i - 1]);
      if (i + 1 <= b) res.push([i + 1, b]);
    }
  }
  return res;
}

/** Différence symétrique de deux listes d'intervalles normalisées (Ctrl+rectangle : bascule). */
function basculer(a: Intervalle[], b: Intervalle[]): Intervalle[] {
  // Balayage des bornes : un index est gardé s'il est couvert par une seule des deux listes.
  const bornes: Array<[number, number]> = [];
  for (const [x, y] of [...a, ...b]) bornes.push([x, 1], [y + 1, -1]);
  bornes.sort((p, q) => p[0] - q[0]);
  const res: Intervalle[] = [];
  let couverture = 0;
  let debut = 0;
  for (let k = 0; k < bornes.length; ) {
    const pos = bornes[k][0];
    const avant = couverture;
    while (k < bornes.length && bornes[k][0] === pos) couverture += bornes[k++][1];
    if (avant !== 1 && couverture === 1) debut = pos;
    else if (avant === 1 && couverture !== 1 && pos - 1 >= debut) res.push([debut, pos - 1]);
  }
  return normaliser(res);
}

/** État de la sélection mis de côté (rectangle de sélection annulé par Échap). */
export interface Instantane {
  curseur: number;
  ancre: number;
  intervalles: Intervalle[];
}

/** Effet d'un rectangle de sélection sur la sélection d'avant le geste. */
export type ModeRectangle = 'remplacer' | 'ajouter' | 'basculer';

/** Modificateurs d'un clic ou d'une touche. */
export interface Modificateurs {
  ctrl: boolean;
  maj: boolean;
}

export const modificateurs = (e: MouseEvent | KeyboardEvent): Modificateurs => ({
  ctrl: e.ctrlKey || e.metaKey,
  maj: e.shiftKey,
});

export class Selection {
  /** Livre « courant » (curseur clavier, panneau de détails), -1 si aucun. */
  curseur = $state(-1);
  intervalles = $state<Intervalle[]>([]);
  /** Point de départ des sélections Maj+clic / Maj+flèches. */
  private ancre = -1;

  /** Le livre courant doit toujours faire partie de la sélection (piles : Détails, « Ouvrir la pile » et
   *  barre d'état s'appuient sur lui). Quand un Ctrl+clic ou un rectangle en mode « basculer » le
   *  retire, il est recalé sur le dernier élément sélectionné, ou -1 si la sélection est vide. */
  constructor(private curseurSelectionne = false) {}

  /** Recale le curseur sur un élément réellement sélectionné (si demandé à la construction). */
  private recaler(): void {
    if (!this.curseurSelectionne || this.curseur < 0 || this.contient(this.curseur)) return;
    const dernier = this.intervalles[this.intervalles.length - 1];
    this.curseur = dernier ? dernier[1] : -1;
    this.ancre = this.curseur;
  }

  get nombre(): number {
    let n = 0;
    for (const [a, b] of this.intervalles) n += b - a + 1;
    return n;
  }

  contient(i: number): boolean {
    for (const [a, b] of this.intervalles) {
      if (i < a) return false;
      if (i <= b) return true;
    }
    return false;
  }

  /** Index sélectionnés, dans l'ordre de la vue. */
  indices(): number[] {
    const res: number[] = [];
    for (const [a, b] of this.intervalles) for (let i = a; i <= b; i++) res.push(i);
    return res;
  }

  vider(): void {
    this.curseur = -1;
    this.ancre = -1;
    this.intervalles = [];
  }

  /** Un seul livre sélectionné (clic simple, flèches sans modificateur). */
  unique(i: number): void {
    this.curseur = i;
    this.ancre = i;
    this.intervalles = i >= 0 ? [[i, i]] : [];
  }

  /** Clic sur le livre `i` (Ctrl : ajoute ou retire ; Maj : de l'ancre à `i` ; Ctrl+Maj : ajoute
   *  de l'ancre à `i` à la sélection, comme l'Explorateur de Windows). */
  cliquer(i: number, m: Modificateurs): void {
    if (m.maj && this.ancre >= 0) {
      this.etendre(i, m.ctrl);
      return;
    }
    if (m.ctrl) {
      this.intervalles = this.contient(i) ? retirer(this.intervalles, i) : normaliser([...this.intervalles, [i, i]]);
      this.curseur = i;
      this.ancre = i;
      this.recaler();
      return;
    }
    this.unique(i);
  }

  /** Clic droit : un livre hors sélection devient la sélection ; sinon elle est gardée
   *  (le menu porte sur tous les livres sélectionnés, comme ComicRack). */
  cliquerDroit(i: number): void {
    if (!this.contient(i)) this.unique(i);
    else this.curseur = i;
  }

  /** Déplacement clavier vers `i` (Maj : étend depuis l'ancre). */
  deplacer(i: number, m: Modificateurs): void {
    if (m.maj) {
      if (this.ancre < 0) this.ancre = this.curseur >= 0 ? this.curseur : i;
      this.etendre(i, m.ctrl);
    } else this.unique(i);
  }

  /** Sélectionne la plage [a, b] (en-tête d'un groupe) ; `ajouter` (Ctrl) : garde la sélection. */
  plage(a: number, b: number, ajouter: boolean): void {
    this.intervalles = normaliser([...(ajouter ? this.intervalles : []), [a, b]]);
    this.curseur = a;
    this.ancre = a;
  }

  /** Ctrl+A. */
  tout(total: number): void {
    if (total <= 0) return;
    this.intervalles = [[0, total - 1]];
    if (this.curseur < 0) this.curseur = 0;
    this.ancre = this.curseur;
  }

  /** Copie de la sélection (début d'un rectangle de sélection). */
  instantane(): Instantane {
    return { curseur: this.curseur, ancre: this.ancre, intervalles: this.intervalles.map(([a, b]) => [a, b] as Intervalle) };
  }

  /** Rétablit une sélection mise de côté (Échap pendant le rectangle). */
  restaurer(s: Instantane): void {
    this.curseur = s.curseur;
    this.ancre = s.ancre;
    this.intervalles = s.intervalles.map(([a, b]) => [a, b] as Intervalle);
  }

  /** Rectangle de sélection : `touches` (intervalles des livres touchés) combinés à la sélection
   *  d'avant le geste `base` (remplacer : sans modificateur ; ajouter : Maj ; basculer : Ctrl).
   *  `dernier` : dernier livre touché, qui devient le livre courant (-1 : curseur inchangé). */
  rectangle(touches: Intervalle[], base: Instantane, mode: ModeRectangle, dernier: number): void {
    const t = normaliser(touches.map(([a, b]) => [a, b] as Intervalle));
    this.intervalles = mode === 'remplacer' ? t : mode === 'ajouter' ? normaliser([...base.intervalles, ...t]) : basculer(base.intervalles, t);
    if (dernier >= 0) {
      this.curseur = dernier;
      this.ancre = dernier;
    } else {
      this.curseur = mode === 'remplacer' ? -1 : base.curseur;
      this.ancre = mode === 'remplacer' ? -1 : base.ancre;
    }
    this.recaler();
  }

  /** Total de la vue réduit : retire ce qui dépasse. */
  borner(total: number): void {
    if (this.curseur >= total) this.curseur = total - 1;
    const dernier = this.intervalles[this.intervalles.length - 1];
    if (dernier && dernier[1] >= total) {
      this.intervalles = normaliser(this.intervalles.map(([a, b]) => [a, Math.min(b, total - 1)] as Intervalle));
    }
    this.recaler();
  }

  private etendre(i: number, ajouter: boolean): void {
    const [a, b] = this.ancre <= i ? [this.ancre, i] : [i, this.ancre];
    this.intervalles = normaliser([...(ajouter ? this.intervalles : []), [a, b]]);
    this.curseur = i;
  }
}
