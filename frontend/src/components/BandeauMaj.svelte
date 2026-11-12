<script lang="ts">
  // Bandeau discret en haut de la fenêtre quand une nouvelle version est disponible (comme
  // `update_banner` de la version egui) : Installer, Nouveautés…, Plus tard (jusqu'au prochain
  // lancement), Ignorer cette version (mémorisé par le backend).
  import { miseAJour } from '../lib/maj.svelte';

  const installable = $derived(!!miseAJour.etat?.installable && !!miseAJour.release?.aPaquet);

  function installer() {
    // La progression et le bilan s'affichent dans la fenêtre « Mise à jour ».
    miseAJour.ouvrir();
    void miseAJour.installer();
  }
</script>

{#if miseAJour.bandeau && miseAJour.release}
  <div class="bandeau" role="status" aria-label="Nouvelle version" data-testid="bandeau-maj">
    <span class="texte"><span aria-hidden="true">⬆</span> ComicRust {miseAJour.release.tag} est disponible</span>
    {#if miseAJour.etat}<span class="discret">(version actuelle : {miseAJour.etat.version})</span>{/if}
    <span class="boutons">
      {#if installable}
        <button type="button" class="principal" onclick={installer} data-testid="bandeau-maj-installer">Installer</button>
      {/if}
      <button type="button" onclick={() => miseAJour.ouvrir()} data-testid="bandeau-maj-nouveautes">Nouveautés…</button>
      <button type="button" onclick={() => (miseAJour.plusTard = true)} title="Masquer jusqu'au prochain lancement" data-testid="bandeau-maj-plus-tard">Plus tard</button>
      <button type="button" onclick={() => void miseAJour.ignorer()} title="Ne plus proposer cette version" data-testid="bandeau-maj-ignorer">Ignorer cette version</button>
    </span>
  </div>
{/if}

<style>
  .bandeau {
    display: flex; flex-wrap: wrap; align-items: center; gap: 4px 12px; padding: 5px 14px; font-size: 0.9em;
    border-bottom: 1px solid var(--cr-accent); background: var(--cr-fond-panneau); color: var(--cr-texte);
  }
  .texte { font-weight: 600; color: var(--cr-accent); }
  .discret { color: var(--cr-texte-discret); }
  .boutons { margin-left: auto; display: flex; flex-wrap: wrap; gap: 6px; }
  button { padding: 2px 10px; }
  button.principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); }
</style>
