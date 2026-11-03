// Couche unique d'appel des commandes Tauri. Le front n'a aucune logique métier : il affiche ce que
// renvoient les commandes (crates/comicrust-tauri/src/app.rs). Les structures sont en camelCase
// (serde `rename_all = "camelCase"`), sauf `InfosApplication` et `Theme` (étape 0, snake_case).
//
// Hors de la fenêtre Tauri (navigateur, `npm run dev`), tous les appels sont routés vers
// `simule.ts` (MODE SIMULÉ : 5 000 livres factices) afin de développer et tester sans le backend.
import type { EventCallback, UnlistenFn } from '@tauri-apps/api/event';

// ───────────────────────────── Types du contrat IPC ─────────────────────────────

export interface InfosApplication {
  version: string;
  dossier_data: string;
  dossier_themes: string;
  dossier_webview: string;
}

export interface Theme {
  id: string;
  nom: string;
  livre: boolean;
}

export type ModeBiblio = 'autonome' | 'comicrack';

export interface EtatBiblio {
  mode: ModeBiblio;
  nbLivres: number;
  premierDemarrage: boolean;
}

export interface ChoixPremierDemarrage {
  mode: ModeBiblio;
  /** Chemin du fichier ComicRack.ini (mode `comicrack` uniquement). */
  cheminIni?: string;
}

/** Clés de tri acceptées par `lister_livres` (docs/tauri/ipc-bibliotheque.md ; le backend trie). */
export type ClefTri =
  | 'serie' | 'titre' | 'parution' | 'ajout' | 'lecture' | 'note' | 'pages' | 'avancement' | 'taille' | 'fichier';
export type SensTri = 'asc' | 'desc';
export type Groupe = 'aucun' | 'serie' | 'editeur';

export interface RequeteLivres {
  recherche?: string;
  tri: ClefTri;
  sensTri: SensTri;
  groupe?: Groupe;
  debut: number;
  nombre: number;
}

export interface LivreResume {
  id: string;
  titre: string;
  serie?: string;
  numero?: string;
  editeur?: string;
  annee?: number;
  pages: number;
  lu: boolean;
  /** À utiliser telle quelle : `cr-couv://<id>`, ou `http://cr-couv.localhost/<id>` sous Windows. */
  urlCouverture: string;
}

export interface ReponseLivres {
  total: number;
  livres: LivreResume[];
}

/** Champs absents = non renseignés. Seuls les champs courants sont déclarés ici. */
export interface LivreDetail extends LivreResume {
  volume?: number;
  mois?: number;
  jour?: number;
  collection?: string;
  progression?: number;
  pageCourante?: number;
  scenariste?: string;
  dessinateur?: string;
  encreur?: string;
  coloriste?: string;
  lettreur?: string;
  genre?: string;
  arcNarratif?: string;
  resume?: string;
  format?: string;
  langue?: string;
  note?: number;
  isbn?: string;
  tailleFichier?: number;
  dateAjout?: string;
  /** Chemin complet du fichier. */
  fichier?: string;
  fichierPresent: boolean;
  manquant: boolean;
}

/** Préférences d'affichage mémorisées dans data\comicrust-tauri.json (section `vue`).
 *  Champ absent = valeur par défaut du front. */
export interface PreferencesVue {
  tri?: ClefTri;
  sensTri?: SensTri;
  groupe?: Groupe;
  vue?: 'grille' | 'tableau';
  /** Taille des couvertures (100 à 260 px). */
  zoom?: number;
  details?: boolean;
  /** Largeur du panneau de détails (220 à 800 px). */
  largeurDetails?: number;
  /** Identifiant du thème (fichier CSS de data\themes). */
  theme?: string;
  /** Réglages du lecteur (sous-objet remplacé en entier à chaque enregistrement). */
  lecteur?: PreferencesLecteur;
}

// ───────────────────────────── Lecteur (étape 2) ─────────────────────────────

export type ModeLecture = 'simple' | 'double' | 'defilement';
export type Ajustement = 'meilleur' | 'largeur' | 'hauteur' | 'reelle';

/** Réglages du lecteur, section `vue.lecteur` de data\comicrust-tauri.json. Champ absent = défaut.
 *  Le backend les garde tels quels (et ignore les champs inconnus ou invalides). */
