// État du panneau des listes (étape 5, docs/tauri/ipc-listes.md) : arbre renvoyé par le backend,
// liste affichée, dossiers dépliés, renommage, opération en cours et bilan. Aucune logique métier :
// comptes, dépôts possibles, positions, lecture seule viennent du backend.
import { SvelteMap, SvelteSet } from 'svelte/reactivity';
import { arbreListes, depotLivresPossible, etatListes, messageErreur, type ArbreListes, type NoeudListe, type ResultatListes } from './api';

/** Texte du contrat (refus de réordonner une vue qui n'est pas dans l'ordre de la liste). */
export const TRIER_POUR_REORDONNER = "Trie la vue dans l'ordre de la liste (« # » croissant) pour réordonner";
/** Version egui (mouse.rs, reorder_allowed) : livres déposés dans une vue qui n'est pas une liste de lecture. */
export const DEPOSER_SUR_LECTURE = 'Dépose les livres sur une liste de lecture du panneau des listes';
/** Intervalle de `etat_listes` (ComicRack lancé ou fermé, listes relues). */
const INTERVALLE_ETAT_MS = 3000;

/** Ligne visible de l'arbre (dossiers repliés exclus). */
export interface LigneArbre {
  noeud: NoeudListe;
  profondeur: number;
  parent: NoeudListe | null;
}

export const glypheListe = (n: NoeudListe, ouvert = false): string =>
  n.type === 'dossier' ? (ouvert ? '📂' : '📁') : n.type === 'lecture' ? '📄' : n.type === 'bibliotheque' ? '▦' : '🔍';

const libelles: Record<string, string> = { dossier: 'Dossier', lecture: 'Liste de lecture', intelligente: 'Liste intelligente', bibliotheque: 'Bibliothèque' };
export const libelleType = (n: NoeudListe): string => libelles[n.type] ?? n.type;

/** Infobulle d'une liste (comme egui) : description, comptes ComicRust et ComicRack. */
export function infobulleListe(n: NoeudListe): string {
  const s = (k: number) => (k > 1 ? 's' : '');
  return [
    `${libelleType(n)} « ${n.nom} »`,
    n.description,
    n.nbLivres !== undefined ? `ComicRust : ${n.nbLivres} livre${s(n.nbLivres)}` : `ComicRust : pas encore calculable${n.inconnu ? ` (${n.inconnu})` : ''}`,
    n.type !== 'dossier' ? `ComicRack (dernier enregistrement) : ${n.nbComicrack} livre${s(n.nbComicrack)}` : '',
  ].filter(Boolean).join('\n');
}

export class EtatListes {
  arbre = $state<ArbreListes | null>(null);
  /** Erreur de chargement de l'arbre. */
  erreur = $state('');
  /** Bilan de la dernière opération (ou son refus), affiché sous le panneau. */
  info = $state<{ texte: string; erreur: boolean } | null>(null);
  /** Liste affichée dans la vue (null : toute la bibliothèque). */
  courante = $state<string | null>(null);
  ouverts = new SvelteSet<string>();
  occupe = $state(false);
  /** Incrémenté après chaque modification : la vue recharge les livres de la liste affichée. */
  version = $state(0);
  /** Renommage en cours dans l'arbre. */
  renommage = $state<{ id: string; texte: string } | null>(null);
  /** Livres glissés au-dessus d'une liste de l'arbre : mise en évidence (ok : dépôt accepté). */
  survolDepot = $state<{ id: string; ok: boolean } | null>(null);
  /** Réponses de `depot_livres_possible` pendant un glisser (null : accepté, sinon la raison). */
  depots = new SvelteMap<string, string | null>();
  /** La sélection doit changer (ResultatListes.selection) : branché par l'application. */
  surSelection: (id: string | null) => void = (id) => (this.courante = id);

  private demandes = new Set<string>();
  private minuterie: ReturnType<typeof setInterval> | undefined;

  async charger(): Promise<void> {
    try {
      this.arbre = await arbreListes();
      this.erreur = '';
      this.verifierCourante();
    } catch (e) {
      this.erreur = messageErreur(e);
    }
  }

  /** `etat_listes` toutes les 3 s : grise / dégrise (ComicRack lancé ou fermé), relit l'arbre si
   *  les listes ont changé (ComicRack a réécrit ComicDb.xml). */
  demarrer(): void {
    this.arreter();
    this.minuterie = setInterval(() => void this.surveiller(), INTERVALLE_ETAT_MS);
  }

  arreter(): void {
    clearInterval(this.minuterie);
  }

  private async surveiller() {
    if (!this.arbre || this.occupe) return;
    try {
      const e = await etatListes();
      if (!this.arbre) return;
      if (e.version !== this.arbre.etat.version) await this.charger();
      else if (JSON.stringify(e) !== JSON.stringify(this.arbre.etat)) this.arbre.etat = e;
    } catch {
      /* état indisponible un instant : on réessaiera */
    }
  }

