import { chargerTheme } from './api';

const CLE = 'comicrust.theme';
export const THEME_DEFAUT = 'moderne-clair';

/** Thème mémorisé (le profil WebView2 est dans data\webview2 : rien hors du dossier portable). */
export function themeMemorise(): string {
  try {
    return localStorage.getItem(CLE) ?? THEME_DEFAUT;
  } catch {
    return THEME_DEFAUT;
  }
}

let feuille: CSSStyleSheet | null = null;

/** Applique le CSS du thème. `adoptedStyleSheets` ne passe pas par `style-src` : une CSP sans
 *  'unsafe-inline' ne le neutralise pas. Repli sur une balise <style> si l'API est indisponible. */
function injecterCss(css: string): void {
  try {
    if (!('adoptedStyleSheets' in document) || typeof CSSStyleSheet === 'undefined') throw new Error('indisponible');
    const nouvelle = new CSSStyleSheet();
    nouvelle.replaceSync(css);
    const autres = document.adoptedStyleSheets.filter((f) => f !== feuille);
    document.adoptedStyleSheets = [...autres, nouvelle];
    feuille = nouvelle;
    document.getElementById('theme-courant')?.remove();
    return;
  } catch {
    /* repli ci-dessous */
  }
  if (feuille) {
    document.adoptedStyleSheets = document.adoptedStyleSheets.filter((f) => f !== feuille);
    feuille = null;
  }
  let balise = document.getElementById('theme-courant') as HTMLStyleElement | null;
  if (!balise) {
    balise = document.createElement('style');
    balise.id = 'theme-courant';
    document.head.appendChild(balise);
  }
  balise.textContent = css;
}

/** Charge le CSS du thème depuis data\themes et le substitue au précédent. */
export async function appliquerTheme(id: string): Promise<string> {
  let choisi = id;
  let css: string;
  try {
    css = await chargerTheme(id);
  } catch (e) {
    // Thème supprimé ou invalide : retour au thème par défaut.
    if (id === THEME_DEFAUT) throw e;
    choisi = THEME_DEFAUT;
    css = await chargerTheme(THEME_DEFAUT);
  }
  injecterCss(css);
  try {
    localStorage.setItem(CLE, choisi);
  } catch {
    /* stockage indisponible : le thème par défaut reviendra au prochain lancement */
  }
  return choisi;
}
