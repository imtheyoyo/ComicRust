import { chargerTheme, messageErreur } from './api';
import { enregistrerPreference, preferences } from './prefs';

export const THEME_DEFAUT = 'moderne-clair';

/** Thème mémorisé dans data\comicrust-tauri.json (après `chargerPreferences`). */
export function themeMemorise(): string {
  return preferences().theme ?? THEME_DEFAUT;
}

/** Le CSS d'un thème est inutilisable (feuille refusée par le navigateur, aucune règle :root). */
export class ErreurThemeInvalide extends Error {}

let feuille: CSSStyleSheet | null = null;
/** CSS actuellement appliqué : évite de réinjecter (et de faire clignoter) un CSS inchangé. */
let cssApplique: string | null = null;
/** Thème actuellement affiché (aperçu compris) et thème enregistré dans les préférences. */
let idApplique: string | null = null;
let idPersiste: string | null = null;
let avertissementsCourants: string[] = [];

/** Thème affiché à l'écran (peut être un aperçu temporaire). */
export function themeApplique(): string | null {
  return idApplique;
}

/** Thème enregistré (celui qui revient après un aperçu annulé). */
export function themePersiste(): string {
  return idPersiste ?? themeMemorise();
}

/** Avertissements du CSS du thème affiché (@import, url(http…) ignorés par le backend…). */
export function avertissementsApplique(): string[] {
  return avertissementsCourants;
}

/** Vérifie qu'une feuille compilée contient au moins une règle `:root` (sinon le thème ne définit rien). */
function aRegleRoot(f: CSSStyleSheet): boolean {
  const cherche = (regles: CSSRuleList): boolean => {
    for (const r of Array.from(regles)) {
      if (r instanceof CSSStyleRule && /:root|\bhtml\b/.test(r.selectorText)) return true;
      if ('cssRules' in r && cherche((r as CSSGroupingRule).cssRules)) return true;
    }
    return false;
  };
  return cherche(f.cssRules);
}

/** Applique le CSS du thème. `adoptedStyleSheets` ne passe pas par `style-src` : une CSP sans
 *  'unsafe-inline' ne le neutralise pas. En cas d'erreur, la dernière feuille valide reste appliquée
 *  (rien n'est retiré avant que la nouvelle feuille ait été compilée et jugée valide). Repli sur une
 *  balise <style> uniquement si l'API des feuilles adoptées est indisponible. */
function injecterCss(css: string): void {
  if ('adoptedStyleSheets' in document && typeof CSSStyleSheet !== 'undefined') {
    const nouvelle = new CSSStyleSheet();
    try {
      nouvelle.replaceSync(css);
    } catch (e) {
      throw new ErreurThemeInvalide(`CSS refusé par le navigateur : ${e instanceof Error ? e.message : String(e)}`);
    }
    if (!aRegleRoot(nouvelle)) {
      throw new ErreurThemeInvalide('CSS inutilisable : aucune règle :root { … } (un thème déclare des variables --cr-*).');
    }
    const autres = document.adoptedStyleSheets.filter((f) => f !== feuille);
    document.adoptedStyleSheets = [...autres, nouvelle];
    feuille = nouvelle;
    document.getElementById('theme-courant')?.remove();
    return;
  }
  let balise = document.getElementById('theme-courant') as HTMLStyleElement | null;
  if (!balise) {
    balise = document.createElement('style');
    balise.id = 'theme-courant';
    document.head.appendChild(balise);
  }
  balise.textContent = css;
}

export interface OptionsTheme {
  /** Enregistrer le choix dans les préférences (défaut : oui). `false` = aperçu temporaire. */
  persister?: boolean;
}

/** Jeton de séquence : seule la dernière application DEMANDÉE a le droit de modifier l'affichage
 *  (une réponse tardive d'un aperçu précédent est ignorée). */
let sequence = 0;

/** Le backend signale un thème absent par « thème introuvable » ; toute autre erreur (CSS refusé…) ne
 *  justifie pas de changer de thème. */
function estIntrouvable(e: unknown): boolean {
  return !(e instanceof ErreurThemeInvalide) && /introuvable/i.test(messageErreur(e));
}

/** Charge le CSS du thème depuis data\themes et le substitue au précédent.
 *  - thème disparu (« introuvable ») ou, au tout premier affichage, thème inutilisable : repli sur
 *    « Moderne clair », JAMAIS enregistré dans les préférences ;
 *  - toute autre erreur (CSS refusé par le backend ou le navigateur) alors qu'un thème est déjà affiché :
 *    l'erreur est renvoyée et l'affichage ne change pas ;
 *  - si deux demandes se chevauchent, seule la dernière modifie l'affichage (l'autre renvoie le thème
 *    alors affiché, sans rien enregistrer).
 *  Renvoie l'identifiant réellement appliqué. */
export async function appliquerTheme(id: string, opts: OptionsTheme = {}): Promise<string> {
  const persister = opts.persister ?? true;
  const mien = ++sequence;
  const perime = () => mien !== sequence;
  let choisi = id;
  let reponse;
  try {
    reponse = await chargerTheme(id);
    if (perime()) return idApplique ?? id;
    if (reponse.css !== cssApplique || id !== idApplique) injecterCss(reponse.css);
  } catch (e) {
    if (perime()) return idApplique ?? id;
    // Rien de valide à l'écran : on tente le thème par défaut ; sinon seul « introuvable » y ramène.
    if (id === THEME_DEFAUT || !(estIntrouvable(e) || !feuille)) throw e;
    choisi = THEME_DEFAUT;
    reponse = await chargerTheme(THEME_DEFAUT);
    if (perime()) return idApplique ?? id;
    injecterCss(reponse.css);
  }
  cssApplique = reponse.css;
  avertissementsCourants = reponse.avertissements;
  idApplique = choisi;
  // Un repli n'est jamais enregistré : le choix de l'utilisateur reste celui des préférences.
  if (persister && choisi === id) {
    idPersiste = choisi;
    enregistrerPreference({ theme: choisi });
  }
  return choisi;
}

/** Aperçu temporaire : applique le thème sans l'enregistrer. Renvoie l'id appliqué. */
export function apercuTheme(id: string): Promise<string> {
  return appliquerTheme(id, { persister: false });
}

/** Annule un aperçu : réaffiche le thème enregistré. */
export function restaurerTheme(): Promise<string> {
  return appliquerTheme(themePersiste(), { persister: false });
}

/** Relit le fichier du thème affiché (modifié à la main, ou thème supprimé) et le réapplique.
 *  Un éventuel repli n'est jamais enregistré. */
export function rechargerThemeCourant(): Promise<string> {
  return appliquerTheme(idApplique ?? themeMemorise(), { persister: false });
}

/** Message lisible d'une erreur de thème. */
export function messageTheme(e: unknown): string {
  return e instanceof ErreurThemeInvalide ? e.message : messageErreur(e);
}
