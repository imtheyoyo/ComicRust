// Scripts Python (tauri-v0.9.0) : état partagé de l'interface (liste des scripts, exécution en
// cours, journal des automatismes). Contrat : docs/tauri/ipc-scripts.md. La logique (lancement de
// Python, validation des formulaires, application des modifications) est dans le backend ; ici on
// affiche et on relaie les réponses de l'utilisateur.
import {
  ecouter, messageErreur, scriptApercu, scriptAppliquer, scriptArreter, scriptHookEditeur, scriptLancer, scriptRepondre,
  scriptsActiver, scriptsAuto, scriptsLister, scriptsRecharger, scriptVignettesVoulues, imageScriptSure,
  type ApercuScript, type BilanScript, type ChooseSpecScript, type EtatScripts, type EvAppliqueScript, type EvAutoScript,
  type EvChoixScript, type EvFinScript, type EvFormulaireScript, type EvJournalScript, type EvProgressionScript,
  type EvQuestionScript, type EvVignetteScript, type DetailQuestionScript, type FormSpecScript, type GenreJournal, type HookScript, type InfoScript,
} from './api';

/** Nombre maximal de lignes gardées dans le journal d'une exécution (les plus anciennes partent). */
const MAX_JOURNAL = 5000;
/** Nombre maximal de lignes gardées dans le journal des automatismes. */
const MAX_AUTO = 1000;

export interface LigneJournal { n: number; genre: GenreJournal; texte: string }
/** Question en cours. `question` absent pour un message ; pour une question d'un ancien
 *  contrat sans détail, on la traite comme une confirmation. */
export interface QuestionScript { reqId: number; genre: 'ask' | 'message'; titre?: string; texte: string; question?: DetailQuestionScript }
export interface FormulaireEnCours { reqId: number; spec: FormSpecScript }
export interface ChoixEnCours { reqId: number; spec: ChooseSpecScript; vignettes: Record<number, string> }

/** Retour du hook Editor : champs proposés (null si le script n'a pas fini normalement). */
export type RappelEditeur = (champs: Record<string, unknown> | null, fin: EvFinScript) => void;

/** Une exécution de script lancée depuis l'interface. */
export class ExecutionScript {
  runId = $state<string | null>(null);
  journal = $state<LigneJournal[]>([]);
  progression = $state<{ fraction?: number; texte?: string; detail?: string; image?: string }>({});
  question = $state<QuestionScript | null>(null);
  formulaire = $state<FormulaireEnCours | null>(null);
  choix = $state<ChoixEnCours | null>(null);
  fin = $state<EvFinScript | null>(null);
  arretDemande = $state(false);
  apercu = $state<ApercuScript | null>(null);
  chargementApercu = $state(false);
  application = $state(false);
  bilan = $state<BilanScript | null>(null);
  abandonne = $state(false);
  erreur = $state('');
  readonly debut = Date.now();
  duree = $state(0);
  private n = 0;

  constructor(
    readonly script: InfoScript,
    readonly hook: HookScript,
    readonly ids: string[],
    readonly editeur?: RappelEditeur,
  ) {}

  /** En cours : lancée et pas finie. */
  get enCours(): boolean {
    return !this.fin && !this.erreur;
  }

  ajouterJournal(genre: GenreJournal, texte: string) {
    const l = { n: this.n++, genre, texte };
    this.journal = this.journal.length >= MAX_JOURNAL ? [...this.journal.slice(-MAX_JOURNAL + 1), l] : [...this.journal, l];
  }
}

class GestionScripts {
  etat = $state<EtatScripts | null>(null);
  erreur = $state('');
  chargement = $state(false);
  gestionOuverte = $state(false);
  /** Onglet de la fenêtre de gestion. */
  ongletGestion = $state<'scripts' | 'journal'>('scripts');
  execution = $state<ExecutionScript | null>(null);
  journalAuto = $state<Array<EvAutoScript & { n: number; heure: string }>>([]);
  private nAuto = 0;
  private abonnements: Array<() => void> = [];
  private demarre = false;
  /** Événements arrivés avant la réponse de script_lancer (runId pas encore connu). */
  private enAttente: Array<{ nom: string; charge: { runId: string } }> = [];

  /** Scripts actifs ayant ce hook. */
  scriptsDe(hook: HookScript): InfoScript[] {
    return (this.etat?.scripts ?? []).filter((s) => s.actif && s.hooks.includes(hook));
  }

  /** Point d'entrée par son id (opaque). */
  script(id: string | undefined): InfoScript | undefined {
    return id ? this.etat?.scripts.find((s) => s.id === id) : undefined;
  }

