// État d'affichage de la bibliothèque : requête courante et pages de livres chargées à la demande.
// Aucune logique métier : recherche, tri et groupement sont faits par le backend (lister_livres).
import { SvelteMap } from 'svelte/reactivity';
import {
  ecouter, listerLivres, messageErreur, type ClefTri, type ErreurScan, type Groupe, type LivreResume,
  type ProgressionScan, type ResultatScan, type SensTri,
} from './api';

export const TAILLE_PAGE = 200;
/** Pages gardées en mémoire de part et d'autre de la fenêtre visible. */
const PAGES_GARDEES = 8;

export interface ParametresRequete {
  recherche: string;
  tri: ClefTri;
  sensTri: SensTri;
  groupe: Groupe;
  /** Étape 5 : liste affichée (absent : toute la bibliothèque). */
  liste?: string;
}

export class Bibliotheque {
  params = $state<ParametresRequete>({ recherche: '', tri: 'serie', sensTri: 'asc', groupe: 'aucun' });

  /** `initiaux` : tri et groupe mémorisés, appliqués avant la première requête. */
  constructor(initiaux: Partial<ParametresRequete> = {}) {
    for (const [c, v] of Object.entries(initiaux)) {
      if (v !== undefined) (this.params as unknown as Record<string, unknown>)[c] = v;
    }
  }
  total = $state(0);
  /** Incrémenté à chaque nouvelle requête : les vues remettent alors le défilement en haut. */
  version = $state(0);
  erreur = $state('');
  /** Liste dont les livres sont affichés (`total` lui correspond), une fois la page 0 reçue. */
  listeChargee = $state<string | undefined | null>(null);
  pages = new SvelteMap<number, LivreResume[]>();

  private generation = 0;
  private enCours = new Set<number>();
  private fenetre = { premier: 0, dernier: 0 };

  livre(i: number): LivreResume | undefined {
    return this.pages.get(Math.floor(i / TAILLE_PAGE))?.[i % TAILLE_PAGE];
  }

  /** Vrai si le livre précédent appartient à un autre groupe (ou s'il n'y a pas de précédent). */
  debutGroupe(i: number): boolean {
    if (this.params.groupe === 'aucun') return false;
    const l = this.livre(i);
    if (!l) return false;
    if (i === 0) return true;
    const p = this.livre(i - 1);
    return !!p && this.nomGroupe(p) !== this.nomGroupe(l);
  }

  nomGroupe(l: LivreResume): string {
    const v = this.params.groupe === 'serie' ? l.serie : l.editeur;
    return v || (this.params.groupe === 'serie' ? 'Sans série' : 'Sans éditeur');
  }

  /** Nouvelle requête (ou rafraîchissement) : charge la page 0, puis la fenêtre visible. */
  async reinitialiser(nouvelle = true): Promise<void> {
    const gen = ++this.generation;
    this.enCours.clear();
    try {
      const r = await listerLivres({ ...this.params, debut: 0, nombre: TAILLE_PAGE });
      if (gen !== this.generation) return;
      this.erreur = '';
      this.total = r.total;
      this.listeChargee = this.params.liste;
      this.pages.clear();
      this.pages.set(0, r.livres);
      if (nouvelle) this.version++;
    } catch (e) {
      if (gen === this.generation) {
        this.erreur = messageErreur(e);
        this.total = 0;
        this.pages.clear();
        this.listeChargee = this.params.liste;
      }
      return;
    }
    this.assurer(this.fenetre.premier, this.fenetre.dernier);
  }

  /** Charge les pages couvrant les index [premier, dernier] (+ une page de chaque côté). */
  assurer(premier: number, dernier: number): void {
    this.fenetre = { premier, dernier };
    if (this.total === 0) return;
    const derniere = Math.floor((this.total - 1) / TAILLE_PAGE);
    const pa = Math.max(0, Math.floor(premier / TAILLE_PAGE) - 1);
    const pb = Math.min(derniere, Math.floor(dernier / TAILLE_PAGE) + 1);
    for (let p = pa; p <= pb; p++) {
      if (!this.pages.has(p) && !this.enCours.has(p)) void this.chargerPage(p);
    }
    for (const p of [...this.pages.keys()]) {
      if (p < pa - PAGES_GARDEES || p > pb + PAGES_GARDEES) this.pages.delete(p);
    }
  }

