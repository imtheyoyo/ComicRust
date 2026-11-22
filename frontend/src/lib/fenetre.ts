// Fenêtre principale : titre dynamique, plein écran, Ctrl+O, fichiers déposés ou passés en argument.
// Aucune logique métier : la nature des chemins et la mémorisation de la géométrie sont dans le
// backend (docs/tauri/ipc-fenetre.md). Comportement de référence : crates/comicrust/src/input.rs
// (F11, Échap, dépôt de fichiers, `update_title`) et main.rs (`initial`) de la version egui.

import { classerChemin, dansTauri, definirTitreFenetre, pleinEcran, type GenreChemin } from './api';

// ───────────────────────────── Titre ─────────────────────────────

/** Le lecteur est affiché (monté) : F11, Échap et Ctrl+O lui appartiennent. */
let lecteurMonte = false;

/** À appeler par le lecteur à son montage (`true`) et à son démontage (`false`). */
export function signalerLecteur(actif: boolean): void {
  lecteurMonte = actif;
}

export const lecteurAffiche = (): boolean => lecteurMonte;

/** Titre du livre ouvert (`null` : aucun) : fenêtre « <livre> — ComicRust <version> » ou
 *  « ComicRust <version> », comme `update_title`. La version est ajoutée par le backend (elle vient de
 *  Cargo) ; hors Tauri, le backend simulé met à jour `document.title`. */
export function definirLivreOuvert(titre: string | null): void {
  const propre = titre?.trim() || null;
  void definirTitreFenetre(propre).catch((e) => console.warn('titre de la fenêtre', e));
}

// ───────────────────────────── Plein écran ─────────────────────────────

/** État connu du plein écran de la fenêtre (mis à jour à chaque redimensionnement). */
let plein = false;

export const pleinEcranFenetreActif = (): boolean => plein;

async function lirePleinEcran(): Promise<boolean> {
  if (dansTauri()) {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      return await getCurrentWindow().isFullscreen();
    } catch {
      /* permission absente : repli sur le document */
    }
  }
  return !!document.fullscreenElement;
}

/** Suit le plein écran de la fenêtre, y compris celui décidé par le lecteur. Renvoie l'arrêt du suivi. */
export function suivrePleinEcran(): () => void {
  const maj = () => void lirePleinEcran().then((p) => (plein = p));
  window.addEventListener('resize', maj);
  document.addEventListener('fullscreenchange', maj);
  maj();
  return () => {
    window.removeEventListener('resize', maj);
    document.removeEventListener('fullscreenchange', maj);
  };
}

/** Bascule le plein écran de la fenêtre (F11). */
export async function basculerPleinEcranFenetre(): Promise<void> {
  const voulu = !plein;
  try {
    await pleinEcran(voulu);
    plein = voulu;
  } catch (e) {
    console.warn('plein écran', e);
  }
}

// ───────────────────────────── Ouvrir un fichier (Ctrl+O) ─────────────────────────────

/** Remplaçant du champ de saisie : dialogue natif « Ouvrir » (lot L8, plugin dialog). Rend le chemin
 *  choisi, ou `null` si l'utilisateur annule. */
type FournisseurDialogue = () => Promise<string | null>;
let fournisseur: FournisseurDialogue | null = null;

/** POINT D'EXTENSION du lot « dialogue natif » : y brancher le sélecteur de fichiers du système. Tant
 *  qu'aucun n'est branché, Ctrl+O ouvre la fenêtre « Ouvrir un fichier » (chemin saisi). */
export function definirDialogueFichier(f: FournisseurDialogue | null): void {
  fournisseur = f;
}

export const dialogueFichierDisponible = (): boolean => fournisseur !== null;

/** Dialogue natif « Ouvrir un fichier » : `null` s'il est annulé, ou s'il n'existe pas encore. */
export async function ouvrirFichierDialogue(): Promise<string | null> {
  return fournisseur ? fournisseur() : null;
}

/** Ce que la vue de la bibliothèque sait faire : lire un chemin dans le lecteur, ou demander le chemin. */
export interface OuvreurFichier {
  ouvrirChemin: (chemin: string) => void | Promise<void>;
  saisirChemin: () => void;
}

let ouvreur: OuvreurFichier | null = null;

/** La vue de la bibliothèque s'enregistre (à son montage) ; renvoie le désenregistrement. */
export function enregistrerOuvreur(o: OuvreurFichier): () => void {
  ouvreur = o;
  return () => {
    if (ouvreur === o) ouvreur = null;
  };
}

/** Attend que la vue de la bibliothèque soit montée (au plus ~2 s : démarrage ou changement de section). */
export async function attendreOuvreur(): Promise<OuvreurFichier | null> {
  for (let i = 0; i < 40 && !ouvreur; i++) await new Promise((r) => setTimeout(r, 50));
  return ouvreur;
}

