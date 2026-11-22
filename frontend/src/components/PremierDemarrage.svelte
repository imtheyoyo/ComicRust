<script lang="ts">
  // Écran de premier démarrage : bibliothèque autonome (base SQLite ou MySQL) ou liée à une
  // installation ComicRack (ComicRack.ini, ou paramètres MySQL saisis à la main).
  // Aussi affiché (tauri-v0.9.1) quand la base de la copie autonome est introuvable (`baseAbsente` :
  // dossier data copié sans comicrust.db) ou quand la bibliothèque ne s'ouvre pas (`probleme`) :
  // l'utilisateur crée une nouvelle base ou en choisit une autre ; rien n'est créé sans son clic.
  // Mode « modification » (lot L7, écart 7) : rejoué depuis Réglages ▸ « Changer de bibliothèque / de
  // connexion… » (comme « Assistant de configuration… » de la version egui). Le formulaire est prérempli
  // avec la bibliothèque actuelle, l'annulation ne change rien, un choix qui échoue n'est pas enregistré
  // et la base actuelle n'est jamais supprimée (confirmation avant de remplacer la bibliothèque affichée).
  import { onMount } from 'svelte';
  import {
    configurationBiblio, configurerMysql, configurerPremierDemarrage, lireConnexionsMysql, type ConfigurationBiblio,
    type EtatBiblio, type ModeBiblio, type TypeStore, messageErreur,
  } from '../lib/api';
  import FormulaireMysql, { mysqlVide, versParametres } from './FormulaireMysql.svelte';

  interface Props {
    onpret: (etat: EtatBiblio) => void;
    /** Chemin de la base SQLite attendue mais absente (copie autonome sans data\comicrust.db). */
    baseAbsente?: string;
    /** Erreur d'ouverture de la bibliothèque enregistrée (l'utilisateur en choisit une autre). */
    probleme?: string;
    /** Retour à l'écran précédent (erreur d'ouverture : « Réessayer »). */
    onretour?: () => void;
    /** Mode « modification » : l'assistant est rouvert sur une bibliothèque déjà configurée. */
    modification?: boolean;
    /** Mode « modification » : abandon (bouton Annuler, Échap) ; rien n'est changé. */
    onannuler?: () => void;
  }
  let { onpret, baseAbsente, probleme, onretour, modification = false, onannuler }: Props = $props();

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
  /** Mode modification : bibliothèque actuelle, confirmation demandée, mots de passe déjà enregistrés. */
  let actuelle = $state<ConfigurationBiblio | null>(null);
  let aConfirmer = $state(false);
  let mdpStore = $state(false);
  let mdpCr = $state(false);

  onMount(() => {
    if (!modification) return;
    void (async () => {
      try {
        const [c, k] = await Promise.all([configurationBiblio(), lireConnexionsMysql()]);
        actuelle = c;
        if (c.mode) mode = c.mode;
        if (c.mode === 'comicrack') {
          if (c.cheminIni) cheminIni = c.cheminIni;
          if (!c.cheminIni && k.comicrackMysql) sourceCr = 'mysql';
        } else store = k.store;
        const pre = (p: { serveur: string; port: number; utilisateur: string; base: string }) =>
          ({ ...mysqlVide(), serveur: p.serveur, port: String(p.port), utilisateur: p.utilisateur, base: p.base });
        if (k.storeMysql) { mysqlStore = pre(k.storeMysql); mdpStore = k.storeMysql.motDePasseDefini; }
        if (k.comicrackMysql) { mysqlCr = pre(k.comicrackMysql); mdpCr = k.comicrackMysql.motDePasseDefini; }
      } catch (e) {
        erreur = messageErreur(e);
      }
    })();
  });

  /** Échap abandonne la modification (jamais pendant l'ouverture). */
  function clavier(e: KeyboardEvent) {
    if (modification && e.key === 'Escape' && !occupe) {
      e.preventDefault();
      if (aConfirmer) aConfirmer = false;
      else onannuler?.();
    }
  }

  /** Base SQLite de ComicRust qui serait créée vide par le choix (aucune base existante). */
  const creeraBaseVide = $derived(modification && mode === 'autonome' && store === 'sqlite' && actuelle !== null && !actuelle.baseSqlitePresente);

  const valide = $derived(
    mode === 'autonome' ? store === 'sqlite' || mysqlStore.serveur.trim().length > 0 : sourceCr === 'mysql' || cheminIni.trim().length > 0,
  );

  /** Valide : en mode modification, une confirmation précède le remplacement de la bibliothèque. */
  function demander() {
    if (!valide || occupe) return;
    if (modification && !aConfirmer) {
      aConfirmer = true;
      return;
    }
    void valider();
  }

  async function valider() {
    if (!valide || occupe) return;
    aConfirmer = false;
    occupe = true;
    erreur = '';
    try {
      const mysql = mode === 'autonome' ? (store === 'mysql' ? mysqlStore : null) : sourceCr === 'mysql' ? mysqlCr : null;
      if (mysql) {
        const p = versParametres(mysql, mode === 'autonome' ? mdpStore : mdpCr);
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

<svelte:window onkeydown={clavier} />
<main class="ecran">
  <form class="carte" onsubmit={(e) => { e.preventDefault(); demander(); }} data-testid="premier-demarrage">
    {#if modification}
      <h1>Changer de bibliothèque ou de connexion</h1>
      <div class="alerte" role="status" data-testid="modification-info">
        <p>Bibliothèque actuelle :
          <strong>{#if !actuelle}…{:else if actuelle.mode === 'comicrack'}liée à ComicRack{#if actuelle.cheminIni} (<code>{actuelle.cheminIni}</code>){:else if actuelle.comicrackMysql} (connexion MySQL saisie){/if}{:else if actuelle.storeMysql}autonome, base MySQL{:else}autonome, base SQLite{/if}</strong>.</p>
        <p class="discret">Rien n'est supprimé : la base de ComicRust (<code>{actuelle?.baseSqlite ?? 'data\\comicrust.db'}</code>) et vos réglages restent en place, vous pourrez y revenir.
          Un choix qui échoue n'est pas enregistré ; « Annuler » ne change rien.</p>
      </div>
    {:else if baseAbsente}
      <h1>Base ComicRust introuvable</h1>
      <div class="alerte" role="status" data-testid="base-absente">
        <p>La base de cette copie autonome n'existe pas :</p>
        <p><code>{baseAbsente}</code></p>
        <p class="discret">Le dossier <code>data</code> a peut-être été copié sans elle, ou elle a été déplacée.
          Remettez le fichier <code>comicrust.db</code> à sa place puis relancez ComicRust, ou bien
          <strong>créez une nouvelle base vide</strong> (vos réglages sont gardés), ou choisissez une autre bibliothèque ci-dessous.</p>
      </div>
    {:else if probleme}
      <h1>Choisir une bibliothèque</h1>
      <div class="alerte" role="status" data-testid="probleme-ouverture">
        <p>La bibliothèque enregistrée ne s'ouvre pas :</p>
        <p class="detail">{probleme}</p>
        <p class="discret">Choisissez la bibliothèque à utiliser. Rien n'est supprimé ni écrasé : une base SQLite existante est
          ouverte telle quelle, une nouvelle n'est créée que si elle n'existe pas.</p>
      </div>
    {:else}
      <h1>Bienvenue dans ComicRust</h1>
      <p class="discret">Choisissez comment ComicRust doit gérer votre bibliothèque. Ce choix est modifiable plus tard.</p>
    {/if}

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
        <FormulaireMysql id="mysql-store" cible="autonome" bind:valeurs={mysqlStore} memorise={mdpStore} />
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
        <FormulaireMysql id="mysql-cr" cible="comicrack" bind:valeurs={mysqlCr} memorise={mdpCr} />
      {/if}
    {/if}

    {#if erreur}<p class="erreur" role="alert">{erreur}</p>{/if}
    <div class="actions">
      {#if onretour}<button type="button" onclick={onretour} disabled={occupe}>Réessayer</button>{/if}
      {#if modification}<button type="button" onclick={() => onannuler?.()} disabled={occupe} data-testid="premier-annuler">Annuler</button>{/if}
      <button class="principal" type="submit" disabled={!valide || occupe} data-testid="premier-valider">
        {occupe ? 'Ouverture…' : modification ? 'Changer de bibliothèque' : baseAbsente && mode === 'autonome' && store === 'sqlite' ? 'Créer une nouvelle base' : 'Continuer'}
      </button>
    </div>
    {#if aConfirmer}
      <div class="alerte" role="alertdialog" aria-label="Confirmer le changement" data-testid="confirmer-changement">
        <p><strong>Remplacer la bibliothèque affichée ?</strong></p>
        <p class="discret">ComicRust va ouvrir {mode === 'comicrack' ? 'la bibliothèque ComicRack choisie' : store === 'mysql' ? 'la base MySQL choisie' : 'la base SQLite locale'}.
          Les listes et l'analyse repartent de cette bibliothèque. La bibliothèque actuelle n'est pas supprimée.
          {#if creeraBaseVide}Aucune base SQLite n'existe encore : une base vide sera créée.{/if}</p>
        <div class="actions">
          <button type="button" onclick={() => (aConfirmer = false)} data-testid="confirmer-non">Revenir</button>
          <button type="button" class="principal" onclick={() => void valider()} data-testid="confirmer-oui">Confirmer</button>
        </div>
      </div>
    {/if}
  </form>
</main>

<style>
  .ecran { min-height: 100%; display: grid; place-items: center; padding: 24px 16px; }
  .carte {
    width: min(560px, 100%); display: grid; gap: 14px; padding: 24px; background: var(--cr-fond-panneau);
    border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); box-shadow: var(--cr-ombre);
  }
  h1 { margin: 0; font-size: 22px; }
  .alerte { display: grid; gap: 6px; padding: 10px 12px; border: 1px solid var(--cr-accent); border-radius: var(--cr-rayon); background: var(--cr-fond); }
  .alerte p { margin: 0; overflow-wrap: anywhere; }
  .alerte .detail { white-space: pre-wrap; color: var(--cr-erreur); }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
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
