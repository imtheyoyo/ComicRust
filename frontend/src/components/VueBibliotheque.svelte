<script lang="ts">
  // Vue Bibliothèque : barre d'outils (recherche, tri, groupement, vue), grille ou tableau
  // virtualisés, panneau de détails et barre d'état. Les données viennent de lister_livres.
  import { onMount, tick, untrack } from 'svelte';
  import {
    ajouterLivresListe, creerListe, decalerGroupeListe, decalerLivresListe, deplacerGroupesListe, deplacerLivresListe, ecouter,
    groupesVue, messageErreur, ouvrirDossierFichier, retirerLivresListe,
    type BibliothequeModifiee, type ClefTri, type Groupe, type VueListe, type NoeudListe, type RequeteLivres, type SourceLecteur,
  } from '../lib/api';
  import { GlisserLivres } from '../lib/glisserLivres.svelte';
  import { DEPOSER_SUR_LECTURE, TRIER_POUR_REORDONNER, glypheListe, type EtatListes } from '../lib/listes.svelte';
  import type { Modificateurs } from '../lib/selection.svelte';
  import { Bibliotheque, TAILLE_PAGE, type SuiviScan } from '../lib/bibliotheque.svelte';
  import { enregistrerPreference, enregistrerVueListe, preferences, vueListe } from '../lib/prefs';
  import { Selection } from '../lib/selection.svelte';
  import DialogInfos from './DialogInfos.svelte';
  import { fermerModale, modale } from '../lib/modale';
  import DialogOperation from './DialogOperation.svelte';
  import DialogRenommer from './DialogRenommer.svelte';
  import GrilleCouvertures from './GrilleCouvertures.svelte';
  import GrillePiles from './GrillePiles.svelte';
  import MenuContextuel, { type EntreeMenu } from './MenuContextuel.svelte';
  import Lecteur from './Lecteur.svelte';
  import PanneauDetails from './PanneauDetails.svelte';
  import VueTableau from './VueTableau.svelte';

  interface Props {
    scan: SuiviScan;
    /** Nombre de livres de la bibliothèque (hors filtre), maintenu pour la barre latérale. */
    nbLivres: number;
    /** Incrémenté par le parent quand le contenu a pu changer (fin de scan). */
    rafraichir: number;
    /** Étape 5 : état des listes (glisser-déposer, commandes de liste). */
    listes: EtatListes;
    /** Liste affichée (null : toute la bibliothèque). */
    liste: NoeudListe | null;
  }
  let { scan, nbLivres = $bindable(), rafraichir, listes, liste }: Props = $props();

  // Préférences mémorisées (data\comicrust-tauri.json), chargées par App avant ce composant :
  // le premier lister_livres part directement avec le tri et le groupe enregistrés.
  const prefs = preferences();
  // Chaque liste a sa propre vue (tri, sens, groupe, grille / tableau), comme dans ComicRack,
  // mémorisée dans data\comicrust-tauri.json (`vue.listes`, jamais dans ComicDb.xml) ; la
  // bibliothèque garde la sienne (champs de premier niveau de `vue`). Une liste jamais réglée
  // s'affiche : liste de lecture dans l'ordre de la liste (« # » croissant), sans groupe ; autre
  // liste avec le tri et le groupe de la bibliothèque.
  const listeInitiale = untrack(() => liste);
  let biblio: VueListeComplete = {
    tri: prefs.tri ?? 'serie', sensTri: prefs.sensTri ?? 'asc', groupe: prefs.groupe ?? 'aucun', vue: prefs.vue ?? 'grille',
    empiler: prefs.empiler ?? false,
  };
  const vueDefaut = (l: NoeudListe): VueListeComplete =>
    l.type === 'lecture' ? { tri: 'ordre', sensTri: 'asc', groupe: 'aucun', vue: biblio.vue, empiler: false } : { ...biblio };
  const vueDe = (l: NoeudListe): VueListeComplete => ({ ...vueDefaut(l), ...(vueListe(l.id) ?? {}) });
  const vueInitiale = listeInitiale ? vueDe(listeInitiale) : biblio;
  const bib = new Bibliotheque({
    tri: vueInitiale.tri, sensTri: vueInitiale.sensTri, groupe: vueInitiale.groupe, liste: listeInitiale?.id,
  });
  // Vue empilée par série (grille seulement) : réglage de la bibliothèque, ou propre à chaque liste.
  bib.empile = vueInitiale.empiler && vueInitiale.vue === 'grille';
  let vue = $state<'grille' | 'tableau'>(vueInitiale.vue);
  let empiler = $state(vueInitiale.empiler);
  /** Pile sélectionnée (titre de la série) : un clic la sélectionne sans l'ouvrir. */
  let pileSel = $state<string | null>(null);
  let zoom = $state(prefs.zoom ?? 150);
  let details = $state(prefs.details ?? true);
  const LARGEUR_MIN = 220;
  const LARGEUR_MAX = 800;
  let largeurDetails = $state(prefs.largeurDetails ?? 320);
  /** Sélection multiple (Ctrl+clic, Maj+clic, Ctrl+A, Maj+flèches) ; `sel.curseur` : livre courant. */
  const sel = new Selection();
  /** Menu contextuel ouvert (position). */
  let menu = $state<{ x: number; y: number; pile?: string } | null>(null);
  /** Fenêtres de l'édition (étape 3), sur les identifiants de la sélection au moment de l'ouverture. */
  let infosDe = $state<string[] | null>(null);
  let operation = $state<{ type: 'conversion' | 'ecritureInfos'; ids: string[] } | null>(null);
  let renommerDe = $state<string[] | null>(null);
  let erreurAction = $state('');
  /** Incrémenté quand des fiches ont changé (bibliotheque-modifiee) : le panneau relit le détail. */
  let fichesModifiees = $state(0);
  /** Livre ouvert dans le lecteur plein écran (livre de la bibliothèque ou fichier), ou null. */
  let livreLu = $state<SourceLecteur | null>(null);
  /** Ouverture d'un fichier hors bibliothèque : chemin saisi (pas de sélecteur de fichiers). */
  let dialogueFichier = $state<HTMLDialogElement>();
  let cheminFichier = $state('');
  let champRecherche = $state<HTMLInputElement>();
  let saisie = $state('');
  let minuterie: ReturnType<typeof setTimeout> | undefined;

  const tris: Array<{ id: ClefTri; titre: string }> = [
    { id: 'serie', titre: 'Série' },
    { id: 'titre', titre: 'Titre' },
    { id: 'parution', titre: 'Parution' },
    { id: 'ajout', titre: "Date d'ajout" },
    { id: 'lecture', titre: 'Dernière lecture' },
    { id: 'note', titre: 'Note' },
    { id: 'pages', titre: 'Pages' },
    { id: 'avancement', titre: 'Avancement' },
    { id: 'taille', titre: 'Taille du fichier' },
    { id: 'fichier', titre: 'Nom du fichier' },
  ];
  const groupes: Array<{ id: Groupe; titre: string }> = [
    { id: 'aucun', titre: 'Aucun groupement' },
    { id: 'serie', titre: 'Par série' },
    { id: 'editeur', titre: 'Par éditeur' },
  ];

  onMount(() => {
    void bib.reinitialiser();
    // Fiches modifiées par l'édition : liste rechargée sans perdre défilement ni sélection.
    let desabonner: (() => void) | undefined;
    void ecouter<BibliothequeModifiee>('bibliotheque-modifiee', () => {
      void bib.reinitialiser(false);
      fichesModifiees++;
    }).then((f) => (desabonner = f));
    return () => {
      clearTimeout(minuterie);
      desabonner?.();
    };
  });

  // Toute modification d'un réglage d'affichage est enregistrée (avec délai, voir prefs.ts). Tri,
  // groupe et grille / tableau d'une liste vont dans sa propre vue (seulement si elle a été réglée :
  // une liste jamais touchée suit les valeurs par défaut) ; ceux de la bibliothèque, dans les siens.
  $effect(() => {
    enregistrerPreference({ zoom, details, largeurDetails });
    const v: VueListeComplete = { tri: bib.params.tri, sensTri: bib.params.sensTri, groupe: bib.params.groupe ?? 'aucun', vue, empiler };
    const id = bib.params.liste;
    untrack(() => {
      if (!id) {
        biblio = v;
        enregistrerPreference(v);
      } else if (liste?.id === id && (vueListe(id) || !memeVue(v, vueDefaut(liste)))) {
        enregistrerVueListe(id, v);
      }
    });
  });

  // Liste choisie dans le panneau : la vue montre ses livres (ou revient à la bibliothèque).
  $effect(() => {
    const id = liste?.id;
    untrack(() => {
      if (id === bib.params.liste) return;
      const v = liste ? vueDe(liste) : biblio;
      vue = v.vue;
      empiler = v.empiler;
      bib.regler(v.empiler && v.vue === 'grille');
      changer({ liste: liste?.id, tri: v.tri, sensTri: v.sensTri, groupe: v.groupe, serie: undefined });
    });
  });

  // Vue empilée activée ou désactivée (ou passage au tableau) : les piles sont chargées ou oubliées ;
  // une pile ouverte est refermée si la vue n'est plus empilée.
  const enPiles = $derived(empiler && vue === 'grille');
  $effect(() => {
    const e = enPiles;
    untrack(() => {
      if (!bib.regler(e)) return;
      if (!e && bib.params.serie !== undefined) changer({ serie: undefined });
      else void bib.reinitialiser(false);
    });
  });
  const pileOuverte = $derived(enPiles && bib.params.serie !== undefined);
  /** Les piles (et non les livres) sont affichées. */
  const pilesAffichees = $derived(enPiles && !pileOuverte);
  /** Pile sélectionnée si elle existe encore dans la vue (recherche, liste ou tri changés). */
  const pileSelectionnee = $derived(enPiles && bib.piles.some((p) => p.serie === pileSel) ? pileSel : null);

  // Listes modifiées (ajout, retrait, réordonnancement, critères…) : la liste affichée est relue
  // sans perdre le défilement.
  let versionListes = untrack(() => listes.version);
  $effect(() => {
    const v = listes.version;
    if (v !== versionListes && untrack(() => bib.params.liste)) void bib.reinitialiser(false);
    versionListes = v;
  });

  // Fin de scan : on recharge sans toucher au défilement ni à la sélection.
  let derniereVersion = -1;
  $effect(() => {
    const v = rafraichir;
    if (derniereVersion >= 0 && v !== derniereVersion) void bib.reinitialiser(false);
    derniereVersion = v;
  });

  $effect(() => {
    if (bib.params.recherche === '' && !bib.params.liste) nbLivres = bib.total;
    sel.borner(bib.total);
  });

  /** Vue complète d'une liste ou de la bibliothèque (tous les champs renseignés). */
  type VueListeComplete = Required<VueListe>;
  function memeVue(a: VueListeComplete, b: VueListeComplete) {
    return a.tri === b.tri && a.sensTri === b.sensTri && a.groupe === b.groupe && a.vue === b.vue && a.empiler === b.empiler;
  }

  function changer(patch: Partial<typeof bib.params>) {
    Object.assign(bib.params, patch);
    sel.vider();
    void bib.reinitialiser();
  }

  function recherche(v: string) {
    saisie = v;
    clearTimeout(minuterie);
    minuterie = setTimeout(() => changer({ recherche: v.trim() }), 200);
  }

  function trier(clef: ClefTri) {
    if (bib.params.tri === clef) changer({ sensTri: bib.params.sensTri === 'asc' ? 'desc' : 'asc' });
    else changer({ tri: clef, sensTri: 'asc' });
  }

  function ouvrir(i: number) {
    const l = bib.livre(i);
    if (!l) return;
    sel.unique(i);
    livreLu = { id: l.id };
  }

  /** Ouvre la pile d'une série : ses livres s'affichent en grille (Échap ou « Piles » pour revenir). */
  function ouvrirPile(serie: string) {
    pileSel = serie;
    changer({ serie });
  }

  async function fermerPile() {
    changer({ serie: undefined });
    await tick();
    document.querySelector<HTMLElement>('[data-testid="grille-piles"]')?.focus();
  }

  function ouvrirFichier() {
    const chemin = cheminFichier.trim();
    if (!chemin) return;
    fermerModale(dialogueFichier);
    livreLu = { chemin };
  }

  /** Lecteur fermé : l'avancement a changé (lu, page courante), on recharge sans perdre la
   *  sélection ni le défilement, puis on rend le focus à la liste. */
  async function lecteurFerme() {
    livreLu = null;
    void bib.reinitialiser(false);
    await tick();
    document.querySelector<HTMLElement>('[data-testid="grille"], [data-testid="tableau"], [data-testid="grille-piles"]')?.focus();
  }

  function choisirVue(v: 'grille' | 'tableau') {
    vue = v;
  }

  // Largeur du panneau de détails : poignée à tirer (ou flèches gauche/droite au clavier).
  const borner = (l: number) =>
    Math.round(Math.min(Math.max(l, LARGEUR_MIN), Math.min(LARGEUR_MAX, Math.max(LARGEUR_MIN, window.innerWidth * 0.6))));

  function debutRedimension(e: PointerEvent) {
    const poignee = e.currentTarget as HTMLElement;
    poignee.setPointerCapture(e.pointerId);
    const x0 = e.clientX;
    const l0 = largeurDetails;
    const bouger = (m: PointerEvent) => (largeurDetails = borner(l0 + x0 - m.clientX));
    const finir = () => {
      poignee.removeEventListener('pointermove', bouger);
      poignee.removeEventListener('pointerup', finir);
      poignee.removeEventListener('pointercancel', finir);
    };
    poignee.addEventListener('pointermove', bouger);
    poignee.addEventListener('pointerup', finir);
    poignee.addEventListener('pointercancel', finir);
    e.preventDefault();
  }

  /** Écouteurs posés par attachement : un séparateur focalisable est un composant interactif
   *  (ARIA), ce que la règle a11y de Svelte ne reconnaît pas sur une `<div>`. */
  function ecouteursPoignee(el: HTMLElement) {
    el.addEventListener('pointerdown', debutRedimension);
    el.addEventListener('keydown', clavierPoignee);
    return () => {
      el.removeEventListener('pointerdown', debutRedimension);
      el.removeEventListener('keydown', clavierPoignee);
    };
  }

  function clavierPoignee(e: KeyboardEvent) {
    const pas = e.shiftKey ? 64 : 16;
    if (e.key === 'ArrowLeft') largeurDetails = borner(largeurDetails + pas);
    else if (e.key === 'ArrowRight') largeurDetails = borner(largeurDetails - pas);
    else return;
    e.preventDefault();
  }

  /** Une fenêtre de l'édition (ou une autre boîte de dialogue) est ouverte. */
  const fenetreOuverte = () => !!(infosDe || operation || renommerDe || document.querySelector('dialog[open]'));

  function clavierGlobal(e: KeyboardEvent) {
    if (livreLu) return; // le lecteur a son propre clavier
    const saisieTexte = (e.target as HTMLElement | null)?.closest('input, textarea, select');
    // Ctrl+I : infos de la sélection (comme ComicRack et la version egui).
    if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === 'i' && !fenetreOuverte()) {
      e.preventDefault();
      if (pileSelectionnee !== null && !pileOuverte) void actionPile(pileSelectionnee, 'infos');
      else void action('infos');
      return;
    }
    // Échap : referme la pile ouverte (retour aux piles).
    if (e.key === 'Escape' && pileOuverte && !saisieTexte && !menu && !fenetreOuverte()) {
      e.preventDefault();
      void fermerPile();
      return;
    }
    // Suppr dans une liste de lecture : retirer les livres de la liste (jamais les fichiers).
    if (e.key === 'Delete' && !saisieTexte && !fenetreOuverte() && liste?.type === 'lecture'
      && (e.target as HTMLElement | null)?.closest('[data-testid="grille"], [data-testid="tableau"], [data-testid="grille-piles"]')) {
      e.preventDefault();
      void action('retirer');
      return;
    }
    if ((e.key === '/' && !saisieTexte) || ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f')) {
      e.preventDefault();
      champRecherche?.focus();
      champRecherche?.select();
    }
  }

  function clavierRecherche(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      recherche('');
      (e.currentTarget as HTMLElement).blur();
    } else if (e.key === 'Enter' || e.key === 'ArrowDown') {
      clearTimeout(minuterie);
      if (saisie.trim() !== bib.params.recherche) changer({ recherche: saisie.trim() });
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        document.querySelector<HTMLElement>('[data-testid="grille"], [data-testid="tableau"], [data-testid="grille-piles"]')?.focus();
      }
    }
  }

  const livreSelectionne = $derived(sel.curseur >= 0 ? bib.livre(sel.curseur) : undefined);
  const nbSel = $derived(sel.nombre);

  // ───── Édition (étape 3) : menu contextuel et fenêtres ─────

  const entreesMenu = $derived.by<EntreeMenu[]>(() => {
    const plusieurs = nbSel > 1;
    const quoi = plusieurs ? `les ${nbSel.toLocaleString('fr-FR')} livres` : 'le livre';
    const raison = listes.raison;
    const refusOrdre = refusReordonner();
    const entreesListes: EntreeMenu[] = [
      { id: 'ajouter', libelle: `➕ Ajouter ${quoi} à une liste…`, separe: true, desactive: !!raison, aide: raison },
      ...(liste?.type === 'lecture'
        ? [
            { id: 'retirer', libelle: `➖ Retirer ${quoi} de la liste`, raccourci: 'Suppr', desactive: !!raison, aide: raison ?? 'Les fichiers ne sont pas supprimés' },
            { id: 'monter', libelle: '⬆ Monter dans la liste', desactive: !!refusOrdre, aide: refusOrdre ?? undefined },
            { id: 'descendre', libelle: '⬇ Descendre dans la liste', desactive: !!refusOrdre, aide: refusOrdre ?? undefined },
            ...(bib.params.groupe === 'serie'
              ? [
                  { id: 'monter-serie', libelle: '⬆ Monter la série', desactive: !!refusOrdre, aide: refusOrdre ?? undefined },
                  { id: 'descendre-serie', libelle: '⬇ Descendre la série', desactive: !!refusOrdre, aide: refusOrdre ?? undefined },
                ]
              : []),
          ]
        : []),
      ...(bib.params.groupe !== 'aucun'
        ? [{ id: 'groupe', libelle: bib.params.groupe === 'serie' ? 'Sélectionner toute la série' : 'Sélectionner tout l\'éditeur' }]
        : []),
    ];
    return [
      { id: 'lire', libelle: 'Lire', raccourci: 'Entrée', desactive: plusieurs, aide: plusieurs ? 'Un seul livre à la fois' : undefined },
      { id: 'infos', libelle: plusieurs ? `✏ Modifier les infos des ${nbSel.toLocaleString('fr-FR')} livres…` : '✏ Modifier les infos…', raccourci: 'Ctrl+I', separe: true },
      { id: 'ecritureInfos', libelle: plusieurs ? 'Écrire les infos dans les fichiers…' : 'Écrire les infos dans le fichier', separe: true,
        aide: 'Seuls les CBZ reçoivent les infos (convertir d\'abord en CBZ)' },
      { id: 'conversion', libelle: plusieurs ? 'Convertir les CBR / CB7 / CBT en CBZ…' : 'Convertir en CBZ', aide: 'Seuls les CBR, CB7 et CBT se convertissent' },
      { id: 'renommer', libelle: 'Renommer / déplacer…' },
      { id: 'dossier', libelle: 'Ouvrir le dossier du fichier', separe: true, desactive: plusieurs,
        aide: plusieurs ? 'Un seul livre à la fois' : 'Montre le fichier dans l\'Explorateur de Windows' },
      ...entreesListes,
    ];
  });

  /** Menu contextuel d'une pile : les actions portent sur tous ses livres. */
  function entreesPile(serie: string): EntreeMenu[] {
    const p = bib.piles.find((x) => x.serie === serie);
    const n = p?.nbLivres ?? 0;
    const quoi = n > 1 ? `les ${n.toLocaleString('fr-FR')} livres` : 'le livre';
    const raison = listes.raison;
    return [
      { id: 'ouvrir', libelle: '📂 Ouvrir la pile', raccourci: 'Entrée' },
      { id: 'infos', libelle: n > 1 ? `✏ Modifier les infos des ${n.toLocaleString('fr-FR')} livres…` : '✏ Modifier les infos…', raccourci: 'Ctrl+I', separe: true },
      { id: 'ecritureInfos', libelle: n > 1 ? 'Écrire les infos dans les fichiers…' : 'Écrire les infos dans le fichier', separe: true,
        aide: 'Seuls les CBZ reçoivent les infos (convertir d\'abord en CBZ)' },
      { id: 'conversion', libelle: n > 1 ? 'Convertir les CBR / CB7 / CBT en CBZ…' : 'Convertir en CBZ', aide: 'Seuls les CBR, CB7 et CBT se convertissent' },
      { id: 'renommer', libelle: 'Renommer / déplacer…' },
      { id: 'ajouter', libelle: `➕ Ajouter ${quoi} à une liste…`, separe: true, desactive: !!raison, aide: raison },
      ...(liste?.type === 'lecture'
        ? [{ id: 'retirer', libelle: `➖ Retirer ${quoi} de la liste`, desactive: !!raison, aide: raison ?? 'Les fichiers ne sont pas supprimés' }]
        : []),
    ];
  }

  async function actionPile(serie: string, id: string) {
    erreurAction = '';
    const p = bib.piles.find((x) => x.serie === serie);
    if (!p) return;
    if (id === 'ouvrir') ouvrirPile(serie);
    else await agirSurIds(id, p.ids);
  }

  /** Identifiants des livres sélectionnés (pages non chargées demandées au backend). */
  async function idsSelection(): Promise<string[]> {
    if (sel.nombre === 0 && sel.curseur >= 0) sel.unique(sel.curseur);
    return (await bib.livresDe(sel.indices())).map((l) => l.id);
  }

  async function action(id: string) {
    erreurAction = '';
    if (id === 'lire') {
      if (sel.curseur >= 0) ouvrir(sel.curseur);
      return;
    }
    if (id === 'groupe') {
      if (sel.curseur >= 0) void selectionnerGroupe(sel.curseur, { ctrl: false, maj: false });
      return;
    }
    if (id === 'monter-serie' || id === 'descendre-serie') {
      if (!liste || sel.curseur < 0) return;
      const idl = liste.id;
      const req = requete();
      try {
        const g = trouverGroupe(await groupesVue(req), sel.curseur);
        if (!g) return;
        const r = await listes.executer('Réordonnancement de la liste', () => decalerGroupeListe(idl, req, g.titre, id === 'monter-serie'));
        if (r) await selectionnerGroupes([g.titre]);
      } catch (e) {
        erreurAction = messageErreur(e);
      }
      return;
    }
    let ids: string[];
    try {
      ids = await idsSelection();
    } catch (e) {
      erreurAction = messageErreur(e);
      return;
    }
    if (ids.length === 0) return;
    if (id === 'monter' || id === 'descendre') {
      if (!liste) return;
      const idl = liste.id;
      const ind = sel.indices();
      const r = await listes.executer('Réordonnancement de la liste', () => decalerLivresListe(idl, ids, id === 'monter'));
      if (r?.info && ind.length) {
        const pas = id === 'monter' ? -1 : 1;
        sel.plage(ind[0] + pas, ind[ind.length - 1] + pas, false);
      }
      return;
    }
    await agirSurIds(id, ids);
  }

  /** Action du menu sur des livres connus par leur identifiant (sélection ou pile entière). */
  async function agirSurIds(id: string, ids: string[]) {
    if (id === 'ajouter') {
      ajoutDe = ids;
      choixAjout = listes.listeProposee();
      nomNouvelle = 'Nouvelle liste de lecture';
      return;
    }
    if (id === 'retirer') {
      if (!liste) return;
      const idl = liste.id;
      const r = await listes.executer('Retrait de la liste', () => retirerLivresListe(idl, ids));
      if (r) sel.vider();
      return;
    }
    if (id === 'infos') infosDe = ids;
    else if (id === 'conversion' || id === 'ecritureInfos') operation = { type: id, ids };
    else if (id === 'renommer') renommerDe = ids;
    else if (id === 'dossier') {
      try {
        await ouvrirDossierFichier(ids[0]);
      } catch (e) {
        erreurAction = `Ouvrir le dossier : ${messageErreur(e)}`;
      }
    }
  }

  /** Précédent / Suivant de « Infos du livre » : la sélection suit, comme dans ComicRack. */
  async function voisin(pas: -1 | 1): Promise<string | null> {
    const i = sel.curseur + pas;
    if (sel.curseur < 0 || i < 0 || i >= bib.total) return null;
    const [l] = await bib.livresDe([i]);
    if (!l) return null;
    sel.unique(i);
    document.querySelector(`[data-index="${i}"]`)?.scrollIntoView({ block: 'nearest' });
    return l.id;
  }
  const avoisin = (pas: -1 | 1) => sel.curseur + pas >= 0 && sel.curseur + pas < bib.total;

  // ───── Listes (étape 5) : glisser-déposer, en-têtes de groupe, ajout à une liste ─────

  /** Réordonner dans la vue : seulement une liste de lecture triée dans l'ordre de la liste, croissant. */
  function refusReordonner(): string | null {
    if (!liste || liste.type !== 'lecture') return DEPOSER_SUR_LECTURE;
    // Lecture seule d'abord : inutile de proposer de trier quand rien ne peut s'écrire.
    if (listes.raison) return listes.raison;
    if (bib.params.tri !== 'ordre' || bib.params.sensTri !== 'asc') return TRIER_POUR_REORDONNER;
    return null;
  }

  /** Requête de la vue affichée : le backend refait la même vue pour traduire rangs et groupes. */
  const requete = (): RequeteLivres => ({ ...bib.params, debut: 0, nombre: TAILLE_PAGE });

  /** Groupes de la vue (`groupes_vue`) avec le premier et le dernier index de chacun. */
  type GroupeIndexe = { titre: string; rang: number; debut: number; fin: number };
  function indexer(g: Array<{ titre: string; nombre: number }>): GroupeIndexe[] {
    let debut = 0;
    return g.map((x, rang) => {
      const r = { titre: x.titre, rang, debut, fin: debut + x.nombre - 1 };
      debut += x.nombre;
      return r;
    });
  }
  const trouverGroupe = (g: Array<{ titre: string; nombre: number }>, i: number) => indexer(g).find((x) => i >= x.debut && i <= x.fin);
  /** Groupes connus pendant un glisser de séries entières. */
  let groupesGlisser: GroupeIndexe[] | null = null;

  /** Sélectionne les groupes nommés (après un déplacement de séries). */
  async function selectionnerGroupes(titres: string[]) {
    const g = indexer(await groupesVue(requete())).filter((x) => titres.includes(x.titre));
    g.forEach((x, k) => sel.plage(x.debut, x.fin, k > 0));
  }

  /** Dernière sélection d'en-tête de série en cours : le glisser l'attend avant de démarrer (L-08). */
  let selectionGroupeEnCours: Promise<void> = Promise.resolve();
  function cliquerGroupe(i: number, m: Modificateurs) {
    selectionGroupeEnCours = selectionnerGroupe(i, m);
  }

  /** Sélectionne tout le groupe (série) qui contient `i` (Ctrl ou Maj : ajouté à la sélection). */
  async function selectionnerGroupe(i: number, m: Modificateurs) {
    try {
      const a = await bib.debutDuGroupe(i);
      const b = await bib.finGroupe(a);
      sel.plage(a, b, m.ctrl || m.maj);
    } catch (e) {
      erreurAction = messageErreur(e);
    }
  }

  async function ajouterA(idListe: string, indices: number[]) {
    try {
      await ajouterIdsA(idListe, (await bib.livresDe(indices)).map((l) => l.id));
    } catch (e) {
      erreurAction = messageErreur(e);
    }
  }

  async function ajouterIdsA(idListe: string, ids: string[]) {
    if (await listes.executer('Ajout à la liste', () => ajouterLivresListe(idListe, ids))) listes.derniereAjout = idListe;
  }

  /** Glisser dans la vue : les livres `indices` vont au rang d'insertion `avant` (le backend calcule
   *  le repère et la position dans la liste). */
  function deplacer(indices: number[], avant: number) {
    if (!liste) return;
    const idl = liste.id;
    const req = requete();
    void (async () => {
      try {
        const ids = (await bib.livresDe(indices)).map((x) => x.id);
        const r = await listes.executer('Réordonnancement de la liste', () => deplacerLivresListe(idl, ids, req, avant));
        if (!r) return;
        // Les livres déplacés restent sélectionnés, à leur nouvelle place.
        const debut = avant - indices.filter((k) => k < avant).length;
        sel.plage(debut, debut + indices.length - 1, false);
      } catch (e) {
        erreurAction = messageErreur(e);
      }
    })();
  }

  /** Séries entières déposées à la frontière `insertion` : les groupes entièrement sélectionnés. */
  function deplacerGroupes(insertion: number) {
    if (!liste || !groupesGlisser) return;
    const idl = liste.id;
    const req = requete();
    const titres = groupesGlisser.filter((g) => {
      for (let i = g.debut; i <= g.fin; i++) if (!sel.contient(i)) return false;
      return true;
    }).map((g) => g.titre);
    if (!titres.length) return;
    void (async () => {
      const r = await listes.executer('Réordonnancement de la liste', () => deplacerGroupesListe(idl, req, titres, insertion));
      if (r) await selectionnerGroupes(titres).catch(() => {});
    })();
  }

  const glisser = new GlisserLivres({
    debut: (groupes) => {
      listes.viderDepots();
      groupesGlisser = null;
      if (groupes) {
        void groupesVue(requete()).then((g) => {
          groupesGlisser = indexer(g);
          glisser.recalculer();
        }, (e) => (erreurAction = messageErreur(e)));
      }
    },
    groupeDe: (i) => groupesGlisser?.find((g) => i >= g.debut && i <= g.fin) ?? null,
    deplacerGroupes,
    contient: (i) => sel.contient(i),
    choisir: (i) => sel.unique(i),
    selectionPrete: () => selectionGroupeEnCours,
    indices: () => sel.indices(),
    series: (ind) => {
      if (bib.params.groupe !== 'serie') return 0;
      return new Set(ind.map((i) => bib.livre(i)).filter((l) => !!l).map((l) => bib.nomGroupe(l!))).size;
    },
    surListe: (id) => ({ refus: listes.depot(id), nom: listes.noeud(id)?.nom ?? '' }),
    refusReordonner,
    horizontal: () => vue === 'grille',
    zone: () => document.querySelector<HTMLElement>('[data-testid="grille"], [data-testid="tableau"], [data-testid="grille-piles"]'),
    ajouter: (idListe, indices) => void ajouterA(idListe, indices),
    deplacer,
  });
  // Glisser une pile vers une liste : tous ses livres sont ajoutés (pas de réordonnancement de piles).
  const glisserPile = new GlisserLivres({
    debut: () => listes.viderDepots(),
    groupeDe: () => null,
    deplacerGroupes: () => {},
    contient: (i) => bib.piles[i] !== undefined && bib.piles[i].serie === pileSelectionnee,
    choisir: (i) => {
      if (bib.piles[i]) pileSel = bib.piles[i].serie;
    },
    selectionPrete: () => Promise.resolve(),
    indices: () => {
      const i = bib.piles.findIndex((p) => p.serie === pileSelectionnee);
      return i >= 0 ? [i] : [];
    },
    nombreLivres: (ind) => ind.reduce((n, i) => n + (bib.piles[i]?.nbLivres ?? 0), 0),
    series: () => 0,
    surListe: (id) => ({ refus: listes.depot(id), nom: listes.noeud(id)?.nom ?? '' }),
    refusReordonner: () => 'Dépose la pile sur une liste de lecture, dans le panneau des listes.',
    horizontal: () => true,
    zone: () => document.querySelector<HTMLElement>('[data-testid="grille-piles"]'),
    ajouter: (idListe, ind) => void ajouterIdsA(idListe, ind.flatMap((i) => bib.piles[i]?.ids ?? [])),
    deplacer: () => {},
  });
  $effect(() => {
    listes.survolDepot = glisserPile.actif ? glisserPile.survol : glisser.survol;
  });
  // Réponse de depot_livres_possible arrivée pendant le glisser : badge et mise en évidence à jour.
  $effect(() => {
    void listes.depots.size;
    untrack(() => {
      glisser.recalculer();
      glisserPile.recalculer();
    });
  });

  // Fenêtre « Ajouter à une liste ».
  let ajoutDe = $state<string[] | null>(null);
  let choixAjout = $state('');
  // Dernière liste de lecture affichée : proposée par « Ajouter à une liste… » depuis la Bibliothèque.
  $effect(() => {
    const c = listes.courante;
    if (c && listes.noeud(c)?.type === 'lecture') listes.derniereOuverte = c;
  });
  let nomNouvelle = $state('');
  async function validerAjout() {
    const ids = ajoutDe;
    if (!ids) return;
    ajoutDe = null;
    if (choixAjout === '+') {
      const nom = nomNouvelle.trim();
      if (!nom) return;
      await listes.executer('Création de la liste', () => creerListe({ type: 'lecture', nom, livres: ids }));
    } else if (choixAjout) {
      const idl = choixAjout;
      listes.derniereAjout = idl;
      await listes.executer('Ajout à la liste', () => ajouterLivresListe(idl, ids));
    }
  }

  const nomListe = $derived(liste ? `${glypheListe(liste)} ${liste.nom}` : '');

  async function fenetreFermee() {
    infosDe = null;
    operation = null;
    renommerDe = null;
    await tick();
    document.querySelector<HTMLElement>('[data-testid="grille"], [data-testid="tableau"], [data-testid="grille-piles"]')?.focus();
  }
