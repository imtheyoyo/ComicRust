<script lang="ts">
  // Grille virtualisée : seules les lignes visibles (+ marge) sont dans le DOM ; les données sont
  // chargées par pages via la Bibliotheque (lister_livres debut/nombre).
  import type { Bibliotheque } from '../lib/bibliotheque.svelte';
  import { cibleClavier } from '../lib/navigation';
  import { modificateurs, type Modificateurs, type Selection } from '../lib/selection.svelte';

  interface Props {
    bib: Bibliotheque;
    /** Sélection multiple (curseur = livre courant). */
    sel: Selection;
    /** Largeur visée d'une couverture, en pixels. */
    zoom: number;
    /** Clic droit sur un livre (menu contextuel), en coordonnées de la fenêtre. */
    onmenu: (i: number, x: number, y: number) => void;
    onouvrir: (i: number) => void;
    /** Appui sur un livre (début possible d'un glisser-déposer, étape 5). */
    onappui?: (e: PointerEvent, i: number, groupes?: boolean) => void;
    /** Clic sur l'en-tête d'un groupe : sélection de tout le groupe (Ctrl : ajoutée). */
    ongroupe?: (i: number, m: Modificateurs) => void;
  }
  let { bib, sel, zoom, onmenu, onouvrir, onappui, ongroupe }: Props = $props();

  const MARGE = 16;
  const ESPACE = 16;
  const LEGENDE = 46;
  const SURPLUS = 2; // lignes rendues hors écran

  let zone = $state<HTMLDivElement>();
  let largeur = $state(800);
  let hauteur = $state(600);
  let haut = $state(0);

  const colonnes = $derived(Math.max(1, Math.floor((largeur - 2 * MARGE + ESPACE) / (zoom + ESPACE))));
  const largeurCellule = $derived((largeur - 2 * MARGE - (colonnes - 1) * ESPACE) / colonnes);
  const hautCouverture = $derived(largeurCellule * 1.5);
  const hautLigne = $derived(hautCouverture + LEGENDE + ESPACE);
  const nbLignes = $derived(Math.ceil(bib.total / colonnes));
  const hautTotal = $derived(nbLignes * hautLigne + 2 * MARGE - ESPACE);

  const ligneA = $derived(Math.max(0, Math.floor((haut - MARGE) / hautLigne) - SURPLUS));
  const ligneB = $derived(Math.min(nbLignes - 1, Math.floor((haut + hauteur - MARGE) / hautLigne) + SURPLUS));
  const indices = $derived.by(() => {
    const premier = ligneA * colonnes;
    const dernier = Math.min(bib.total - 1, (ligneB + 1) * colonnes - 1);
    const liste: number[] = [];
    for (let i = premier; i <= dernier; i++) liste.push(i);
    return liste;
  });

  $effect(() => {
    if (bib.total > 0) bib.assurer(ligneA * colonnes, Math.min(bib.total - 1, (ligneB + 1) * colonnes - 1));
  });

  // Nouvelle requête : retour en haut.
  $effect(() => {
    bib.version;
    if (zone) zone.scrollTop = 0;
  });

  function afficher(i: number) {
    if (!zone) return;
    const haute = MARGE + Math.floor(i / colonnes) * hautLigne;
    const basse = haute + hautCouverture + LEGENDE;
    if (haute < zone.scrollTop) zone.scrollTop = haute - MARGE;
    else if (basse > zone.scrollTop + hauteur) zone.scrollTop = basse - hauteur + MARGE;
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
    const lignesPage = Math.max(1, Math.floor(hauteur / hautLigne));
    const cible = cibleClavier(e.key, sel.curseur, bib.total, colonnes, lignesPage);
    if (cible === null) return;
    e.preventDefault();
    // Maj+flèches : étend la sélection depuis l'ancre.
    sel.deplacer(cible, modificateurs(e));
    afficher(cible);
  }

</script>

<!-- svelte-ignore a11y_interactive_supports_focus -->
<div
  class="zone"
  bind:this={zone}
  bind:clientWidth={largeur}
  bind:clientHeight={hauteur}
  onscroll={() => (haut = zone!.scrollTop)}
  onkeydown={clavier}
  role="listbox"
  tabindex="0"
  aria-label="Couvertures des livres"
  aria-activedescendant={sel.curseur >= 0 ? `livre-${sel.curseur}` : undefined}
  aria-multiselectable="true"
  data-testid="grille"
