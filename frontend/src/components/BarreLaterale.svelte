<script lang="ts">
  import type { ModeBiblio } from '../lib/api';

  export type Section = 'bibliotheque' | 'series' | 'listes';

  interface Props {
    section: Section;
    nbLivres: number;
    mode: ModeBiblio;
    onsection: (s: Section) => void;
    ondossiers: () => void;
    onreglages: () => void;
  }
  let { section, nbLivres, mode, onsection, ondossiers, onreglages }: Props = $props();

  const entrees: Array<{ id: Section; titre: string; glyphe: string }> = [
    { id: 'bibliotheque', titre: 'Bibliothèque', glyphe: '▦' },
    { id: 'series', titre: 'Séries', glyphe: '❏' },
    { id: 'listes', titre: 'Listes', glyphe: '☰' },
  ];
</script>

<nav class="laterale" aria-label="Navigation principale">
  <div class="marque">ComicRust</div>
  <ul>
    {#each entrees as e (e.id)}
      <li>
        <button class="entree" class:actif={section === e.id} aria-current={section === e.id ? 'page' : undefined} onclick={() => onsection(e.id)}>
          <span class="glyphe" aria-hidden="true">{e.glyphe}</span>
          <span class="nom">{e.titre}</span>
          {#if e.id === 'bibliotheque'}<span class="compte">{nbLivres.toLocaleString('fr-FR')}</span>{/if}
        </button>
      </li>
    {/each}
  </ul>
  <div class="bas">
    <button class="entree" onclick={ondossiers}><span class="glyphe" aria-hidden="true">📁</span><span class="nom">Dossiers surveillés</span></button>
    <button class="entree" onclick={onreglages}><span class="glyphe" aria-hidden="true">⚙</span><span class="nom">Réglages</span></button>
    <div class="mode" title="Mode de la bibliothèque">{mode === 'comicrack' ? 'Lié à ComicRack' : 'Autonome'}</div>
  </div>
</nav>

<style>
  .laterale {
    width: 210px; flex: none; display: flex; flex-direction: column; padding: 12px 8px;
    background: var(--cr-fond-panneau); border-right: 1px solid var(--cr-bordure);
  }
  .marque { font-size: 18px; font-weight: 700; padding: 4px 10px 14px; color: var(--cr-accent); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .bas { margin-top: auto; display: grid; gap: 2px; }
  .entree {
    width: 100%; display: flex; align-items: center; gap: 10px; text-align: left; padding: 8px 10px;
    background: none; border: 0; border-radius: var(--cr-rayon);
  }
  .entree.actif { background: var(--cr-accent); color: var(--cr-accent-texte); }
  .entree.actif:hover:not(:disabled) { background: var(--cr-accent); }
  .glyphe { width: 18px; text-align: center; }
  .nom { flex: 1; }
  .compte { font-size: 12px; opacity: 0.8; font-variant-numeric: tabular-nums; }
  .mode { padding: 8px 10px 0; color: var(--cr-texte-discret); font-size: 12px; }
</style>
