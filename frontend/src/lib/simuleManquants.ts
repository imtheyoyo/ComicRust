// MODE SIMULÉ (lot L3) : faux backend des livres manquants (docs/tauri/ipc-manquants.md), utilisé
// seulement hors de Tauri pour développer et tester l'interface. Aucun disque n'est lu : les livres
// dont `fichierPresent` est faux dans la bibliothèque simulée sont les livres manquants.
//
// Paramètres d'adresse :
// - `?manquants=volume` : les livres 3 et 4 sont sur un volume injoignable (ni marqués ni comptés,
//   « Disque inaccessible » à l'ouverture) ;
// - `?manquants=lectureSeule` : fiches non modifiables (raison affichée, commandes d'écriture refusées) ;
// - `?manquants=massif` : la vérification signale une disparition massive bloquée (rien n'est marqué).
// `window.__comicrustSimule.manquants` expose aux tests l'état (`volumeAbsent`, `retires`, appels).
import type {
  BilanOperationManquants, BilanVerification, DemandeRelocalisation, EtatManquants, LivreResume, PreparationRetrait,
  PropositionManquant, ResultatRecherche, SondeLivre,
} from './api';

interface LivreSimule extends LivreResume {
  fichier: string;
  fichierPresent: boolean;
}

export interface ContexteManquants {
  livres: LivreSimule[];
  /** Motif de lecture seule de la bibliothèque simulée (undefined : fiches modifiables). */
  lectureSeule: () => string | undefined;
  /** Motif propre aux fichiers des livres (`raisonFichiers`), sinon celui des fiches. */
  lectureSeuleFichiers?: () => string | undefined;
  emettre: (nom: string, charge: unknown) => void;
  pause: (ms: number) => Promise<unknown>;
  /** Le contenu des vues a changé (cache de l'ordre à recalculer). */
  invalider: () => void;
}

const RACINE_ABSENTE = '\\\\NAS\\comics';
const SEUIL_CONFIRMATION = 100;
const EXTENSION = /\.(cb[zr7tw]|zip|rar|7z|pdf|djvu)$/i;
const nomDe = (chemin: string) => chemin.split(/[\\/]/).pop() ?? chemin;

