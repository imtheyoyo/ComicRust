<script lang="ts" module>
  export interface EntreeMenu {
    id: string;
    libelle: string;
    raccourci?: string;
    desactive?: boolean;
    /** Infobulle (raison de la désactivation, par exemple). */
    aide?: string;
    /** Séparateur avant l'entrée. */
    separe?: boolean;
    /** Simple intitulé de section (non interactif, `role="presentation"`), pas une commande. */
    titre?: boolean;
    /** Désactivée : la raison (`aide`) est aussi écrite sous le libellé, pas seulement en infobulle. */
    aideVisible?: boolean;
    /** Sous-menu latéral (un seul niveau) : l'entrée ouvre le sous-menu au survol, au clic ou à
     *  Flèche droite ; elle n'a pas d'action propre (son `id` n'est jamais passé à `onchoix`). Les
     *  entrées du sous-menu sont de même forme (leur éventuel `sousMenu` est ignoré). */
    sousMenu?: EntreeMenu[];
  }
</script>

<script lang="ts">
  // Menu contextuel (clic droit, touche Menu ou Maj+F10) : flèches, Entrée, Échap, clic ailleurs.
  // Sous-menus (`EntreeMenu.sousMenu`) : panneau latéral à côté du menu, ouvert au survol (après un
  // court délai), au clic ou à Flèche droite ; refermé par Flèche gauche, Échap ou en survolant une
  // autre entrée (avec délai, pour ne pas se fermer en traversant le menu en diagonale).
  import { onDestroy, onMount, tick } from 'svelte';
  import { origineFocus, restaurerFocus } from '../lib/modale';

  interface Props {
    x: number;
    y: number;
    entrees: EntreeMenu[];
    onchoix: (id: string) => void;
    onfermer: () => void;
  }
  let { x, y, entrees, onchoix, onfermer }: Props = $props();

  /** Délai avant l'ouverture d'un sous-menu au survol (évite les ouvertures en passant). */
  const DELAI_OUVERTURE = 150;
  /** Délai avant la fermeture d'un sous-menu quand le pointeur le quitte ou survole une autre entrée :
   *  laisse le temps de traverser en diagonale vers le sous-menu. */
  const DELAI_FERMETURE = 400;

  // Élément qui avait le focus avant l'ouverture : on le lui rend à la fermeture (Suppr, F2… continuent de marcher).
  const origine = origineFocus();
  onDestroy(() => {
    clearTimeout(minuterie);
    restaurerFocus(origine);
  });

  let menu = $state<HTMLDivElement>();
  let sousPanneau = $state<HTMLDivElement>();
  let pos = $state({ x: 0, y: 0 });
  /** Entrée dont le sous-menu est ouvert (id) et position du panneau (null : pas encore placé). */
  let ouvert = $state<string | null>(null);
  let posSous = $state<{ x: number; y: number } | null>(null);
  let minuterie: ReturnType<typeof setTimeout> | undefined;

  const entreeSous = $derived(ouvert === null ? undefined : entrees.find((e) => e.id === ouvert && e.sousMenu && !e.desactive));
  const idSous = 'menu-contextuel-sous';

  onMount(() => {
    void (async () => {
      pos = { x, y };
      await tick();
      // Garde le menu dans la fenêtre.
      const r = menu!.getBoundingClientRect();
      pos = { x: Math.max(4, Math.min(x, innerWidth - r.width - 4)), y: Math.max(4, Math.min(y, innerHeight - r.height - 4)) };
      boutons()[0]?.focus();
    })();
  });

  const boutonsDe = (panneau: HTMLElement | undefined) =>
    [...(panneau?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? [])];
  const boutons = () => boutonsDe(menu);
  /** Bouton de l'entrée `id` dans le menu principal. */
  const boutonDe = (id: string) =>
    [...(menu?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? [])].find((b) => b.dataset.menu === id);

  /** Place le sous-menu : à droite du menu, à gauche s'il n'y a pas la place, toujours dans la fenêtre,
   *  aligné sur l'entrée ; plus haut que la fenêtre, il défile (`max-height` + `overflow-y`). */
  function placerSous() {
    const b = ouvert === null ? undefined : boutonDe(ouvert);
    if (!b || !sousPanneau || !menu) return;
    const rb = b.getBoundingClientRect();
    const rm = menu.getBoundingClientRect();
    const l = sousPanneau.offsetWidth;
    const h = sousPanneau.offsetHeight;
    let gauche = rm.right - 2;
    if (gauche + l > innerWidth - 4) gauche = rm.left - l + 2;
    gauche = Math.max(4, Math.min(gauche, innerWidth - l - 4));
    const haut = Math.max(4, Math.min(rb.top - 5, innerHeight - h - 4));
    posSous = { x: gauche, y: haut };
  }

  // Placement (après rendu) à l'ouverture et quand le contenu du sous-menu change (scripts chargés
  // pendant que le menu est ouvert).
  $effect(() => {
    void ouvert;
    void entreeSous?.sousMenu?.length;
    if (!entreeSous) return;
    void tick().then(placerSous);
  });

  function ouvrir(id: string, focus: boolean) {
    clearTimeout(minuterie);
    if (ouvert !== id) { ouvert = id; posSous = null; }
    // Le panneau reste invisible tant qu'il n'est pas placé, et un élément invisible ne prend pas le focus :
    // placement d'abord, focus après le rendu qui le rend visible.
    if (focus) void tick().then(() => { placerSous(); return tick(); }).then(() => { (boutonsDe(sousPanneau)[0] ?? sousPanneau)?.focus(); });
  }
  function fermerSous(rendreFocus: boolean) {
    clearTimeout(minuterie);
    const id = ouvert;
    // Le panneau qui contient le focus disparaît : le focus revient toujours à l'entrée parente
    // (sinon il tombe sur le body et plus aucune touche n'atteint le menu).
    const focusDedans = !!sousPanneau?.contains(document.activeElement);
    ouvert = null;
    posSous = null;
    if ((rendreFocus || focusDedans) && id !== null) boutonDe(id)?.focus();
  }
  function planifierFermeture() {
    clearTimeout(minuterie);
    if (ouvert !== null) minuterie = setTimeout(() => fermerSous(false), DELAI_FERMETURE);
  }

  /** Survol d'une entrée du menu principal : ouvre son sous-menu (avec délai) ou ferme celui d'une autre entrée. */
  function survolPrincipal(e: EntreeMenu) {
    clearTimeout(minuterie);
    if (e.sousMenu && !e.desactive) {
      if (ouvert === e.id) return;
      minuterie = setTimeout(() => ouvrir(e.id, false), ouvert === null ? DELAI_OUVERTURE : 0);
    } else planifierFermeture();
  }

  function clavier(e: KeyboardEvent, sous: boolean) {
    const panneau = sous ? sousPanneau : menu;
    const liste = boutonsDe(panneau);
    const k = liste.indexOf(document.activeElement as HTMLButtonElement);
    const courante = !sous && k >= 0 ? entrees.find((x) => x.id === liste[k].dataset.menu) : undefined;
    if (e.key === 'ArrowDown') liste[(k + 1) % liste.length]?.focus();
    else if (e.key === 'ArrowUp') liste[k < 0 ? liste.length - 1 : (k - 1 + liste.length) % liste.length]?.focus();
    else if (e.key === 'Home') liste[0]?.focus();
    else if (e.key === 'End') liste[liste.length - 1]?.focus();
    else if (e.key === 'ArrowRight' && courante?.sousMenu && !courante.desactive) ouvrir(courante.id, true);
    else if (e.key === 'ArrowLeft' && sous) fermerSous(true);
    else if (e.key === 'Escape' && (sous || ouvert !== null)) fermerSous(true);
    else if (e.key === 'Escape' || e.key === 'Tab') onfermer();
    else return;
    // Menu principal : le focus quitte l'entrée parente d'un sous-menu ouvert au survol → on le referme.
    if (!sous && ouvert !== null && ['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)
      && (document.activeElement as HTMLElement | null)?.dataset.menu !== ouvert) fermerSous(false);
    e.preventDefault();
    e.stopPropagation();
  }

  function dehors(e: PointerEvent) {
    const cible = e.target as Node;
    if (menu && !menu.contains(cible) && !sousPanneau?.contains(cible)) onfermer();
  }

  function cliquer(e: EntreeMenu, sous: boolean) {
    if (!sous && e.sousMenu) { ouvrir(e.id, true); return; }
    onfermer();
    onchoix(e.id);
  }
</script>

<svelte:window onpointerdown={dehors} onblur={onfermer} onresize={onfermer} />

{#snippet entreesDe(liste: EntreeMenu[], sous: boolean)}
  {#each liste as e (e.id)}
    {#if e.separe}<div class="separateur" role="separator"></div>{/if}
    {#if e.titre}
      <div class="titre" role="presentation" data-menu={e.id}>{e.libelle}</div>
    {:else}
      {@const parent = !sous && !!e.sousMenu}
      <button role="menuitem" disabled={e.desactive} aria-disabled={e.desactive ? 'true' : undefined} title={e.aide}
        aria-haspopup={parent ? 'menu' : undefined} aria-expanded={parent ? ouvert === e.id : undefined}
        aria-controls={parent && ouvert === e.id ? idSous : undefined}
        class:ouvert={parent && ouvert === e.id}
        onpointerenter={sous ? () => clearTimeout(minuterie) : () => survolPrincipal(e)}
        onclick={() => cliquer(e, sous)} data-menu={e.id}>
        <span>{e.libelle}{#if e.desactive && e.aideVisible && e.aide}<small class="raison">{e.aide}</small>{/if}</span>{#if parent}<span class="chevron" aria-hidden="true">▸</span>{:else if e.raccourci}<kbd>{e.raccourci}</kbd>{/if}
      </button>
    {/if}
  {/each}
{/snippet}

<div class="menu" bind:this={menu} role="menu" tabindex="-1" aria-label="Actions sur la sélection" style:left="{pos.x}px" style:top="{pos.y}px"
  onkeydown={(e) => clavier(e, false)} oncontextmenu={(e) => e.preventDefault()} onscroll={placerSous}
  onpointerenter={() => clearTimeout(minuterie)} onpointerleave={planifierFermeture} data-testid="menu-contextuel">
  {@render entreesDe(entrees, false)}
</div>

{#if entreeSous?.sousMenu}
  <div class="menu sous" id={idSous} bind:this={sousPanneau} role="menu" tabindex="-1" aria-label={entreeSous.libelle}
    style:left="{posSous?.x ?? 0}px" style:top="{posSous?.y ?? 0}px" style:visibility={posSous ? 'visible' : 'hidden'}
    onkeydown={(e) => clavier(e, true)} oncontextmenu={(e) => e.preventDefault()}
    onpointerenter={() => clearTimeout(minuterie)} onpointerleave={planifierFermeture} data-testid="sous-menu-contextuel" data-parent={entreeSous.id}>
    {@render entreesDe(entreeSous.sousMenu, true)}
  </div>
{/if}

<style>
  .menu {
    position: fixed; z-index: 50; min-width: 240px; padding: 4px; display: flex; flex-direction: column;
    max-height: calc(100vh - 8px); overflow-y: auto; /* menu long : défile plutôt que déborder */
    background: var(--cr-fond-panneau); border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); box-shadow: var(--cr-ombre);
  }
  .menu.sous { z-index: 51; max-width: min(420px, calc(100vw - 8px)); }
  .menu button {
    display: flex; justify-content: space-between; gap: 18px; text-align: left; border: 0; background: none; padding: 6px 10px;
  }
  .menu button:hover:not(:disabled), .menu button:focus-visible { background: var(--cr-accent); color: var(--cr-accent-texte); outline: none; }
  /* Entrée dont le sous-menu est ouvert : reste repérée quand le pointeur est dans le sous-menu. */
  .menu button.ouvert:not(:disabled) { background: var(--cr-accent); color: var(--cr-accent-texte); }
  .chevron { opacity: 0.75; }
  .titre { padding: 4px 10px 2px; font-size: 12px; opacity: 0.7; }
  .raison { display: block; font-size: 11px; opacity: 0.8; max-width: 260px; white-space: normal; }
  kbd { font: inherit; font-size: 12px; opacity: 0.75; }
  .separateur { height: 1px; margin: 4px 2px; background: var(--cr-bordure); }
</style>
