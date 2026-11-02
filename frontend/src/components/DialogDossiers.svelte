<script lang="ts">
  // Dossiers surveillés : liste, ajout, retrait, lancement du scan avec barre de progression.
  import { ajouterDossier, lancerScan, listerDossiers, retirerDossier } from '../lib/api';
  import type { SuiviScan } from '../lib/bibliotheque.svelte';

  interface Props {
    scan: SuiviScan;
  }
  let { scan }: Props = $props();

  let dialogue = $state<HTMLDialogElement>();
  let dossiers = $state<string[]>([]);
  let nouveau = $state('');
  let erreur = $state('');

  export async function ouvrir() {
    dialogue?.showModal();
    await actualiser();
  }

  async function actualiser() {
    try {
      dossiers = await listerDossiers();
    } catch (e) {
      erreur = String(e);
    }
  }

  async function agir(action: () => Promise<string[] | void>) {
    erreur = '';
    try {
      const liste = await action();
      if (liste) dossiers = liste;
      else await actualiser();
    } catch (e) {
      erreur = String(e);
    }
  }

  const ajouter = () =>
    agir(async () => {
      const chemin = nouveau.trim();
      if (!chemin) return;
      const liste = await ajouterDossier(chemin);
      nouveau = '';
      return liste;
    });

  const lancer = () =>
    agir(async () => {
      scan.annoncer();
      try {
        await lancerScan();
      } catch (e) {
        scan.enCours = false;
        throw e;
      }
    });

  const pourcent = $derived(scan.total > 0 ? Math.round((scan.fait / scan.total) * 100) : 0);
</script>

<dialog bind:this={dialogue} aria-labelledby="titre-dossiers" data-testid="dialogue-dossiers">
  <h2 id="titre-dossiers">Dossiers surveillés</h2>

  {#if dossiers.length === 0}
    <p class="discret">Aucun dossier surveillé : ajoutez un dossier contenant vos BD.</p>
  {:else}
    <ul>
      {#each dossiers as d (d)}
        <li>
          <code title={d}>{d}</code>
          <button onclick={() => agir(() => retirerDossier(d))} aria-label="Retirer {d}">Retirer</button>
        </li>
      {/each}
    </ul>
  {/if}

  <form class="ajout" onsubmit={(e) => { e.preventDefault(); void ajouter(); }}>
    <input type="text" bind:value={nouveau} placeholder="Chemin du dossier à ajouter" aria-label="Chemin du dossier à ajouter" spellcheck="false" autocomplete="off" />
    <button type="submit" disabled={!nouveau.trim()}>Ajouter</button>
  </form>

  <div class="scan">
    <button onclick={lancer} disabled={scan.enCours || dossiers.length === 0}>
      {scan.enCours ? 'Scan en cours…' : 'Lancer le scan'}
    </button>
    {#if scan.enCours}
      <div class="barre-progression" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={pourcent} aria-label="Progression du scan">
        <div style:width="{pourcent}%"></div>
      </div>
      <span class="discret">{scan.fait.toLocaleString('fr-FR')} / {scan.total.toLocaleString('fr-FR')} fichiers</span>
    {:else if scan.resultat}
      <span class="discret">
        Dernier scan : {scan.resultat.ajoutes} ajoutés, {scan.resultat.misAJour} mis à jour, {scan.resultat.manquants} manquants.
      </span>
    {/if}
  </div>

  {#if scan.erreur}<p class="erreur" role="alert">Échec du scan : {scan.erreur}</p>{/if}
  {#if erreur}<p class="erreur" role="alert">{erreur}</p>{/if}

  <form method="dialog" class="pied"><button>Fermer</button></form>
</dialog>

<style>
  ul { list-style: none; margin: 0 0 12px; padding: 0; display: grid; gap: 6px; }
  li { display: flex; align-items: center; gap: 10px; justify-content: space-between; padding: 6px 10px; border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); }
  code { font-family: ui-monospace, Consolas, monospace; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ajout { display: flex; gap: 8px; margin-bottom: 14px; }
  .ajout input { flex: 1; min-width: 0; }
  .scan { display: grid; gap: 8px; justify-items: start; margin-bottom: 14px; }
  .scan .barre-progression { width: 100%; }
  .discret { color: var(--cr-texte-discret); margin: 0; }
  .erreur { color: var(--cr-erreur); }
  .pied { display: flex; justify-content: flex-end; margin: 0; }
</style>
