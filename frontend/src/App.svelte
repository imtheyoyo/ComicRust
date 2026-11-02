<script lang="ts">
  import { onMount } from 'svelte';
  import { dansTauri, infos, listerThemes, type InfosApplication, type Theme } from './lib/api';
  import { appliquerTheme, themeMemorise } from './lib/theme';

  let info = $state<InfosApplication | null>(null);
  let themes = $state<Theme[]>([]);
  let themeActif = $state('');
  let erreur = $state('');
  const tauri = dansTauri();

  async function choisir(id: string) {
    try {
      themeActif = await appliquerTheme(id);
    } catch (e) {
      erreur = String(e);
    }
  }

  async function actualiser() {
    try {
      themes = await listerThemes();
    } catch (e) {
      erreur = String(e);
    }
  }

  onMount(async () => {
    if (!tauri) return;
    try {
      info = await infos();
      themes = await listerThemes();
      await choisir(themeMemorise());
    } catch (e) {
      erreur = String(e);
    }
  });
</script>

<main>
  <header>
    <h1>ComicRust</h1>
    <span class="version">{info ? `v${info.version} (Tauri)` : 'Tauri'}</span>
  </header>

  {#if !tauri}
    <p class="carte">
      Cette page doit s'ouvrir dans la fenêtre ComicRust (Tauri) : lancez
      <code>cargo tauri dev</code> ou <code>ComicRust-Tauri.exe</code>.
    </p>
  {:else}
    <section class="carte">
      <h2>Bienvenue</h2>
      <p>
        Squelette de la version Tauri 2 : la fenêtre, les commandes IPC et les thèmes sont en
        place ; les fonctions de ComicRust y seront portées étape par étape (voir
        <code>docs/tauri/PLAN.md</code>).
      </p>
    </section>

    <section class="carte">
      <h2>Thème</h2>
      <div class="ligne">
        <label for="theme">Apparence</label>
        <select id="theme" value={themeActif} onchange={(e) => choisir(e.currentTarget.value)}>
          {#each themes as t (t.id)}
            <option value={t.id}>{t.nom}{t.livre ? '' : ' (perso)'}</option>
          {/each}
        </select>
        <button onclick={actualiser}>Actualiser la liste</button>
      </div>
      {#if info}
        <p class="discret">
          Ajoutez un fichier <code>.css</code> dans <code>{info.dossier_themes}</code>, puis
          « Actualiser la liste » : aucun redémarrage ni recompilation.
        </p>
      {/if}
    </section>

    {#if info}
      <section class="carte">
        <h2>Mode portable</h2>
        <dl>
          <dt>Données</dt><dd><code>{info.dossier_data}</code></dd>
          <dt>Profil WebView2</dt><dd><code>{info.dossier_webview}</code></dd>
        </dl>
      </section>
    {/if}
  {/if}

  {#if erreur}
    <p class="carte erreur" role="alert">{erreur}</p>
  {/if}
</main>

<style>
  main { max-width: 760px; margin: 0 auto; padding: 24px 16px; display: grid; gap: 16px; }
  header { display: flex; align-items: baseline; gap: 12px; }
  h1 { margin: 0; font-size: 26px; }
  h2 { margin: 0 0 8px; font-size: 16px; }
  .version, .discret { color: var(--cr-texte-discret); }
  .carte {
    background: var(--cr-fond-panneau);
    border: 1px solid var(--cr-bordure);
    border-radius: var(--cr-rayon);
    box-shadow: var(--cr-ombre);
    padding: 16px;
    margin: 0;
  }
  .erreur { color: var(--cr-erreur); border-color: var(--cr-erreur); }
  .ligne { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  select, button {
    font: inherit;
    color: var(--cr-texte);
    background: var(--cr-fond-panneau);
    border: 1px solid var(--cr-bordure);
    border-radius: var(--cr-rayon);
    padding: 5px 10px;
  }
  button { cursor: pointer; }
  button:hover { background: var(--cr-fond-survol); }
  button:focus-visible, select:focus-visible { outline: 2px solid var(--cr-accent); outline-offset: 1px; }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 4px 12px; margin: 0; }
  dt { color: var(--cr-texte-discret); }
  dd { margin: 0; word-break: break-all; }
  code { font-family: ui-monospace, Consolas, monospace; background: var(--cr-fond-survol); padding: 1px 5px; border-radius: 3px; }
</style>
