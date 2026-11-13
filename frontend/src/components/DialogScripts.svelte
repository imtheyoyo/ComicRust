<script lang="ts">
  // « Gérer les scripts… » (comme scripts_manager_window de la version egui) : Python trouvé ou
  // erreur lisible, avertissements de lecture des scripts, case « Exécuter les scripts
  // automatiques » (désactivée par défaut), Recharger, liste des scripts (nom, description, hooks,
  // actif), configuration (hook ConfigScript) ou réglages enregistrés (secrets masqués), et
  // journal des automatismes (événement scripts:auto).
  import {
    messageErreur, scriptReglagesEcrire, scriptReglagesLire, SECRET_MASQUE, type HookScript, type InfoScript,
  } from '../lib/api';
  import { modale } from '../lib/modale';
  import { scripts } from '../lib/scripts.svelte';

  const HOOKS: Record<HookScript, string> = {
    Books: 'Livres choisis', Library: 'Bibliothèque', Editor: 'Fenêtre Infos', ConfigScript: 'Configuration',
    BookOpened: 'Livre ouvert (auto)', BookClosed: 'Livre fermé (auto)', NewBooks: 'Nouveaux livres (auto)', Startup: 'Démarrage (auto)',
  };

  /** Réglages en cours de modification : script, valeurs lues, valeurs saisies. */
  let reglages = $state<{ script: InfoScript; lues: Record<string, unknown>; saisies: Record<string, unknown>; erreur: string; message: string; json: Record<string, string> } | null>(null);
  let dialogue = $state<HTMLDialogElement>();

  const etat = $derived(scripts.etat);

  async function ouvrirReglages(s: InfoScript) {
    try {
      const lues = await scriptReglagesLire(s.id);
      const json: Record<string, string> = {};
      for (const [k, v] of Object.entries(lues)) if (v !== null && typeof v === 'object') json[k] = JSON.stringify(v, null, 1);
      reglages = { script: s, lues, saisies: { ...lues }, erreur: '', message: '', json };
    } catch (e) {
      reglages = { script: s, lues: {}, saisies: {}, erreur: messageErreur(e), message: '', json: {} };
    }
  }

  async function enregistrerReglages() {
    if (!reglages) return;
    const r = reglages;
    const valeurs: Record<string, unknown> = { ...r.saisies };
    try {
      for (const [k, t] of Object.entries(r.json)) valeurs[k] = JSON.parse(t);
    } catch {
      r.erreur = 'Une valeur au format JSON est illisible.';
      return;
    }
    try {
      // Le backend rend les réglages enregistrés (secrets masqués) : ils remplacent la saisie.
      const lues = await scriptReglagesEcrire(r.script.id, valeurs);
      const json: Record<string, string> = {};
      for (const [k, v] of Object.entries(lues ?? {})) if (v !== null && typeof v === 'object') json[k] = JSON.stringify(v, null, 1);
      reglages = { script: r.script, lues: lues ?? {}, saisies: { ...(lues ?? {}) }, erreur: '', message: 'Réglages enregistrés.', json };
    } catch (e) {
      r.erreur = messageErreur(e);
    }
  }

  /** Point d'entrée ConfigScript du fichier de `s` (id dans `s.reglages`, opaque). */
  function entreeConfig(s: InfoScript): InfoScript | undefined {
    if (!s.reglages) return undefined;
    return scripts.script(s.reglages) ?? (s.reglages === s.id ? s : undefined);
  }

  function configurer(s: InfoScript) {
    const c = entreeConfig(s);
    // Un ConfigScript se lance même si le script est désactivé (comme la version egui).
    if (c) void scripts.lancer(c, 'ConfigScript', []);
  }

  const peutFermer = () => true;
  function fermer() {
    scripts.gestionOuverte = false;
    reglages = null;
  }
  const estSecret = (k: string) => reglages?.lues[k] === SECRET_MASQUE;
</script>

