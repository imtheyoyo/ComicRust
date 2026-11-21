<script lang="ts">
  // Bandeau de la vue « Livres manquants » (lot L3) : actions globales (vérifier, chercher dans un
  // dossier, retirer), avancement de la vérification, avertissements (volume injoignable, disparition
  // massive : livres non marqués) et raison d'une éventuelle lecture seule.
  import { manquants } from '../lib/manquants.svelte';

  interface Props {
    /** « Retirer… » : sélection, à défaut tous les livres affichés (la vue connaît les identifiants). */
    onretirer: () => void;
  }
  let { onretirer }: Props = $props();

  const v = $derived(manquants.verification);
  const d = $derived(manquants.etat?.derniere);
</script>

<div class="bandeau" data-testid="bandeau-manquants">
  <div class="ligne">
    <button type="button" onclick={() => manquants.retourBibliotheque()} data-testid="manquants-toute">✖ Toute la bibliothèque</button>
    <button type="button" onclick={() => void manquants.verifier()} disabled={!!v} data-testid="manquants-verifier">🔄 Vérifier à nouveau</button>
    <button type="button" onclick={() => manquants.demanderDossier([])} disabled={!!manquants.raison || manquants.nombre === 0} title={manquants.raison} data-testid="manquants-dossier">🔍 Rechercher dans un dossier…</button>
    <button type="button" onclick={onretirer} disabled={!!manquants.raison || manquants.nombre === 0} title={manquants.raison ?? 'Retire les fiches (jamais les fichiers) : la sélection, ou tous les livres affichés'} data-testid="manquants-retirer">🗑 Retirer…</button>
    {#if v}
      <span class="avancement" role="status" aria-live="polite" data-testid="manquants-avancement">
        Vérification {v.fait.toLocaleString('fr-FR')} / {v.total.toLocaleString('fr-FR')}
        <button type="button" onclick={() => manquants.arreterTravail()}>Arrêter</button>
      </span>
    {:else if d}
      <span class="discret" data-testid="manquants-resume">{d.resume}</span>
    {/if}
  </div>
  {#if manquants.raison}<p class="avis" data-testid="manquants-lecture-seule">🔒 {manquants.raison}</p>{/if}
  {#each manquants.avertissements as a (a)}<p class="avis" role="status" data-testid="manquants-avertissement">{a}</p>{/each}
  {#if d?.avertissement}<p class="avis" role="status">{d.avertissement}</p>{/if}
</div>

<style>
  .bandeau { padding: 6px 14px; border-bottom: 1px solid var(--cr-bordure); background: var(--cr-fond-panneau); font-size: 0.9em; }
  .ligne { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 8px; }
  .avancement { display: inline-flex; align-items: center; gap: 8px; }
  .discret { color: var(--cr-texte-discret); }
  .avis { margin: 4px 0 0; color: var(--cr-texte-discret); }
</style>
