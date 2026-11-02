<script lang="ts">
  import { onMount } from 'svelte';
  import BarreLaterale, { type Section } from './components/BarreLaterale.svelte';
  import DialogDossiers from './components/DialogDossiers.svelte';
  import DialogReglages from './components/DialogReglages.svelte';
  import PremierDemarrage from './components/PremierDemarrage.svelte';
  import VueBibliotheque from './components/VueBibliotheque.svelte';
  import { dansTauri, ouvrirBibliotheque, type EtatBiblio } from './lib/api';
  import { SuiviScan } from './lib/bibliotheque.svelte';
  import { appliquerTheme, themeMemorise } from './lib/theme';

  let etat = $state<EtatBiblio | null>(null);
  let erreur = $state('');
  let section = $state<Section>('bibliotheque');
  let themeActif = $state('');
  let nbLivres = $state(0);
  let rafraichir = $state(0);
  const scan = new SuiviScan();
  let dossiers = $state<DialogDossiers>();
  let reglages = $state<DialogReglages>();

  onMount(() => {
    void (async () => {
      try {
        themeActif = await appliquerTheme(themeMemorise());
        await scan.demarrer(() => rafraichir++);
        etat = await ouvrirBibliotheque();
        nbLivres = etat.nbLivres;
      } catch (e) {
        erreur = String(e);
      }
    })();
    return () => scan.arreter();
  });

  function pret(e: EtatBiblio) {
    etat = e;
    nbLivres = e.nbLivres;
  }
</script>

{#if erreur}
  <p class="message erreur" role="alert">{erreur}</p>
{:else if !etat}
  <p class="message">Ouverture de la bibliothèque…{dansTauri() ? '' : ' (mode simulé)'}</p>
{:else if etat.premierDemarrage}
  <PremierDemarrage onpret={pret} />
{:else}
  <div class="appli">
    <BarreLaterale
      {section}
      {nbLivres}
      mode={etat.mode}
      onsection={(s) => (section = s)}
      ondossiers={() => dossiers?.ouvrir()}
      onreglages={() => reglages?.ouvrir()}
    />
    {#if section === 'bibliotheque'}
      <VueBibliotheque {scan} bind:nbLivres {rafraichir} />
    {:else}
      <main class="bientot">
        <h1>{section === 'series' ? 'Séries' : 'Listes'}</h1>
        <p>
          {section === 'series'
            ? 'La navigation par séries arrivera dans une prochaine étape.'
            : 'Les listes ComicRack (intelligentes et manuelles) arriveront à l\'étape 5.'}
        </p>
      </main>
    {/if}
  </div>
  <DialogDossiers bind:this={dossiers} {scan} />
  <DialogReglages bind:this={reglages} {themeActif} onTheme={(id) => (themeActif = id)} />
{/if}

<style>
  .appli { height: 100vh; display: flex; }
  .message { padding: 40px; text-align: center; color: var(--cr-texte-discret); }
  .erreur { color: var(--cr-erreur); }
  .bientot { flex: 1; display: grid; place-content: center; text-align: center; color: var(--cr-texte-discret); }
  .bientot h1 { color: var(--cr-texte); margin: 0 0 8px; }
</style>
