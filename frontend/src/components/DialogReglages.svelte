<script lang="ts">
  // Réglages d'affichage : choix du thème (fichiers CSS de data\themes) et dossiers du mode portable.
  import { infos, listerThemes, type InfosApplication, type Theme, messageErreur } from '../lib/api';
  import { appliquerTheme } from '../lib/theme';

  interface Props {
    themeActif: string;
    onTheme: (id: string) => void;
  }
  let { themeActif, onTheme }: Props = $props();

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
</script>

<dialog bind:this={dialogue} aria-labelledby="titre-reglages">
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
  .pied { display: flex; justify-content: flex-end; margin: 0; }
</style>
