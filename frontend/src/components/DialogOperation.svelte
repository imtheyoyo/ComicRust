<script lang="ts">
  // Opérations sur les fichiers de la sélection (menu contextuel) : « Écrire les infos dans le
  // fichier » et « Convertir en CBZ » (convertir_cbr_cbz). Étapes : confirmation, garde-fou des
  // opérations en masse (confirmationRequise), progression (edition-progression, Arrêter), bilan.
  import { modale } from '../lib/modale';
  import { onDestroy, onMount } from 'svelte';
  import {
    annulerOperation, convertirCbrCbz, ecouter, ecrireInfosFichiers, enregistrerReglagesEdition, lireReglagesEdition,
    messageErreur, type BilanOperation, type ProgressionEdition,
  } from '../lib/api';
  import ResultatsLot, { pourcent } from './ResultatsLot.svelte';

  interface Props {
    operation: 'conversion' | 'ecritureInfos';
    ids: string[];
    onfermer: () => void;
  }
  let { operation, ids, onfermer }: Props = $props();

  let etape = $state<'choix' | 'confirmation' | 'cours' | 'bilan'>('choix');
  let rangerOriginal = $state(false);
  let messageGardeFou = $state('');
  let progression = $state<ProgressionEdition | null>(null);
  let bilan = $state<BilanOperation | null>(null);
  let erreur = $state('');
  let desabonner: (() => void) | undefined;

  const conversion = $derived(operation === 'conversion');
  const n = $derived(ids.length);
  const titre = $derived(
    conversion
      ? (n > 1 ? `Convertir ${n.toLocaleString('fr-FR')} livres en CBZ` : 'Convertir en CBZ')
      : (n > 1 ? `Écrire les infos dans ${n.toLocaleString('fr-FR')} fichiers` : 'Écrire les infos dans le fichier'),
  );

  onMount(() => {
    void ecouter<ProgressionEdition>('edition-progression', (p) => {
      if (etape === 'cours' && p.operation === operation) progression = p;
    }).then((f) => (desabonner = f));
    if (conversion) {
      lireReglagesEdition().then((r) => (rangerOriginal = r.rangerOriginal)).catch(() => {});
    }
  });
  onDestroy(() => desabonner?.());

  async function lancer(confirme = false) {
    erreur = '';
    etape = 'cours';
    progression = null;
    try {
      if (conversion) void enregistrerReglagesEdition({ rangerOriginal }).catch(() => {});
      const b = conversion ? await convertirCbrCbz(ids, rangerOriginal, confirme) : await ecrireInfosFichiers(ids, confirme);
      if (b.confirmationRequise) {
        messageGardeFou = b.confirmationRequise.message;
        etape = 'confirmation';
        return;
      }
      bilan = b;
      etape = 'bilan';
    } catch (e) {
      erreur = messageErreur(e);
      etape = 'choix';
    }
  }

  /** Le parent retire la fenêtre ({#if}) : pas de dialog.close() ici (voir lib/modale.ts). */
  function fermer() {
    onfermer();
  }

  /** Échap : pas pendant l'opération (le bilan doit arriver). */
  const peutFermer = () => etape !== 'cours';
</script>

<dialog aria-labelledby="titre-operation" use:modale={{ peutFermer, fermer, ouvrirAuMontage: true }} data-testid="dialogue-operation">
  <h2 id="titre-operation">{titre}</h2>

  {#if etape === 'choix'}
    {#if conversion}
      <p>Les CBR, CB7 et CBT sont convertis en CBZ à côté de l'original (pages recopiées telles quelles, infos de la fiche écrites
        dans le ComicInfo.xml). Les autres formats sont ignorés.</p>
      <label class="case">
        <input type="checkbox" bind:checked={rangerOriginal} data-testid="ranger-original" />
        Ranger l'original dans le dossier « _originaux » (sinon il reste en place)
      </label>
    {:else}
      <p>La fiche de chaque livre est écrite dans le ComicInfo.xml de son fichier. Seuls les CBZ reçoivent les infos (convertir
        d'abord en CBZ) ; les autres formats sont ignorés.</p>
    {/if}
    {#if erreur}<p class="erreur" role="alert">{erreur}</p>{/if}
    <div class="pied">
      <button onclick={fermer}>Annuler</button>
      <button class="principal" onclick={() => void lancer()} data-testid="operation-lancer">{conversion ? 'Convertir' : 'Écrire'}</button>
    </div>
  {:else if etape === 'confirmation'}
    <p class="garde-fou" role="alert">{messageGardeFou}</p>
    <div class="pied">
      <button onclick={fermer}>Annuler</button>
      <button class="principal" onclick={() => void lancer(true)} data-testid="operation-confirmer">Confirmer</button>
    </div>
  {:else if etape === 'cours'}
    <p aria-live="polite">{progression ? `${progression.fait} / ${progression.total}${progression.livre ? ' — ' + progression.livre : ''}` : 'Démarrage…'}</p>
    <div class="barre-progression" role="progressbar" aria-label="Progression" aria-valuemin="0" aria-valuemax="100" aria-valuenow={pourcent(progression)}>
      <div style:width="{pourcent(progression)}%"></div>
    </div>
    <div class="pied">
      <button onclick={() => void annulerOperation()} title="Arrêter entre deux livres (jamais au milieu d'un fichier)">Arrêter</button>
    </div>
  {:else if bilan}
    <ResultatsLot {bilan} />
    <div class="pied">
      <button class="principal" onclick={fermer} data-testid="operation-fermer">Fermer</button>
    </div>
  {/if}
</dialog>

<style>
  p { margin: 0 0 10px; line-height: 1.45; }
  .case { display: flex; gap: 6px; align-items: center; margin-bottom: 10px; }
  .case input { accent-color: var(--cr-accent); }
  .erreur, .garde-fou { color: var(--cr-erreur); }
  .pied { display: flex; justify-content: flex-end; gap: 8px; margin-top: 12px; }
  button.principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); font-weight: 600; }
  button.principal:hover:not(:disabled) { background: var(--cr-accent); filter: brightness(1.1); }
</style>
