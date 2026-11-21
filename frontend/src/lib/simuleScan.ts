// MODE SIMULÉ (lot « Scan et dossiers ») : faux backend de l'analyse des dossiers
// (docs/tauri/ipc-scan.md), utilisé seulement hors de Tauri. Il imite la progression, l'arrêt
// (« Arrêter l'analyse »), le bilan avec erreurs, l'analyse automatique du démarrage et les réglages
// « Analyse et suivi ». La logique réelle est dans le backend ; ceci ne sert qu'à développer et
// tester le front.
//
// Paramètres d'adresse :
// - `?scanAuto` : l'analyse du démarrage a lieu (réglage activé, mode autonome, en ligne, des
//   dossiers) ; sans ce paramètre elle n'a pas lieu, pour ne pas perturber les autres scénarios ;
// - `?scanErreurs=N` : nombre d'erreurs du bilan (défaut 2) ;
// - `?scanLent` : analyse trois fois plus lente (laisse le temps de cliquer sur « Arrêter »).
import type { BilanScan, ModeBiblio, ReglagesAnalyse } from './api';

export interface ContexteScan {
  emettre: (nom: string, charge: unknown) => void;
  pause: (ms: number) => Promise<unknown>;
  mode: () => ModeBiblio;
  /** Raison d'impossibilité d'écrire quand la base est injoignable (`undefined` : en ligne). */
  horsLigne: () => string | undefined;
  dossiers: () => string[];
}

const TOTAL = 1240;

export function creerScanSimule(ctx: ContexteScan) {
  const q = () => new URLSearchParams(location.search);
  let enCours = false;
  let annule = false;
  let reglages: ReglagesAnalyse = { auDemarrage: true, suivreComicrack: true };

  function bilanSimule(arretee: boolean, fait: number): BilanScan {
    const n = q().has('scanErreurs') ? Math.max(0, Number(q().get('scanErreurs')) || 0) : 2;
    const erreurs = Array.from({ length: Math.min(n, 500) }, (_, i) => ({
      chemin: `D:\\BD\\Marvel\\Cassé ${i + 1}.cbz`,
      message: 'archive illisible : fin de fichier inattendue',
    }));
    const resume = `${fait} fichiers : 12 ajoutés, 34 mis à jour (0 déplacés), 3 manquants, 0 retrouvés, ${n} erreurs${arretee ? ' — interrompu' : ''}`;
    const dossiers = ctx.dossiers().length;
    const base: BilanScan = { resume, dossiers, fichiers: fait, arretee, erreurs, erreursTotal: n, texte: '' };
    base.texte =
      `ComicRust v0.0.0 — analyse des dossiers\n${resume}\n${dossiers} dossier(s) analysé(s).\n` +
      erreurs.map((e) => `- ${e.chemin} : ${e.message}\n`).join('');
    return base;
  }

  async function analyser() {
    enCours = true;
    annule = false;
    const lent = q().has('scanLent') ? 3 : 1;
    let fait = 0;
    for (; fait <= TOTAL && !annule; fait += 31) {
      ctx.emettre('scan-progression', { fait: Math.min(fait, TOTAL), total: TOTAL });
      await ctx.pause(60 * lent);
    }
    const arretee = annule;
    fait = arretee ? Math.min(fait, TOTAL) : TOTAL;
    ctx.emettre('scan-progression', { fait, total: TOTAL });
    enCours = false;
    ctx.emettre('scan-bilan', bilanSimule(arretee, fait));
    const part = arretee ? fait / TOTAL : 1;
    ctx.emettre('scan-termine', {
      ajoutes: Math.round(12 * part),
      misAJour: Math.round(34 * part),
      manquants: Math.round(3 * part),
    });
  }

  /** Commande de l'analyse, ou `undefined` si elle n'est pas de ce lot. */
  function appeler(commande: string, args: Record<string, unknown>): unknown {
    switch (commande) {
      case 'lancer_scan':
        if (ctx.mode() === 'comicrack') throw new Error("l'analyse des dossiers n'existe qu'en mode autonome (en mode lié, ComicRack gère sa bibliothèque)");
        if (enCours) throw new Error('une analyse est déjà en cours');
        void analyser();
        return null;
      case 'arreter_scan':
        if (!enCours) return false;
        annule = true;
        return true;
      case 'lancer_scan_auto':
        if (
          !q().has('scanAuto') || !reglages.auDemarrage || ctx.mode() !== 'autonome' || ctx.horsLigne() ||
          ctx.dossiers().length === 0 || enCours
        ) return false;
        void analyser();
        return true;
      case 'lire_reglages_analyse':
        return { ...reglages };
      case 'enregistrer_reglages_analyse':
        reglages = { ...reglages, ...(args.reglages as Partial<ReglagesAnalyse>) };
        return { ...reglages };
      default:
        return undefined;
    }
  }

  return { appeler };
}
