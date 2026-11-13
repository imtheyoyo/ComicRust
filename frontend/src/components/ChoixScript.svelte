<script lang="ts">
  // Liste de choix demandée par un script (`comicrust.choose`, événement script:choix) : tableau
  // ou fiches avec vignettes. Les vignettes « lazy » / « on_demand » sont demandées au backend
  // quand leur ligne devient visible (IntersectionObserver → script_vignettes_voulues), chaque rang
  // une seule fois, et arrivent par script:vignette. Clavier : ↑ ↓ Début Fin, Espace (choix
  // multiple), Entrée = choisir, Échap = annuler ; double-clic = choisir. Délai de choix
  // automatique (timeout) arrêté par toute action de l'utilisateur.
  import { onDestroy, onMount, tick } from 'svelte';
  import { imageScriptSure, type ChooseSpecScript } from '../lib/api';
  import { texteValeur } from '../lib/formulaireScript';
  import { modale } from '../lib/modale';

  interface Props {
    spec: ChooseSpecScript;
    /** Vignettes reçues après coup (rang → URL sûre). */
    vignettes: Record<number, string>;
    nomScript: string;
    onvoulues: (rangs: number[]) => void;
    onrepondre: (valeur: number | number[] | null) => void;
    onarreter: () => void;
  }
  let { spec, vignettes, nomScript, onvoulues, onrepondre, onarreter }: Props = $props();

  const lignes = $derived((spec.rows ?? []).map((r) => (Array.isArray(r) ? r.map(texteValeur) : [texteValeur(r)])));
  const nbCols = $derived(Math.max(1, (spec.columns ?? []).length, ...lignes.map((l) => l.length)));
  const colonnes = $derived(Array.from({ length: nbCols }, (_, i) => texteValeur(spec.columns?.[i])));
  const tardives = $derived(new Set([...(spec.lazy ?? []), ...(spec.on_demand ?? [])]));
  const images = $derived((spec.images ?? []).map((x) => imageScriptSure(x)));
  const avecVignettes = $derived(images.some(Boolean) || tardives.size > 0);
  const fiches = $derived(spec.layout === 'cards' || (spec.layout !== 'table' && avecVignettes));
  const taille = $derived(Math.min(320, Math.max(24, spec.thumb_size ?? 64)));
  const multi = $derived(!!spec.multi);
  // svelte-ignore state_referenced_locally
  const avecFiltre = spec.filter ?? (spec.rows ?? []).length > 8;

  // svelte-ignore state_referenced_locally
  const defaut = (Array.isArray(spec.default) ? spec.default : typeof spec.default === 'number' ? [spec.default] : [])
    .filter((i) => Number.isInteger(i) && i >= 0 && i < (spec.rows ?? []).length);
  // svelte-ignore state_referenced_locally
  let choisis = $state<Set<number>>(new Set(spec.multi ? defaut : defaut.slice(0, 1)));
  let curseur = $state(defaut[0] ?? 0);
  let filtre = $state('');
  let tri = $state<{ col: number; sens: 1 | -1 } | null>(null);
  // svelte-ignore state_referenced_locally
  let reste = $state<number | null>(spec.timeout && spec.timeout > 0 && defaut.length ? Math.min(3600, Math.ceil(spec.timeout)) : null);
  let minuterie: ReturnType<typeof setInterval> | undefined;
  let liste = $state<HTMLDivElement>();
  let fini = false;
  const titreId = `titre-choix-${Math.random().toString(36).slice(2, 8)}`;

  /** Rangs affichés (filtre, tri), dans l'ordre d'affichage. */
  const visibles = $derived.by(() => {
    const f = filtre.trim().toLowerCase();
    let r = lignes.map((_, i) => i);
    if (f) r = r.filter((i) => lignes[i].some((c) => c.toLowerCase().includes(f)));
    if (tri) {
      const { col, sens } = tri;
      r.sort((a, b) => sens * (lignes[a][col] ?? '').localeCompare(lignes[b][col] ?? '', 'fr', { numeric: true }) || a - b);
    }
    return r;
  });

  // ───── Vignettes à la demande ─────
  const demandes = new Set<number>();
  let enFile: number[] = [];
  let envoi: ReturnType<typeof setTimeout> | undefined;
  let observateur: IntersectionObserver | undefined;

  function vouloir(rang: number) {
    if (!tardives.has(rang) || demandes.has(rang) || vignettes[rang]) return;
    demandes.add(rang);
    enFile.push(rang);
    clearTimeout(envoi);
    envoi = setTimeout(() => {
      const l = enFile;
      enFile = [];
      if (l.length) onvoulues(l);
    }, 40);
  }

  /** Attachement : chaque ligne à vignette tardive est observée tant qu'elle est affichée. */
  function observer(el: HTMLElement) {
    observateur?.observe(el);
    return () => observateur?.unobserve(el);
  }

  onMount(() => {
    observateur = new IntersectionObserver((entrees) => {
      for (const e of entrees) if (e.isIntersecting) vouloir(Number((e.target as HTMLElement).dataset.rang));
    }, { root: liste, rootMargin: '120px 0px' });
    for (const el of liste?.querySelectorAll<HTMLElement>('[data-tardive]') ?? []) observateur.observe(el);
    if (reste !== null) {
      minuterie = setInterval(() => {
        if (reste === null) return;
        reste -= 1;
        if (reste <= 0) { clearInterval(minuterie); valider(); }
      }, 1000);
    }
    void tick().then(() => liste?.querySelector<HTMLElement>(`[data-rang="${curseur}"]`)?.scrollIntoView({ block: 'nearest' }));
  });
  onDestroy(() => {
    observateur?.disconnect();
    clearInterval(minuterie);
    clearTimeout(envoi);
  });

  function action() {
    if (reste !== null) { reste = null; clearInterval(minuterie); }
  }

  function repondre(v: number | number[] | null) {
    if (fini) return;
    fini = true;
    onrepondre(v);
  }

  function valider() {
    const l = [...choisis].sort((a, b) => a - b);
    if (multi) repondre(l);
    else if (l.length === 1) repondre(l[0]);
  }

  function choisir(rang: number, basculer = false) {
    action();
    curseur = rang;
    vouloir(rang);
    if (!multi) choisis = new Set([rang]);
    else if (basculer) {
      const s = new Set(choisis);
      if (s.has(rang)) s.delete(rang);
      else s.add(rang);
      choisis = s;
    }
  }

  async function deplacer(pas: number | 'debut' | 'fin') {
    const v = visibles;
    if (!v.length) return;
    const k = v.indexOf(curseur);
    const j = pas === 'debut' ? 0 : pas === 'fin' ? v.length - 1 : Math.max(0, Math.min(v.length - 1, (k < 0 ? 0 : k) + pas));
    choisir(v[j]);
    await tick();
    const el = liste?.querySelector<HTMLElement>(`[data-rang="${v[j]}"]`);
    el?.scrollIntoView({ block: 'nearest' });
    el?.focus({ preventScroll: true });
  }

  function clavier(e: KeyboardEvent) {
    const champ = (e.target as HTMLElement).closest('input, select');
    if (e.key === 'ArrowDown') void deplacer(1);
    else if (e.key === 'ArrowUp') void deplacer(-1);
    else if (e.key === 'PageDown' && !champ) void deplacer(10);
    else if (e.key === 'PageUp' && !champ) void deplacer(-10);
    else if (e.key === 'Home' && !champ) void deplacer('debut');
    else if (e.key === 'End' && !champ) void deplacer('fin');
    else if (e.key === ' ' && multi && !champ) choisir(curseur, true);
    else if (e.key === 'Enter' && !(e.target as HTMLElement).closest('button')) {
      if (!multi && visibles.includes(curseur)) choisis = new Set([curseur]);
      valider();
    } else { if (e.key !== 'Escape') action(); return; }
    e.preventDefault();
  }

  const vignette = (rang: number) => images[rang] ?? vignettes[rang];
  const peutValider = $derived(multi || choisis.size === 1);
  const trierPar = (col: number) => { action(); tri = tri?.col === col ? (tri.sens === 1 ? { col, sens: -1 } : null) : { col, sens: 1 }; };
