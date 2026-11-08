<script lang="ts">
  // Tableau virtualisé (lignes de hauteur fixe) : mêmes données paginées que la grille.
  import type { ClefTri } from '../lib/api';
  import type { Bibliotheque } from '../lib/bibliotheque.svelte';
  import { cibleClavier } from '../lib/navigation';
  import { modificateurs, type Modificateurs, type Selection } from '../lib/selection.svelte';

  interface Props {
    bib: Bibliotheque;
    /** Sélection multiple (curseur = livre courant). */
    sel: Selection;
    /** Clic droit sur un livre (menu contextuel), en coordonnées de la fenêtre. */
    onmenu: (i: number, x: number, y: number) => void;
    onouvrir: (i: number) => void;
    ontri: (clef: ClefTri) => void;
    /** Appui sur un livre (début possible d'un glisser-déposer, étape 5). */
    onappui?: (e: PointerEvent, i: number, groupes?: boolean) => void;
    /** Clic sur l'en-tête d'un groupe : sélection de tout le groupe (Ctrl : ajoutée). */
    ongroupe?: (i: number, m: Modificateurs) => void;
  }
  let { bib, sel, onmenu, onouvrir, ontri, onappui, ongroupe }: Props = $props();

  const HAUT_LIGNE = 40;
  const HAUT_ENTETE = 34;
  const SURPLUS = 6;

  /** Liste affichée : colonne « # » (rang dans la liste, tri « ordre »). */
  const avecRang = $derived(!!bib.params.liste);
  const colonnes = $derived<Array<{ clef: ClefTri | null; titre: string; classe?: string }>>([
    ...(avecRang ? [{ clef: 'ordre' as ClefTri, titre: '#', classe: 'num' }] : []),
    { clef: null, titre: '' },
    { clef: 'titre', titre: 'Titre' },
    { clef: 'serie', titre: 'Série' },
    { clef: null, titre: 'N°', classe: 'num' },
    { clef: null, titre: 'Éditeur' },
    { clef: 'parution', titre: 'Année', classe: 'num' },
    { clef: 'pages', titre: 'Pages', classe: 'num' },
    { clef: null, titre: 'Lu', classe: 'num' },
  ]);

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
    if (e.key === 'Enter' && sel.curseur >= 0) {
      e.preventDefault();
      onouvrir(sel.curseur);
      return;
    }
    // Ctrl+A : tous les livres de la vue (recherche comprise).
    if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      sel.tout(bib.total);
      return;
    }
    // Touche « Menu » ou Maj+F10 : menu contextuel du livre courant.
    if ((e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10')) && sel.curseur >= 0) {
      e.preventDefault();
      const r = zone?.querySelector(`[data-index="${sel.curseur}"]`)?.getBoundingClientRect();
      sel.cliquerDroit(sel.curseur);
      onmenu(sel.curseur, r ? r.left + 20 : 100, r ? r.top + 20 : 100);
      return;
    }
    const lignesPage = Math.max(1, Math.floor((hauteur - HAUT_ENTETE) / HAUT_LIGNE) - 1);
    const cible = cibleClavier(e.key, sel.curseur, bib.total, 1, lignesPage);
    if (cible === null) return;
    e.preventDefault();
    // Maj+flèches : étend la sélection depuis l'ancre.
    sel.deplacer(cible, modificateurs(e));
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
  class:avec-rang={avecRang}
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
        class:selectionne={sel.contient(i)}
        class:courant={i === sel.curseur}
        data-index={i}
        class:debut-groupe={bib.debutGroupe(i)}
        style:transform="translateY({i * HAUT_LIGNE}px)"
        style:height="{HAUT_LIGNE}px"
        role="row"
        aria-rowindex={i + 1}
        aria-selected={sel.contient(i)}
        tabindex="-1"
        onclick={(e) => sel.cliquer(i, modificateurs(e))}
        oncontextmenu={(e) => { e.preventDefault(); sel.cliquerDroit(i); onmenu(i, e.clientX, e.clientY); }}
        ondblclick={() => onouvrir(i)}
        onpointerdown={(e) => onappui?.(e, i)}
      >
        {#if l}
          {#if avecRang}<span class="cellule num discret">{l.rang ?? ''}</span>{/if}
          <span class="mini"><img src={l.urlCouverture} alt="" loading="lazy" decoding="async" draggable="false" /></span>
          <span class="cellule" title={l.titre}>{l.titre}</span>
          {#if bib.debutGroupe(i) && bib.params.groupe === 'serie'}
            <!-- En-tête de la série (vue groupée) : clic = toute la série ; on peut la glisser aussitôt. -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <span class="cellule tete-groupe" title="{bib.nomGroupe(l)} — cliquer pour sélectionner toute la série (Ctrl : l'ajouter)" data-groupe={i}
              onpointerdown={(e) => { if (e.button !== 0) return; e.stopPropagation(); ongroupe?.(i, modificateurs(e)); onappui?.(e, i, true); }}
              onclick={(e) => e.stopPropagation()}>▸ {bib.nomGroupe(l)}</span>
          {:else}
            <span class="cellule discret" title={l.serie}>{l.serie ?? '—'}</span>
          {/if}
          <span class="cellule num">{l.numero ?? ''}</span>
          <span class="cellule discret" title={l.editeur}>{l.editeur ?? '—'}</span>
          <span class="cellule num">{l.annee ?? ''}</span>
          <span class="cellule num">{l.pages}</span>
          <span class="cellule num">{l.lu ? '✓' : ''}</span>
        {:else}
          {#if avecRang}<span></span>{/if}<span></span><span class="cellule"><span class="squelette"></span></span>
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
  .avec-rang .entete, .avec-rang .ligne {
    grid-template-columns: 40px 30px minmax(120px, 2fr) minmax(90px, 1.3fr) 56px minmax(80px, 1fr) 60px 56px 36px;
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
  .ligne.courant:not(.selectionne) { box-shadow: inset 0 0 0 1px var(--cr-accent); }
  .ligne.debut-groupe { border-top: 2px solid var(--cr-accent); }
  .tete-groupe { font-weight: 600; color: var(--cr-accent); cursor: pointer; }
  .ligne.selectionne .tete-groupe { color: inherit; }
  .cellule { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .discret { color: var(--cr-texte-discret); }
  .num { text-align: right; font-variant-numeric: tabular-nums; }
  .mini { width: 26px; height: 34px; background: var(--cr-fond-survol); border-radius: 2px; overflow: hidden; display: block; }
  .mini img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .squelette { display: block; height: 12px; width: 50%; background: var(--cr-fond-survol); border-radius: var(--cr-rayon); }
</style>
