<script lang="ts">
  // Rapport de plantage de la session précédente (`data\plantage.txt`, comme la version egui : message
  // montré une fois au lancement suivant). « Copier » met le rapport complet dans le presse-papiers
  // (à joindre au rapport de recette) ; la fermeture supprime le fichier. Créé au démarrage de
  // l'application, avant même le choix de la bibliothèque.
  import { onMount } from 'svelte';
  import { acquitterPlantage, lirePlantage, type RapportPlantage } from '../lib/api';
  import { modale } from '../lib/modale';

  let rapport = $state<RapportPlantage | null>(null);
  let copie = $state(false);
  let echecCopie = $state(false);
  let texte = $state<HTMLTextAreaElement>();

  onMount(() => {
    // Jamais bloquant : un échec de lecture du rapport ne gêne pas le démarrage.
    void lirePlantage()
      .then((r) => (rapport = r))
      .catch((e) => console.warn('rapport de plantage', e));
  });

  async function copier() {
    echecCopie = false;
    try {
      await navigator.clipboard.writeText(rapport?.texte ?? '');
      copie = true;
      setTimeout(() => (copie = false), 1500);
    } catch {
      // Presse-papiers refusé : le texte est sélectionné, Ctrl+C le copie.
      echecCopie = true;
      texte?.focus();
      texte?.select();
    }
  }

  /** Fermeture (bouton ou Échap) : le fichier est supprimé, le rapport ne reviendra pas. */
  function fermer() {
    rapport = null;
    void acquitterPlantage().catch((e) => console.warn('suppression du rapport de plantage', e));
  }
</script>

{#if rapport}
  <dialog use:modale={{ ouvrirAuMontage: true, fermer }} aria-labelledby="titre-plantage" data-testid="dialogue-plantage">
    <h2 id="titre-plantage">ComicRust s'est fermé de façon inattendue</h2>
    <p>Lors de la dernière utilisation, ComicRust a rencontré une erreur et s'est arrêté. Le rapport ci-dessous peut être copié et joint au rapport de recette.</p>
    <pre class="resume" data-testid="plantage-resume">{rapport.resume}</pre>
    <details>
      <summary>Rapport complet</summary>
      <textarea bind:this={texte} readonly rows="12" spellcheck="false" aria-label="Rapport complet" data-testid="plantage-texte">{rapport.texte}</textarea>
    </details>
    <p class="discret">Fichier : {rapport.fichier} (supprimé à la fermeture de cette fenêtre).</p>
    {#if echecCopie}<p class="alerte" role="alert">Copie impossible : le rapport est sélectionné, utilisez Ctrl+C.</p>{/if}
    <div class="pied">
      <button type="button" onclick={() => void copier()} data-testid="plantage-copier">{copie ? 'Copié ✓' : 'Copier'}</button>
      <button type="button" class="principal" onclick={fermer} data-testid="plantage-fermer">Fermer</button>
    </div>
  </dialog>
{/if}

<style>
  .resume { margin: 8px 0; padding: 8px; white-space: pre-wrap; overflow-wrap: anywhere; font-size: 12px; background: var(--cr-fond-survol); border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); }
  details { margin: 8px 0; }
  textarea { width: 100%; box-sizing: border-box; margin-top: 6px; font-family: ui-monospace, Consolas, monospace; font-size: 11px; resize: vertical; }
  .discret { color: var(--cr-texte-discret); font-size: 12px; overflow-wrap: anywhere; }
  .alerte { color: var(--cr-erreur); }
  .pied { display: flex; justify-content: flex-end; gap: 8px; margin-top: 12px; }
  .pied .principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); font-weight: 600; }
</style>
