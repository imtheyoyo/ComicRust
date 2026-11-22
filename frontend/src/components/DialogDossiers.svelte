<script lang="ts">
  // Dossiers surveillés : liste, ajout, retrait, lancement du scan avec barre de progression.
  import { fermerModale, modale } from '../lib/modale';
  import { ajouterDossier, lancerScan, listerDossiers, retirerDossier, messageErreur } from '../lib/api';
  import type { SuiviScan } from '../lib/bibliotheque.svelte';
  import { choisirDossier, dialogueDisponible } from '../lib/dialogue';

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
      erreur = messageErreur(e);
    }
  }

  async function agir(action: () => Promise<string[] | void>) {
    erreur = '';
    try {
      const liste = await action();
      if (liste) dossiers = liste;
      else await actualiser();
    } catch (e) {
      erreur = messageErreur(e);
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

  /** « Parcourir… » : dialogue natif de dossier, puis ajout direct du dossier choisi. */
  async function parcourir() {
    const chemin = await choisirDossier({ titre: 'Ajouter un dossier de BD', cheminInitial: nouveau });
    if (!chemin) return;
    nouveau = chemin;
    await ajouter();
  }

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

  /** Résultat de « Copier le bilan » : message discret, ou erreur si le presse-papiers est refusé. */
  let copie = $state('');
  async function copierBilan() {
    const texte = scan.bilan?.texte;
    if (!texte) return;
    copie = '';
    try {
      await navigator.clipboard.writeText(texte);
      copie = 'Bilan copié dans le presse-papiers.';
    } catch {
      // Repli : champ temporaire sélectionné (presse-papiers asynchrone refusé par la WebView).
      const zone = document.createElement('textarea');
      zone.value = texte;
      zone.setAttribute('readonly', '');
      zone.style.position = 'fixed';
      zone.style.opacity = '0';
      document.body.appendChild(zone);
      zone.select();
      const ok = document.execCommand('copy');
      zone.remove();
      copie = ok ? 'Bilan copié dans le presse-papiers.' : 'Copie impossible : sélectionnez le texte du bilan à la main.';
    }
  }
</script>

<dialog bind:this={dialogue} use:modale={{ peutFermer: () => !nouveau.trim() }} aria-labelledby="titre-dossiers" data-testid="dialogue-dossiers">
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
    {#if dialogueDisponible()}<button type="button" onclick={() => void parcourir()} data-testid="dossiers-parcourir">Parcourir…</button>{/if}
    <button type="submit" disabled={!nouveau.trim()}>Ajouter</button>
  </form>

  <div class="scan">
    <button onclick={lancer} disabled={scan.enCours || dossiers.length === 0}>
      {scan.enCours ? 'Scan en cours…' : 'Lancer le scan'}
    </button>
    {#if scan.enCours}
      <button onclick={() => void scan.arreterAnalyse()} disabled={scan.arretDemande} data-testid="arreter-analyse">
        {scan.arretDemande ? 'Arrêt en cours…' : 'Arrêter l\'analyse'}
      </button>
      <div class="barre-progression" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={pourcent} aria-label="Progression du scan">
        <div style:width="{pourcent}%"></div>
      </div>
      <span class="discret">{scan.fait.toLocaleString('fr-FR')} / {scan.total.toLocaleString('fr-FR')} fichiers</span>
    {:else if scan.resultat}
      <span class="discret" data-testid="dernier-scan">
        {scan.arretee ? 'Analyse arrêtée' : 'Dernier scan'} : {scan.resultat.ajoutes} ajoutés, {scan.resultat.misAJour} mis à jour, {scan.resultat.manquants} manquants.
      </span>
    {/if}
  </div>

  {#if !scan.enCours && scan.bilan}
    <section class="bilan" aria-labelledby="titre-bilan" data-testid="bilan-scan">
      <h3 id="titre-bilan">Bilan de l'analyse</h3>
      <p class="discret" data-testid="bilan-resume">{scan.bilan.resume}</p>
      <p class="discret">{scan.bilan.dossiers} dossier{scan.bilan.dossiers > 1 ? 's' : ''} analysé{scan.bilan.dossiers > 1 ? 's' : ''}.</p>
      {#if scan.bilan.erreurs.length > 0}
        <p class="titre-erreurs">Erreurs et avertissements ({scan.bilan.erreursTotal.toLocaleString('fr-FR')})</p>
        <ul class="erreurs" data-testid="bilan-erreurs">
          {#each scan.bilan.erreurs as e, i (i)}
            <li><code title={e.chemin}>{e.chemin}</code><span>{e.message}</span></li>
          {/each}
        </ul>
        {#if scan.bilan.erreursTotal > scan.bilan.erreurs.length}
          <p class="discret">… et {(scan.bilan.erreursTotal - scan.bilan.erreurs.length).toLocaleString('fr-FR')} autre(s) non détaillée(s).</p>
        {/if}
      {:else}
        <p class="discret">Aucune erreur.</p>
      {/if}
      <button onclick={() => void copierBilan()} data-testid="copier-bilan">📋 Copier le bilan</button>
      {#if copie}<p class="discret" role="status">{copie}</p>{/if}
    </section>
  {/if}

  {#if scan.erreur}<p class="erreur" role="alert">Échec du scan : {scan.erreur}</p>{/if}
  {#if erreur}<p class="erreur" role="alert">{erreur}</p>{/if}

  <div class="pied"><button onclick={() => fermerModale(dialogue)}>Fermer</button></div>
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
  .bilan { display: grid; gap: 6px; justify-items: start; margin-bottom: 14px; padding: 8px 10px; border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); }
  .bilan h3 { margin: 0; font-size: 14px; }
  .titre-erreurs { margin: 4px 0 0; font-weight: 600; }
  .erreurs { list-style: none; margin: 0; padding: 0; width: 100%; max-height: 220px; overflow-y: auto; display: grid; gap: 4px; }
  .erreurs li { display: grid; gap: 1px; padding: 4px 8px; border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); font-size: 12px; }
  .erreurs li code { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .erreurs li span { color: var(--cr-erreur); }
  .erreur { color: var(--cr-erreur); }
  .pied { display: flex; justify-content: flex-end; margin: 0; }
</style>
