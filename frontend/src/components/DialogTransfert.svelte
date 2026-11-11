<script lang="ts">
  // « Transférer la bibliothèque ComicRack » (réglages) : copie des livres (avec leur lecture, leurs
  // notes…), des listes et des dossiers de ComicRack dans la base de ComicRust. ComicRack n'est que
  // lu ; le mot de passe MySQL reste dans le backend. Étapes : choix, progression (transfert-
  // progression, Arrêter), bilan. Contrat : docs/tauri/ipc-transfert.md.
  import { fermerModale, modale } from '../lib/modale';
  import { onDestroy } from 'svelte';
  import {
    annulerTransfert, apercuTransfert, ecouter, lancerTransfert, messageErreur, type ApercuTransfert, type EtatBiblio,
    type ProgressionTransfert, type ResumeTransfert, type SourceTransfert,
  } from '../lib/api';

  interface Props {
    /** La bibliothèque a été rouverte par le backend (bascule en autonome, rechargement). */
    onbibliotheque?: (etat: EtatBiblio) => void;
  }
  let { onbibliotheque }: Props = $props();

  const MAX_AVERTISSEMENTS = 10;

  let dialogue = $state<HTMLDialogElement>();
  let etape = $state<'choix' | 'cours' | 'bilan'>('choix');
  let apercu = $state<ApercuTransfert | null>(null);
  let source = $state<SourceTransfert>('comicdb');
  let comicdb = $state('');
  let listes = $state(true);
  let dossiers = $state(true);
  let basculer = $state(true);
  let progression = $state<ProgressionTransfert | null>(null);
  let resume = $state<ResumeTransfert | null>(null);
  let erreur = $state('');
  let arretDemande = $state(false);
  let desabonner: (() => void) | undefined;

  const pourcent = $derived(progression && progression.total ? Math.round((progression.fait / progression.total) * 100) : 0);
  const peutLancer = $derived(!apercu?.bloque && (source === 'mysql' ? !!apercu?.mysql : !!comicdb.trim()));
  const nb = (n: number | undefined) => (n ?? 0).toLocaleString('fr-FR');

  export async function ouvrir() {
    if (!dialogue?.open) {
      // Une fenêtre fermée pendant un bilan repart du choix ; pendant un transfert, elle reste telle quelle.
      if (etape !== 'cours') etape = 'choix';
      resume = null;
      erreur = '';
      dialogue?.showModal();
    }
    if (etape === 'cours') return;
    try {
      const a = await apercuTransfert();
      apercu = a;
      source = a.sourceParDefaut;
      comicdb = a.comicdb;
      basculer = a.mode === 'comicrack' || !a.destinationExiste;
    } catch (e) {
      erreur = messageErreur(e);
    }
  }

  async function lancer() {
    erreur = '';
    arretDemande = false;
    progression = null;
    etape = 'cours';
    desabonner?.();
    desabonner = await ecouter<ProgressionTransfert>('transfert-progression', (p) => (progression = p));
    try {
      const r = await lancerTransfert({ source, comicdb: comicdb.trim() || undefined, listes, dossiers, basculer });
      resume = r;
      etape = 'bilan';
      if (r.etat) onbibliotheque?.(r.etat);
    } catch (e) {
      erreur = messageErreur(e);
      etape = 'choix';
    } finally {
      desabonner?.();
      desabonner = undefined;
    }
  }

  function arreter() {
    arretDemande = true;
    void annulerTransfert();
  }

  onDestroy(() => desabonner?.());

  /** Échap : pas pendant le transfert (le bilan doit arriver). */
  const peutFermer = () => etape !== 'cours';
</script>

