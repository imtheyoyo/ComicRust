// Préférences d'affichage mémorisées par vue (profil WebView2 portable : rien hors du dossier data).
export function lirePref<T>(clef: string, defaut: T): T {
  try {
    const v = localStorage.getItem('comicrust.' + clef);
    return v === null ? defaut : (JSON.parse(v) as T);
  } catch {
    return defaut;
  }
}

export function ecrirePref(clef: string, valeur: unknown): void {
  try {
    localStorage.setItem('comicrust.' + clef, JSON.stringify(valeur));
  } catch {
    /* stockage indisponible : les valeurs par défaut reviendront au prochain lancement */
  }
}
