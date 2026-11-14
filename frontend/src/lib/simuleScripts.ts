// MODE SIMULÉ (tauri-v0.9.0) : faux backend des scripts Python (docs/tauri/ipc-scripts.md), utilisé
// seulement hors de Tauri pour développer et tester l'interface. Aucun Python n'est lancé : chaque
// script simulé est une petite fonction asynchrone qui émet les mêmes événements que le backend
// (journal, progression, questions texte / choix / confirmation, formulaire, choix à vignettes,
// fin) et prépare des modifications à prévisualiser. Les identifiants des scripts sont ceux des
// points d'entrée (`fichier.py:fonction`) ; les réglages sont communs à tout le fichier.
//
// Paramètres d'adresse :
// - `?scripts=lectureSeule` : bibliothèque en lecture seule (aperçu visible, application refusée) ;
// - `?scripts=sansPython` : Python introuvable (erreur lisible dans la gestion) ;
// - `?scripts=vide` : aucun script.
// L'application des modifications prend un peu de temps (250 ms par ligne) : l'état « application
// en cours » (créneau occupé, Arrêter entre deux lignes) est ainsi visible et testable.
// `window.__comicrustScripts` expose aux tests les appels reçus (réponses, vignettes, application…).
import type {
  ApercuScript, BilanScript, ChooseSpecScript, DetailQuestionScript, EtatScripts, FormSpecScript, HookScript, InfoScript, LigneApercu,
} from './api';
import { SPEC_CONFIGURER_BD2 } from './simuleFormulairesReels';

/** Libellés français des champs (comme `libelle` du backend). */
const LIBELLES: Record<string, string> = {
  Title: 'Titre', Series: 'Série', Number: 'Numéro', Writer: 'Scénariste', Penciller: 'Dessinateur', Summary: 'Résumé',
  Fichier: 'Fichier', 'ComicInfo.xml': 'ComicInfo.xml',
};

export interface ContexteScripts {
  emettre: (nom: string, charge: unknown) => void;
  pause: (ms: number) => Promise<unknown>;
  /** Titre et série d'un livre simulé (undefined : inconnu). */
  livre: (id: string) => { titre: string; serie?: string; numero?: string; editeur?: string } | undefined;
  /** Motif de lecture seule de la bibliothèque simulée (mode lié sans écriture, hors ligne). */
  lectureSeule: () => string | undefined;
}

const PIEGE = '<img src=x onerror="window.__piege=1">';

/** Image PNG factice (couverture d'album, image de progression) en URL `data:image/png;base64`,
 *  seule forme d'image acceptée par le front (comme celles du backend ; le SVG est refusé). */
function imagePng(texte: string, teinte: number, l = 120, h = 170): string {
  const c = document.createElement('canvas');
  c.width = l;
  c.height = h;
  const g = c.getContext('2d');
  if (!g) return '';
  g.fillStyle = `hsl(${teinte} 55% 40%)`;
  g.fillRect(0, 0, l, h);
  g.fillStyle = `hsla(${(teinte + 150) % 360}, 70%, 62%, 0.85)`;
  g.beginPath();
  g.arc(l * 0.7, h * 0.3, l / 4, 0, 2 * Math.PI);
  g.fill();
  g.fillStyle = '#fff';
  g.font = 'bold 16px sans-serif';
  g.fillText(texte.slice(0, 14), 8, h - 12);
  return c.toDataURL('image/png');
}

interface Execution {
  runId: string;
  script: string;
  /** Nom du script (titre des questions). */
  nom: string;
  arret: boolean;
  /** Questions en attente de réponse : reqId → résolution. */
  attentes: Map<number, (v: unknown) => void>;
  lignes: LigneApercu[];
  conflits: string[];
  /** Valeurs demandées déjà identiques à la fiche. */
  inchanges: number;
  fini: boolean;
  applique: boolean;
  /** Arrêt de l'application demandé (fait entre deux lignes). */
  arretApplication: boolean;
  /** Choix ouvert : rangs dont la vignette a déjà été envoyée. */
  vignettes: Map<number, Set<number>>;
}

class Arret extends Error {}

