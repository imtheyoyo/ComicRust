// MODE SIMULÉ (étape 5) : faux backend des listes ComicRack (docs/tauri/ipc-listes.md), utilisé
// seulement hors de Tauri. Arbre réaliste (dossiers, listes de lecture, intelligentes, liste au
// contenu inconnu), évaluation simplifiée des critères, déplacements dans la vue (livres, séries),
// import / export .cbl factices, lecture seule (mode lié sans écriture) et « ComicRack lancé »
// (`?comicrackOuvert`). La logique réelle est dans le backend ; ceci ne sert qu'à tester le front.
import type {
  Apercu, ArbreListes, CatalogueCriteres, Combinaison, Critere, CriteresListe, DemandeNouvelleListe, DestinationListe,
  EditeurListe, EtatListes, GroupeVue, ListeDepart, ModeCriteres, NatureValeur, NoeudCritere, NoeudListe, Operateur,
  PlaceListe, RapportCbl, RequeteLivres, ResultatListes, TypeCritere, TypeListe,
} from './api';

/** Livre factice tel que simule.ts le garde (champs lus par les critères). */
export interface LivreListe {
  id: string; titre: string; serie?: string; numero?: string; editeur?: string; annee?: number; mois: number;
  genre: string; scenariste: string; dessinateur: string; resume: string; note: number; pages: number;
  fichier: string; fichierPresent: boolean; pageCourante: number; lu: boolean; ajout: number;
}

export interface ContexteListes {
  livres: LivreListe[];
  /** Raison de la lecture seule (mode lié sans « Lecture et écriture »), sinon undefined. */
  lectureSeule: () => string | undefined;
  /** ComicRack lancé (mode lié, `?comicrackOuvert`). */
  comicrackLance: () => boolean;
  /** Vue décrite par une requête de lister_livres (même ordre que l'écran). */
  vue: (q: RequeteLivres) => LivreListe[];
  /** Titre du groupe d'un livre, tel qu'affiché (« (sans série) » compris). */
  titreGroupe: (l: LivreListe, groupe: string) => string;
}

/** Textes du contrat (ipc-listes.md, « Erreurs courantes »). */
export const COMICRACK_LANCE =
  'Ferme ComicRack pour modifier ses listes : il réécrit ComicDb.xml en se fermant (et toutes les 10 minutes) et effacerait les modifications.';
export const LECTURE_SEULE =
  'Lecture seule : active « Lecture et écriture » dans les réglages de l\'édition pour modifier les listes de ComicRack (ComicRack fermé).';

interface ListeInterne {
  id: string;
  nom: string;
  type: TypeListe;
  description: string;
  nbComicrack: number;
  enfants?: ListeInterne[];
  combinaison?: Combinaison;
  ids?: string[];
  criteres?: CriteresListe;
}

// ───── Catalogue des critères (extrait de comicrust-core/src/matcher_catalog.rs) ─────

const op = (o: number, libelle: string, valeurs: 0 | 1 | 2, unite?: string): Operateur => ({ op: o, libelle, valeurs, unite });
const TEXTE_OPS = [op(0, 'est', 1), op(1, 'contient', 1), op(2, 'contient une partie de', 1), op(3, 'contient tout', 1),
  op(4, 'commence par', 1), op(5, 'finit par', 1), op(6, 'liste contient', 1), op(7, 'expression régulière', 1)];
const OPS: Record<NatureValeur, Operateur[]> = {
  texte: TEXTE_OPS,
  valeurPerso: TEXTE_OPS,
  nombre: [op(0, 'est', 1), op(1, 'est plus grand', 1), op(2, 'est plus petit', 1), op(3, 'est dans la fourchette', 2)],
  date: [op(0, 'est', 1), op(1, 'est après', 1), op(2, 'est avant', 1), op(3, 'dans les derniers', 1, 'jours'), op(4, 'est dans la fourchette', 2)],
  ouiNon: [op(0, 'est Oui', 0), op(1, 'est Non', 0), op(2, 'est Inconnu', 0)],
  manga: [op(0, 'est Oui', 0), op(1, 'est Oui (de droite à gauche)', 0), op(2, 'est Non', 0), op(3, 'est Inconnu', 0)],
  activation: [op(0, 'Oui', 0), op(1, 'Non', 0)],
  expression: [op(0, 'est Vrai', 1), op(1, 'est Faux', 1)],
};

type Lecteur = (l: LivreListe) => string | number | boolean | undefined;
type CritereInterne = TypeCritere & { lire?: Lecteur; categorie: string };
const AUJOURDHUI = Date.UTC(2026, 10, 8);
const JOUR = 86_400_000;

const CATEGORIES: Record<NatureValeur, string> = {
  texte: 'Texte', valeurPerso: 'Texte', nombre: 'Nombres', date: 'Dates', ouiNon: 'Oui / Non', manga: 'Oui / Non',
  activation: 'Oui / Non', expression: 'Modules de ComicRack',
};

