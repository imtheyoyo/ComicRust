<script lang="ts">
  // Fenêtre d'exécution d'un script (comme script_run_window de la version egui) : progression
  // (fraction, texte, détail, image), journal défilant coloré par genre, Arrêter ; questions
  // (ask : Oui / Non, message : OK), formulaires et listes de choix dans leurs propres fenêtres ;
  // à la fin, aperçu des modifications (cases à cocher, conflits) puis Appliquer / Annuler et
  // bilan. En lecture seule, l'aperçu reste visible mais Appliquer est désactivé avec le motif.
  // Les fenêtres de question / formulaire / choix sont des <dialog> frères (pas enfants) : Échap
  // n'atteint ainsi que la fenêtre du dessus (voir lib/modale.ts).
  import { tick } from 'svelte';
  import type { GenreModif } from '../lib/api';
  import { modale } from '../lib/modale';
  import { scripts, type ExecutionScript } from '../lib/scripts.svelte';
  import ChoixScript from './ChoixScript.svelte';
  import FormulaireScript from './FormulaireScript.svelte';

  interface Props { execution: ExecutionScript }
  let { execution: e }: Props = $props();

  let journalEl = $state<HTMLDivElement>();
  let suivreJournal = true;
  /** Lignes de l'aperçu cochées (clés). */
  let cochees = $state<Set<string>>(new Set());
  let apercuVu: unknown = null;
  let copie = $state('');

  const GENRES: Record<GenreModif, string> = { fiche: 'Fiche', renommage: 'Renommage', comicinfo: 'ComicInfo' };
  const HOOKS: Record<string, string> = {
    Books: 'sur les livres choisis', Library: 'sur toute la bibliothèque', Editor: 'depuis la fenêtre Infos', ConfigScript: 'configuration',
  };

  // Aperçu reçu : tout est coché sauf les lignes en conflit (à cocher en connaissance de cause).
  $effect(() => {
    const a = e.apercu;
    if (a && a !== apercuVu) {
      apercuVu = a;
      cochees = new Set(a.lignes.filter((l) => !l.conflit).map((l) => l.cle));
    }
  });

  // Journal : défile vers le bas tant que l'utilisateur n'est pas remonté.
  $effect(() => {
    void e.journal.length;
    if (suivreJournal) void tick().then(() => { if (journalEl) journalEl.scrollTop = journalEl.scrollHeight; });
  });
  function defilement() {
    if (journalEl) suivreJournal = journalEl.scrollTop + journalEl.clientHeight >= journalEl.scrollHeight - 8;
  }

  const pourcent = $derived(typeof e.progression.fraction === 'number' ? Math.round(e.progression.fraction * 100) : null);
  const nb = $derived.by(() => {
    const l = e.apercu?.lignes ?? [];
    return { fiche: l.filter((x) => x.genre === 'fiche').length, renommage: l.filter((x) => x.genre === 'renommage').length, comicinfo: l.filter((x) => x.genre === 'comicinfo').length };
  });
  const peutFermer = () => !e.enCours && !e.application && !e.question && !e.formulaire && !e.choix;
  const etatTexte = $derived.by(() => {
    if (e.erreur) return { texte: `✖ Lancement impossible : ${e.erreur}`, classe: 'erreur' };
    if (!e.fin) return null;
    if (e.fin.etat === 'termine') return { texte: `✔ Terminé en ${e.duree.toFixed(1).replace('.', ',')} s.`, classe: 'ok' };
    if (e.fin.etat === 'annule') return { texte: '⏹ Script arrêté.', classe: 'attention' };
    return { texte: `✖ Erreur : ${e.fin.erreur ?? 'le script a échoué'}`, classe: 'erreur' };
  });

  function basculer(cle: string, oui: boolean) {
    const s = new Set(cochees);
    if (oui) s.add(cle);
    else s.delete(cle);
    cochees = s;
  }

  async function copier(texte: string, quoi: string) {
    try {
      await navigator.clipboard.writeText(texte);
      copie = `${quoi} copié.`;
    } catch {
      copie = 'Copie impossible.';
    }
  }

  const resumeBilan = $derived.by(() => {
    const b = e.bilan;
    if (!b) return '';
    const p = [`${b.fiches} fiche(s)`, `${b.renommages} renommage(s)`, `${b.comicinfo} ComicInfo.xml écrit(s)`];
    if (b.erreurs.length) p.push(`${b.erreurs.length} erreur(s)`);
    return p.join(', ') + (b.arrete ? ' — arrêté en cours de route' : '');
  });
