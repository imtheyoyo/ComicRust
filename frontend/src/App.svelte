<script lang="ts">
  import { onMount, tick } from 'svelte';
  import DepotFichiers from './components/DepotFichiers.svelte';
  import DialogPlantage from './components/DialogPlantage.svelte';
  import { gererClavierFenetre, lecteurAffiche, suivrePleinEcran } from './lib/fenetre';
  import BarreLaterale, { type Section } from './components/BarreLaterale.svelte';
  import BandeauBase from './components/BandeauBase.svelte';
  import BandeauMaj from './components/BandeauMaj.svelte';
  import DialogMaj from './components/DialogMaj.svelte';
  import DialogDossiers from './components/DialogDossiers.svelte';
  import DialogReglages from './components/DialogReglages.svelte';
  import DialogAPropos from './components/DialogAPropos.svelte';
  import DialogTransfert from './components/DialogTransfert.svelte';
  import DialogScripts from './components/DialogScripts.svelte';
  import FenetreScript from './components/FenetreScript.svelte';
  import PremierDemarrage from './components/PremierDemarrage.svelte';
  import VueBibliotheque from './components/VueBibliotheque.svelte';
  import { dansTauri, ecouter, ouvrirBibliotheque, type BibliothequeModifiee, type EtatBiblio, messageErreur } from './lib/api';
  import { SuiviScan } from './lib/bibliotheque.svelte';
  import { connexion } from './lib/connexion.svelte';
  import { EtatListes } from './lib/listes.svelte';
  import { jauges } from './lib/jauges.svelte';
  import { miseAJour } from './lib/maj.svelte';
  import { scripts } from './lib/scripts.svelte';
  import { ID_MANQUANTS, NOEUD_MANQUANTS, manquants } from './lib/manquants.svelte';
  import DialogManquants from './components/DialogManquants.svelte';
  import { chargerPreferences, enregistrerDerniereListe, enregistrerPreference, preferences, rouvrirDerniereListe } from './lib/prefs';
  import { appliquerTheme, messageTheme, themeMemorise } from './lib/theme';

  let etat = $state<EtatBiblio | null>(null);
  let erreur = $state('');
  /** Bibliothèque qui ne s'ouvre pas : l'assistant est affiché pour en choisir une autre. */
  let choisirAutre = $state(false);
  let section = $state<Section>('bibliotheque');
  let themeActif = $state('');
  /** Avertissement discret : le thème choisi n'a pas pu être appliqué (le démarrage continue). */
  let avertissementTheme = $state('');
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
  let apropos = $state<DialogAPropos>();
  /** Assistant de configuration rejoué (Réglages ▸ « Changer de bibliothèque / de connexion… »). */
  let modifierBiblio = $state(false);
  /** Panneau des listes affiché (mémorisé : `vue.listesVisibles`). Masqué : barre latérale réduite. */
  let listesVisibles = $state(true);

  /** Attente maximale de l'arbre des listes avant d'afficher la vue (ms). */
  const ATTENTE_RESTAURATION_MS = 2500;

  /** Ouvre la bibliothèque enregistrée (démarrage, « Réessayer » après une erreur). */
  async function ouvrir() {
    erreur = '';
    choisirAutre = false;
    try {
      const ouverte = await ouvrirBibliotheque();
      if (!ouverte.premierDemarrage) {
        // Liste à rouvrir : la vue n'est affichée qu'après sa résolution, pour ne pas montrer (et
        // charger) d'abord la bibliothèque entière. Attente plafonnée : au-delà, la restauration se
        // fait quand même, un peu plus tard.
        const restauration = restaurerDerniereListe();
        if (rouvrirDerniereListe() && preferences().derniereListe) {
          await Promise.race([restauration, new Promise((r) => setTimeout(r, ATTENTE_RESTAURATION_MS))]);
        }
      }
      etat = ouverte;
      nbLivres = etat.nbLivres;
      if (!etat.premierDemarrage) {
        listes.demarrer();
        // Scripts (tauri-v0.9.0) : liste et écoute des événements, jamais bloquant.
        void scripts.demarrer();
        // Analyse des dossiers au démarrage (réglage « Analyse et suivi », activée par défaut).
        void scan.lancerAuto();
        void demarrerManquants(etat.mode);
      }
    } catch (e) {
      erreur = messageErreur(e);
    }
  }

  /** Nombre de changements de liste ou de section faits depuis le démarrage : la restauration de la
   *  dernière liste s'efface devant un choix fait par l'utilisateur pendant le chargement. */
  let navigations = 0;

  /** Démarrage : charge l'arbre des listes puis, si le réglage « Rouvrir la dernière liste au
   *  démarrage » est actif (par défaut), revient sur la dernière liste sélectionnée (dossier, liste de
   *  lecture, liste intelligente), quelle que soit la section ou le lecteur ouvert à la fermeture.
   *  Aucun livre n'est rouvert ; l'identifiant est comparé sans tenir compte de la casse. Jamais d'erreur : liste supprimée (identifiant inconnu : autre
   *  bibliothèque, liste effacée) = bibliothèque entière, et la mémoire est effacée (pas de boucle) ;
   *  arbre illisible ou indisponible = bibliothèque, la mémoire est gardée pour le prochain démarrage. */
  async function restaurerDerniereListe() {
    const avant = navigations;
    await listes.charger();
    if (!rouvrirDerniereListe()) return;
    const id = preferences().derniereListe;
    if (!id || navigations !== avant || !listes.arbre) return;
    const n = listes.retrouver(id); // sans tenir compte de la casse du GUID
    if (n) {
      listes.devoiler(n.id); // la ligne est visible dans l'arbre (dossiers parents dépliés)
      choisirListe(n.id);
    } else if (!listes.arbre.etat.erreur) enregistrerDerniereListe(null);
  }

  onMount(() => {
    void (async () => {
      try {
        // Préférences d'affichage d'abord : le thème et le premier lister_livres en dépendent.
        const prefs = await chargerPreferences();
        if (typeof prefs.largeurListes === 'number') largeurListes = borneListes(prefs.largeurListes);
        if (typeof prefs.listesVisibles === 'boolean') listesVisibles = prefs.listesVisibles;
        // Un thème inutilisable ne doit jamais empêcher le démarrage : on garde les replis d'app.css.
        try {
          const voulu = themeMemorise();
          themeActif = await appliquerTheme(voulu);
          if (themeActif !== voulu) avertissementTheme = `Le thème « ${voulu} » est indisponible : le thème par défaut est affiché (votre choix est conservé).`;
        } catch (e) {
          avertissementTheme = `Thème non appliqué : ${messageTheme(e)} Les couleurs de secours sont utilisées ; corrigez le thème dans les Réglages.`;
        }
        await scan.demarrer(() => {
          rafraichir++;
          void manquants.rafraichir();
        });
        await connexion.demarrer();
        // Mise à jour : jamais bloquante (vérification automatique en arrière-plan, bandeau seul).
        void miseAJour.demarrer();
        await ouvrir();
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
      if (m.origine === 'externe' || m.origine === 'manquants') void listes.charger();
    }).then((f) => (desabonnerBase = f));
    // Plein écran de la fenêtre suivi (F11 et Échap de la bibliothèque : lib/fenetre.ts).
    const arreterPleinEcran = suivrePleinEcran();
    return () => {
      arreterPleinEcran();
      desabonnerBase?.();
      scan.arreter();
      connexion.arreter();
      miseAJour.arreter();
      listes.arreter();
      jauges.arreter();
      scripts.arreter();
      manquants.arreter();
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

  function basculerListes() {
    listesVisibles = !listesVisibles;
    enregistrerPreference({ listesVisibles });
  }

  /** Ctrl+Maj+L (comme le bouton « 📂 Listes » d'egui) : pas dans une saisie ni sous une fenêtre ouverte. */
  function clavierListes(e: KeyboardEvent) {
    if (!(e.ctrlKey || e.metaKey) || !e.shiftKey || e.altKey || e.key.toLowerCase() !== 'l') return;
    if (lecteurAffiche() || modifierBiblio) return;
    if ((e.target as HTMLElement | null)?.closest('input, textarea, select') || document.querySelector('dialog[open], [aria-modal="true"]')) return;
    e.preventDefault();
    basculerListes();
  }

  /** Assistant rejoué terminé : même rechargement que la fin d'un transfert (listes, vue, analyse). */
  function biblioChangee(e: EtatBiblio) {
    modifierBiblio = false;
    bibliothequeRouverte(e);
  }

  function pret(e: EtatBiblio) {
    erreur = '';
    choisirAutre = false;
    etat = e;
    nbLivres = e.nbLivres;
    void listes.charger();
    listes.demarrer();
    void scripts.demarrer();
    void scan.lancerAuto();
    void demarrerManquants(e.mode);
  }

  /** Livres manquants (lot L3) : suivi du compteur ; en mode ComicRack, vérification automatique à
   *  l'ouverture (la marque n'y est jamais écrite dans la base : elle vit en mémoire). En mode autonome,
   *  le scan des dossiers tient la base à jour. */
  async function demarrerManquants(mode: string) {
    manquants.surModification = () => void listes.charger();
    manquants.surRetour = () => choisirListe(null);
    await manquants.demarrer(mode === 'comicrack');
  }

  /** Le backend a rouvert la bibliothèque (transfert terminé : bascule en autonome ou rechargement). */
  function bibliothequeRouverte(e: EtatBiblio) {
    etat = e;
    nbLivres = e.nbLivres;
    manquants.masquer();
    section = 'bibliotheque';
    listes.courante = null;
    enregistrerDerniereListe(null); // autre bibliothèque : l'ancienne liste n'y existe pas
    void listes.charger();
    rafraichir++;
    void scan.lancerAuto();
    void demarrerManquants(e.mode);
  }

  /** « Rechercher une mise à jour… » (Réglages) : fenêtre « Mise à jour » et recherche manuelle. */
  function rechercherMaj() {
    miseAJour.ouvrir();
    if (!miseAJour.occupe) void miseAJour.verifier();
  }

  function choisirSection(s: Section) {
    manquants.masquer();
    navigations++;
    section = s;
    if (s !== 'listes') {
      listes.courante = null;
      enregistrerDerniereListe(null);
    }
  }

  function choisirListe(id: string | null) {
    navigations++;
    // Liste virtuelle « Livres manquants » : aucune liste de l'arbre n'est courante.
    if (id === ID_MANQUANTS) {
      manquants.afficher();
      listes.courante = null;
      section = 'listes';
      return;
    }
    manquants.masquer();
    listes.courante = id || null;
    section = id ? 'listes' : 'bibliotheque';
    // Mémorisée par identifiant stable (debounce et écriture côté backend : prefs.ts).
    enregistrerDerniereListe(id || null);
  }

  /** Fichier reçu de l'extérieur (argument, dépôt) : la vue qui porte le lecteur doit être affichée. */
  async function versBibliotheque() {
    if (section === 'bibliotheque' || listeAffichee) return;
    choisirSection('bibliotheque');
    await tick();
  }

  const listeAffichee = $derived(manquants.affiche ? NOEUD_MANQUANTS : section === 'listes' ? listes.noeud(listes.courante) ?? null : null);
</script>

<!-- F11, Échap (sortie du plein écran) et Ctrl+O hors lecteur ; phase de capture : avant les touches de la vue. -->
<svelte:window onkeydowncapture={gererClavierFenetre} onkeydown={clavierListes} />
{#if erreur && choisirAutre}
  <PremierDemarrage onpret={pret} probleme={erreur} onretour={() => void ouvrir()} />
{:else if erreur}
  <div class="message" data-testid="erreur-ouverture">
    <p class="erreur" role="alert">{erreur}</p>
    <p class="boutons-erreur">
      <button type="button" onclick={() => void ouvrir()}>Réessayer</button>
      <button type="button" class="principal" onclick={() => (choisirAutre = true)} data-testid="choisir-bibliotheque">Choisir une autre bibliothèque…</button>
    </p>
  </div>
{:else if !etat}
  <p class="message">Ouverture de la bibliothèque…{dansTauri() ? '' : ' (mode simulé)'}</p>
{:else if etat.premierDemarrage}
  <PremierDemarrage onpret={pret} baseAbsente={etat.baseAbsente} />
{:else}
  <div class="cadre" inert={modifierBiblio}>
  {#if avertissementTheme}
    <div class="avis-theme" role="status" data-testid="avertissement-theme">
      <span>{avertissementTheme}</span>
      <button type="button" onclick={() => (avertissementTheme = '')} aria-label="Fermer l'avertissement">×</button>
    </div>
  {/if}
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
      {listesVisibles}
      onbasculerlistes={basculerListes}
      onaide={() => void apropos?.ouvrir()}
    />
    {#if listesVisibles}
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
    {/if}
    {#if section === 'bibliotheque' || listeAffichee}
      <VueBibliotheque {scan} bind:nbLivres {rafraichir} {listes} liste={listeAffichee} />
    {:else if section === 'listes'}
      <main class="bientot">
        <h1>Listes</h1>
        <p>Choisis une liste dans le panneau de gauche.</p>
      </main>
    {/if}
  </div>
  </div>
  {#if modifierBiblio}
    <!-- L'application reste en place dessous (annuler ne change rien) ; `inert` la rend inaccessible au clavier. -->
    <div class="superposition" role="dialog" aria-modal="true" aria-label="Changer de bibliothèque ou de connexion" data-testid="assistant-modification">
      <PremierDemarrage modification onpret={biblioChangee} onannuler={() => (modifierBiblio = false)} />
    </div>
  {/if}
  <DepotFichiers {listes} {scan} {versBibliotheque} />
  <DialogDossiers bind:this={dossiers} {scan} />
  <DialogReglages bind:this={reglages} {themeActif} mode={etat.mode} onTheme={(id) => (themeActif = id)} onecriture={() => void listes.charger()} ontransfert={() => void transfert?.ouvrir()} onmaj={rechercherMaj} onassistant={() => (modifierBiblio = true)} />
  <DialogAPropos bind:this={apropos} />
  <DialogTransfert bind:this={transfert} onbibliotheque={bibliothequeRouverte} />
  <DialogManquants />
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
<!-- Rapport de plantage de la session précédente (data\plantage.txt) : montré dans tous les états. -->
<DialogPlantage />

<style>
  .cadre { height: 100vh; display: flex; flex-direction: column; }
  .superposition { position: fixed; inset: 0; z-index: 50; overflow: auto; background: var(--cr-fond); }
  .avis-theme { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 4px 12px; font-size: 12px; color: var(--cr-texte-discret); border-bottom: 1px solid var(--cr-bordure); }
  .avis-theme button { border: none; background: none; color: inherit; cursor: pointer; font-size: 16px; line-height: 1; }
  .appli { flex: 1; min-height: 0; display: flex; }
  .poignee-listes { flex: none; width: 5px; margin-left: -5px; position: relative; z-index: 1; cursor: col-resize; touch-action: none; }
  .poignee-listes:hover, .poignee-listes:focus-visible { background: var(--cr-accent); outline: none; }
  .message { padding: 40px; text-align: center; color: var(--cr-texte-discret); }
  .erreur { color: var(--cr-erreur); white-space: pre-wrap; overflow-wrap: anywhere; }
  .boutons-erreur { display: flex; justify-content: center; gap: 8px; }
  .boutons-erreur .principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); font-weight: 600; }
  .bientot { flex: 1; display: grid; place-content: center; text-align: center; color: var(--cr-texte-discret); }
  .bientot h1 { color: var(--cr-texte); margin: 0 0 8px; }
</style>
