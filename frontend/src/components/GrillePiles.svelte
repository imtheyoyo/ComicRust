<script lang="ts">
  // Vue empilée par série (« 📚 Empiler par série ») : une pile par série, couvertures décalées,
  // nombre de livres et barre de lecture. Un clic sélectionne la pile sans l'ouvrir ; double-clic ou
  // Entrée l'ouvre ; glisser une pile vers une liste ajoute tous ses livres. Grille virtualisée
  // comme GrilleCouvertures ; les piles viennent de lister_piles (une seule réponse).
  import type { Pile } from '../lib/api';
  import { cibleClavier } from '../lib/navigation';

  interface Props {
    piles: Pile[];
    /** Série (titre de la pile) sélectionnée, ou null. */
    selection: string | null;
    /** Incrémenté à chaque nouvelle requête : retour en haut. */
    version: number;
    /** Largeur visée d'une couverture, en pixels. */
    zoom: number;
    onselection: (serie: string) => void;
    onouvrir: (serie: string) => void;
    /** Clic droit (ou touche Menu) sur une pile, en coordonnées de la fenêtre. */
    onmenu: (serie: string, x: number, y: number) => void;
    /** Appui sur une pile (début possible d'un glisser vers une liste) ; `i` = rang de la pile. */
    onappui?: (e: PointerEvent, i: number) => void;
  }
  let { piles, selection, version, zoom, onselection, onouvrir, onmenu, onappui }: Props = $props();

  const MARGE = 16;
  const ESPACE = 16;
  const LEGENDE = 46;
  const SURPLUS = 2; // lignes rendues hors écran
  /** Décalage des couvertures de fond, en pixels. */
  const DECALAGE = 6;

  let zone = $state<HTMLDivElement>();
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

  const courant = $derived(selection === null ? -1 : piles.findIndex((p) => p.serie === selection));

  // Nouvelle requête : retour en haut.
  $effect(() => {
    void version;
    if (zone) zone.scrollTop = 0;
  });

  function afficher(i: number) {
    if (!zone) return;
    const haute = MARGE + Math.floor(i / colonnes) * hautLigne;
    const basse = haute + hautCouverture + LEGENDE;
    if (haute < zone.scrollTop) zone.scrollTop = haute - MARGE;
    else if (basse > zone.scrollTop + hauteur) zone.scrollTop = basse - hauteur + MARGE;
  }

  const lus = (p: Pile) => `${p.nbLivres} livre${p.nbLivres > 1 ? 's' : ''}, ${p.nbLus} lu${p.nbLus > 1 ? 's' : ''}`;

  function clavier(e: KeyboardEvent) {
    if (e.key === 'Enter' && courant >= 0) {
      e.preventDefault();
      onouvrir(piles[courant].serie);
      return;
    }
    if ((e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10')) && courant >= 0) {
      e.preventDefault();
      const r = zone?.querySelector(`[data-pile="${courant}"]`)?.getBoundingClientRect();
      onmenu(piles[courant].serie, r ? r.left + 20 : 100, r ? r.top + 20 : 100);
      return;
    }
    const lignesPage = Math.max(1, Math.floor(hauteur / hautLigne));
    const cible = cibleClavier(e.key, courant, piles.length, colonnes, lignesPage);
    if (cible === null) return;
    e.preventDefault();
    onselection(piles[cible].serie);
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
  aria-label="Piles de livres par série"
  aria-activedescendant={courant >= 0 ? `pile-${courant}` : undefined}
  data-testid="grille-piles"
>
  <div class="piste" style:height="{hautTotal}px">
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
        class:selectionne={p.serie === selection}
        data-pile={i}
        style:width="{largeurCellule}px"
        style:transform="translate({MARGE + col * (largeurCellule + ESPACE)}px, {MARGE + ligne * hautLigne}px)"
        role="option"
        aria-selected={p.serie === selection}
        tabindex="-1"
        title="{p.serie} — {lus(p)}"
        onclick={() => onselection(p.serie)}
        oncontextmenu={(e) => { e.preventDefault(); onselection(p.serie); onmenu(p.serie, e.clientX, e.clientY); }}
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
  .lecture { position: absolute; left: 0; right: 0; bottom: 0; height: 5px; background: rgb(0 0 0 / 45%); }
  .lecture > span { display: block; height: 100%; background: var(--cr-accent); }
  .legende { height: 46px; padding: 4px 2px 0; overflow: hidden; }
  .titre { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sous { color: var(--cr-texte-discret); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .cellule:hover .devant { border-color: var(--cr-accent); }
  .cellule.selectionne .devant { outline: 3px solid var(--cr-accent); outline-offset: 1px; border-color: var(--cr-accent); }
  .cellule.selectionne .titre { color: var(--cr-accent); }
</style>
