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
  let balise = document.getElementById('theme-courant') as HTMLStyleElement | null;
  if (!balise) {
    balise = document.createElement('style');
    balise.id = 'theme-courant';
    document.head.appendChild(balise);
  }
  balise.textContent = css;
  try {
    localStorage.setItem(CLE, choisi);
  } catch {
    /* stockage indisponible : le thème par défaut reviendra au prochain lancement */
  }
  return choisi;
}
