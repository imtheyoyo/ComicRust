<script lang="ts">
  // Indicateur d'état de la base (étape 6) : « base injoignable, nouvel essai… » avec la cause en
  // info-bulle et un bouton « Réessayer » ; un bref message annonce le retour de la base.
  import { connexion } from '../lib/connexion.svelte';
</script>

{#if !connexion.enLigne}
  <div class="bandeau hors" role="alert" data-testid="bandeau-hors-ligne">
    <span class="texte">⚠ Base injoignable, nouvel essai…</span>
    {#if connexion.essais > 1}<span class="discret">({connexion.essais} échecs)</span>{/if}
    {#if connexion.message}<span class="cause" title={connexion.message}>{connexion.message}</span>{/if}
    <span class="info">Consultation seulement tant que la base ne répond pas : les modifications sont désactivées.</span>
    <button type="button" onclick={() => connexion.reessayer()} disabled={connexion.reessai}>
      {connexion.reessai ? 'Essai…' : 'Réessayer'}
    </button>
    {#if connexion.erreur}<span class="cause" role="status">{connexion.erreur}</span>{/if}
  </div>
{:else if connexion.revenue}
  <div class="bandeau ok" role="status" data-testid="bandeau-en-ligne">✔ La base répond de nouveau : modifications de nouveau possibles.</div>
{/if}

<style>
  .bandeau {
    display: flex; flex-wrap: wrap; align-items: center; gap: 4px 12px; padding: 6px 14px; font-size: 0.9em;
    border-bottom: 1px solid var(--cr-bordure); background: var(--cr-fond-panneau);
  }
  .hors { border-bottom-color: var(--cr-erreur); }
  .hors .texte { color: var(--cr-erreur); font-weight: 600; }
  .ok { color: var(--cr-accent); }
  .discret, .info { color: var(--cr-texte-discret); }
  .cause { color: var(--cr-texte-discret); flex: 1 1 12em; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  button { margin-left: auto; }
</style>
