// MODE SIMULÉ (étape 6) : faux backend du transfert ComicRack → ComicRust (docs/tauri/ipc-transfert.md),
// utilisé seulement hors de Tauri. Il imite l'aperçu, la progression, l'arrêt et le bilan (5 323
// livres, 47 listes, idempotent : le second passage met à jour sans dupliquer). La logique réelle
// est dans le backend ; ceci ne sert qu'à développer et tester le front.
//
// Paramètres d'adresse : `?sansMysql` (aucune base MySQL trouvée), `?transfertErreur` (le transfert
// échoue), `?transfertBloque` (analyse en cours).
import type { ApercuTransfert, DemandeTransfert, EtatBiblio, ResumeTransfert } from './api';

const LIVRES = 5323;
const LISTES = 47;

export interface ContexteTransfert {
  emettre: (nom: string, charge: unknown) => void;
  pause: (ms: number) => Promise<unknown>;
  /** État simulé de la bibliothèque affichée. */
  etat: () => EtatBiblio;
  /** Bascule en autonome (après un transfert complet) ; renvoie le nouvel état. */
  basculer: () => EtatBiblio;
}

export function creerTransfertSimule(ctx: ContexteTransfert) {
  const q = () => new URLSearchParams(location.search);
  let dejaTransfere = false;
  let annule = false;
  let enCours = false;

  function apercu(): ApercuTransfert {
    const mysql = q().has('sansMysql') ? undefined : 'comicrack@192.168.1.10:3306/comicrack';
    return {
      mysql,
      sourceParDefaut: mysql ? 'mysql' : 'comicdb',
      comicdb: 'D:\\ComicRack\\Base\\ComicDb.xml',
      comicdbPresent: true,
      destination: 'SQLite (mode simulé) data\\comicrust.db',
      destinationExiste: dejaTransfere,
      nbLivresDestination: dejaTransfere ? LIVRES : undefined,
      mode: ctx.etat().mode,
      bloque: q().has('transfertBloque') ? 'Analyse des dossiers en cours : attends sa fin (ou arrête-la) avant de transférer.' : undefined,
    };
  }

  async function lancer(demande: DemandeTransfert): Promise<ResumeTransfert> {
    if (enCours) throw new Error('une opération est déjà en cours');
    if (q().has('transfertErreur')) throw new Error('connexion à comicrack@192.168.1.10:3306/comicrack impossible : délai dépassé');
    enCours = true;
    annule = false;
    try {
      const phases: Array<[string, number]> = [];
      if (demande.source === 'mysql') phases.push(['lecture de la base MySQL de ComicRack…', LIVRES]);
      phases.push(['écriture dans la base de ComicRust (livres, puis listes et dossiers)…', LIVRES]);
      let interrompu = false;
      for (const [phase, total] of phases) {
        for (let fait = 0; fait <= total && !interrompu; fait += 400) {
          ctx.emettre('transfert-progression', { phase, fait: Math.min(fait, total), total });
          await ctx.pause(60);
          interrompu = annule;
        }
        if (interrompu) break;
        ctx.emettre('transfert-progression', { phase, fait: total, total });
      }
      const repris = dejaTransfere && !interrompu;
      const r: ResumeTransfert = {
        livresSource: LIVRES,
        ajoutes: interrompu ? 1200 : repris ? 0 : LIVRES,
        remplaces: 0,
        misAJour: repris ? LIVRES : 0,
        ecartes: 0,
        doublonsEvites: repris ? LIVRES : 0,
        listes: interrompu || !demande.listes ? 0 : LISTES,
        dossiers: interrompu || !demande.dossiers || repris ? 0 : 3,
        cheminsRamenes: 0,
        fichiersGardes: 0,
        interrompu,
        avertissements: interrompu ? [] : ['3 chemins de fichiers de ComicRack ne correspondent à aucun dossier surveillé (gardés tels quels).'],
        nbLivresDestination: interrompu ? 1200 : LIVRES,
        doublonsRestants: 0,
        resume: '',
        bascule: false,
      };
      r.resume =
        `${LIVRES} livres de ComicRack : ${r.ajoutes} ajoutés, ${r.remplaces} remplacés (déjà ajoutés par l'analyse des dossiers), ${r.misAJour} mis à jour ; ` +
        `${r.listes} listes ; ${r.dossiers} dossiers ajoutés.` +
        (interrompu ? " Interrompu : les lots déjà écrits sont gardés, ComicRust n'a pas changé de bibliothèque. Relance le transfert pour le terminer." : '');
      if (!interrompu) {
        dejaTransfere = true;
        if (demande.basculer) {
          r.etat = ctx.basculer();
          r.bascule = true;
        }
      }
      return r;
    } finally {
      enCours = false;
    }
  }

  /** Commande du contrat (`apercu_transfert`, `lancer_transfert`, `annuler_transfert`). */
  async function appeler(commande: string, args: Record<string, unknown>): Promise<unknown> {
    switch (commande) {
      case 'apercu_transfert': return apercu();
      case 'lancer_transfert': return lancer(args.demande as DemandeTransfert);
      case 'annuler_transfert': annule = true; return null;
      default: return undefined;
    }
  }
  return { appeler };
}
