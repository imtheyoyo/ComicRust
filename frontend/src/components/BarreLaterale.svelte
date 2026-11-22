<script lang="ts">
  import type { ModeBiblio } from '../lib/api';
  import type { EtatListes } from '../lib/listes.svelte';
  import { manquants } from '../lib/manquants.svelte';
  import { jauges } from '../lib/jauges.svelte';
  import Jauges from './Jauges.svelte';
  import PanneauListes from './PanneauListes.svelte';

  export type Section = 'bibliotheque' | 'listes';

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
    /** Panneau des listes affiché (lot L7) ; masqué, la barre se réduit à une colonne de boutons. */
    listesVisibles?: boolean;
    /** Afficher / masquer le panneau des listes (bouton ; raccourci Ctrl+Maj+L géré par l'application). */
    onbasculerlistes?: () => void;
    /** Ouvre « À propos / Aide » (F1). */
    onaide?: () => void;
  }
  let {
    section, nbLivres, mode, onsection, ondossiers, onreglages, listes, onliste, largeur = 250,
    listesVisibles = true, onbasculerlistes, onaide,
  }: Props = $props();

  // « Listes » n'est plus une entrée : l'arbre des listes occupe le milieu du panneau.
  const entrees: Array<{ id: Section; titre: string; glyphe: string }> = [
    { id: 'bibliotheque', titre: 'Bibliothèque', glyphe: '▦' },
  ];
</script>

{#if !listesVisibles}
  <!-- Panneau des listes masqué : colonne étroite, les fonctions restent accessibles. -->
  <nav class="laterale rail" aria-label="Navigation principale" data-testid="barre-reduite">
    <button class="entree" class:actif={section === 'bibliotheque' && !manquants.affiche} onclick={() => onsection('bibliotheque')}
      title="Bibliothèque ({nbLivres.toLocaleString('fr-FR')} livres)" aria-label="Bibliothèque"><span class="glyphe" aria-hidden="true">▦</span></button>
    <button class="entree" onclick={() => onbasculerlistes?.()} title="Afficher les listes (Ctrl+Maj+L)" aria-label="Afficher les listes" data-testid="afficher-listes"><span class="glyphe" aria-hidden="true">📂</span></button>
    <div class="bas">
      <button class="entree" onclick={ondossiers} title="Dossiers surveillés" aria-label="Dossiers surveillés"><span class="glyphe" aria-hidden="true">📁</span></button>
      <button class="entree" onclick={onreglages} title="Réglages" aria-label="Réglages"><span class="glyphe" aria-hidden="true">⚙</span></button>
      <button class="entree" onclick={() => onaide?.()} title="Aide et à propos (F1)" aria-label="Aide et à propos"><span class="glyphe" aria-hidden="true">?</span></button>
    </div>
  </nav>
{:else}
<nav class="laterale" aria-label="Navigation principale" style:width="{largeur}px">
  <div class="marque">ComicRust</div>
  <ul>
    {#each entrees as e (e.id)}
      <li>
        <button class="entree" class:actif={section === e.id && !(e.id === 'bibliotheque' && manquants.affiche)} aria-current={section === e.id ? 'page' : undefined} onclick={() => onsection(e.id)}>
          <span class="glyphe" aria-hidden="true">{e.glyphe}</span>
          <span class="nom">{e.titre}</span>
          {#if e.id === 'bibliotheque'}
            {#if jauges.racine}<Jauges jauge={jauges.racine} />{:else}<span class="compte">{nbLivres.toLocaleString('fr-FR')}</span>{/if}
          {/if}
        </button>
      </li>
    {/each}
  </ul>
  <PanneauListes {listes} onafficher={onliste} />
  <div class="bas">
    <button class="entree" onclick={() => onbasculerlistes?.()} title="Masquer le panneau des listes (Ctrl+Maj+L)" data-testid="masquer-listes"><span class="glyphe" aria-hidden="true">📂</span><span class="nom">Masquer les listes</span></button>
    <button class="entree" onclick={ondossiers}><span class="glyphe" aria-hidden="true">📁</span><span class="nom">Dossiers surveillés</span></button>
    <button class="entree" onclick={onreglages}><span class="glyphe" aria-hidden="true">⚙</span><span class="nom">Réglages</span></button>
    <button class="entree" onclick={() => onaide?.()} title="Version, PDFium, raccourcis, diagnostic (F1)" data-testid="ouvrir-apropos"><span class="glyphe" aria-hidden="true">?</span><span class="nom">Aide et à propos</span></button>
    <div class="mode" title="Mode de la bibliothèque">{mode === 'comicrack' ? 'Lié à ComicRack' : 'Autonome'}</div>
  </div>
</nav>
{/if}

<style>
  .laterale {
    width: 250px; flex: none; min-height: 0; display: flex; flex-direction: column; padding: 12px 8px;
    background: var(--cr-fond-panneau); border-right: 1px solid var(--cr-bordure);
  }
  .laterale.rail { width: 48px; padding: 12px 4px; align-items: stretch; }
  .rail .entree { justify-content: center; padding: 8px 0; }
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
