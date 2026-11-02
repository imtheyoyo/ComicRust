<script lang="ts">
  // Vue Bibliothèque : barre d'outils (recherche, tri, groupement, vue), grille ou tableau
  // virtualisés, panneau de détails et barre d'état. Les données viennent de lister_livres.
  import { onMount } from 'svelte';
  import type { ClefTri, Groupe } from '../lib/api';
  import { Bibliotheque, type SuiviScan } from '../lib/bibliotheque.svelte';
  import { ecrirePref, lirePref } from '../lib/prefs';
  import GrilleCouvertures from './GrilleCouvertures.svelte';
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

  const bib = new Bibliotheque();
  let vue = $state<'grille' | 'tableau'>(lirePref('vue', 'grille'));
  let zoom = $state(lirePref('zoom', 150));
  let details = $state(lirePref('details', true));
  let selection = $state(-1);
  let message = $state('');
  let champRecherche = $state<HTMLInputElement>();
  let saisie = $state('');
  let minuterie: ReturnType<typeof setTimeout> | undefined;
  let minuterieMessage: ReturnType<typeof setTimeout> | undefined;

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
    return () => { clearTimeout(minuterie); clearTimeout(minuterieMessage); };
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
    if (selection >= bib.total) selection = bib.total - 1;
  });

  function changer(patch: Partial<typeof bib.params>) {
    Object.assign(bib.params, patch);
    selection = -1;
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
    message = `Lecteur : disponible à l'étape 2 (« ${l.titre} »).`;
    clearTimeout(minuterieMessage);
    minuterieMessage = setTimeout(() => (message = ''), 4000);
  }

  function choisirVue(v: 'grille' | 'tableau') {
    vue = v;
    ecrirePref('vue', v);
  }

  function clavierGlobal(e: KeyboardEvent) {
    const saisieTexte = (e.target as HTMLElement | null)?.closest('input, textarea, select');
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

  const livreSelectionne = $derived(selection >= 0 ? bib.livre(selection) : undefined);
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
        <input type="range" min="100" max="260" step="10" bind:value={zoom} onchange={() => ecrirePref('zoom', zoom)} aria-label="Taille des couvertures" />
      </label>
    {/if}
    <div class="groupe-boutons" role="group" aria-label="Type de vue">
      <button aria-pressed={vue === 'grille'} class:actif={vue === 'grille'} onclick={() => choisirVue('grille')} title="Grille de couvertures">▦ Grille</button>
      <button aria-pressed={vue === 'tableau'} class:actif={vue === 'tableau'} onclick={() => choisirVue('tableau')} title="Tableau">☰ Tableau</button>
    </div>
    <button
      aria-pressed={details}
      class:actif={details}
      onclick={() => { details = !details; ecrirePref('details', details); }}
      title="Afficher ou masquer le panneau de détails"
    >Détails</button>
  </header>

  <div class="corps">
    <div class="liste">
      {#if bib.erreur}
        <p class="erreur" role="alert">{bib.erreur}</p>
      {:else if bib.total === 0 && bib.version > 0}
        <p class="vide">{bib.params.recherche ? 'Aucun livre ne correspond à la recherche.' : 'La bibliothèque est vide : ajoutez un dossier surveillé, puis lancez un scan.'}</p>
      {:else if vue === 'grille'}
        <GrilleCouvertures {bib} {selection} {zoom} onselection={(i) => (selection = i)} onouvrir={ouvrir} />
      {:else}
        <VueTableau {bib} {selection} onselection={(i) => (selection = i)} onouvrir={ouvrir} ontri={trier} />
      {/if}
    </div>
    {#if details}
      <PanneauDetails livre={livreSelectionne} onouvrir={() => ouvrir(selection)} />
    {/if}
  </div>

  <footer class="etat" aria-live="polite">
    <span data-testid="compteur">
      {bib.total.toLocaleString('fr-FR')} livre{bib.total > 1 ? 's' : ''}{bib.params.recherche ? ' (filtré)' : ''}
      {#if selection >= 0}· sélection {(selection + 1).toLocaleString('fr-FR')}{/if}
    </span>
    {#if message}<span class="message">{message}</span>{/if}
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

<style>
  .vue { flex: 1; min-width: 0; display: flex; flex-direction: column; }
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
  .liste { flex: 1; min-width: 0; display: flex; flex-direction: column; background: var(--cr-fond); }
  .vide, .erreur { margin: 40px auto; color: var(--cr-texte-discret); text-align: center; max-width: 420px; }
  .erreur { color: var(--cr-erreur); }
  .etat {
    display: flex; align-items: center; gap: 16px; padding: 4px 12px; min-height: 28px; font-size: 12px;
    background: var(--cr-fond-panneau); border-top: 1px solid var(--cr-bordure); color: var(--cr-texte-discret);
  }
  .message { color: var(--cr-texte); }
  .alerte { color: var(--cr-erreur); }
  .scan { display: flex; align-items: center; gap: 8px; }
  .petite { display: inline-block; width: 140px; height: 8px; overflow: hidden; background: var(--cr-fond-survol); border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); }
  .petite > span { display: block; height: 100%; background: var(--cr-accent); }
</style>
