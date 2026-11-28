// MODE SIMULÉ : faux backend utilisé uniquement hors de Tauri (navigateur), pour développer et
// tester le front avec 5 000 livres factices. Il imite le contrat IPC de api.ts, y compris la
// latence et les événements de scan. Rien de ceci n'est utilisé dans la fenêtre Tauri.
import type {
  PreferencesVue, ClefTri, ChoixMysql, ConnexionsMysql, EtatBiblio, ParametresMysql, LivreDetail, LivreOuvert, LivreResume, PageInfo, RequeteLivres,
  ReponseLivres, ReponsePiles, Pile,
} from './api';
import { creerEditionSimulee, LECTURE_SEULE_FICHIERS } from './simuleEdition';
import { creerBaseSimulee } from './simuleBase';
import { creerListesSimulees } from './simuleListes';
import { creerTransfertSimule } from './simuleTransfert';
import { creerMajSimulee } from './simuleMaj';
import { creerScanSimule } from './simuleScan';
import { COMMANDES_REGLAGES, creerReglagesSimules } from './simuleReglages';
import { creerScriptsSimules } from './simuleScripts';
import { creerManquantsSimules } from './simuleManquants';
import { creerLectureEtatSimulee } from './simuleLectureEtat';
import { COMMANDES_THEMES, creerThemesSimules, themesRefuses } from './simuleThemes';
import { COMMANDES_FENETRE, creerFenetreSimulee } from './simuleFenetre';
import { COMMANDES_RECENTS, creerRecentsSimules } from './simuleLecteur';
import { comparerTitres, titreGroupe } from './simuleGroupes';

const NB_LIVRES = Number(new URLSearchParams(location.search).get('n')) || 5000;
const LATENCE_MS = 12;
const pause = (ms: number) => new Promise((r) => setTimeout(r, ms));

// ───────────── Génération déterministe des livres ─────────────

