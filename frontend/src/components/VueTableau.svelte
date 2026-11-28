<script lang="ts">
  // Tableau virtualisé (lignes de hauteur fixe) : mêmes données paginées que la grille.
  import type { ClefTri } from '../lib/api';
  import type { Bibliotheque } from '../lib/bibliotheque.svelte';
  import { cibleClavier } from '../lib/navigation';
  import { modificateurs, type Modificateurs, type Selection } from '../lib/selection.svelte';
  import { RectangleSelection, touchesLignes } from '../lib/rectangleSelection.svelte';
  import { COLONNES, colonnesVisibles, largeurColonne, LARGEUR_MAX, LARGEUR_MIN, LARGEUR_MIN_FICHIER, type Colonne } from '../lib/colonnes';
  import { enregistrerTableau, reglagesTableau } from '../lib/prefs';
  import { ID_MANQUANTS } from '../lib/manquants.svelte';

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
  // La liste virtuelle « Livres manquants » n'a pas d'ordre propre : pas de colonne « # ».
  const avecRang = $derived(!!bib.params.liste && bib.params.liste !== ID_MANQUANTS);

  // Colonnes comme egui (11 colonnes, ordre et formats de lib/colonnes.ts) ; visibilité et largeurs
  // choisies sont mémorisées dans data\comicrust-tauri.json (`vue.tableau`).
  const reglages = reglagesTableau();
  let idsVisibles = $state<string[] | undefined>(reglages.colonnes);
  let largeurs = $state<Record<string, number>>(reglages.largeurs ?? {});
  const colonnes = $derived(colonnesVisibles(idsVisibles));
  const largeursPx = $derived(colonnes.map((c) => largeurColonne(c, largeurs)));
  const gabarit = $derived([...(avecRang ? ['40px'] : []), '30px', ...largeursPx.map((w) => `${w}px`)].join(' '));
  /** Largeur du contenu : cellules, espaces de 10 px entre elles et marges de 12 px. */
  const largeurTotale = $derived(
    24 + (avecRang ? 40 : 0) + 30 + largeursPx.reduce((a, b) => a + b, 0) + 10 * (colonnes.length + (avecRang ? 1 : 0)),
  );
  const sauver = () => enregistrerTableau({ colonnes: idsVisibles, largeurs });

  /** Redimensionnement : glisser la poignée à droite de l'en-tête (double-clic : largeur d'egui). */
  function redimensionner(e: PointerEvent, c: Colonne, i: number) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const depart = e.clientX;
    const w0 = largeursPx[i];
    const min = c.id === 'fichier' ? LARGEUR_MIN_FICHIER : LARGEUR_MIN;
    const bouger = (m: PointerEvent) => {
      largeurs = { ...largeurs, [c.id]: Math.round(Math.min(LARGEUR_MAX, Math.max(min, w0 + m.clientX - depart))) };
    };
    const fin = () => {
      window.removeEventListener('pointermove', bouger);
      window.removeEventListener('pointerup', fin);
      window.removeEventListener('pointercancel', fin);
      sauver();
    };
    window.addEventListener('pointermove', bouger);
    window.addEventListener('pointerup', fin);
    window.addEventListener('pointercancel', fin);
  }
  function reinitialiserLargeur(c: Colonne) {
    const { [c.id]: _, ...reste } = largeurs;
    largeurs = reste;
    sauver();
  }

  // Menu « Colonnes » (bouton de l'en-tête ou clic droit sur l'en-tête) : cases à cocher.
  let menu = $state<{ x: number; y: number } | null>(null);
  function ouvrirMenu(x: number, y: number) {
    menu = { x: Math.max(4, Math.min(x, window.innerWidth - 230)), y: Math.max(4, Math.min(y, window.innerHeight - 380)) };
  }
  function basculer(c: Colonne) {
    const visibles = colonnes.map((x) => x.id);
    const suite = visibles.includes(c.id) ? visibles.filter((id) => id !== c.id) : [...visibles, c.id];
    if (suite.length === 0) return; // au moins une colonne
    const ordonnees = COLONNES.filter((x) => suite.includes(x.id)).map((x) => x.id);
    idsVisibles = ordonnees.length === COLONNES.length ? undefined : ordonnees;
    sauver();
  }
  function toutAfficher() {
    idsVisibles = undefined;
    sauver();
  }
  $effect(() => {
    if (!menu) return;
    const hors = (e: PointerEvent) => {
      if (!(e.target as HTMLElement).closest('[data-testid="colonnes-menu"], [data-testid="colonnes-bouton"]')) menu = null;
    };
    const echap = (e: KeyboardEvent) => {
      if (e.key === 'Escape') menu = null;
    };
    window.addEventListener('pointerdown', hors, true);
    window.addEventListener('keydown', echap);
    return () => {
      window.removeEventListener('pointerdown', hors, true);
      window.removeEventListener('keydown', echap);
    };
  });

  let zone = $state<HTMLDivElement>();
  let piste = $state<HTMLDivElement>();
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

  // Rectangle de sélection : les lignes occupent toute la largeur, il part donc du vide sous la
  // dernière ligne (vue courte). Lignes touchées calculées par leur rang (vue virtualisée).
  const rectangle = new RectangleSelection({
    sel: () => sel,
    zone: () => zone ?? null,
    piste: () => piste ?? null,
    touches: (b, _px, py) => touchesLignes(b, py, bib.total, HAUT_LIGNE),
    margeHaut: () => HAUT_ENTETE,
  });
  $effect(() => () => rectangle.detruire());

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
  onpointerdown={rectangle.appui}
  role="grid"
  class:avec-rang={avecRang}
  tabindex="0"
  aria-label="Tableau des livres"
  aria-rowcount={bib.total}
  data-testid="tableau"
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="entete" role="row" tabindex="-1" style:height="{HAUT_ENTETE}px" style:grid-template-columns={gabarit} style:min-width="{largeurTotale}px"
    oncontextmenu={(e) => { e.preventDefault(); ouvrirMenu(e.clientX, e.clientY); }}>
    {#if avecRang}
      <div role="columnheader" class="cel-entete num"><button class="ch num" onclick={() => ontri('ordre')} tabindex="-1" title="Ordre de la liste">#{fleche('ordre')}</button></div>
    {/if}
    <div role="columnheader" class="cel-entete"><button class="ch bouton-colonnes" type="button" tabindex="-1" aria-label="Choisir les colonnes" title="Choisir les colonnes" aria-haspopup="menu"
      aria-expanded={!!menu} data-testid="colonnes-bouton"
      onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); if (menu) menu = null; else ouvrirMenu(r.left, r.bottom + 2); }}>☰</button></div>
    {#each colonnes as c, i (c.id)}
      <div class="cel-entete {c.num ? 'num' : ''}" role="columnheader" data-colonne={c.id}>
        {#if c.tri}
          {@const clef = c.tri}
          <button class="ch {c.num ? 'num' : ''}" onclick={() => ontri(clef)} tabindex="-1"
            >{c.titre}{fleche(c.tri)}</button
          >
        {:else}
          <span class="ch {c.num ? 'num' : ''}">{c.titre}</span>
        {/if}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span class="poignee-col" data-testid="poignee-colonne" data-colonne={c.id} title="Glisser pour redimensionner (double-clic : largeur d'origine)"
          onpointerdown={(e) => redimensionner(e, c, i)} ondblclick={() => reinitialiserLargeur(c)}></span>
      </div>
    {/each}
  </div>
  {#if menu}
    <div class="menu-colonnes" role="menu" aria-label="Colonnes du tableau" data-testid="colonnes-menu" style:left="{menu.x}px" style:top="{menu.y}px">
      {#each COLONNES as c (c.id)}
        <label>
          <input type="checkbox" checked={colonnes.some((x) => x.id === c.id)} disabled={colonnes.length === 1 && colonnes[0].id === c.id}
            onchange={() => basculer(c)} data-colonne={c.id} />{c.titre}
        </label>
      {/each}
      <button type="button" onclick={toutAfficher} disabled={!idsVisibles}>Tout afficher</button>
    </div>
  {/if}
  <!-- La piste occupe au moins toute la hauteur visible : le vide sous les lignes en fait partie. -->
  <div class="piste" bind:this={piste} style:height="{bib.total * HAUT_LIGNE}px" style:min-height="{Math.max(0, hauteur - HAUT_ENTETE)}px" style:min-width="{largeurTotale}px">
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
        style:grid-template-columns={gabarit}
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
          {#if avecRang}<span class="cellule num discret" role="gridcell">{l.rang ?? ''}</span>{/if}
          <span class="mini" role="gridcell"><img src={l.urlCouverture} alt="" loading="lazy" decoding="async" draggable="false" /></span>
          {#if bib.debutGroupe(i) && !(bib.params.groupe === 'serie' && colonnes.some((c) => c.id === 'serie'))}
            <!-- En-tête du groupe (autre regroupement que la série, ou colonne Série masquée) : étiquette au-dessus de
                 la première ligne ; clic = tout le groupe, on peut la glisser aussitôt. -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <span class="etiquette-groupe" role="gridcell" tabindex="-1" title="{bib.nomGroupe(l)} — cliquer pour sélectionner tout le groupe (Ctrl : l'ajouter)" data-groupe={i}
              onpointerdown={(e) => { if (e.button !== 0) return; e.stopPropagation(); ongroupe?.(i, modificateurs(e)); onappui?.(e, i, true); }}
              onclick={(e) => e.stopPropagation()}>▸ {bib.nomGroupe(l)}</span>
          {/if}
          {#each colonnes as c (c.id)}
            {#if c.id === 'titre'}
              <span class="cellule" role="gridcell" class:manquant={l.manquant} title={l.manquant ? `${l.titre} — fichier introuvable` : l.titre}>{#if l.manquant}<span class="alerte-manquant" data-testid="indicateur-manquant" aria-label="Fichier introuvable">⚠ </span>{/if}{l.titre}</span>
            {:else if c.id === 'serie' && bib.debutGroupe(i) && bib.params.groupe === 'serie'}
              <!-- En-tête de la série (vue groupée) : clic = toute la série ; on peut la glisser aussitôt. -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <span class="cellule tete-groupe" role="gridcell" tabindex="-1" title="{bib.nomGroupe(l)} — cliquer pour sélectionner toute la série (Ctrl : l'ajouter)" data-groupe={i}
                onpointerdown={(e) => { if (e.button !== 0) return; e.stopPropagation(); ongroupe?.(i, modificateurs(e)); onappui?.(e, i, true); }}
                onclick={(e) => e.stopPropagation()}>▸ {bib.nomGroupe(l)}</span>
            {:else}
              <span class="cellule {c.num ? 'num' : ''} {c.id === 'serie' || c.num ? '' : 'discret'}" role="gridcell" data-col={c.id} title={c.cellule(l)}>{c.cellule(l) || (c.num ? '' : '—')}</span>
            {/if}
          {/each}
        {:else}
          {#if avecRang}<span role="gridcell"></span>{/if}<span role="gridcell"></span><span class="cellule" role="gridcell"><span class="squelette"></span></span>
        {/if}
      </div>
    {/each}
    {#if rectangle.boite}
      {@const b = rectangle.boite}
      <div class="rectangle" data-testid="rectangle-selection" style:left="{b.x1}px" style:top="{b.y1}px"
        style:width="{b.x2 - b.x1}px" style:height="{b.y2 - b.y1}px"></div>
    {/if}
  </div>
</div>

<style>
  .alerte-manquant { color: var(--cr-erreur); font-weight: 700; }
  .cellule.manquant { color: var(--cr-texte-discret); }
  .zone { position: relative; overflow-y: auto; overflow-x: auto; flex: 1; min-height: 0; outline: none; }
  .zone:focus-visible { box-shadow: inset 0 0 0 2px var(--cr-accent); }
  .entete, .ligne { display: grid; align-items: center; column-gap: 10px; padding: 0 12px; }
  .cel-entete { position: relative; min-width: 0; overflow: visible; }
  .cel-entete .ch { display: block; max-width: 100%; overflow: hidden; text-overflow: ellipsis; }
  .cel-entete.num .ch, .ch.num { text-align: right; width: 100%; }
  /* Poignée de redimensionnement : à cheval sur l'espace entre deux colonnes. */
  .poignee-col { position: absolute; top: -8px; bottom: -8px; right: -9px; width: 10px; cursor: col-resize; z-index: 3; }
  .poignee-col:hover { background: color-mix(in srgb, var(--cr-accent) 45%, transparent); }
  .bouton-colonnes { text-align: center; }
  .menu-colonnes {
    position: fixed; z-index: 20; display: flex; flex-direction: column; gap: 2px; padding: 6px; min-width: 200px;
    background: var(--cr-fond-panneau); border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); box-shadow: var(--cr-ombre);
  }
  .menu-colonnes label { display: flex; gap: 8px; align-items: center; padding: 3px 6px; cursor: pointer; }
  .menu-colonnes label:hover { background: var(--cr-fond-survol); }
  .menu-colonnes button { margin-top: 4px; }
  .etiquette-groupe {
    position: absolute; top: 0; left: 44px; z-index: 1; max-width: 40%; padding: 0 8px; font-size: 10px; line-height: 12px; font-weight: 600;
    background: var(--cr-accent); color: var(--cr-accent-texte); border-radius: 0 0 var(--cr-rayon) var(--cr-rayon);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; cursor: pointer;
  }
  .entete {
    position: sticky; top: 0; z-index: 2; background: var(--cr-fond-panneau);
    border-bottom: 1px solid var(--cr-bordure); color: var(--cr-texte-discret); font-weight: 600;
  }
  .ch { font: inherit; color: inherit; background: none; border: 0; padding: 0; text-align: left; cursor: pointer; white-space: nowrap; }
  button.ch:hover { color: var(--cr-texte); }
  .piste { position: relative; }
  /* Rectangle de sélection, aux couleurs du thème. */
  .rectangle {
    position: absolute; z-index: 1; pointer-events: none; border: 1px solid var(--cr-rectangle-bordure);
    background: var(--cr-rectangle-fond); border-radius: 2px;
  }
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
