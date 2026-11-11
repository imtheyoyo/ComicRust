// MODE SIMULÉ (étape 3) : faux backend de l'édition (docs/tauri/ipc-edition.md), utilisé seulement
// hors de Tauri. Il imite les réponses du contrat (fiches, valeurs multiples, validation,
// suggestions, garde-fou, progression, bilans, aperçu du renommage) sur les livres factices de
// simule.ts. La logique réelle est dans le backend ; ceci ne sert qu'à développer et tester le front.
import type {
  BilanOperation, ChampFiche, ChampModifie, DemandeModification, DemandeRenommage, ErreurChamp, FicheEdition,
  LigneRenommage, ModeBiblio, ModeleNom, Operation, Proposition, ReglagesEdition, ResultatLivre, ResultatRenommage,
  TypeChamp,
} from './api';
import { creerPagesSimulees } from './simulePages';

/** Livre factice tel que simule.ts le garde (champs modifiables par l'édition). */
export interface LivreSimule {
  id: string; titre: string; serie?: string; numero?: string; editeur?: string; annee?: number; mois: number;
  genre: string; scenariste: string; dessinateur: string; resume: string; note: number; pages: number;
  fichier: string; fichierPresent: boolean; urlCouverture: string; pageCourante: number; lu: boolean;
}

export interface ContexteSimule {
  livres: LivreSimule[];
  mode: () => ModeBiblio;
  emettre: (nom: string, charge: unknown) => void;
  /** Les fiches ont changé : l'ordre en cache de lister_livres est à recalculer. */
  invalider: () => void;
  /** Étape 6 : raison d'impossibilité d'écrire quand la base est injoignable (`undefined` : en ligne). */
  horsLigne: () => string | undefined;
  pause: (ms: number) => Promise<unknown>;
}

const SEUIL = 100;

