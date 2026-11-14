// MODE SIMULÉ (étape 8) : faux backend des thèmes (docs/tauri/ipc-themes.md), utilisé seulement hors
// de Tauri. Les thèmes livrés sont les CSS de crates/comicrust-tauri/themes ; les thèmes personnels
// (import) vivent en mémoire jusqu'au rechargement de la page.
import type { ThemeCharge, ThemeInfo } from './api';

const LIVRES = import.meta.glob('../../../crates/comicrust-tauri/themes/*.css', {
  query: '?raw', import: 'default', eager: true,
}) as Record<string, string>;

interface Fichier { id: string; css: string; livre: boolean }

const idDuChemin = (chemin: string) => chemin.split('/').pop()!.replace(/\.css$/, '');

const fichiers = new Map<string, Fichier>();
for (const [chemin, css] of Object.entries(LIVRES)) {
  const id = idDuChemin(chemin);
  fichiers.set(id, { id, css, livre: true });
}

/** Thèmes que `charger_theme` refuse (erreur du backend autre que « introuvable »), pour les tests :
 *  `?themesRefuses=id1,id2` au chargement, ou modifiable ensuite via `__comicrustSimule.themesRefuses`. */
export const themesRefuses = new Set<string>(
  (new URLSearchParams(location.search).get('themesRefuses') ?? '').split(',').filter(Boolean),
);

const nomDe = (f: Fichier) => /@nom:\s*(.+?)\s*\*\//.exec(f.css)?.[1] ?? f.id;

/** Valeur d'une variable --cr-* déclarée dans le CSS (première occurrence), ou `null`. */
function variable(css: string, nom: string): string | null {
  return new RegExp(`--cr-${nom}\\s*:\\s*([^;}]+)`).exec(css)?.[1].trim() ?? null;
}

/** Avertissements simulés : mêmes motifs que le backend (@import et ressources réseau ignorés). */
function avertissementsDe(css: string): string[] {
  const a: string[] = [];
  if (/@import/i.test(css)) a.push('@import ignoré : un thème ne peut pas charger d\'autre feuille.');
  if (/url\(\s*['"]?https?:/i.test(css)) a.push('url(http…) ignoré : aucune ressource réseau n\'est chargée.');
  return a;
}

function info(f: Fichier): ThemeInfo {
  const fond = variable(f.css, 'fond');
  const accent = variable(f.css, 'accent');
  const texte = variable(f.css, 'texte');
  return {
    id: f.id, nom: nomDe(f), livre: f.livre,
    couleurs: fond && accent && texte ? { fond, accent, texte } : null,
    avertissements: avertissementsDe(f.css),
  };
}

function idLibre(nom: string): string {
  const base = nom.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'theme';
  let id = base;
  for (let n = 2; fichiers.has(id); n++) id = `${base}-${n}`;
  return id;
}

/** Commandes IPC de thèmes gérées par ce module. */
export const COMMANDES_THEMES = [
  'lister_themes', 'charger_theme', 'importer_theme', 'exporter_theme', 'supprimer_theme',
  'reinitialiser_themes_livres', 'reveler_dossier_themes',
];

export function creerThemesSimules() {
  return {
    appeler(commande: string, args: Record<string, unknown>): unknown {
      switch (commande) {
        case 'lister_themes':
          return [...fichiers.values()].map(info).sort((a, b) => Number(b.livre) - Number(a.livre) || a.nom.localeCompare(b.nom));
        case 'charger_theme': {
          const f = fichiers.get(String(args.id));
          if (!f) throw new Error(`thème introuvable : ${String(args.id)}`);
          // Comme le backend face à un fichier modifié à la main (parenthèse non fermée…) : refus.
          if (themesRefuses.has(f.id)) throw new Error(`thème « ${f.id} » refusé : parenthèse non fermée.`);
          const r: ThemeCharge = { css: f.css, avertissements: avertissementsDe(f.css) };
          return r;
        }
        case 'importer_theme': {
          const css = String(args.css ?? '');
          if (!css.trim()) throw new Error('le fichier est vide');
          if (!/:root\s*\{/.test(css)) throw new Error('CSS refusé : aucune règle :root { … } trouvée (un thème déclare des variables --cr-*).');
          const nom = String(args.nom ?? '').trim() || 'Thème importé';
          const id = idLibre(nom);
          const corps = /@nom:/.test(css) ? css : `/* @nom: ${nom} */\n${css}`;
          const f = { id, css: corps, livre: false };
          fichiers.set(id, f);
          return info(f);
        }
        case 'exporter_theme': {
          const f = fichiers.get(String(args.id));
          if (!f) throw new Error(`thème introuvable : ${String(args.id)}`);
          return f.css;
        }
        case 'supprimer_theme': {
          const f = fichiers.get(String(args.id));
          if (!f) throw new Error(`thème introuvable : ${String(args.id)}`);
          if (f.livre) throw new Error('un thème livré ne peut pas être supprimé (utilisez « Réinitialiser les thèmes livrés »)');
          fichiers.delete(f.id);
          return null;
        }
        case 'reinitialiser_themes_livres': {
          const ids: string[] = [];
          for (const [chemin, css] of Object.entries(LIVRES)) {
            const id = idDuChemin(chemin);
            fichiers.set(id, { id, css, livre: true });
            ids.push(id);
          }
          return ids;
        }
        case 'reveler_dossier_themes':
          return null;
        default:
          return undefined;
      }
    },
  };
}
