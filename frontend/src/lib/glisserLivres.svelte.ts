// Glisser-déposer de livres (étape 5), à la souris (événements pointer, comme l'onglet Pages :
// pas de glisser HTML5, que WebView2 réserve aux fichiers déposés dans la fenêtre) :
// - sur une liste de lecture du panneau des listes ([data-liste-id]) : ajout ;
// - entre deux livres de la vue ([data-index]) : réordonnancement de la liste affichée, si elle
//   est triée dans l'ordre de la liste (y compris une série entière sélectionnée par son en-tête).
// Un badge suit le pointeur (« 3 livres — Ajouter à « … » », ou le refus), un trait montre la place.

/** Ce que ferait le dépôt à l'endroit survolé. */
export type Indication =
  | { type: 'deplacer'; avant: number }
  | { type: 'groupes'; insertion: number }
  | { type: 'ajouter'; liste: string; nom: string }
  | { type: 'refus'; raison: string };

export interface ContexteGlisser {
  /** Le livre `i` est-il sélectionné ? */
  contient: (i: number) => boolean;
  /** Sélectionne le seul livre `i` (glisser commencé hors de la sélection). */
  choisir: (i: number) => void;
  /** Index sélectionnés. */
  indices: () => number[];
  /** Nombre de séries de la sélection (0 sans regroupement par série). */
  series: (indices: number[]) => number;
  /** Glisser commencé (les réponses de dépôt précédentes ne valent plus). */
  debut: (groupes: boolean) => void;
  /** Livres glissés au-dessus d'une liste : null si accepté, sinon la raison, undefined si la
   *  réponse du backend est attendue ; et son nom. */
  surListe: (id: string) => { refus: string | null | undefined; nom: string };
  /** Glisser de séries entières : groupe de la vue qui contient l'index `i` (rang du groupe,
   *  premier et dernier index), ou null si les groupes ne sont pas encore connus. */
  groupeDe: (i: number) => { rang: number; debut: number; fin: number } | null;
  /** Réordonner dans la vue : null si possible, sinon la raison. */
  refusReordonner: () => string | null;
  /** Disposition de la vue : la moitié gauche/droite (grille) ou haute/basse (tableau) décide. */
  horizontal: () => boolean;
  /** Zone de la vue (défilement automatique près des bords). */
  zone: () => HTMLElement | null;
  ajouter: (liste: string, indices: number[]) => void;
  deplacer: (indices: number[], avant: number) => void;
  /** Séries entières déposées à la frontière `insertion` entre groupes. */
  deplacerGroupes: (insertion: number) => void;
}

const SEUIL = 6;

export class GlisserLivres {
  actif = $state(false);
  x = $state(0);
  y = $state(0);
  nb = $state(0);
  nbSeries = $state(0);
  indication = $state<Indication | null>(null);
  /** Trait d'insertion, en coordonnées de la fenêtre. */
  trait = $state<{ x: number; y: number; l: number; h: number } | null>(null);
  /** Liste survolée (mise en évidence dans l'arbre) et si elle accepte. */
  survol = $state<{ id: string; ok: boolean } | null>(null);

  private depart: { x0: number; y0: number; index: number; groupes: boolean } | null = null;
  /** Glisser de séries entières (commencé sur un en-tête de groupe). */
  private groupes = false;
  private indices: number[] = [];
  private defilement: ReturnType<typeof setInterval> | undefined;

  constructor(private ctx: ContexteGlisser) {}

  /** Appui sur le livre `i` (bouton gauche) : le glisser commence après quelques pixels.
   *  `groupes` : appui sur l'en-tête d'un groupe (déplacement de séries entières). */
  appui(e: PointerEvent, i: number, groupes = false): void {
    if (e.button !== 0 || ((e.ctrlKey || e.metaKey || e.shiftKey) && !groupes)) return;
    this.depart = { x0: e.clientX, y0: e.clientY, index: i, groupes };
    window.addEventListener('pointermove', this.bouger);
    window.addEventListener('pointerup', this.lacher);
    window.addEventListener('pointercancel', this.annuler);
    window.addEventListener('keydown', this.clavier, true);
  }

  private bouger = (e: PointerEvent) => {
    if (!this.depart) return;
    if (!this.actif) {
      if (Math.hypot(e.clientX - this.depart.x0, e.clientY - this.depart.y0) < SEUIL) return;
      if (!this.ctx.contient(this.depart.index)) this.ctx.choisir(this.depart.index);
      this.indices = this.ctx.indices();
      if (this.indices.length === 0) return;
      this.nb = this.indices.length;
      this.nbSeries = this.ctx.series(this.indices);
      this.groupes = this.depart.groupes;
      this.ctx.debut(this.groupes);
      this.actif = true;
      document.body.classList.add('glisser-livres');
      this.defilement = setInterval(() => this.defiler(), 30);
    }
    this.x = e.clientX;
    this.y = e.clientY;
    this.calculer();
  };

  private defiler() {
    const z = this.ctx.zone();
    if (!z || !this.actif) return;
    const r = z.getBoundingClientRect();
    if (this.x < r.left || this.x > r.right) return;
    const bord = 36;
    if (this.y < r.top + bord && this.y > r.top - bord) z.scrollTop -= 14;
    else if (this.y > r.bottom - bord && this.y < r.bottom + bord) z.scrollTop += 14;
    else return;
    this.calculer();
  }

