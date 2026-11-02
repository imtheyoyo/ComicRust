<script lang="ts">
  // Tableau virtualisé (lignes de hauteur fixe) : mêmes données paginées que la grille.
  import type { ClefTri } from '../lib/api';
  import type { Bibliotheque } from '../lib/bibliotheque.svelte';
  import { cibleClavier } from '../lib/navigation';

  interface Props {
    bib: Bibliotheque;
    selection: number;
    onselection: (i: number) => void;
    onouvrir: (i: number) => void;
    ontri: (clef: ClefTri) => void;
  }
  let { bib, selection, onselection, onouvrir, ontri }: Props = $props();

  const HAUT_LIGNE = 40;
  const HAUT_ENTETE = 34;
  const SURPLUS = 6;

  const colonnes: Array<{ clef: ClefTri | null; titre: string; classe?: string }> = [
    { clef: null, titre: '' },
    { clef: 'titre', titre: 'Titre' },
    { clef: 'serie', titre: 'Série' },
    { clef: null, titre: 'N°', classe: 'num' },
    { clef: null, titre: 'Éditeur' },
    { clef: 'parution', titre: 'Année', classe: 'num' },
    { clef: 'pages', titre: 'Pages', classe: 'num' },
    { clef: null, titre: 'Lu', classe: 'num' },
  ];

  let zone = $state<HTMLDivElement>();
  let hauteur = $state(600);
  let haut = $state(0);

  const premier = $derived(Math.max(0, Math.floor(haut / HAUT_LIGNE) - SURPLUS));
  const dernier = $derived(Math.min(bib.total - 1, Math.ceil((haut + hauteur) / HAUT_LIGNE) + SURPLUS));
  const indices = $derived.by(() => {
    const liste: number[] = [];
    for (let i = premier; i <= dernier; i++) liste.push(i);
    return liste;
  });

  $effect(() => {
    if (bib.total > 0) bib.assurer(premier, dernier);
  });
  $effect(() => {
    bib.version;
    if (zone) zone.scrollTop = 0;
  });

  function afficher(i: number) {
    if (!zone) return;
    const h = i * HAUT_LIGNE;
    const visible = hauteur - HAUT_ENTETE;
    if (h < zone.scrollTop) zone.scrollTop = h;
    else if (h + HAUT_LIGNE > zone.scrollTop + visible) zone.scrollTop = h + HAUT_LIGNE - visible;
  }

  function clavier(e: KeyboardEvent) {
    if (e.key === 'Enter' && selection >= 0) {
      e.preventDefault();
      onouvrir(selection);
      return;
    }
    const lignesPage = Math.max(1, Math.floor((hauteur - HAUT_ENTETE) / HAUT_LIGNE) - 1);
    const cible = cibleClavier(e.key, selection, bib.total, 1, lignesPage);
    if (cible === null) return;
    e.preventDefault();
    onselection(cible);
    afficher(cible);
  }

  const fleche = (clef: ClefTri | null) =>
    clef && bib.params.tri === clef ? (bib.params.sensTri === 'asc' ? ' ▲' : ' ▼') : '';
</script>

<div
  class="zone"
  bind:this={zone}
  bind:clientHeight={hauteur}
  onscroll={() => (haut = zone!.scrollTop)}
  onkeydown={clavier}
  role="grid"
  tabindex="0"
  aria-label="Tableau des livres"
  aria-rowcount={bib.total}
  data-testid="tableau"
>
  <div class="entete" role="row" style:height="{HAUT_ENTETE}px">
    {#each colonnes as c (c.titre)}
      {#if c.clef}
        {@const clef = c.clef}
        <button class="ch {c.classe ?? ''}" role="columnheader" onclick={() => ontri(clef)} tabindex="-1"
          >{c.titre}{fleche(c.clef)}</button
        >
      {:else}
        <span class="ch {c.classe ?? ''}" role="columnheader">{c.titre}</span>
      {/if}
    {/each}
  </div>
  <div class="piste" style:height="{bib.total * HAUT_LIGNE}px">
    {#each indices as i (i)}
      {@const l = bib.livre(i)}
      <!-- Le clavier est géré par la zone du tableau. -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div
        class="ligne"
        class:selectionne={i === selection}
        class:debut-groupe={bib.debutGroupe(i)}
        style:transform="translateY({i * HAUT_LIGNE}px)"
        style:height="{HAUT_LIGNE}px"
        role="row"
        aria-rowindex={i + 1}
        aria-selected={i === selection}
        tabindex="-1"
        onclick={() => onselection(i)}
        ondblclick={() => onouvrir(i)}
      >
        {#if l}
          <span class="mini"><img src={l.urlCouverture} alt="" loading="lazy" decoding="async" draggable="false" /></span>
          <span class="cellule" title={l.titre}>{l.titre}</span>
          <span class="cellule discret" title={l.serie}>{l.serie ?? '—'}</span>
          <span class="cellule num">{l.numero ?? ''}</span>
          <span class="cellule discret" title={l.editeur}>{l.editeur ?? '—'}</span>
          <span class="cellule num">{l.annee ?? ''}</span>
          <span class="cellule num">{l.pages}</span>
          <span class="cellule num">{l.lu ? '✓' : ''}</span>
        {:else}
          <span></span><span class="cellule"><span class="squelette"></span></span>
        {/if}
      </div>
    {/each}
  </div>
</div>

<style>
  .zone { position: relative; overflow-y: auto; overflow-x: hidden; flex: 1; min-height: 0; outline: none; }
  .zone:focus-visible { box-shadow: inset 0 0 0 2px var(--cr-accent); }
  .entete, .ligne {
    display: grid; align-items: center; column-gap: 10px; padding: 0 12px;
    grid-template-columns: 30px minmax(120px, 2fr) minmax(90px, 1.3fr) 56px minmax(80px, 1fr) 60px 56px 36px;
  }
  .entete {
    position: sticky; top: 0; z-index: 2; background: var(--cr-fond-panneau);
    border-bottom: 1px solid var(--cr-bordure); color: var(--cr-texte-discret); font-weight: 600;
  }
  .ch { font: inherit; color: inherit; background: none; border: 0; padding: 0; text-align: left; cursor: pointer; white-space: nowrap; }
  button.ch:hover { color: var(--cr-texte); }
  .piste { position: relative; }
  .ligne { position: absolute; top: 0; left: 0; right: 0; contain: layout paint style; cursor: pointer; user-select: none; border-bottom: 1px solid var(--cr-bordure); }
  .ligne:hover { background: var(--cr-fond-survol); }
  .ligne.selectionne { background: var(--cr-accent); color: var(--cr-accent-texte); }
  .ligne.selectionne .discret { color: inherit; }
  .ligne.debut-groupe { border-top: 2px solid var(--cr-accent); }
  .cellule { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .discret { color: var(--cr-texte-discret); }
  .num { text-align: right; font-variant-numeric: tabular-nums; }
  .mini { width: 26px; height: 34px; background: var(--cr-fond-survol); border-radius: 2px; overflow: hidden; display: block; }
  .mini img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .squelette { display: block; height: 12px; width: 50%; background: var(--cr-fond-survol); border-radius: var(--cr-rayon); }
</style>
