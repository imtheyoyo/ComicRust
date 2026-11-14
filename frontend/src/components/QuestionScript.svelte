<script lang="ts">
  // Question d'un script (événement script:question) : message (OK), confirmation (Oui / Non),
  // saisie de texte (masquée si secrète) ou choix dans une liste. Réponse transmise telle que la
  // veut le backend (docs/tauri/ipc-scripts.md) : texte → chaîne, choix → la chaîne choisie,
  // confirmation → booléen, null → valeur par défaut du script ; message → null.
  // Tout texte venant du script est affiché comme du texte (jamais d'innerHTML).
  import { untrack } from 'svelte';
  import { modale } from '../lib/modale';
  import type { QuestionScript } from '../lib/scripts.svelte';

  interface Props {
    question: QuestionScript;
    nomScript: string;
    onrepondre: (valeur: unknown) => void;
    onarreter: () => void;
  }
  let { question: qu, nomScript, onrepondre, onarreter }: Props = $props();

  // Valeurs lues une fois : le parent recrée ce composant pour chaque question ({#key reqId}).
  const detail = untrack(() => (qu.genre === 'ask' ? (qu.question ?? { type: 'confirmation' as const, defaut: null, choix: [], secret: false }) : null));
  const type = detail?.type ?? null;
  const secret = detail?.secret === true;
  const valeurDefaut = (): string => {
    const d = detail?.defaut;
    if (secret || d === null || d === undefined) return '';
    return typeof d === 'string' ? d : String(d);
  };
  /** Texte saisi (question texte). */
  let texte = $state(type === 'texte' ? valeurDefaut() : '');
  /** Choix retenu (question choix) : valeur par défaut si elle est proposée, sinon le premier. */
  let choisi = $state(type === 'choix' && detail ? (detail.choix.includes(valeurDefaut()) ? valeurDefaut() : (detail.choix[0] ?? '')) : '');
  /** Confirmation : bouton mis en avant selon la valeur par défaut (Oui si absente). */
  const nonParDefaut = type === 'confirmation' && detail?.defaut === false;

  const titre = $derived(qu.genre === 'message' ? 'Message du script'
    : type === 'texte' ? 'Saisie demandée par le script'
    : type === 'choix' ? 'Choix demandé par le script'
    : 'Question du script');

  function valider(ev: SubmitEvent) {
    ev.preventDefault();
    if (type === 'texte') onrepondre(texte);
    else if (type === 'choix') onrepondre(detail && detail.choix.length ? choisi : null);
  }
</script>

<dialog class="question" aria-labelledby="titre-question" aria-describedby="texte-question"
  use:modale={{ peutFermer: () => true, fermer: () => onrepondre(null), ouvrirAuMontage: true }}
  data-testid="question-script" data-type={type ?? 'message'}>
  <form onsubmit={valider}>
    <p class="discret petit">📜 {qu.titre || nomScript}</p>
    <h2 id="titre-question">{titre}</h2>
    <p id="texte-question" class="texte-question">{qu.texte}</p>

    {#if type === 'texte'}
      <!-- svelte-ignore a11y_autofocus -->
      <input class="saisie" type={secret ? 'password' : 'text'} bind:value={texte} autofocus spellcheck="false"
        autocomplete={secret ? 'new-password' : 'off'} aria-labelledby="texte-question" data-testid="question-texte" />
      {#if secret}<p class="discret petit">Saisie masquée. Laisser vide pour garder la valeur actuelle.</p>{/if}
    {:else if type === 'choix' && detail}
      {#if detail.choix.length === 0}
        <p class="discret">Aucun choix proposé par le script.</p>
      {:else}
        <!-- svelte-ignore a11y_autofocus -->
        <select class="saisie" bind:value={choisi} autofocus aria-labelledby="texte-question" data-testid="question-choix"
          size={Math.min(8, Math.max(2, detail.choix.length))} ondblclick={() => onrepondre(choisi)}>
          {#each detail.choix as c, k (k)}<option value={c}>{c}</option>{/each}
        </select>
      {/if}
    {/if}

    <footer>
      <button type="button" class="mini" onclick={onarreter}>⏹ Arrêter le script</button>
      <span class="espace"></span>
      {#if type === 'confirmation'}
        <!-- svelte-ignore a11y_autofocus -->
        <button type="button" class:principal={!nonParDefaut} autofocus={!nonParDefaut} onclick={() => onrepondre(true)} data-testid="question-oui">Oui</button>
        <!-- svelte-ignore a11y_autofocus -->
        <button type="button" class:principal={nonParDefaut} autofocus={nonParDefaut} onclick={() => onrepondre(false)} data-testid="question-non">Non</button>
      {:else if type === 'texte' || type === 'choix'}
        <button type="button" onclick={() => onrepondre(null)} title="Garder la valeur par défaut du script" data-testid="question-annuler">Annuler</button>
        <button type="submit" class="principal" disabled={type === 'choix' && !detail?.choix.length} data-testid="question-ok">OK</button>
      {:else}
        <!-- svelte-ignore a11y_autofocus -->
        <button type="button" class="principal" autofocus onclick={() => onrepondre(null)} data-testid="question-ok">OK</button>
      {/if}
    </footer>
  </form>
</dialog>

<style>
  .question { width: min(560px, calc(100vw - 32px)); }
  .question:not([open]) { display: none; }
  form { display: flex; flex-direction: column; gap: 8px; }
  h2 { margin: 0; overflow-wrap: anywhere; }
  p { margin: 0; }
  .discret { color: var(--cr-texte-discret); }
  .petit { font-size: 12px; }
  /* Aide longue (réglages de « Ranger les fichiers ») : jusqu’à 60 % de la hauteur. */
  .texte-question { white-space: pre-wrap; overflow-wrap: anywhere; max-height: min(60vh, 520px); overflow-y: auto; }
  .saisie {
    font: inherit; color: var(--cr-texte); background: var(--cr-fond); border: 1px solid var(--cr-bordure);
    border-radius: var(--cr-rayon); padding: 5px 8px; width: 100%; box-sizing: border-box;
  }
  .saisie:focus-visible { outline: 2px solid var(--cr-accent); }
  footer { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; border-top: 1px solid var(--cr-bordure); padding-top: 8px; margin-top: 4px; }
  .espace { flex: 1; }
  .mini { padding: 2px 8px; font-size: 12px; }
  button.principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); font-weight: 600; min-width: 80px; }
  button.principal:hover:not(:disabled) { background: var(--cr-accent); filter: brightness(1.1); }
</style>