/** Ctrl+O : dialogue natif s'il existe, sinon fenêtre de saisie du chemin. */
export async function demanderOuvertureFichier(): Promise<void> {
  if (!ouvreur) return;
  if (!dialogueFichierDisponible()) {
    ouvreur.saisirChemin();
    return;
  }
  const chemin = await ouvrirFichierDialogue();
  if (!chemin) return;
  // Même aiguillage qu'un fichier déposé (un .cbl s'importe, il ne s'ouvre pas comme livre).
  if (routeurChemins) await routeurChemins(chemin);
  else await ouvreur?.ouvrirChemin(chemin);
}

let routeurChemins: ((chemin: string) => Promise<void>) | null = null;
/** Le composant des fichiers déposés fournit l'aiguillage par nature de chemin (Ctrl+O). */
export function enregistrerRouteurChemins(f: (chemin: string) => Promise<void>): () => void {
  routeurChemins = f;
  return () => {
    if (routeurChemins === f) routeurChemins = null;
  };
}

// ───────────────────────────── Clavier de la fenêtre ─────────────────────────────

/** Fenêtre modale ou menu ouvert : le clavier est à lui seul. */
const calqueOuvert = (): boolean => !!document.querySelector('dialog[open], [aria-modal="true"], [role="menu"]');

/** Raccourcis de la fenêtre hors lecteur (le lecteur gère ses propres touches) : F11 plein écran,
 *  Échap pour en sortir, Ctrl+O. À brancher en phase de capture sur `window`. */
export function gererClavierFenetre(e: KeyboardEvent): void {
  if (lecteurMonte || e.altKey) return;
  const ctrl = e.ctrlKey || e.metaKey;
  if (e.key === 'F11' && !ctrl && !e.shiftKey) {
    e.preventDefault();
    void basculerPleinEcranFenetre();
  } else if (e.key === 'Escape' && plein && !calqueOuvert()) {
    // Échap quitte le plein écran avant tout autre usage (pile ouverte, sélection…).
    e.preventDefault();
    e.stopPropagation();
    void basculerPleinEcranFenetre();
  } else if (ctrl && !e.shiftKey && e.key.toLowerCase() === 'o' && !calqueOuvert()) {
    e.preventDefault(); // pas le « Ouvrir » du navigateur
    void demanderOuvertureFichier();
  }
}

// ───────────────────────────── Fichiers déposés ─────────────────────────────

/** Écoute les fichiers déposés sur la fenêtre (Tauri : `onDragDropEvent`). `survol` : un glisser
 *  est au-dessus de la fenêtre. En mode simulé : événements `comicrust-simule-depot` (détail :
 *  liste de chemins) et `comicrust-simule-survol` (détail : booléen). Renvoie l'arrêt de l'écoute. */
export async function ecouterDepots(rappel: (chemins: string[]) => void, survol: (actif: boolean) => void): Promise<() => void> {
  if (dansTauri()) {
    const { getCurrentWebview } = await import('@tauri-apps/api/webview');
    return getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === 'enter' || p.type === 'over') survol(true);
      else if (p.type === 'drop') {
        survol(false);
        if (p.paths.length) rappel(p.paths);
      } else survol(false);
    });
  }
  const depot = (e: Event) => rappel((e as CustomEvent<string[]>).detail);
  const sur = (e: Event) => survol(!!(e as CustomEvent<boolean>).detail);
  window.addEventListener('comicrust-simule-depot', depot);
  window.addEventListener('comicrust-simule-survol', sur);
  return () => {
    window.removeEventListener('comicrust-simule-depot', depot);
    window.removeEventListener('comicrust-simule-survol', sur);
  };
}

/** Actions possibles sur un chemin reçu de l'extérieur ; fournies par le composant `DepotFichiers`. */
export interface ActionsChemin {
  /** Fichier (ou dossier choisi comme livre) à lire dans le lecteur. */
  lire: (chemin: string) => Promise<void>;
  /** `.cbl` à importer dans les listes. */
  importerListe: (chemin: string) => Promise<void>;
  /** Dossier : choix entre lecture et ajout aux dossiers de la bibliothèque. */
  proposerDossier: (chemin: string) => void;
  /** Chemin introuvable ou échec. */
  erreur: (message: string) => void;
}

/** Aiguille un chemin selon sa nature (comme `open_path` de la version egui : `.cbl` = liste, le reste
 *  = livre ; un dossier propose en plus de l'ajouter aux dossiers). */
export async function traiterChemin(chemin: string, a: ActionsChemin, genre?: GenreChemin): Promise<void> {
  const g = genre ?? (await classerChemin(chemin));
  switch (g) {
    case 'absent':
      a.erreur(`Fichier ou dossier introuvable : ${chemin}`);
      break;
    case 'liste':
      await a.importerListe(chemin);
      break;
    case 'dossier':
      a.proposerDossier(chemin);
      break;
    default:
      await a.lire(chemin);
  }
}
