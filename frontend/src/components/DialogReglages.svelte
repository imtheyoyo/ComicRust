<script lang="ts">
  // Réglages : choix du thème (fichiers CSS de data\themes), dossiers du mode portable et, en mode
  // lié à ComicRack, l'option « Lecture et écriture » (section `edition` de data\comicrust-tauri.json).
  import { fermerModale, modale } from '../lib/modale';
  import {
    enregistrerReglagesEdition, infos, lireReglagesEdition, listerThemes, messageErreur, type InfosApplication,
    type ModeBiblio, type Theme,
  } from '../lib/api';
  import { appliquerTheme } from '../lib/theme';

  interface Props {
    themeActif: string;
    onTheme: (id: string) => void;
    /** Mode de la bibliothèque : l'option d'écriture ComicRack n'est proposée qu'en mode lié. */
    mode?: ModeBiblio;
    /** L'option « Lecture et écriture » a changé (les listes relisent leur état). */
    onecriture?: () => void;
    /** « Transférer la bibliothèque ComicRack… » : ferme les réglages et ouvre la fenêtre du transfert. */
    ontransfert?: () => void;
    /** « Rechercher une mise à jour… » : ferme les réglages et ouvre la fenêtre « Mise à jour ». */
    onmaj?: () => void;
  }
  let { themeActif, onTheme, mode = 'autonome', onecriture, ontransfert, onmaj }: Props = $props();
  let ecritureComicrack = $state(false);
  let confirmerEcriture = $state(false);
  let enregistre = $state('');

  let dialogue = $state<HTMLDialogElement>();
  let themes = $state<Theme[]>([]);
  let info = $state<InfosApplication | null>(null);
  let erreur = $state('');

  export async function ouvrir() {
    dialogue?.showModal();
    await actualiser();
  }

  async function actualiser() {
    try {
      [themes, info] = await Promise.all([listerThemes(), infos()]);
      if (mode === 'comicrack') ecritureComicrack = (await lireReglagesEdition()).ecritureComicrack;
    } catch (e) {
      erreur = messageErreur(e);
    }
  }

  async function choisir(id: string) {
    try {
      onTheme(await appliquerTheme(id));
    } catch (e) {
      erreur = messageErreur(e);
    }
  }

  /** Cocher demande d'abord de lire l'avertissement ; décocher (retour à la lecture seule) est immédiat. */
  function basculerEcriture(e: Event & { currentTarget: HTMLInputElement }) {
    if (e.currentTarget.checked && !ecritureComicrack) {
      e.currentTarget.checked = false;
      confirmerEcriture = true;
      return;
    }
    void changerEcriture(e.currentTarget.checked);
  }

  async function changerEcriture(actif: boolean) {
    confirmerEcriture = false;
    try {
      ecritureComicrack = (await enregistrerReglagesEdition({ ecritureComicrack: actif })).ecritureComicrack;
      enregistre = ecritureComicrack ? 'Lecture et écriture activée.' : 'Lecture seule rétablie.';
      onecriture?.();
    } catch (err) {
      erreur = messageErreur(err);
    }
  }
</script>

