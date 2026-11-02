// MODE SIMULÉ : faux backend utilisé uniquement hors de Tauri (navigateur), pour développer et
// tester le front avec 5 000 livres factices. Il imite le contrat IPC de api.ts, y compris la
// latence et les événements de scan. Rien de ceci n'est utilisé dans la fenêtre Tauri.
import type {
  PreferencesVue, ClefTri, EtatBiblio, LivreDetail, LivreResume, RequeteLivres, ReponseLivres, Theme,
} from './api';

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
      fichier: `D:\\BD\\${serie.replace(/[^\w ]/g, '')}\\${serie.replace(/[^\w ]/g, '')} ${numero.padStart(3, '0')}.cbz`,
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
  }
};
const cmp = (a: string | number, b: string | number) =>
  typeof a === 'number' && typeof b === 'number' ? a - b : String(a).localeCompare(String(b), 'fr');

let ordreCle = '';
let ordre: LivreInterne[] = LIVRES;

function ordonner(q: RequeteLivres): LivreInterne[] {
  const k = JSON.stringify([q.recherche ?? '', q.tri, q.sensTri, q.groupe ?? 'aucun']);
  if (k === ordreCle) return ordre;
  const mot = (q.recherche ?? '').trim().toLowerCase();
  const liste = mot
    ? LIVRES.filter((l) => {
        const texte = `${l.titre} ${l.serie ?? ''} ${l.editeur ?? ''} ${l.fichier}`.toLowerCase();
        return mot.split(/\s+/).every((m) => texte.includes(m)); // tous les mots requis, comme le backend
      })
    : LIVRES.slice();
  const sens = q.sensTri === 'desc' ? -1 : 1;
  const groupe = q.groupe && q.groupe !== 'aucun' ? q.groupe : null;
  liste.sort((a, b) => {
    if (groupe) {
      const g = cmp(a[groupe] ?? '', b[groupe] ?? '');
      if (g) return g;
    }
    return sens * cmp(cle(a, q.tri), cle(b, q.tri)) || Number(a.id) - Number(b.id);
  });
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

let etat: EtatBiblio = {
  mode: 'autonome',
  nbLivres: LIVRES.length,
  premierDemarrage: new URLSearchParams(location.search).has('premier'),
};
// Préférences d'affichage : repli en mémoire (perdues au rechargement). `?prefs=<json>` simule un
// redémarrage avec des préférences déjà enregistrées ; `window.__comicrustSimule` les expose aux tests.
const simuleTests: { prefs: PreferencesVue; requetes: RequeteLivres[] } = { prefs: {}, requetes: [] };
try {
  const brut = new URLSearchParams(location.search).get('prefs');
  if (brut) simuleTests.prefs = JSON.parse(brut) as PreferencesVue;
} catch {
  /* paramètre illisible : préférences vides */
}
(globalThis as { __comicrustSimule?: typeof simuleTests }).__comicrustSimule = simuleTests;

let dossiers = ['D:\\BD\\Marvel', 'D:\\BD\\Franco-belge', '\\\\NAS\\comics\\Image'];

const THEMES = import.meta.glob('../../../crates/comicrust-tauri/themes/*.css', {
  query: '?raw', import: 'default',
}) as Record<string, () => Promise<string>>;
const idTheme = (chemin: string) => chemin.split('/').pop()!.replace(/\.css$/, '');

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
      const rep: ReponseLivres = { total: liste.length, livres: liste.slice(q.debut, q.debut + q.nombre).map(resume) };
      return rep;
    }
    case 'detail_livre': {
      const l = LIVRES[Number(args.id) - 1];
      if (!l) throw new Error('Livre introuvable.');
      const d: LivreDetail = { ...l, tailleFichier: l.pages * 310_000, langue: 'fr', format: 'CBZ', manquant: !l.fichierPresent, progression: l.lu ? 100 : 0 };
      return d;
    }
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
    default:
      throw new Error(`commande inconnue (mode simulé) : ${commande}`);
  }
}