  /** Livres aux index donnés (sélection multiple) : les pages absentes du cache sont demandées
   *  au backend sans être gardées (la fenêtre visible n'est pas perturbée). */
  async livresDe(indices: number[]): Promise<LivreResume[]> {
    const manquantes = new Set<number>();
    for (const i of indices) {
      const p = Math.floor(i / TAILLE_PAGE);
      if (!this.pages.has(p)) manquantes.add(p);
    }
    const chargees = new Map<number, LivreResume[]>();
    await Promise.all(
      [...manquantes].map(async (p) => {
        const r = await listerLivres({ ...this.params, debut: p * TAILLE_PAGE, nombre: TAILLE_PAGE });
        chargees.set(p, r.livres);
      }),
    );
    const res: LivreResume[] = [];
    for (const i of indices) {
      const p = Math.floor(i / TAILLE_PAGE);
      const l = (this.pages.get(p) ?? chargees.get(p))?.[i % TAILLE_PAGE];
      if (l) res.push(l);
    }
    return res;
  }

  /** Dernier index du groupe qui commence à `i` (en-tête de série : sélection de toute la série).
   *  Les pages absentes sont demandées au backend. */
  async finGroupe(i: number): Promise<number> {
    const premier = (await this.livresDe([i]))[0];
    if (!premier || this.params.groupe === 'aucun') return i;
    const nom = this.nomGroupe(premier);
    let j = i;
    while (j + 1 < this.total) {
      const lot: number[] = [];
      for (let k = j + 1; k < Math.min(this.total, j + 1 + TAILLE_PAGE); k++) lot.push(k);
      const livres = await this.livresDe(lot);
      const k = livres.findIndex((l) => this.nomGroupe(l) !== nom);
      if (k >= 0) return j + k;
      j += livres.length;
      if (livres.length === 0) break;
    }
    return j;
  }

  /** Premier index du groupe qui contient `i`. */
  async debutDuGroupe(i: number): Promise<number> {
    if (this.params.groupe === 'aucun') return i;
    let k = i;
    while (k > 0) {
      const [p, l] = await this.livresDe([k - 1, k]);
      if (!p || !l || this.nomGroupe(p) !== this.nomGroupe(l)) break;
      k--;
    }
    return k;
  }

  private async chargerPage(p: number): Promise<void> {
    const gen = this.generation;
    this.enCours.add(p);
    try {
      const r = await listerLivres({ ...this.params, debut: p * TAILLE_PAGE, nombre: TAILLE_PAGE });
      if (gen !== this.generation) return;
      this.total = r.total;
      this.pages.set(p, r.livres);
    } catch (e) {
      if (gen === this.generation) this.erreur = messageErreur(e);
    } finally {
      if (gen === this.generation) this.enCours.delete(p);
    }
  }
}

/** État du scan des dossiers surveillés, alimenté par les événements du backend. */
export class SuiviScan {
  enCours = $state(false);
  fait = $state(0);
  total = $state(0);
  resultat = $state<ResultatScan | null>(null);
  erreur = $state('');
  private desabonner: Array<() => void> = [];

  async demarrer(surTermine: () => void, surErreur: () => void = surTermine): Promise<void> {
    this.desabonner.push(
      await ecouter<ProgressionScan>('scan-progression', (p) => {
        this.enCours = true;
        this.fait = p.fait;
        this.total = p.total;
      }),
      await ecouter<ResultatScan>('scan-termine', (r) => {
        this.enCours = false;
        this.resultat = r;
        surTermine();
      }),
      await ecouter<ErreurScan>('scan-erreur', (e) => {
        this.enCours = false;
        this.erreur = e?.message || 'Erreur inconnue.';
        surErreur(); // le scan a pu ajouter des livres avant d'échouer : on recharge la liste
      }),
    );
  }

  /** À appeler juste après `lancer_scan()` pour afficher la barre sans attendre le 1er événement. */
  annoncer(): void {
    this.enCours = true;
    this.fait = 0;
    this.total = 0;
    this.resultat = null;
    this.erreur = '';
  }

  arreter(): void {
    this.desabonner.forEach((f) => f());
    this.desabonner = [];
  }
}
