<script lang="ts">
  // Vue Bibliothèque : barre d'outils (recherche, tri, groupement, vue), grille ou tableau
  // virtualisés, panneau de détails et barre d'état. Les données viennent de lister_livres.
  import { onMount, tick } from 'svelte';
  import { ecouter, messageErreur, ouvrirDossierFichier, type BibliothequeModifiee, type ClefTri, type Groupe, type SourceLecteur } from '../lib/api';
  import { Bibliotheque, type SuiviScan } from '../lib/bibliotheque.svelte';
  import { enregistrerPreference, preferences } from '../lib/prefs';
  import { Selection } from '../lib/selection.svelte';
  import DialogInfos from './DialogInfos.svelte';
  import { fermerModale, modale } from '../lib/modale';
  import DialogOperation from './DialogOperation.svelte';
  import DialogRenommer from './DialogRenommer.svelte';
  import GrilleCouvertures from './GrilleCouvertures.svelte';
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
  }
  let { scan, nbLivres = $bindable(), rafraichir }: Props = $props();

  // Préférences mémorisées (data\comicrust-tauri.json), chargées par App avant ce composant :
  // le premier lister_livres part directement avec le tri et le groupe enregistrés.
  const prefs = preferences();
  const bib = new Bibliotheque({ tri: prefs.tri, sensTri: prefs.sensTri, groupe: prefs.groupe });
  let vue = $state<'grille' | 'tableau'>(prefs.vue ?? 'grille');
  let zoom = $state(prefs.zoom ?? 150);
  let details = $state(prefs.details ?? true);
  const LARGEUR_MIN = 220;
  const LARGEUR_MAX = 800;
  let largeurDetails = $state(prefs.largeurDetails ?? 320);
  /** Sélection multiple (Ctrl+clic, Maj+clic, Ctrl+A, Maj+flèches) ; `sel.curseur` : livre courant. */
  const sel = new Selection();
  /** Menu contextuel ouvert (position). */
  let menu = $state<{ x: number; y: number } | null>(null);
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

  // Toute modification d'un réglage d'affichage est enregistrée (avec délai, voir prefs.ts).
  $effect(() => {
    enregistrerPreference({
      tri: bib.params.tri, sensTri: bib.params.sensTri, groupe: bib.params.groupe,
      vue, zoom, details, largeurDetails,
    });
  });

  // Fin de scan : on recharge sans toucher au défilement ni à la sélection.
  let derniereVersion = -1;
  $effect(() => {
    const v = rafraichir;
    if (derniereVersion >= 0 && v !== derniereVersion) void bib.reinitialiser(false);
    derniereVersion = v;
  });

  $effect(() => {
    if (bib.params.recherche === '') nbLivres = bib.total;
    sel.borner(bib.total);
  });

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
    document.querySelector<HTMLElement>('[data-testid="grille"], [data-testid="tableau"]')?.focus();
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
      void action('infos');
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
        document.querySelector<HTMLElement>('[data-testid="grille"], [data-testid="tableau"]')?.focus();
      }
    }
  }

  const livreSelectionne = $derived(sel.curseur >= 0 ? bib.livre(sel.curseur) : undefined);
  const nbSel = $derived(sel.nombre);

  // ───── Édition (étape 3) : menu contextuel et fenêtres ─────

  const entreesMenu = $derived.by<EntreeMenu[]>(() => {
    const plusieurs = nbSel > 1;
    return [
      { id: 'lire', libelle: 'Lire', raccourci: 'Entrée', desactive: plusieurs, aide: plusieurs ? 'Un seul livre à la fois' : undefined },
      { id: 'infos', libelle: plusieurs ? `✏ Modifier les infos des ${nbSel.toLocaleString('fr-FR')} livres…` : '✏ Modifier les infos…', raccourci: 'Ctrl+I', separe: true },
      { id: 'ecritureInfos', libelle: plusieurs ? 'Écrire les infos dans les fichiers…' : 'Écrire les infos dans le fichier', separe: true,
        aide: 'Seuls les CBZ reçoivent les infos (convertir d\'abord en CBZ)' },
      { id: 'conversion', libelle: plusieurs ? 'Convertir les CBR / CB7 / CBT en CBZ…' : 'Convertir en CBZ', aide: 'Seuls les CBR, CB7 et CBT se convertissent' },
      { id: 'renommer', libelle: 'Renommer / déplacer…' },
      { id: 'dossier', libelle: 'Ouvrir le dossier du fichier', separe: true, desactive: plusieurs,
        aide: plusieurs ? 'Un seul livre à la fois' : 'Montre le fichier dans l\'Explorateur de Windows' },
    ];
  });

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
    let ids: string[];
    try {
      ids = await idsSelection();
    } catch (e) {
      erreurAction = messageErreur(e);
      return;
    }
    if (ids.length === 0) return;
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

  async function fenetreFermee() {
    infosDe = null;
    operation = null;
    renommerDe = null;
    await tick();
    document.querySelector<HTMLElement>('[data-testid="grille"], [data-testid="tableau"]')?.focus();
  }
</script>

<svelte:window onkeydown={clavierGlobal} />

<div class="vue">
  <header class="outils" role="toolbar" aria-label="Outils de la bibliothèque">
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
        <p class="vide">{bib.params.recherche ? 'Aucun livre ne correspond à la recherche.' : 'La bibliothèque est vide : ajoutez un dossier surveillé, puis lancez un scan.'}</p>
      {:else if vue === 'grille'}
        <GrilleCouvertures {bib} {sel} {zoom} onmenu={(_, x, y) => (menu = { x, y })} onouvrir={ouvrir} />
      {:else}
        <VueTableau {bib} {sel} onmenu={(_, x, y) => (menu = { x, y })} onouvrir={ouvrir} ontri={trier} />
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
      <PanneauDetails livre={livreSelectionne} largeur={largeurDetails} onouvrir={() => ouvrir(sel.curseur)}
        onmodifier={() => void action('infos')} nbSelection={nbSel} rafraichir={fichesModifiees} />
    {/if}
  </div>

  <footer class="etat" aria-live="polite">
    <span data-testid="compteur">
      {bib.total.toLocaleString('fr-FR')} livre{bib.total > 1 ? 's' : ''}{bib.params.recherche ? ' (filtré)' : ''}
      {#if nbSel > 1}· {nbSel.toLocaleString('fr-FR')} sélectionnés{:else if sel.curseur >= 0}· sélection {(sel.curseur + 1).toLocaleString('fr-FR')}{/if}
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

{#if menu}
  <MenuContextuel x={menu.x} y={menu.y} entrees={entreesMenu} onchoix={(id) => void action(id)} onfermer={() => (menu = null)} />
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
