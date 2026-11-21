<script lang="ts">
  // Fenêtres des livres manquants (lot L3, docs/tauri/ipc-manquants.md), montées une fois dans
  // l'application : « Fichier introuvable » (ouverture d'un livre), « Retrouver le fichier » (chemin
  // saisi), « Chercher dans un dossier » (avancement, Arrêter), confirmation des propositions,
  // « Retirer de la bibliothèque » et message d'information. L'état est dans `manquants`.
  import { manquants, selecteurs } from '../lib/manquants.svelte';
  import { modale } from '../lib/modale';

  const m = manquants;
  const ferme = (champ: 'introuvable' | 'retrouver' | 'dossier' | 'relocalisation' | 'retrait') => () => {
    m[champ] = null;
  };

  async function parcourirFichier() {
    const f = m.retrouver;
    const c = f && (await selecteurs.fichier?.(f.chemin));
    if (f && c) f.chemin = c;
  }
  async function parcourirDossier() {
    const f = m.dossier;
    const c = f && (await selecteurs.dossier?.(f.dossier));
    if (f && c) f.dossier = c;
  }

  const nbCoches = $derived(m.relocalisation?.propositions.filter((p) => p.coche).length ?? 0);
  const av = $derived(m.etat?.recherche);
  const bilanRetrait = $derived(m.retrait?.prep?.bilan);
  const retraitPret = $derived(!!m.retrait?.prep && !m.retrait.occupe && (!m.retrait.prep.confirmationRequise || m.retrait.confirme));
</script>