</script>

<svelte:window onkeydown={clavierGlobal} />

<div class="vue">
  <header class="outils" role="toolbar" aria-label="Outils de la bibliothèque">
    {#if liste}<h1 class="titre-liste" title={liste.description || liste.nom} data-testid="titre-liste">{nomListe}</h1>{/if}
    <input
      bind:this={champRecherche}
      class="recherche"
      type="search"
      placeholder="Rechercher (titre, série, éditeur)…  /"
      aria-label="Rechercher dans la bibliothèque"
      value={saisie}
      oninput={(e) => recherche(e.currentTarget.value)}
      onkeydown={clavierRecherche}
      data-testid="recherche"
    />
    <label class="outil">Tri
      <select value={bib.params.tri} onchange={(e) => changer({ tri: e.currentTarget.value as ClefTri })} data-testid="tri">
        {#if liste?.type === 'lecture'}<option value="ordre"># Ordre de la liste</option>{/if}
        {#each tris as t (t.id)}<option value={t.id}>{t.titre}</option>{/each}
      </select>
    </label>
    <button
      onclick={() => changer({ sensTri: bib.params.sensTri === 'asc' ? 'desc' : 'asc' })}
      aria-label={bib.params.sensTri === 'asc' ? 'Tri croissant (cliquer pour décroissant)' : 'Tri décroissant (cliquer pour croissant)'}
      title="Sens du tri"
    >{bib.params.sensTri === 'asc' ? '▲' : '▼'}</button>
    <label class="outil">Groupe
      <select value={bib.params.groupe} onchange={(e) => changer({ groupe: e.currentTarget.value as Groupe })} data-testid="groupe">
        {#each groupes as g (g.id)}<option value={g.id}>{g.titre}</option>{/each}
      </select>
    </label>
    {#if vue === 'grille'}
      <button
        aria-pressed={empiler}
        class:actif={empiler}
        onclick={() => (empiler = !empiler)}
        title="Une pile par série : un clic la sélectionne, un double-clic (ou Entrée) l'ouvre"
        data-testid="empiler"
      >📚 Empiler par série</button>
    {/if}
    <span class="espace"></span>
    {#if vue === 'grille'}
      <label class="outil">Taille
        <input type="range" min="100" max="260" step="10" bind:value={zoom} aria-label="Taille des couvertures" />
      </label>
    {/if}
    <div class="groupe-boutons" role="group" aria-label="Type de vue">
      <button aria-pressed={vue === 'grille'} class:actif={vue === 'grille'} onclick={() => choisirVue('grille')} title="Grille de couvertures">▦ Grille</button>
      <button aria-pressed={vue === 'tableau'} class:actif={vue === 'tableau'} onclick={() => choisirVue('tableau')} title="Tableau">☰ Tableau</button>
    </div>
    <button
      aria-pressed={details}
      class:actif={details}
      onclick={() => (details = !details)}
      title="Afficher ou masquer le panneau de détails"
    >Détails</button>
    <button onclick={() => dialogueFichier?.showModal()} title="Lire un fichier qui n'est pas dans la bibliothèque" data-testid="ouvrir-fichier">Ouvrir un fichier…</button>
  </header>

  <div class="corps">
    <div class="liste">
      {#if bib.erreur}
        <p class="erreur" role="alert">{bib.erreur}</p>
      {:else if bib.total === 0 && bib.version > 0}
        <p class="vide">{bib.params.recherche ? 'Aucun livre ne correspond à la recherche.'
          : liste?.type === 'lecture' ? 'Liste vide : glisse des livres de la bibliothèque sur son nom, dans le panneau des listes.'
          : liste?.inconnu ? `ComicRust ne connaît pas encore le contenu de cette liste : ${liste.inconnu}.`
          : liste ? 'Aucun livre dans cette liste.'
          : 'La bibliothèque est vide : ajoutez un dossier surveillé, puis lancez un scan.'}</p>
      {:else if enPiles && !pileOuverte}
        <GrillePiles piles={bib.piles} selection={pileSelectionnee} version={bib.version} {zoom}
          onselection={(serie) => (pileSel = serie)} onouvrir={ouvrirPile}
          onmenu={(serie, x, y) => (menu = { x, y, pile: serie })}
          onappui={(e, i) => glisserPile.appui(e, i)} />
      {:else if vue === 'grille'}
        {#if pileOuverte}
          <div class="barre-pile" data-testid="barre-pile">
            <button onclick={() => void fermerPile()} title="Revenir aux piles (Échap)" data-testid="fermer-pile">← Piles</button>
            <strong class="nom-pile">📚 {bib.params.serie}</strong>
            <span class="discret">{bib.total.toLocaleString('fr-FR')} livre{bib.total > 1 ? 's' : ''}</span>
          </div>
        {/if}
        <GrilleCouvertures {bib} {sel} {zoom} onmenu={(_, x, y) => (menu = { x, y })} onouvrir={ouvrir}
          onappui={(e, i, g) => glisser.appui(e, i, g)} ongroupe={cliquerGroupe} />
      {:else}
        <VueTableau {bib} {sel} onmenu={(_, x, y) => (menu = { x, y })} onouvrir={ouvrir} ontri={trier}
          onappui={(e, i, g) => glisser.appui(e, i, g)} ongroupe={cliquerGroupe} />
      {/if}
    </div>
    {#if details}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <div
        class="poignee"
        role="separator"
        aria-orientation="vertical"
        aria-label="Largeur du panneau de détails"
        aria-valuemin={LARGEUR_MIN}
        aria-valuemax={LARGEUR_MAX}
        aria-valuenow={largeurDetails}
        tabindex="0"
        title="Tirer pour élargir ou réduire le panneau de détails"
        {@attach ecouteursPoignee}
        data-testid="poignee-details"
      ></div>
      <PanneauDetails livre={pilesAffichees ? undefined : livreSelectionne} largeur={largeurDetails} onouvrir={() => ouvrir(sel.curseur)}
        onmodifier={() => void action('infos')} nbSelection={pilesAffichees ? 0 : nbSel} rafraichir={fichesModifiees} />
    {/if}
  </div>

  <footer class="etat" aria-live="polite">
    <span data-testid="compteur">
      {#if bib.listeChargee !== (liste?.id ?? undefined)}Chargement…{:else}
        {bib.total.toLocaleString('fr-FR')} livre{bib.total > 1 ? 's' : ''}{liste ? ` dans « ${liste.nom} »` : ''}{bib.params.recherche ? ' (filtré)' : ''}
      {/if}
      {#if pilesAffichees}· {bib.piles.length.toLocaleString('fr-FR')} pile{bib.piles.length > 1 ? 's' : ''}{#if pileSelectionnee !== null} · pile « {pileSelectionnee} »{/if}
      {:else if nbSel > 1}· {nbSel.toLocaleString('fr-FR')} sélectionnés{:else if sel.curseur >= 0}· sélection {(sel.curseur + 1).toLocaleString('fr-FR')}{/if}
    </span>
    {#if erreurAction}<span class="alerte" role="alert">{erreurAction}</span>{/if}
    <span class="espace"></span>
    {#if scan.enCours}
      <span class="scan">
        Scan {scan.fait.toLocaleString('fr-FR')} / {scan.total.toLocaleString('fr-FR')}
        <span class="barre-progression petite" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={scan.total ? Math.round((scan.fait / scan.total) * 100) : 0} aria-label="Progression du scan"
          ><span style:width="{scan.total ? (scan.fait / scan.total) * 100 : 0}%"></span></span>
      </span>
    {:else if scan.erreur}
      <span class="alerte">Échec du scan : {scan.erreur}</span>
    {:else if scan.resultat}
      <span>Scan terminé : {scan.resultat.ajoutes} ajoutés, {scan.resultat.misAJour} mis à jour, {scan.resultat.manquants} manquants</span>
    {/if}
  </footer>
</div>

<dialog bind:this={dialogueFichier} use:modale={{ peutFermer: () => !cheminFichier.trim() }} aria-labelledby="titre-ouvrir-fichier" data-testid="dialogue-fichier">
  <h2 id="titre-ouvrir-fichier">Ouvrir un fichier</h2>
  <form class="ouvrir-fichier" onsubmit={(e) => { e.preventDefault(); ouvrirFichier(); }}>
    <label for="chemin-fichier">Chemin d'une BD (CBZ, CBR, CB7, PDF…), d'un dossier d'images ou d'une image :</label>
    <input id="chemin-fichier" type="text" bind:value={cheminFichier} placeholder="D:\BD\Thanos 01.cbz" spellcheck="false" autocomplete="off" data-testid="chemin-fichier" />
    <div class="pied">
      <button type="button" onclick={() => fermerModale(dialogueFichier)}>Annuler</button>
      <button type="submit" disabled={!cheminFichier.trim()}>Ouvrir</button>
    </div>
  </form>
</dialog>

{#if glisser.actif || glisserPile.actif}
  {@const g = glisserPile.actif ? glisserPile : glisser}
  {@const b = g.badge}
  <div class="badge-glisser" class:refus={b.refus} style:left="{g.x + 14}px" style:top="{g.y + 16}px" data-testid="badge-glisser" aria-hidden="true">
    <div>{b.quoi}</div>
    {#if b.action}<div class="action">{b.action}</div>{/if}
  </div>
  {#if g.trait}
    <div class="trait-insertion" style:left="{g.trait.x}px" style:top="{g.trait.y}px" style:width="{g.trait.l}px" style:height="{g.trait.h}px" data-testid="trait-insertion"></div>
  {/if}
{/if}

{#if ajoutDe}
  <dialog use:modale={{ ouvrirAuMontage: true, fermer: () => (ajoutDe = null) }} aria-labelledby="titre-ajout" data-testid="dialogue-ajout-liste">
    <h2 id="titre-ajout">Ajouter {ajoutDe.length > 1 ? `${ajoutDe.length.toLocaleString('fr-FR')} livres` : 'le livre'} à une liste</h2>
    <form class="ouvrir-fichier" onsubmit={(e) => { e.preventDefault(); void validerAjout(); }}>
      <label for="choix-liste">Liste de lecture :</label>
      <select id="choix-liste" bind:value={choixAjout} data-testid="choix-liste">
        {#each listes.listesDeLecture() as l (l.id)}<option value={l.id}>{l.nom}</option>{/each}
        <option value="+">Nouvelle liste de lecture…</option>
      </select>
      {#if choixAjout === '+'}
        <label for="nom-nouvelle">Nom de la nouvelle liste :</label>
        <input id="nom-nouvelle" type="text" bind:value={nomNouvelle} spellcheck="false" />
      {/if}
      <div class="pied">
        <button type="button" onclick={() => (ajoutDe = null)}>Annuler</button>
        <button type="submit" disabled={!choixAjout || (choixAjout === '+' && !nomNouvelle.trim())} data-testid="valider-ajout">Ajouter</button>
      </div>
    </form>
  </dialog>
{/if}

{#if menu}
  {@const pile = menu.pile}
  <MenuContextuel x={menu.x} y={menu.y} entrees={pile !== undefined ? entreesPile(pile) : entreesMenu}
    onchoix={(id) => void (pile !== undefined ? actionPile(pile, id) : action(id))} onfermer={() => (menu = null)} />
{/if}
{#if infosDe}
  <DialogInfos ids={infosDe} onvoisin={voisin} {avoisin} onfermer={fenetreFermee} />
{/if}
{#if operation}
  <DialogOperation operation={operation.type} ids={operation.ids} onfermer={fenetreFermee} />
{/if}
{#if renommerDe}
  <DialogRenommer ids={renommerDe} onfermer={fenetreFermee} />
{/if}

{#if livreLu}
  {#key livreLu}
    <Lecteur source={livreLu} onfermer={lecteurFerme} />
  {/key}
{/if}

<style>
  .vue { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .ouvrir-fichier { display: grid; gap: 8px; min-width: min(520px, calc(100vw - 64px)); }
  .ouvrir-fichier .pied { display: flex; justify-content: flex-end; gap: 8px; }
  .outils {
    display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 8px 12px;
    background: var(--cr-fond-panneau); border-bottom: 1px solid var(--cr-bordure);
  }
  .recherche { width: min(340px, 100%); }
  .titre-liste { margin: 0 6px 0 0; font-size: 15px; font-weight: 700; max-width: 260px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .barre-pile {
    display: flex; align-items: center; gap: 10px; padding: 6px 12px;
    background: var(--cr-fond-panneau); border-bottom: 1px solid var(--cr-bordure);
  }
  .nom-pile { max-width: 50%; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .discret { color: var(--cr-texte-discret); }
  .badge-glisser {
    position: fixed; z-index: 60; pointer-events: none; padding: 5px 9px; font-size: 12px; max-width: 340px;
    background: var(--cr-fond-panneau); border: 1px solid var(--cr-accent); border-radius: var(--cr-rayon); box-shadow: var(--cr-ombre);
  }
  .badge-glisser .action { color: var(--cr-accent); font-weight: 600; }
  .badge-glisser.refus { border-color: var(--cr-erreur); }
  .badge-glisser.refus .action { color: var(--cr-erreur); }
  .trait-insertion { position: fixed; z-index: 59; pointer-events: none; background: var(--cr-accent); border-radius: 2px; }
  :global(body.glisser-livres), :global(body.glisser-livres *) { cursor: grabbing !important; }
  .outil { display: flex; align-items: center; gap: 6px; color: var(--cr-texte-discret); }
  .outil input[type='range'] { width: 90px; padding: 0; accent-color: var(--cr-accent); }
  .espace { flex: 1; }
  .groupe-boutons { display: flex; }
  .groupe-boutons button:first-child { border-top-right-radius: 0; border-bottom-right-radius: 0; }
  .groupe-boutons button:last-child { border-top-left-radius: 0; border-bottom-left-radius: 0; margin-left: -1px; }
  button.actif { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); }
  button.actif:hover:not(:disabled) { background: var(--cr-accent); }
  .corps { flex: 1; min-height: 0; display: flex; }
  .poignee { flex: none; width: 5px; margin-right: -5px; position: relative; z-index: 1; cursor: col-resize; touch-action: none; }
  .poignee:hover, .poignee:focus-visible { background: var(--cr-accent); outline: none; }
  .liste { flex: 1; min-width: 0; display: flex; flex-direction: column; background: var(--cr-fond); }
  .vide, .erreur { margin: 40px auto; color: var(--cr-texte-discret); text-align: center; max-width: 420px; }
  .erreur { color: var(--cr-erreur); }
  .etat {
    display: flex; align-items: center; gap: 16px; padding: 4px 12px; min-height: 28px; font-size: 12px;
    background: var(--cr-fond-panneau); border-top: 1px solid var(--cr-bordure); color: var(--cr-texte-discret);
  }
  .alerte { color: var(--cr-erreur); }
  .scan { display: flex; align-items: center; gap: 8px; }
  .petite { display: inline-block; width: 140px; height: 8px; overflow: hidden; background: var(--cr-fond-survol); border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); }
  .petite > span { display: block; height: 100%; background: var(--cr-accent); }
</style>