  /** Raison pour laquelle on ne peut pas lancer de script (Python absent, script en cours…). */
  get raisonIndisponible(): string | undefined {
    if (this.etat?.python.erreur) return this.etat.python.erreur;
    if (this.execution?.enCours || this.execution?.application) return 'Un script est déjà en cours.';
    // Exécution interactive connue du backend mais pas de cette fenêtre (page rechargée…).
    const autre = this.etat?.executionEnCours;
    if (autre && autre !== this.execution?.runId) return 'Un script est déjà en cours.';
    return undefined;
  }

  async demarrer() {
    if (this.demarre) return;
    this.demarre = true;
    const ec = <T extends { runId: string }>(nom: string, f: (e: ExecutionScript, c: T) => void) =>
      ecouter<T>(nom, (c) => this.router(nom, c, f as (e: ExecutionScript, c: { runId: string }) => void)).then((u) => this.abonnements.push(u));
    await Promise.all([
      ec<EvJournalScript>('script:journal', (e, c) => e.ajouterJournal(c.genre, String(c.texte ?? ''))),
      ec<EvProgressionScript>('script:progression', (e, c) => {
        const p = { ...e.progression };
        if (typeof c.fraction === 'number' && Number.isFinite(c.fraction)) p.fraction = Math.min(1, Math.max(0, c.fraction));
        if (c.texte !== undefined) p.texte = String(c.texte);
        if (c.detail !== undefined) p.detail = String(c.detail);
        if (c.image !== undefined) p.image = imageScriptSure(c.image);
        e.progression = p;
      }),
      ec<EvQuestionScript>('script:question', (e, c) => {
        const genre = c.genre === 'message' ? 'message' : 'ask';
        e.question = {
          reqId: c.reqId, genre, titre: c.titre, texte: String(c.texte ?? ''),
          question: genre === 'ask' ? detailQuestion(c.question) : undefined,
        };
      }),
      ec<EvFormulaireScript>('script:formulaire', (e, c) => { e.formulaire = { reqId: c.reqId, spec: c.spec ?? {} }; }),
      ec<EvChoixScript>('script:choix', (e, c) => { e.choix = { reqId: c.reqId, spec: c.spec ?? {}, vignettes: {} }; }),
      ec<EvVignetteScript>('script:vignette', (e, c) => {
        const img = imageScriptSure(c.image);
        if (e.choix && e.choix.reqId === c.reqId && img) e.choix.vignettes = { ...e.choix.vignettes, [c.rang]: img };
      }),
      ec<EvFinScript>('script:fin', (e, c) => this.terminer(e, c)),
      ec<EvAppliqueScript>('script:applique', (e, c) => { if (!e.bilan) e.bilan = c.bilan; }),
      ecouter<EvAutoScript>('scripts:auto', (c) => {
        const l = { script: String(c.script ?? ''), hook: String(c.hook ?? ''), ligne: String(c.ligne ?? ''), n: this.nAuto++, heure: new Date().toLocaleTimeString('fr-FR') };
        this.journalAuto = this.journalAuto.length >= MAX_AUTO ? [...this.journalAuto.slice(-MAX_AUTO + 1), l] : [...this.journalAuto, l];
      }).then((u) => this.abonnements.push(u)),
    ]);
    await this.charger();
  }

  arreter() {
    for (const u of this.abonnements) u();
    this.abonnements = [];
    this.demarre = false;
  }

  private routeurs = new Map<string, (e: ExecutionScript, c: { runId: string }) => void>();

  private router(nom: string, c: { runId: string }, f: (e: ExecutionScript, c: { runId: string }) => void) {
    this.routeurs.set(nom, f);
    const e = this.execution;
    if (!e) return;
    if (e.runId === null && e.enCours) {
      // Lancement pas encore confirmé : on garde l'événement (borné) pour le rejouer.
      if (this.enAttente.length < 2000) this.enAttente.push({ nom, charge: c });
      return;
    }
    if (c.runId === e.runId) f(e, c);
  }

  private rejouer(e: ExecutionScript) {
    const l = this.enAttente;
    this.enAttente = [];
    for (const { nom, charge } of l) if (charge.runId === e.runId) this.routeurs.get(nom)?.(e, charge);
  }

  private terminer(e: ExecutionScript, c: EvFinScript) {
    e.fin = c;
    e.duree = (Date.now() - e.debut) / 1000;
    e.question = null;
    e.formulaire = null;
    e.choix = null;
    void this.charger(); // executionEnCours relu
    if (e.editeur) {
      e.editeur(c.etat === 'termine' ? (c.champs ?? {}) : null, c);
      // Comme la version egui : pas d'aperçu, la fenêtre du script se ferme si tout s'est bien passé.
      if (c.etat === 'termine' && this.execution === e) this.execution = null;
      return;
    }
    if (c.etat === 'termine' && c.nbModifs > 0) void this.chargerApercu();
    // Arrêté ou en erreur : l'aperçu de ce qui a été demandé reste proposé (bouton).
  }

  async charger() {
    this.chargement = true;
    try {
      this.etat = await scriptsLister();
      this.erreur = '';
    } catch (e) {
      this.erreur = messageErreur(e);
    } finally {
      this.chargement = false;
    }
  }

