<script lang="ts">
  // Vue empilée par série (« 📚 Empiler par série ») : une pile par série, couvertures décalées,
  // nombre de livres et barre de lecture. Sélection multiple comme la grille de couvertures (clic,
  // Ctrl/Maj + clic, Maj + flèches, Ctrl+A, rectangle tracé dans le vide) ; double-clic ou Entrée
  // ouvre la pile courante ; glisser des piles vers une liste ajoute tous leurs livres. Grille
  // virtualisée comme GrilleCouvertures ; les piles viennent de lister_piles (une seule réponse).
  import { untrack } from 'svelte';
  import type { Pile } from '../lib/api';
  import { cibleClavier } from '../lib/navigation';
  import { modificateurs, type Selection } from '../lib/selection.svelte';
  import { RectangleSelection, touchesGrille } from '../lib/rectangleSelection.svelte';

  interface Props {
    piles: Pile[];
    /** Sélection multiple, par rang de pile (curseur = pile courante). */
    sel: Selection;
    /** Incrémenté à chaque nouvelle requête : retour en haut. */
    version: number;
    /** Largeur visée d'une couverture, en pixels. */
    zoom: number;
    onouvrir: (serie: string) => void;
    /** Clic droit (ou touche Menu) : menu des piles sélectionnées, en coordonnées de la fenêtre. */
    onmenu: (x: number, y: number) => void;
    /** Appui sur une pile (début possible d'un glisser vers une liste) ; `i` = rang de la pile. */
    onappui?: (e: PointerEvent, i: number) => void;
  }
  let { piles, sel, version, zoom, onouvrir, onmenu, onappui }: Props = $props();

  const MARGE = 16;
  const ESPACE = 16;
  const LEGENDE = 46;
  const SURPLUS = 2; // lignes rendues hors écran
  /** Décalage des couvertures de fond, en pixels. */
  const DECALAGE = 6;

  let zone = $state<HTMLDivElement>();
  let piste = $state<HTMLDivElement>();
  let largeur = $state(800);
  let hauteur = $state(600);
  let haut = $state(0);

  const colonnes = $derived(Math.max(1, Math.floor((largeur - 2 * MARGE + ESPACE) / (zoom + ESPACE))));
  const largeurCellule = $derived((largeur - 2 * MARGE - (colonnes - 1) * ESPACE) / colonnes);
  const hautCouverture = $derived(largeurCellule * 1.5);
  const hautLigne = $derived(hautCouverture + LEGENDE + ESPACE);
  const nbLignes = $derived(Math.ceil(piles.length / colonnes));
  const hautTotal = $derived(nbLignes * hautLigne + 2 * MARGE - ESPACE);

  const ligneA = $derived(Math.max(0, Math.floor((haut - MARGE) / hautLigne) - SURPLUS));
  const ligneB = $derived(Math.min(nbLignes - 1, Math.floor((haut + hauteur - MARGE) / hautLigne) + SURPLUS));
  const indices = $derived.by(() => {
    const dernier = Math.min(piles.length - 1, (ligneB + 1) * colonnes - 1);
    const liste: number[] = [];
    for (let i = ligneA * colonnes; i <= dernier; i++) liste.push(i);
    return liste;
  });

  /** Pile courante (curseur clavier), -1 si aucune. */
  const courant = $derived(sel.curseur < piles.length ? sel.curseur : -1);

  // Nouvelle requête : retour en haut, puis la pile sélectionnée est rendue visible (retour d'une
  // pile ouverte par Échap ou « ← Piles » : on retrouve la pile qu'on vient de quitter).
  $effect(() => {
    void version;
    const z = zone;
    if (!z) return;
    // Seule la version déclenche : ni la sélection ni la taille de la zone (lues par `afficher`).
    untrack(() => {
      z.scrollTop = 0;
      if (courant >= 0) afficher(courant);
    });
  });

  // Rectangle de sélection tracé dans le vide, même géométrie que la grille de couvertures : les
  // piles hors écran (absentes du DOM) sont comptées aussi.
  const rectangle = new RectangleSelection({
    sel: () => sel,
    zone: () => zone ?? null,
    piste: () => piste ?? null,
    touches: (b, px, py) =>
      touchesGrille(b, px, py, {
        total: piles.length, colonnes, marge: MARGE, largeur: largeurCellule, espace: ESPACE,
        hautCellule: hautCouverture + LEGENDE, hautLigne,
      }),
  });
  $effect(() => () => rectangle.detruire());

  function afficher(i: number) {
    if (!zone) return;
    const haute = MARGE + Math.floor(i / colonnes) * hautLigne;
    const basse = haute + hautCouverture + LEGENDE;
    if (haute < zone.scrollTop) zone.scrollTop = haute - MARGE;
    else if (basse > zone.scrollTop + hauteur) zone.scrollTop = basse - hauteur + MARGE;
  }

  const lus = (p: Pile) => `${p.nbLivres} livre${p.nbLivres > 1 ? 's' : ''}, ${p.nbLus} lu${p.nbLus > 1 ? 's' : ''}`;

  function clavier(e: KeyboardEvent) {
    // Entrée : ouvre la pile courante (avec plusieurs piles sélectionnées, la courante).
    if (e.key === 'Enter' && courant >= 0) {
      e.preventDefault();
      onouvrir(piles[courant].serie);
      return;
    }
    // Ctrl+A : toutes les piles de la vue.
    if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      sel.tout(piles.length);
      return;
    }
    // Touche « Menu » ou Maj+F10 : menu des piles sélectionnées.
    if ((e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10')) && courant >= 0) {
      e.preventDefault();
      const r = zone?.querySelector(`[data-pile="${courant}"]`)?.getBoundingClientRect();
      sel.cliquerDroit(courant);
      onmenu(r ? r.left + 20 : 100, r ? r.top + 20 : 100);
      return;
    }
    const lignesPage = Math.max(1, Math.floor(hauteur / hautLigne));
    const cible = cibleClavier(e.key, courant, piles.length, colonnes, lignesPage);
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
  onpointerdown={rectangle.appui}
  role="listbox"
  tabindex="0"
  aria-label="Piles de livres par série"
  aria-activedescendant={courant >= 0 ? `pile-${courant}` : undefined}
  aria-multiselectable="true"
  data-testid="grille-piles"
>
  <!-- Au moins toute la hauteur visible : le vide sous la dernière ligne fait partie de la piste. -->
  <div class="piste" bind:this={piste} style:height="{hautTotal}px" style:min-height="{hauteur}px">
    {#each indices as i (i)}
      {@const p = piles[i]}
      {@const ligne = Math.floor(i / colonnes)}
      {@const col = i % colonnes}
      {@const pile = p.nbLivres > 1}
      <!-- Le clavier est géré par la zone (aria-activedescendant). -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div
        id="pile-{i}"
        class="cellule"
        class:selectionne={sel.contient(i)}
        class:courant={i === courant}
        data-pile={i}
        style:width="{largeurCellule}px"
        style:transform="translate({MARGE + col * (largeurCellule + ESPACE)}px, {MARGE + ligne * hautLigne}px)"
        role="option"
        aria-selected={sel.contient(i)}
        tabindex="-1"
        title="{p.serie} — {lus(p)}"
        onclick={(e) => sel.cliquer(i, modificateurs(e))}
        oncontextmenu={(e) => { e.preventDefault(); sel.cliquerDroit(i); onmenu(e.clientX, e.clientY); }}
        ondblclick={() => onouvrir(p.serie)}
        onpointerdown={(e) => onappui?.(e, i)}
      >
        <div class="couverture" style:height="{hautCouverture}px">
          <!-- Couvertures de fond décalées : une pile de 2 livres en a une, de 3 et plus en a deux. -->
          {#if p.nbLivres > 2}<span class="fond" style:left="{2 * DECALAGE}px" style:bottom="{2 * DECALAGE}px"></span>{/if}
          {#if pile}<span class="fond" style:left="{DECALAGE}px" style:bottom="{DECALAGE}px"></span>{/if}
          <span class="devant" class:seule={!pile}>
            <img src={p.urlCouverture} alt="" loading="lazy" decoding="async" draggable="false" />
            {#if pile}<span class="nombre" data-testid="nombre-pile">{p.nbLivres}</span>{/if}
            <span
              class="lecture"
              role="progressbar"
              aria-label="Livres lus de la série"
              aria-valuemin="0"
              aria-valuemax={p.nbLivres}
              aria-valuenow={p.nbLus}
            ><span style:width="{(p.nbLus / p.nbLivres) * 100}%"></span></span>
          </span>
        </div>
        <div class="legende">
          <div class="titre">{p.serie}</div>
          <div class="sous">{lus(p)}</div>
        </div>
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
  .zone { position: relative; overflow-y: auto; overflow-x: hidden; flex: 1; min-height: 0; outline: none; }
  .zone:focus-visible { box-shadow: inset 0 0 0 2px var(--cr-accent); }
  .piste { position: relative; width: 100%; }
  /* Rectangle de sélection, aux couleurs du thème (repli : l'accent). */
  .rectangle {
    position: absolute; z-index: 3; pointer-events: none; border-radius: 2px;
    border: 1px solid var(--cr-rectangle-bordure, var(--cr-accent));
    background: var(--cr-rectangle-fond, color-mix(in srgb, var(--cr-accent) 22%, transparent));
  }
  .cellule {
    position: absolute; top: 0; left: 0; cursor: pointer; user-select: none;
    border-radius: var(--cr-rayon); padding: 0; contain: layout paint;
  }
  .couverture { position: relative; }
  /* La couverture de devant est plus petite que la cellule quand des couvertures de fond dépassent
     en haut à droite : tout reste dans le cadre. */
  .devant, .fond {
    position: absolute; left: 0; bottom: 0; width: calc(100% - 12px); height: calc(100% - 12px);
    overflow: hidden; background: var(--cr-fond-survol);
    border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); box-shadow: var(--cr-ombre);
  }
  .devant.seule { width: 100%; height: 100%; }
  .fond { background: color-mix(in srgb, var(--cr-fond-survol) 80%, var(--cr-bordure)); }
  .devant img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .nombre {
    position: absolute; left: 6px; top: 6px; min-width: 22px; padding: 1px 6px; text-align: center;
    border-radius: var(--cr-rayon); background: var(--cr-accent); color: var(--cr-accent-texte);
    font-size: 12px; font-weight: 700;
  }
  /* Barre de lecture : part des livres lus de la série. */
  .lecture { position: absolute; left: 0; right: 0; bottom: 0; height: 5px; background: var(--cr-lecture-piste, rgb(0 0 0 / 45%)); }
  .lecture > span { display: block; height: 100%; background: var(--cr-accent); }
  .legende { height: 46px; padding: 4px 2px 0; overflow: hidden; }
  .titre { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sous { color: var(--cr-texte-discret); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .cellule:hover .devant { border-color: var(--cr-accent); }
  .cellule.selectionne .devant { outline: 3px solid var(--cr-accent); outline-offset: 1px; border-color: var(--cr-accent); }
  .cellule.selectionne .titre { color: var(--cr-accent); }
  .cellule.courant:not(.selectionne) .devant { outline: 2px dashed var(--cr-accent); outline-offset: 1px; }
</style>
