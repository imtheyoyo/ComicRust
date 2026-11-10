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
  }
</script>

<script lang="ts">
  // Menu contextuel (clic droit, touche Menu ou Maj+F10) : flèches, Entrée, Échap, clic ailleurs.
  import { onDestroy, onMount, tick } from 'svelte';
  import { restaurerFocus } from '../lib/modale';

  interface Props {
    x: number;
    y: number;
    entrees: EntreeMenu[];
    onchoix: (id: string) => void;
    onfermer: () => void;
  }
  let { x, y, entrees, onchoix, onfermer }: Props = $props();

  // Élément qui avait le focus avant l'ouverture : on le lui rend à la fermeture (Suppr, F2… continuent de marcher).
  const origine = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  onDestroy(() => restaurerFocus(origine));

  let menu = $state<HTMLDivElement>();
  let pos = $state({ x: 0, y: 0 });

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

  const boutons = () => [...(menu?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? [])];

  function clavier(e: KeyboardEvent) {
    const liste = boutons();
    const k = liste.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === 'ArrowDown') liste[(k + 1) % liste.length]?.focus();
    else if (e.key === 'ArrowUp') liste[(k - 1 + liste.length) % liste.length]?.focus();
    else if (e.key === 'Home') liste[0]?.focus();
    else if (e.key === 'End') liste[liste.length - 1]?.focus();
    else if (e.key === 'Escape' || e.key === 'Tab') onfermer();
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  function dehors(e: PointerEvent) {
    if (menu && !menu.contains(e.target as Node)) onfermer();
  }
</script>

<svelte:window onpointerdown={dehors} onblur={onfermer} onresize={onfermer} />

<div class="menu" bind:this={menu} role="menu" tabindex="-1" aria-label="Actions sur la sélection" style:left="{pos.x}px" style:top="{pos.y}px"
  onkeydown={clavier} oncontextmenu={(e) => e.preventDefault()} data-testid="menu-contextuel">
  {#each entrees as e (e.id)}
    {#if e.separe}<div class="separateur" role="separator"></div>{/if}
    <button role="menuitem" disabled={e.desactive} title={e.aide} onclick={() => { onfermer(); onchoix(e.id); }} data-menu={e.id}>
      <span>{e.libelle}</span>{#if e.raccourci}<kbd>{e.raccourci}</kbd>{/if}
    </button>
  {/each}
</div>

<style>
  .menu {
    position: fixed; z-index: 50; min-width: 240px; padding: 4px; display: flex; flex-direction: column;
    background: var(--cr-fond-panneau); border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); box-shadow: var(--cr-ombre);
  }
  .menu button {
    display: flex; justify-content: space-between; gap: 18px; text-align: left; border: 0; background: none; padding: 6px 10px;
  }
  .menu button:hover:not(:disabled), .menu button:focus-visible { background: var(--cr-accent); color: var(--cr-accent-texte); outline: none; }
  kbd { font: inherit; font-size: 12px; opacity: 0.75; }
  .separateur { height: 1px; margin: 4px 2px; background: var(--cr-bordure); }
</style>