{#if m.introuvable}
  {@const s = m.introuvable.sonde}
  <dialog class="manquants" aria-labelledby="titre-introuvable" use:modale={{ fermer: ferme('introuvable'), peutFermer: () => !m.introuvable?.attente, ouvrirAuMontage: true }} data-testid="dialogue-introuvable">
    {#if s.etat === 'volumeInjoignable'}
      <h2 id="titre-introuvable">Disque inaccessible</h2>
      <p><strong>{s.titre}</strong></p>
      <p class="chemin">{s.fichier}</p>
      <p>Le volume <strong>{s.racine ?? ''}</strong> ne répond pas (disque ou partage réseau débranché ?). Le livre n'est pas marqué manquant : rebranchez le disque puis réessayez.</p>
      <div class="pied">
        <button onclick={() => m.fermerIntrouvable()} disabled={m.introuvable.attente}>Annuler</button>
        <button class="principal" onclick={() => void m.reessayer()} disabled={m.introuvable.attente} data-testid="introuvable-reessayer">{m.introuvable.attente ? 'Vérification…' : 'Réessayer'}</button>
      </div>
    {:else}
      <h2 id="titre-introuvable">Fichier introuvable</h2>
      <p><strong>{s.titre}</strong></p>
      <p class="chemin">{s.fichier}</p>
      <p>Ce fichier a été déplacé, renommé ou supprimé. Le livre reste dans la bibliothèque, marqué « manquant ».</p>
      {#if s.avertissement}<p class="discret">{s.avertissement}</p>{/if}
      {#if m.raison}<p class="discret" data-testid="introuvable-lecture-seule">{m.raison}</p>{/if}
      <div class="pied">
        <button onclick={() => m.fermerIntrouvable()} data-testid="introuvable-fermer">Fermer</button>
        <button onclick={() => void m.demanderRetrait([s.id])} disabled={!!m.raison} data-testid="introuvable-retirer">Retirer de la bibliothèque…</button>
        <button onclick={() => m.demanderDossier([s.id])} disabled={!!m.raison} data-testid="introuvable-dossier">Chercher dans un dossier…</button>
        <button class="principal" onclick={() => m.demanderRetrouver(s.id, s.titre)} disabled={!!m.raison} data-testid="introuvable-retrouver">Retrouver le fichier…</button>
      </div>
    {/if}
  </dialog>
{/if}

{#if m.retrouver}
  {@const f = m.retrouver}
  <dialog class="manquants" aria-labelledby="titre-retrouver" use:modale={{ fermer: ferme('retrouver'), peutFermer: () => !f.occupe, ouvrirAuMontage: true }} data-testid="dialogue-retrouver">
    <h2 id="titre-retrouver">Retrouver le fichier</h2>
    <p><strong>{f.legende}</strong></p>
    <form onsubmit={(e) => { e.preventDefault(); void m.validerRetrouver(); }}>
      <label for="chemin-retrouve">Nouveau chemin du fichier</label>
      <div class="ligne">
        <input id="chemin-retrouve" type="text" bind:value={f.chemin} spellcheck="false" autocomplete="off" placeholder="D:\BD\Série\Tome 01.cbz" data-testid="chemin-retrouve" />
        {#if selecteurs.fichier}<button type="button" onclick={() => void parcourirFichier()}>Parcourir…</button>{/if}
      </div>
      {#if f.erreur}<p class="erreur" role="alert">{f.erreur}</p>{/if}
      <div class="pied">
        <button type="button" onclick={ferme('retrouver')} disabled={f.occupe}>Annuler</button>
        <button type="submit" class="principal" disabled={f.occupe || !f.chemin.trim()} data-testid="retrouver-valider">{f.occupe ? 'Examen…' : 'Continuer'}</button>
      </div>
    </form>
  </dialog>
{/if}

{#if m.dossier}
  {@const f = m.dossier}
  <dialog class="manquants" aria-labelledby="titre-dossier" use:modale={{ fermer: ferme('dossier'), peutFermer: () => !f.occupe, ouvrirAuMontage: true }} data-testid="dialogue-dossier">
    <h2 id="titre-dossier">Chercher dans un dossier</h2>
    <p>{f.ids.length === 1 ? 'Un livre manquant' : `${f.ids.length.toLocaleString('fr-FR')} livres manquants`} : les fichiers du dossier (sous-dossiers compris) sont comparés par leur nom.</p>
    <form onsubmit={(e) => { e.preventDefault(); void m.validerDossier(); }}>
      <label for="dossier-recherche">Dossier où chercher</label>
      <div class="ligne">
        <input id="dossier-recherche" type="text" bind:value={f.dossier} disabled={f.occupe} spellcheck="false" autocomplete="off" placeholder="D:\BD" data-testid="dossier-recherche" />
        {#if selecteurs.dossier}<button type="button" onclick={() => void parcourirDossier()} disabled={f.occupe}>Parcourir…</button>{/if}
      </div>
      {#if f.occupe}
        <p aria-live="polite" data-testid="recherche-avancement">
          {#if av}{av.comparaison ? 'Comparaison des noms…' : `${av.fichiers.toLocaleString('fr-FR')} fichiers examinés…`}{:else}Démarrage…{/if}
        </p>
      {/if}
      {#if f.erreur}<p class="erreur" role="alert">{f.erreur}</p>{/if}
      <div class="pied">
        {#if f.occupe}
          <button type="button" onclick={() => m.arreterTravail()} data-testid="recherche-arreter">Arrêter</button>
        {:else}
          <button type="button" onclick={ferme('dossier')}>Annuler</button>
          <button type="submit" class="principal" disabled={!f.dossier.trim()} data-testid="dossier-chercher">Chercher</button>
        {/if}
      </div>
    </form>
  </dialog>
{/if}

{#if m.relocalisation}
  {@const f = m.relocalisation}
  <dialog class="manquants large" aria-labelledby="titre-reloc" use:modale={{ fermer: ferme('relocalisation'), peutFermer: () => !f.occupe, ouvrirAuMontage: true }} data-testid="dialogue-relocalisation">
    <h2 id="titre-reloc">Mettre à jour {f.propositions.length === 1 ? 'la fiche' : 'les fiches'}</h2>
    <p class="discret">Source : {f.source}{f.nonRetrouves ? ` · ${f.nonRetrouves} livre${f.nonRetrouves > 1 ? 's' : ''} non retrouvé${f.nonRetrouves > 1 ? 's' : ''}` : ''}</p>
    <div class="liste">
      {#each f.propositions as p (p.id)}
        <label class="proposition" class:douteuse={!p.sure}>
          <input type="checkbox" bind:checked={p.coche} disabled={f.occupe} />
          <span class="texte">
            <strong>{p.legende}</strong>
            <span class="chemin" title={p.ancien}>avant : {p.ancien}</span>
            <span class="chemin" title={p.nouveau}>après : {p.nouveau}</span>
            <span class="discret">{p.correspondance}{p.autres ? ` · ${p.autres} autre${p.autres > 1 ? 's' : ''} fichier${p.autres > 1 ? 's' : ''} possible${p.autres > 1 ? 's' : ''}` : ''}</span>
            {#if p.doublon}<span class="erreur">⚠ {p.doublon}</span>{/if}
          </span>
        </label>
      {/each}
    </div>
    {#if f.erreur}<p class="erreur" role="alert">{f.erreur}</p>{/if}
    <div class="pied">
      <button onclick={ferme('relocalisation')} disabled={f.occupe}>Annuler</button>
      <button class="principal" onclick={() => void m.validerRelocalisation()} disabled={f.occupe || nbCoches === 0} data-testid="reloc-valider"
        >Mettre à jour {nbCoches} fiche{nbCoches > 1 ? 's' : ''}</button>
    </div>
  </dialog>
{/if}

{#if m.retrait}
  {@const f = m.retrait}
  <dialog class="manquants" aria-labelledby="titre-retrait" use:modale={{ fermer: ferme('retrait'), peutFermer: () => !f.occupe, ouvrirAuMontage: true }} data-testid="dialogue-retrait">
    <h2 id="titre-retrait">Retirer de la bibliothèque</h2>
    {#if !f.prep}
      <p aria-live="polite">Vérification des fichiers…</p>
      <div class="pied"><button onclick={ferme('retrait')}>Annuler</button></div>
    {:else}
      <p>
        {f.ids.length === 1 ? 'Ce livre sera retiré' : `Ces ${f.ids.length.toLocaleString('fr-FR')} livres seront retirés`} de la bibliothèque.
        <strong>Les fichiers ne sont pas supprimés.</strong>
      </p>
      <ul class="legendes">
        {#each f.prep.legendes as l (l)}<li>{l}</li>{/each}
        {#if f.prep.autres}<li class="discret">… et {f.prep.autres.toLocaleString('fr-FR')} autre{f.prep.autres > 1 ? 's' : ''}</li>{/if}
      </ul>
      {#if bilanRetrait}<p class="discret" data-testid="retrait-bilan">{bilanRetrait.resume}</p>
        {#if bilanRetrait.avertissement}<p class="erreur">{bilanRetrait.avertissement}</p>{/if}
      {/if}
      {#if f.prep.comicrack}
        <p class="erreur">Bibliothèque ComicRack : fermez ComicRack avant de continuer, ses fiches seront modifiées dans la base partagée.</p>
      {/if}
      {#if f.prep.confirmationRequise}
        <label class="confirme"><input type="checkbox" bind:checked={f.confirme} data-testid="retrait-confirme" />
          Je confirme le retrait de {f.ids.length.toLocaleString('fr-FR')} livres, dont des fichiers encore présents.</label>
      {/if}
      {#if f.erreur}<p class="erreur" role="alert">{f.erreur}</p>{/if}
      <div class="pied">
        <button onclick={ferme('retrait')} disabled={f.occupe}>Annuler</button>
        <button class="principal danger" onclick={() => void m.validerRetrait()} disabled={!retraitPret} data-testid="retrait-valider">{f.occupe ? 'Retrait…' : 'Retirer'}</button>
      </div>
    {/if}
  </dialog>
{/if}

{#if m.info}
  <dialog class="manquants" aria-labelledby="titre-info-manquants" use:modale={{ fermer: () => (m.info = ''), ouvrirAuMontage: true }} data-testid="dialogue-info-manquants">
    <h2 id="titre-info-manquants">Livres manquants</h2>
    <p class="message-info">{m.info}</p>
    <div class="pied"><button class="principal" onclick={() => (m.info = '')} data-testid="info-manquants-fermer">Fermer</button></div>
  </dialog>
{/if}

<style>
  .manquants { width: min(560px, calc(100vw - 32px)); }
  .manquants.large { width: min(780px, calc(100vw - 32px)); }
  .chemin { display: block; font-family: ui-monospace, Consolas, monospace; font-size: 12px; overflow-wrap: anywhere; color: var(--cr-texte-discret); }
  .discret { color: var(--cr-texte-discret); font-size: 12px; }
  .erreur { color: var(--cr-erreur); overflow-wrap: anywhere; }
  .message-info { white-space: pre-wrap; overflow-wrap: anywhere; }
  label { display: block; margin-bottom: 4px; color: var(--cr-texte-discret); }
  .ligne { display: flex; gap: 6px; }
  .ligne input { flex: 1; min-width: 0; font-family: ui-monospace, Consolas, monospace; }
  .liste { max-height: 48vh; overflow: auto; border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); background: var(--cr-fond); }
  .proposition { display: flex; gap: 8px; align-items: flex-start; padding: 6px 8px; margin: 0; color: var(--cr-texte); border-bottom: 1px solid var(--cr-bordure); }
  .proposition.douteuse { background: var(--cr-fond-survol); }
  .texte { display: grid; gap: 2px; min-width: 0; }
  .legendes { margin: 4px 0; padding-left: 18px; max-height: 30vh; overflow: auto; }
  .confirme { color: var(--cr-texte); display: flex; gap: 6px; align-items: flex-start; margin-top: 8px; }
  .pied { display: flex; justify-content: flex-end; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
  button.principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); font-weight: 600; }
  button.principal.danger { background: var(--cr-erreur); border-color: var(--cr-erreur); }
  button.principal:hover:not(:disabled) { filter: brightness(1.1); }
</style>
