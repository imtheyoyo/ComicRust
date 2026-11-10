<script lang="ts">
  import { onMount } from 'svelte';
  import BarreLaterale, { type Section } from './components/BarreLaterale.svelte';
  import DialogDossiers from './components/DialogDossiers.svelte';
  import DialogReglages from './components/DialogReglages.svelte';
  import PremierDemarrage from './components/PremierDemarrage.svelte';
  import VueBibliotheque from './components/VueBibliotheque.svelte';
  import { dansTauri, ouvrirBibliotheque, type EtatBiblio, messageErreur } from './lib/api';
  import { SuiviScan } from './lib/bibliotheque.svelte';
  import { EtatListes } from './lib/listes.svelte';
  import { chargerPreferences } from './lib/prefs';
  import { appliquerTheme, themeMemorise } from './lib/theme';

  let etat = $state<EtatBiblio | null>(null);
  let erreur = $state('');
  let section = $state<Section>('bibliotheque');
  let themeActif = $state('');
  let nbLivres = $state(0);
  let rafraichir = $state(0);
  const scan = new SuiviScan();
  /** Listes ComicRack (étape 5) : arbre partagé par le panneau et la vue. */
  const listes = new EtatListes();
  let dossiers = $state<DialogDossiers>();
  let reglages = $state<DialogReglages>();

  onMount(() => {
    void (async () => {
      try {
        // Préférences d'affichage d'abord : le thème et le premier lister_livres en dépendent.
        await chargerPreferences();
        themeActif = await appliquerTheme(themeMemorise());
        await scan.demarrer(() => rafraichir++);
        etat = await ouvrirBibliotheque();
        nbLivres = etat.nbLivres;
        if (!etat.premierDemarrage) {
          void listes.charger();
          listes.demarrer();
        }
      } catch (e) {
        erreur = messageErreur(e);
      }
    })();
    // État lisible par le pilotage MCP des tests (get_state, variante -mcp) : lecture seule.
    (window as unknown as { __comicrust_etat?: () => unknown }).__comicrust_etat = () => ({
      section,
      mode: etat?.mode ?? null,
      premierDemarrage: etat?.premierDemarrage ?? null,
      nbLivres,
      listeCourante: listes.courante,
      theme: themeActif,
      erreur: erreur || null,
    });
    // Le backend choisit parfois la liste à afficher (liste créée, liste affichée supprimée).
    listes.surSelection = choisirListe;
    return () => {
      scan.arreter();
      listes.arreter();
    };
  });

  function pret(e: EtatBiblio) {
    etat = e;
    nbLivres = e.nbLivres;
    void listes.charger();
    listes.demarrer();
  }

  function choisirSection(s: Section) {
    section = s;
    if (s !== 'listes') listes.courante = null;
  }

  function choisirListe(id: string | null) {
    listes.courante = id || null;
    section = id ? 'listes' : 'bibliotheque';
  }

  const listeAffichee = $derived(section === 'listes' ? listes.noeud(listes.courante) ?? null : null);
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
      onsection={choisirSection}
      ondossiers={() => dossiers?.ouvrir()}
      onreglages={() => reglages?.ouvrir()}
      {listes}
      onliste={choisirListe}
    />
    {#if section === 'bibliotheque' || listeAffichee}
      <VueBibliotheque {scan} bind:nbLivres {rafraichir} {listes} liste={listeAffichee} />
    {:else if section === 'listes'}
      <main class="bientot">
        <h1>Listes</h1>
        <p>Choisis une liste dans le panneau de gauche.</p>
      </main>
    {:else}
      <main class="bientot">
        <h1>Séries</h1>
        <p>La navigation par séries arrivera dans une prochaine étape.</p>
      </main>
    {/if}
  </div>
  <DialogDossiers bind:this={dossiers} {scan} />
  <DialogReglages bind:this={reglages} {themeActif} mode={etat.mode} onTheme={(id) => (themeActif = id)} onecriture={() => void listes.charger()} />
{/if}

<style>
  .appli { height: 100vh; display: flex; }
  .message { padding: 40px; text-align: center; color: var(--cr-texte-discret); }
  .erreur { color: var(--cr-erreur); }
  .bientot { flex: 1; display: grid; place-content: center; text-align: center; color: var(--cr-texte-discret); }
  .bientot h1 { color: var(--cr-texte); margin: 0 0 8px; }
</style>