export function creerManquantsSimules(ctx: ContexteManquants) {
  const q = new URLSearchParams(location.search);
  const option = q.get('manquants');
  /** Livres marqués manquants. */
  const marques = new Set(ctx.livres.filter((l) => !l.fichierPresent).map((l) => l.id));
  /** Livres sur un volume injoignable (jamais marqués) ; modifiable par les tests. */
  const volumeAbsent = new Set<string>(option === 'volume' ? ['3', '4'] : []);
  /** Fiches retirées de la bibliothèque. */
  const retires = new Set<string>();
  const appels: Array<{ commande: string; args: Record<string, unknown> }> = [];
  let verification: { fait: number; total: number } | undefined;
  let recherche: { dossier: string; fichiers: number; comparaison: boolean } | undefined;
  let derniere: BilanVerification | undefined;
  let arret = false;

  const livre = (id: string) => ctx.livres.find((l) => l.id === id);
  const vivants = () => ctx.livres.filter((l) => !retires.has(l.id));
  const manquantsVivants = () => [...marques].filter((id) => !retires.has(id));
  const legende = (l: LivreResume) => [l.serie, l.numero ? `#${l.numero}` : '', l.titre].filter(Boolean).join(' ');

  function modifie(ids: string[], retire = 0) {
    ctx.invalider();
    ctx.emettre('bibliotheque-modifiee', { ids, origine: 'manquants', supprimes: retire });
  }
  function exigerEcriture() {
    const r = ctx.lectureSeule() ?? (option === 'lectureSeule' ? 'Bibliothèque en lecture seule (simulation).' : undefined);
    if (r) throw new Error(r);
  }

  const exposition = { volumeAbsent, retires, appels, marques };
  (globalThis as { __comicrustSimule?: { manquants?: typeof exposition } }).__comicrustSimule ??= {};
  (globalThis as unknown as { __comicrustSimule: { manquants?: typeof exposition } }).__comicrustSimule.manquants = exposition;

  /** Bilan d'une vérification des livres `cibles` (marque ou démarque, sauf volume injoignable). */
  function verifierLivres(cibles: LivreSimule[], marquer: boolean): BilanVerification {
    const injoignables = cibles.filter((l) => volumeAbsent.has(l.id));
    let manquants = 0;
    let presents = 0;
    const changes: string[] = [];
    const massif = option === 'massif';
    for (const l of cibles) {
      if (volumeAbsent.has(l.id)) continue;
      const absent = !l.fichierPresent;
      if (absent) manquants++;
      else presents++;
      if (!marquer || massif) continue;
      if (absent && !marques.has(l.id)) { marques.add(l.id); changes.push(l.id); }
      if (!absent && marques.has(l.id)) { marques.delete(l.id); changes.push(l.id); }
    }
    if (changes.length) modifie(changes);
    const bloques = massif ? [{ racine: 'D:\\', livres: cibles.length, disparus: manquants }] : [];
    return {
      resume: `${manquants.toLocaleString('fr-FR')} manquant${manquants > 1 ? 's' : ''}, ${presents.toLocaleString('fr-FR')} présent${presents > 1 ? 's' : ''}`,
      manquants,
      presents,
      injoignables: injoignables.length ? [{ racine: RACINE_ABSENTE, livres: injoignables.length }] : [],
      bloques,
      indetermines: 0,
      interrompu: false,
      concluant: injoignables.length === 0 && bloques.length === 0,
      ...(massif ? { avertissement: 'Disparition massive : aucune fiche marquée.' } : {}),
    };
  }

  function appeler(commande: string, args: Record<string, unknown>): unknown | Promise<unknown> | undefined {
    switch (commande) {
      case 'etat_manquants':
        return ((): EtatManquants => {
          const raison = ctx.lectureSeule() ?? (option === 'lectureSeule' ? 'Bibliothèque en lecture seule (simulation).' : undefined);
          return {
            nbManquants: manquantsVivants().length,
            ...(verification ? { verification: { ...verification } } : {}),
            ...(recherche ? { recherche: { ...recherche } } : {}),
            ...(derniere ? { derniere } : {}),
            ...(raison ? { raisonLectureSeule: raison } : {}),
            ...(raison ? { raisonFichiers: ctx.lectureSeuleFichiers?.() ?? raison } : {}),
          };
        })();
      case 'verifier_manquants':
        return (async () => {
          if (verification) throw new Error('Une vérification est déjà en cours.');
          appels.push({ commande, args });
          arret = false;
          const cibles = vivants();
          verification = { fait: 0, total: cibles.length };
          for (let k = 1; k <= 4 && !arret; k++) {
            await ctx.pause(120);
            verification = { fait: Math.round((cibles.length * k) / 4), total: cibles.length };
          }
          const interrompu = arret;
          const bilan = interrompu ? { ...verifierLivres([], false), interrompu: true, concluant: false } : verifierLivres(cibles, true);
          derniere = bilan;
          verification = undefined;
          return bilan;
        })();
      case 'arreter_manquants':
        arret = true;
        return null;
      case 'sonder_livre':
        return ((): SondeLivre => {
          const l = livre(String(args.id));
          if (!l || retires.has(l.id)) throw new Error('Livre introuvable.');
          appels.push({ commande, args });
          const base = { id: l.id, titre: legende(l), fichier: l.fichier };
          if (volumeAbsent.has(l.id)) {
            return { ...base, etat: 'volumeInjoignable', racine: RACINE_ABSENTE, avertissement: 'Le volume ne répond pas : livre non marqué.' };
          }
          if (!l.fichierPresent) {
            if (!marques.has(l.id)) { marques.add(l.id); modifie([l.id]); }
            return { ...base, etat: 'introuvable' };
          }
          if (marques.has(l.id)) { marques.delete(l.id); modifie([l.id]); }
          return { ...base, etat: 'present' };
        })();
      case 'proposer_fichier':
        return ((): PropositionManquant => {
          exigerEcriture();
          appels.push({ commande, args });
          const l = livre(String(args.id));
          const chemin = String(args.chemin ?? '').trim().replace(/^"|"$/g, '');
          if (!l) throw new Error('Livre introuvable.');
          if (!chemin) throw new Error('chemin vide');
          if (!EXTENSION.test(chemin) || /absent/i.test(chemin)) throw new Error(`fichier introuvable : ${chemin}`);
          const autre = ctx.livres.find((x) => x.id !== l.id && x.fichier.toLowerCase() === chemin.toLowerCase());
          return {
            id: l.id, legende: legende(l), ancien: l.fichier, nouveau: chemin,
            correspondance: nomDe(chemin).toLowerCase() === nomDe(l.fichier).toLowerCase() ? 'même nom de fichier' : 'nom différent : à vérifier',
            sure: nomDe(chemin).toLowerCase() === nomDe(l.fichier).toLowerCase(), autres: 0,
            ...(autre ? { doublon: `ce fichier est déjà celui de « ${legende(autre)} »` } : {}),
            coche: !autre,
          };
        })();
      case 'chercher_manquants':
        return (async (): Promise<ResultatRecherche> => {
          exigerEcriture();
          appels.push({ commande, args });
          const dossier = String(args.dossier ?? '').trim();
          if (!dossier) throw new Error('dossier vide');
          if (/inexistant/i.test(dossier)) throw new Error(`dossier introuvable : ${dossier}`);
          const ids = (args.ids as string[]) ?? [];
          const cibles = (ids.length === 0 ? manquantsVivants() : ids.length === 1 ? ids : ids.filter((i) => marques.has(i)))
            .map(livre).filter((l): l is LivreSimule => !!l);
          if (cibles.length === 0) {
            return { dossier, propositions: [], nonRetrouves: 0, fichiersExamines: 0, erreurs: [], interrompu: false, info: 'Aucun livre manquant à rechercher.' };
          }
          arret = false;
          for (let k = 1; k <= 4 && !arret; k++) {
            recherche = { dossier, fichiers: k * 250, comparaison: k === 4 };
            await ctx.pause(150);
          }
          recherche = undefined;
          if (arret) return { dossier, propositions: [], nonRetrouves: cibles.length, fichiersExamines: 1000, erreurs: [], interrompu: true };
          const trouves = /vide/i.test(dossier) ? [] : cibles.slice(0, 3);
          return {
            dossier,
            propositions: trouves.map((l, k) => ({
              id: l.id, legende: legende(l), ancien: l.fichier, nouveau: `${dossier.replace(/[\\/]+$/, '')}\\${nomDe(l.fichier)}`,
              correspondance: k === 0 ? 'même nom de fichier' : 'même nom (sans l\'extension)', sure: k === 0, autres: 0, coche: true,
            })),
            nonRetrouves: cibles.length - trouves.length,
            fichiersExamines: 1000,
            erreurs: [],
            interrompu: false,
            ...(trouves.length === 0 ? { info: `Aucun fichier retrouvé dans ${dossier}.` } : {}),
          };
        })();
      case 'appliquer_relocalisations':
        return ((): BilanOperationManquants => {
          exigerEcriture();
          appels.push({ commande, args });
          const demandes = (args.demandes as DemandeRelocalisation[]) ?? [];
          const ids: string[] = [];
          for (const d of demandes) {
            const l = livre(d.id);
            // Fiche changée depuis la proposition : ignorée (comme le backend).
            if (!l || l.fichier !== d.ancien) continue;
            l.fichier = d.nouveau;
            l.fichierPresent = true;
            marques.delete(l.id);
            ids.push(l.id);
          }
          if (ids.length) modifie(ids);
          const ignorees = demandes.length - ids.length;
          return { nb: ids.length, info: `✔ ${ids.length} fiche${ids.length > 1 ? 's' : ''} mise${ids.length > 1 ? 's' : ''} à jour${ignorees ? ` (${ignorees} ignorée${ignorees > 1 ? 's' : ''} : fiche modifiée entre-temps)` : ''}.` };
        })();
      case 'preparer_retrait':
        return (async (): Promise<PreparationRetrait> => {
          exigerEcriture();
          appels.push({ commande, args });
          const ids = [...new Set((args.ids as string[]) ?? [])].filter((i) => livre(i) && !retires.has(i));
          await ctx.pause(80);
          const cibles = ids.map(livre).filter((l): l is LivreSimule => !!l);
          const bilan = verifierLivres(cibles, false);
          return {
            ids,
            legendes: cibles.slice(0, 10).map(legende),
            autres: Math.max(0, cibles.length - 10),
            bilan,
            confirmationRequise: !bilan.concluant || bilan.presents > 0 || ids.length > SEUIL_CONFIRMATION,
            comicrack: false,
          };
        })();
      case 'retirer_livres':
        return ((): BilanOperationManquants => {
          exigerEcriture();
          appels.push({ commande, args });
          const ids = [...new Set((args.ids as string[]) ?? [])].filter((i) => livre(i) && !retires.has(i));
          const cibles = ids.map(livre).filter((l): l is LivreSimule => !!l);
          const bilan = verifierLivres(cibles, false);
          const exige = !bilan.concluant || bilan.presents > 0 || ids.length > SEUIL_CONFIRMATION;
          if (exige && args.confirme !== true) throw new Error('Confirmation exigée : des fichiers sont encore présents ou injoignables.');
          for (const i of ids) { retires.add(i); marques.delete(i); }
          if (ids.length) modifie(ids, ids.length);
          return { nb: ids.length, info: `✔ ${ids.length} livre${ids.length > 1 ? 's' : ''} retiré${ids.length > 1 ? 's' : ''} de la bibliothèque (les fichiers sont intacts).` };
        })();
    }
    return undefined;
  }

  return {
    appeler,
    /** Identifiants des livres de la liste « Livres manquants » (ordre de la bibliothèque). */
    membres: () => ctx.livres.filter((l) => marques.has(l.id) && !retires.has(l.id)).map((l) => l.id),
    est: (id: string) => marques.has(id),
    retire: (id: string) => retires.has(id),
    nbRetires: () => retires.size,
    injoignable: (id: string) => volumeAbsent.has(id),
  };
}
