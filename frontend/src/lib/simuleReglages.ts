// MODE SIMULÉ (lot L7) : faux backend de « À propos / Aide », de la description de la bibliothèque pour
// l'assistant rejouable, de « Relire » et de « Copier les livres » (docs/tauri/ipc-reglages-aide.md).
// Utilisé seulement hors de Tauri. Paramètres d'adresse :
// - `?pdfiumAbsent` : PDFium non chargé ;
// - `?relireErreur` : « Relire » échoue (analyse en cours) ;
// - `?baseSqliteAbsente` : la base SQLite de ComicRust n'existe pas (l'assistant le signale).
// `window.__comicrustSimule.reglages` expose les traces (relectures, rapports demandés, diagnostic).
import type { ConfigurationBiblio, ConnexionsMysql, EtatBiblio, InfosDiagnostic, RequeteLivres } from './api';

export const COMMANDES_REGLAGES = ['infos_diagnostic', 'texte_diagnostic', 'configuration_biblio', 'relire_bibliotheque', 'rapport_livres'];

export interface ContexteReglages {
  etat: () => EtatBiblio;
  connexions: () => ConnexionsMysql;
  /** Livres simulés (série, numéro, année) pour le rapport. */
  livres: () => Array<{ serie?: string; numero?: string; annee?: number }>;
}

export interface TracesReglages {
  relectures: number;
  rapports: Array<{ requete: RequeteLivres; titre: string }>;
}

export function creerReglagesSimules(ctx: ContexteReglages) {
  const q = () => new URLSearchParams(location.search);
  const traces: TracesReglages = { relectures: 0, rapports: [] };
  const g = globalThis as unknown as { __comicrustSimule?: Record<string, unknown> };
  g.__comicrustSimule ??= {};
  g.__comicrustSimule.reglages = traces;

  function infos(): InfosDiagnostic {
    const e = ctx.etat();
    const k = ctx.connexions();
    const absente = q().has('pdfiumAbsent');
    return {
      version: '0.0.0-simule',
      systeme: 'windows x86_64 (simulé)',
      mode: e.premierDemarrage ? undefined : e.mode,
      source: e.premierDemarrage ? '' : e.source,
      nbLivres: e.premierDemarrage ? undefined : e.nbLivres,
      dossierExe: 'C:\\ComicRust',
      dossierData: 'C:\\ComicRust\\data',
      dossierThemes: 'C:\\ComicRust\\data\\themes',
      dossierWebview: 'C:\\ComicRust\\data\\webview2',
      cheminIni: e.mode === 'comicrack' && e.source === 'xml' ? 'C:\\ComicRack\\ComicDb.xml' : undefined,
      baseSqlite: 'C:\\ComicRust\\data\\comicrust.db',
      baseSqlitePresente: !q().has('baseSqliteAbsente'),
      storeMysql: k.store === 'mysql',
      comicrackMysql: !!k.comicrackMysql,
      pdfium: absente
        ? { charge: false, erreur: 'PDFium indisponible : pdfium.dll introuvable' }
        : { charge: true, chemin: 'C:\\ComicRust\\pdfium.dll' },
      depot: 'https://github.com/imtheyoyo/ComicRust/',
    };
  }

  /** Même texte que `diagnostic::texte_diagnostic` (sans secret). */
  function texte(): string {
    const i = infos();
    return [
      `ComicRust (Tauri) v${i.version} — informations de diagnostic`,
      `Système : ${i.systeme}`,
      `Mode : ${i.mode === 'comicrack' ? 'lié à ComicRack' : i.mode === 'autonome' ? 'autonome' : 'aucune bibliothèque ouverte'}`,
      ...(i.nbLivres !== undefined ? [`Livres : ${i.nbLivres}`] : []),
      i.storeMysql
        ? 'Base de ComicRust : MySQL (paramètres non inclus)'
        : `Base de ComicRust : SQLite ${i.baseSqlite} (${i.baseSqlitePresente ? 'présente' : 'absente'})`,
      `Dossier data : ${i.dossierData}`,
      i.pdfium.charge ? `PDFium : chargé (${i.pdfium.chemin})` : `PDFium : non chargé — ${i.pdfium.erreur}`,
      `Dépôt : ${i.depot}`,
      '',
    ].join('\n');
  }

  function configuration(): ConfigurationBiblio {
    const e = ctx.etat();
    const k = ctx.connexions();
    return {
      mode: e.premierDemarrage ? undefined : e.mode,
      cheminIni: e.mode === 'comicrack' && e.source === 'xml' ? 'C:\\ComicRack\\ComicDb.xml' : undefined,
      storeMysql: k.store === 'mysql',
      comicrackMysql: !!k.comicrackMysql,
      baseSqlite: 'C:\\ComicRust\\data\\comicrust.db',
      baseSqlitePresente: !q().has('baseSqliteAbsente'),
      baseSqliteTaille: q().has('baseSqliteAbsente') ? undefined : 4_194_304,
    };
  }

  return {
    appeler(commande: string, args: Record<string, unknown>): unknown {
      switch (commande) {
        case 'infos_diagnostic':
          return infos();
        case 'texte_diagnostic':
          return texte();
        case 'configuration_biblio':
          return configuration();
        case 'relire_bibliotheque':
          if (q().has('relireErreur')) throw new Error('analyse en cours : attends sa fin avant de changer de bibliothèque');
          traces.relectures++;
          return ctx.etat();
        case 'rapport_livres': {
          const requete = args.requete as RequeteLivres;
          const titre = String(args.titre ?? '');
          traces.rapports.push({ requete, titre });
          const livres = ctx.livres();
          const lignes = livres.slice(0, 20).map((l, i) => `${l.serie ?? ''} #${l.numero ?? i + 1} (v1, ${l.annee ?? 0}) — D:\\BD\\${l.serie ?? 'Livre'} ${l.numero ?? i + 1}.cbz`);
          return `${titre.trim() || 'Bibliothèque'} — ${livres.length} livres (ComicRust v0.0.0-simule)\n\n${lignes.join('\n')}\n`;
        }
      }
      return undefined;
    },
  };
}
