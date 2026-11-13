<script lang="ts">
  import { onMount } from 'svelte';
  import BarreLaterale, { type Section } from './components/BarreLaterale.svelte';
  import BandeauBase from './components/BandeauBase.svelte';
  import BandeauMaj from './components/BandeauMaj.svelte';
  import DialogMaj from './components/DialogMaj.svelte';
  import DialogDossiers from './components/DialogDossiers.svelte';
  import DialogReglages from './components/DialogReglages.svelte';
  import DialogTransfert from './components/DialogTransfert.svelte';
  import DialogScripts from './components/DialogScripts.svelte';
  import FenetreScript from './components/FenetreScript.svelte';
  import PremierDemarrage from './components/PremierDemarrage.svelte';
  import VueBibliotheque from './components/VueBibliotheque.svelte';
  import { dansTauri, ecouter, ouvrirBibliotheque, type BibliothequeModifiee, type EtatBiblio, messageErreur } from './lib/api';
  import { SuiviScan } from './lib/bibliotheque.svelte';
  import { connexion } from './lib/connexion.svelte';
  import { EtatListes } from './lib/listes.svelte';
  import { miseAJour } from './lib/maj.svelte';
  import { scripts } from './lib/scripts.svelte';
  import { chargerPreferences, enregistrerPreference } from './lib/prefs';
  import { appliquerTheme, themeMemorise } from './lib/theme';

  let etat = $state<EtatBiblio | null>(null);
  let erreur = $state('');
  let section = $state<Section>('bibliotheque');
  let themeActif = $state('');
  let nbLivres = $state(0);
  let rafraichir = $state(0);
  // Colonne des listes (barre latérale) : largeur réglable, mémorisée (section vue des préférences).
  const LISTES_DEFAUT = 250;
  const LISTES_MIN = 180;
  const LISTES_MAX = 500;
  let largeurListes = $state(LISTES_DEFAUT);
  const scan = new SuiviScan();
  /** Listes ComicRack (étape 5) : arbre partagé par le panneau et la vue. */
  const listes = new EtatListes();
  let dossiers = $state<DialogDossiers>();
  let reglages = $state<DialogReglages>();
  let transfert = $state<DialogTransfert>();

  onMount(() => {
    void (async () => {
      try {
        // Préférences d'affichage d'abord : le thème et le premier lister_livres en dépendent.
        const prefs = await chargerPreferences();
        if (typeof prefs.largeurListes === 'number') largeurListes = borneListes(prefs.largeurListes);
        themeActif = await appliquerTheme(themeMemorise());
        await scan.demarrer(() => rafraichir++);
        await connexion.demarrer();
        // Mise à jour : jamais bloquante (vérification automatique en arrière-plan, bandeau seul).
        void miseAJour.demarrer();
        etat = await ouvrirBibliotheque();
        nbLivres = etat.nbLivres;
        if (!etat.premierDemarrage) {
          void listes.charger();
          listes.demarrer();
          // Scripts (tauri-v0.9.0) : liste et écoute des événements, jamais bloquant.
          void scripts.demarrer();
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
      baseEnLigne: connexion.enLigne,
      erreur: erreur || null,
    });
    // Le backend choisit parfois la liste à afficher (liste créée, liste affichée supprimée).
    listes.surSelection = choisirListe;
    // Modification faite hors de ComicRust (ComicRack, autre poste) : les comptes des listes changent.
    let desabonnerBase: (() => void) | undefined;
    void ecouter<BibliothequeModifiee>('bibliotheque-modifiee', (m) => {
      if (m.origine === 'externe') void listes.charger();
    }).then((f) => (desabonnerBase = f));
    return () => {
      desabonnerBase?.();
      scan.arreter();
      connexion.arreter();
      miseAJour.arreter();
      listes.arreter();
      scripts.arreter();
    };
  });

  /** Largeur permise : bornes fixes, et jamais plus de 60 % de la fenêtre (le min l'emporte). */
  const borneListes = (l: number) =>
    Math.round(Math.min(Math.max(l, LISTES_MIN), Math.max(LISTES_MIN, Math.min(LISTES_MAX, window.innerWidth * 0.6))));

  function fixerLargeurListes(l: number) {
    largeurListes = borneListes(l);
    enregistrerPreference({ largeurListes });
  }

  function debutRedimensionListes(e: PointerEvent) {
    if (e.button !== 0) return;
    const poignee = e.currentTarget as HTMLElement;
    // Le preventDefault() plus bas (pas de sélection de texte pendant le tirage) annule aussi la
    // prise de focus du clic : sans ce focus explicite, les flèches, Début et Fin allaient à
    // l'élément qui avait le focus avant (grille, arbre…) et la poignée ne réagissait pas (R-02).
    poignee.focus({ preventScroll: true });
    poignee.setPointerCapture(e.pointerId);
    const x0 = e.clientX;
    const l0 = largeurListes;
    const bouger = (m: PointerEvent) => fixerLargeurListes(l0 + m.clientX - x0);
    const finir = () => {
      poignee.removeEventListener('pointermove', bouger);
      poignee.removeEventListener('pointerup', finir);
      poignee.removeEventListener('pointercancel', finir);
    };
    poignee.addEventListener('pointermove', bouger);
    poignee.addEventListener('pointerup', finir);
    poignee.addEventListener('pointercancel', finir);
    e.preventDefault();
  }

  function clavierPoigneeListes(e: KeyboardEvent) {
    const pas = e.shiftKey ? 64 : 16;
    if (e.key === 'ArrowRight') fixerLargeurListes(largeurListes + pas);
    else if (e.key === 'ArrowLeft') fixerLargeurListes(largeurListes - pas);
    else if (e.key === 'Home') fixerLargeurListes(LISTES_MIN);
    else if (e.key === 'End') fixerLargeurListes(LISTES_MAX);
    else return;
    e.preventDefault();
  }

  /** Écouteurs posés par attachement (séparateur focalisable : composant interactif ARIA). */
  function ecouteursPoigneeListes(el: HTMLElement) {
    el.addEventListener('pointerdown', debutRedimensionListes);
    el.addEventListener('keydown', clavierPoigneeListes);
    const defaut = () => fixerLargeurListes(LISTES_DEFAUT);
    el.addEventListener('dblclick', defaut);
    return () => {
      el.removeEventListener('pointerdown', debutRedimensionListes);
      el.removeEventListener('keydown', clavierPoigneeListes);
      el.removeEventListener('dblclick', defaut);
    };
  }

  function pret(e: EtatBiblio) {
    etat = e;
    nbLivres = e.nbLivres;
    void listes.charger();
    listes.demarrer();
    void scripts.demarrer();
  }

  /** Le backend a rouvert la bibliothèque (transfert terminé : bascule en autonome ou rechargement). */
  function bibliothequeRouverte(e: EtatBiblio) {
    etat = e;
    nbLivres = e.nbLivres;
    section = 'bibliotheque';
    listes.courante = null;
    void listes.charger();
    rafraichir++;
  }

  /** « Rechercher une mise à jour… » (Réglages) : fenêtre « Mise à jour » et recherche manuelle. */
  function rechercherMaj() {
    miseAJour.ouvrir();
    if (!miseAJour.occupe) void miseAJour.verifier();
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
  <div class="cadre">
  <BandeauMaj />
  <BandeauBase />
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
      largeur={largeurListes}
    />
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      class="poignee-listes"
      role="separator"
      aria-orientation="vertical"
      aria-label="Largeur de la colonne des listes"
      aria-valuemin={LISTES_MIN}
      aria-valuemax={LISTES_MAX}
      aria-valuenow={largeurListes}
      tabindex="0"
      title="Tirer pour élargir ou réduire la colonne des listes (double-clic : largeur par défaut)"
      {@attach ecouteursPoigneeListes}
      data-testid="poignee-listes"
    ></div>
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
  </div>
  <DialogDossiers bind:this={dossiers} {scan} />
  <DialogReglages bind:this={reglages} {themeActif} mode={etat.mode} onTheme={(id) => (themeActif = id)} onecriture={() => void listes.charger()} ontransfert={() => void transfert?.ouvrir()} onmaj={rechercherMaj} />
  <DialogTransfert bind:this={transfert} onbibliotheque={bibliothequeRouverte} />
  {#if scripts.gestionOuverte}
    <DialogScripts />
  {/if}
  {#if scripts.execution}
    {#key scripts.execution}
      <FenetreScript execution={scripts.execution} />
    {/key}
  {/if}
{/if}
{#if miseAJour.fenetre}
  <DialogMaj />
{/if}

<style>
  .cadre { height: 100vh; display: flex; flex-direction: column; }
  .appli { flex: 1; min-height: 0; display: flex; }
  .poignee-listes { flex: none; width: 5px; margin-left: -5px; position: relative; z-index: 1; cursor: col-resize; touch-action: none; }
  .poignee-listes:hover, .poignee-listes:focus-visible { background: var(--cr-accent); outline: none; }
  .message { padding: 40px; text-align: center; color: var(--cr-texte-discret); }
  .erreur { color: var(--cr-erreur); }
  .bientot { flex: 1; display: grid; place-content: center; text-align: center; color: var(--cr-texte-discret); }
  .bientot h1 { color: var(--cr-texte); margin: 0 0 8px; }
</style>