>
  <div class="piste" style:height="{hautTotal}px">
    {#each indices as i (i)}
      {@const l = bib.livre(i)}
      {@const ligne = Math.floor(i / colonnes)}
      {@const col = i % colonnes}
      <!-- Le clavier est géré par la zone (aria-activedescendant). -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div
        id="livre-{i}"
        class="cellule"
        class:selectionne={sel.contient(i)}
        class:courant={i === sel.curseur}
        data-index={i}
        style:width="{largeurCellule}px"
        style:transform="translate({MARGE + col * (largeurCellule + ESPACE)}px, {MARGE + ligne * hautLigne}px)"
        role="option"
        aria-selected={sel.contient(i)}
        tabindex="-1"
        onclick={(e) => sel.cliquer(i, modificateurs(e))}
        oncontextmenu={(e) => { e.preventDefault(); sel.cliquerDroit(i); onmenu(i, e.clientX, e.clientY); }}
        ondblclick={() => onouvrir(i)}
        onpointerdown={(e) => onappui?.(e, i)}
      >
        <div class="couverture" style:height="{hautCouverture}px">
          {#if l}
            <img src={l.urlCouverture} alt="" loading="lazy" decoding="async" draggable="false" />
            {#if l.lu}<span class="lu" title="Lu">✓</span>{/if}
            {#if bib.debutGroupe(i)}
              <!-- En-tête du groupe : clic = toute la série (ou tout l'éditeur) ; on peut la glisser aussitôt. -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <span class="groupe" title="{bib.nomGroupe(l)} — cliquer pour sélectionner tout le groupe (Ctrl : l'ajouter)" data-groupe={i}
                onpointerdown={(e) => { if (e.button !== 0) return; e.stopPropagation(); ongroupe?.(i, modificateurs(e)); onappui?.(e, i, true); }}
                onclick={(e) => e.stopPropagation()}>{bib.nomGroupe(l)}</span>
            {/if}
          {/if}
        </div>
        <div class="legende">
          {#if l}
            <div class="titre" title={l.titre}>{l.titre}</div>
            <div class="sous">{l.serie ?? '—'}{l.numero ? ` · n° ${l.numero}` : ''}</div>
          {:else}
            <div class="squelette"></div>
          {/if}
        </div>
      </div>
    {/each}
  </div>
</div>

<style>
  .zone { position: relative; overflow-y: auto; overflow-x: hidden; flex: 1; min-height: 0; outline: none; }
  .zone:focus-visible { box-shadow: inset 0 0 0 2px var(--cr-accent); }
  .piste { position: relative; width: 100%; }
  .cellule {
    position: absolute; top: 0; left: 0; cursor: pointer; user-select: none;
    border-radius: var(--cr-rayon); padding: 0; contain: layout paint;
  }
  .couverture {
    position: relative; overflow: hidden; background: var(--cr-fond-survol);
    border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); box-shadow: var(--cr-ombre);
  }
  .couverture img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .legende { height: 46px; padding: 4px 2px 0; overflow: hidden; }
  .titre { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sous { color: var(--cr-texte-discret); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .squelette { height: 12px; width: 70%; margin-top: 4px; background: var(--cr-fond-survol); border-radius: var(--cr-rayon); }
  .cellule:hover .couverture { border-color: var(--cr-accent); }
  .cellule.selectionne .couverture { outline: 3px solid var(--cr-accent); outline-offset: 1px; border-color: var(--cr-accent); }
  .cellule.selectionne .titre { color: var(--cr-accent); }
  .cellule.courant:not(.selectionne) .couverture { outline: 2px dashed var(--cr-accent); outline-offset: 1px; }
  .lu {
    position: absolute; right: 6px; bottom: 6px; width: 20px; height: 20px; border-radius: 50%;
    background: var(--cr-accent); color: var(--cr-accent-texte); font-size: 12px; line-height: 20px; text-align: center;
  }
  .groupe {
    position: absolute; left: 6px; top: 6px; max-width: calc(100% - 12px); padding: 1px 7px;
    border-radius: var(--cr-rayon); background: var(--cr-accent); color: var(--cr-accent-texte);
    font-size: 11px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; cursor: pointer;
  }
  .groupe:hover { filter: brightness(1.15); }
</style>