function alea(graine: number) {
  let s = graine >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const SERIES = [
  'Thanos', 'Les Gardiens de la Galaxie', 'Astérix', 'Blake et Mortimer', 'Lanfeust de Troy',
  'Spirou et Fantasio', 'Batman : Année Un', 'Saga', 'Paper Girls', 'Les Cités obscures',
  'Largo Winch', 'XIII', 'Tintin', 'Les Tuniques bleues', 'Walking Dead', 'Watchmen',
  'Ms. Marvel', 'Usagi Yojimbo', 'Le Chat du Rabbin', 'Corto Maltese', 'Valérian', 'Persepolis',
  'Fables', 'Hellboy', 'Sandman', 'Les Passagers du vent', 'Bone', 'Invincible', 'Monstress', 'Prophet',
];
const EDITEURS = ['Panini Comics', 'Dupuis', 'Dargaud', 'Delcourt', 'Glénat', 'Urban Comics', 'Casterman', 'Soleil', 'Image Comics', 'Le Lombard'];
const GENRES = ['Science-fiction', 'Aventure', 'Super-héros', 'Fantasy', 'Policier', 'Humour', 'Drame'];
const NOMS = ['Jean Dupont', 'Marie Lambert', 'Hugo Martin', 'Léa Bernard', 'Tom Moreau', 'Sarah Petit', 'Paul Roux', 'Éva Girard'];
const MOTS = ['Le retour', 'La chute', "L'ombre", 'Origines', 'Rédemption', 'Le dernier jour', 'Les héritiers', 'Tempête', 'Sang neuf', 'Le pacte', 'Au-delà', 'Mémoire', 'La guerre des mondes', 'Le secret', 'Crépuscule'];

interface LivreInterne extends LivreResume {
  genre: string; scenariste: string; dessinateur: string; mois: number; ajout: number;
  resume: string; fichier: string; fichierPresent: boolean; pageCourante: number; note: number;
  /** Relecture en cours d'un livre lu (`pageCourante` = position de la relecture). */
  enRelecture?: boolean;
  /** Page la plus loin vue dans ComicRust (comme `Avancement::page_max`) : le Détail l'affiche. */
  pageMax?: number;
}

const couvertureCache = new Map<string, string>();
function couverture(serie: string, titre: string, numero: string): string {
  const clef = serie + '#' + numero;
  let u = couvertureCache.get(clef);
  if (!u) {
    let h = 0;
    for (const c of serie) h = (h * 31 + c.charCodeAt(0)) % 360;
    const echap = (t: string) => t.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
    const svg =
      `<svg xmlns="http://www.w3.org/2000/svg" width="200" height="300" viewBox="0 0 200 300">` +
      `<rect width="200" height="300" fill="hsl(${h} 55% 38%)"/>` +
      `<rect y="170" width="200" height="130" fill="hsl(${(h + 40) % 360} 60% 22%)"/>` +
      `<circle cx="150" cy="80" r="46" fill="hsl(${(h + 150) % 360} 70% 62%)" opacity=".85"/>` +
      `<text x="14" y="34" font-family="sans-serif" font-size="17" font-weight="700" fill="#fff">${echap(serie.slice(0, 20))}</text>` +
      `<text x="14" y="276" font-family="sans-serif" font-size="30" font-weight="700" fill="#fff">#${echap(numero)}</text></svg>`;
    u = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
    couvertureCache.set(clef, u);
  }
  return u;
}

function genererLivres(n: number): LivreInterne[] {
  const r = alea(42);
  const livres: LivreInterne[] = [];
  for (let i = 1; i <= n; i++) {
    const serie = SERIES[Math.floor(r() * SERIES.length)];
    const numero = String(1 + Math.floor(r() * 60));
    const sansSerie = r() < 0.04;
    const editeur = EDITEURS[SERIES.indexOf(serie) % EDITEURS.length];
    const titre = `${MOTS[Math.floor(r() * MOTS.length)]} (${i})`;
    livres.push({
      id: String(i),
      titre,
      serie: sansSerie ? undefined : serie,
      numero: sansSerie ? undefined : numero,
      editeur: r() < 0.03 ? undefined : editeur,
      annee: 1960 + Math.floor(r() * 66),
      pages: 24 + Math.floor(r() * 180),
      lu: r() < 0.35,
      urlCouverture: couverture(serie, titre, numero),
      genre: GENRES[Math.floor(r() * GENRES.length)],
      scenariste: NOMS[Math.floor(r() * NOMS.length)],
      dessinateur: NOMS[Math.floor(r() * NOMS.length)],
      mois: 1 + Math.floor(r() * 12),
      ajout: i,
      resume: 'Résumé factice généré par le mode simulé. '.repeat(1 + Math.floor(r() * 4)).trim(),
      // Un livre sur cinq en CBR (conversion CBR → CBZ, écriture des infos refusée).
      fichier: `D:\\BD\\${serie.replace(/[^\w ]/g, '')}\\${serie.replace(/[^\w ]/g, '')} ${numero.padStart(3, '0')}.${i % 5 === 0 ? 'cbr' : 'cbz'}`,
      fichierPresent: r() > 0.02,
      pageCourante: 0,
      note: Math.floor(r() * 11) / 2,
    });
  }
  return livres;
}

const LIVRES = genererLivres(NB_LIVRES);

// ───────────── Requêtes (filtre, tri, groupe) avec cache de l'ordre ─────────────

/** Arc, format et avancement simulés (lot L5) : un livre sur sept a un arc, le format suit l'extension. */
const arcDe = (l: LivreInterne) => (Number(l.id) % 7 === 0 ? `Arc ${1 + (Number(l.id) % 3)}` : undefined);
const formatDe = (l: LivreInterne) => (l.fichier.endsWith('.cbr') ? 'CBR' : 'CBZ');
const avancementDe = (l: LivreInterne) => (l.lu ? 100 : l.pageCourante > 0 ? Math.max(1, Math.round((l.pageCourante / Math.max(1, l.pages)) * 100)) : 0);
/** Titre de groupe d'un livre pour un regroupement (même règles que le backend). */
const groupeDe = (l: LivreInterne, g: RequeteLivres['groupe']) =>
  titreGroupe({ ...l, arc: arcDe(l), format: formatDe(l), avancement: avancementDe(l) }, g);

const cle = (l: LivreInterne, c: ClefTri): string | number => {
  switch (c) {
    case 'titre': return l.titre;
    case 'serie': return (l.serie ?? '') + ' ' + String(l.numero ?? 0).padStart(4, '0');
    case 'parution': return (l.annee ?? 0) * 100 + l.mois;
    case 'ajout': return l.ajout;
    case 'lecture': return l.lu ? l.ajout : 0;
    case 'note': return l.note;
    case 'pages': return l.pages;
    case 'avancement': return l.lu ? 100 : l.pageCourante;
    case 'taille': return l.pages * 310_000;
    case 'fichier': return l.fichier;
    case 'ordre': return l.ajout; // sans liste : ordre de la bibliothèque
  }
};
const cmp = (a: string | number, b: string | number) =>
  typeof a === 'number' && typeof b === 'number' ? a - b : String(a).localeCompare(String(b), 'fr');

let ordreCle = '';
let ordre: LivreInterne[] = LIVRES;

function ordonner(q: RequeteLivres): LivreInterne[] {
  // Étape 5 : livres d'une liste (contenu recalculé à chaque requête : les listes changent).
  const membres = q.liste === '@manquants' ? mq.membres() : q.liste ? listes.livresDe(q.liste) : null;
  const k = JSON.stringify([q.recherche ?? '', q.tri, q.sensTri, q.groupe ?? 'aucun']);
  if (!membres && k === ordreCle) return ordre;
  const position = new Map((membres ?? []).map((id, i) => [id, i]));
  const base = membres ? membres.map((id) => LIVRES[Number(id) - 1]).filter((l) => l && !mq.retire(l.id)) : mq.nbRetires() ? LIVRES.filter((l) => !mq.retire(l.id)) : LIVRES;
  const mot = (q.recherche ?? '').trim().toLowerCase();
  const liste = mot
    ? base.filter((l) => {
        const texte = `${l.titre} ${l.serie ?? ''} ${l.editeur ?? ''} ${l.fichier}`.toLowerCase();
        return mot.split(/\s+/).every((m) => texte.includes(m)); // tous les mots requis, comme le backend
      })
    : base.slice();
  const sens = q.sensTri === 'desc' ? -1 : 1;
  const groupe = q.groupe && q.groupe !== 'aucun' ? q.groupe : null;
  if (membres && q.tri === 'ordre') {
    // « # » : ordre de la liste ; groupés, les groupes suivent la place de leur premier livre
    // (déplacer une série entière dans la liste la déplace dans la vue, recette v0.10.2 S-01).
    const pos = (l: LivreInterne) => position.get(l.id) ?? 0;
    const rangGroupe = new Map<string, number>();
    if (groupe) {
      for (const l of liste) {
        const g = groupeDe(l, groupe).toLowerCase();
        rangGroupe.set(g, Math.min(rangGroupe.get(g) ?? Infinity, pos(l)));
      }
    }
    liste.sort((a, b) => (groupe ? (rangGroupe.get(groupeDe(a, groupe).toLowerCase())! - rangGroupe.get(groupeDe(b, groupe).toLowerCase())!) * sens : 0) || sens * (pos(a) - pos(b)));
    return liste;
  }
  liste.sort((a, b) => {
    if (groupe) {
      // Comme le backend : titres comparés en minuscules, sens inversé en tri décroissant.
      const g = comparerTitres(groupeDe(a, groupe), groupeDe(b, groupe));
      if (g) return sens * g;
    }
    return sens * cmp(cle(a, q.tri), cle(b, q.tri)) || Number(a.id) - Number(b.id);
  });
  if (membres) return liste;
  ordreCle = k;
  ordre = liste;
  return liste;
}

/** Vue d'une requête, restreinte à la pile ouverte (`serie`) s'il y en a une. */
const titrePile = (l: LivreInterne) => l.serie ?? '(sans série)';
function ordonnerPile(q: RequeteLivres): LivreInterne[] {
  const liste = ordonner(q);
  return q.serie === undefined ? liste : liste.filter((l) => titrePile(l) === q.serie);
}

const resume = (l: LivreInterne, g?: RequeteLivres['groupe']): LivreResume => ({
  id: l.id, titre: l.titre, serie: l.serie, numero: l.numero, editeur: l.editeur,
  annee: l.annee, pages: l.pages, lu: l.lu, urlCouverture: l.urlCouverture,
  mois: l.mois, scenariste: l.scenariste, note: l.note || undefined, fichier: l.fichier,
  ajoute: `2024-${String(1 + (l.ajout % 12)).padStart(2, '0')}-${String(1 + (l.ajout % 28)).padStart(2, '0')}T10:00:00`,
  avancement: avancementDe(l) || undefined,
  ...(g && g !== 'aucun' ? { groupe: groupeDe(l, g) } : {}),
  ...(mq.est(l.id) ? { manquant: true } : {}),
});

// ───────────── Événements simulés ─────────────

const bus = new EventTarget();
const emettre = (nom: string, charge: unknown) => bus.dispatchEvent(new CustomEvent(nom, { detail: charge }));

export function ecouterSimule(nom: string, rappel: (c: unknown) => void): () => void {
  const f = (e: Event) => rappel((e as CustomEvent).detail);
  bus.addEventListener(nom, f);
  simuleTests.ecoutes[nom] = (simuleTests.ecoutes[nom] ?? 0) + 1;
  let retire = false;
  return () => {
    if (retire) return;
    retire = true;
    simuleTests.ecoutes[nom]--;
    bus.removeEventListener(nom, f);
  };
}

// Analyse des dossiers, arrêt, bilan, analyse du démarrage et réglages : voir simuleScan.ts.

// ───────────── État simulé ─────────────

// `?mode=comicrack` : bibliothèque liée à ComicRack (lecture seule tant que « Lecture et écriture »
// n'est pas cochée dans Réglages).
// `?baseAbsente` : copie autonome sans data\comicrust.db (assistant avec « Créer une nouvelle base »).
// `?erreurOuverture` : ouverture impossible (message d'erreur avec « Réessayer » / « Choisir… »).
let etat: EtatBiblio = {
  mode: new URLSearchParams(location.search).get('mode') === 'comicrack' ? 'comicrack' : 'autonome',
  nbLivres: LIVRES.length,
  premierDemarrage: new URLSearchParams(location.search).has('premier') || new URLSearchParams(location.search).has('baseAbsente'),
  source: 'sqlite',
  ...(new URLSearchParams(location.search).has('baseAbsente') ? { baseAbsente: 'C:\\ComicRust\\data\\comicrust.db' } : {}),
};
let erreurOuverture = new URLSearchParams(location.search).has('erreurOuverture');
// Connexions MySQL « enregistrées » (sans mot de passe : seulement `motDePasseDefini`).
let connexionsSimulees: ConnexionsMysql = { store: 'sqlite' };
// Préférences d'affichage : repli en mémoire (perdues au rechargement). `?prefs=<json>` simule un
// redémarrage avec des préférences déjà enregistrées ; `window.__comicrustSimule` les expose aux tests.
interface Avancement { jeton: string; page: number; termine: boolean }
const simuleTests: {
  prefs: PreferencesVue; requetes: RequeteLivres[]; avancements: Avancement[]; ouverts: string[]; fermes: string[];
  /** Commandes d'édition reçues (étape 3), pour vérifier ce que le front envoie. */
  edition: Array<{ commande: string; args: Record<string, unknown> }>;
  /** Piles absentes de `lister_piles` (simule des séries qui disparaissent : les rangs des piles suivantes
   *  se décalent, pour vérifier que la sélection est retrouvée par titre). */
  pilesMasquees: Set<string>;
  /** Thèmes refusés par `charger_theme` (voir simuleThemes). */
  themesRefuses: Set<string>;
  /** Émet un événement du backend simulé (ex. `scan-termine`). */
  emettre: (nom: string, charge: unknown) => void;
  /** Taille de chaque demande `rangs_vue` (suivi de la sélection par identifiant). */
  rangs: number[];
  /** Écoutes actives par événement (fuites d'abonnement). */
  ecoutes: Record<string, number>;
  /** Retards (ms) appliqués APRÈS le calcul de la réponse, un par appel de la commande : une
   *  réponse calculée tôt arrive tard (réponses périmées, courses entre requêtes). */
  retardsApres: Record<string, number[]>;
} = { rangs: [], ecoutes: {}, retardsApres: {}, prefs: {}, requetes: [], avancements: [], ouverts: [], fermes: [], edition: [], pilesMasquees: new Set(), themesRefuses, emettre: (n, c) => emettre(n, c) };
// `?persistance` : les préférences survivent au rechargement de la page (sessionStorage), pour simuler un
// redémarrage de l'application ; `?prefs=<json>` ne s'applique alors qu'au tout premier chargement.
const persistance = new URLSearchParams(location.search).has('persistance');
const CLEF_PERSISTANCE = 'comicrust.simule.prefs';
try {
  const brut = new URLSearchParams(location.search).get('prefs');
  const gardees = persistance ? sessionStorage.getItem(CLEF_PERSISTANCE) : null;
  if (gardees) simuleTests.prefs = JSON.parse(gardees) as PreferencesVue;
  else if (brut) simuleTests.prefs = JSON.parse(brut) as PreferencesVue;
} catch {
  /* paramètre illisible : préférences vides */
}
(globalThis as { __comicrustSimule?: typeof simuleTests }).__comicrustSimule = simuleTests;

// ───────────── Lecteur simulé ─────────────
// Pages factices : la couverture, une double page (paysage) toutes les 9 pages à partir de la 7e
// (comme l'échantillon de la recette N-09), des pages sans dimensions connues (déduites au
// chargement), une page supprimée (la 5e) pour les livres d'identifiant pair, la 12e tournée de 180° pour le
// livre n° 1, et pour le livre n° 3 la 2e tournée de 90° et la 3e de 270° (miniatures, E-03). Les livres
// dont l'identifiant finit par 7 sont marqués manga.
const sessions = new Map<string, LivreInterne>();
let numeroSession = 0;
const REPRISE_DIFFERENTE = new URLSearchParams(location.search).has('repriseDiff');
// `?latenceAvancement=ms` : `enregistrer_avancement` répond (et note la page) après ce délai, comme
// un backend lent ; sert à vérifier que la fermeture attend l'envoi en cours (C-06, tauri-v0.7.3).
const LATENCE_AVANCEMENT = Number(new URLSearchParams(location.search).get('latenceAvancement')) || 0;
/** Séances simulées : livre déjà lu à l'ouverture (relecture) et fin revue pendant la séance. */
const relectures = new Map<string, { dejaLu: boolean; finVue: boolean }>();

function pagesSimulees(l: LivreInterne): PageInfo[] {
  // Ordre et attributs enregistrés par l'onglet Pages (étape 4), s'ils ont changé.
  const fiche = edition.pages.ordre(l.id);
  if (fiche) {
    return fiche.map((p, i) => {
      const double = p.image >= 6 && (p.image - 6) % 9 === 0;
      const sansDims = p.image % 13 === 4;
      return {
        index: i, image: p.image, largeur: sansDims ? undefined : double ? 2000 : 1000, hauteur: sansDims ? undefined : 1500,
        double: double && !sansDims, type: p.type, supprimee: p.type === 'Deleted', rotation: p.rotation,
      };
    });
  }
  const pages: PageInfo[] = [];
  for (let i = 0; i < l.pages; i++) {
    const double = i >= 6 && (i - 6) % 9 === 0;
    const sansDims = i % 13 === 4;
    pages.push({
      index: i,
      image: i,
      largeur: sansDims ? undefined : double ? 2000 : 1000,
      hauteur: sansDims ? undefined : 1500,
      double: double && !sansDims,
      type: i === 0 ? 'FrontCover' : 'Story',
      supprimee: i === 4 && Number(l.id) % 2 === 0,
      rotation: l.id === '1' && i === 11 ? 180 : l.id === '3' ? ({ 1: 90, 2: 270 } as Record<number, number>)[i] ?? 0 : 0,
    });
  }
  return pages;
}

let dossiers = ['D:\\BD\\Marvel', 'D:\\BD\\Franco-belge', '\\\\NAS\\comics\\Image'];

// Thèmes (étape 8) : voir simuleThemes.ts.
const themesSimules = creerThemesSimules();

// Surveillance de la base et hors ligne (étape 6) : voir simuleBase.ts.
const base = creerBaseSimulee(emettre, () => etat.mode);

// Analyse des dossiers (lot « Scan et dossiers ») : voir simuleScan.ts.
const scanSimule = creerScanSimule({ emettre, pause, mode: () => etat.mode, horsLigne: base.motif, dossiers: () => dossiers });

// Édition (étape 3) : voir simuleEdition.ts.
const edition = creerEditionSimulee({
  livres: LIVRES,
  mode: () => etat.mode,
  horsLigne: base.motif,
  emettre,
  invalider: () => { ordreCle = ''; },
  pause,
});

// Listes ComicRack (étape 5) : voir simuleListes.ts. `?comicrackOuvert` : ComicRack lancé (mode lié).
const listes = creerListesSimulees({
  livres: LIVRES,
  lectureSeule: () => {
    const h = base.motif();
    if (h) return h;
    if (etat.mode !== 'comicrack') return undefined;
    const r = edition('lire_reglages_edition', {}) as { ecritureComicrack: boolean };
    return r.ecritureComicrack ? undefined : 'lecture seule';
  },
  comicrackLance: () => etat.mode === 'comicrack' && new URLSearchParams(location.search).has('comicrackOuvert'),
  vue: (q) => ordonnerPile(q),
  titreGroupe: (l, g) => groupeDe(l as LivreInterne, g as RequeteLivres['groupe']),
});

// Transfert ComicRack → ComicRust (étape 6) : voir simuleTransfert.ts.
const transfert = creerTransfertSimule({
  emettre,
  pause,
  etat: () => etat,
  basculer: () => {
    etat = { ...etat, mode: 'autonome' };
    return etat;
  },
});

// Mise à jour (tauri-v0.8.0) : voir simuleMaj.ts.
const maj = creerMajSimulee({ emettre, pause });

// Aide, assistant rejouable, Relire, Copier les livres (lot L7) : voir simuleReglages.ts.
const reglagesSimules = creerReglagesSimules({ etat: () => etat, connexions: () => connexionsSimulees, livres: () => LIVRES });

// Fenêtre et lancement (lot L2) : voir simuleFenetre.ts.
const fenetreSimulee = creerFenetreSimulee();

// Fichiers récents (lot L6) : voir simuleLecteur.ts.
const recentsSimules = creerRecentsSimules();

// Scripts Python (tauri-v0.9.0) : voir simuleScripts.ts.
const scriptsSimules = creerScriptsSimules({
  emettre,
  pause,
  livre: (id) => LIVRES[Number(id) - 1],
  lectureSeule: () => {
    const h = base.motif();
    if (h) return h;
    if (etat.mode !== 'comicrack') return undefined;
    const r = edition('lire_reglages_edition', {}) as { ecritureComicrack: boolean };
    return r.ecritureComicrack ? undefined : 'Bibliothèque ComicRack en lecture seule (cocher « Lecture et écriture » dans Réglages).';
  },
});

// Livres manquants (lot L3) : voir simuleManquants.ts.
const mq = creerManquantsSimules({
  livres: LIVRES,
  lectureSeule: () => {
    const h = base.motif();
    if (h) return h;
    if (etat.mode !== 'comicrack') return undefined;
    const r = edition('lire_reglages_edition', {}) as { ecritureComicrack: boolean };
    return r.ecritureComicrack ? undefined : 'Bibliothèque ComicRack en lecture seule (cocher « Lecture et écriture » dans Réglages).';
  },
  // Actions sur les fichiers des livres : texte propre aux fichiers (comme `raison_fichiers`).
  lectureSeuleFichiers: () => {
    const h = base.motif();
    if (h) return h;
    if (etat.mode !== 'comicrack') return undefined;
    const r = edition('lire_reglages_edition', {}) as { ecritureComicrack: boolean };
    return r.ecritureComicrack ? undefined : LECTURE_SEULE_FICHIERS;
  },
  emettre,
  pause,
  invalider: () => { ordreCle = ''; },
});

// « Marquer comme lu / non lu » (lot menu du livre) : voir simuleLectureEtat.ts.
const lectureEtat = creerLectureEtatSimulee({
  livres: LIVRES,
  lectureSeule: () => {
    const h = base.motif();
    if (h) return h;
    if (etat.mode !== 'comicrack') return undefined;
    const r = edition('lire_reglages_edition', {}) as { ecritureComicrack: boolean };
    return r.ecritureComicrack ? undefined : 'Bibliothèque ComicRack en lecture seule (cocher « Lecture et écriture » dans Réglages).';
  },
  emettre,
  invalider: () => { ordreCle = ''; },
});

export async function appelerSimule(commande: string, args: Record<string, unknown>): Promise<unknown> {
  const r = await appelerSimuleBrut(commande, args);
  const retard = simuleTests.retardsApres[commande]?.shift();
  if (retard) await pause(retard);
  return r;
}

async function appelerSimuleBrut(commande: string, args: Record<string, unknown>): Promise<unknown> {
  await pause(LATENCE_MS);
  const rl = lectureEtat.appeler(commande, args);
  if (rl !== undefined) return rl;
  const rm = maj.appeler(commande, args);
  if (rm !== undefined) return rm;
  if (COMMANDES_THEMES.includes(commande)) return themesSimules.appeler(commande, args);
  if (COMMANDES_REGLAGES.includes(commande)) return reglagesSimules.appeler(commande, args);
  if (COMMANDES_FENETRE.includes(commande)) return fenetreSimulee.appeler(commande, args);
  if (COMMANDES_RECENTS.includes(commande)) return recentsSimules.appeler(commande, args);
  if (commande.startsWith('script')) {
    const rs = scriptsSimules.appeler(commande, args);
    if (rs !== undefined) return rs;
  }
  const rq = mq.appeler(commande, args);
  if (rq !== undefined) return rq;
  switch (commande) {
    case 'infos':
      return {
        version: '0.0.0-simule', dossier_data: '(mode simulé) data', dossier_themes: '(mode simulé) data/themes',
        dossier_webview: '(mode simulé) data/webview2',
      };
    case 'lire_preferences_vue':
      return { ...simuleTests.prefs };
    case 'enregistrer_preferences_vue':
    {
      // Comme le backend : fusion champ par champ, et liste par liste pour `listes` (`null` efface).
      const m = args.preferences as PreferencesVue;
      const listes = { ...(simuleTests.prefs.listes ?? {}) };
      for (const [id, v] of Object.entries(m.listes ?? {})) {
        if (v) listes[id] = v;
        else delete listes[id];
      }
      simuleTests.prefs = { ...simuleTests.prefs, ...m, listes: Object.keys(listes).length ? listes : undefined };
      // Comme le backend : une chaîne vide efface la dernière liste (retour à la bibliothèque).
      if (m.derniereListe === '') delete simuleTests.prefs.derniereListe;
      if (persistance) {
        try {
          sessionStorage.setItem(CLEF_PERSISTANCE, JSON.stringify(simuleTests.prefs));
        } catch {
          /* stockage indisponible */
        }
      }
    }
      return { ...simuleTests.prefs };
    case 'ouvrir_bibliotheque':
      if (erreurOuverture) {
        erreurOuverture = false; // « Réessayer » réussit
        throw new Error('connexion à comicrust@nas:3306/comicrust impossible : délai dépassé');
      }
      return etat;
    case 'configurer_premier_demarrage': {
      const choix = args.choix as { mode: EtatBiblio['mode'] };
      etat = { mode: choix.mode, nbLivres: LIVRES.length, premierDemarrage: false, source: choix.mode === 'comicrack' ? 'xml' : 'sqlite' };
      return etat;
    }
    // Étape 6 : connexion MySQL. Serveur simulé : « ok » accepte tout, « injoignable » (ou toute
    // autre valeur de `serveur` vide) échoue ; le mot de passe n'est jamais gardé ni renvoyé.
    case 'test_connexion_mysql': {
      const p = args.parametres as ParametresMysql;
      if (!p.serveur.trim() || !p.utilisateur.trim() || !p.base.trim()) throw new Error('serveur, utilisateur et base sont nécessaires');
      if (p.serveur.trim() === 'injoignable') throw new Error(`connexion à ${p.utilisateur}@${p.serveur}:${p.port}/${p.base} impossible : délai dépassé`);
      return { nbLivres: LIVRES.length, description: `${p.utilisateur}@${p.serveur}:${p.port}/${p.base}` };
    }
    case 'lire_connexions_mysql':
      return { store: connexionsSimulees.store, ...(connexionsSimulees.storeMysql ? { storeMysql: connexionsSimulees.storeMysql } : {}), ...(connexionsSimulees.comicrackMysql ? { comicrackMysql: connexionsSimulees.comicrackMysql } : {}) };
    case 'configurer_mysql': {
      const choix = args.choix as ChoixMysql;
      const p = choix.mysql;
      if (choix.mode === 'comicrack' || choix.store === 'mysql') {
        if (!p) throw new Error('paramètres MySQL manquants');
        if (p.serveur.trim() === 'injoignable') throw new Error(`connexion à ${p.utilisateur}@${p.serveur}:${p.port}/${p.base} impossible : délai dépassé`);
        const pub = { serveur: p.serveur.trim(), port: p.port, utilisateur: p.utilisateur.trim(), motDePasseDefini: !!p.motDePasse, base: p.base.trim() };
        connexionsSimulees = choix.mode === 'comicrack' ? { store: 'sqlite', comicrackMysql: pub } : { store: 'mysql', storeMysql: pub };
      } else {
        connexionsSimulees = { store: 'sqlite' };
      }
      etat = { mode: choix.mode, nbLivres: LIVRES.length, premierDemarrage: false, source: choix.mode === 'autonome' && choix.store !== 'mysql' ? 'sqlite' : 'mysql' };
      return etat;
    }
    case 'lister_livres': {
      const q = args.requete as RequeteLivres;
      simuleTests.requetes.push(q);
      const liste = ordonnerPile(q);
      // Étape 5 : rang du livre dans la liste affichée (colonne « # »).
      const rangs = q.liste && q.liste !== '@manquants' ? new Map(listes.livresDe(q.liste).map((id, i) => [id, i + 1])) : null;
      const rep: ReponseLivres = {
        total: liste.length,
        livres: liste.slice(q.debut, q.debut + q.nombre).map((l) => (rangs ? { ...resume(l, q.groupe), rang: rangs.get(l.id) } : resume(l, q.groupe))),
      };
      return rep;
    }
    case 'rangs_vue': {
      // Même vue que lister_livres (cmd_vue.rs) : rang de chaque identifiant, null s'il est absent.
      const q = args.requete as RequeteLivres;
      simuleTests.rangs.push((args.ids as string[]).length);
      const rangs = new Map(ordonnerPile(q).map((l, i) => [l.id, i]));
      return (args.ids as string[]).map((id) => rangs.get(id) ?? null);
    }
    case 'lister_piles': {
      const q = args.requete as RequeteLivres;
      simuleTests.requetes.push(q);
      const liste = ordonner({ ...q, serie: undefined });
      const piles = new Map<string, Pile>();
      for (const l of liste) {
        const t = titrePile(l);
        if (simuleTests.pilesMasquees.has(t)) continue;
        let p = piles.get(t);
        if (!p) piles.set(t, (p = { serie: t, nbLivres: 0, nbLus: 0, ids: [], urlCouverture: l.urlCouverture, ...(q.groupe && q.groupe !== 'aucun' && q.groupe !== 'serie' ? { groupe: groupeDe(l, q.groupe) } : {}) }));
        p.nbLivres++;
        if (l.lu) p.nbLus++;
        p.ids.push(l.id);
      }
      const rep: ReponsePiles = { totalLivres: liste.length, piles: [...piles.values()] };
      return rep;
    }
    case 'detail_livre': {
      const l = LIVRES[Number(args.id) - 1];
      if (!l) throw new Error('Livre introuvable.');
      // Comme `fusionner_lecture` : la page affichée est la plus loin vue, pas la page courante.
      const loin = Math.max(l.pageMax ?? 0, l.pageCourante);
      const d: LivreDetail = { ...l, tailleFichier: l.pages * 310_000, langue: 'fr', format: 'CBZ', manquant: mq.est(l.id), fichierPresent: l.fichierPresent && !mq.injoignable(l.id), progression: l.lu ? 100 : Math.round((loin / Math.max(1, l.pages - 1)) * 100), pageCourante: loin, pageLue: loin > 0 ? loin + 1 : undefined, sourceLecture: l.lu || loin > 0 ? (Number(l.id) % 5 === 0 ? 'comicrust' : 'comicrack') : undefined };
      if (l.lu) d.pageLue = l.pages;
      if (l.lu && l.enRelecture && l.pageCourante + 1 < l.pages) {
        const page = l.pageCourante + 1;
        d.relecture = { page, total: l.pages, pourcentage: Math.min(99, Math.max(1, Math.round((page * 100) / l.pages))) };
      }
      return d;
    }
    case 'ouvrir_livre': {
      const l = LIVRES[Number(args.id) - 1];
      if (!l) throw new Error('Livre introuvable.');
      if (!l.fichierPresent) throw new Error(`Fichier introuvable : ${l.fichier}`);
      const jeton = `s${++numeroSession}`;
      sessions.set(jeton, l);
      simuleTests.ouverts.push(String(args.id));
      relectures.set(jeton, { dejaLu: l.lu, finVue: false });
      // Comme `Lecteur::ouvrir_chemin` : reprise à la page mémorisée, même pour un livre lu (sa
      // dernière page s'il a été fini : cas de la recette C-05).
      const reprise = Math.min(l.pageCourante, l.pages - 1);
      const ouvert: LivreOuvert = {
        jeton, id: l.id, titre: l.serie ? `${l.serie} #${l.numero} — ${l.titre}` : l.titre,
        fichier: l.fichier, format: 'eComic (ZIP)', nbPages: l.pages,
        pageReprise: reprise, manga: l.id.endsWith('7'),
        urlBase: `simule://page/${jeton}`, pages: pagesSimulees(l),
      };
      // `?repriseDiff` : la bibliothèque a retenu une autre page (3e page plus loin).
      if (REPRISE_DIFFERENTE) ouvert.pageRepriseComicrack = Math.min(reprise + 3, l.pages - 1);
      return ouvert;
    }
    case 'ouvrir_fichier': {
      const chemin = String(args.chemin ?? '').trim().replace(/^"|"$/g, '');
      if (!chemin) throw new Error('chemin vide');
      simuleTests.ouverts.push(chemin);
      const l = LIVRES.find((x) => x.fichier.toLowerCase() === chemin.toLowerCase());
      if (l) return appelerSimule('ouvrir_livre', { id: l.id });
      if (!/\.(cb[zr7tw]|zip|rar|7z|pdf|djvu)$/i.test(chemin)) throw new Error(`fichier introuvable : ${chemin}`);
      const nom = chemin.split(/[\\/]/).pop() ?? chemin;
      const interne = { ...LIVRES[0], id: 'f' + numeroSession, titre: nom.replace(/\.[^.]+$/, ''), pages: 24, pageCourante: 0, lu: false };
      const jeton = `s${++numeroSession}`;
      sessions.set(jeton, interne);
      const ouvert: LivreOuvert = {
        jeton, titre: interne.titre, fichier: chemin, format: 'eComic (ZIP)', nbPages: 24, pageReprise: 0,
        manga: false, urlBase: `simule://page/${jeton}`, pages: pagesSimulees(interne),
      };
      return ouvert;
    }
    case 'enregistrer_avancement': {
      const l = sessions.get(args.jeton as string);
      if (!l) throw new Error('Session de lecture inconnue.');
      const a = { jeton: args.jeton as string, page: args.page as number, termine: args.termine as boolean };
      if (LATENCE_AVANCEMENT) await new Promise((r) => setTimeout(r, LATENCE_AVANCEMENT));
      simuleTests.avancements.push(a);
      l.pageCourante = a.page;
      l.pageMax = Math.max(l.pageMax ?? 0, a.page, a.termine ? l.pages - 1 : 0);
      if (a.termine) l.lu = true;
      // Relecture (comme `Lecteur::enregistrer_avancement`) : séance ouverte sur un livre lu, fin
      // pas encore revue.
      const r = relectures.get(a.jeton);
      if (r) {
        r.finVue ||= a.termine || a.page >= l.pages - 1;
        l.enRelecture = r.dejaLu && !r.finVue;
      }
      return null;
    }
    case 'fermer_livre':
      sessions.delete(args.jeton as string);
      relectures.delete(args.jeton as string);
      simuleTests.fermes.push(args.jeton as string);
      return null;
    case 'lister_dossiers':
      return dossiers.slice();
    case 'ajouter_dossier':
      if (!dossiers.includes(args.chemin as string)) dossiers.push(args.chemin as string);
      return dossiers.slice().sort();
    case 'retirer_dossier':
      dossiers = dossiers.filter((d) => d !== args.chemin);
      return dossiers.slice();
    case 'lancer_scan':
    case 'arreter_scan':
    case 'lancer_scan_auto':
    case 'lire_reglages_analyse':
    case 'enregistrer_reglages_analyse':
      return scanSimule.appeler(commande, args);
    case 'apercu_transfert':
    case 'lancer_transfert':
    case 'annuler_transfert':
      return transfert.appeler(commande, args);
    case 'etat_base':
    case 'reessayer_base':
      return base.appeler(commande);
    default: {
      simuleTests.edition.push({ commande, args: JSON.parse(JSON.stringify(args)) as Record<string, unknown> });
      const rl = listes.appeler(commande, args);
      if (rl !== undefined) return rl;
      const r = edition(commande, args);
      if (r === undefined) throw new Error(`commande inconnue (mode simulé) : ${commande}`);
      return r;
    }
  }
}
