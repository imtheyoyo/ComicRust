<script lang="ts" module>
  let compteur = 0;
  /** Préfixe unique d'identifiants (labels `for`) pour chaque liste de contrôles affichée. */
  export const prefixeUnique = () => `cs${++compteur}`;
</script>

<script lang="ts">
  // Rendu générique des contrôles d'un formulaire de script (FormSpec de
  // comicrust_core::scripting::forms) : texte, multiligne, mot de passe, nombre, case, liste,
  // boutons radio, liste à cocher, liste éditable, tableau, chemin, texte fixe, séparateur, image,
  // groupe, colonnes, onglets, bouton. Récursif pour les conteneurs. Tout texte venant du script
  // est inséré comme texte (aucun {@html}) ; les images passent par imageScriptSure.
  import { imageScriptSure, type ControleScript } from '../lib/api';
  import {
    celluleVide, choixNormaux, colonnesTable, estSecret, groupesInsertion, indication, rangChoix, texteValeur,
  } from '../lib/formulaireScript';
  import ControlesScript from './ControlesScript.svelte';

  interface Props {
    controles: ControleScript[];
    valeurs: Record<string, unknown>;
    inactifs: Set<ControleScript>;
    erreurs: Record<string, string>;
    /** Largeur de la colonne des libellés (px). */
    largeurLibelles: number;
    /** Bouton du formulaire cliqué (clé). */
    onbouton: (cle: string) => void;
    /** Hérité d'un conteneur désactivé. */
    desactive?: boolean;
  }
  let { controles, valeurs = $bindable(), inactifs, erreurs, largeurLibelles, onbouton, desactive = false }: Props = $props();

  const prefixe = prefixeUnique();
  const idDe = (c: ControleScript, i: number) => `${prefixe}-${c.key ? c.key.replace(/[^\w-]/g, '_') : i}`;
  /** Onglet affiché de chaque contrôle « tabs » (par rang). */
  let ongletsActifs = $state<Record<number, number>>({});
  /** Menu « Insérer » ouvert (rang du contrôle) et son filtre. */
  let insertionOuverte = $state<number | null>(null);
  let filtreInsertion = $state('');
  /** Saisie en cours des listes éditables (par clé). */
  let saisieListe = $state<Record<string, string>>({});
  let selectionListe = $state<Record<string, number>>({});

  const inactif = (c: ControleScript) => desactive || inactifs.has(c);
  /** Contrôles « en bloc » (liste à cocher, liste éditable, tableau, texte multiligne) : libellé
   *  au-dessus et toute la largeur, comme la version egui. Avant la tauri-v0.9.1, ils restaient
   *  dans la colonne de droite de la grille : dans une colonne étroite (onglet « Données » de
   *  « Configurer BD2 » : 2 colonnes et des libellés de 360 px), il ne leur restait que ~40 px et
   *  la liste des champs était invisible. */
  const enBloc = (c: ControleScript) =>
    ['multichoice', 'list_edit', 'table'].includes(c.type) ||
    (['text', 'textarea'].includes(c.type) && Math.min(100, c.rows ?? (c.type === 'textarea' ? 4 : 1)) > 1);
  /** Sans colonne de libellé : conteneurs, textes fixes, cases (libellé à droite), contrôles en
   *  bloc et contrôles sans libellé (comme `label_cell` de la version egui). */
  const pleineLargeur = (c: ControleScript) =>
    ['label', 'separator', 'image', 'group', 'columns', 'tabs', 'button', 'checkbox'].includes(c.type) ||
    enBloc(c) || !(c.label ?? '').trim();

  function fixer(cle: string | undefined, v: unknown) {
    if (cle) valeurs[cle] = v;
  }

  function saisirNombre(c: ControleScript, brut: string) {
    const t = brut.trim().replace(',', '.');
    fixer(c.key, t === '' ? null : Number(t));
  }

  function inserer(c: ControleScript, id: string, morceau: string) {
    const el = document.getElementById(id) as HTMLInputElement | HTMLTextAreaElement | null;
    const actuel = texteValeur(valeurs[c.key!]);
    const debut = el?.selectionStart ?? actuel.length;
    const fin = el?.selectionEnd ?? actuel.length;
    fixer(c.key, actuel.slice(0, debut) + morceau + actuel.slice(fin));
    insertionOuverte = null;
    filtreInsertion = '';
    queueMicrotask(() => {
      el?.focus();
      el?.setSelectionRange(debut + morceau.length, debut + morceau.length);
    });
  }

  async function coller(c: ControleScript) {
    try {
      const t = await navigator.clipboard.readText();
      fixer(c.key, t.trim());
    } catch {
      /* presse-papiers refusé : rien */
    }
  }

  function ajouterListe(c: ControleScript) {
    const t = (saisieListe[c.key!] ?? '').trim();
    if (!t) return;
    const l = Array.isArray(valeurs[c.key!]) ? [...(valeurs[c.key!] as string[])] : [];
    l.push(t);
    fixer(c.key, l);
    saisieListe[c.key!] = '';
    selectionListe[c.key!] = l.length - 1;
  }

  function retirerListe(c: ControleScript) {
    const l = [...((valeurs[c.key!] as string[]) ?? [])];
    const i = selectionListe[c.key!] ?? -1;
    if (i < 0 || i >= l.length) return;
    l.splice(i, 1);
    fixer(c.key, l);
    selectionListe[c.key!] = Math.min(i, l.length - 1);
  }

  function deplacerListe(c: ControleScript, pas: -1 | 1) {
    const l = [...((valeurs[c.key!] as string[]) ?? [])];
    const i = selectionListe[c.key!] ?? -1;
    const j = i + pas;
    if (i < 0 || j < 0 || j >= l.length) return;
    [l[i], l[j]] = [l[j], l[i]];
    fixer(c.key, l);
    selectionListe[c.key!] = j;
  }

  function cellule(c: ControleScript, r: number, k: number, v: unknown) {
    const lignes = ((valeurs[c.key!] as unknown[][]) ?? []).map((x) => [...x]);
    lignes[r][k] = v;
    fixer(c.key, lignes);
  }

  function ajouterLigne(c: ControleScript) {
    const cols = colonnesTable(c);
    fixer(c.key, [...((valeurs[c.key!] as unknown[][]) ?? []), cols.map(celluleVide)]);
  }

  function retirerLigne(c: ControleScript, r: number) {
    fixer(c.key, ((valeurs[c.key!] as unknown[][]) ?? []).filter((_, i) => i !== r));
  }

  function basculerMulti(c: ControleScript, valeur: unknown, coche: boolean) {
    const ch = choixNormaux(c.choices);
    const actuel = Array.isArray(valeurs[c.key!]) ? (valeurs[c.key!] as unknown[]) : [];
    const garde = ch.filter((x) => (x.valeur === valeur ? coche : actuel.some((a) => rangChoix([x], a) === 0)));
    fixer(c.key, garde.map((x) => x.valeur));
  }

  const multiCoche = (c: ControleScript, v: unknown) =>
    Array.isArray(valeurs[c.key!]) && (valeurs[c.key!] as unknown[]).some((a) => rangChoix([{ valeur: v, libelle: '' }], a) === 0);

  const styleTaille = (n: number | undefined) => (typeof n === 'number' && n > 0 ? `${Math.min(n, 4000)}px` : undefined);