</script>

<dialog class="choix" aria-labelledby={titreId} use:modale={{ peutFermer: () => true, fermer: () => repondre(null), ouvrirAuMontage: true }}
  onkeydown={clavier} onpointerdown={action} data-testid="choix-script">
  <p class="script">📜 {nomScript}</p>
  <h2 id={titreId}>{spec.title || 'Choisir'}</h2>
  {#if spec.message}<p class="message">{spec.message}</p>{/if}
  <div class="outils">
    {#if avecFiltre}
      <input type="search" placeholder="Filtrer…" aria-label="Filtrer la liste" bind:value={filtre} oninput={action} data-testid="choix-filtre" />
    {/if}
    {#if fiches && nbCols > 1}
      <label>Trier par
        <select value={tri ? `${tri.col}:${tri.sens}` : ''} onchange={(e) => { action(); const [c, s] = e.currentTarget.value.split(':'); tri = c === '' ? null : { col: Number(c), sens: Number(s) as 1 | -1 }; }}>
          <option value="">Ordre du script</option>
          {#each colonnes as c, k (k)}<option value="{k}:1">{c || `Colonne ${k + 1}`} ▲</option><option value="{k}:-1">{c || `Colonne ${k + 1}`} ▼</option>{/each}
        </select>
      </label>
    {/if}
    <span class="compte">{visibles.length} / {lignes.length}</span>
  </div>

  <div class="liste" bind:this={liste} role="listbox" aria-label={spec.title || 'Choix'} aria-multiselectable={multi} tabindex="-1" data-testid="choix-liste">
    {#if !fiches}
      <div class="ligne entete" role="presentation" style:grid-template-columns={`${multi ? '28px ' : ''}${avecVignettes ? `${taille}px ` : ''}repeat(${nbCols}, minmax(0, 1fr))`}>
        {#if multi}<span></span>{/if}
        {#if avecVignettes}<span></span>{/if}
        {#each colonnes as c, k (k)}
          <button type="button" class="tete" onclick={() => trierPar(k)} aria-label="Trier par {c || `colonne ${k + 1}`}">{c}{tri?.col === k ? (tri.sens === 1 ? ' ▲' : ' ▼') : ''}</button>
        {/each}
      </div>
    {/if}
    {#each visibles as rang (rang)}
      {@const sel = choisis.has(rang)}
      {@const img = vignette(rang)}
      {#if tardives.has(rang)}
        <!-- svelte-ignore a11y_click_events_have_key_events (clavier géré par la fenêtre : flèches, Espace, Entrée) -->
        <div class="ligne" class:fiche={fiches} class:sel class:curseur={curseur === rang} role="option" aria-selected={sel} tabindex={curseur === rang ? 0 : -1}
          data-rang={rang} data-tardive="1" {@attach observer}
          style:grid-template-columns={fiches ? undefined : `${multi ? '28px ' : ''}${taille}px repeat(${nbCols}, minmax(0, 1fr))`}
          onclick={(e) => choisir(rang, multi && (e.ctrlKey || e.metaKey || (e.target as HTMLElement).matches('input')))}
          ondblclick={() => { choisis = new Set(multi ? [...choisis, rang] : [rang]); valider(); }}>
          {@render contenu(rang, sel, img)}
        </div>
      {:else}
        <!-- svelte-ignore a11y_click_events_have_key_events (clavier géré par la fenêtre : flèches, Espace, Entrée) -->
        <div class="ligne" class:fiche={fiches} class:sel class:curseur={curseur === rang} role="option" aria-selected={sel} tabindex={curseur === rang ? 0 : -1}
          data-rang={rang}
          style:grid-template-columns={fiches ? undefined : `${multi ? '28px ' : ''}${avecVignettes ? `${taille}px ` : ''}repeat(${nbCols}, minmax(0, 1fr))`}
          onclick={(e) => choisir(rang, multi && (e.ctrlKey || e.metaKey || (e.target as HTMLElement).matches('input')))}
          ondblclick={() => { choisis = new Set(multi ? [...choisis, rang] : [rang]); valider(); }}>
          {@render contenu(rang, sel, img)}
        </div>
      {/if}
    {:else}
      <p class="vide">Aucun élément{filtre ? ' ne correspond au filtre' : ''}.</p>
    {/each}
  </div>

  <footer>
    {#if reste !== null}<span class="delai" role="status">Choix automatique dans {reste} s (toute action l'arrête)</span>{/if}
    <button type="button" class="arret" onclick={onarreter}>⏹ Arrêter le script</button>
    <span class="espace"></span>
    <button type="button" onclick={() => repondre(null)} data-testid="choix-annuler">Annuler</button>
    <button type="button" class="principal" disabled={!peutValider} onclick={valider} data-testid="choix-valider">Choisir{multi ? ` (${choisis.size})` : ''}</button>
  </footer>
</dialog>

{#snippet contenu(rang: number, sel: boolean, img: string | undefined)}
  {#if multi}<input type="checkbox" checked={sel} tabindex="-1" aria-label="Choisir la ligne {rang + 1}" />{/if}
  {#if avecVignettes}
    <span class="vignette" style:width="{fiches ? Math.round(taille * 0.72) : taille}px" style:height="{taille}px">
      {#if img}<img src={img} alt="" draggable="false" />{:else if tardives.has(rang)}<span class="attente" aria-label="Vignette en chargement">…</span>{/if}
    </span>
  {/if}
  {#if fiches}
    <span class="textes">
      <strong>{lignes[rang][0] ?? ''}</strong>
      {#each lignes[rang].slice(1) as t, k (k)}
        {#if t}<span>{colonnes[k + 1] ? `${colonnes[k + 1]} : ` : ''}{t}</span>{/if}
      {/each}
    </span>
  {:else}
    {#each Array.from({ length: nbCols }, (_, k) => lignes[rang][k] ?? '') as t, k (k)}<span class="cellule" title={t}>{t}</span>{/each}
  {/if}
{/snippet}

<style>
  .choix { width: min(760px, calc(100vw - 32px)); height: min(640px, calc(100vh - 32px)); display: flex; flex-direction: column; gap: 8px; }
  .choix:not([open]) { display: none; }
  h2 { margin: 0; }
  .script { margin: 0; font-size: 12px; color: var(--cr-texte-discret); }
  .message { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  .outils { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
  .outils input[type='search'] { flex: 1; min-width: 160px; }
  .compte { color: var(--cr-texte-discret); font-size: 12px; }
  .liste { flex: 1; min-height: 0; overflow-y: auto; border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); }
  .ligne { display: grid; gap: 8px; align-items: center; padding: 4px 8px; border-bottom: 1px solid var(--cr-bordure); cursor: default; }
  .ligne.fiche { display: flex; align-items: flex-start; gap: 12px; }
  .ligne:hover { background: var(--cr-fond-survol); }
  .ligne.sel { background: var(--cr-accent); color: var(--cr-accent-texte); }
  .ligne.curseur { outline: 2px solid var(--cr-accent); outline-offset: -2px; }
  .ligne.sel.curseur { outline-color: var(--cr-texte); }
  .ligne:focus-visible { outline: 2px solid var(--cr-texte); outline-offset: -2px; }
  .entete { position: sticky; top: 0; z-index: 1; background: var(--cr-fond-survol); font-weight: 600; }
  .tete { border: 0; background: none; text-align: left; padding: 2px 0; font-weight: 600; color: inherit; }
  .cellule { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .textes { display: flex; flex-direction: column; gap: 2px; min-width: 0; overflow-wrap: anywhere; }
  .vignette { flex: none; display: grid; place-items: center; background: var(--cr-fond-survol); border-radius: var(--cr-rayon); overflow: hidden; }
  .vignette img { width: 100%; height: 100%; object-fit: contain; }
  .attente { color: var(--cr-texte-discret); }
  .vide { padding: 12px; color: var(--cr-texte-discret); }
  footer { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
  .delai { color: var(--cr-accent); font-size: 12px; }
  .arret { font-size: 12px; }
  .espace { flex: 1; }
  button.principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); font-weight: 600; }
  button.principal:hover:not(:disabled) { background: var(--cr-accent); filter: brightness(1.1); }
</style>