// Catalogue des champs (libellés et types de comicrust-core/src/edit/catalog.rs).
const CATALOGUE: Array<[string, string, TypeChamp, boolean]> = [
  ['Series', 'Série', 'texte', true], ['Number', 'Numéro', 'texte', true], ['Count', 'Nombre de numéros', 'entier', true],
  ['Volume', 'Volume', 'entier', true], ['Title', 'Titre', 'texte', true], ['AlternateSeries', 'Série alternative', 'texte', true],
  ['AlternateNumber', 'Numéro alternatif', 'texte', true], ['AlternateCount', 'Nombre de numéros (série alternative)', 'entier', true],
  ['Year', 'Année', 'entier', true], ['Month', 'Mois', 'entier', true], ['Day', 'Jour', 'entier', true],
  ['Writer', 'Scénariste', 'liste', true], ['Penciller', 'Dessinateur', 'liste', true], ['Inker', 'Encreur', 'liste', true],
  ['Colorist', 'Coloriste', 'liste', true], ['Letterer', 'Lettreur', 'liste', true],
  ['CoverArtist', 'Dessinateur de couverture', 'liste', true], ['Editor', 'Directeur de collection', 'liste', true],
  ['Translator', 'Traducteur', 'liste', true], ['Rating', 'Note', 'note', false],
  ['Publisher', 'Éditeur', 'texte', true], ['Imprint', 'Label', 'texte', true], ['Genre', 'Genre', 'liste', true],
  ['Format', 'Format', 'texte', true], ['AgeRating', 'Public', 'texte', true], ['LanguageISO', 'Langue', 'langue', true],
  ['BlackAndWhite', 'Noir et blanc', 'ouiNon', true], ['Manga', 'Manga', 'manga', true],
  ['SeriesComplete', 'Série complète', 'ouiNon', false], ['StoryArc', 'Arc narratif', 'texte', true],
  ['SeriesGroup', 'Groupe de séries', 'texte', true], ['Tags', 'Étiquettes', 'liste', true],
  ['CommunityRating', 'Note de la communauté', 'note', true], ['Web', 'Site web', 'texte', true],
  ['PreferredFrontCover', 'Couverture préférée', 'entier', true], ['EnableProposed', 'Utiliser les valeurs proposées', 'booleen', false],
  ['Characters', 'Personnages', 'liste', true], ['Teams', 'Équipes', 'liste', true],
  ['MainCharacterOrTeam', 'Personnage ou équipe principal', 'texte', true], ['Locations', 'Lieux', 'liste', true],
  ['ScanInformation', 'Informations de numérisation', 'texteLong', true], ['Summary', 'Résumé', 'texteLong', true],
  ['Notes', 'Notes', 'texteLong', true], ['Review', 'Critique', 'texteLong', true],
  ['BookStore', 'Magasin', 'texte', false], ['BookOwner', 'Propriétaire', 'texte', false], ['BookAge', 'Âge du livre', 'texte', false],
  ['BookCondition', 'État', 'texte', false], ['BookCollectionStatus', 'Statut dans la collection', 'texte', false],
  ['BookPrice', 'Prix', 'prix', false], ['BookLocation', 'Emplacement', 'texte', false], ['ISBN', 'ISBN', 'texte', false],
  ['Released', 'Date de parution', 'date', false], ['Checked', 'Coché', 'booleen', false],
  ['BookNotes', 'Notes sur le livre', 'texteLong', false],
];
const ONGLETS: Array<[string, string[]]> = [
  ['Résumé', ['Series', 'Number', 'Count', 'Volume', 'Title', 'AlternateSeries', 'AlternateNumber', 'AlternateCount', 'Year', 'Month', 'Day', 'Writer', 'Penciller', 'Inker', 'Colorist', 'Letterer', 'CoverArtist', 'Editor', 'Translator', 'Rating']],
  ['Détails', ['Publisher', 'Imprint', 'Genre', 'Format', 'AgeRating', 'LanguageISO', 'BlackAndWhite', 'Manga', 'SeriesComplete', 'StoryArc', 'SeriesGroup', 'Tags', 'CommunityRating', 'Web', 'PreferredFrontCover', 'EnableProposed']],
  ['Intrigue et notes', ['Characters', 'Teams', 'MainCharacterOrTeam', 'Locations', 'ScanInformation', 'Summary', 'Notes', 'Review']],
  ['Catalogue', ['BookStore', 'BookOwner', 'BookAge', 'BookCondition', 'BookCollectionStatus', 'BookPrice', 'BookLocation', 'ISBN', 'Released', 'Checked', 'BookNotes']],
];
const BORNES: Record<string, [number, number]> = {
  Count: [0, 1_000_000], Volume: [0, 1_000_000], AlternateCount: [0, 1_000_000], Year: [0, 9999], Month: [1, 12],
  Day: [1, 31], PreferredFrontCover: [0, 1_000_000],
};
const CHOIX: Partial<Record<TypeChamp, Array<{ valeur: string; libelle: string }>>> = {
  ouiNon: [{ valeur: '', libelle: 'Inconnu' }, { valeur: 'No', libelle: 'Non' }, { valeur: 'Yes', libelle: 'Oui' }],
  manga: [
    { valeur: '', libelle: 'Inconnu' }, { valeur: 'No', libelle: 'Non' }, { valeur: 'Yes', libelle: 'Oui' },
    { valeur: 'YesAndRightToLeft', libelle: 'Oui, de droite à gauche' },
  ],
  booleen: [{ valeur: 'Oui', libelle: 'Oui' }, { valeur: 'Non', libelle: 'Non' }],
};
const USUELLES: Record<string, string[]> = {
  AgeRating: ['Tout public', 'Adolescents', 'Adultes', 'Everyone', 'Teen', 'Mature 17+'],
  Format: ['Album', 'Intégrale', 'One-shot', 'Comic', 'Graphic Novel', 'Trade Paperback'],
  LanguageISO: ['fr', 'en', 'de', 'es', 'it', 'ja'],
};
const def = (nom: string) => CATALOGUE.find((c) => c[0] === nom);