export function creerScriptsSimules(ctx: ContexteScripts) {
  const q = new URLSearchParams(location.search);
  const variante = q.get('scripts') ?? '';
  const tests = {
    lancements: [] as Array<{ id: string; hook: string; ids: string[] }>,
    reponses: [] as Array<{ runId: string; reqId: number; valeur: unknown }>,
    vignettesVoulues: [] as number[][],
    appliques: [] as string[][],
    arrets: [] as string[],
    reglages: [] as Array<{ id: string; valeurs: Record<string, unknown> }>,
    editeur: [] as Array<{ id: string; livreId: string; champs: Record<string, string> }>,
  };
  (globalThis as { __comicrustScripts?: typeof tests }).__comicrustScripts = tests;

  let auto = false;
  let scripts: InfoScript[] = variante === 'vide' ? [] : [
    {
      id: 'nettoyage.py:nettoyer', nom: 'Nettoyage des titres', fichier: 'nettoyage.py', hooks: ['Books'], actif: true,
      description: 'Retire les espaces en trop et corrige la casse des titres.', auteur: 'ComicRust',
    },
    {
      id: 'bedetheque/bd2.py:scraper', nom: 'Bedetheque Scraper 2', fichier: 'bedetheque/bd2.py', hooks: ['Books', 'Editor'], actif: true,
      description: 'Remplit les fiches depuis bedetheque.com (série, album, auteurs, résumé).',
      image: imagePng('BD2', 30, 32, 32), reglages: 'BD2_CONFIG',
    },
    {
      // #@Key : identifiant libre (opaque pour le front).
      id: 'BD2_CONFIG', nom: 'Configurer BD2', fichier: 'bedetheque/bd2.py', hooks: ['ConfigScript'], actif: true,
      description: 'Options de Bedetheque Scraper 2.', reglages: 'BD2_CONFIG',
    },
    {
      // Vraie fenêtre de réglages du script d'exemple (spec réelle du backend, tauri-v0.9.1).
      id: 'bedetheque/bd2.py:ConfigureBD2', nom: 'Configurer BD2 (fenêtre complète)', fichier: 'bedetheque/bd2.py', hooks: ['Library'], actif: true,
      description: 'Réglages de Bedetheque Scraper 2 (Préférences, Données, Logging) : formulaire réel du script d\'exemple.',
    },
    {
      id: 'rangement.py:ranger', nom: 'Rangement de la bibliothèque', fichier: 'rangement.py', hooks: ['Library'], actif: true,
      description: 'Renomme les fichiers d\'après « Série - Numéro - Titre ».',
    },
    {
      id: 'casse.py:lancer', nom: `Script en erreur ${PIEGE}`, fichier: 'casse.py', hooks: ['Books'], actif: true,
      description: 'Échoue exprès (exception Python).',
    },
    {
      id: 'nouveaux.py:marquer', nom: 'Marquer les nouveaux livres', fichier: 'nouveaux.py', hooks: ['NewBooks', 'BookOpened'], actif: false,
      description: 'Automatisme : ajoute la valeur personnalisée « nouveau ».',
    },
  ];
  const avertissements = variante === 'vide' ? [] : ['vieux.py : ligne 3 : #@Hook inconnu « Bookz » (ignoré)'];
  /** Réglages par fichier (communs à tous les points d'entrée du fichier). */
  const reglages: Record<string, Record<string, unknown>> = {
    'bedetheque/bd2.py': { langue: 'fr', delai: 2, couverture: true, cleApi: 'secret-simule', dossier: 'D:\\BD\\Couvertures' },
  };
  const reglagesDe = (id: string): Record<string, unknown> | undefined => {
    const s = scripts.find((x) => x.id === id);
    if (!s) throw new Error(`script inconnu : ${id}`);
    return reglages[s.fichier];
  };
  /** Comme le backend : secret défini → « •••• », non défini → vide ; clés secrètes listées dans
   *  `$secrets` (le champ reste masqué même vide). */
  const masquer = (v: Record<string, unknown> | undefined) => {
    const m: Record<string, unknown> = Object.fromEntries(Object.entries(v ?? {}).map(([k, x]) => [k, secrets.has(k) ? (x ? '••••' : '') : x]));
    if (v && Object.keys(v).length) {
      for (const k of secrets) m[k] ??= '';
      m['$secrets'] = [...secrets].sort();
    }
    return m;
  };
  /** cleApi : défini ; jeton : secret jamais défini (champ vide mais masqué). */
  const secrets = new Set(['cleApi', 'jeton']);

  const execs = new Map<string, Execution>();
  let numero = 0;
  let reqSuivant = 1;
  let interactif: Execution | null = null;
  /** Exécution en cours d'application (executionEnCours). */
  let enApplication: string | null = null;

  const etat = (): EtatScripts => {
    const motif = variante === 'lectureSeule' ? 'Bibliothèque ComicRack en lecture seule (cocher « Lecture et écriture » dans Réglages).' : ctx.lectureSeule();
    return {
      python: variante === 'sansPython'
        ? { erreur: 'Python introuvable : ni python-embed\\python.exe à côté de ComicRust, ni « py » / « python » dans le PATH.' }
        : { chemin: 'C:\\ComicRust\\python-embed\\python.exe (3.12.7)' },
      scripts: scripts.map((s) => ({ ...s })),
      avertissements: [...avertissements],
      auto,
      ecritureAutorisee: !motif,
      ...(motif ? { motifLectureSeule: motif } : {}),
      dossier: 'C:\\ComicRust\\scripts',
      ...(enApplication ? { executionEnCours: enApplication } : interactif && !interactif.fini ? { executionEnCours: interactif.runId } : {}),
    };
  };

  // ───── Outils d'exécution ─────

  const journal = (e: Execution, genre: string, texte: string) => ctx.emettre('script:journal', { runId: e.runId, genre, texte });
  const progression = (e: Execution, p: Record<string, unknown>) => ctx.emettre('script:progression', { runId: e.runId, ...p });
  async function attendre(e: Execution, ms: number) {
    await ctx.pause(ms);
    if (e.arret) throw new Arret();
  }
  function demander(e: Execution, nom: string, charge: Record<string, unknown>): Promise<unknown> {
    if (e.arret) return Promise.reject(new Arret());
    const reqId = reqSuivant++;
    return new Promise((resolve, reject) => {
      e.attentes.set(reqId, (v) => (e.arret ? reject(new Arret()) : resolve(v)));
      ctx.emettre(nom, { runId: e.runId, reqId, ...charge });
    });
  }
  /** Question `ask` (titre = nom du script). Comme le cœur : une réponse invalide ou `null`
   *  rend la valeur par défaut (texte → chaîne, choix → un des choix, confirmation → booléen). */
  async function ask(e: Execution, texte: string, q: Partial<DetailQuestionScript> & { type: DetailQuestionScript['type'] }): Promise<unknown> {
    const secret = q.secret === true;
    const question: DetailQuestionScript = { type: q.type, defaut: secret ? null : (q.defaut ?? null), choix: q.choix ?? [], secret };
    const defaut = q.defaut ?? (q.type === 'confirmation' ? false : q.type === 'texte' ? '' : null);
    const v = await demander(e, 'script:question', { genre: 'ask', titre: e.nom, texte, question });
    const valide = (q.type === 'texte' && typeof v === 'string') || (q.type === 'choix' && typeof v === 'string' && question.choix.includes(v))
      || (q.type === 'confirmation' && typeof v === 'boolean');
    return valide ? v : defaut;
  }
  const confirmer = (e: Execution, texte: string, defaut = true) => ask(e, texte, { type: 'confirmation', defaut });
  const message = (e: Execution, texte: string) => demander(e, 'script:question', { genre: 'message', titre: e.nom, texte });
  const formulaire = (e: Execution, spec: FormSpecScript) => demander(e, 'script:formulaire', { spec });
  function choix(e: Execution, spec: ChooseSpecScript, vignette: (rang: number) => string): Promise<unknown> {
    const p = demander(e, 'script:choix', { spec });
    const reqId = reqSuivant - 1;
    e.vignettes.set(reqId, new Set());
    fournisseurs.set(`${e.runId}#${reqId}`, vignette);
    return p.finally(() => fournisseurs.delete(`${e.runId}#${reqId}`));
  }
  const fournisseurs = new Map<string, (rang: number) => string>();

  function modif(e: Execution, l: Omit<LigneApercu, 'cle' | 'libelle'>) {
    if (l.genre === 'fiche' && l.avant === l.apres) { e.inchanges++; return; }
    e.lignes.push({ ...l, libelle: LIBELLES[l.champ] ?? l.champ, cle: `${e.runId}-${e.lignes.length}` });
  }

  function terminer(e: Execution, etat: 'termine' | 'annule' | 'erreur', erreur?: string, champs?: Record<string, unknown>) {
    e.fini = true;
    for (const r of e.attentes.values()) r(null);
    e.attentes.clear();
    if (interactif === e) interactif = null;
    ctx.emettre('script:fin', { runId: e.runId, etat, nbModifs: e.lignes.length, ...(erreur ? { erreur } : {}), ...(champs ? { champs } : {}) });
  }

  // ───── Scripts simulés ─────

  async function nettoyage(e: Execution, ids: string[]) {
    journal(e, 'systeme', `nettoyage.py : nettoyer() sur ${ids.length} livre(s)`);
    for (let i = 0; i < ids.length; i++) {
      const l = ctx.livre(ids[i]);
      progression(e, { fraction: (i + 1) / ids.length, texte: `Livre ${i + 1} / ${ids.length}`, detail: l?.titre ?? ids[i] });
      await attendre(e, 120);
      if (!l) continue;
      const propre = l.titre.replace(/\s*\(\d+\)$/, '').toUpperCase().slice(0, 1) + l.titre.replace(/\s*\(\d+\)$/, '').slice(1).toLowerCase();
      if (propre !== l.titre) modif(e, { livreId: ids[i], titre: l.titre, champ: 'Title', avant: l.titre, apres: propre, genre: 'fiche' });
      journal(e, 'sortie', `${l.titre} → ${propre}`);
    }
    if (ids[0]) modif(e, { livreId: ids[0], titre: ctx.livre(ids[0])?.titre ?? ids[0], champ: 'ComicInfo.xml', avant: '', apres: 'écrire les infos dans le fichier', genre: 'comicinfo' });
    journal(e, 'info', 'Terminé.');
    terminer(e, 'termine');
  }

  const SPEC_OPTIONS = (couverture: string, valeurs?: Record<string, unknown>): FormSpecScript => ({
    title: 'Bedetheque Scraper 2 — options',
    message: 'Choisis ce qui doit être rempli. Les champs déjà remplis ne sont pas remplacés sauf si « Tout écraser » est coché.',
    buttons: ['Rechercher', 'Annuler'],
    width: 640,
    values: valeurs ?? { langue: 'fr', delai: 2 },
    controls: [
      {
        type: 'tabs', tabs: [
          {
            title: 'Général', controls: [
              { type: 'columns', widths: [140, null], columns: [
                [{ type: 'image', data: couverture, caption: 'Couverture actuelle', max_width: 120, max_height: 170 }],
                [
                  { type: 'text', key: 'recherche', label: 'Série recherchée', required: true, default: 'Astérix', placeholder: 'Nom de la série ou lien',
                    paste: true, hints: [['^https?://www\\.bedetheque\\.com/', '✔ Lien Bedetheque'], ['^\\d+$', '✔ Numéro d\'album']], hint_none: 'Recherche par nom',
                    help: 'Nom de la série, ou lien direct d\'un album de bedetheque.com' },
                  { type: 'radio', key: 'langue', label: 'Langue', choices: [['fr', 'Français'], ['en', 'Anglais'], ['nl', 'Néerlandais']], horizontal: true },
                  { type: 'number', key: 'delai', label: 'Pause entre albums (s)', min: 0, max: 30, step: 0.5, decimals: 1 },
                  { type: 'choice', key: 'editeur', label: 'Éditeur préféré', choices: ['Dargaud', 'Dupuis', 'Casterman', 'Le Lombard'], editable: true, default: 'Dargaud' },
                ],
              ] },
              { type: 'checkbox', key: 'tout', label: 'Tout écraser (remplacer les champs déjà remplis)', default: false },
              { type: 'label', text: 'Attention : « Tout écraser » remplace aussi tes corrections manuelles.', style: 'warning', enabled_if: 'tout' },
              { type: 'checkbox', key: 'auto', label: 'Choix automatique', default: false, help: 'Choisir seul le premier résultat après un délai' },
              { type: 'number', key: 'attente', label: 'Délai du choix automatique (s)', default: 5, min: 1, max: 60, enabled_if: 'auto' },
            ],
          },
          {
            title: 'Champs', controls: [
              { type: 'multichoice', key: 'champs', label: 'Champs à remplir', required: true, height: 140,
                choices: [['Series', 'Série'], ['Number', 'Numéro'], ['Title', 'Titre'], ['Writer', 'Scénariste'], ['Penciller', 'Dessinateur'], ['Summary', 'Résumé'], ['Web', 'Lien Web']],
                default: ['Series', 'Number', 'Title', 'Writer'] },
              { type: 'list_edit', key: 'exclusions', label: 'Séries à ignorer', default: ['Tintin'], choices: ['Astérix', 'Spirou', 'XIII'], unique: true },
              { type: 'table', key: 'substitutions', label: 'Substitutions', columns: [
                { title: 'Chercher' }, { title: 'Remplacer par' }, { title: 'Casse', type: 'checkbox' }, { title: 'Champ', type: 'choice', choices: ['Titre', 'Série', 'Tous'] },
              ], default: [['Tome', 'T.', false, 'Titre']] },
            ],
          },
          {
            title: 'Avancé', controls: [
              { type: 'secret', key: 'cleApi', label: 'Clé d\'accès', placeholder: 'laisser vide pour garder la clé actuelle' },
              { type: 'textarea', key: 'modele', label: 'Modèle du résumé', rows: 3, default: '{resume}\n— source : Bedetheque',
                inserts: [{ group: 'Album', items: [['{resume}', 'Résumé'], ['{titre}', 'Titre']] }, { group: 'Série', items: [['{serie}', 'Série']] }] },
              { type: 'folder', key: 'dossier', label: 'Dossier des couvertures', default: 'D:\\BD\\Couvertures' },
              { type: 'group', title: 'Connexion', controls: [
                { type: 'label', text: `Libellé piège affiché tel quel : ${PIEGE}`, style: 'small' },
                { type: 'button', key: 'tester', label: 'Tester la connexion' },
              ] },
              { type: 'separator' },
              { type: 'label', text: 'Réglages lus depuis App.Config (ComicRack).', style: 'weak' },
            ],
          },
        ],
      },
    ],
  });

  async function bedetheque(e: Execution, ids: string[], editeur: boolean) {
    journal(e, 'systeme', `bedetheque/bd2.py : scraper() sur ${ids.length} livre(s)`);
    const premier = ctx.livre(ids[0] ?? '');
    let options: Record<string, unknown> | null = null;
    if (!editeur) {
      const oui = await confirmer(e, `Rechercher ${ids.length} album(s) sur bedetheque.com ?\n(une pause est faite entre deux albums)`);
      if (oui !== true) { journal(e, 'info', 'Abandon demandé par l\'utilisateur.'); terminer(e, 'annule'); return; }
      let couverture = imagePng(premier?.serie ?? 'BD', 210);
      for (;;) {
        // Formulaire dynamique : rouvert avec les valeurs saisies après « Tester la connexion ».
        options = (await formulaire(e, SPEC_OPTIONS(couverture, options ?? undefined))) as Record<string, unknown> | null;
        if (!options) { journal(e, 'info', 'Formulaire annulé.'); terminer(e, 'annule'); return; }
        if (options['$bouton'] !== 'tester') break;
        journal(e, 'info', 'Connexion à bedetheque.com : OK (simulée).');
        couverture = imagePng('Testé ✔', 120);
      }
      journal(e, 'sortie', `Options : ${JSON.stringify(options)}`);
    }
    const series = ['Astérix', 'Astérix (édition spéciale)', 'Astérix chez les Pictes', 'Les Aventures d\'Astérix', 'Astérix et ses amis'];
    const rows: string[][] = [];
    for (let i = 0; i < 30; i++) {
      rows.push([`${series[i % series.length]} — T${i + 1}`, String(1959 + i), i === 3 ? PIEGE : ['Dargaud', 'Hachette', 'Albert René'][i % 3], `${44 + (i % 5) * 4} pages`]);
    }
    const choisi = await choix(e, {
      title: 'Choisir l\'album', message: `Résultats pour « ${premier?.serie ?? 'Astérix'} » (${rows.length}). Double-clic pour choisir.`,
      columns: ['Album', 'Année', 'Éditeur', 'Pages'], rows, on_demand: rows.map((_, i) => i), filter: true, thumb_size: 72, layout: 'cards', default: 0,
    }, (r) => imagePng(`T${r + 1}`, (r * 37) % 360));
    if (choisi === null || choisi === undefined) { journal(e, 'info', 'Aucun album choisi.'); terminer(e, 'annule'); return; }
    const r = Array.isArray(choisi) ? Number(choisi[0]) : Number(choisi);
    const album = rows[r] ?? rows[0];
    journal(e, 'info', `Album choisi : ${album[0]}`);
    if (editeur) {
      await attendre(e, 200);
      terminer(e, 'termine', undefined, {
        Series: 'Astérix', Number: String(r + 1), Title: album[0].split(' — ')[0], Writer: 'René Goscinny', Penciller: 'Albert Uderzo',
        Summary: `Résumé simulé ${PIEGE}`, Inconnu: 'champ inconnu ignoré', $ecrireDansFichier: true,
      });
      return;
    }
    for (let i = 0; i < ids.length; i++) {
      const l = ctx.livre(ids[i]);
      progression(e, { fraction: i / ids.length, texte: `Album ${i + 1} / ${ids.length}`, detail: l?.titre ?? ids[i], image: imagePng(`T${i + 1}`, (i * 53) % 360) });
      journal(e, 'sortie', `GET https://www.bedetheque.com/album-${1000 + i}.html (200)`);
      await attendre(e, 350);
      if (!l) continue;
      modif(e, { livreId: ids[i], titre: l.titre, champ: 'Series', avant: l.serie ?? '', apres: 'Astérix', genre: 'fiche' });
      modif(e, { livreId: ids[i], titre: l.titre, champ: 'Writer', avant: '', apres: 'René Goscinny', genre: 'fiche' });
      modif(e, { livreId: ids[i], titre: l.titre, champ: 'Summary', avant: '', apres: `Résumé ${PIEGE}`, genre: 'fiche' });
      if (i === 0) {
        modif(e, { livreId: ids[i], titre: l.titre, champ: 'Fichier', avant: `D:\\BD\\${l.titre}.cbz`, apres: 'D:\\BD\\Astérix\\Astérix 01.cbz', genre: 'renommage',
          conflit: 'un fichier du même nom existe déjà à la destination' });
      }
      if (i === 1) journal(e, 'erreur', 'Avertissement : couverture introuvable pour cet album.');
    }
    progression(e, { fraction: 1, texte: 'Terminé', detail: '' });
    if (e.lignes.some((l) => l.conflit)) e.conflits.push('1 renommage en conflit : il ne sera fait que si tu le coches.');
    if (ids[0]) modif(e, { livreId: ids[0], titre: ctx.livre(ids[0])?.titre ?? ids[0], champ: 'Number', avant: '1', apres: '1', genre: 'fiche' });
    await message(e, `${ids.length} album(s) traité(s). Vérifie l'aperçu avant d'appliquer.`);
    terminer(e, 'termine');
  }

  async function configurer(e: Execution) {
    const v = (reglages['bedetheque/bd2.py'] ??= {});
    // comicrust.ask_choice / comicrust.ask(secret=True) / comicrust.ask : questions simples.
    const LANGUES: Record<string, string> = { Français: 'fr', Anglais: 'en', Néerlandais: 'nl' };
    const langue = await ask(e, 'Langue des recherches sur bedetheque.com ?', {
      type: 'choix', choix: Object.keys(LANGUES), defaut: Object.keys(LANGUES).find((k) => LANGUES[k] === v.langue) ?? 'Français',
    });
    v.langue = LANGUES[String(langue)] ?? v.langue;
    const cle = await ask(e, `Clé d'accès à l'API ${PIEGE}\n(vide : garder la clé actuelle)`, { type: 'texte', secret: true, defaut: '' });
    if (typeof cle === 'string' && cle) v.cleApi = cle;
    const prefixe = await ask(e, 'Préfixe des numéros de tome :', { type: 'texte', defaut: String(v.prefixe ?? 'T.') });
    v.prefixe = prefixe;
    journal(e, 'info', `Langue : ${String(v.langue)} ; préfixe : ${String(prefixe)} ; clé ${cle ? 'changée' : 'inchangée'}.`);
    const r = (await formulaire(e, {
      title: 'Configurer BD2', buttons: ['Enregistrer', 'Annuler'],
      values: { langue: v.langue, delai: v.delai, couverture: v.couverture, dossier: v.dossier },
      controls: [
        { type: 'radio', key: 'langue', label: 'Langue', choices: [['fr', 'Français'], ['en', 'Anglais']] },
        { type: 'number', key: 'delai', label: 'Pause (s)', min: 0, max: 30 },
        { type: 'checkbox', key: 'couverture', label: 'Télécharger la couverture' },
        { type: 'file', key: 'dossier', label: 'Fichier App.Config', filters: [['Configuration', ['config']]] },
        { type: 'secret', key: 'cleApi', label: 'Clé d\'accès' },
      ],
    })) as Record<string, unknown> | null;
    if (!r) { terminer(e, 'annule'); return; }
    for (const [k, x] of Object.entries(r)) if (k !== '$bouton' && !(secrets.has(k) && x === '')) v[k] = x;
    journal(e, 'info', 'Réglages enregistrés.');
    terminer(e, 'termine');
  }

  /** « Configurer BD2 » réel : formulaire du script d'exemple. Les tests peuvent afficher un autre
   *  formulaire réel à la place (`window.__comicrustFormulaireReel = spec` avant le lancement). */
  async function configurerReel(e: Execution) {
    const autre = (globalThis as { __comicrustFormulaireReel?: FormSpecScript }).__comicrustFormulaireReel;
    const spec = autre ?? SPEC_CONFIGURER_BD2;
    const r = (await formulaire(e, JSON.parse(JSON.stringify(spec)) as FormSpecScript)) as Record<string, unknown> | null;
    if (!r) { journal(e, 'info', 'Réglages inchangés.'); terminer(e, 'annule'); return; }
    journal(e, 'info', `Bouton : ${String(r['$bouton'] ?? '')} ; champs à récupérer : ${Array.isArray(r.Donnees) ? r.Donnees.length : 0}.`);
    terminer(e, 'termine');
  }

  async function rangement(e: Execution) {
    journal(e, 'systeme', 'rangement.py : ranger() sur toute la bibliothèque');
    for (let i = 1; i <= 8; i++) {
      progression(e, { fraction: i / 8, texte: `Analyse ${i * 625} / 5000` });
      await attendre(e, 150);
      const l = ctx.livre(String(i));
      if (l) modif(e, { livreId: String(i), titre: l.titre, champ: 'Fichier', avant: `D:\\BD\\vrac\\${i}.cbz`, apres: `D:\\BD\\${l.serie ?? 'Divers'}\\${l.serie ?? ''} ${l.numero ?? ''}.cbz`, genre: 'renommage' });
    }
    terminer(e, 'termine');
  }

  async function casse(e: Execution) {
    journal(e, 'systeme', 'casse.py : lancer()');
    journal(e, 'sortie', `Sortie piège : ${PIEGE}`);
    await attendre(e, 200);
    journal(e, 'erreur', 'Traceback (most recent call last):\n  File "casse.py", line 12, in lancer\n    1 / 0\nZeroDivisionError: division by zero');
    terminer(e, 'erreur', 'ZeroDivisionError: division by zero (casse.py, ligne 12)');
  }

  function lancer(id: string, hook: HookScript, ids: string[], editeur = false): string {
    const s = scripts.find((x) => x.id === id);
    if (!s) throw new Error(`script inconnu : ${id}`);
    if (!['Books', 'Library', 'ConfigScript', 'Editor'].includes(hook)) throw new Error(`le hook ${hook} ne se lance pas depuis un menu`);
    if (!s.hooks.includes(hook)) throw new Error(`« ${s.nom} » n'a pas le hook ${hook}`);
    // Un ConfigScript se lance même si le script est désactivé.
    if (!s.actif && hook !== 'ConfigScript') throw new Error(`« ${s.nom} » est désactivé (Gérer les scripts)`);
    if (variante === 'sansPython') throw new Error('Python introuvable : impossible de lancer un script.');
    if ((interactif && !interactif.fini) || enApplication) throw new Error('Un script est déjà en cours : attends sa fin ou arrête-le.');
    if (hook === 'Books' && ids.length === 0) throw new Error('aucun livre sélectionné');
    // Library : ids vide = toute la bibliothèque (le backend prend tous les livres).
    const e: Execution = {
      runId: `run${++numero}`, script: id, nom: s.nom, arret: false, attentes: new Map(), lignes: [], conflits: [], inchanges: 0,
      fini: false, applique: false, arretApplication: false, vignettes: new Map(),
    };
    execs.set(e.runId, e);
    interactif = e;
    tests.lancements.push({ id, hook, ids: [...ids] });
    const corps = id.startsWith('nettoyage') ? () => nettoyage(e, ids)
      : id === 'bedetheque/bd2.py:scraper' ? () => bedetheque(e, ids, editeur)
      : id === 'BD2_CONFIG' ? () => configurer(e)
      : id === 'bedetheque/bd2.py:ConfigureBD2' ? () => configurerReel(e)
      : id.startsWith('rangement') ? () => rangement(e)
      : () => casse(e);
    // Démarrage après la réponse (comme le backend : les premiers événements peuvent la suivre de peu).
    setTimeout(() => {
      corps().catch((err) => {
        if (err instanceof Arret) { journal(e, 'systeme', 'Script arrêté.'); terminer(e, 'annule'); }
        else terminer(e, 'erreur', String(err));
      });
    }, 5);
    return e.runId;
  }

  /** Application (asynchrone, 250 ms par ligne) : le créneau interactif reste occupé jusqu'au
   *  bilan (`executionEnCours`, lancement refusé) ; `script_arreter` l'arrête entre deux lignes. */
  async function appliquer(e: Execution, liste: string[]): Promise<BilanScript> {
    const motif = etat().motifLectureSeule;
    if (motif) throw new Error(`Application refusée : ${motif}`);
    if (e.applique) throw new Error('Modifications déjà appliquées.');
    if (enApplication) throw new Error('application déjà en cours');
    if (interactif && !interactif.fini && interactif !== e) throw new Error('Un autre script est en cours : attends sa fin avant d\'appliquer.');
    const cles = new Set(liste);
    tests.appliques.push([...cles]);
    enApplication = e.runId;
    e.arretApplication = false;
    const b: BilanScript = { fiches: 0, renommages: 0, comicinfo: 0, erreurs: [], arrete: false, resume: '' };
    try {
      const choisies = e.lignes.filter((l) => cles.has(l.cle));
      for (let i = 0; i < choisies.length; i++) {
        if (e.arretApplication) {
          b.arrete = true;
          b.erreurs.push(`${choisies.length - i} opération(s) non faite(s) : arrêt demandé avant leur tour`);
          break;
        }
        const l = choisies[i];
        progression(e, { texte: `Ligne ${i + 1} / ${choisies.length} : ${l.titre}` });
        await ctx.pause(250);
        if (l.conflit) { b.erreurs.push(`${l.titre} : ${l.conflit}`); continue; }
        if (l.genre === 'fiche') b.fiches++;
        else if (l.genre === 'renommage') b.renommages++;
        else b.comicinfo++;
      }
      e.applique = true;
      b.resume = `${b.fiches} fiche(s) modifiée(s), ${b.renommages} renommage(s), ${b.comicinfo} ComicInfo.xml écrit(s)`
        + (b.erreurs.length ? `, ${b.erreurs.length} problème(s)` : '') + (b.arrete ? ', arrêté' : '') + '.';
    } finally {
      enApplication = null;
    }
    journal(e, 'systeme', `Bilan : ${b.resume}`);
    ctx.emettre('script:applique', { runId: e.runId, bilan: b });
    return b;
  }

  function exec(args: Record<string, unknown>): Execution {
    const e = execs.get(String(args.runId));
    if (!e) throw new Error(`exécution inconnue : ${String(args.runId)}`);
    return e;
  }

  function appeler(commande: string, args: Record<string, unknown>): unknown {
    switch (commande) {
      case 'scripts_lister':
      case 'scripts_recharger':
        return etat();
      case 'scripts_activer': {
        const s = scripts.find((x) => x.id === args.id);
        if (!s) throw new Error(`script inconnu : ${String(args.id)}`);
        scripts = scripts.map((x) => (x.id === args.id ? { ...x, actif: !!args.actif } : x));
        return etat();
      }
      case 'scripts_auto':
        auto = !!args.actif;
        if (auto) {
          // Un automatisme simulé se déclenche peu après (journal des automatismes).
          setTimeout(() => ctx.emettre('scripts:auto', { script: 'Marquer les nouveaux livres', hook: 'NewBooks', ligne: `3 nouveau(x) livre(s) marqué(s) ${PIEGE}` }), 300);
        }
        return etat();
      case 'script_lancer':
        return lancer(String(args.id), args.hook as HookScript, (args.ids as string[]) ?? []);
      case 'script_hook_editeur':
        tests.editeur.push({ id: String(args.id), livreId: String(args.livreId), champs: { ...(args.champs as Record<string, string>) } });
        return lancer(String(args.id), 'Editor', [String(args.livreId)], true);
      case 'script_repondre': {
        const e = exec(args);
        tests.reponses.push({ runId: e.runId, reqId: Number(args.reqId), valeur: JSON.parse(JSON.stringify(args.valeur ?? null)) });
        const r = e.attentes.get(Number(args.reqId));
        if (!r) throw new Error('question déjà répondue ou inconnue');
        e.attentes.delete(Number(args.reqId));
        r(args.valeur ?? null);
        return null;
      }
      case 'script_vignettes_voulues': {
        const e = exec(args);
        const reqId = Number(args.reqId);
        const rangs = (args.rangs as number[]) ?? [];
        tests.vignettesVoulues.push([...rangs]);
        const deja = e.vignettes.get(reqId);
        const f = fournisseurs.get(`${e.runId}#${reqId}`);
        if (!deja || !f) return null;
        rangs.filter((r) => !deja.has(r)).forEach((rang, k) => {
          deja.add(rang);
          setTimeout(() => {
            if (fournisseurs.has(`${e.runId}#${reqId}`)) ctx.emettre('script:vignette', { runId: e.runId, reqId, rang, image: f(rang) });
          }, 80 + k * 20);
        });
        return null;
      }
      case 'script_arreter': {
        const e = exec(args);
        tests.arrets.push(e.runId);
        // Pendant l'application : arrêt entre deux lignes (comme entre deux livres).
        if (enApplication === e.runId) { e.arretApplication = true; return null; }
        if (e.fini) return null;
        e.arret = true;
        for (const r of e.attentes.values()) r(null);
        return null;
      }
      case 'script_apercu': {
        const e = exec(args);
        const motif = etat().motifLectureSeule;
        const a: ApercuScript = e.applique
          ? { lignes: [], conflits: [], inchanges: 0, lectureSeule: !!motif, ...(motif ? { motif } : {}) }
          : { lignes: e.lignes.map((l) => ({ ...l })), conflits: [...e.conflits], inchanges: e.inchanges, lectureSeule: !!motif, ...(motif ? { motif } : {}) };
        return a;
      }
      case 'script_appliquer':
        return appliquer(exec(args), (args.cles as string[]) ?? []);
      case 'script_reglages_lire':
        return masquer(reglagesDe(String(args.id)));
      case 'script_reglages_ecrire': {
        const id = String(args.id);
        const valeurs = (args.valeurs as Record<string, unknown>) ?? {};
        tests.reglages.push({ id, valeurs: JSON.parse(JSON.stringify(valeurs)) as Record<string, unknown> });
        reglagesDe(id);
        const fichier = scripts.find((x) => x.id === id)!.fichier;
        const v = (reglages[fichier] ??= {});
        for (const [k, x] of Object.entries(valeurs)) if (k !== '$secrets' && !(secrets.has(k) && x === '••••')) v[k] = x;
        return masquer(v);
      }
      default:
        return undefined;
    }
  }

  return { appeler };
}
