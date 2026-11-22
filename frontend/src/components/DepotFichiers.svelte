<script lang="ts">
  // Fichier, dossier ou liste .cbl reçu de l'extérieur : passé en argument de ligne de commande
  // (`ComicRust.exe fichier.cbz`) ou déposé sur la fenêtre. Comme `open_path` de la version egui :
  // .cbl → importé dans les listes, le reste → lu dans le lecteur ; un dossier laisse le choix entre
  // le lire comme livre (dossier d'images) et l'ajouter aux dossiers de la bibliothèque.
  // Affiché seulement quand la bibliothèque est ouverte (le chemin du lancement attend jusque-là).
  import { onMount } from 'svelte';
  import { ajouterDossier, importerCbl, lancerScan, lancementInitial, messageErreur, type GenreChemin } from '../lib/api';
  import type { SuiviScan } from '../lib/bibliotheque.svelte';
  import { attendreOuvreur, ecouterDepots, enregistrerRouteurChemins, traiterChemin, type ActionsChemin } from '../lib/fenetre';
  import type { EtatListes } from '../lib/listes.svelte';
  import { fermerModale, modale } from '../lib/modale';

  interface Props {
    listes: EtatListes;
    scan: SuiviScan;
    /** Affiche la vue de la bibliothèque (l'écran « Listes » vide n'a pas de lecteur) et rend la main
     *  quand elle est montée. */
    versBibliotheque: () => Promise<void>;
  }
  let { listes, scan, versBibliotheque }: Props = $props();

  /** Un glisser est au-dessus de la fenêtre. */
  let survol = $state(false);
  let dossierPropose = $state<string | null>(null);
  let dialogue = $state<HTMLDialogElement>();
  let message = $state<{ texte: string; erreur: boolean } | null>(null);
  let minuterie: ReturnType<typeof setTimeout> | undefined;

  function dire(texte: string, erreur = false) {
    message = { texte, erreur };
    clearTimeout(minuterie);
    if (!erreur) minuterie = setTimeout(() => (message = null), 6000);
  }

  const actions: ActionsChemin = {
    lire: async (chemin) => {
      await versBibliotheque();
      const o = await attendreOuvreur();
      if (!o) throw new Error("La bibliothèque n'est pas affichée.");
      await o.ouvrirChemin(chemin);
    },
    importerListe: async (chemin) => {
      // Bilan et erreurs : sous le panneau des listes (comme tout import .cbl).
      const r = await listes.executer('Import de la liste', async () => (await importerCbl(chemin)).resultat);
      if (!r) dire('Import de la liste impossible (voir sous le panneau des listes).', true);
    },
    proposerDossier: (chemin) => (dossierPropose = chemin),
    erreur: (texte) => dire(texte, true),
  };

  async function traiter(chemin: string, genre?: GenreChemin) {
    try {
      await traiterChemin(chemin, actions, genre);
    } catch (e) {
      dire(messageErreur(e), true);
    }
  }

  /** Dossier choisi : lu comme livre (dossier d'images). */
  function lireDossier() {
    const c = dossierPropose;
    fermerModale(dialogue);
    dossierPropose = null;
    if (c) void traiter(c, 'livre');
  }

  /** Dossier choisi : ajouté aux dossiers de la bibliothèque, puis analysé. */
  async function ajouterAuxDossiers() {
    const c = dossierPropose;
    fermerModale(dialogue);
    dossierPropose = null;
    if (!c) return;
    try {
      await ajouterDossier(c);
      scan.annoncer();
      try {
        await lancerScan();
      } catch (e) {
        scan.enCours = false;
        throw e;
      }
      dire(`Dossier ajouté à la bibliothèque : ${c}. Analyse en cours.`);
    } catch (e) {
      dire(messageErreur(e), true);
    }
  }

  onMount(() => {
    let vivant = true;
    let arreter: (() => void) | undefined;
    const oublierRouteur = enregistrerRouteurChemins((c) => traiter(c));
    // Premier chemin déposé seulement (comme la version egui).
    void ecouterDepots(
      (chemins) => void traiter(chemins[0]),
      (actif) => (survol = actif),
    )
      .then((f) => (vivant ? (arreter = f) : f()))
      .catch((e) => console.warn('glisser-déposer indisponible', e));
    // Argument de ligne de commande : rendu une seule fois par le backend.
    void lancementInitial()
      .then((l) => {
        if (l && vivant) void traiter(l.chemin, l.genre);
      })
      .catch((e) => console.warn('argument de lancement', e));
    return () => {
      vivant = false;
      oublierRouteur();
      arreter?.();
      clearTimeout(minuterie);
    };
  });
</script>

{#if survol}
  <div class="depot" aria-hidden="true" data-testid="depot-survol">
    <p>Déposez une BD, un dossier d'images ou une liste .cbl pour l'ouvrir</p>
  </div>
{/if}

{#if message}
  <div class="message" class:erreur={message.erreur} role={message.erreur ? 'alert' : 'status'} data-testid="depot-message">
    <span>{message.texte}</span>
    <button type="button" onclick={() => (message = null)} aria-label="Fermer le message">×</button>
  </div>
{/if}

{#if dossierPropose}
  <dialog bind:this={dialogue} use:modale={{ ouvrirAuMontage: true, fermer: () => (dossierPropose = null) }} aria-labelledby="titre-depot-dossier" data-testid="dialogue-depot-dossier">
    <h2 id="titre-depot-dossier">Dossier déposé</h2>
    <p class="chemin">{dossierPropose}</p>
    <p>Que faire de ce dossier ?</p>
    <div class="choix">
      <button type="button" class="principal" onclick={lireDossier} data-testid="depot-lire-dossier">Le lire comme un livre (images du dossier)</button>
      <button type="button" onclick={() => void ajouterAuxDossiers()} data-testid="depot-ajouter-dossier">L'ajouter aux dossiers de la bibliothèque et l'analyser</button>
      <button type="button" onclick={() => (dossierPropose = null)} data-testid="depot-annuler">Annuler</button>
    </div>
  </dialog>
{/if}

<style>
  .depot { position: fixed; inset: 0; z-index: 1000; display: grid; place-content: center; pointer-events: none; background: color-mix(in srgb, var(--cr-fond) 70%, transparent); border: 3px dashed var(--cr-accent); margin: 6px; border-radius: var(--cr-rayon); }
  .depot p { margin: 0; padding: 14px 22px; font-size: 18px; font-weight: 600; color: var(--cr-texte); background: var(--cr-fond-panneau); border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); }
  .message { position: fixed; right: 16px; bottom: 16px; z-index: 900; max-width: min(520px, calc(100vw - 32px)); display: flex; align-items: flex-start; gap: 8px; padding: 8px 12px; font-size: 13px; color: var(--cr-texte); background: var(--cr-fond-panneau); border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); box-shadow: var(--cr-ombre); overflow-wrap: anywhere; }
  .message.erreur { border-color: var(--cr-erreur); color: var(--cr-erreur); }
  .message button { border: none; background: none; color: inherit; cursor: pointer; font-size: 16px; line-height: 1; }
  .chemin { font-family: ui-monospace, Consolas, monospace; font-size: 12px; overflow-wrap: anywhere; color: var(--cr-texte-discret); }
  .choix { display: grid; gap: 8px; }
  .choix .principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); font-weight: 600; }
</style>
