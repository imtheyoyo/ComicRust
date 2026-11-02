// Préférences d'affichage (tri, groupe, vue, taille des couvertures, panneau de détails, thème).
// Mode portable strict : elles sont gardées par le backend dans data\comicrust-tauri.json (section
// `vue`), jamais dans le localStorage de la WebView. Chargées une fois au démarrage (avant le premier
// lister_livres), puis enregistrées avec un délai pour regrouper les changements rapides.
import { dansTauri, enregistrerPreferencesVue, lirePreferencesVue, type PreferencesVue } from './api';

/** Délai d'enregistrement après le dernier changement (ms). */
export const DELAI_ENREGISTREMENT = 400;

let courantes: PreferencesVue = {};
let enAttente: PreferencesVue = {};
let minuterie: ReturnType<typeof setTimeout> | undefined;

/** Anciennes clés du localStorage (tauri-v0.2.0 et avant), reprises une fois puis effacées. */
const ANCIENNES = ['theme', 'vue', 'zoom', 'details'] as const;

function reprendreAnciennes(): PreferencesVue {
  const reprises: Record<string, unknown> = {};
  try {
    for (const c of ANCIENNES) {
      const brut = localStorage.getItem('comicrust.' + c);
      if (brut === null) continue;
      try {
        reprises[c] = c === 'theme' ? brut : JSON.parse(brut);
      } catch {
        /* valeur illisible : ignorée */
      }
      localStorage.removeItem('comicrust.' + c);
    }
  } catch {
    /* stockage indisponible */
  }
  return reprises as PreferencesVue;
}

/** Lit les préférences enregistrées (vides en cas d'erreur : valeurs par défaut). */
export async function chargerPreferences(): Promise<PreferencesVue> {
  try {
    courantes = (await lirePreferencesVue()) ?? {};
  } catch (e) {
    console.warn('préférences d\'affichage illisibles, valeurs par défaut', e);
    courantes = {};
  }
  if (dansTauri() && Object.keys(courantes).length === 0) {
    const reprises = reprendreAnciennes();
    if (Object.keys(reprises).length > 0) {
      courantes = { ...reprises };
      enregistrerPreference(reprises); // le backend valide et ignore ce qui est inconnu
      vider();
    }
  }
  return { ...courantes };
}

/** Préférences connues (après `chargerPreferences`). */
export function preferences(): PreferencesVue {
  return { ...courantes };
}

/** Note les champs modifiés et programme l'enregistrement (rien si aucune valeur ne change). */
export function enregistrerPreference(patch: PreferencesVue): void {
  let change = false;
  for (const [c, v] of Object.entries(patch) as Array<[keyof PreferencesVue, unknown]>) {
    if (v === undefined || courantes[c] === v) continue;
    (courantes as Record<string, unknown>)[c] = v;
    (enAttente as Record<string, unknown>)[c] = v;
    change = true;
  }
  if (!change) return;
  clearTimeout(minuterie);
  minuterie = setTimeout(vider, DELAI_ENREGISTREMENT);
}

/** Envoie tout de suite les changements en attente (fermeture de la fenêtre). */
export function vider(): void {
  clearTimeout(minuterie);
  minuterie = undefined;
  if (Object.keys(enAttente).length === 0) return;
  const envoi = enAttente;
  enAttente = {};
  enregistrerPreferencesVue(envoi).catch((e) => {
    console.warn('préférences d\'affichage non enregistrées', e);
  });
}

if (typeof window !== 'undefined') {
  window.addEventListener('pagehide', vider);
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') vider();
  });
}
