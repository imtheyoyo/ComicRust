// Dialogues natifs de fichiers (lot L8, docs/tauri/ipc-dialogue.md) : « Ouvrir », « Choisir un
// dossier », « Enregistrer sous », via le plugin Tauri `dialog` (commandes `plugin:dialog|open` et
// `plugin:dialog|save`, appelées directement : pas de paquet npm en plus). Aucune logique métier.
//
// Hors Tauri (mode simulé, navigateur), il n'y a pas de dialogue : `dialogueDisponible()` rend faux, les
// boutons « Parcourir… » sont cachés et les fenêtres gardent la saisie du chemin. Les tests peuvent
// brancher un dialogue factice avec `window.__comicrustDialogueSimule` (voir `ChoixDialogue`).
import { dansTauri } from './api';
import { definirDialogueFichier } from './fenetre';
import { selecteurs } from './manquants.svelte';

/** Filtre d'extensions : `{ nom: 'Listes de lecture', extensions: ['cbl'] }` (sans point). */
export interface FiltreDialogue {
  nom: string;
  extensions: string[];
}

export interface OptionsDialogue {
  titre?: string;
  /** Chemin (dossier, ou fichier à présélectionner) où ouvrir le dialogue. */
  cheminInitial?: string;
  filtres?: FiltreDialogue[];
}

export type GenreDialogue = 'fichier' | 'dossier' | 'enregistrer';

/** Remplaçant d'un dialogue en mode simulé : posé par les tests sur `window.__comicrustDialogueSimule`. */
export type ChoixDialogue = (genre: GenreDialogue, options: OptionsDialogue) => Promise<string | null>;

declare global {
  interface Window {
    __comicrustDialogueSimule?: ChoixDialogue;
  }
}

/** Un dialogue est utilisable : fenêtre Tauri, ou dialogue factice posé par un test. */
export function dialogueDisponible(): boolean {
  return dansTauri() || (typeof window !== 'undefined' && typeof window.__comicrustDialogueSimule === 'function');
}

/** Chemin Windows normal : retire le préfixe étendu `\\?\` (et `\\?\UNC\serveur` devient `\\serveur`).
 *  Le plugin le fait déjà (`simplified`) ; cette garde protège le reste de l'application. */
export function cheminNormal(chemin: string): string {
  if (chemin.startsWith('\\\\?\\UNC\\')) return '\\\\' + chemin.slice(8);
  if (chemin.startsWith('\\\\?\\')) return chemin.slice(4);
  return chemin;
}

/** Entrée du plugin : `{ name, extensions }`. */
const filtresPlugin = (f?: FiltreDialogue[]) =>
  (f ?? []).filter((x) => x.extensions.length > 0).map((x) => ({ name: x.nom, extensions: x.extensions.map((e) => e.replace(/^\./, '')) }));

/** Le plugin renvoie un chemin (texte), ou `null` si l'utilisateur annule ; on tolère un objet `{ path }`. */
function extraire(r: unknown): string | null {
  if (typeof r === 'string') return r ? cheminNormal(r) : null;
  if (Array.isArray(r)) return extraire(r[0]);
  if (r && typeof r === 'object' && typeof (r as { path?: unknown }).path === 'string') return extraire((r as { path: string }).path);
  return null;
}

async function ouvrirDialogue(genre: GenreDialogue, o: OptionsDialogue): Promise<string | null> {
  if (!dansTauri()) {
    const factice = typeof window !== 'undefined' ? window.__comicrustDialogueSimule : undefined;
    return factice ? factice(genre, o) : null;
  }
  const { invoke } = await import('@tauri-apps/api/core');
  const defaultPath = o.cheminInitial?.trim() || undefined;
  if (genre === 'enregistrer') {
    return extraire(await invoke('plugin:dialog|save', { options: { title: o.titre, defaultPath, filters: filtresPlugin(o.filtres) } }));
  }
  return extraire(
    await invoke('plugin:dialog|open', {
      options: {
        title: o.titre,
        defaultPath,
        filters: genre === 'fichier' ? filtresPlugin(o.filtres) : [],
        directory: genre === 'dossier',
        multiple: false,
      },
    }),
  );
}

/** « Ouvrir » : rend le fichier choisi, ou `null` (annulé, ou dialogue indisponible). */
export const choisirFichier = (o: OptionsDialogue = {}): Promise<string | null> => ouvrirDialogue('fichier', o);

/** Choix d'un dossier : rend le dossier choisi, ou `null`. */
export const choisirDossier = (o: OptionsDialogue = {}): Promise<string | null> => ouvrirDialogue('dossier', o);

/** « Enregistrer sous » : rend le fichier à écrire (le système confirme l'écrasement), ou `null`. */
export const enregistrerSous = (o: OptionsDialogue = {}): Promise<string | null> => ouvrirDialogue('enregistrer', o);

/** Filtres usuels. */
export const FILTRE_CBL: FiltreDialogue = { nom: 'Listes de lecture ComicRack (*.cbl)', extensions: ['cbl'] };
export const FILTRE_XML: FiltreDialogue = { nom: 'ComicDb.xml', extensions: ['xml'] };
export const FILTRE_INI: FiltreDialogue = { nom: 'ComicRack.ini ou ComicDb.xml', extensions: ['ini', 'xml'] };
export const FILTRE_COMICS: FiltreDialogue = {
  nom: 'Bandes dessinées',
  extensions: ['cbz', 'cbr', 'cb7', 'cbt', 'zip', 'rar', '7z', 'tar', 'pdf', 'epub'],
};
export const FILTRE_TOUS: FiltreDialogue = { nom: 'Tous les fichiers', extensions: ['*'] };

/** Dossier d'un chemin (pour rouvrir le dialogue au bon endroit) ; texte vide si inconnu. */
export function dossierDe(chemin: string): string {
  const i = Math.max(chemin.lastIndexOf('\\'), chemin.lastIndexOf('/'));
  return i > 0 ? chemin.slice(0, i) : '';
}

/** Branche les dialogues sur les points d'extension (Ctrl+O et bouton « Ouvrir un fichier… », « Retrouver
 *  le fichier », « Chercher dans un dossier »). À appeler une fois au démarrage. Sans dialogue
 *  disponible, rien n'est branché : les fenêtres gardent la saisie du chemin. */
export function installerDialogues(): void {
  if (!dialogueDisponible()) return;
  definirDialogueFichier(() => choisirFichier({ titre: 'Ouvrir un fichier', filtres: [FILTRE_COMICS, FILTRE_CBL, FILTRE_TOUS] }));
  selecteurs.fichier = (initial) => choisirFichier({ titre: 'Retrouver le fichier', cheminInitial: initial, filtres: [FILTRE_COMICS, FILTRE_TOUS] });
  selecteurs.dossier = (initial) => choisirDossier({ titre: 'Dossier où chercher', cheminInitial: initial });
}
