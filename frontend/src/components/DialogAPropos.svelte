<script lang="ts">
  // « À propos / Aide » (lot L7, écart 10) : version, état de PDFium, dossier data, lien du dépôt,
  // raccourcis clavier et « Copier les informations de diagnostic » (sans secret : le texte est fait
  // par le backend, docs/tauri/ipc-reglages-aide.md). F1 ou l'entrée de la barre latérale l'ouvre.
  import { infosDiagnostic, majOuvrirPage, messageErreur, texteDiagnostic, type InfosDiagnostic } from '../lib/api';
  import { lecteurAffiche } from '../lib/fenetre';
  import { fermerModale, modale } from '../lib/modale';
  import { RACCOURCIS } from '../lib/raccourcis';

  let dialogue = $state<HTMLDialogElement>();
  let infos = $state<InfosDiagnostic | null>(null);
  let erreur = $state('');
  let copie = $state(false);
  let texte = $state('');
  let zoneTexte = $state<HTMLTextAreaElement>();

  export async function ouvrir() {
    erreur = '';
    texte = '';
    copie = false;
    dialogue?.showModal();
    try {
      infos = await infosDiagnostic();
    } catch (e) {
      erreur = messageErreur(e);
    }
  }

  /** F1 : ouvre l'aide, sauf si une autre fenêtre est déjà ouverte. */
  function clavier(e: KeyboardEvent) {
    if (e.key !== 'F1' || e.ctrlKey || e.altKey || e.shiftKey || e.metaKey) return;
    if (lecteurAffiche() || e.defaultPrevented) return; // le lecteur a sa propre aide F1
    e.preventDefault();
    if (dialogue?.open || document.querySelector('dialog[open]')) return;
    void ouvrir();
  }

  async function copier() {
    erreur = '';
    texte = '';
    try {
      const t = await texteDiagnostic();
      try {
        await navigator.clipboard.writeText(t);
        copie = true;
        setTimeout(() => (copie = false), 2000);
      } catch {
        // Presse-papiers refusé : le texte est montré et sélectionné (Ctrl+C).
        texte = t;
        setTimeout(() => { zoneTexte?.focus(); zoneTexte?.select(); }, 0);
      }
    } catch (e) {
      erreur = messageErreur(e);
    }
  }

  const ouvrirDepot = () => void majOuvrirPage(infos?.depot ?? 'https://github.com/imtheyoyo/ComicRust/').catch((e) => (erreur = messageErreur(e)));
</script>

<svelte:window onkeydown={clavier} />

<dialog bind:this={dialogue} use:modale aria-labelledby="titre-apropos" data-testid="dialogue-apropos">
  <h2 id="titre-apropos">À propos de ComicRust</h2>
  {#if erreur}<p class="erreur" role="alert" data-testid="erreur-apropos">{erreur}</p>{/if}
  {#if infos}
    <p><strong data-testid="apropos-version">ComicRust {infos.version}</strong> — lecteur et gestionnaire de BD en Rust, inspiré de ComicRack.</p>
    <p><a href={infos.depot} onclick={(e) => { e.preventDefault(); ouvrirDepot(); }} data-testid="apropos-depot">{infos.depot}</a></p>
    <dl>
      <dt>Système</dt><dd>{infos.systeme}</dd>
      <dt>PDF (PDFium)</dt>
      <dd data-testid="apropos-pdfium" class:alerte={!infos.pdfium.charge}>
        {#if infos.pdfium.charge}Chargé ✔{#if infos.pdfium.chemin} — <code>{infos.pdfium.chemin}</code>{:else} (bibliothèque du système){/if}
        {:else}Non chargé : {infos.pdfium.erreur}{#if infos.pdfium.chemin} (<code>{infos.pdfium.chemin}</code>){/if} — les PDF sont indisponibles, le reste fonctionne.{/if}
      </dd>
      <dt>Données</dt><dd data-testid="apropos-data"><code>{infos.dossierData}</code></dd>
      <dt>Bibliothèque</dt>
      <dd>{infos.mode === 'comicrack' ? 'Liée à ComicRack' : infos.mode === 'autonome' ? 'Autonome' : 'Aucune ouverte'}{infos.nbLivres !== undefined ? ` — ${infos.nbLivres.toLocaleString('fr-FR')} livres` : ''}</dd>
    </dl>
  {/if}
  <div class="ligne">
    <button type="button" onclick={() => void copier()} data-testid="apropos-copier">{copie ? 'Copié ✓' : 'Copier les informations de diagnostic'}</button>
    <span class="discret petit">Version, système, mode, chemins : aucun mot de passe ni identifiant.</span>
  </div>
  {#if texte}
    <textarea bind:this={zoneTexte} readonly rows="8" aria-label="Informations de diagnostic" data-testid="apropos-texte">{texte}</textarea>
    <p class="alerte" role="alert">Copie impossible : le texte est sélectionné, utilisez Ctrl+C.</p>
  {/if}
  <h3>Raccourcis clavier</h3>
  <div class="raccourcis" data-testid="liste-raccourcis">
    {#each RACCOURCIS as g (g.titre)}
      <section>
        <h4>{g.titre}</h4>
        <dl class="touches">
          {#each g.raccourcis as r (r.touches)}
            <dt><kbd>{r.touches}</kbd></dt><dd>{r.action}</dd>
          {/each}
        </dl>
      </section>
    {/each}
  </div>
  <div class="pied">
    <button type="button" class="principal" onclick={() => fermerModale(dialogue)} data-testid="apropos-fermer">Fermer</button>
  </div>
</dialog>

<style>
  dialog { max-height: calc(100vh - 32px); overflow-y: auto; }
  h3 { margin: 16px 0 6px; font-size: 15px; }
  h4 { margin: 10px 0 4px; font-size: 13px; color: var(--cr-texte-discret); }
  p { margin: 6px 0; }
  a { color: var(--cr-accent); overflow-wrap: anywhere; }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 4px 12px; margin: 8px 0; }
  dt { color: var(--cr-texte-discret); }
  dd { margin: 0; overflow-wrap: anywhere; }
  .touches dt { color: var(--cr-texte); }
  kbd { font-family: ui-monospace, Consolas, monospace; font-size: 12px; background: var(--cr-fond-survol); border: 1px solid var(--cr-bordure); border-radius: 4px; padding: 1px 5px; white-space: normal; }
  code { font-family: ui-monospace, Consolas, monospace; background: var(--cr-fond-survol); padding: 1px 5px; border-radius: 3px; }
  .ligne { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin-top: 8px; }
  .discret { color: var(--cr-texte-discret); }
  .petit { font-size: 12px; }
  .erreur, .alerte { color: var(--cr-erreur); }
  textarea { width: 100%; margin-top: 8px; font-family: ui-monospace, Consolas, monospace; font-size: 11px; resize: vertical; }
  .pied {
    display: flex; justify-content: flex-end; margin: 12px -20px -20px; padding: 10px 20px; position: sticky; bottom: -20px;
    background: var(--cr-fond-panneau); border-top: 1px solid var(--cr-bordure);
  }
  .pied .principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); }
</style>