  private async faire(f: () => Promise<EtatScripts>) {
    try {
      this.etat = await f();
      this.erreur = '';
    } catch (e) {
      this.erreur = messageErreur(e);
    }
  }
  recharger = () => this.faire(scriptsRecharger);
  activer = (id: string, actif: boolean) => this.faire(() => scriptsActiver(id, actif));
  basculerAuto = (actif: boolean) => this.faire(() => scriptsAuto(actif));

  ouvrirGestion(onglet: 'scripts' | 'journal' = 'scripts') {
    this.ongletGestion = onglet;
    this.gestionOuverte = true;
    void this.charger();
  }

  /** Lance un script (hook Books : ids des livres ; Library / ConfigScript : liste vide). */
  async lancer(script: InfoScript, hook: HookScript, ids: string[]) {
    await this.demarrer();
    const e = new ExecutionScript(script, hook, ids);
    this.execution = e;
    this.enAttente = [];
    try {
      e.runId = await scriptLancer(script.id, hook, ids);
      this.rejouer(e);
    } catch (err) {
      e.erreur = messageErreur(err);
      this.enAttente = [];
    }
  }

  /** Hook Editor (fenêtre Infos) : `rappel` reçoit les champs proposés à la fin. */
  async lancerEditeur(script: InfoScript, livreId: string, champs: Record<string, string>, rappel: RappelEditeur): Promise<string | null> {
    await this.demarrer();
    const e = new ExecutionScript(script, 'Editor', [livreId], rappel);
    this.execution = e;
    this.enAttente = [];
    try {
      e.runId = await scriptHookEditeur(script.id, livreId, champs);
      this.rejouer(e);
      return null;
    } catch (err) {
      e.erreur = messageErreur(err);
      this.enAttente = [];
      return e.erreur;
    }
  }

  /** Réponse à la question / au formulaire / au choix en cours. */
  async repondre(quoi: 'question' | 'formulaire' | 'choix', valeur: unknown) {
    const e = this.execution;
    if (!e?.runId) return;
    const req = e[quoi];
    if (!req) return;
    e[quoi] = null;
    try {
      await scriptRepondre(e.runId, req.reqId, valeur);
    } catch (err) {
      e.ajouterJournal('erreur', `Réponse non transmise : ${messageErreur(err)}`);
    }
  }

  vignettesVoulues(rangs: number[]) {
    const e = this.execution;
    if (!e?.runId || !e.choix || rangs.length === 0) return;
    void scriptVignettesVoulues(e.runId, e.choix.reqId, rangs).catch(() => {});
  }

  async arreterScript() {
    const e = this.execution;
    if (!e?.runId || !e.enCours) return;
    e.arretDemande = true;
    try {
      await scriptArreter(e.runId);
    } catch (err) {
      e.ajouterJournal('erreur', `Arrêt impossible : ${messageErreur(err)}`);
    }
  }

  async chargerApercu() {
    const e = this.execution;
    if (!e?.runId) return;
    e.chargementApercu = true;
    try {
      e.apercu = await scriptApercu(e.runId);
    } catch (err) {
      e.erreur = '';
      e.ajouterJournal('erreur', `Aperçu impossible : ${messageErreur(err)}`);
    } finally {
      e.chargementApercu = false;
    }
  }

  async appliquer(cles: string[]) {
    const e = this.execution;
    if (!e?.runId || !e.apercu) return;
    e.application = true;
    try {
      e.bilan = await scriptAppliquer(e.runId, cles);
    } catch (err) {
      e.ajouterJournal('erreur', `Application refusée : ${messageErreur(err)}`);
      e.bilan = { fiches: 0, renommages: 0, comicinfo: 0, erreurs: [messageErreur(err)], arrete: false, resume: 'Rien n\'a été appliqué' };
    } finally {
      e.application = false;
      void this.charger();
    }
  }

  abandonner() {
    if (this.execution) this.execution.abandonne = true;
  }

  fermerExecution() {
    if (this.execution?.enCours) return;
    this.execution = null;
    // executionEnCours du backend relu (sinon un état périmé bloquerait les lancements).
    void this.charger();
  }
}

/** Détail d'une question `ask`, lu avec prudence (valeurs venant d'un script). */
function detailQuestion(q: unknown): DetailQuestionScript {
  const o = (q && typeof q === 'object' ? q : {}) as Partial<DetailQuestionScript>;
  const type = o.type === 'texte' || o.type === 'choix' || o.type === 'confirmation' ? o.type : 'confirmation';
  const choix = Array.isArray(o.choix) ? o.choix.map((c) => String(c ?? '')) : [];
  const secret = o.secret === true;
  return { type, defaut: secret ? null : (o.defaut ?? null), choix, secret };
}

export const scripts = new GestionScripts();
