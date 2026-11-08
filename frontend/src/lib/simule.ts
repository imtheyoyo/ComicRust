// MODE SIMULÉ : faux backend utilisé uniquement hors de Tauri (navigateur), pour développer et
// tester le front avec 5 000 livres factices. Il imite le contrat IPC de api.ts, y compris la
// latence et les événements de scan. Rien de ceci n'est utilisé dans la fenêtre Tauri.
import type {
  PreferencesVue, ClefTri, EtatBiblio, LivreDetail, LivreOuvert, LivreResume, PageInfo, RequeteLivres,
  ReponseLivres, Theme,
} from './api';
import { creerEditionSimulee } from './simuleEdition';
import { creerListesSimulees } from './simuleListes';

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
  const membres = q.liste ? listes.livresDe(q.liste) : null;
  const k = JSON.stringify([q.recherche ?? '', q.tri, q.sensTri, q.groupe ?? 'aucun']);
  if (!membres && k === ordreCle) return ordre;
  const position = new Map((membres ?? []).map((id, i) => [id, i]));
  const base = membres ? membres.map((id) => LIVRES[Number(id) - 1]).filter(Boolean) : LIVRES;
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
        const g = l[groupe] ?? '';
        rangGroupe.set(g, Math.min(rangGroupe.get(g) ?? Infinity, pos(l)));
      }
    }
    liste.sort((a, b) => (groupe ? (rangGroupe.get(a[groupe] ?? '')! - rangGroupe.get(b[groupe] ?? '')!) * sens : 0) || sens * (pos(a) - pos(b)));
    return liste;
  }
  liste.sort((a, b) => {
    if (groupe) {
      const g = cmp(a[groupe] ?? '', b[groupe] ?? '');
      if (g) return g;
    }
    return sens * cmp(cle(a, q.tri), cle(b, q.tri)) || Number(a.id) - Number(b.id);
  });
  if (membres) return liste;
  ordreCle = k;
  ordre = liste;
  return liste;
}

const resume = (l: LivreInterne): LivreResume => ({
  id: l.id, titre: l.titre, serie: l.serie, numero: l.numero, editeur: l.editeur,
  annee: l.annee, pages: l.pages, lu: l.lu, urlCouverture: l.urlCouverture,
});

// ───────────── Événements simulés ─────────────

const bus = new EventTarget();
const emettre = (nom: string, charge: unknown) => bus.dispatchEvent(new CustomEvent(nom, { detail: charge }));

export function ecouterSimule(nom: string, rappel: (c: unknown) => void): () => void {
  const f = (e: Event) => rappel((e as CustomEvent).detail);
  bus.addEventListener(nom, f);
  return () => bus.removeEventListener(nom, f);
}

let scanEnCours = false;
async function simulerScan() {
  if (scanEnCours) return;
  scanEnCours = true;
  const total = 1240;
  for (let fait = 0; fait <= total; fait += 31) {
    emettre('scan-progression', { fait: Math.min(fait, total), total });
    await pause(60);
  }
  emettre('scan-progression', { fait: total, total });
  scanEnCours = false;
  emettre('scan-termine', { ajoutes: 12, misAJour: 34, manquants: 3 });
}

// ───────────── État simulé ─────────────