function creerCatalogue(nb: number): CritereInterne[] {
  /** Date d'ajout factice : un livre tous les 0,3 jour jusqu'à aujourd'hui. */
  const ajout = (l: LivreListe) => AUJOURDHUI - Math.round((nb - l.ajout) * 0.3) * JOUR;
  const c = (type: string, libelle: string, nature: NatureValeur, lire?: Lecteur, unite?: string, statSerie = false): CritereInterne => ({
    type, libelle, nature, unite, statSerie, evalue: !!lire, texte: nature === 'texte' || nature === 'valeurPerso',
    operateurs: OPS[nature], lire, categorie: statSerie ? 'Statistiques de série' : CATEGORIES[nature],
  });
  return [
    c('ComicBookAllPropertiesMatcher', 'Tout', 'texte', (l) => [l.titre, l.serie, l.editeur, l.genre, l.scenariste, l.dessinateur, l.resume].join(' ')),
    c('ComicBookSeriesMatcher', 'Série', 'texte', (l) => l.serie ?? ''),
    c('ComicBookTitleMatcher', 'Titre', 'texte', (l) => l.titre),
    c('ComicBookStoryArcMatcher', 'Arc narratif', 'texte'),
    c('ComicBookWriterMatcher', 'Scénariste', 'texte', (l) => l.scenariste),
    c('ComicBookPencillerMatcher', 'Dessinateur', 'texte', (l) => l.dessinateur),
    c('ComicBookPublisherMatcher', 'Éditeur', 'texte', (l) => l.editeur ?? ''),
    c('ComicBookGenreMatcher', 'Genre', 'texte', (l) => l.genre),
    c('ComicBookSummaryMatcher', 'Résumé', 'texte', (l) => l.resume),
    c('ComicBookTagsMatcher', 'Étiquettes', 'texte'),
    c('ComicBookFileMatcher', 'Fichier', 'texte', (l) => l.fichier.split('\\').pop() ?? ''),
    c('ComicBookFullPathMatcher', 'Chemin du fichier', 'texte', (l) => l.fichier),
    c('ComicBookCustomValuesMatcher', 'Valeur personnalisée', 'valeurPerso'),
    c('ComicBookNumberMatcher', 'Numéro', 'nombre', (l) => (l.numero ? Number(l.numero) : undefined)),
    c('ComicBookYearMatcher', 'Année', 'nombre', (l) => l.annee),
    c('ComicBookMonthMatcher', 'Mois', 'nombre', (l) => l.mois),
    c('ComicBookPageCountMatcher', 'Nombre de pages', 'nombre', (l) => l.pages),
    c('ComicBookReadPercentageMatcher', 'Pourcentage de lecture', 'nombre', (l) => (l.lu ? 100 : Math.round((l.pageCourante / Math.max(1, l.pages - 1)) * 100))),
    c('ComicBookRatingMatcher', 'Mon évaluation', 'nombre', (l) => l.note),
    c('ComicBookFileSizeMatcher', 'Taille du fichier', 'nombre', (l) => Math.round((l.pages * 310_000) / 1_048_576), 'Mo'),
    c('ComicBookAddedMatcher', 'Ajouté', 'date', ajout),
    c('ComicBookOpenedMatcher', 'Ouvert', 'date'),
    c('ComicBookPublishedMatcher', 'Publié', 'date', (l) => (l.annee ? Date.UTC(l.annee, l.mois - 1, 1) : undefined)),
    c('ComicBookBlackAndWhiteMatcher', 'Noir et Blanc', 'ouiNon'),
    c('ComicBookIsMissingMatcher', 'Est manquant', 'ouiNon', (l) => !l.fichierPresent),
    c('ComicBookMangaMatcher', 'Manga', 'manga', (l) => l.id.endsWith('7')),
    c('ComicBookDuplicateMatcher', 'Seulement les doublons', 'activation'),
    c('SmartListSeriesCountMatcher', 'Série : Numéros', 'nombre', undefined, undefined, true),
    c('SmartListSeriesGapsMatcher', 'Série : Blancs', 'nombre', undefined, undefined, true),
    c('ComicBookExpressionMatcher', 'Expression', 'expression'),
  ];
}

// ───── Évaluation simplifiée des critères ─────

function texteCorrespond(v: string, o: number, cherche: string, ignorer: boolean): boolean {
  const n = (t: string) => (ignorer ? t.toLowerCase() : t);
  const a = n(v);
  const b = n(cherche).trim();
  const mots = b.split(/\s+/).filter(Boolean);
  switch (o) {
    case 0: return a === b;
    case 1: return a.includes(b);
    case 2: return mots.some((m) => a.includes(m));
    case 3: return mots.every((m) => a.includes(m));
    case 4: return a.startsWith(b);
    case 5: return a.endsWith(b);
    case 6: return a.split(',').map((x) => x.trim()).includes(b);
    case 7: try { return new RegExp(cherche, ignorer ? 'i' : '').test(v); } catch { return false; }
    default: return false;
  }
}

const estDate = (t: string) => /^\d{4}-\d{2}-\d{2}$/.test(t.trim()) && !Number.isNaN(Date.parse(t.trim()));
const estNombre = (t: string) => t.trim() !== '' && Number.isFinite(Number(t.trim().replace(',', '.')));
const nombre = (t: string) => Number(t.trim().replace(',', '.'));
const copie = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;
const pluriel = (n: number) => (n > 1 ? 's' : '');

