// MODE SIMULÉ (tauri-v0.8.0) : faux backend de la mise à jour (docs/tauri/ipc-maj.md), utilisé
// seulement hors de Tauri pour développer et tester l'interface. La vérification répond après 1 s,
// le téléchargement progresse par paliers puis l'installation se termine avec un bilan. Rien n'est
// téléchargé ni installé.
//
// Paramètres d'adresse :
// - `?maj=ajour` : aucune nouvelle version ; `?maj=echec` : GitHub injoignable ;
// - `?maj=echecInstall` : le paquet téléchargé est refusé (SHA-256 incorrect) ;
// - `?maj=noninstallable` : version de développement / hors Windows (`installable` faux) ;
// - `?majAuto=non` : « Vérifier au démarrage » décoché au premier lancement.
// Les réglages (vérification au démarrage, version ignorée) sont gardés dans le stockage local du
// navigateur pour simuler un redémarrage ; `?majOubli` les efface.
import type { EtatMaj, ReleaseMaj, StatutMaj } from './api';

const CLEF = 'comicrust-simule-maj';
const VERSION = '0.8.0';

const RELEASE: ReleaseMaj = {
  tag: 'tauri-v0.8.1',
  nom: 'ComicRust tauri-v0.8.1',
  taille: 18_400_000,
  aPaquet: true,
  urlPage: 'https://github.com/imtheyoyo/ComicRust/releases/tag/tauri-v0.8.1',
  notes: [
    '## [tauri-v0.8.1] — 2026-11-20',
    '',
    '### Nouveautés',
    '- **Mise à jour intégrée** : bandeau et fenêtre « Mise à jour », paquet vérifié (SHA-256).',
    '- Notes de version lisibles, voir [la page de la release](https://github.com/imtheyoyo/ComicRust/releases).',
    '* Réglage `Vérifier au démarrage`.',
    '',
    '### Corrections',
    '1. Le Détail du premier livre est à jour quand on ouvre un autre livre à sa place (C-05).',
    '2. Texte piégé affiché tel quel : <img src=x onerror="window.__piege=1"> et [lien](javascript:alert(1)).',
    '',
    'Paragraphe libre sur deux lignes',
    'qui se suivent.',
  ].join('\n'),
};

export interface ContexteMaj {
  emettre: (nom: string, charge: unknown) => void;
  pause: (ms: number) => Promise<unknown>;
}

interface Reglages { verifierAuDemarrage: boolean; versionIgnoree: string | null }

export function creerMajSimulee(ctx: ContexteMaj) {
  const q = new URLSearchParams(location.search);
  const variante = q.get('maj') ?? '';
  let reglages: Reglages = { verifierAuDemarrage: q.get('majAuto') !== 'non', versionIgnoree: null };
  try {
    if (q.has('majOubli')) localStorage.removeItem(CLEF);
    const brut = localStorage.getItem(CLEF);
    if (brut) reglages = { ...reglages, ...(JSON.parse(brut) as Partial<Reglages>) };
  } catch {
    /* stockage indisponible : réglages par défaut */
  }
  let statut: StatutMaj = { etat: 'inactif', manuel: false };
  let occupe = false;
  /** Journal lisible par les tests (`window.__comicrustSimuleMaj`). */
  const journal: Array<{ commande: string; args: Record<string, unknown> }> = [];
  (globalThis as { __comicrustSimuleMaj?: unknown }).__comicrustSimuleMaj = { journal, statut: () => statut };

  const enregistrer = () => {
    try { localStorage.setItem(CLEF, JSON.stringify(reglages)); } catch { /* sans effet */ }
  };
  const publier = (s: StatutMaj) => {
    statut = s;
    ctx.emettre('maj://statut', s);
  };
  const etat = (): EtatMaj => ({
    version: VERSION,
    versionComparee: VERSION,
    verifierAuDemarrage: reglages.verifierAuDemarrage,
    versionIgnoree: reglages.versionIgnoree,
    statut,
    installable: variante !== 'noninstallable',
  });

  async function verifier(manuel: boolean) {
    occupe = true;
    publier({ etat: 'verification', manuel });
    await ctx.pause(1000);
    occupe = false;
    if (variante === 'echec') {
      publier({ etat: 'echec', manuel, erreur: 'Impossible de joindre GitHub (api.github.com) : délai dépassé. Vérifie ta connexion à Internet.' });
    } else if (variante === 'ajour' || (!manuel && reglages.versionIgnoree === RELEASE.tag)) {
      // Vérification automatique : une version ignorée n'est pas proposée (règle du contrat).
      publier({ etat: 'aJour', manuel });
    } else {
      publier({ etat: 'disponible', manuel, release: RELEASE });
    }
  }

  async function installer(manuel: boolean) {
    occupe = true;
    publier({ etat: 'installation', manuel, release: RELEASE });
    const total = RELEASE.taille;
    for (let recu = 0; recu < total; recu += 1_150_000) {
      ctx.emettre('maj://progression', { recu, total });
      await ctx.pause(100);
      if (variante === 'echecInstall' && recu > total / 2) {
        occupe = false;
        publier({ etat: 'echec', manuel, release: RELEASE, erreur: "SHA-256 du paquet incorrect : le téléchargement est corrompu, rien n'a été installé." });
        return;
      }
    }
    ctx.emettre('maj://progression', { recu: total, total });
    await ctx.pause(300);
    occupe = false;
    publier({ etat: 'installee', manuel, release: RELEASE, bilan: { remplaces: 42, restesOld: 1 } });
  }

  return {
    /** Renvoie `undefined` si la commande n'est pas une commande de mise à jour. */
    appeler(commande: string, args: Record<string, unknown>): unknown {
      if (!commande.startsWith('maj_')) return undefined;
      journal.push({ commande, args: JSON.parse(JSON.stringify(args)) as Record<string, unknown> });
      switch (commande) {
        case 'maj_etat':
          return etat();
        case 'maj_verifier':
          if (occupe) throw new Error('Une vérification ou une installation est déjà en cours.');
          void verifier(args.manuel === true);
          return null;
        case 'maj_installer':
          if (variante === 'noninstallable') throw new Error("L'installation automatique n'est possible que dans la version Windows de ComicRust.");
          if (occupe) throw new Error('Une vérification ou une installation est déjà en cours.');
          if (args.tag !== RELEASE.tag) throw new Error(`release inconnue : ${String(args.tag)}`);
          void installer(statut.manuel);
          return null;
        case 'maj_redemarrer':
          if (statut.etat !== 'installee') throw new Error("Aucune mise à jour installée : rien à redémarrer.");
          // Redémarrage simulé : la page est rechargée (la version reste la même en mode simulé).
          setTimeout(() => location.reload(), 300);
          return null;
        case 'maj_regler':
          if (typeof args.verifierAuDemarrage === 'boolean') reglages.verifierAuDemarrage = args.verifierAuDemarrage;
          if ('versionIgnoree' in args && args.versionIgnoree !== undefined) reglages.versionIgnoree = (args.versionIgnoree as string | null) ?? null;
          enregistrer();
          return etat();
        default:
          throw new Error(`commande inconnue (mode simulé) : ${commande}`);
      }
    },
  };
}
