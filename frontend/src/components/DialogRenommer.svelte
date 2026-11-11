<script lang="ts">
  // « Renommer / déplacer » les fichiers de la sélection d'après un modèle de nom (format d'export
  // de ComicRack) : APERÇU calculé par le backend (renommer_deplacer apercu:true) à chaque
  // changement, puis application des seules lignes sans problème (jamais d'écrasement).
  import { connexion } from '../lib/connexion.svelte';
  import { modale } from '../lib/modale';
  import { onDestroy, onMount } from 'svelte';
  import {
    annulerOperation, ecouter, lireReglagesEdition, messageErreur, modelesDeNom, renommerDeplacer,
    type BilanOperation, type ModeleNom, type ProgressionEdition, type ResultatRenommage,
  } from '../lib/api';
  import ResultatsLot, { pourcent } from './ResultatsLot.svelte';

  interface Props {
    ids: string[];
    onfermer: () => void;
  }
  let { ids, onfermer }: Props = $props();

  let modeles = $state<ModeleNom[]>([]);
  let modele = $state('');
  let dossierCible = $state('');
  let apercu = $state<ResultatRenommage | null>(null);
  /** Paramètres de l'aperçu affiché (le bouton n'applique qu'un aperçu à jour). */
  let apercuDe = $state('');
  let calcul = $state(false);
  let erreur = $state('');
  let etape = $state<'apercu' | 'confirmation' | 'cours' | 'bilan'>('apercu');
  let messageGardeFou = $state('');
  let progression = $state<ProgressionEdition | null>(null);
  let bilan = $state<BilanOperation | null>(null);
  let pret = $state(false);
  /** Modèle et dossier tels qu'à l'ouverture (réglages enregistrés) : référence des « modifications ». */
  let initial = $state('');
  let minuterie: ReturnType<typeof setTimeout> | undefined;
  let sequence = 0;
  let desabonner: (() => void) | undefined;

  onMount(() => {
    void ecouter<ProgressionEdition>('edition-progression', (p) => {
      if (etape === 'cours' && p.operation === 'renommage') progression = p;
    }).then((f) => (desabonner = f));
    void (async () => {
      try {
        const [m, r] = await Promise.all([modelesDeNom(), lireReglagesEdition()]);
        modeles = m;
        modele = r.modeleNom || m[0]?.modele || '';
        dossierCible = r.dossierCible ?? '';
      } catch (e) {
        erreur = messageErreur(e);
      }
      initial = JSON.stringify([modele, dossierCible.trim()]);
      pret = true;
    })();
  });
  onDestroy(() => {
    desabonner?.();
    clearTimeout(minuterie);
  });

  const cle = $derived(JSON.stringify([modele, dossierCible.trim()]));

  // Aperçu recalculé ~300 ms après chaque modification du modèle ou du dossier.
  $effect(() => {
    const k = cle;
    if (!pret || etape !== 'apercu') return;
    clearTimeout(minuterie);
    const n = ++sequence;
    calcul = true;
    minuterie = setTimeout(async () => {
      try {
        const r = await renommerDeplacer({ ids, modele, dossierCible: dossierCible.trim() || undefined, apercu: true });
        if (n !== sequence) return;
        apercu = r;
        apercuDe = k;
        erreur = '';
      } catch (e) {
        if (n !== sequence) return;
        apercu = null;
        apercuDe = '';
        erreur = messageErreur(e);
      } finally {
        if (n === sequence) calcul = false;
      }
    }, 300);
  });

  const aJour = $derived(!!apercu && apercuDe === cle && !calcul);
  const nbProblemes = $derived(apercu?.lignes.filter((l) => l.probleme).length ?? 0);
  const nbInchanges = $derived(apercu?.lignes.filter((l) => l.inchange && !l.probleme).length ?? 0);

  async function appliquer(confirme = false) {
    etape = 'cours';
    progression = null;
    erreur = '';
    try {
      const r = await renommerDeplacer({ ids, modele, dossierCible: dossierCible.trim() || undefined, apercu: false, confirme });
      if (r.bilan?.confirmationRequise) {
        messageGardeFou = r.bilan.confirmationRequise.message;
        etape = 'confirmation';
        return;
      }
      bilan = r.bilan ?? null;
      etape = 'bilan';
    } catch (e) {
      erreur = messageErreur(e);
      etape = 'apercu';
    }
  }

  /** Le parent retire la fenêtre ({#if}) : pas de dialog.close() ici (voir lib/modale.ts). */
  function fermer() {
    onfermer();
  }

  /** Modèle de nom ou dossier cible changé depuis l'ouverture (recette tauri-v0.5.0 D-01). */
  const modifie = $derived(pret && cle !== initial);

  /** Échap, comme « Infos du livre » : pas pendant l'opération (le bilan doit arriver), ni quand
   *  le modèle ou le dossier a été modifié (Annuler pour fermer) ; le bilan affiché se ferme. */
  const peutFermer = () => etape === 'bilan' || (etape !== 'cours' && !modifie);

  const modeleChoisi = $derived(modeles.find((m) => m.modele === modele)?.modele ?? '');