<dialog bind:this={dialogue} use:modale aria-labelledby="titre-reglages">
  <h2 id="titre-reglages">Réglages</h2>
  <div class="ligne">
    <label for="theme">Apparence</label>
    <select id="theme" value={themeActif} onchange={(e) => choisir(e.currentTarget.value)}>
      {#each themes as t (t.id)}
        <option value={t.id}>{t.nom}{t.livre ? '' : ' (perso)'}</option>
      {/each}
    </select>
    <button onclick={actualiser}>Actualiser la liste</button>
  </div>
  {#if mode === 'comicrack'}
    <fieldset class="comicrack" data-testid="reglage-ecriture">
      <legend>Bibliothèque ComicRack</legend>
      <label class="case">
        <input type="checkbox" checked={ecritureComicrack} onchange={basculerEcriture} data-testid="ecriture-comicrack" />
        Lecture et écriture (lecture, fichiers convertis, infos écrites sont enregistrés dans la base de ComicRack ;
        listes modifiables, ComicRack fermé)
      </label>
      <p class="discret petit">
        {ecritureComicrack
          ? 'ComicRack lancé voit les modifications en quelques secondes. ComicRust ne réécrit que les éléments concernés de la fiche (page lue, date d\'ouverture, fichier…), jamais le reste. Les listes (dans ComicDb.xml) ne sont modifiables que ComicRack fermé.'
          : 'Lecture seule : la lecture faite dans ComicRust est gardée dans ses réglages ; ni la base ni les listes de ComicRack ne sont modifiées.'}
        Sans effet pour une bibliothèque ComicDb.xml (fiches toujours en lecture seule).
      </p>
      {#if confirmerEcriture}
        <div class="avertissement" role="alertdialog" aria-label="Avertissement" data-testid="avertissement-ecriture">
          <p><strong>⚠ ComicRust va écrire dans la base de ComicRack.</strong></p>
          <p>Faites d'abord une sauvegarde de la base (MySQL). Une fiche modifiée entre-temps dans ComicRack n'est jamais écrasée
            (conflit signalé), mais évitez de modifier les mêmes livres dans les deux programmes en même temps.</p>
          <div class="boutons">
            <button onclick={() => (confirmerEcriture = false)}>Annuler</button>
            <button class="principal" onclick={() => void changerEcriture(true)} data-testid="confirmer-ecriture">Activer l'écriture</button>
          </div>
        </div>
      {/if}
      {#if enregistre}<p class="ok" role="status">{enregistre}</p>{/if}
    </fieldset>
  {/if}
  {#if ontransfert}
    <fieldset class="comicrack" data-testid="reglage-transfert">
      <legend>Passer de ComicRack à ComicRust</legend>
      <p class="discret petit transfert">Copie les livres, la lecture, les listes et les dossiers de ComicRack dans la base de ComicRust
        (ComicRack n'est pas modifié ; le transfert peut être relancé sans doublon).</p>
      <button onclick={() => { fermerModale(dialogue); ontransfert(); }} data-testid="ouvrir-transfert">Transférer la bibliothèque ComicRack…</button>
    </fieldset>
  {/if}
  {#if onmaj}
    <fieldset class="comicrack" data-testid="reglage-maj">
      <legend>Mise à jour</legend>
      <p class="discret petit transfert">Cherche une nouvelle version de ComicRust sur GitHub ; le paquet est vérifié avant d'être installé.</p>
      <button onclick={() => { fermerModale(dialogue); onmaj(); }} data-testid="ouvrir-maj">Rechercher une mise à jour…</button>
    </fieldset>
  {/if}
  {#if info}
    <p class="discret">
      Ajoutez un fichier <code>.css</code> dans <code>{info.dossier_themes}</code>, puis « Actualiser la liste » :
      aucun redémarrage ni recompilation.
    </p>
    <dl>
      <dt>Version</dt><dd>{info.version}</dd>
      <dt>Données</dt><dd><code>{info.dossier_data}</code></dd>
      <dt>Profil WebView2</dt><dd><code>{info.dossier_webview}</code></dd>
    </dl>
  {/if}
  {#if erreur}<p class="erreur" role="alert">{erreur}</p>{/if}
  <form method="dialog" class="pied"><button>Fermer</button></form>
</dialog>

<style>
  .ligne { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .discret { color: var(--cr-texte-discret); }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 4px 12px; margin: 0 0 12px; }
  dt { color: var(--cr-texte-discret); }
  dd { margin: 0; word-break: break-all; }
  code { font-family: ui-monospace, Consolas, monospace; background: var(--cr-fond-survol); padding: 1px 5px; border-radius: 3px; }
  .erreur { color: var(--cr-erreur); }
  .comicrack { border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); margin: 12px 0; padding: 8px 12px; }
  .comicrack legend { padding: 0 4px; font-weight: 600; }
  .case { display: flex; gap: 8px; align-items: flex-start; }
  .case input { margin-top: 3px; accent-color: var(--cr-accent); }
  .petit { font-size: 12px; margin: 6px 0 0 24px; }
  .petit.transfert { margin: 0 0 8px; }
  .avertissement { margin: 8px 0 0; padding: 8px 10px; border: 1px solid var(--cr-erreur); border-radius: var(--cr-rayon); }
  .avertissement p { margin: 0 0 6px; }
  .avertissement strong { color: var(--cr-erreur); }
  .boutons { display: flex; justify-content: flex-end; gap: 8px; }
  .ok { color: var(--cr-accent); margin: 6px 0 0; }
  button.principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); }
  .pied { display: flex; justify-content: flex-end; margin: 0; }
</style>