<dialog bind:this={dialogue} use:modale={{ peutFermer }} aria-labelledby="titre-transfert" data-testid="dialogue-transfert">
  <h2 id="titre-transfert">Transférer la bibliothèque ComicRack</h2>

  {#if etape === 'choix'}
    <p>Copie les livres de ComicRack (infos, lecture, notes…), ses listes et ses dossiers dans la base de ComicRust, pour
      pouvoir se passer de ComicRack. <strong>ComicRack n'est pas modifié</strong> : il n'est que lu.</p>
    {#if !apercu && !erreur}
      <p class="discret">Recherche de ComicRack…</p>
    {:else if apercu}
      <fieldset>
        <legend>Depuis ComicRack</legend>
        {#if apercu.mysql}
          <label class="case">
            <input type="radio" name="source-transfert" value="mysql" bind:group={source} data-testid="source-mysql" />
            Base MySQL de ComicRack (<code>{apercu.mysql}</code>)
          </label>
        {/if}
        <label class="case">
          <input type="radio" name="source-transfert" value="comicdb" bind:group={source} data-testid="source-comicdb" />
          ComicDb.xml (ComicRack sans MySQL)
        </label>
        <label class="chemin">
          <span>{source === 'mysql' ? 'ComicDb.xml (listes, dossiers) :' : 'ComicDb.xml :'}</span>
          <input type="text" bind:value={comicdb} spellcheck="false" autocomplete="off" data-testid="chemin-comicdb"
            placeholder="Chemin de ComicDb.xml" />
        </label>
        {#if !apercu.mysql}
          <p class="discret petit">Aucune base MySQL trouvée dans ComicRack.ini : si ComicRack range ses livres dans MySQL,
            indique son ComicRack.ini à la première ouverture (mode lié) pour que la base soit proposée ici.</p>
        {/if}
      </fieldset>

      <fieldset>
        <legend>Vers ComicRust</legend>
        <p class="destination" data-testid="destination-transfert"><code>{apercu.destination}</code></p>
        {#if apercu.destinationExiste}
          <p class="discret petit">Cette base contient déjà {nb(apercu.nbLivresDestination)} livres. Relancer un transfert ne duplique rien : les
            fiches sont reprises de ComicRack, mais la lecture faite dans ComicRust entre-temps et les fichiers convertis ou
            déplacés sont gardés. Les modifications de fiches faites dans ComicRust seront remplacées.</p>
        {/if}
        <label class="case"><input type="checkbox" bind:checked={listes} data-testid="transfert-listes" /> Listes (dossiers, listes de lecture, listes intelligentes)</label>
        <label class="case"><input type="checkbox" bind:checked={dossiers} data-testid="transfert-dossiers" /> Dossiers de BD surveillés par ComicRack</label>
        <label class="case"><input type="checkbox" bind:checked={basculer} data-testid="transfert-basculer" /> Ensuite, utiliser ComicRust en autonome (sans ComicRack)</label>
      </fieldset>

      {#if apercu.bloque}<p class="erreur" role="alert">{apercu.bloque}</p>{/if}
    {/if}
    {#if erreur}<p class="erreur" role="alert" data-testid="erreur-transfert">{erreur}</p>{/if}
    <div class="pied">
      <button onclick={() => fermerModale(dialogue)}>Fermer</button>
      <button class="principal" disabled={!peutLancer} onclick={() => void lancer()} data-testid="transfert-lancer">Transférer</button>
    </div>
  {:else if etape === 'cours'}
    <p aria-live="polite" data-testid="phase-transfert">{progression?.phase || 'préparation…'}</p>
    <div class="barre-progression" role="progressbar" aria-label="Progression du transfert" aria-valuemin="0" aria-valuemax="100" aria-valuenow={pourcent}>
      <div style:width="{pourcent}%"></div>
    </div>
    <p class="discret">{nb(progression?.fait)} / {nb(progression?.total)}</p>
    <div class="pied">
      <button onclick={arreter} disabled={arretDemande} title="Les lots déjà écrits sont gardés ; relancer le transfert le termine" data-testid="transfert-arreter">
        {arretDemande ? 'Arrêt en cours…' : 'Arrêter'}
      </button>
    </div>
  {:else if resume}
    <p class:ok={!resume.interrompu} class:avert={resume.interrompu} role="status" data-testid="resume-transfert">
      {resume.interrompu ? '⏹' : '✔'} {resume.resume}
    </p>
    <dl class="bilan">
      <dt>Livres dans ComicRust</dt><dd data-testid="bilan-livres">{nb(resume.nbLivresDestination)}</dd>
      <dt>Listes</dt><dd data-testid="bilan-listes">{nb(resume.listes)}</dd>
      <dt>Doublons évités</dt><dd data-testid="bilan-doublons-evites">{nb(resume.doublonsEvites)}</dd>
      <dt>Doublons restants</dt><dd data-testid="bilan-doublons">{resume.doublonsRestants === undefined ? '—' : nb(resume.doublonsRestants)}</dd>
    </dl>
    {#if resume.bascule}<p class="discret">ComicRust affiche maintenant sa propre bibliothèque (autonome).</p>{/if}
    {#each resume.avertissements.slice(0, MAX_AVERTISSEMENTS) as a (a)}
      <p class="avert petit">⚠ {a}</p>
    {/each}
    {#if resume.avertissements.length > MAX_AVERTISSEMENTS}
      <p class="discret petit">… et {resume.avertissements.length - MAX_AVERTISSEMENTS} autres avertissements.</p>
    {/if}
    <div class="pied">
      <button class="principal" onclick={() => fermerModale(dialogue)} data-testid="transfert-fermer">Fermer</button>
    </div>
  {/if}
</dialog>

<style>
  dialog { width: min(600px, 92vw); }
  p { margin: 0 0 10px; line-height: 1.45; }
  fieldset { border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); margin: 0 0 12px; padding: 8px 12px; }
  legend { padding: 0 4px; font-weight: 600; }
  .case { display: flex; gap: 8px; align-items: flex-start; margin-bottom: 6px; }
  .case input { margin-top: 3px; accent-color: var(--cr-accent); }
  .chemin { display: grid; gap: 4px; margin: 4px 0 6px; }
  .chemin input { min-width: 0; }
  .destination { margin-bottom: 6px; word-break: break-all; }
  code { font-family: ui-monospace, Consolas, monospace; background: var(--cr-fond-survol); padding: 1px 5px; border-radius: 3px; }
  .discret { color: var(--cr-texte-discret); }
  .petit { font-size: 12px; }
  .erreur { color: var(--cr-erreur); }
  .ok { color: var(--cr-accent); }
  .avert { color: var(--cr-texte); }
  .bilan { display: grid; grid-template-columns: max-content 1fr; gap: 4px 16px; margin: 0 0 12px; }
  .bilan dt { color: var(--cr-texte-discret); }
  .bilan dd { margin: 0; font-variant-numeric: tabular-nums; }
  .pied { display: flex; justify-content: flex-end; gap: 8px; margin-top: 12px; }
  button.principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); font-weight: 600; }
  button.principal:hover:not(:disabled) { background: var(--cr-accent); filter: brightness(1.1); }
</style>