<dialog bind:this={dialogue} class="gestion" aria-labelledby="titre-gestion-scripts" use:modale={{ peutFermer, fermer, ouvrirAuMontage: true }} data-testid="dialogue-scripts">
  <h2 id="titre-gestion-scripts">Scripts</h2>

  <div class="onglets" role="tablist" aria-label="Onglets de la gestion des scripts">
    <button role="tab" id="onglet-scripts" aria-selected={scripts.ongletGestion === 'scripts'} aria-controls="panneau-gestion" class:actif={scripts.ongletGestion === 'scripts'}
      onclick={() => (scripts.ongletGestion = 'scripts')}>Scripts</button>
    <button role="tab" id="onglet-journal" aria-selected={scripts.ongletGestion === 'journal'} aria-controls="panneau-gestion" class:actif={scripts.ongletGestion === 'journal'}
      onclick={() => (scripts.ongletGestion = 'journal')} data-testid="onglet-journal-auto">Journal des automatismes{scripts.journalAuto.length ? ` (${scripts.journalAuto.length})` : ''}</button>
  </div>

  <div id="panneau-gestion" role="tabpanel" aria-labelledby={scripts.ongletGestion === 'scripts' ? 'onglet-scripts' : 'onglet-journal'} class="panneau">
    {#if scripts.ongletGestion === 'journal'}
      <p class="discret petit">Exécutions automatiques (livre ouvert, nouveaux livres…) depuis le lancement de ComicRust.</p>
      <div class="journal-auto" role="log" aria-label="Journal des automatismes" data-testid="journal-auto">
        {#each scripts.journalAuto as l (l.n)}
          <div><span class="discret">{l.heure}</span> <strong>{l.script}</strong> <span class="discret">({l.hook})</span> — {l.ligne}</div>
        {:else}
          <div class="discret">Rien pour l'instant.{etat && !etat.auto ? ' Les scripts automatiques sont désactivés.' : ''}</div>
        {/each}
      </div>
    {:else}
      {#if scripts.erreur}<p class="erreur" role="alert">{scripts.erreur}</p>{/if}
      {#if etat}
        <div class="python">
          {#if etat.python.erreur}
            <p class="erreur" role="alert" data-testid="python-erreur">⚠ {etat.python.erreur}</p>
          {:else}
            <p class="discret petit" data-testid="python-chemin">Python : {etat.python.chemin ?? 'trouvé'}</p>
          {/if}
          <button onclick={() => void scripts.recharger()} data-testid="recharger-scripts">⟳ Recharger</button>
        </div>
        {#if !etat.ecritureAutorisee}
          <p class="note" role="note">🔒 {etat.motifLectureSeule ?? 'Bibliothèque en lecture seule'} — les scripts peuvent montrer un aperçu mais rien ne sera appliqué.</p>
        {/if}
        <label class="case" title="Scripts lancés tout seuls (livre ouvert ou fermé, nouveaux livres, démarrage)">
          <input type="checkbox" checked={etat.auto} onchange={(e) => void scripts.basculerAuto(e.currentTarget.checked)} data-testid="scripts-auto" />
          Exécuter les scripts automatiques
        </label>
        {#each etat.avertissements as a, k (k)}<p class="avertissement">⚠ {a}</p>{/each}

        {#if etat.scripts.length === 0}
          <p class="discret">Aucun script : place des fichiers .py dans le dossier « scripts » de ComicRust{etat.dossier ? ` (${etat.dossier})` : ''}, puis « Recharger ».</p>
        {:else}
          <p class="discret petit" data-testid="dossier-scripts">Dossier : {etat.dossier}</p>
          <ul class="liste" data-testid="liste-scripts">
            {#each etat.scripts as s (s.id)}
              <li class:inactif={!s.actif}>
                <label class="case nom">
                  <input type="checkbox" checked={s.actif} onchange={(e) => void scripts.activer(s.id, e.currentTarget.checked)} aria-label="Activer {s.nom}" />
                  {#if s.image}<img src={s.image} alt="" class="icone" />{/if}
                  <strong>{s.nom}</strong>
                </label>
                {#if s.description}<p class="petit">{s.description}</p>{/if}
                <p class="hooks">
                  {#each s.hooks as h (h)}<span class="hook">{HOOKS[h] ?? h}</span>{/each}
                  <span class="discret petit">{s.fichier}{s.auteur ? ` — ${s.auteur}` : ''}</span>
                </p>
                <div class="actions">
                  {#if entreeConfig(s)}
                    <button class="mini" disabled={!!scripts.raisonIndisponible} title={scripts.raisonIndisponible ?? 'Fenêtre de configuration du script'}
                      onclick={() => configurer(s)} data-testid="configurer-{s.id}">⚙ Configurer…</button>
                  {/if}
                  <button class="mini" onclick={() => void ouvrirReglages(s)} title="Réglages enregistrés du fichier {s.fichier} (communs à ses points d'entrée)"
                    data-testid="reglages-{s.id}">Réglages…</button>
                </div>
                {#if reglages?.script.id === s.id}
                  {@const r = reglages}
                  <form class="reglages" onsubmit={(e) => { e.preventDefault(); void enregistrerReglages(); }} aria-label="Réglages de {s.nom}" data-testid="reglages-script">
                    {#if Object.keys(r.lues).length === 0 && !r.erreur}<p class="discret petit">Aucun réglage enregistré pour ce script.</p>{/if}
                    {#each Object.keys(r.lues) as k (k)}
                      {@const v = r.saisies[k]}
                      {@const idc = `reglage-${k.replace(/[^\w-]/g, '_')}`}
                      <label for={idc}>{k}</label>
                      {#if k in r.json}
                        <textarea id={idc} rows="3" bind:value={r.json[k]}></textarea>
                      {:else if typeof r.lues[k] === 'boolean'}
                        <input id={idc} type="checkbox" checked={!!v} onchange={(e) => (r.saisies[k] = e.currentTarget.checked)} />
                      {:else if typeof r.lues[k] === 'number'}
                        <input id={idc} type="number" value={Number(v)} oninput={(e) => (r.saisies[k] = Number(e.currentTarget.value))} />
                      {:else if estSecret(k)}
                        <input id={idc} type="password" autocomplete="new-password" value={String(v ?? '')} title="Secret défini : laisser « •••• » pour le garder"
                          onfocus={(e) => { if (e.currentTarget.value === SECRET_MASQUE) e.currentTarget.select(); }}
                          oninput={(e) => (r.saisies[k] = e.currentTarget.value)} data-secret="1" />
                      {:else}
                        <input id={idc} type="text" spellcheck="false" value={String(v ?? '')} oninput={(e) => (r.saisies[k] = e.currentTarget.value)} />
                      {/if}
                    {/each}
                    {#if r.erreur}<p class="erreur plein" role="alert">{r.erreur}</p>{/if}
                    {#if r.message}<p class="ok plein" role="status">{r.message}</p>{/if}
                    <div class="plein boutons">
                      <button type="button" onclick={() => (reglages = null)}>Fermer</button>
                      {#if Object.keys(r.lues).length}<button type="submit" class="principal">Enregistrer</button>{/if}
                    </div>
                  </form>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
      {:else if scripts.chargement}
        <p class="discret">Lecture des scripts…</p>
      {/if}
    {/if}
  </div>

  <div class="pied">
    <button class="principal" onclick={() => fermer()} data-testid="fermer-gestion">Fermer</button>
  </div>
</dialog>

<style>
  .gestion { width: min(760px, calc(100vw - 32px)); max-height: calc(100vh - 32px); display: flex; flex-direction: column; gap: 10px; }
  .gestion:not([open]) { display: none; }
  h2 { margin: 0; }
  p { margin: 0; }
  .onglets { display: flex; gap: 4px; border-bottom: 1px solid var(--cr-bordure); padding-bottom: 6px; }
  .onglets button.actif { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); }
  .panneau { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 8px; padding-right: 4px; }
  .python { display: flex; align-items: center; gap: 10px; justify-content: space-between; }
  .discret { color: var(--cr-texte-discret); }
  .petit { font-size: 12px; }
  .erreur { color: var(--cr-erreur); overflow-wrap: anywhere; }
  .ok { color: var(--cr-accent); }
  .avertissement { color: var(--cr-accent); font-size: 12px; overflow-wrap: anywhere; }
  .note { padding: 6px 8px; border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); font-size: 13px; }
  .case { display: inline-flex; align-items: center; gap: 6px; }
  .case input { accent-color: var(--cr-accent); }
  .liste { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .liste li { border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); padding: 8px 10px; display: flex; flex-direction: column; gap: 4px; }
  .liste li.inactif { opacity: 0.7; }
  .nom strong { overflow-wrap: anywhere; }
  .icone { width: 20px; height: 20px; object-fit: contain; }
  .hooks { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; }
  .hook { font-size: 11px; padding: 1px 7px; border: 1px solid var(--cr-accent); border-radius: 999px; }
  .actions { display: flex; gap: 6px; }
  .mini { padding: 2px 8px; font-size: 12px; }
  .journal-auto {
    min-height: 160px; max-height: 360px; overflow-y: auto; padding: 6px 8px; font-size: 12px; font-family: ui-monospace, Consolas, monospace;
    background: var(--cr-fond); border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); white-space: pre-wrap; overflow-wrap: anywhere;
  }
  .reglages { display: grid; grid-template-columns: minmax(100px, max-content) minmax(0, 1fr); gap: 6px 10px; align-items: center; padding: 8px; background: var(--cr-fond); border-radius: var(--cr-rayon); }
  .reglages label { color: var(--cr-texte-discret); overflow-wrap: anywhere; }
  .reglages input[type='password'], .reglages input[type='number'], .reglages textarea {
    font: inherit; color: var(--cr-texte); background: var(--cr-fond-panneau); border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); padding: 5px 8px;
  }
  .reglages input[type='checkbox'] { justify-self: start; accent-color: var(--cr-accent); }
  .plein { grid-column: 1 / -1; }
  .boutons { display: flex; justify-content: flex-end; gap: 8px; }
  .pied { display: flex; justify-content: flex-end; }
  button.principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); font-weight: 600; }
  button.principal:hover:not(:disabled) { background: var(--cr-accent); filter: brightness(1.1); }
</style>