// `?mode=comicrack` : bibliothèque liée à ComicRack (lecture seule tant que « Lecture et écriture »
// n'est pas cochée dans Réglages).
let etat: EtatBiblio = {
  mode: new URLSearchParams(location.search).get('mode') === 'comicrack' ? 'comicrack' : 'autonome',
  nbLivres: LIVRES.length,
  premierDemarrage: new URLSearchParams(location.search).has('premier'),
};
// Préférences d'affichage : repli en mémoire (perdues au rechargement). `?prefs=<json>` simule un
// redémarrage avec des préférences déjà enregistrées ; `window.__comicrustSimule` les expose aux tests.
interface Avancement { jeton: string; page: number; termine: boolean }
const simuleTests: {
  prefs: PreferencesVue; requetes: RequeteLivres[]; avancements: Avancement[]; ouverts: string[]; fermes: string[];
  /** Commandes d'édition reçues (étape 3), pour vérifier ce que le front envoie. */
  edition: Array<{ commande: string; args: Record<string, unknown> }>;
} = { prefs: {}, requetes: [], avancements: [], ouverts: [], fermes: [], edition: [] };
try {
  const brut = new URLSearchParams(location.search).get('prefs');
  if (brut) simuleTests.prefs = JSON.parse(brut) as PreferencesVue;
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

const THEMES = import.meta.glob('../../../crates/comicrust-tauri/themes/*.css', {
  query: '?raw', import: 'default',
}) as Record<string, () => Promise<string>>;
const idTheme = (chemin: string) => chemin.split('/').pop()!.replace(/\.css$/, '');

// Édition (étape 3) : voir simuleEdition.ts.
const edition = creerEditionSimulee({
  livres: LIVRES,
  mode: () => etat.mode,
  emettre,
  invalider: () => { ordreCle = ''; },
  pause,
});

// Listes ComicRack (étape 5) : voir simuleListes.ts. `?comicrackOuvert` : ComicRack lancé (mode lié).
const listes = creerListesSimulees({
  livres: LIVRES,
  lectureSeule: () => {
    if (etat.mode !== 'comicrack') return undefined;
    const r = edition('lire_reglages_edition', {}) as { ecritureComicrack: boolean };
    return r.ecritureComicrack ? undefined : 'lecture seule';
  },
  comicrackLance: () => etat.mode === 'comicrack' && new URLSearchParams(location.search).has('comicrackOuvert'),
  vue: (q) => ordonner(q),
  titreGroupe: (l, g) => (g === 'editeur' ? l.editeur ?? '(sans éditeur)' : l.serie ?? '(sans série)'),
});

export async function appelerSimule(commande: string, args: Record<string, unknown>): Promise<unknown> {
  await pause(LATENCE_MS);
  switch (commande) {
    case 'infos':
      return {
        version: '0.0.0-simule', dossier_data: '(mode simulé) data', dossier_themes: '(mode simulé) data/themes',
        dossier_webview: '(mode simulé) data/webview2',
      };
    case 'lister_themes': {
      const themes: Theme[] = [];
      for (const [chemin, charger] of Object.entries(THEMES)) {
        const css = await charger();
        themes.push({ id: idTheme(chemin), nom: /@nom:\s*(.+?)\s*\*\//.exec(css)?.[1] ?? idTheme(chemin), livre: true });
      }
      return themes;
    }
    case 'charger_theme': {
      const entree = Object.entries(THEMES).find(([c]) => idTheme(c) === args.id);
      if (!entree) throw new Error(`thème introuvable : ${String(args.id)}`);
      return entree[1]();
    }
    case 'lire_preferences_vue':
      return { ...simuleTests.prefs };
    case 'enregistrer_preferences_vue':
      simuleTests.prefs = { ...simuleTests.prefs, ...(args.preferences as PreferencesVue) };
      return { ...simuleTests.prefs };
    case 'ouvrir_bibliotheque':
      return etat;
    case 'configurer_premier_demarrage': {
      const choix = args.choix as { mode: EtatBiblio['mode'] };
      etat = { mode: choix.mode, nbLivres: LIVRES.length, premierDemarrage: false };
      return etat;
    }
    case 'lister_livres': {
      const q = args.requete as RequeteLivres;
      simuleTests.requetes.push(q);
      const liste = ordonner(q);
      // Étape 5 : rang du livre dans la liste affichée (colonne « # »).
      const rangs = q.liste ? new Map(listes.livresDe(q.liste).map((id, i) => [id, i + 1])) : null;
      const rep: ReponseLivres = {
        total: liste.length,
        livres: liste.slice(q.debut, q.debut + q.nombre).map((l) => (rangs ? { ...resume(l), rang: rangs.get(l.id) } : resume(l))),
      };
      return rep;
    }
    case 'detail_livre': {
      const l = LIVRES[Number(args.id) - 1];
      if (!l) throw new Error('Livre introuvable.');
      const d: LivreDetail = { ...l, tailleFichier: l.pages * 310_000, langue: 'fr', format: 'CBZ', manquant: !l.fichierPresent, progression: l.lu ? 100 : Math.round((l.pageCourante / Math.max(1, l.pages - 1)) * 100), pageCourante: l.pageCourante };
      return d;
    }
    case 'ouvrir_livre': {
      const l = LIVRES[Number(args.id) - 1];
      if (!l) throw new Error('Livre introuvable.');
      if (!l.fichierPresent) throw new Error(`Fichier introuvable : ${l.fichier}`);
      const jeton = `s${++numeroSession}`;
      sessions.set(jeton, l);
      simuleTests.ouverts.push(String(args.id));
      const reprise = l.lu ? 0 : Math.min(l.pageCourante, l.pages - 1);
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
      simuleTests.avancements.push(a);
      l.pageCourante = a.page;
      if (a.termine) l.lu = true;
      return null;
    }
    case 'fermer_livre':
      sessions.delete(args.jeton as string);
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
      void simulerScan();
      return null;
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
