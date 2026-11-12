<script lang="ts">
  // Fenêtre « Mise à jour de ComicRust » (comme `update_window` de la version egui) : version
  // installée, vérification au démarrage, recherche manuelle, notes de version, téléchargement avec
  // progression en Mo, bilan et redémarrage. Créée à la demande (`miseAJour.fenetre`).
  import { majOuvrirPage } from '../lib/api';
  import { fermerModale, modale } from '../lib/modale';
  import { enMo, miseAJour } from '../lib/maj.svelte';
  import NotesVersion from './NotesVersion.svelte';

  let dialogue = $state<HTMLDialogElement>();
  const etat = $derived(miseAJour.etat);
  const statut = $derived(miseAJour.statutAffiche);
  const release = $derived(statut.release);
  const installable = $derived(!!etat?.installable);
  const ignoree = $derived(!!release && etat?.versionIgnoree === release.tag);
  const fraction = $derived.by(() => {
    const p = miseAJour.progression;
    return p && p.total > 0 ? Math.min(1, p.recu / p.total) : 0;
  });

  /** Bilan de l'installation : fichiers remplacés, anciens fichiers (.old) encore utilisés. */
  function texteBilan(b: { remplaces: number; restesOld: number }): string {
    const s = (n: number) => (n > 1 ? 's' : '');
    let t = `${b.remplaces} fichier${s(b.remplaces)} remplacé${s(b.remplaces)}.`;
    if (b.restesOld > 0) {
      t += ` ${b.restesOld} ancien${s(b.restesOld)} fichier${s(b.restesOld)} (.old) encore utilisé${s(b.restesOld)} : `
        + `${b.restesOld > 1 ? 'ils seront supprimés' : 'il sera supprimé'} au prochain démarrage.`;
    }
    return t;
  }
</script>

