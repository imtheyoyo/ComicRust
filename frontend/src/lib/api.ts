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
  /** Quand la bibliothèque et ce poste n'ont pas retenu la même page (`pageRepriseComicrack`) :
   *  demander (défaut), prendre celle de la bibliothèque, ou celle de ComicRust (réglage
   *  « Reprise » de la version egui). */
  reprise?: ChoixReprise;
}

export type ChoixReprise = 'demander' | 'comicrack' | 'comicrust';

/** Type de page ComicRack (`FrontCover`, `Story`, `Advertisement`…), informatif. */
export interface PageInfo {
  /** Numéro de page du livre (rang dans la liste complète, pages supprimées comprises, 0 = première) :
   *  index des URL de pages, de `pageReprise` et de `enregistrer_avancement`. `pages[i].index === i`. */
  index: number;
  /** Image du fichier (rang dans l'ordre naturel des noms) ; diffère de `index` si réordonné. */
  image: number;
  /** Dimensions en pixels de l'image d'origine (avant rotation), si connues (sinon déduites au chargement). */
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
  /** GUID du livre ; absent pour un fichier hors bibliothèque (ouvrir_fichier). */
  id?: string;
  titre: string;
  /** Chemin complet du fichier ouvert. */
  fichier: string;
  /** Format (« eComic (ZIP) », « Document PDF », « Dossier d'images »…). */
  format: string;
  /** Nombre de pages du livre (pages supprimées comprises) = pages.length. */
  nbPages: number;
  /** Numéro de la page à afficher à l'ouverture (jamais une page supprimée) : celle de ComicRust. */
  pageReprise: number;
  /** Présent si la page retenue par la bibliothèque (ComicRack, ou base ComicRust) diffère de
   *  `pageReprise` : le lecteur propose le choix (réglage `reprise`). */
  pageRepriseComicrack?: number;
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

// ───────────────────────────── Infos et édition (étape 3, docs/tauri/ipc-edition.md) ─────────────────────────────

/** Section `edition` de data\comicrust-tauri.json. */
export interface ReglagesEdition {
  /** Option « Lecture et écriture » du mode lié ComicRack (défaut false = lecture seule). */
  ecritureComicrack: boolean;
  /** Case « Écrire aussi les infos dans le fichier ». */
  ecrireDansFichier: boolean;
  /** Conversion : ranger l'original dans « _originaux ». */
  rangerOriginal: boolean;
  modeleNom?: string;
  dossierCible?: string;
}

export type TypeChamp =
  | 'texte' | 'texteLong' | 'liste' | 'entier' | 'note' | 'prix' | 'ouiNon' | 'manga' | 'booleen' | 'date' | 'langue';

export interface Choix { valeur: string; libelle: string }

export interface ChampFiche {
  /** Nom ComicRack (`Series`, `Writer`…) ou `CustomValue:<clé>`. */
  nom: string;
  libelle: string;
  type: TypeChamp;
  /** Texte à afficher et modifier ('' si vide ou si multiple). */
  valeur: string;
  /** Plusieurs livres aux valeurs différentes. */
  multiple: boolean;
  choix?: Choix[];
  min?: number;
  max?: number;
  /** Le champ propose les valeurs de la bibliothèque (proposer_valeurs). */
  suggestions: boolean;
  dansComicInfo: boolean;
  /** Texte d'aide du champ vide. */
  indication?: string;
}

export interface Onglet { nom: string; champs: string[] }

export interface LivreEdite {
  id: string;
  titre: string;
  fichier?: string;
  urlCouverture: string;
  pages: number;
  tailleFichier?: number;
  progression?: number;
  /** Infos pas encore écrites dans le fichier (ComicInfoIsDirty). */
  infosAEcrire: boolean;
  /** Le fichier reçoit les infos (CBZ/ZIP). */
  cbz: boolean;
}

export interface FicheEdition {
  ids: string[];
  multiple: boolean;
  livres: LivreEdite[];
  onglets: Onglet[];
  champs: ChampFiche[];
  /** Présent : consultation seulement, avec la marche à suivre. */
  lectureSeule?: string;
  ecrireDansFichierPossible: boolean;
  ecrireDansFichier: boolean;
  seuilConfirmation: number;
}

/** `valeur` '' : vider le champ. */
export interface ChampModifie { nom: string; valeur: string }
export interface ErreurChamp { nom: string; message: string }
/** `texte` : contenu du champ si on choisit la proposition. */
export interface Proposition { valeur: string; texte: string }

export interface DemandeModification {
  ids: string[];
  /** SEULEMENT les champs modifiés par l'utilisateur. */
  champs: ChampModifie[];
  /** Liste COMPLÈTE des pages (un seul livre), seulement si l'onglet Pages a été modifié. */
  pages?: PageModifiee[];
  ecrireDansFichier: boolean;
  confirme?: boolean;
}

export type Operation = 'modification' | 'ecritureInfos' | 'conversion' | 'renommage';
export type Statut = 'ok' | 'inchange' | 'ignore' | 'erreur';

export interface ResultatLivre {
  id: string;
  titre: string;
  statut: Statut;
  message?: string;
  fichier?: string;
}

export interface BilanOperation {
  operation: Operation;
  /** Présent : rien n'a été fait, rappeler avec `confirme: true` après accord. */
  confirmationRequise?: { nbLivres: number; seuil: number; message: string };
  resume: string;
  complet: boolean;
  interrompu: boolean;
  livres: ResultatLivre[];
  /** Bilan à copier (une ligne par livre). */
  rapport: string;
}

export interface ProgressionEdition { operation: Operation; fait: number; total: number; livre: string }
export interface BibliothequeModifiee { ids: string[] }

export interface ModeleNom { nom: string; modele: string }

export interface DemandeRenommage {
  ids: string[];
  modele: string;
  /** Absent ou vide : chaque fichier reste dans son dossier. */
  dossierCible?: string;
  apercu: boolean;
  confirme?: boolean;
}

export interface LigneRenommage {
  id: string;
  titre: string;
  de: string;
  vers: string;
  deNom: string;
  versNom: string;
  /** Présent : cette ligne ne sera pas appliquée. */
  probleme?: string;
  inchange: boolean;
}

export interface ResultatRenommage {
  lignes: LigneRenommage[];
  aRenommer: number;
  bilan?: BilanOperation;
}

// ───── Onglet Pages (étape 4, docs/tauri/ipc-pages.md) ─────

export type TypePage = 'FrontCover' | 'InnerCover' | 'Roundup' | 'Story' | 'Advertisement' | 'Editorial'
  | 'Letters' | 'Preview' | 'BackCover' | 'Other' | 'Deleted';

export interface ChoixPage { valeur: string; libelle: string; raccourci?: string }

/** Une page de la liste complète (ordre de lecture). */
export interface PageLivre {
  /** Rang dans l'ordre de lecture (recalculé par le backend) : `pages[i].index === i`. */
  index: number;
  /** Rang de l'image dans le fichier : identifiant STABLE de la page (sélection, opérations). */
  image: number;
  /** Nom de l'image dans l'archive. */
  nom: string;
  /** Valeur ComicRack (une TypePage, rarement des drapeaux combinés à renvoyer tels quels). */
  type: string;
  libelleType: string;
  supprimee: boolean;
  /** Page double (lecture seule, calculée comme ComicRack). */
  double: boolean;
  /** 0 | 90 | 180 | 270, à appliquer à l'affichage (la vignette n'est pas tournée). */
  rotation: number;
  /** 'Default' | 'Near' | 'Far'. */
  position: string;
  favori?: string | null;
  largeur?: number;
  hauteur?: number;
  urlVignette: string;
}

export interface PagesLivre {
  id: string;
  /** Séance de vignettes (protocole cr-page), à fermer par fermer_pages. */
  jeton: string;
  urlBase: string;
  largeurVignette: number;
  nbImages: number;
  /** Liste complète dans l'ordre de lecture ENREGISTRÉ, supprimées comprises. */
  pages: PageLivre[];
  modifiable: boolean;
  lectureSeule?: string;
  ecrireDansFichierPossible: boolean;
  types: ChoixPage[];
  /** valeur '0' | '90' | '180' | '270'. */
  rotations: ChoixPage[];
  positions: ChoixPage[];
}

/** Opération sur la liste, calculée par le backend (règles de ComicRack). */
export type OperationPages =
  | { op: 'deplacer'; images: number[]; position: number }
  | { op: 'mettreAuDebut'; images: number[] }
  | { op: 'mettreALaFin'; images: number[] }
  | { op: 'type'; images: number[]; valeur: string }
  | { op: 'basculerSupprimee'; images: number[] }
  | { op: 'rotation'; images: number[]; valeur: number }
  | { op: 'position'; images: number[]; valeur: string }
  | { op: 'favori'; image: number; texte?: string }
  | { op: 'ordreOrigine' }
  | { op: 'trierParNom'; numerique: boolean }
  | { op: 'reinitialiserTypes' }
  | { op: 'toutReinitialiser' };

/** Page envoyée à appliquer_modifications (un PageLivre convient tel quel). */
export interface PageModifiee {
  image: number;
  type?: string;
  supprimee?: boolean;
  rotation?: number;
  position?: string;
  favori?: string | null;
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
/** Ouvre un fichier (ou un dossier d'images, ou une image : son dossier) hors bibliothèque. */
export const ouvrirFichier = (chemin: string) => appeler<LivreOuvert>('ouvrir_fichier', { chemin });

/** Livre à ouvrir dans le lecteur : un livre de la bibliothèque ou un fichier. */
export type SourceLecteur = { id: string } | { chemin: string };
export const fermerLivre = (jeton: string) => appeler<void>('fermer_livre', { jeton });
/** `page` : index dans le fichier de la page affichée ; `termine` : la dernière page a été vue. */
export const enregistrerAvancement = (jeton: string, page: number, termine: boolean) =>
  appeler<void>('enregistrer_avancement', { jeton, page, termine });

// Infos et édition (étape 3).
export const lireReglagesEdition = () => appeler<ReglagesEdition>('lire_reglages_edition');
export const enregistrerReglagesEdition = (reglages: Partial<ReglagesEdition>) =>
  appeler<ReglagesEdition>('enregistrer_reglages_edition', { reglages });
export const lireFiches = (ids: string[]) => appeler<FicheEdition>('lire_fiches', { ids });
export const proposerValeurs = (champ: string, texte: string, tout = false, max?: number) =>
  appeler<Proposition[]>('proposer_valeurs', { champ, texte, tout, max });
export const verifierChamps = (champs: ChampModifie[]) => appeler<ErreurChamp[]>('verifier_champs', { champs });
export const appliquerModifications = (demande: DemandeModification) =>
  appeler<BilanOperation>('appliquer_modifications', { demande });
export const ajouterValeurPerso = (cle: string, existantes: string[]) =>
  appeler<ChampFiche>('ajouter_valeur_perso', { cle, existantes });
export const ecrireInfosFichiers = (ids: string[], confirme?: boolean) =>
  appeler<BilanOperation>('ecrire_infos_fichiers', { ids, confirme });
export const convertirCbrCbz = (ids: string[], rangerOriginal?: boolean, confirme?: boolean) =>
  appeler<BilanOperation>('convertir_cbr_cbz', { ids, rangerOriginal, confirme });
export const modelesDeNom = () => appeler<ModeleNom[]>('modeles_de_nom');
export const renommerDeplacer = (demande: DemandeRenommage) =>
  appeler<ResultatRenommage>('renommer_deplacer', { demande });
export const annulerOperation = () => appeler<void>('annuler_operation');
export const lirePages = (id: string, remplace?: string) => appeler<PagesLivre>('lire_pages', { id, remplace });
export const fermerPages = (jeton: string) => appeler<void>('fermer_pages', { jeton });
export const transformerPages = (pages: PageLivre[], operation: OperationPages) =>
  appeler<PageLivre[]>('transformer_pages', { pages, operation });

/** URL d'une vignette, ou de l'image à la largeur `largeur` (0 = taille d'origine) pour l'aperçu
 *  agrandi et la copie, à partir de `urlVignette`. En mode simulé, image SVG générée. */
export function urlImagePage(livre: PagesLivre, p: PageLivre, largeur?: number): string {
  if (!dansTauri() && livre.urlBase.startsWith('simule:')) {
    const faux = { jeton: livre.jeton, titre: livre.id, pages: [{ ...p, index: p.image, rotation: 0 }] } as unknown as LivreOuvert;
    return imageSimulee(faux, p.image);
  }
  if (largeur === undefined) return p.urlVignette;
  const base = p.urlVignette.replace(/\?.*$/, '');
  return largeur > 0 ? `${base}?l=${largeur}` : base;
}

/** Montre le fichier du livre dans son dossier (Explorateur de Windows, fichier sélectionné). */
export const ouvrirDossierFichier = (id: string) => appeler<void>('ouvrir_dossier_fichier', { id });

/** URL d'une page. `largeurMax` : largeur souhaitée en pixels physiques (0 = taille d'origine ;
 *  le backend ne grossit jamais l'image). En mode simulé, image SVG générée. */
export function urlPage(livre: LivreOuvert, index: number, largeurMax: number): string {
  if (!dansTauri() && livre.urlBase.startsWith('simule:')) return imageSimulee(livre, index);
  return `${livre.urlBase}/${index}?l=${largeurMax}`;
}

const imagesSimulees = new Map<string, string>();
/** Page factice du mode simulé (SVG aux dimensions de la page, numéro bien visible). */
function imageSimulee(livre: LivreOuvert, index: number): string {
  const clef = livre.jeton + '/' + index;
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
