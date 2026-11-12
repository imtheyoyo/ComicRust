<script lang="ts">
  import type { ModeBiblio } from '../lib/api';
  import type { EtatListes } from '../lib/listes.svelte';
  import PanneauListes from './PanneauListes.svelte';

  export type Section = 'bibliotheque' | 'series' | 'listes';

  interface Props {
    section: Section;
    nbLivres: number;
    mode: ModeBiblio;
    onsection: (s: Section) => void;
    ondossiers: () => void;
    onreglages: () => void;
    /** Étape 5 : listes ComicRack (arbre sous la navigation). */
    listes: EtatListes;
    /** Liste choisie dans l'arbre (null : plus de liste affichée). */
    onliste: (id: string | null) => void;
    /** Largeur en px (réglable, mémorisée). */
    largeur?: number;
  }
  let { section, nbLivres, mode, onsection, ondossiers, onreglages, listes, onliste, largeur = 250 }: Props = $props();

  // « Listes » n'est plus une entrée : l'arbre des listes occupe le milieu du panneau.
  const entrees: Array<{ id: Section; titre: string; glyphe: string }> = [
    { id: 'bibliotheque', titre: 'Bibliothèque', glyphe: '▦' },
    { id: 'series', titre: 'Séries', glyphe: '❏' },
  ];
</script>

<nav class="laterale" aria-label="Navigation principale" style:width="{largeur}px">
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
  <PanneauListes {listes} onafficher={onliste} />
  <div class="bas">
    <button class="entree" onclick={ondossiers}><span class="glyphe" aria-hidden="true">📁</span><span class="nom">Dossiers surveillés</span></button>
    <button class="entree" onclick={onreglages}><span class="glyphe" aria-hidden="true">⚙</span><span class="nom">Réglages</span></button>
    <div class="mode" title="Mode de la bibliothèque">{mode === 'comicrack' ? 'Lié à ComicRack' : 'Autonome'}</div>
  </div>
</nav>

<style>
  .laterale {
    width: 250px; flex: none; min-height: 0; display: flex; flex-direction: column; padding: 12px 8px;
    background: var(--cr-fond-panneau); border-right: 1px solid var(--cr-bordure);
  }
  .marque { font-size: 18px; font-weight: 700; padding: 4px 10px 14px; color: var(--cr-accent); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .bas { margin-top: 8px; display: grid; gap: 2px; border-top: 1px solid var(--cr-bordure); padding-top: 6px; }
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
