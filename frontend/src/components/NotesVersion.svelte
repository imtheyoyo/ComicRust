<script lang="ts">
  // Notes de version rendues en texte simple et sûr (voir lib/notesMaj.ts) : aucun innerHTML,
  // les liens ne sont cliquables que vers http(s).
  import { blocsNotes, type Segment } from '../lib/notesMaj';
  import { majOuvrirPage } from '../lib/api';

  let { notes }: { notes: string } = $props();
  const blocs = $derived(blocsNotes(notes));
</script>

{#snippet ligne(segs: Segment[])}
  {#each segs as s, i (i)}
    {#if s.type === 'gras'}<strong>{s.texte}</strong>
    {:else if s.type === 'code'}<code>{s.texte}</code>
    {:else if s.type === 'lien' && s.url}<a href={s.url} title={s.url} onclick={(e) => { e.preventDefault(); if (s.url) void majOuvrirPage(s.url).catch(() => {}); }}>{s.texte}</a>
    {:else if s.type === 'lien'}{s.texte}
    {:else}{s.texte}{/if}
  {/each}
{/snippet}

<div class="notes" data-testid="maj-notes">
  {#each blocs as b, i (i)}
    {#if b.type === 'titre'}
      <p class="titre" class:grand={b.niveau <= 2} role="heading" aria-level={Math.min(6, b.niveau + 2)}>{@render ligne(b.segments)}</p>
    {:else if b.type === 'liste'}
      {#if b.ordonnee}
        <ol>{#each b.elements as e, j (j)}<li>{@render ligne(e)}</li>{/each}</ol>
      {:else}
        <ul>{#each b.elements as e, j (j)}<li>{@render ligne(e)}</li>{/each}</ul>
      {/if}
    {:else}
      <p>{@render ligne(b.segments)}</p>
    {/if}
  {:else}
    <p class="discret">Pas de notes pour cette version.</p>
  {/each}
</div>

<style>
  .notes {
    max-height: 220px; overflow: auto; padding: 6px 10px; margin: 6px 0;
    border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); background: var(--cr-fond-panneau);
    overflow-wrap: anywhere;
  }
  p { margin: 4px 0; }
  .titre { font-weight: 600; margin-top: 8px; }
  .titre.grand { font-size: 1.05em; }
  ul, ol { margin: 4px 0; padding-left: 22px; }
  li { margin: 2px 0; }
  code { font-family: ui-monospace, Consolas, monospace; background: var(--cr-fond-survol); padding: 0 4px; border-radius: 3px; }
  a { color: var(--cr-accent); }
  .discret { color: var(--cr-texte-discret); }
</style>