  /** Cible sous le pointeur. */
  private calculer() {
    this.trait = null;
    this.survol = null;
    const el = document.elementFromPoint(this.x, this.y) as HTMLElement | null;
    const ligne = el?.closest<HTMLElement>('[data-liste-id]');
    if (ligne) {
      const id = ligne.dataset.listeId!;
      const { refus, nom } = this.ctx.surListe(id);
      if (refus === undefined) {
        this.indication = null; // réponse du backend attendue
        return;
      }
      this.survol = { id, ok: !refus };
      this.indication = refus ? { type: 'refus', raison: refus } : { type: 'ajouter', liste: id, nom };
      return;
    }
    const z = this.ctx.zone();
    const cellule = el?.closest<HTMLElement>('[data-index]');
    if (!z || !el || !z.contains(el)) {
      this.indication = null;
      return;
    }
    const refus = this.ctx.refusReordonner();
    if (refus) {
      this.indication = { type: 'refus', raison: refus };
      return;
    }
    if (!cellule) {
      this.indication = null;
      return;
    }
    const r = cellule.getBoundingClientRect();
    const i = Number(cellule.dataset.index);
    const h = this.ctx.horizontal();
    if (this.groupes) {
      this.calculerGroupes(i, r, h);
      return;
    }
    const apres = h ? this.x > r.left + r.width / 2 : this.y > r.top + r.height / 2;
    this.indication = { type: 'deplacer', avant: i + (apres ? 1 : 0) };
    const zr = z.getBoundingClientRect();
    this.trait = h
      ? { x: (apres ? r.right + 5 : r.left - 8), y: r.top, l: 3, h: r.height }
      : { x: Math.max(zr.left, r.left), y: (apres ? r.bottom : r.top) - 1, l: Math.min(r.width, zr.width), h: 3 };
  }

  /** Séries entières : avant le groupe survolé (première moitié de ses livres) ou après lui. */
  private calculerGroupes(i: number, r: DOMRect, h: boolean) {
    const g = this.ctx.groupeDe(i);
    if (!g) {
      this.indication = null;
      return;
    }
    const moitie = h ? this.x > r.left + r.width / 2 : this.y > r.top + r.height / 2;
    const apres = g.fin === g.debut ? moitie : i - g.debut >= (g.fin - g.debut + 1) / 2;
    this.indication = { type: 'groupes', insertion: g.rang + (apres ? 1 : 0) };
    const bord = document.querySelector<HTMLElement>(`[data-index="${apres ? g.fin : g.debut}"]`)?.getBoundingClientRect() ?? r;
    const zr = this.ctx.zone()!.getBoundingClientRect();
    this.trait = h
      ? { x: apres ? bord.right + 5 : bord.left - 8, y: bord.top, l: 3, h: bord.height }
      : { x: Math.max(zr.left, bord.left), y: (apres ? bord.bottom : bord.top) - 1, l: Math.min(bord.width, zr.width), h: 3 };
  }

  /** Les réponses attendues (dépôt sur une liste, groupes de la vue) sont arrivées. */
  recalculer(): void {
    if (this.actif) this.calculer();
  }

  private lacher = () => {
    const ind = this.actif ? this.indication : null;
    const indices = this.indices;
    const avaitGlisse = this.actif;
    this.terminer();
    if (avaitGlisse) {
      // Le clic qui suit le lâcher ne doit pas changer la sélection.
      const bloquer = (e: MouseEvent) => { e.stopPropagation(); e.preventDefault(); };
      window.addEventListener('click', bloquer, { capture: true, once: true });
      setTimeout(() => window.removeEventListener('click', bloquer, { capture: true }), 0);
    }
    if (!ind) return;
    if (ind.type === 'ajouter') this.ctx.ajouter(ind.liste, indices);
    else if (ind.type === 'deplacer') this.ctx.deplacer(indices, ind.avant);
    else if (ind.type === 'groupes') this.ctx.deplacerGroupes(ind.insertion);
  };

  private annuler = () => this.terminer();

  private clavier = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && this.actif) {
      e.preventDefault();
      e.stopPropagation();
      this.terminer();
    }
  };

  private terminer() {
    this.depart = null;
    this.actif = false;
    this.indication = null;
    this.trait = null;
    this.survol = null;
    clearInterval(this.defilement);
    document.body.classList.remove('glisser-livres');
    window.removeEventListener('pointermove', this.bouger);
    window.removeEventListener('pointerup', this.lacher);
    window.removeEventListener('pointercancel', this.annuler);
    window.removeEventListener('keydown', this.clavier, true);
  }

  /** Texte du badge. */
  get badge(): { quoi: string; action: string | null; refus: boolean } {
    const s = (k: number) => (k > 1 ? 's' : '');
    const quoi = this.nbSeries > 0
      ? `📚 ${this.nbSeries} série${s(this.nbSeries)} · ${this.nb} livre${s(this.nb)}`
      : `📚 ${this.nb} livre${s(this.nb)}`;
    const i = this.indication;
    if (!i) return { quoi, action: null, refus: false };
    if (i.type === 'deplacer' || i.type === 'groupes') return { quoi, action: 'Déplacer ici', refus: false };
    if (i.type === 'ajouter') return { quoi, action: `Ajouter à « ${i.nom} »`, refus: false };
    return { quoi, action: `⛔ ${i.raison}`, refus: true };
  }
}