export interface PreferencesLecteur {
  /** Défaut `simple`. */
  mode?: ModeLecture;
  /** Défaut `meilleur` (page entière). */
  ajustement?: Ajustement;
  /** Double page : la couverture (1re page) est affichée seule. Défaut `true`. */
  couvertureSeule?: boolean;
  /** Sens de lecture droite → gauche pour les livres non marqués manga. Défaut `false`
   *  (un livre marqué manga dans ses infos s'ouvre toujours en sens manga). */
  manga?: boolean;
  /** Bandeau de miniatures affiché. Défaut `false`. */
  miniatures?: boolean;
  /** Grossissement de la loupe (1,5 à 6). Défaut 2,5. */
  grossissementLoupe?: number;
}

/** Type de page ComicRack (`FrontCover`, `Story`, `Advertisement`…), informatif. */
export interface PageInfo {
  /** Index de la page dans le fichier (0 = première). */
  index: number;
  /** Dimensions en pixels de l'image d'origine, si connues (sinon déduites au chargement). */
  largeur?: number;
  hauteur?: number;
  /** Page double (scan de deux pages côte à côte) : toujours affichée seule en double page. */
  double: boolean;
  type?: string;
  /** Page supprimée dans ComicRack : masquée par le lecteur. */
  supprimee: boolean;
  /** Rotation à appliquer à l'affichage, en degrés (0, 90, 180, 270). */
  rotation: number;
}

export interface LivreOuvert {
  /** Identifiant de la session de lecture, à rendre à fermer_livre / enregistrer_avancement. */
  jeton: string;
  id: string;
  titre: string;
  nbPages: number;
  /** Index (dans le fichier) de la page à afficher à l'ouverture. */
  pageReprise: number;
  /** Livre marqué manga (lecture droite → gauche). */
  manga: boolean;
  /** Base des URL des pages, utilisée telle quelle : `${urlBase}/${index}?l=${largeurMax}`. */
  urlBase: string;
  pages: PageInfo[];
}

export interface ResultatScan {
  ajoutes: number;
  misAJour: number;
  manquants: number;
}

export interface ErreurScan {
  message: string;
}

export interface ProgressionScan {
  fait: number;
  total: number;
}

// ───────────────────────────── Aiguillage Tauri / simulé ─────────────────────────────

/** `true` dans la fenêtre Tauri ; `false` dans un navigateur (mode simulé). */
export const dansTauri = (): boolean => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

async function appeler<T>(commande: string, args?: Record<string, unknown>): Promise<T> {
  if (dansTauri()) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(commande, args);
  }
  const { appelerSimule } = await import('./simule');
  return (await appelerSimule(commande, args ?? {})) as T;
}

/** Abonnement à un événement du backend (ou de la simulation). Renvoie la fonction de désabonnement. */
export async function ecouter<T>(nom: string, rappel: (charge: T) => void): Promise<UnlistenFn> {
  if (dansTauri()) {
    const { listen } = await import('@tauri-apps/api/event');
    const cb: EventCallback<T> = (e) => rappel(e.payload);
    return listen<T>(nom, cb);
  }
  const { ecouterSimule } = await import('./simule');
  return ecouterSimule(nom, rappel as (c: unknown) => void);
}

// ───────────────────────────── Commandes ─────────────────────────────

export const infos = () => appeler<InfosApplication>('infos');
export const listerThemes = () => appeler<Theme[]>('lister_themes');
export const chargerTheme = (id: string) => appeler<string>('charger_theme', { id });

export const lirePreferencesVue = () => appeler<PreferencesVue>('lire_preferences_vue');
/** Fusionne les champs fournis ; renvoie l'ensemble à jour. */
export const enregistrerPreferencesVue = (preferences: PreferencesVue) =>
  appeler<PreferencesVue>('enregistrer_preferences_vue', { preferences });

export const ouvrirBibliotheque = () => appeler<EtatBiblio>('ouvrir_bibliotheque');
export const configurerPremierDemarrage = (choix: ChoixPremierDemarrage) =>
  appeler<EtatBiblio>('configurer_premier_demarrage', { choix });

export const listerLivres = (requete: RequeteLivres) => appeler<ReponseLivres>('lister_livres', { requete });
export const detailLivre = (id: string) => appeler<LivreDetail>('detail_livre', { id });

export const listerDossiers = () => appeler<string[]>('lister_dossiers');
/** Renvoie la liste à jour des dossiers surveillés. */
export const ajouterDossier = (chemin: string) => appeler<string[]>('ajouter_dossier', { chemin });
export const retirerDossier = (chemin: string) => appeler<string[]>('retirer_dossier', { chemin });
export const lancerScan = () => appeler<void>('lancer_scan');

