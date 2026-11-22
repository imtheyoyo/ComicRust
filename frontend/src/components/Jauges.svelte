<script lang="ts">
  // Jauges d'une liste, comme ComicRack : trois petits compteurs colorés à droite du nom
  // (rouge : nouveaux livres, jaune : livres non lus, vert : total de livres). Les compteurs
  // « nouveaux » et « non lus » à zéro sont masqués (le total reste toujours visible). Texte
  // alternatif complet pour les lecteurs d'écran : « 12 nouveaux, 40 non lus, 120 livres ».
  import type { Jauge } from '../lib/api';
  import { libelleJauge } from '../lib/jauges.svelte';

  interface Props {
    jauge: Jauge;
  }
  let { jauge }: Props = $props();

  const nombre = (n: number) => n.toLocaleString('fr-FR');
  const libelle = $derived(libelleJauge(jauge));
</script>

<span class="jauges" role="img" aria-label={libelle} title={libelle} data-testid="jauges">
  {#if jauge.nouveaux > 0}<span class="pastille nouveaux" aria-hidden="true" data-testid="jauge-nouveaux">{nombre(jauge.nouveaux)}</span>{/if}
  {#if jauge.nonLus > 0}<span class="pastille non-lus" aria-hidden="true" data-testid="jauge-non-lus">{nombre(jauge.nonLus)}</span>{/if}
  <span class="pastille total" aria-hidden="true" data-testid="jauge-total">{nombre(jauge.total)}</span>
</span>

<style>
  .jauges { display: inline-flex; align-items: center; gap: 2px; flex: none; margin-left: auto; }
  .pastille {
    min-width: 18px; padding: 0 5px; border-radius: 9px; text-align: center;
    font-size: 11px; font-weight: 600; line-height: 16px; font-variant-numeric: tabular-nums;
  }
  .nouveaux { background: var(--cr-jauge-nouveaux-fond); color: var(--cr-jauge-nouveaux-texte); }
  .non-lus { background: var(--cr-jauge-non-lus-fond); color: var(--cr-jauge-non-lus-texte); }
  .total { background: var(--cr-jauge-total-fond); color: var(--cr-jauge-total-texte); }
</style>