</script>

<dialog class="renommer" aria-labelledby="titre-renommer" use:modale={{ peutFermer, fermer, ouvrirAuMontage: true }} data-testid="dialogue-renommer">
  <h2 id="titre-renommer">Renommer / déplacer {ids.length > 1 ? `${ids.length.toLocaleString('fr-FR')} fichiers` : 'le fichier'}</h2>

  {#if etape === 'apercu' || etape === 'confirmation'}
    <div class="reglages">
      <label for="modele-predefini">Modèle</label>
      <select id="modele-predefini" value={modeleChoisi} onchange={(e) => { if (e.currentTarget.value) modele = e.currentTarget.value; }}>
        <option value="">(personnalisé)</option>
        {#each modeles as m (m.modele)}<option value={m.modele}>{m.nom}</option>{/each}
      </select>
      <label for="modele-nom">Modèle de nom</label>
      <input id="modele-nom" type="text" bind:value={modele} spellcheck="false" autocomplete="off" data-testid="modele-nom" />
      <label for="dossier-cible">Dossier cible</label>
      <input id="dossier-cible" type="text" bind:value={dossierCible} spellcheck="false" autocomplete="off"
        placeholder="Vide : chaque fichier reste dans son dossier (simple renommage)" data-testid="dossier-cible" />
    </div>
    <details class="aide">
      <summary>Syntaxe des modèles</summary>
      <ul>
        <li><code>{'{series}'}</code>, <code>{'{number}'}</code>, <code>{'{volume}'}</code>, <code>{'{year}'}</code>, <code>{'{month}'}</code>, <code>{'{title}'}</code>,
          <code>{'{publisher}'}</code>, <code>{'{format}'}</code>, <code>{'{filename}'}</code> (nom actuel)… : valeur du champ ;</li>
        <li><code>{'{number:000}'}</code> : nombre complété par des zéros (007) ; <code>{'{{clé}}'}</code> : valeur personnalisée ;</li>
        <li><code>[ … ]</code> : partie omise si un champ qu'elle contient est vide ; <code>\x</code> : caractère tel quel ;</li>
        <li><code>/</code> : sous-dossier. L'extension est gardée ; les caractères interdits de Windows sont remplacés.</li>
      </ul>
    </details>

    <div class="apercu" aria-busy={calcul} data-testid="apercu-renommage">
      {#if erreur}
        <p class="erreur" role="alert">{erreur}</p>
      {:else if apercu}
        <p class="compte" aria-live="polite">
          Aperçu{calcul ? ' (mise à jour…)' : ''} : <strong>{apercu.aRenommer}</strong> fichier(s) à renommer{nbInchanges ? `, ${nbInchanges} déjà bien nommé(s)` : ''}{nbProblemes ? `, ${nbProblemes} non appliqué(s) (problème)` : ''}.
        </p>
        <table>
          <thead><tr><th scope="col">Actuel</th><th scope="col">Nouveau</th></tr></thead>
          <tbody>
            {#each apercu.lignes as l (l.id)}
              <tr class:probleme={!!l.probleme} class:inchange={l.inchange && !l.probleme}>
                <td title={l.de}>{l.deNom}</td>
                <td title={l.vers}>
                  {#if l.probleme}<span class="motif">⚠ {l.probleme}</span>{#if l.versNom}<br /><span class="discret">{l.versNom}</span>{/if}
                  {:else if l.inchange}<span class="discret">inchangé</span>
                  {:else}{l.versNom}{/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <p class="discret">Calcul de l'aperçu…</p>
      {/if}
    </div>

    {#if etape === 'confirmation'}
      <p class="garde-fou" role="alert">{messageGardeFou}</p>
      <div class="pied">
        <button onclick={() => (etape = 'apercu')}>Annuler</button>
        <button class="principal" onclick={() => void appliquer(true)} data-testid="renommer-confirmer">Confirmer</button>
      </div>
    {:else}
      <div class="pied">
        <button onclick={fermer}>Annuler</button>
        <button class="principal" disabled={!aJour || !apercu?.aRenommer || !connexion.enLigne} title={connexion.motifEcriture} onclick={() => void appliquer()} data-testid="renommer-appliquer"
          >Renommer {apercu && aJour ? apercu.aRenommer : ''} fichier(s)</button>
      </div>
    {/if}
  {:else if etape === 'cours'}
    <p aria-live="polite">{progression ? `${progression.fait} / ${progression.total}${progression.livre ? ' — ' + progression.livre : ''}` : 'Démarrage…'}</p>
    <div class="barre-progression" role="progressbar" aria-label="Progression" aria-valuemin="0" aria-valuemax="100" aria-valuenow={pourcent(progression)}>
      <div style:width="{pourcent(progression)}%"></div>
    </div>
    <div class="pied"><button onclick={() => void annulerOperation()}>Arrêter</button></div>
  {:else if bilan}
    <ResultatsLot {bilan} />
    <div class="pied"><button class="principal" onclick={fermer} data-testid="renommer-fermer">Fermer</button></div>
  {/if}
</dialog>

<style>
  .renommer { width: min(820px, calc(100vw - 32px)); }
  .reglages { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 6px 10px; align-items: center; }
  .reglages label { color: var(--cr-texte-discret); }
  .reglages input { font-family: ui-monospace, Consolas, monospace; }
  .reglages select { justify-self: start; }
  .aide { margin: 8px 0; font-size: 12px; color: var(--cr-texte-discret); }
  .aide summary { cursor: pointer; }
  .aide ul { margin: 4px 0 0; padding-left: 18px; line-height: 1.6; }
  code { font-family: ui-monospace, Consolas, monospace; background: var(--cr-fond-survol); padding: 0 4px; border-radius: 3px; }
  .apercu { max-height: 46vh; overflow: auto; border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); padding: 6px 8px; background: var(--cr-fond); }
  .apercu[aria-busy='true'] table { opacity: 0.6; }
  .compte { margin: 0 0 6px; }
  table { width: 100%; border-collapse: collapse; font-size: 12px; table-layout: fixed; }
  th { text-align: left; color: var(--cr-texte-discret); font-weight: 600; padding: 3px 4px; border-bottom: 1px solid var(--cr-bordure); }
  td { padding: 3px 4px; border-bottom: 1px solid var(--cr-bordure); overflow-wrap: anywhere; vertical-align: top; }
  tr.probleme td { color: var(--cr-erreur); }
  tr.inchange td, .discret { color: var(--cr-texte-discret); }
  .erreur, .garde-fou { color: var(--cr-erreur); }
  .pied { display: flex; justify-content: flex-end; gap: 8px; margin-top: 12px; }
  button.principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); font-weight: 600; }
  button.principal:hover:not(:disabled) { background: var(--cr-accent); filter: brightness(1.1); }
</style>