</script>

<dialog class="execution" aria-labelledby="titre-execution" use:modale={{ peutFermer, fermer: () => scripts.fermerExecution(), ouvrirAuMontage: true }}
  data-testid="fenetre-script">
  <h2 id="titre-execution">📜 {e.script.nom}</h2>
  <p class="discret petit">{HOOKS[e.hook] ?? e.hook}{e.hook === 'Books' ? ` (${e.ids.length} livre${e.ids.length > 1 ? 's' : ''})` : ''} — {e.script.fichier}</p>

  {#if e.enCours || (!e.apercu && !e.bilan)}
    <div class="progression">
      {#if e.progression.image}<img class="image" src={e.progression.image} alt="" draggable="false" />{/if}
      <div class="textes">
        <div class="barre-progression" class:indetermine={pourcent === null && e.enCours} role="progressbar" aria-label="Progression du script"
          aria-valuemin="0" aria-valuemax="100" aria-valuenow={pourcent ?? undefined}>
          <div style:width="{pourcent ?? (e.enCours ? 30 : 100)}%"></div>
        </div>
        <span aria-live="polite" data-testid="progression-texte">{e.progression.texte ?? (e.enCours ? 'En cours…' : '')}{pourcent !== null && e.enCours ? ` (${pourcent} %)` : ''}</span>
        {#if e.progression.detail}<span class="discret">{e.progression.detail}</span>{/if}
      </div>
    </div>
  {/if}

  <!-- svelte-ignore a11y_no_noninteractive_tabindex (journal défilable au clavier) -->
  <div class="journal" bind:this={journalEl} onscroll={defilement} role="log" aria-label="Journal du script" tabindex="0" data-testid="journal-script">
    {#each e.journal as l (l.n)}<div class="genre-{l.genre}">{l.texte}</div>{:else}<div class="discret">Journal vide pour l'instant.</div>{/each}
  </div>

  {#if etatTexte}<p class="etat {etatTexte.classe}" role="status" data-testid="etat-script">{etatTexte.texte}</p>{/if}

  {#if e.fin && !e.editeur && !e.apercu && !e.bilan}
    {#if e.chargementApercu}
      <p>Préparation de l'aperçu…</p>
    {:else if e.fin.nbModifs === 0}
      <p class="discret">Aucune modification demandée.</p>
    {:else}
      <p>{e.fin.nbModifs} modification(s) préparée(s) avant {e.fin.etat === 'annule' ? "l'arrêt" : "l'erreur"}.</p>
      <button onclick={() => void scripts.chargerApercu()} data-testid="voir-apercu">Voir l'aperçu de ces modifications</button>
    {/if}
  {/if}

  {#if e.apercu && !e.bilan && !e.abandonne}
    {@const a = e.apercu}
    <section class="apercu" aria-label="Aperçu des modifications" data-testid="apercu-script">
      <h3>Aperçu des modifications</h3>
      <p>{[nb.fiche ? `${nb.fiche} champ(s) de fiche` : '', nb.renommage ? `${nb.renommage} renommage(s)` : '', nb.comicinfo ? `${nb.comicinfo} ComicInfo.xml` : ''].filter(Boolean).join(' · ') || 'Aucune modification.'}</p>
      {#each a.conflits as m, k (k)}<p class="conflit" role="alert">⚠ {m}</p>{/each}
      {#if a.lectureSeule}<p class="lecture-seule" role="note" data-testid="motif-lecture-seule">🔒 Lecture seule : {a.motif ?? 'la bibliothèque ne peut pas être modifiée.'}</p>{/if}
      <div class="ligne-outils">
        <button class="mini" onclick={() => (cochees = new Set(a.lignes.map((l) => l.cle)))} data-testid="tout-cocher">Tout cocher</button>
        <button class="mini" onclick={() => (cochees = new Set())} data-testid="tout-decocher">Tout décocher</button>
        <span class="discret petit">{cochees.size} / {a.lignes.length} cochée(s)</span>
      </div>
      <div class="table-apercu">
        <table>
          <thead><tr><th><span class="sr">Appliquer</span></th><th>Livre</th><th>Champ</th><th>Avant</th><th></th><th>Après</th><th>Genre</th></tr></thead>
          <tbody>
            {#each a.lignes as l (l.cle)}
              <tr class:conflit={!!l.conflit} class:decoche={!cochees.has(l.cle)} title={l.conflit ? `Conflit : ${l.conflit}` : undefined}>
                <td><input type="checkbox" checked={cochees.has(l.cle)} onchange={(ev) => basculer(l.cle, ev.currentTarget.checked)} aria-label="Appliquer {l.champ} de {l.titre}" /></td>
                <td class="titre">{l.titre}</td>
                <td>{l.champ}</td>
                <td class="valeur avant">{l.avant}</td>
                <td aria-hidden="true">→</td>
                <td class="valeur">{l.apres}{#if l.conflit}<span class="conflit-texte">⚠ {l.conflit}</span>{/if}</td>
                <td><span class="genre g-{l.genre}">{GENRES[l.genre] ?? l.genre}</span></td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </section>
  {/if}

  {#if e.abandonne}<p data-testid="abandon">Modifications abandonnées : rien n'a été appliqué.</p>{/if}
  {#if e.bilan}
    <section class="bilan" data-testid="bilan-script">
      <p><strong>Bilan : {resumeBilan}.</strong></p>
      {#each e.bilan.erreurs as m, k (k)}<p class="erreur">• {m}</p>{/each}
    </section>
  {/if}

  <footer>
    {#if e.journal.length}<button class="mini" onclick={() => void copier(e.journal.map((l) => l.texte).join('\n'), 'Journal')}>📋 Copier le journal</button>{/if}
    {#if copie}<span class="discret petit" role="status">{copie}</span>{/if}
    <span class="espace"></span>
    {#if e.enCours}
      {#if e.arretDemande}<span>Arrêt en cours…</span>{:else}<button onclick={() => void scripts.arreterScript()} title="Demande au script de s'arrêter (arrêt forcé après 3 s)" data-testid="arreter-script">⏹ Arrêter</button>{/if}
    {:else if e.apercu && !e.bilan && !e.abandonne}
      <button onclick={() => scripts.abandonner()} title="Abandonner ces modifications" data-testid="annuler-apercu">Annuler</button>
      <button class="principal" disabled={e.apercu.lectureSeule || cochees.size === 0 || e.application}
        title={e.apercu.lectureSeule ? `Lecture seule : ${e.apercu.motif ?? ''}` : cochees.size === 0 ? 'Rien de coché' : 'Appliquer les modifications cochées'}
        onclick={() => void scripts.appliquer([...cochees])} data-testid="appliquer-script">{e.application ? 'Application…' : `Appliquer (${cochees.size})`}</button>
    {:else}
      <button class="principal" onclick={() => scripts.fermerExecution()} data-testid="fermer-script">Fermer</button>
    {/if}
  </footer>
</dialog>

{#if e.question}
  {@const qu = e.question}
  {#key qu.reqId}
    <dialog class="question" aria-labelledby="titre-question" aria-describedby="texte-question"
      use:modale={{ peutFermer: () => true, fermer: () => void scripts.repondre('question', null), ouvrirAuMontage: true }} data-testid="question-script">
      <p class="discret petit">📜 {e.script.nom}</p>
      <h2 id="titre-question">{qu.titre || (qu.genre === 'ask' ? 'Question du script' : 'Message du script')}</h2>
      <p id="texte-question" class="texte-question">{qu.texte}</p>
      <footer>
        <button class="mini" onclick={() => void scripts.arreterScript()}>⏹ Arrêter le script</button>
        <span class="espace"></span>
        {#if qu.genre === 'ask'}
          <!-- svelte-ignore a11y_autofocus -->
          <button class="principal" autofocus onclick={() => void scripts.repondre('question', true)} data-testid="question-oui">Oui</button>
          <button onclick={() => void scripts.repondre('question', false)} data-testid="question-non">Non</button>
        {:else}
          <!-- svelte-ignore a11y_autofocus -->
          <button class="principal" autofocus onclick={() => void scripts.repondre('question', null)} data-testid="question-ok">OK</button>
        {/if}
      </footer>
    </dialog>
  {/key}
{/if}

{#if e.formulaire}
  {#key e.formulaire.reqId}
    <FormulaireScript spec={e.formulaire.spec} nomScript={e.script.nom} onrepondre={(v) => void scripts.repondre('formulaire', v)}
      onarreter={() => void scripts.arreterScript()} />
  {/key}
{/if}

{#if e.choix}
  {#key e.choix.reqId}
    <ChoixScript spec={e.choix.spec} vignettes={e.choix.vignettes} nomScript={e.script.nom} onvoulues={(r) => scripts.vignettesVoulues(r)}
      onrepondre={(v) => void scripts.repondre('choix', v)} onarreter={() => void scripts.arreterScript()} />
  {/key}
{/if}

<style>
  .execution { width: min(860px, calc(100vw - 32px)); max-height: calc(100vh - 32px); display: flex; flex-direction: column; gap: 8px; }
  .execution:not([open]), .question:not([open]) { display: none; }
  h2 { margin: 0; overflow-wrap: anywhere; }
  h3 { margin: 0; font-size: 15px; }
  p { margin: 0; }
  .discret { color: var(--cr-texte-discret); }
  .petit { font-size: 12px; }
  .progression { display: flex; gap: 12px; align-items: center; }
  .progression .image { width: 64px; max-height: 90px; object-fit: contain; border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); }
  .progression .textes { flex: 1; display: flex; flex-direction: column; gap: 4px; min-width: 0; overflow-wrap: anywhere; }
  .barre-progression.indetermine > div { animation: va-et-vient 1.2s ease-in-out infinite alternate; }
  @keyframes va-et-vient { from { margin-left: 0; } to { margin-left: 70%; } }
  .journal {
    min-height: 90px; max-height: 220px; flex: 0 1 auto; overflow-y: auto; padding: 6px 8px; font-family: ui-monospace, Consolas, monospace; font-size: 12px;
    background: var(--cr-fond); border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); white-space: pre-wrap; overflow-wrap: anywhere;
  }
  .journal:focus-visible { outline: 2px solid var(--cr-accent); }
  .genre-erreur { color: var(--cr-erreur); }
  .genre-systeme { color: var(--cr-texte-discret); font-style: italic; }
  .genre-info { color: var(--cr-accent); }
  .etat.ok { color: var(--cr-accent); font-weight: 600; }
  .etat.attention { font-weight: 600; }
  .etat.erreur, .erreur { color: var(--cr-erreur); }
  .apercu { display: flex; flex-direction: column; gap: 6px; min-height: 0; flex: 1; }
  .conflit, .conflit-texte { color: var(--cr-erreur); }
  .conflit-texte { display: block; font-size: 12px; }
  .lecture-seule { padding: 6px 8px; border: 1px solid var(--cr-erreur); border-radius: var(--cr-rayon); color: var(--cr-erreur); }
  .ligne-outils { display: flex; gap: 8px; align-items: center; }
  .table-apercu { flex: 1; min-height: 120px; max-height: 320px; overflow: auto; border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); }
  table { border-collapse: collapse; width: 100%; font-size: 13px; }
  th { position: sticky; top: 0; background: var(--cr-fond-survol); text-align: left; padding: 4px 6px; }
  td { padding: 3px 6px; border-top: 1px solid var(--cr-bordure); vertical-align: top; }
  td input { accent-color: var(--cr-accent); }
  .titre { max-width: 180px; overflow-wrap: anywhere; }
  .valeur { overflow-wrap: anywhere; white-space: pre-wrap; max-width: 220px; }
  .avant { color: var(--cr-texte-discret); }
  tr.decoche td { opacity: 0.55; }
  tr.conflit td:first-child { box-shadow: inset 3px 0 0 var(--cr-erreur); }
  .genre { font-size: 11px; padding: 1px 6px; border: 1px solid var(--cr-bordure); border-radius: 999px; white-space: nowrap; }
  .g-renommage { border-color: var(--cr-accent); }
  .g-comicinfo { border-style: dashed; }
  .bilan { display: flex; flex-direction: column; gap: 4px; }
  footer { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; border-top: 1px solid var(--cr-bordure); padding-top: 8px; }
  .espace { flex: 1; }
  .mini { padding: 2px 8px; font-size: 12px; }
  .question { width: min(480px, calc(100vw - 32px)); }
  .texte-question { white-space: pre-wrap; overflow-wrap: anywhere; max-height: 320px; overflow-y: auto; margin-bottom: 12px; }
  button.principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); font-weight: 600; min-width: 80px; }
  button.principal:hover:not(:disabled) { background: var(--cr-accent); filter: brightness(1.1); }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); }
</style>
