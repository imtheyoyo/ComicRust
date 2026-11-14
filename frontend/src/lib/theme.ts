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

/** Charge le CSS du thème depuis data\themes et le substitue au précédent.
 *  - thème disparu (ou, au démarrage, CSS inutilisable) : repli sur « Moderne clair » ;
 *  - CSS invalide alors qu'un thème valide est déjà affiché : l'erreur est renvoyée et le thème
 *    affiché ne change pas.
 *  Renvoie l'identifiant réellement appliqué. */
export async function appliquerTheme(id: string, opts: OptionsTheme = {}): Promise<string> {
  const persister = opts.persister ?? true;
  let choisi = id;
  let reponse;
  try {
    reponse = await chargerTheme(id);
    if (reponse.css !== cssApplique || id !== idApplique) injecterCss(reponse.css);
  } catch (e) {
    const invalide = e instanceof ErreurThemeInvalide;
    // Thème invalide mais un thème valide est déjà à l'écran : on le garde et on signale l'erreur.
    if (id === THEME_DEFAUT || (invalide && feuille)) throw e;
    // Thème supprimé / illisible (ou premier affichage) : retour au thème par défaut.
    choisi = THEME_DEFAUT;
    reponse = await chargerTheme(THEME_DEFAUT);
    injecterCss(reponse.css);
  }
  cssApplique = reponse.css;
  avertissementsCourants = reponse.avertissements;
  idApplique = choisi;
  if (persister) {
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
 *  Si le thème affiché est le thème enregistré, un repli éventuel est enregistré aussi. */
export async function rechargerThemeCourant(): Promise<string> {
  const id = idApplique ?? themeMemorise();
  const enregistre = id === themePersiste();
  const choisi = await appliquerTheme(id, { persister: false });
  if (enregistre && choisi !== id) {
    idPersiste = choisi;
    enregistrerPreference({ theme: choisi });
  }
  return choisi;
}

/** Message lisible d'une erreur de thème. */
export function messageTheme(e: unknown): string {
  return e instanceof ErreurThemeInvalide ? e.message : messageErreur(e);
}