export const ouvrirLivre = (id: string) => appeler<LivreOuvert>('ouvrir_livre', { id });
export const fermerLivre = (jeton: string) => appeler<void>('fermer_livre', { jeton });
/** `page` : index dans le fichier de la page affichée ; `termine` : la dernière page a été vue. */
export const enregistrerAvancement = (jeton: string, page: number, termine: boolean) =>
  appeler<void>('enregistrer_avancement', { jeton, page, termine });

/** URL d'une page. `largeurMax` : largeur souhaitée en pixels physiques (0 = taille d'origine ;
 *  le backend ne grossit jamais l'image). En mode simulé, image SVG générée. */
export function urlPage(livre: LivreOuvert, index: number, largeurMax: number): string {
  if (!dansTauri() && livre.urlBase.startsWith('simule:')) return imageSimulee(livre, index);
  return `${livre.urlBase}/${index}?l=${largeurMax}`;
}

const imagesSimulees = new Map<string, string>();
/** Page factice du mode simulé (SVG aux dimensions de la page, numéro bien visible). */
function imageSimulee(livre: LivreOuvert, index: number): string {
  const clef = livre.id + '/' + index;
  let u = imagesSimulees.get(clef);
  if (u) return u;
  const p = livre.pages.find((x) => x.index === index);
  const double = p?.double ?? false;
  const l = p?.largeur ?? (double ? 2000 : 1000);
  const h = p?.hauteur ?? 1500;
  let teinte = 0;
  for (const c of livre.titre) teinte = (teinte * 31 + c.charCodeAt(0)) % 360;
  teinte = (teinte + index * 23) % 360;
  const cases = [];
  const marge = 40;
  const lignes = 3;
  const hc = (h - marge * (lignes + 1)) / lignes;
  for (let r = 0; r < lignes; r++) {
    const y = marge + r * (hc + marge);
    const nb = 1 + ((index + r) % 2);
    const lc = (l - marge * (nb + 1)) / nb;
    for (let k = 0; k < nb; k++) {
      cases.push(`<rect x="${marge + k * (lc + marge)}" y="${y}" width="${lc}" height="${hc}" fill="hsl(${(teinte + 40 * (r + k)) % 360} 45% 72%)" stroke="#111" stroke-width="6"/>`);
    }
  }
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="${l}" height="${h}" viewBox="0 0 ${l} ${h}">` +
    `<rect width="${l}" height="${h}" fill="#fbf8f0"/>${cases.join('')}` +
    `<circle cx="${l / 2}" cy="${h / 2}" r="${Math.min(l, h) / 6}" fill="hsl(${teinte} 70% 45%)" opacity=".85"/>` +
    `<text x="${l / 2}" y="${h / 2 + 50}" text-anchor="middle" font-family="sans-serif" font-size="150" font-weight="700" fill="#fff">${index + 1}</text>` +
    `<text x="${l - 30}" y="${h - 14}" text-anchor="end" font-family="sans-serif" font-size="28" fill="#333">${double ? 'double page · ' : ''}page ${index + 1}</text>` +
    `<text x="30" y="${h - 14}" font-family="sans-serif" font-size="28" fill="#333">◀ début</text></svg>`;
  u = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
  imagesSimulees.set(clef, u);
  return u;
}

/** Plein écran de la fenêtre : API de fenêtre Tauri (permission `core:window:allow-set-fullscreen`),
 *  sinon plein écran du document (navigateur, ou permission absente). */
export async function pleinEcran(actif: boolean): Promise<void> {
  if (dansTauri()) {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().setFullscreen(actif);
      return;
    } catch (e) {
      console.warn('plein écran Tauri indisponible, repli sur le document', e);
    }
  }
  if (actif && !document.fullscreenElement) await document.documentElement.requestFullscreen?.();
  else if (!actif && document.fullscreenElement) await document.exitFullscreen?.();
}

/** Message lisible pour une erreur d'IPC : le backend rejette avec un texte français (qui peut
 *  contenir un chemin) ; on tolère aussi Error, objet `{message}` ou valeur absente. */
export function messageErreur(e: unknown): string {
  if (typeof e === 'string' && e.trim()) return e;
  if (e instanceof Error && e.message) return e.message;
  if (e && typeof e === 'object' && 'message' in e && typeof (e as { message: unknown }).message === 'string') {
    return (e as { message: string }).message || 'Erreur inconnue.';
  }
  return 'Erreur inconnue.';
}