export function creerListesSimulees(ctx: ContexteListes) {
  const { livres } = ctx;
  const catalogue = creerCatalogue(livres.length);
  const infoDe = (type: string) => catalogue.find((c) => c.type === type);
  let numero = 0;
  let version = 1;
  const nouvelId = () => `5e${(++numero).toString(16).padStart(6, '0')}-0000-4000-8000-${Date.now().toString(16).slice(-12).padStart(12, '0')}`;

  // ───── Arbre de départ ─────
  const parSerie = (s: string) => livres.filter((l) => l.serie === s).sort((a, b) => Number(a.numero) - Number(b.numero) || Number(a.id) - Number(b.id));
  const premiers = (s: string, n: number) => parSerie(s).slice(0, n).map((l) => l.id);
  const critere = (type: string, o: number, valeur = '', valeur2 = '', non = false): Critere =>
    ({ genre: 'critere', non, type, op: o, valeur, valeur2, ignorerCasse: true, modifiable: true, origine: 'x' });
  const crit = (nom: string, mode: ModeCriteres, noeuds: NoeudCritere[], extra: Partial<CriteresListe> = {}): CriteresListe =>
    ({ nom, description: '', mode, noeuds, depart: null, pasDansDepart: false, limite: null, retires: 0, ...extra });
  const L = (id: string, nom: string, ids: string[], description = '', nbComicrack = ids.length): ListeInterne =>
    ({ id, nom, type: 'lecture', description, nbComicrack, ids });
  const I = (id: string, nom: string, c: CriteresListe, description = '', nbComicrack = 0): ListeInterne =>
    ({ id, nom, type: 'intelligente', description, nbComicrack, criteres: c });
  const D = (id: string, nom: string, enfants: ListeInterne[], description = ''): ListeInterne =>
    ({ id, nom, type: 'dossier', description, nbComicrack: 0, combinaison: 'ou', enfants });

  const racine: ListeInterne[] = [
    { id: '00000000-0000-0000-0000-00000000b1b1', nom: 'Bibliothèque', type: 'bibliotheque', description: '', nbComicrack: livres.length },
    D('f0000000-0000-4000-8000-00000000ma01', 'Marvel', [
      L('l0000000-0000-4000-8000-0000thanos01', 'Thanos — ordre chronologique', premiers('Thanos', 40), 'Liste de lecture chronologique'),
      I('s0000000-0000-4000-8000-0000gardien1', 'Gardiens de la Galaxie', crit('Gardiens de la Galaxie', 'tous', [critere('ComicBookSeriesMatcher', 1, 'Gardiens')]), '', 151),
      D('f0000000-0000-4000-8000-0000evenemt', 'Événements', [
        L('l0000000-0000-4000-8000-00gauntlet1', 'Infinity Gauntlet', [...premiers('Thanos', 3), ...premiers('Les Gardiens de la Galaxie', 3)]),
        L('l0000000-0000-4000-8000-00civilwar1', 'Civil War', []),
      ]),
    ], 'Lectures Marvel'),
    D('f0000000-0000-4000-8000-00000000fb01', 'Franco-belge', [
      L('l0000000-0000-4000-8000-00malecture', 'Ma lecture', [...premiers('Astérix', 4), ...premiers('Tintin', 4), ...premiers('XIII', 4)], 'Astérix, Tintin, XIII'),
      I('s0000000-0000-4000-8000-000asterix1', 'Astérix non lus', crit('Astérix non lus', 'tous', [
        critere('ComicBookSeriesMatcher', 0, 'Astérix'), critere('ComicBookReadPercentageMatcher', 2, '100'),
      ])),
    ]),
    L('l0000000-0000-4000-8000-0000000alire', 'À lire', livres.filter((l) => !l.lu && Number(l.id) % 97 === 3).slice(0, 25).map((l) => l.id)),
    I('s0000000-0000-4000-8000-0000recents1', 'Ajoutés ces 30 derniers jours', crit('Ajoutés ces 30 derniers jours', 'tous', [critere('ComicBookAddedMatcher', 3, '30')])),
    I('s0000000-0000-4000-8000-00grospaves', 'Gros pavés SF ou fantasy', crit('Gros pavés SF ou fantasy', 'tous', [
      critere('ComicBookPageCountMatcher', 1, '150'),
      { genre: 'groupe', non: false, mode: 'auMoinsUn', origine: 'g', noeuds: [critere('ComicBookGenreMatcher', 0, 'Science-fiction'), critere('ComicBookGenreMatcher', 0, 'Fantasy')] },
    ]), 'Plus de 150 pages'),
    I('s0000000-0000-4000-8000-0000dupuis01', 'Dupuis (5 au hasard)', crit('Dupuis (5 au hasard)', 'tous', [critere('ComicBookPublisherMatcher', 0, 'Dupuis')], {
      limite: { valeur: '5', unite: 'livres', selection: 'hasard' }, retires: 2,
    }), '', 5),
    I('s0000000-0000-4000-8000-00manquants', 'Livres manquants', crit('Livres manquants', 'tous', [critere('ComicBookIsMissingMatcher', 0)])),
    I('s0000000-0000-4000-8000-000inconnu1', 'Étiquettes virtuelles', crit('Étiquettes virtuelles', 'tous', [
      { genre: 'critere', non: false, type: 'ComicBookVirtualTagMatcher', op: 2, valeur: 'x', valeur2: '', ignorerCasse: true, modifiable: false,
        libelle: 'Étiquette virtuelle contient une partie de « x »', origine: '0' },
    ]), 'Critère propre à ComicRack', 37),
  ];

  // ───── Parcours ─────
  type Place = { l: ListeInterne; parent: ListeInterne | null; freres: ListeInterne[]; rang: number };
  function trouver(id: string, liste = racine, parent: ListeInterne | null = null): Place | null {
    for (let i = 0; i < liste.length; i++) {
      const l = liste[i];
      if (l.id.toLowerCase() === id.toLowerCase()) return { l, parent, freres: liste, rang: i };
      if (l.enfants) {
        const r = trouver(id, l.enfants, l);
        if (r) return r;
      }
    }
    return null;
  }
  const exiger = (id: string) => {
    const r = trouver(String(id ?? ''));
    if (!r) throw new Error('liste introuvable');
    return r;
  };
  const tous = (liste = racine): ListeInterne[] => liste.flatMap((l) => [l, ...(l.enfants ? tous(l.enfants) : [])]);

  // ───── Contenu ─────
  type Membres = { ids: string[] } | { inconnu: string };

  function erreursDe(c: CriteresListe): string[] {
    const err: string[] = [];
    let k = 0;
    const voir = (n: NoeudCritere[]) => {
      for (const x of n) {
        if (x.genre === 'groupe') { voir(x.noeuds); continue; }
        k++;
        if (!x.modifiable) continue;
        const info = infoDe(x.type);
        if (!info) continue;
        const nb = info.operateurs.find((o) => o.op === x.op)?.valeurs ?? 0;
        if (info.nature === 'valeurPerso' && !x.valeur.trim()) err.push(`Critère ${k} (${info.libelle}) : nom de la valeur obligatoire`);
        for (const v of [x.valeur, x.valeur2].slice(0, nb)) {
          if (info.nature === 'nombre' && !estNombre(v)) err.push(`Critère ${k} (${info.libelle}) : « ${v.trim()} » n'est pas un nombre`);
          else if (info.nature === 'nombre' && x.type === 'ComicBookRatingMatcher' && (nombre(v) < 0 || nombre(v) > 5)) err.push(`Critère ${k} (${info.libelle}) : la note va de 0 à 5`);
          else if (info.nature === 'date' && x.op === 3 && !/^\d+$/.test(v.trim())) err.push(`Critère ${k} (${info.libelle}) : « ${v.trim()} » n'est pas un nombre de jours`);
          else if (info.nature === 'date' && x.op !== 3 && !estDate(v)) err.push(`Critère ${k} (${info.libelle}) : « ${v.trim()} » n'est pas une date AAAA-MM-JJ`);
        }
      }
    };
    voir(c.noeuds);
    if (c.limite && !/^[1-9]\d*$/.test(c.limite.valeur.trim())) err.push(`Limite : « ${c.limite.valeur.trim()} » n'est pas un nombre entier positif`);
    return err;
  }

  function inconnuDe(n: NoeudCritere[]): string | undefined {
    for (const c of n) {
      if (c.genre === 'groupe') {
        const r = inconnuDe(c.noeuds);
        if (r) return r;
      } else if (!c.modifiable) return `critère pas encore pris en charge (${c.libelle ?? c.type})`;
      else if (!infoDe(c.type)?.lire) return `critère pas encore pris en charge : ${infoDe(c.type)?.libelle ?? c.type}`;
    }
    return undefined;
  }

  function correspond(l: LivreListe, c: NoeudCritere): boolean {
    if (c.genre === 'groupe') {
      const r = c.mode === 'tous' ? c.noeuds.every((x) => correspond(l, x)) : c.noeuds.some((x) => correspond(l, x));
      return c.non ? !r : r;
    }
    const info = infoDe(c.type)!;
    const v = info.lire!(l);
    let r = false;
    switch (info.nature) {
      case 'texte': r = texteCorrespond(String(v ?? ''), c.op, c.valeur, c.ignorerCasse); break;
      case 'nombre': {
        if (typeof v !== 'number') break;
        const a = nombre(c.valeur);
        const b = nombre(c.valeur2);
        r = c.op === 0 ? v === a : c.op === 1 ? v > a : c.op === 2 ? v < a : v >= Math.min(a, b) && v <= Math.max(a, b);
        break;
      }
      case 'date': {
        if (typeof v !== 'number') break;
        const a = Date.parse(c.valeur.trim());
        const b = Date.parse(c.valeur2.trim());
        const jour = Math.floor(v / JOUR) * JOUR;
        r = c.op === 0 ? jour === a : c.op === 1 ? jour > a : c.op === 2 ? jour < a
          : c.op === 3 ? v >= AUJOURDHUI - nombre(c.valeur) * JOUR : jour >= Math.min(a, b) && jour <= Math.max(a, b);
        break;
      }
      case 'ouiNon': r = c.op === 0 ? v === true : c.op === 1 ? v === false : false; break;
      case 'manga': r = c.op === 0 || c.op === 1 ? v === true : c.op === 2 ? v === false : false; break;
      default: break;
    }
    return c.non ? !r : r;
  }

  function evaluer(c: CriteresListe, pile: Set<string>): Membres {
    const inc = inconnuDe(c.noeuds);
    if (inc) return { inconnu: inc };
    const err = erreursDe(c);
    if (err.length) return { inconnu: err[0] };
    let depart: LivreListe[] = livres;
    if (c.depart) {
      const b = trouver(c.depart);
      if (!b) return { inconnu: 'liste de départ introuvable' };
      const mb = membres(b.l, pile);
      if ('inconnu' in mb) return { inconnu: `liste de départ « ${b.l.nom} » : ${mb.inconnu}` };
      const ens = new Set(mb.ids);
      depart = c.pasDansDepart ? livres.filter((l) => !ens.has(l.id)) : mb.ids.map((id) => livres[Number(id) - 1]).filter(Boolean);
    }
    const garde = (l: LivreListe) =>
      c.noeuds.length === 0 || (c.mode === 'tous' ? c.noeuds.every((x) => correspond(l, x)) : c.noeuds.some((x) => correspond(l, x)));
    let res = depart.filter(garde);
    if (c.limite) {
      const v = Number(c.limite.valeur);
      if (c.limite.selection === 'hasard') {
        let s = 7;
        res = res.map((l) => ({ l, k: (s = (s * 16807) % 2147483647) })).sort((a, b) => a.k - b.k).map((x) => x.l);
      }
      if (c.limite.unite === 'livres') res = res.slice(0, v);
      else {
        const max = v * (c.limite.unite === 'go' ? 1024 ** 3 : 1024 ** 2);
        let somme = 0;
        res = res.filter((l) => (somme += l.pages * 310_000) <= max);
      }
    }
    return { ids: res.map((l) => l.id) };
  }

  function membres(l: ListeInterne, pile = new Set<string>()): Membres {
    if (pile.has(l.id)) return { inconnu: 'liste qui dépend d\'elle-même' };
    pile = new Set(pile).add(l.id);
    switch (l.type) {
      case 'bibliotheque': return { ids: livres.map((x) => x.id) };
      case 'lecture': return { ids: l.ids!.slice() };
      case 'intelligente': return evaluer(l.criteres!, pile);
      case 'dossier': {
        const parts = (l.enfants ?? []).map((e) => membres(e, pile));
        const inc = parts.find((p): p is { inconnu: string } => 'inconnu' in p);
        if (inc) return inc;
        const listes = parts.map((p) => (p as { ids: string[] }).ids);
        if (l.combinaison === 'vide' || listes.length === 0) return { ids: [] };
        if (l.combinaison === 'et') {
          const [a, ...reste] = listes;
          const ens = reste.map((x) => new Set(x));
          return { ids: a.filter((id) => ens.every((e) => e.has(id))) };
        }
        return { ids: [...new Set(listes.flat())] };
      }
    }
  }

  // ───── Réponses ─────
  function etat(): EtatListes {
    const lance = ctx.comicrackLance();
    const ro = lance ? COMICRACK_LANCE : ctx.lectureSeule() ? LECTURE_SEULE : undefined;
    return {
      source: ctx.lectureSeule() === undefined && !lance ? 'comicrust' : 'comicdb',
      modifiable: !ro, lectureSeule: ro, comicrackLance: lance, version,
    };
  }

  function noeud(l: ListeInterne, freres: ListeInterne[], rang: number, modifiable: boolean): NoeudListe {
    const m = membres(l);
    const n: NoeudListe = {
      id: l.id, nom: l.nom, type: l.type, nbComicrack: l.nbComicrack,
      accepteLivres: modifiable && l.type === 'lecture', peutMonter: modifiable && rang > 0, peutDescendre: modifiable && rang < freres.length - 1,
    };
    if (l.description) n.description = l.description;
    if ('ids' in m) n.nbLivres = m.ids.length;
    else n.inconnu = m.inconnu;
    if (l.type === 'dossier') {
      n.combinaison = l.combinaison ?? 'ou';
      n.enfants = (l.enfants ?? []).map((e, i, t) => noeud(e, t, i, modifiable));
    }
    return n;
  }

  function arbre(): ArbreListes {
    const e = etat();
    return { etat: e, listes: racine.map((l, i, t) => noeud(l, t, i, e.modifiable)) };
  }
  function resultat(info: string, extra: Partial<ResultatListes> = {}): ResultatListes {
    version++;
    return { arbre: arbre(), info, avertissements: [], ...extra };
  }

  function verifierEcriture() {
    const e = etat();
    if (!e.modifiable) throw new Error(e.lectureSeule ?? 'Listes non modifiables.');
  }

  function inserer(l: ListeInterne, place: { parent?: string; position?: number; apresListe?: string }) {
    if (place.apresListe) {
      const a = exiger(place.apresListe);
      a.freres.splice(a.rang + 1, 0, l);
      return;
    }
    let cible = racine;
    if (place.parent) {
      const p = exiger(place.parent).l;
      if (p.type !== 'dossier') throw new Error('Le parent doit être un dossier.');
      cible = p.enfants ??= [];
    }
    const k = place.position === undefined ? cible.length : Math.max(0, Math.min(place.position, cible.length));
    cible.splice(k, 0, l);
  }

  /** Listes intelligentes dont la liste de départ est `id` (ou une de ses sous-listes). */
  function dependantes(id: string): string[] {
    const sous = new Set(tous([exiger(id).l]).map((x) => x.id));
    return tous().filter((x) => x.criteres?.depart && sous.has(x.criteres.depart) && !sous.has(x.id)).map((x) => x.nom);
  }

  function placeValide(glissee: string, cible: string, place: PlaceListe): boolean {
    const g = trouver(glissee);
    const c = trouver(cible);
    if (!g || !c || g.l.type === 'bibliotheque') return false;
    if (tous([g.l]).some((x) => x.id === c.l.id)) return false;
    if (place === 'dans' && c.l.type !== 'dossier') return false;
    return true;
  }

  function vueListe(id: string, q: RequeteLivres): string[] {
    const l = exiger(id).l;
    if (l.type !== 'lecture') throw new Error('Seule une liste de lecture se réordonne.');
    if (q.tri !== 'ordre' || q.sensTri !== 'asc') throw new Error("Trie la vue dans l'ordre de la liste (« # » croissant) pour réordonner");
    return ctx.vue(q).map((x) => x.id);
  }

  /** Place `bouges` (gardés dans leur ordre de la liste) devant `repere` (null : à la fin). */
  function placer(l: ListeInterne, bouges: Set<string>, repere: string | null) {
    const deplaces = l.ids!.filter((x) => bouges.has(x));
    const reste = l.ids!.filter((x) => !bouges.has(x));
    let k = repere ? reste.indexOf(repere) : reste.length;
    if (k < 0) k = reste.length;
    reste.splice(k, 0, ...deplaces);
    l.ids = reste;
  }

  function groupes(q: RequeteLivres): Array<GroupeVue & { ids: string[] }> {
    const res: Array<GroupeVue & { ids: string[] }> = [];
    if (!q.groupe || q.groupe === 'aucun') return res;
    for (const x of ctx.vue(q)) {
      const t = ctx.titreGroupe(x, q.groupe);
      const d = res[res.length - 1];
      if (d && d.titre === t) { d.nombre++; d.ids.push(x.id); } else res.push({ titre: t, nombre: 1, ids: [x.id] });
    }
    return res;
  }

  function listesDepart(id?: string): ListeDepart[] {
    const exclus = new Set<string>();
    if (id) {
      // Elle-même, son contenu et celles qui partent d'elle (pas de boucle).
      for (const x of tous([exiger(id).l])) exclus.add(x.id);
      let change = true;
      while (change) {
        change = false;
        for (const x of tous()) if (x.criteres?.depart && exclus.has(x.criteres.depart) && !exclus.has(x.id)) { exclus.add(x.id); change = true; }
      }
    }
    const res: ListeDepart[] = [];
    const parcourir = (l: ListeInterne[], p: number) => {
      for (const x of l) {
        if (x.type === 'bibliotheque' || exclus.has(x.id)) continue;
        res.push({ id: x.id, nom: x.nom, profondeur: p, type: x.type });
        if (x.enfants) parcourir(x.enfants, p + 1);
      }
    };
    parcourir(racine, 0);
    return res;
  }

  function rapportCbl(chemin: string): RapportCbl & { ids: string[] } {
    if (!/\.cbl$/i.test(chemin)) throw new Error('Choisis un fichier .cbl (liste de lecture ComicRack).');
    if (/introuvable/i.test(chemin)) throw new Error(`Fichier introuvable : ${chemin}`);
    const nom = (chemin.split(/[\\/]/).pop() ?? 'Liste').replace(/\.cbl$/i, '');
    const ids = premiers('Thanos', 14);
    const manquants = [15, 16, 19, 22].map((n, k) => ({ rang: 15 + k * 2, serie: 'Thanos', numero: String(n), volume: 2003, annee: 2004 }));
    const total = ids.length + manquants.length + 1;
    return {
      nom, total, trouves: ids.length, parIdentifiant: 9, parSerie: ids.length - 9, doublons: 1, manquants, ids,
      texte: `Liste « ${nom} » : ${ids.length} / ${total} numéros trouvés\n` + manquants.map((m) => `  #${m.rang} ${m.serie} (${m.volume}) #${m.numero}`).join('\n'),
    };
  }

  /** Commande des listes, ou `undefined` si ce n'est pas une commande de l'étape 5. */
  function appeler(commande: string, a: Record<string, unknown>): unknown {
    const id = String(a.id ?? '');
    switch (commande) {
      case 'etat_listes':
        return etat();
      case 'arbre_listes':
        return arbre();
      case 'bilan_listes':
        return tous().filter((l) => l.type !== 'dossier').map((l) => {
          const m = membres(l);
          const n = 'ids' in m ? String(m.ids.length) : '?';
          return `${n === String(l.nbComicrack) ? ' ' : '≠'} ${l.nom} : ComicRust ${n} / ComicRack ${l.nbComicrack}`;
        }).join('\n');
      case 'creer_liste': {
        verifierEcriture();
        const d = a.demande as DemandeNouvelleListe;
        const nom = d.nom?.trim() || (d.type === 'dossier' ? 'Nouveau dossier' : 'Nouvelle liste');
        const l: ListeInterne = d.type === 'dossier' ? D(nouvelId(), nom, []) : L(nouvelId(), nom, [...new Set(d.livres ?? [])], '', 0);
        inserer(l, d);
        const n = l.ids?.length ?? 0;
        return resultat(n ? `✔ Liste « ${nom} » créée avec ${n} livre${pluriel(n)}` : `✔ Création de « ${nom} »`,
          { selection: l.id, creee: l.id, renommer: d.livres?.length ? undefined : l.id });
      }
      case 'renommer_liste': {
        verifierEcriture();
        const nom = String(a.nom ?? '').trim();
        if (!nom) throw new Error('Nom vide.');
        const l = exiger(id).l;
        l.nom = nom;
        if (l.criteres) l.criteres.nom = nom;
        return resultat(`✔ Renommage en « ${nom} »`);
      }
      case 'modifier_liste': {
        verifierEcriture();
        const l = exiger(id).l;
        if (typeof a.nom === 'string' && a.nom.trim()) l.nom = a.nom.trim();
        if (typeof a.description === 'string') l.description = a.description.trim();
        if (a.combinaison && l.type === 'dossier') l.combinaison = a.combinaison as Combinaison;
        return resultat(`✔ Modification de « ${l.nom} »`);
      }
      case 'dupliquer_liste': {
        verifierEcriture();
        const r = exiger(id);
        if (r.l.type === 'bibliotheque') throw new Error('La « Bibliothèque » ne peut pas être copiée.');
        const c = copie(r.l);
        const renouveler = (x: ListeInterne) => { x.id = nouvelId(); x.enfants?.forEach(renouveler); };
        renouveler(c);
        c.nom = `Copie de ${r.l.nom}`;
        r.freres.splice(r.rang + 1, 0, c);
        return resultat(`✔ Copie de « ${r.l.nom} »`, { selection: c.id, creee: c.id });
      }
      case 'infos_suppression': {
        const l = exiger(id).l;
        return { nom: l.nom, sousListes: tous(l.enfants ?? []).length, dependantes: dependantes(l.id) };
      }
      case 'supprimer_liste': {
        verifierEcriture();
        const r = exiger(id);
        if (r.l.type === 'bibliotheque') throw new Error('La « Bibliothèque » ne peut pas être supprimée.');
        const dep = dependantes(r.l.id);
        const affichee = a.listeAffichee as string | undefined;
        const disparait = !!affichee && tous([r.l]).some((x) => x.id === affichee);
        r.freres.splice(r.rang, 1);
        for (const x of tous()) if (x.criteres?.depart && !trouver(x.criteres.depart)) x.criteres.depart = null;
        return resultat(`✔ « ${r.l.nom} » supprimée${dep.length ? ` ; partent maintenant de toute la bibliothèque : ${dep.map((d) => `« ${d} »`).join(', ')}` : ''}`,
          disparait ? { selection: null } : {});
      }
      case 'destinations_liste': {
        const r = exiger(id);
        const sous = new Set(tous([r.l]).map((x) => x.id));
        const res: DestinationListe[] = [{ id: null, nom: '(premier niveau)', profondeur: 0, actuel: r.parent === null }];
        const parcourir = (l: ListeInterne[], p: number) => {
          for (const x of l) {
            if (x.type !== 'dossier' || sous.has(x.id)) continue;
            res.push({ id: x.id, nom: x.nom, profondeur: p, actuel: r.parent?.id === x.id });
            parcourir(x.enfants ?? [], p + 1);
          }
        };
        parcourir(racine, 0);
        return res;
      }
      case 'deplacer_liste': {
        verifierEcriture();
        const r = exiger(id);
        const parent = (a.parent as string | null) ?? null;
        if (parent && tous([r.l]).some((x) => x.id === parent)) throw new Error('Impossible : une liste ne va pas dans elle-même ou un de ses sous-dossiers.');
        r.freres.splice(r.rang, 1);
        inserer(r.l, { parent: parent ?? undefined, position: a.position as number | undefined });
        return resultat(`✔ Déplacement de « ${r.l.nom} »`);
      }
      case 'decaler_liste': {
        verifierEcriture();
        const r = exiger(id);
        const j = r.rang + (a.haut ? -1 : 1);
        if (j < 0 || j >= r.freres.length) return resultat('');
        r.freres.splice(r.rang, 1);
        r.freres.splice(j, 0, r.l);
        return resultat(`✔ Déplacement de « ${r.l.nom} »`);
      }
      case 'place_depot_liste':
        return etat().modifiable && placeValide(String(a.glissee), String(a.cible), a.place as PlaceListe);
      case 'deposer_liste': {
        verifierEcriture();
        const g = exiger(String(a.glissee));
        const place = a.place as PlaceListe;
        if (!placeValide(g.l.id, String(a.cible), place)) throw new Error('Dépôt impossible à cet endroit.');
        g.freres.splice(g.rang, 1);
        const c = exiger(String(a.cible));
        if (place === 'dans') (c.l.enfants ??= []).push(g.l);
        else c.freres.splice(c.rang + (place === 'apres' ? 1 : 0), 0, g.l);
        return resultat(`✔ Déplacement de « ${g.l.nom} »`);
      }
      case 'infos_conversion': {
        const l = exiger(id).l;
        const m = membres(l);
        if ('inconnu' in m) throw new Error(`« ${l.nom} » ne peut pas être convertie : ComicRust ne connaît pas son contenu (${m.inconnu}).`);
        return { nom: l.nom, nbLivres: m.ids.length };
      }
      case 'convertir_en_lecture': {
        verifierEcriture();
        const l = exiger(id).l;
        const m = membres(l);
        if ('inconnu' in m) throw new Error(`« ${l.nom} » ne peut pas être convertie : ComicRust ne connaît pas son contenu (${m.inconnu}).`);
        l.type = 'lecture';
        l.ids = m.ids;
        delete l.criteres;
        return resultat(`✔ « ${l.nom} » est maintenant une liste de lecture (${m.ids.length} livre${pluriel(m.ids.length)})`);
      }
      case 'depot_livres_possible': {
        const l = exiger(id).l;
        if (l.type === 'bibliotheque') return 'La bibliothèque contient déjà tous les livres';
        if (l.type === 'dossier') return 'Dossier : dépose les livres sur une liste de lecture';
        if (l.type === 'intelligente') return 'Liste intelligente : son contenu dépend de ses critères';
        if (a.listeAffichee && String(a.listeAffichee).toLowerCase() === l.id.toLowerCase()) return 'Les livres sont déjà dans cette liste';
        return etat().lectureSeule ?? null;
      }
      case 'ajouter_livres_liste': {
        verifierEcriture();
        const l = exiger(id).l;
        if (l.type !== 'lecture') throw new Error('Seule une liste de lecture reçoit des livres.');
        const ens = new Set(l.ids);
        const nouveaux = [...new Set(a.livres as string[])].filter((x) => !ens.has(x));
        const pos = a.position as number | undefined;
        l.ids!.splice(pos === undefined ? l.ids!.length : pos, 0, ...nouveaux);
        const deja = (a.livres as string[]).length - nouveaux.length;
        return resultat(`✔ ${nouveaux.length} livre${pluriel(nouveaux.length)} ajouté${pluriel(nouveaux.length)} à « ${l.nom} »${deja ? ` (${deja} déjà dans la liste)` : ''}`);
      }
      case 'retirer_livres_liste': {
        verifierEcriture();
        const l = exiger(id).l;
        if (l.type !== 'lecture') throw new Error('Seule une liste de lecture peut perdre des livres.');
        const ens = new Set(a.livres as string[]);
        const avant = l.ids!.length;
        l.ids = l.ids!.filter((x) => !ens.has(x));
        const n = avant - l.ids.length;
        return resultat(`✔ ${n} livre${pluriel(n)} retiré${pluriel(n)} de « ${l.nom} »`);
      }
      case 'decaler_livres_liste': {
        verifierEcriture();
        const l = exiger(id).l;
        if (l.type !== 'lecture') throw new Error('Seule une liste de lecture se réordonne.');
        const ens = new Set(a.livres as string[]);
        const pos = l.ids!.map((x, i) => (ens.has(x) ? i : -1)).filter((i) => i >= 0);
        if (!pos.length) return resultat('');
        if (a.haut) {
          if (pos[0] === 0) return resultat('');
          placer(l, ens, l.ids![pos[0] - 1]);
        } else {
          const der = pos[pos.length - 1];
          if (der === l.ids!.length - 1) return resultat('');
          const reste = l.ids!.filter((x) => !ens.has(x));
          const k = reste.indexOf(l.ids![der + 1]);
          placer(l, ens, reste[k + 1] ?? null);
        }
        return resultat(`✔ Réordonnancement de « ${l.nom} »`);
      }
      case 'deplacer_livres_liste': {
        verifierEcriture();
        const l = exiger(id).l;
        const vue = vueListe(id, a.requete as RequeteLivres);
        const ens = new Set(a.livres as string[]);
        let k = Number(a.insertion);
        while (k < vue.length && ens.has(vue[k])) k++;
        placer(l, ens, k < vue.length ? vue[k] : null);
        return resultat(`✔ Réordonnancement de « ${l.nom} »`);
      }
      case 'groupes_vue':
        return groupes(a.requete as RequeteLivres).map(({ titre, nombre: n }) => ({ titre, nombre: n }));
      case 'deplacer_groupes_liste': {
        verifierEcriture();
        const l = exiger(id).l;
        vueListe(id, a.requete as RequeteLivres);
        const gs = groupes(a.requete as RequeteLivres);
        const choisis = new Set(a.groupes as string[]);
        const ens = new Set(gs.filter((g) => choisis.has(g.titre)).flatMap((g) => g.ids));
        let k = Number(a.insertion);
        while (k < gs.length && choisis.has(gs[k].titre)) k++;
        placer(l, ens, k < gs.length ? gs[k].ids[0] : null);
        return resultat(`✔ Réordonnancement de « ${l.nom} »`);
      }
      case 'decaler_groupe_liste': {
        verifierEcriture();
        const l = exiger(id).l;
        vueListe(id, a.requete as RequeteLivres);
        const gs = groupes(a.requete as RequeteLivres);
        const k = gs.findIndex((g) => g.titre === a.groupe);
        if (k < 0) throw new Error('Groupe introuvable.');
        const j = k + (a.haut ? -1 : 2);
        if ((a.haut && k === 0) || (!a.haut && k === gs.length - 1)) return resultat('');
        placer(l, new Set(gs[k].ids), j < gs.length ? gs[j].ids[0] : null);
        return resultat(`✔ Réordonnancement de « ${l.nom} »`);
      }
      case 'catalogue_criteres': {
        const noms = [...new Set(catalogue.map((c) => c.categorie))];
        const r: CatalogueCriteres = {
          categories: noms.map((nom) => ({
            nom,
            criteres: catalogue.filter((c) => c.categorie === nom).map(({ lire: _l, categorie: _c, ...c }) => c).sort((x, y) => x.libelle.localeCompare(y.libelle, 'fr')),
          })),
          defaut: { type: 'ComicBookSeriesMatcher', op: 1 },
        };
        return r;
      }
      case 'nouveau_critere': {
        const t = infoDe(String(a.typeCritere));
        if (!t) throw new Error('Type de critère inconnu.');
        const ancien = a.ancien as Critere | undefined;
        const base: Critere = { genre: 'critere', non: ancien?.non ?? false, type: t.type, op: t.operateurs[0].op, valeur: '', valeur2: '', ignorerCasse: ancien?.ignorerCasse ?? true, modifiable: true, origine: ancien?.origine };
        const avant = ancien && infoDe(ancien.type);
        if (ancien && avant && avant.nature === t.nature) {
          base.op = t.operateurs.some((o) => o.op === ancien.op) ? ancien.op : t.operateurs[0].op;
          base.valeur = ancien.valeur;
          base.valeur2 = ancien.valeur2;
        }
        return base;
      }
      case 'lire_criteres': {
        const e = etat();
        let r: EditeurListe;
        if (a.id) {
          const l = exiger(id).l;
          if (!l.criteres) throw new Error('Ce n\'est pas une liste intelligente.');
          r = { id: l.id, criteres: { ...copie(l.criteres), nom: l.nom, description: l.description }, listesDepart: listesDepart(l.id) };
        } else {
          r = { criteres: crit('Nouvelle liste intelligente', 'tous', [{ ...critere('ComicBookSeriesMatcher', 1), origine: undefined }]), listesDepart: listesDepart() };
        }
        if (!e.modifiable) r.lectureSeule = e.lectureSeule;
        return r;
      }
      case 'apercu_criteres': {
        const c = a.criteres as CriteresListe;
        const err = erreursDe(c);
        if (err.length) return { erreurs: err } satisfies Apercu;
        const m = evaluer(c, new Set(a.id ? [String(a.id)] : []));
        return ('inconnu' in m ? { inconnu: m.inconnu, erreurs: [] } : { nbLivres: m.ids.length, erreurs: [] }) satisfies Apercu;
      }
      case 'enregistrer_criteres': {
        verifierEcriture();
        const c = copie(a.criteres as CriteresListe);
        const nom = c.nom.trim();
        if (!nom) throw new Error('Nom vide.');
        const err = erreursDe(c);
        if (err.length) throw new Error(err.join('\n'));
        if (c.reinitialiserRetires) c.retires = 0;
        delete c.reinitialiserRetires;
        if (a.id) {
          const l = exiger(id).l;
          if (c.depart && !listesDepart(l.id).some((x) => x.id === c.depart)) throw new Error('Cette liste de départ créerait une boucle.');
          l.nom = nom;
          l.description = c.description;
          l.criteres = c;
          return resultat(`✔ Modification de « ${nom} »`);
        }
        const l = I(nouvelId(), nom, c, c.description);
        inserer(l, a as { parent?: string; position?: number; apresListe?: string });
        return resultat(`✔ Création de « ${nom} »`, { selection: l.id, creee: l.id });
      }
      case 'analyser_cbl': {
        const { ids: _ids, ...r } = rapportCbl(String(a.chemin ?? '').trim().replace(/^"|"$/g, ''));
        return r;
      }
      case 'importer_cbl': {
        verifierEcriture();
        const { ids, ...rapport } = rapportCbl(String(a.chemin ?? '').trim().replace(/^"|"$/g, ''));
        const nom = (a.nom as string | undefined)?.trim() || rapport.nom;
        const l = L(nouvelId(), nom, ids, `Importée de ${String(a.chemin)}`, 0);
        inserer(l, a as { parent?: string; position?: number; apresListe?: string });
        return {
          resultat: resultat(`✔ Liste « ${nom} » importée : ${rapport.trouves} / ${rapport.total} numéros trouvés`, { selection: l.id, creee: l.id }),
          rapport,
        };
      }
      case 'exporter_cbl': {
        const l = exiger(id).l;
        let chemin = String(a.chemin ?? '').trim();
        if (!chemin) throw new Error('Chemin vide.');
        if (!/\.cbl$/i.test(chemin)) chemin += '.cbl';
        const m = membres(l);
        return { chemin, livres: 'ids' in m ? m.ids.length : 0 };
      }
      default:
        return undefined;
    }
  }

  /** Livres de la liste dans son ordre (lister_livres `liste`) ; contenu inconnu : aucun livre. */
  function livresDe(id: string): string[] {
    const m = membres(exiger(id).l);
    return 'ids' in m ? m.ids : [];
  }

  return { appeler, livresDe };
}