<dialog bind:this={dialogue} use:modale={{ fermer: () => miseAJour.fermer(), ouvrirAuMontage: true }} aria-labelledby="titre-maj" data-testid="dialog-maj">
  <h2 id="titre-maj">Mise à jour de ComicRust</h2>
  {#if etat}
    <p class="ligne">Version installée : <strong data-testid="maj-version">{etat.version}</strong></p>
    {#if etat.versionComparee && etat.versionComparee !== etat.version}
      <p class="avertissement">Mode recette : comparée aux releases comme la version {etat.versionComparee}.</p>
    {/if}
    <label class="case">
      <input type="checkbox" checked={etat.verifierAuDemarrage} onchange={(e) => void miseAJour.reglerDemarrage(e.currentTarget.checked)} data-testid="maj-auto" />
      Vérifier les mises à jour au démarrage
    </label>
  {:else}
    <p class="discret">Lecture de l'état de la mise à jour…</p>
  {/if}

  <div class="etat" aria-live="polite" data-testid="maj-etat" data-etat={statut.etat}>
    {#if statut.etat === 'inactif'}
      <p class="discret">Aucune vérification en cours.</p>
    {:else if statut.etat === 'verification'}
      <p><span class="roue" aria-hidden="true"></span> Recherche d'une nouvelle version sur GitHub…</p>
    {:else if statut.etat === 'aJour'}
      <p class="ok">✔ Tu as la dernière version.</p>
    {:else if statut.etat === 'disponible' && release}
      <p><strong>ComicRust {release.tag} est disponible.</strong>{#if release.nom && release.nom !== `ComicRust ${release.tag}`} ({release.nom}){/if}</p>
      <NotesVersion notes={release.notes} />
      <p><a href={release.urlPage} onclick={(e) => { e.preventDefault(); void majOuvrirPage(release.urlPage).catch(() => {}); }} data-testid="maj-page">Page de la release sur GitHub</a></p>
      {#if ignoree}
        <p class="discret petit">Cette version est ignorée : le bandeau ne la propose plus.
          <button type="button" class="lien" onclick={() => void miseAJour.neplusIgnorer()} data-testid="maj-ne-plus-ignorer">Ne plus l'ignorer</button></p>
      {/if}
      {#if !release.aPaquet}
        <p>Cette version n'a pas de paquet Windows : télécharge-la depuis GitHub.</p>
      {:else}
        <button type="button" class="principal" disabled={!installable || miseAJour.occupe} onclick={() => void miseAJour.installer()} data-testid="maj-installer">
          Télécharger et installer ({enMo(release.taille)})
        </button>
        {#if installable}
          <p class="discret petit">Le paquet est vérifié (SHA-256) avant d'installer. Tes réglages (dossier data) sont conservés.</p>
        {:else}
          <p class="avertissement petit" data-testid="maj-non-installable">L'installation automatique n'est possible que dans la version Windows de ComicRust
            (pas dans cette version de développement ou hors Windows) : télécharge le paquet depuis la page de la release.</p>
        {/if}
      {/if}
    {:else if statut.etat === 'installation'}
      <p>Installation de ComicRust {release?.tag ?? ''}…</p>
      <div class="barre" role="progressbar" aria-label="Téléchargement de la mise à jour" aria-valuemin="0" aria-valuemax="100"
        aria-valuenow={Math.round(fraction * 100)} data-testid="maj-progression">
        <div class="rempli" style:width="{fraction * 100}%"></div>
        <span class="texte-barre">{enMo(miseAJour.progression?.recu ?? 0)} / {enMo(miseAJour.progression?.total ?? release?.taille ?? 0)}</span>
      </div>
    {:else if statut.etat === 'installee'}
      <p class="ok">✔ ComicRust {release?.tag ?? ''} est installée. Redémarre ComicRust pour l'utiliser.</p>
      {#if statut.bilan}
        <p class="petit" data-testid="maj-bilan">{texteBilan(statut.bilan)}</p>
      {/if}
      <button type="button" class="principal" onclick={() => void miseAJour.redemarrer()} data-testid="maj-redemarrer">Redémarrer maintenant</button>
    {:else if statut.etat === 'echec'}
      <p class="erreur" role="alert" data-testid="maj-erreur">✖ {statut.erreur || 'La mise à jour a échoué.'}</p>
      {#if release}<p><a href={release.urlPage} onclick={(e) => { e.preventDefault(); void majOuvrirPage(release.urlPage).catch(() => {}); }}>Page de la release sur GitHub</a></p>{/if}
    {/if}
  </div>
  {#if miseAJour.erreurCommande}<p class="erreur" role="alert" data-testid="maj-erreur-commande">✖ {miseAJour.erreurCommande}</p>{/if}

  <div class="pied">
    <button type="button" onclick={() => void miseAJour.verifier()} disabled={miseAJour.occupe} data-testid="maj-rechercher">Rechercher maintenant</button>
    <button type="button" onclick={() => fermerModale(dialogue)} data-testid="maj-fermer">Fermer</button>
  </div>
</dialog>

<style>
  dialog { width: min(560px, calc(100vw - 48px)); }
  h2 { margin-top: 0; }
  .ligne { margin: 0 0 6px; }
  .case { display: flex; gap: 8px; align-items: center; margin: 6px 0; }
  .case input { accent-color: var(--cr-accent); }
  .etat { border-top: 1px solid var(--cr-bordure); border-bottom: 1px solid var(--cr-bordure); margin: 10px 0; padding: 6px 0 10px; }
  .discret { color: var(--cr-texte-discret); }
  .petit { font-size: 12px; }
  .ok { color: var(--cr-accent); }
  .erreur { color: var(--cr-erreur); overflow-wrap: anywhere; }
  .avertissement { color: var(--cr-erreur); }
  a { color: var(--cr-accent); }
  button.principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); }
  button.principal:disabled { opacity: 0.5; }
  button.lien { background: none; border: 0; padding: 0; color: var(--cr-accent); text-decoration: underline; cursor: pointer; }
  .barre { position: relative; height: 22px; border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); background: var(--cr-fond-survol); overflow: hidden; }
  .rempli { position: absolute; inset: 0 auto 0 0; background: var(--cr-accent); opacity: 0.55; transition: width 0.1s linear; }
  .texte-barre { position: relative; display: block; text-align: center; line-height: 20px; font-size: 12px; font-variant-numeric: tabular-nums; color: var(--cr-texte); }
  .roue {
    display: inline-block; width: 12px; height: 12px; vertical-align: -1px; border-radius: 50%;
    border: 2px solid var(--cr-bordure); border-top-color: var(--cr-accent); animation: tourne 0.8s linear infinite;
  }
  @keyframes tourne { to { transform: rotate(360deg); } }
  .pied { display: flex; justify-content: flex-end; gap: 8px; }
</style>
