<script lang="ts">
  // Écran de premier démarrage : bibliothèque autonome (base SQLite ou MySQL) ou liée à une
  // installation ComicRack (ComicRack.ini, ou paramètres MySQL saisis à la main).
  import {
    configurerMysql, configurerPremierDemarrage, type EtatBiblio, type ModeBiblio, type TypeStore, messageErreur,
  } from '../lib/api';
  import FormulaireMysql, { mysqlVide, versParametres } from './FormulaireMysql.svelte';

  interface Props {
    onpret: (etat: EtatBiblio) => void;
  }
  let { onpret }: Props = $props();

  let mode = $state<ModeBiblio>('autonome');
  let cheminIni = $state('');
  /** Mode autonome : où vit la base de ComicRust. */
  let store = $state<TypeStore>('sqlite');
  /** Mode lié : `ComicRack.ini` ou paramètres MySQL saisis. */
  let sourceCr = $state<'ini' | 'mysql'>('ini');
  let mysqlStore = $state(mysqlVide());
  let mysqlCr = $state(mysqlVide());
  let occupe = $state(false);
  let erreur = $state('');

  const valide = $derived(
    mode === 'autonome' ? store === 'sqlite' || mysqlStore.serveur.trim().length > 0 : sourceCr === 'mysql' || cheminIni.trim().length > 0,
  );

  async function valider() {
    if (!valide || occupe) return;
    occupe = true;
    erreur = '';
    try {
      const mysql = mode === 'autonome' ? (store === 'mysql' ? mysqlStore : null) : sourceCr === 'mysql' ? mysqlCr : null;
      if (mysql) {
        const p = versParametres(mysql);
        if (typeof p === 'string') throw new Error(p);
        onpret(await configurerMysql({ mode, store: mode === 'autonome' ? 'mysql' : undefined, mysql: p }));
      } else {
        onpret(await configurerPremierDemarrage({ mode, cheminIni: mode === 'comicrack' ? cheminIni.trim() : undefined }));
      }
    } catch (e) {
      erreur = messageErreur(e);
    } finally {
      occupe = false;
    }
  }
</script>

<main class="ecran">
  <form class="carte" onsubmit={(e) => { e.preventDefault(); void valider(); }} data-testid="premier-demarrage">
    <h1>Bienvenue dans ComicRust</h1>
    <p class="discret">Choisissez comment ComicRust doit gérer votre bibliothèque. Ce choix est modifiable plus tard.</p>

    <fieldset>
      <legend class="sr">Mode de bibliothèque</legend>
      <label class="choix" class:actif={mode === 'autonome'}>
        <input type="radio" name="mode" value="autonome" bind:group={mode} />
        <span>
          <strong>Bibliothèque autonome</strong>
          <small>ComicRust crée et gère sa propre base (dossier <code>data</code> portable). Idéal pour repartir de zéro.</small>
        </span>
      </label>
      <label class="choix" class:actif={mode === 'comicrack'}>
        <input type="radio" name="mode" value="comicrack" bind:group={mode} />
        <span>
          <strong>Lié à ComicRack</strong>
          <small>Utilise la bibliothèque d'une installation ComicRack existante (listes, lectures et métadonnées conservées).</small>
        </span>
      </label>
    </fieldset>

    {#if mode === 'autonome'}
      <fieldset data-testid="choix-store">
        <legend>Base de ComicRust</legend>
        <label class="option"><input type="radio" name="store" value="sqlite" bind:group={store} /> Fichier SQLite local (<code>data\comicrust.db</code>)</label>
        <label class="option"><input type="radio" name="store" value="mysql" bind:group={store} /> Serveur MySQL / MariaDB (base existante, les tables sont créées)</label>
      </fieldset>
      {#if store === 'mysql'}
        <FormulaireMysql id="mysql-store" cible="autonome" bind:valeurs={mysqlStore} />
      {/if}
    {:else}
      <fieldset data-testid="choix-source-cr">
        <legend>Source ComicRack</legend>
        <label class="option"><input type="radio" name="sourcecr" value="ini" bind:group={sourceCr} /> Fichier <code>ComicRack.ini</code> (ou <code>ComicDb.xml</code>)</label>
        <label class="option"><input type="radio" name="sourcecr" value="mysql" bind:group={sourceCr} /> Saisir les paramètres MySQL de ComicRack</label>
      </fieldset>
      {#if sourceCr === 'ini'}
        <label class="champ">
          <span>Fichier <code>ComicRack.ini</code> de l'installation</span>
          <input type="text" bind:value={cheminIni} placeholder="C:\ComicRack\ComicRack.ini" autocomplete="off" spellcheck="false" />
        </label>
      {:else}
        <FormulaireMysql id="mysql-cr" cible="comicrack" bind:valeurs={mysqlCr} />
      {/if}
    {/if}

    {#if erreur}<p class="erreur" role="alert">{erreur}</p>{/if}
    <button class="principal" type="submit" disabled={!valide || occupe}>{occupe ? 'Ouverture…' : 'Continuer'}</button>
  </form>
</main>

<style>
  .ecran { min-height: 100%; display: grid; place-items: center; padding: 24px 16px; }
  .carte {
    width: min(560px, 100%); display: grid; gap: 14px; padding: 24px; background: var(--cr-fond-panneau);
    border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); box-shadow: var(--cr-ombre);
  }
  h1 { margin: 0; font-size: 22px; }
  .discret { margin: 0; color: var(--cr-texte-discret); }
  fieldset { border: 0; padding: 0; margin: 0; display: grid; gap: 10px; }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
  .choix {
    display: flex; gap: 12px; align-items: flex-start; padding: 12px; cursor: pointer;
    border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon);
  }
  .choix:hover { background: var(--cr-fond-survol); }
  .choix.actif { border-color: var(--cr-accent); box-shadow: inset 0 0 0 1px var(--cr-accent); }
  .choix input { margin-top: 3px; accent-color: var(--cr-accent); }
  .choix span { display: grid; gap: 2px; }
  small { color: var(--cr-texte-discret); font-size: 13px; }
  .champ { display: grid; gap: 6px; }
  legend { font-weight: 600; margin-bottom: 4px; padding: 0; }
  .option { display: flex; gap: 8px; align-items: center; cursor: pointer; }
  .option input { accent-color: var(--cr-accent); }
  input[type='text'] {
    font: inherit; color: var(--cr-texte); background: var(--cr-fond); border: 1px solid var(--cr-bordure);
    border-radius: var(--cr-rayon); padding: 7px 10px;
  }
  input:focus-visible { outline: 2px solid var(--cr-accent); outline-offset: 1px; }
  .erreur { color: var(--cr-erreur); margin: 0; }
  .principal {
    justify-self: end; font: inherit; cursor: pointer; padding: 7px 26px; background: var(--cr-accent);
    color: var(--cr-accent-texte); border: 1px solid var(--cr-accent); border-radius: var(--cr-rayon);
  }
  .principal:disabled { opacity: 0.5; cursor: default; }
  .principal:focus-visible { outline: 2px solid var(--cr-accent); outline-offset: 2px; }
  code { font-family: ui-monospace, Consolas, monospace; background: var(--cr-fond-survol); padding: 1px 5px; border-radius: 3px; }
</style>
