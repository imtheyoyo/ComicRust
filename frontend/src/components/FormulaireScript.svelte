<script lang="ts">
  // Formulaire demandé par un script (`comicrust.form`, événement script:formulaire). Les valeurs
  // partent dans script_repondre : objet des valeurs (+ clé réservée « $bouton » : bouton cliqué),
  // ou null si l'utilisateur annule (bouton d'annulation, Échap). Entrée : premier bouton qui
  // valide ; la validation locale (obligatoire, bornes, doublons) reprend celle du cœur.
  import { tick } from 'svelte';
  import { CLE_BOUTON_FORMULAIRE, type FormSpecScript } from '../lib/api';
  import { boutons, controlesInactifs, valider, valeursInitiales } from '../lib/formulaireScript';
  import { modale } from '../lib/modale';
  import ControlesScript from './ControlesScript.svelte';

  interface Props {
    spec: FormSpecScript;
    nomScript: string;
    onrepondre: (valeur: Record<string, unknown> | null) => void;
    onarreter: () => void;
  }
  let { spec, nomScript, onrepondre, onarreter }: Props = $props();

  // Valeurs du formulaire : calculées une fois à l'ouverture (le composant est recréé par {#key}).
  // svelte-ignore state_referenced_locally
  let valeurs = $state<Record<string, unknown>>(valeursInitiales(spec));
  let essaye = $state(false);
  let dialogue = $state<HTMLDialogElement>();
  let fini = false;

  const b = $derived(boutons(spec));
  const inactifs = $derived(controlesInactifs(spec, valeurs));
  const erreurs = $derived(valider(spec, valeurs));
  const nbErreurs = $derived(Object.keys(erreurs).length);
  const largeurLibelles = $derived(Math.min(600, Math.max(60, spec.label_width ?? 160)));
  const titreId = `titre-form-${Math.random().toString(36).slice(2, 8)}`;

  function repondre(v: Record<string, unknown> | null) {
    if (fini) return;
    fini = true;
    onrepondre(v);
  }

  async function cliquer(bouton: string, valide: boolean, annule: boolean) {
    if (annule) { repondre(null); return; }
    if (valide && nbErreurs > 0) {
      essaye = true;
      await tick();
      dialogue?.querySelector<HTMLElement>('[aria-invalid="true"], .erreur')?.closest('.controle')?.querySelector<HTMLElement>('input, select, textarea')?.focus();
      return;
    }
    repondre({ ...JSON.parse(JSON.stringify(valeurs)) as Record<string, unknown>, [CLE_BOUTON_FORMULAIRE]: bouton });
  }

  function boutonFormulaire(cle: string) {
    const c = trouver(spec.controls ?? [], cle);
    void cliquer(cle, !!c?.validate, false);
  }
  function trouver(l: NonNullable<FormSpecScript['controls']>, cle: string): (typeof l)[number] | undefined {
    for (const c of l) {
      if (c.type === 'button' && c.key === cle) return c;
      const sous = [...(c.controls ?? []), ...(c.tabs ?? []).flatMap((t) => t.controls ?? []),
        ...(c.type === 'columns' ? (c.columns ?? []).flatMap((x) => (Array.isArray(x) ? x : [])) : [])] as typeof l;
      const r = trouver(sous, cle);
      if (r) return r;
    }
    return undefined;
  }

  function clavier(e: KeyboardEvent) {
    if (e.key !== 'Enter' || e.defaultPrevented || e.shiftKey || e.altKey) return;
    const t = e.target as HTMLElement;
    if (t.closest('textarea, button, select[size], [role="tab"]')) return;
    const k = b.liste.findIndex((_, i) => i !== b.annuler);
    if (k < 0) return;
    e.preventDefault();
    void cliquer(b.liste[k], true, false);
  }
</script>

<dialog bind:this={dialogue} class="formulaire" aria-labelledby={titreId} style:width={spec.width ? `min(${Math.min(spec.width + 40, 4000)}px, calc(100vw - 32px))` : undefined}
  use:modale={{ peutFermer: () => true, fermer: () => repondre(null), ouvrirAuMontage: true }} onkeydown={clavier} data-testid="formulaire-script">
  <p class="script">📜 {nomScript}</p>
  <h2 id={titreId}>{spec.title || 'Formulaire du script'}</h2>
  {#if spec.message}<p class="message">{spec.message}</p>{/if}
  <div class="corps">
    <ControlesScript controles={spec.controls ?? []} bind:valeurs {inactifs} erreurs={essaye ? erreurs : {}} {largeurLibelles} onbouton={boutonFormulaire} />
  </div>
  {#if essaye && nbErreurs > 0}<p class="resume-erreurs" role="alert">{nbErreurs} champ(s) à corriger avant de continuer.</p>{/if}
  <footer>
    <button type="button" class="arret" onclick={onarreter} title="Arrêter le script">⏹ Arrêter le script</button>
    <span class="espace"></span>
    {#each b.liste as bt, i (bt)}
      <button type="button" class:principal={i === b.liste.findIndex((_, k) => k !== b.annuler)} onclick={() => void cliquer(bt, true, i === b.annuler)}
        data-bouton={bt}>{bt}</button>
    {/each}
  </footer>
</dialog>

<style>
  .formulaire { width: min(640px, calc(100vw - 32px)); max-height: calc(100vh - 32px); display: flex; flex-direction: column; gap: 8px; }
  .formulaire:not([open]) { display: none; }
  h2 { margin: 0; }
  .script { margin: 0; font-size: 12px; color: var(--cr-texte-discret); overflow-wrap: anywhere; }
  .message { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  .corps { flex: 1; min-height: 0; overflow-y: auto; padding: 2px 4px 2px 0; }
  .resume-erreurs { margin: 0; color: var(--cr-erreur); }
  footer { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; border-top: 1px solid var(--cr-bordure); padding-top: 10px; }
  .espace { flex: 1; }
  .arret { font-size: 12px; }
  button.principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); font-weight: 600; min-width: 80px; }
  button.principal:hover:not(:disabled) { background: var(--cr-accent); filter: brightness(1.1); }
</style>
