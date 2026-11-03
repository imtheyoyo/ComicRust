<script lang="ts" module>
  import type { ProgressionEdition, Statut } from '../lib/api';

  export const LIBELLES_STATUT: Record<Statut, string> = {
    ok: 'OK', inchange: 'Déjà à jour', ignore: 'Ignoré', erreur: 'Erreur',
  };

  /** Pourcentage d'une progression (0 sans total). */
  export const pourcent = (p: ProgressionEdition | null) => (p && p.total ? Math.round((p.fait / p.total) * 100) : 0);
</script>

<script lang="ts">
  // Bilan d'une opération d'édition (BilanOperation) : résumé, résultats par livre, rapport à copier.
  import type { BilanOperation } from '../lib/api';

  interface Props {
    bilan: BilanOperation;
    /** Liste des livres repliée au départ (un seul livre, tout réussi). */
    replie?: boolean;
  }
  let { bilan, replie = false }: Props = $props();

  let copie = $state(false);
  const problemes = $derived(bilan.livres.filter((l) => l.statut === 'erreur').length);

  async function copier() {
    try {
      await navigator.clipboard.writeText(`${bilan.resume}\n${bilan.rapport}`);
      copie = true;
      setTimeout(() => (copie = false), 1500);
    } catch {
      /* presse-papiers refusé : le rapport reste lisible dans la liste */
    }
  }
</script>

<div class="bilan" class:echec={!bilan.complet} data-testid="bilan" role="status">
  <p class="resume">
    {bilan.complet ? '✓' : '⚠'} {bilan.resume}{bilan.interrompu ? ' (opération arrêtée)' : ''}
  </p>
  {#if bilan.livres.length > 0}
    <details open={!replie || problemes > 0}>
      <summary>Résultats par livre ({bilan.livres.length.toLocaleString('fr-FR')}{problemes ? `, ${problemes} erreur(s)` : ''})</summary>
      <ul class="livres" data-testid="bilan-livres">
        {#each bilan.livres as r (r.id)}
          <li class="statut-{r.statut}">
            <span class="pastille">{LIBELLES_STATUT[r.statut]}</span>
            <span class="titre" title={r.titre}>{r.titre}</span>
            {#if r.message || r.fichier}
              <span class="detail" title={r.fichier}>{r.message ?? ''}{r.message && r.fichier ? ' · ' : ''}{r.fichier ?? ''}</span>
            {/if}
          </li>
        {/each}
      </ul>
    </details>
    <button class="copier" onclick={copier}>{copie ? 'Rapport copié' : 'Copier le rapport'}</button>
  {/if}
</div>

<style>
  .bilan { border: 1px solid var(--cr-bordure); border-left: 4px solid var(--cr-accent); border-radius: var(--cr-rayon); padding: 8px 10px; background: var(--cr-fond); }
  .bilan.echec { border-left-color: var(--cr-erreur); }
  .resume { margin: 0 0 6px; font-weight: 600; }
  .echec .resume { color: var(--cr-erreur); }
  summary { cursor: pointer; color: var(--cr-texte-discret); }
  .livres { list-style: none; margin: 6px 0; padding: 0; max-height: 180px; overflow-y: auto; font-size: 12px; }
  .livres li { display: grid; grid-template-columns: 86px minmax(0, 1fr); column-gap: 8px; padding: 3px 0; border-bottom: 1px solid var(--cr-bordure); }
  .livres .detail { grid-column: 2; color: var(--cr-texte-discret); overflow-wrap: anywhere; }
  .titre { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pastille { font-weight: 600; color: var(--cr-texte-discret); }
  .statut-ok .pastille { color: var(--cr-accent); }
  .statut-erreur .pastille, .statut-erreur .detail { color: var(--cr-erreur); }
  .copier { font-size: 12px; padding: 3px 8px; }
</style>
