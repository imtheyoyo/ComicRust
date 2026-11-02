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