</script>

<div class="grille" style:--libelles="{largeurLibelles}px">
  {#each controles as c, i (i)}
    {@const id = idDe(c, i)}
    {@const off = inactif(c)}
    {@const err = c.key ? erreurs[c.key] : undefined}
    {@const aide = c.help || undefined}
    {#if !pleineLargeur(c)}
      <label class="libelle" class:off for={id} title={aide}>{c.label ?? ''}{c.required ? ' *' : ''}</label>
    {/if}
    <div class="controle" class:plein={pleineLargeur(c)} class:off data-cle={c.key || undefined} data-type={c.type}>
      {#if enBloc(c) && (c.label ?? '').trim()}
        <div class="entete">
          {#if c.type === 'text' || c.type === 'textarea'}
            <label class="titre-bloc" for={id} title={aide}>{c.label}{c.required ? ' *' : ''}</label>
          {:else}
            <span class="titre-bloc" id="{id}-titre" title={aide}>{c.label}{c.required ? ' *' : ''}</span>
          {/if}
          {#if c.type === 'multichoice'}
            {@const n = choixNormaux(c.choices).length}
            {@const coches = choixNormaux(c.choices).filter((x) => multiCoche(c, x.valeur)).length}
            <span class="discret petit" data-testid="multi-compte">{coches}/{n}</span>
          {/if}
        </div>
      {/if}
      {#if c.type === 'text' || c.type === 'textarea' || c.type === 'secret'}
        {@const groupes = groupesInsertion(c)}
        {@const lignes = Math.max(1, Math.min(100, c.rows ?? (c.type === 'textarea' ? 4 : 1)))}
        <div class="ligne">
          {#if lignes > 1}
            <textarea {id} rows={lignes} disabled={off} placeholder={c.placeholder ?? ''} title={aide} aria-invalid={!!err}
              aria-describedby={err ? `${id}-err` : undefined} style:width={styleTaille(c.width)}
              value={texteValeur(valeurs[c.key!])} oninput={(e) => fixer(c.key, e.currentTarget.value)}></textarea>
          {:else}
            <input {id} type={estSecret(c) ? 'password' : 'text'} disabled={off} placeholder={c.placeholder ?? ''} title={aide}
              autocomplete={estSecret(c) ? 'new-password' : 'off'} spellcheck="false" aria-invalid={!!err}
              aria-describedby={err ? `${id}-err` : undefined} style:width={styleTaille(c.width)}
              value={texteValeur(valeurs[c.key!])} oninput={(e) => fixer(c.key, e.currentTarget.value)} />
          {/if}
          {#if c.paste && lignes === 1 && !estSecret(c)}
            <button type="button" disabled={off} onclick={() => void coller(c)} title="Coller le presse-papiers">📋 Coller</button>
          {/if}
          {#if groupes.length}
            <button type="button" disabled={off} aria-expanded={insertionOuverte === i} aria-haspopup="listbox"
              onclick={() => { insertionOuverte = insertionOuverte === i ? null : i; filtreInsertion = ''; }}>Insérer ▾</button>
          {/if}
        </div>
        {#if insertionOuverte === i && groupes.length}
          <div class="insertion" role="group" aria-label="Insérer un morceau de texte">
            <!-- svelte-ignore a11y_autofocus -->
            <input type="search" placeholder="Filtrer…" bind:value={filtreInsertion} autofocus aria-label="Filtrer les morceaux"
              onkeydown={(e) => { if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); insertionOuverte = null; } }} />
            <div class="liste-insertion">
              {#each groupes as g, gi (gi)}
                {@const items = g.items.filter((x) => !filtreInsertion.trim() || (x.libelle + ' ' + texteValeur(x.valeur)).toLowerCase().includes(filtreInsertion.trim().toLowerCase()))}
                {#if items.length}
                  {#if g.titre}<div class="titre-groupe">{g.titre}</div>{/if}
                  {#each items as it, k (k)}
                    <button type="button" onclick={() => inserer(c, id, texteValeur(it.valeur))}>
                      <span>{it.libelle}</span>{#if it.libelle !== texteValeur(it.valeur)}<code>{texteValeur(it.valeur)}</code>{/if}
                    </button>
                  {/each}
                {/if}
              {/each}
            </div>
          </div>
        {/if}
        {#if !estSecret(c)}
          {@const h = indication(c, texteValeur(valeurs[c.key!]))}
          {#if h}<span class="indication" class:ok={h[0]}>{h[1]}</span>{/if}
        {/if}
      {:else if c.type === 'number'}
        {@const v = valeurs[c.key!]}
        <input {id} type="number" class="nombre" disabled={off} title={aide} min={c.min} max={c.max} step={c.step && c.step > 0 ? c.step : (c.decimals ? 10 ** -c.decimals : 1)}
          aria-invalid={!!err} aria-describedby={err ? `${id}-err` : undefined} style:width={styleTaille(c.width)}
          value={typeof v === 'number' && Number.isFinite(v) ? v : ''} oninput={(e) => saisirNombre(c, e.currentTarget.value)} />
      {:else if c.type === 'checkbox'}
        <label class="case" title={aide}>
          <input {id} type="checkbox" disabled={off} checked={!!valeurs[c.key!]} onchange={(e) => fixer(c.key, e.currentTarget.checked)} />
          <span>{c.label ?? ''}</span>
        </label>
      {:else if c.type === 'choice'}
        {@const ch = choixNormaux(c.choices)}
        {#if c.editable}
          <input {id} type="text" list="{id}-liste" disabled={off} title={aide} spellcheck="false" style:width={styleTaille(c.width)}
            aria-invalid={!!err} value={texteValeur(valeurs[c.key!])}
            oninput={(e) => { const t = e.currentTarget.value; const r = ch.findIndex((x) => x.libelle === t); fixer(c.key, r >= 0 ? ch[r].valeur : t); }} />
          <datalist id="{id}-liste">{#each ch as x, k (k)}<option value={x.libelle}></option>{/each}</datalist>
        {:else}
          {@const r = rangChoix(ch, valeurs[c.key!])}
          <select {id} disabled={off} title={aide} aria-invalid={!!err} style:width={styleTaille(c.width)} value={String(r)}
            onchange={(e) => { const k = Number(e.currentTarget.value); fixer(c.key, k >= 0 ? ch[k].valeur : null); }}>
            {#if r < 0}<option value="-1">—</option>{/if}
            {#each ch as x, k (k)}<option value={String(k)}>{x.libelle}</option>{/each}
          </select>
        {/if}
      {:else if c.type === 'radio'}
        {@const ch = choixNormaux(c.choices)}
        {@const r = rangChoix(ch, valeurs[c.key!])}
        <div {id} class="radios" class:horizontal={c.horizontal} role="radiogroup" aria-label={c.label ?? c.key} title={aide}>
          {#each ch as x, k (k)}
            <label class="case"><input type="radio" name="{id}-r" disabled={off} checked={r === k} onchange={() => fixer(c.key, x.valeur)} /><span>{x.libelle}</span></label>
          {/each}
        </div>
      {:else if c.type === 'multichoice'}
        {@const ch = choixNormaux(c.choices)}
        <div {id} class="multi" role="group" aria-label={(c.label ?? '').trim() ? undefined : c.key} aria-labelledby={(c.label ?? '').trim() ? `${id}-titre` : undefined}
          title={aide} style:max-height={styleTaille(c.height) ?? '180px'}>
          {#each ch as x, k (k)}
            <label class="case"><input type="checkbox" disabled={off} checked={multiCoche(c, x.valeur)} onchange={(e) => basculerMulti(c, x.valeur, e.currentTarget.checked)} /><span>{x.libelle}</span></label>
          {/each}
        </div>
        {#if ch.length > 1}
          <div class="ligne petit">
            <button type="button" class="mini" disabled={off} onclick={() => fixer(c.key, ch.map((x) => x.valeur))}>Tout cocher</button>
            <button type="button" class="mini" disabled={off} onclick={() => fixer(c.key, [])}>Tout décocher</button>
          </div>
        {/if}
      {:else if c.type === 'list_edit'}
        {@const l = Array.isArray(valeurs[c.key!]) ? (valeurs[c.key!] as unknown[]).map(texteValeur) : []}
        <select {id} size={Math.max(3, Math.min(8, l.length + 1))} disabled={off} title={aide} class="liste-edit" style:height={styleTaille(c.height)}
          value={String(selectionListe[c.key!] ?? -1)} onchange={(e) => (selectionListe[c.key!] = Number(e.currentTarget.value))}
          onkeydown={(e) => { if (e.key === 'Delete') { e.preventDefault(); retirerListe(c); } }}>
          {#each l as x, k (k)}<option value={String(k)}>{x}</option>{/each}
        </select>
        <div class="ligne">
          <input type="text" list="{id}-sugg" disabled={off} placeholder="Nouvel élément" aria-label="Nouvel élément de {c.label ?? c.key}"
            value={saisieListe[c.key!] ?? ''} oninput={(e) => (saisieListe[c.key!] = e.currentTarget.value)}
            onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); ajouterListe(c); } }} />
          {#if c.choices?.length}<datalist id="{id}-sugg">{#each c.choices as s, k (k)}<option value={texteValeur(s)}></option>{/each}</datalist>{/if}
          <button type="button" class="mini" disabled={off} onclick={() => ajouterListe(c)}>➕ Ajouter</button>
          <button type="button" class="mini" disabled={off} onclick={() => retirerListe(c)} aria-label="Retirer l'élément choisi">➖</button>
          <button type="button" class="mini" disabled={off} onclick={() => deplacerListe(c, -1)} aria-label="Monter l'élément choisi">⬆</button>
          <button type="button" class="mini" disabled={off} onclick={() => deplacerListe(c, 1)} aria-label="Descendre l'élément choisi">⬇</button>
        </div>
      {:else if c.type === 'table'}
        {@const cols = colonnesTable(c)}
        {@const lignes = Array.isArray(valeurs[c.key!]) ? (valeurs[c.key!] as unknown[][]) : []}
        <div class="table-defile" style:max-height={styleTaille(c.height) ?? '220px'}>
          <table {id} aria-label={(c.label ?? '').trim() || c.key}>
            <thead><tr>{#each cols as col, k (k)}<th style:width={styleTaille(col.width)}>{col.title}</th>{/each}<th><span class="sr">Actions</span></th></tr></thead>
            <tbody>
              {#each lignes as ligne, r (r)}
                <tr>
                  {#each cols as col, k (k)}
                    <td>
                      {#if col.type === 'checkbox'}
                        <input type="checkbox" disabled={off} checked={!!ligne[k]} aria-label="{col.title} ligne {r + 1}" onchange={(e) => cellule(c, r, k, e.currentTarget.checked)} />
                      {:else if col.type === 'choice'}
                        <select disabled={off} aria-label="{col.title} ligne {r + 1}" value={String(Math.max(0, rangChoix(col.choix, ligne[k])))}
                          onchange={(e) => cellule(c, r, k, col.choix[Number(e.currentTarget.value)]?.valeur ?? null)}>
                          {#each col.choix as x, j (j)}<option value={String(j)}>{x.libelle}</option>{/each}
                        </select>
                      {:else if col.type === 'number'}
                        <input type="number" disabled={off} aria-label="{col.title} ligne {r + 1}" value={Number(ligne[k]) || 0}
                          oninput={(e) => cellule(c, r, k, Number(e.currentTarget.value.replace(',', '.')) || 0)} />
                      {:else}
                        <input type="text" disabled={off} aria-label="{col.title} ligne {r + 1}" value={texteValeur(ligne[k])} spellcheck="false"
                          oninput={(e) => cellule(c, r, k, e.currentTarget.value)} />
                      {/if}
                    </td>
                  {/each}
                  <td><button type="button" class="mini" disabled={off} onclick={() => retirerLigne(c, r)} aria-label="Supprimer la ligne {r + 1}">✖</button></td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        <div class="ligne"><button type="button" class="mini" disabled={off} onclick={() => ajouterLigne(c)}>➕ Ajouter une ligne</button></div>
      {:else if c.type === 'folder' || c.type === 'file'}
        <input {id} type="text" disabled={off} spellcheck="false" style:width={styleTaille(c.width)} aria-invalid={!!err}
          placeholder={c.type === 'folder' ? 'Chemin du dossier' : 'Chemin du fichier'}
          title={(aide ? aide + ' — ' : '') + 'Saisir ou coller le chemin complet' + (c.filters?.length ? ` (${c.filters.map((f) => texteValeur(f[0])).join(', ')})` : '')}
          value={texteValeur(valeurs[c.key!])} oninput={(e) => fixer(c.key, e.currentTarget.value)} />
      {:else if c.type === 'label'}
        <p class="texte style-{c.style || 'normal'}" title={aide}>{c.text ?? c.label ?? ''}</p>
      {:else if c.type === 'separator'}
        <hr />
      {:else if c.type === 'image'}
        {@const src = imageScriptSure(c.data)}
        <figure>
          {#if src}
            <img {src} alt={c.caption || c.label || 'Image du script'} style:max-width="{Math.min(c.max_width ?? 240, 4000)}px" style:max-height="{Math.min(c.max_height ?? 320, 4000)}px" draggable="false" />
          {:else}
            <span class="discret">(image refusée)</span>
          {/if}
          {#if c.caption}<figcaption>{c.caption}</figcaption>{/if}
        </figure>
      {:else if c.type === 'group'}
        <fieldset disabled={off}>
          {#if c.title || c.label}<legend>{c.title || c.label}</legend>{/if}
          <ControlesScript controles={c.controls ?? []} bind:valeurs {inactifs} {erreurs} {largeurLibelles} {onbouton} desactive={off} />
        </fieldset>
      {:else if c.type === 'columns'}
        <div class="colonnes">
          {#each (c.columns ?? []) as col, k (k)}
            {@const w = c.widths?.[k]}
            <div class="colonne" style:flex={typeof w === 'number' && w > 0 ? `0 0 ${Math.min(w, 4000)}px` : '1 1 0'}>
              <ControlesScript controles={Array.isArray(col) ? (col as ControleScript[]) : []} bind:valeurs {inactifs} {erreurs} {largeurLibelles} {onbouton} desactive={off} />
            </div>
          {/each}
        </div>
      {:else if c.type === 'tabs'}
        {@const actif = Math.min(ongletsActifs[i] ?? 0, (c.tabs?.length ?? 1) - 1)}
        <div class="onglets" role="tablist" aria-label={c.label || 'Onglets'}>
          {#each (c.tabs ?? []) as t, k (k)}
            <button type="button" role="tab" id="{id}-t{k}" aria-selected={actif === k} aria-controls="{id}-p" class:actif={actif === k}
              tabindex={actif === k ? 0 : -1} onclick={() => (ongletsActifs[i] = k)}
              onkeydown={(e) => {
                const n = c.tabs?.length ?? 1;
                const j = e.key === 'ArrowRight' ? (k + 1) % n : e.key === 'ArrowLeft' ? (k - 1 + n) % n : -1;
                if (j >= 0) { e.preventDefault(); ongletsActifs[i] = j; document.getElementById(`${id}-t${j}`)?.focus(); }
              }}>{t.title ?? `Onglet ${k + 1}`}</button>
          {/each}
        </div>
        <div class="panneau" role="tabpanel" id="{id}-p" aria-labelledby="{id}-t{actif}">
          {#each (c.tabs ?? []) as t, k (k)}
            <div hidden={actif !== k}>
              <ControlesScript controles={t.controls ?? []} bind:valeurs {inactifs} {erreurs} {largeurLibelles} {onbouton} desactive={off} />
            </div>
          {/each}
        </div>
      {:else if c.type === 'button'}
        <button type="button" {id} disabled={off} title={aide} onclick={() => onbouton(c.key ?? '')}>{c.label || c.key}</button>
      {:else}
        <span class="discret">Contrôle inconnu : {c.type}</span>
      {/if}
      {#if err}<span class="erreur" id="{id}-err" role="alert">⚠ {err}</span>{/if}
    </div>
  {/each}
</div>

<style>
  /* Colonne des libellés : `label_width` du script, mais jamais plus de 45 % de la place (grille
     imbriquée dans une colonne ou un groupe étroit : le contrôle garde de quoi s'afficher). */
  .grille { display: grid; grid-template-columns: minmax(min(80px, 30%), min(var(--libelles), 45%)) minmax(0, 1fr); gap: 8px 12px; align-items: start; }
  .entete { display: flex; align-items: baseline; gap: 8px; min-width: 0; }
  .titre-bloc { color: var(--cr-texte-discret); overflow-wrap: anywhere; }
  .libelle { padding-top: 6px; color: var(--cr-texte-discret); overflow-wrap: anywhere; }
  .controle { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .controle.plein { grid-column: 1 / -1; }
  .off { opacity: 0.55; }
  .ligne { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; min-width: 0; }
  .ligne > input[type='text'], .ligne > input[type='password'] { flex: 1; min-width: 120px; }
  input[type='password'], input[type='number'], textarea {
    font: inherit; color: var(--cr-texte); background: var(--cr-fond-panneau); border: 1px solid var(--cr-bordure);
    border-radius: var(--cr-rayon); padding: 5px 8px;
  }
  textarea { flex: 1; min-width: 0; resize: vertical; }
  textarea:focus-visible, input[type='password']:focus-visible, input[type='number']:focus-visible { outline: 2px solid var(--cr-accent); outline-offset: 1px; }
  .nombre { width: 120px; }
  [aria-invalid='true'] { border-color: var(--cr-erreur); }
  .case { display: inline-flex; align-items: center; gap: 6px; }
  .case input { accent-color: var(--cr-accent); }
  .radios { display: flex; flex-direction: column; gap: 4px; padding-top: 4px; }
  .radios.horizontal { flex-direction: row; flex-wrap: wrap; gap: 14px; }
  .multi { display: flex; flex-direction: column; gap: 3px; overflow-y: auto; overflow-x: hidden; min-height: 60px; min-width: 160px; padding: 6px 8px; border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); }
  .multi .case { align-items: flex-start; }
  .multi .case span { overflow-wrap: anywhere; }
  .liste-edit { min-width: 220px; width: 100%; }
  .table-defile { overflow: auto; border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); }
  table { border-collapse: collapse; width: 100%; }
  th { position: sticky; top: 0; background: var(--cr-fond-survol); text-align: left; padding: 4px 6px; font-weight: 600; }
  td { padding: 2px 4px; border-top: 1px solid var(--cr-bordure); }
  td input[type='text'], td input[type='number'], td select { width: 100%; padding: 3px 6px; }
  .indication { font-size: 12px; color: var(--cr-texte-discret); }
  .indication.ok { color: var(--cr-accent); }
  .erreur { font-size: 12px; color: var(--cr-erreur); }
  .texte { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  .style-bold { font-weight: 700; }
  .style-small { font-size: 12px; }
  .style-weak { color: var(--cr-texte-discret); }
  .style-heading { font-size: 16px; font-weight: 700; }
  .style-warning { color: var(--cr-accent); font-weight: 600; }
  .style-error { color: var(--cr-erreur); font-weight: 600; }
  hr { width: 100%; border: 0; border-top: 1px solid var(--cr-bordure); margin: 2px 0; }
  figure { margin: 0; display: inline-flex; flex-direction: column; gap: 4px; align-items: flex-start; }
  figure img { border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); object-fit: contain; }
  figcaption { font-size: 12px; color: var(--cr-texte-discret); }
  fieldset { border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); padding: 8px 10px; margin: 0; min-width: 0; }
  legend { padding: 0 4px; font-weight: 600; }
  .colonnes { display: flex; gap: 14px; align-items: flex-start; min-width: 0; }
  .colonne { min-width: 0; }
  .onglets { display: flex; flex-wrap: wrap; gap: 4px; border-bottom: 1px solid var(--cr-bordure); padding-bottom: 6px; }
  .onglets button.actif { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); }
  .panneau { padding-top: 8px; }
  .insertion { display: flex; flex-direction: column; gap: 4px; padding: 6px; border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); background: var(--cr-fond); }
  .liste-insertion { max-height: 180px; overflow-y: auto; display: flex; flex-direction: column; }
  .liste-insertion button { display: flex; justify-content: space-between; gap: 10px; border: 0; background: none; text-align: left; padding: 3px 6px; }
  .liste-insertion button:hover, .liste-insertion button:focus-visible { background: var(--cr-accent); color: var(--cr-accent-texte); }
  .titre-groupe { font-size: 12px; font-weight: 700; color: var(--cr-texte-discret); padding: 4px 6px 0; }
  code { font-size: 12px; opacity: 0.8; }
  .mini { padding: 2px 8px; }
  .petit { font-size: 12px; }
  .discret { color: var(--cr-texte-discret); }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); }
</style>