export function creerEditionSimulee(ctx: ContexteSimule) {
  const { livres } = ctx;
  const parId = (id: string) => livres[Number(id) - 1];
  /** Champs hors des données du livre factice (et valeurs personnalisées), par livre. */
  const autres = new Map<string, Record<string, string>>();
  const extra = (id: string) => {
    let e = autres.get(id);
    if (!e) {
      const n = Number(id);
      e = { LanguageISO: 'fr', Format: n % 4 === 0 ? 'Intégrale' : 'Album', EnableProposed: 'Oui', Checked: 'Oui' };
      if (n % 3 === 0) e['CustomValue:Source'] = 'Bedetheque';
      if (n % 7 === 0) e['CustomValue:Lu en'] = String(2020 + (n % 6));
      autres.set(id, e);
    }
    return e;
  };
  let reglages: ReglagesEdition = { ecritureComicrack: false, ecrireDansFichier: false, rangerOriginal: false };
  let enCours = false;
  let annule = false;
  /** Fiches modifiées depuis la dernière écriture du fichier (ComicInfoIsDirty). */
  const aEcrire = new Set<string>();

  const cbz = (l: LivreSimule) => /\.(cbz|zip)$/i.test(l.fichier);
  const lectureSeule = () =>
    ctx.horsLigne() ??
    (ctx.mode() === 'comicrack' && !reglages.ecritureComicrack
      ? "Bibliothèque ComicRack en lecture seule : les fiches ne sont pas modifiées. Pour les modifier, cochez « Lecture et écriture » dans Réglages (de préférence ComicRack fermé)."
      : undefined);
  /** Onglet Pages (étape 4) : simulePages.ts. */
  const pagesSim = creerPagesSimulees({ livres, lectureSeule, cbz, pause: ctx.pause });

  function lire(l: LivreSimule, nom: string): string {
    switch (nom) {
      case 'Series': return l.serie ?? '';
      case 'Number': return l.numero ?? '';
      case 'Title': return l.titre;
      case 'Publisher': return l.editeur ?? '';
      case 'Year': return l.annee !== undefined ? String(l.annee) : '';
      case 'Month': return String(l.mois);
      case 'Writer': return l.scenariste;
      case 'Penciller': return l.dessinateur;
      case 'Genre': return l.genre;
      case 'Summary': return l.resume;
      case 'Rating': return l.note ? String(l.note).replace('.', ',') : '';
      default: return extra(l.id)[nom] ?? '';
    }
  }

  function ecrire(l: LivreSimule, nom: string, v: string): void {
    const n = v === '' ? undefined : Number(v.replace(',', '.'));
    switch (nom) {
      case 'Series': l.serie = v || undefined; break;
      case 'Number': l.numero = v || undefined; break;
      case 'Title': l.titre = v; break;
      case 'Publisher': l.editeur = v || undefined; break;
      case 'Year': l.annee = n; break;
      case 'Month': l.mois = n ?? 1; break;
      case 'Writer': l.scenariste = v; break;
      case 'Penciller': l.dessinateur = v; break;
      case 'Genre': l.genre = v; break;
      case 'Summary': l.resume = v; break;
      case 'Rating': l.note = n ?? 0; break;
      default: {
        const e = extra(l.id);
        if (v === '' && nom.startsWith('CustomValue:')) delete e[nom];
        else e[nom] = v;
      }
    }
  }

  function champ(nom: string, valeurs: string[]): ChampFiche {
    const perso = nom.startsWith('CustomValue:');
    const d = def(nom);
    const type: TypeChamp = perso ? 'texte' : d![2];
    const multiple = valeurs.some((v) => v !== valeurs[0]);
    const c: ChampFiche = {
      nom, libelle: perso ? nom.slice('CustomValue:'.length) : d![1], type,
      valeur: multiple ? '' : valeurs[0] ?? '', multiple,
      suggestions: !perso && ['texte', 'liste', 'langue'].includes(type) && nom !== 'Title' && nom !== 'Web',
      dansComicInfo: perso ? false : d![3],
    };
    if (CHOIX[type]) c.choix = CHOIX[type];
    if (BORNES[nom]) [c.min, c.max] = BORNES[nom];
    if (type === 'date') c.indication = 'JJ/MM/AAAA';
    else if (type === 'liste') c.indication = 'valeurs séparées par des virgules';
    else if (type === 'note') c.indication = 'de 0 à 5';
    return c;
  }

  function fiches(ids: string[]): FicheEdition {
    const ls = ids.map(parId).filter((l): l is LivreSimule => !!l);
    if (ls.length === 0) throw new Error('Aucun livre connu dans la sélection.');
    const cles = new Map<string, string>();
    for (const l of ls) {
      for (const k of Object.keys(extra(l.id))) {
        if (k.startsWith('CustomValue:') && !cles.has(k.toLowerCase())) cles.set(k.toLowerCase(), k);
      }
    }
    const perso = [...cles.values()].sort((a, b) => a.localeCompare(b, 'fr'));
    const noms = [...CATALOGUE.map((c) => c[0]), ...perso];
    return {
      ids: ls.map((l) => l.id),
      multiple: ls.length > 1,
      livres: ls.map((l) => ({
        id: l.id, titre: l.serie ? `${l.serie} #${l.numero}` : l.titre, fichier: l.fichier, urlCouverture: l.urlCouverture,
        pages: l.pages, tailleFichier: l.pages * 310_000,
        progression: l.lu ? 100 : Math.round((l.pageCourante / Math.max(1, l.pages - 1)) * 100),
        infosAEcrire: aEcrire.has(l.id), cbz: cbz(l),
      })),
      onglets: [...ONGLETS.map(([nom, champs]) => ({ nom, champs })), { nom: 'Personnalisé', champs: perso }],
      champs: noms.map((n) => champ(n, ls.map((l) => lire(l, n)))),
      lectureSeule: lectureSeule(),
      ecrireDansFichierPossible: ls.some(cbz),
      ecrireDansFichier: reglages.ecrireDansFichier,
      seuilConfirmation: SEUIL,
    };
  }

  function verifier(champs: ChampModifie[]): ErreurChamp[] {
    const erreurs: ErreurChamp[] = [];
    for (const { nom, valeur } of champs) {
      const v = valeur.trim();
      if (!v) continue;
      const d = def(nom);
      if (!d) continue;
      let m = '';
      if (d[2] === 'entier') {
        const [a, b] = BORNES[nom] ?? [0, 1e9];
        if (!/^-?\d+$/.test(v)) m = 'nombre entier attendu';
        else if (Number(v) < a || Number(v) > b) m = `entre ${a} et ${b}`;
      } else if (d[2] === 'note') {
        const x = Number(v.replace(',', '.'));
        if (!Number.isFinite(x) || x < 0 || x > 5) m = 'note de 0 à 5 attendue (décimales permises : 3,5)';
      } else if (d[2] === 'prix') {
        if (!Number.isFinite(Number(v.replace(',', '.'))) || Number(v.replace(',', '.')) < 0) m = 'prix attendu (12,50)';
      } else if (d[2] === 'date') {
        if (!/^\d{1,2}\/\d{1,2}\/\d{4}$/.test(v)) m = 'date attendue au format JJ/MM/AAAA';
      } else if (d[2] === 'langue') {
        if (!/^[a-z]{2,3}(-[A-Za-z]{2})?$/i.test(v)) m = 'code de langue ISO attendu (fr, en…)';
      }
      if (m) erreurs.push({ nom, message: m });
    }
    return erreurs;
  }

  function proposer(champ: string, texte: string, tout: boolean, max: number): Proposition[] {
    const d = def(champ);
    if (!d) return [];
    const liste = d[2] === 'liste';
    const i = liste ? texte.lastIndexOf(',') : -1;
    const avant = i >= 0 ? texte.slice(0, i + 1) + ' ' : '';
    const saisie = (i >= 0 ? texte.slice(i + 1) : texte).trim().toLowerCase();
    const vues = new Map<string, string>();
    for (const v of USUELLES[champ] ?? []) vues.set(v.toLowerCase(), v);
    for (const l of livres) {
      const brut = lire(l, champ);
      for (const v of liste ? brut.split(',') : [brut]) {
        const t = v.trim();
        if (t && !vues.has(t.toLowerCase())) vues.set(t.toLowerCase(), t);
      }
    }
    const toutes = [...vues.values()].sort((a, b) => a.localeCompare(b, 'fr', { numeric: true }));
    let res: string[];
    if (tout) res = toutes.filter((v) => v.toLowerCase().includes(saisie));
    else {
      if (!saisie || toutes.some((v) => v.toLowerCase() === saisie)) return [];
      const debut = toutes.filter((v) => v.toLowerCase().startsWith(saisie));
      const dedans = toutes.filter((v) => !v.toLowerCase().startsWith(saisie) && v.toLowerCase().includes(saisie));
      res = [...debut, ...dedans].slice(0, max);
    }
    return res.map((v) => ({ valeur: v, texte: (avant + v).trimStart() }));
  }

  // ───── Lots (progression, annulation, bilan) ─────

  async function lot(
    operation: Operation, ids: string[], confirme: boolean | undefined,
    traiter: (l: LivreSimule) => Omit<ResultatLivre, 'id' | 'titre'>,
  ): Promise<BilanOperation> {
    if (ids.length > SEUIL && !confirme) {
      const quoi = { modification: 'Modifier', ecritureInfos: 'Écrire les infos de', conversion: 'Convertir', renommage: 'Renommer' }[operation];
      return {
        operation, resume: 'Confirmation requise : rien n\'a été fait.', complet: false, interrompu: false, livres: [], rapport: '',
        confirmationRequise: { nbLivres: ids.length, seuil: SEUIL, message: `${quoi} ${ids.length} livres ? (au-delà de ${SEUIL} livres, ComicRust demande confirmation)` },
      };
    }
    if (enCours) throw new Error('Une opération est déjà en cours.');
    enCours = true;
    annule = false;
    const res: ResultatLivre[] = [];
    try {
      const ls = ids.map(parId).filter((l): l is LivreSimule => !!l);
      for (let k = 0; k < ls.length; k++) {
        if (annule) break;
        const l = ls[k];
        const titre = l.serie ? `${l.serie} #${l.numero}` : l.titre;
        ctx.emettre('edition-progression', { operation, fait: k, total: ls.length, livre: titre });
        await ctx.pause(ls.length > 20 ? 4 : 120);
        res.push({ id: l.id, titre, ...traiter(l) });
      }
      ctx.emettre('edition-progression', { operation, fait: res.length, total: ls.length, livre: '' });
    } finally {
      enCours = false;
    }
    const compte = (s: string) => res.filter((r) => r.statut === s).length;
    const verbe = { modification: 'modifié(s)', ecritureInfos: 'écrit(s)', conversion: 'converti(s)', renommage: 'renommé(s)' }[operation];
    const morceaux = [`${compte('ok')} livre(s) ${verbe}`];
    if (compte('inchange')) morceaux.push(`${compte('inchange')} déjà à jour`);
    if (compte('ignore')) morceaux.push(`${compte('ignore')} ignoré(s)`);
    if (compte('erreur')) morceaux.push(`${compte('erreur')} erreur(s)`);
    if (annule) morceaux.push('interrompu');
    const modifies = res.filter((r) => r.statut === 'ok').map((r) => r.id);
    if (modifies.length) {
      ctx.invalider();
      ctx.emettre('bibliotheque-modifiee', { ids: modifies });
    }
    const libelleStatut = { ok: 'OK', inchange: 'déjà à jour', ignore: 'ignoré', erreur: 'ERREUR' };
    return {
      operation, resume: morceaux.join(', '), complet: compte('erreur') === 0 && !annule, interrompu: annule, livres: res,
      rapport: res.map((r) => `${libelleStatut[r.statut]}\t${r.titre}${r.message ? ' — ' + r.message : ''}${r.fichier ? ' → ' + r.fichier : ''}`).join('\n'),
    };
  }

  async function modifier(d: DemandeModification): Promise<BilanOperation> {
    const ro = lectureSeule();
    if (ro) throw new Error(ro);
    const err = verifier(d.champs);
    if (err.length) throw new Error(`Valeur(s) refusée(s) : ${err.map((e) => def(e.nom)?.[1] ?? e.nom).join(', ')}`);
    reglages.ecrireDansFichier = d.ecrireDansFichier;
    if (d.pages && d.ids.length !== 1) throw new Error('Les pages ne se modifient que pour un seul livre.');
    return lot('modification', d.ids, d.confirme, (l) => {
      if (l.id === '13') return { statut: 'erreur', message: 'conflit : fiche modifiée dans ComicRack entre-temps, rien d\'écrasé' };
      const change = d.champs.filter((c) => lire(l, c.nom) !== c.valeur.trim());
      let pagesChangees = false;
      if (d.pages) {
        try {
          pagesChangees = pagesSim.enregistrer(l.id, d.pages);
        } catch (e) {
          return { statut: 'erreur', message: String((e as Error).message) };
        }
      }
      if (change.length === 0 && !pagesChangees) return { statut: 'inchange' };
      for (const c of change) ecrire(l, c.nom, c.valeur.trim());
      const ci = pagesChangees || change.some((c) => def(c.nom)?.[3]);
      if (ci) aEcrire.add(l.id);
      if (d.ecrireDansFichier) {
        if (cbz(l)) {
          aEcrire.delete(l.id);
          return { statut: 'ok', message: 'fiche modifiée, ComicInfo.xml écrit' };
        }
        return { statut: 'ok', message: 'fiche modifiée ; fichier non CBZ : infos non écrites' };
      }
      return { statut: 'ok' };
    });
  }

  // ───── Renommage ─────

  function valeurModele(l: LivreSimule, nom: string): string {
    const n = nom.trim().toLowerCase();
    if (n === 'filename') return (l.fichier.split('\\').pop() ?? '').replace(/\.[^.]+$/, '');
    if (n === 'language') return lire(l, 'LanguageISO');
    const c = CATALOGUE.find((x) => x[0].toLowerCase() === n);
    const v = c ? lire(l, c[0]) : '';
    // Fiche vide : valeur proposée d'après le nom du fichier (« Série 012.cbz »), comme ComicRack.
    if (!v && (n === 'series' || n === 'number')) {
      const m = /^(.*?)\s*(\d+)?$/.exec((l.fichier.split('\\').pop() ?? '').replace(/\.[^.]+$/, ''));
      return (n === 'series' ? m?.[1] : m?.[2]?.replace(/^0+(?=\d)/, '')) ?? '';
    }
    return v;
  }

  /** Développe `[ … ]`, `{champ}`, `{champ:000}` ; rend [texte, réussi]. */
  function developper(m: string, l: LivreSimule): [string, boolean] {
    let res = '';
    let ok = true;
    for (let i = 0; i < m.length; i++) {
      const c = m[i];
      if (c === '\\' && i + 1 < m.length) res += m[++i];
      else if (c === '[') {
        let prof = 1;
        let j = i + 1;
        while (j < m.length && prof) { if (m[j] === '[') prof++; else if (m[j] === ']') prof--; j++; }
        const [t, r] = developper(m.slice(i + 1, j - 1), l);
        if (r) res += t;
        i = j - 1;
      } else if (c === '{') {
        const j = m.indexOf('}', i);
        if (j < 0) { res += m.slice(i); break; }
        const [nom, fmt] = m.slice(i + 1, j).split(':');
        let v = valeurModele(l, nom);
        if (fmt && /^0+$/.test(fmt) && /^\d+$/.test(v)) v = v.padStart(fmt.length, '0');
        if (!v) ok = false;
        res += v.replace(/:/g, ' -').replace(/[\\|]/g, '-').replace(/[<>?*"]/g, '');
        i = j;
      } else res += c;
    }
    return [res, ok];
  }

  function renommer(d: DemandeRenommage): ResultatRenommage | Promise<ResultatRenommage> {
    const ro = lectureSeule();
    if (ro) throw new Error(ro);
    if (!d.modele.trim()) throw new Error('Le modèle de nom est vide.');
    const vus = new Map<string, string>();
    const lignes: LigneRenommage[] = d.ids.map(parId).filter((l): l is LivreSimule => !!l).map((l) => {
      const titre = l.serie ? `${l.serie} #${l.numero}` : l.titre;
      const dossier = l.fichier.slice(0, l.fichier.lastIndexOf('\\'));
      const ext = l.fichier.slice(l.fichier.lastIndexOf('.'));
      const rel = developper(d.modele, l)[0].split('/').map((s) => s.trim().replace(/[. ]+$/, '')).filter(Boolean).join('\\');
      const base = d.dossierCible?.trim() ? d.dossierCible.trim().replace(/[\\/]+$/, '') : dossier;
      const vers = rel ? `${base}\\${rel}${ext}` : '';
      const memeDossier = vers.slice(0, vers.lastIndexOf('\\')) === dossier;
      const ligne: LigneRenommage = {
        id: l.id, titre, de: l.fichier, vers, deNom: memeDossier ? l.fichier.split('\\').pop()! : l.fichier,
        versNom: memeDossier ? vers.split('\\').pop()! : vers, inchange: vers.toLowerCase() === l.fichier.toLowerCase(),
      };
      if (!l.fichierPresent) ligne.probleme = 'fichier introuvable';
      else if (!rel) ligne.probleme = 'nom vide d\'après le modèle';
      else if (vers.length > 259) ligne.probleme = 'chemin de plus de 259 caractères';
      else if (!ligne.inchange && vus.has(vers.toLowerCase())) ligne.probleme = `même destination que « ${vus.get(vers.toLowerCase())} »`;
      else if (!ligne.inchange && livres.some((x) => x !== l && x.fichier.toLowerCase() === vers.toLowerCase())) ligne.probleme = 'destination déjà existante';
      if (vers) vus.set(vers.toLowerCase(), titre);
      return ligne;
    });
    const aRenommer = lignes.filter((x) => !x.probleme && !x.inchange).length;
    if (d.apercu) return { lignes, aRenommer };
    reglages.modeleNom = d.modele;
    reglages.dossierCible = d.dossierCible;
    const parLigne = new Map(lignes.map((x) => [x.id, x]));
    return lot('renommage', d.ids, d.confirme, (l) => {
      const x = parLigne.get(l.id)!;
      if (x.probleme) return { statut: 'erreur', message: x.probleme };
      if (x.inchange) return { statut: 'inchange' };
      l.fichier = x.vers;
      return { statut: 'ok', fichier: x.vers };
    }).then((bilan) => ({ lignes, aRenommer, bilan }));
  }

  const MODELES: ModeleNom[] = [
    { nom: 'Série v2 #12 (2024)', modele: '{series}[ v{volume}][ #{number}][ ({year})]' },
    { nom: 'Export ComicRack', modele: '[{format} ][{series}][ {volume}][ #{number}][ ({year}[/{month}])]' },
    { nom: 'Éditeur / Série / Série #012', modele: '{publisher}/{series}/{series}[ #{number:000}]' },
  ];

  /** Commande d'édition, ou `undefined` si la commande n'est pas de l'étape 3. */
  function appelerEdition(commande: string, args: Record<string, unknown>): unknown {
    switch (commande) {
      case 'lire_reglages_edition':
        return { ...reglages };
      case 'enregistrer_reglages_edition':
        reglages = { ...reglages, ...(args.reglages as Partial<ReglagesEdition>) };
        return { ...reglages };
      case 'lire_fiches':
        return fiches(args.ids as string[]);
      case 'proposer_valeurs':
        return proposer(String(args.champ), String(args.texte ?? ''), !!args.tout, Number(args.max) || 8);
      case 'verifier_champs':
        return verifier(args.champs as ChampModifie[]);
      case 'appliquer_modifications':
        return modifier(args.demande as DemandeModification);
      case 'ajouter_valeur_perso': {
        const cle = String(args.cle ?? '').trim();
        if (!cle) throw new Error('Le nom de la valeur personnalisée est vide.');
        if (/[,=\p{Cc}]/u.test(cle)) throw new Error('Le nom ne peut contenir ni « , » ni « = ».');
        if ((args.existantes as string[]).some((e) => e.toLowerCase() === cle.toLowerCase())) {
          throw new Error(`La valeur personnalisée « ${cle} » existe déjà.`);
        }
        return champ('CustomValue:' + cle, ['']);
      }
      case 'ecrire_infos_fichiers':
        return lot('ecritureInfos', args.ids as string[], args.confirme as boolean | undefined, (l) => {
          if (!l.fichierPresent) return { statut: 'erreur', message: 'fichier introuvable' };
          if (!cbz(l)) return { statut: 'ignore', message: 'seuls les CBZ reçoivent les infos' };
          aEcrire.delete(l.id);
          return { statut: 'ok' };
        });
      case 'convertir_cbr_cbz': {
        const ranger = (args.rangerOriginal as boolean | undefined) ?? reglages.rangerOriginal;
        if (ranger && lectureSeule()) throw new Error('Bibliothèque en lecture seule : l\'original ne peut pas être rangé (il reste en place).');
        return lot('conversion', args.ids as string[], args.confirme as boolean | undefined, (l) => {
          if (!l.fichierPresent) return { statut: 'erreur', message: 'fichier introuvable' };
          if (!/\.(cbr|rar|cb7|7z|cbt|tar)$/i.test(l.fichier)) return { statut: 'ignore', message: 'déjà au format CBZ ou format non convertible' };
          const cible = l.fichier.replace(/\.[^.]+$/, '.cbz');
          if (!lectureSeule()) l.fichier = cible;
          return { statut: 'ok', fichier: cible, message: ranger ? 'original rangé dans « _originaux »' : undefined };
        });
      }
      case 'modeles_de_nom':
        return MODELES.map((m) => ({ ...m }));
      case 'renommer_deplacer':
        return renommer(args.demande as DemandeRenommage);
      case 'annuler_operation':
        annule = true;
        return null;
      case 'ouvrir_dossier_fichier':
        // Mode simulé : rien à ouvrir ; noté pour les tests (window.__dossiersOuverts).
        ((globalThis as unknown as { __dossiersOuverts?: string[] }).__dossiersOuverts ??= []).push(String(args.id));
        return null;
      default:
        return pagesSim.commande(commande, args);
    }
  };
  return Object.assign(appelerEdition, { pages: pagesSim });
}