  /** La liste affichée a disparu (relecture) : retour à la bibliothèque. */
  private verifierCourante() {
    if (this.courante && !this.noeud(this.courante)) this.surSelection(null);
  }

  /** Raison pour laquelle les listes ne sont pas modifiables (sinon undefined). */
  get raison(): string | undefined {
    const e = this.arbre?.etat;
    if (!e) return 'Listes non chargées';
    if (!e.modifiable) return e.lectureSeule ?? e.erreur ?? 'Listes non modifiables';
    return this.occupe ? 'Enregistrement en cours…' : undefined;
  }

  /** Exécute une commande qui modifie les listes : arbre, bilan, sélection, renommage à jour.
   *  Échec : « <opération> : rien n'a été enregistré. <raison> » (ipc-listes.md). */
  async executer(operation: string, f: () => Promise<ResultatListes>): Promise<ResultatListes | null> {
    this.occupe = true;
    try {
      const r = await f();
      this.arbre = r.arbre;
      const texte = [r.info, ...(r.avertissements ?? []).map((a) => `⚠ ${a}`)].filter(Boolean).join('\n');
      if (texte) this.info = { texte, erreur: false };
      if (r.creee) this.devoiler(r.creee);
      if (r.selection !== undefined) this.surSelection(r.selection);
      else this.verifierCourante();
      if (r.renommer) {
        const n = this.noeud(r.renommer);
        if (n) this.renommage = { id: n.id, texte: n.nom };
      }
      this.version++;
      return r;
    } catch (e) {
      this.info = { texte: `${operation} : rien n'a été enregistré. ${messageErreur(e)}`, erreur: true };
      void this.charger(); // l'état a pu changer (ComicRack lancé entre-temps…)
      return null;
    } finally {
      this.occupe = false;
    }
  }

  noeud(id: string | null | undefined, liste = this.arbre?.listes ?? []): NoeudListe | undefined {
    if (!id) return undefined;
    for (const n of liste) {
      if (n.id === id) return n;
      const r = n.enfants && this.noeud(id, n.enfants);
      if (r) return r;
    }
    return undefined;
  }

  /** Parent et rang d'une liste. */
  place(id: string, liste = this.arbre?.listes ?? [], parent: NoeudListe | null = null): { parent: NoeudListe | null; rang: number } | null {
    for (let i = 0; i < liste.length; i++) {
      if (liste[i].id === id) return { parent, rang: i };
      const r = liste[i].enfants && this.place(id, liste[i].enfants, liste[i]);
      if (r) return r;
    }
    return null;
  }

  /** Lignes visibles dans l'ordre d'affichage (la « Bibliothèque » est dans la navigation principale). */
  get lignes(): LigneArbre[] {
    const res: LigneArbre[] = [];
    const parcourir = (liste: NoeudListe[], profondeur: number, parent: NoeudListe | null) => {
      for (const noeud of liste) {
        if (noeud.type === 'bibliotheque') continue;
        res.push({ noeud, profondeur, parent });
        if (noeud.enfants && this.ouverts.has(noeud.id)) parcourir(noeud.enfants, profondeur + 1, noeud);
      }
    };
    parcourir(this.arbre?.listes ?? [], 0, null);
    return res;
  }

  /** Déplie les dossiers qui contiennent la liste (liste créée, importée, déplacée). */
  devoiler(id: string): void {
    let p = this.place(id)?.parent;
    while (p) {
      this.ouverts.add(p.id);
      p = this.place(p.id)?.parent ?? null;
    }
  }

  /** Dépôt de livres sur la liste `id` : null accepté, chaîne = raison, undefined = réponse attendue
   *  (`depot_livres_possible`, demandé une fois par glisser ; voir viderDepots). */
  depot(id: string): string | null | undefined {
    if (this.depots.has(id)) return this.depots.get(id);
    if (!this.demandes.has(id)) {
      this.demandes.add(id);
      void depotLivresPossible(id, this.courante ?? undefined).then(
        (r) => this.depots.set(id, r),
        (e) => this.depots.set(id, messageErreur(e)),
      );
    }
    return undefined;
  }

  /** Nouveau glisser : les réponses précédentes ne valent plus (lecture seule, liste affichée…). */
  viderDepots(): void {
    this.demandes.clear();
    this.depots.clear();
  }

  /** Listes de lecture qui acceptent des livres (« Ajouter à une liste… »), avec leurs dossiers. */
  listesDeLecture(liste = this.arbre?.listes ?? [], chemin = ''): Array<{ id: string; nom: string }> {
    return liste.flatMap((n) =>
      n.type === 'lecture' && n.accepteLivres
        ? [{ id: n.id, nom: chemin + n.nom }]
        : n.enfants
          ? this.listesDeLecture(n.enfants, `${chemin}${n.nom} ▸ `)
          : [],
    );
  }
}
