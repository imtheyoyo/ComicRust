<script lang="ts">
  // Critères d'une liste intelligente (ou d'un groupe), récursif : une ligne par critère
  // (Non, champ, opérateur, valeur(s), casse, monter / descendre / supprimer), groupes imbriqués
  // avec leur « tous / au moins un » et leur case Non (comme MatcherGroupEditor de ComicRack).
  // Le changement de champ est calculé par le backend (nouveau_critere).
  import { messageErreur, nouveauCritere, type CatalogueCriteres, type Critere, type GroupeCriteres, type NoeudCritere } from '../lib/api';
  import NoeudsCriteres from './NoeudsCriteres.svelte';

  interface Props {
    /** Tableau réactif ($state) modifié en place. */
    noeuds: NoeudCritere[];
    catalogue: CatalogueCriteres;
    profondeur?: number;
    /** Erreur d'un appel au backend (changement de champ). */
    onerreur?: (message: string) => void;
  }
  let { noeuds, catalogue, profondeur = 0, onerreur }: Props = $props();

  const types = $derived(catalogue.categories.flatMap((c) => c.criteres));
  const info = (type: string) => types.find((c) => c.type === type);

  function critereVide(): Critere {
    return { genre: 'critere', non: false, type: catalogue.defaut.type, op: catalogue.defaut.op, valeur: '', valeur2: '', ignorerCasse: true, modifiable: true };
  }
  const groupeVide = (): GroupeCriteres => ({ genre: 'groupe', non: false, mode: 'tous', noeuds: [critereVide()] });

  /** Change le champ : le backend garde opérateur et valeurs si la nature de la valeur est la même. */
  async function changerChamp(k: number, c: Critere, type: string) {
    try {
      const n = await nouveauCritere(type, $state.snapshot(c) as Critere);
      // Le backend renvoie un critère « nu » : on remet le marqueur de genre attendu par les commandes.
      if (noeuds[k] === c) noeuds[k] = { ...n, genre: 'critere' };
    } catch (e) {
      onerreur?.(messageErreur(e));
    }
  }

  function deplacer(k: number, pas: -1 | 1) {
    const j = k + pas;
    if (j < 0 || j >= noeuds.length) return;
    const [n] = noeuds.splice(k, 1);
    noeuds.splice(j, 0, n);
  }

  const indication = (c: Critere) => (info(c.type)?.nature === 'date' ? (c.op === 3 ? 'nombre' : 'AAAA-MM-JJ') : '');
</script>

<div class="noeuds" class:imbrique={profondeur > 0}>
  {#if noeuds.length === 0}
    <p class="discret">Aucun critère : tous les livres de la liste de départ.</p>
  {/if}
  {#each noeuds as n, k (n)}
    {#if n.genre === 'groupe'}
      <fieldset class="groupe" data-testid="groupe-criteres">
        <legend>
          <label class="non" title="Inverse le groupe (« ! » dans ComicRack)"><input type="checkbox" bind:checked={n.non} /> Non</label>
          Groupe : correspond à
          <select bind:value={n.mode} aria-label="Combinaison du groupe">
            <option value="tous">tous</option>
            <option value="auMoinsUn">au moins un</option>
          </select>
          des critères suivants
          <span class="actions">
            <button type="button" class="petit" disabled={k === 0} onclick={() => deplacer(k, -1)} title="Monter" aria-label="Monter le groupe">↑</button>
            <button type="button" class="petit" disabled={k === noeuds.length - 1} onclick={() => deplacer(k, 1)} title="Descendre" aria-label="Descendre le groupe">↓</button>
            <button type="button" class="petit" onclick={() => noeuds.splice(k, 1)} title="Supprimer le groupe" aria-label="Supprimer le groupe">✖</button>
          </span>
        </legend>
        <NoeudsCriteres noeuds={n.noeuds} {catalogue} profondeur={profondeur + 1} {onerreur} />
      </fieldset>
    {:else if !n.modifiable}
      <div class="critere brut" data-testid="critere">
        <span title="Conservé tel quel à l'enregistrement ; modifiable seulement dans ComicRack.">⚠ {n.non ? 'Non : ' : ''}{n.libelle ?? n.type}</span>
        <span class="actions">
          <button type="button" class="petit" onclick={() => noeuds.splice(k, 1)} title="Supprimer le critère" aria-label="Supprimer le critère">✖</button>
        </span>
      </div>
    {:else}
      {@const i = info(n.type)}
      {@const o = i?.operateurs.find((x) => x.op === n.op)}
      <div class="critere" data-testid="critere">
        <label class="non" title="Inverse le critère (« ! » dans ComicRack)"><input type="checkbox" bind:checked={n.non} /> Non</label>
        <select value={n.type} onchange={(e) => void changerChamp(k, n, e.currentTarget.value)} aria-label="Champ" data-testid="champ-critere">
          {#if !i}<option value={n.type}>{n.type}</option>{/if}
          {#each catalogue.categories as c (c.nom)}
            <optgroup label={c.nom}>
              {#each c.criteres as t (t.type)}<option value={t.type}>{t.libelle}{t.evalue ? '' : ' *'}</option>{/each}
            </optgroup>
          {/each}
        </select>
        {#if i}
          <select bind:value={n.op} aria-label="Opérateur" data-testid="operateur-critere">
            {#each i.operateurs as x (x.op)}<option value={x.op}>{x.libelle}</option>{/each}
          </select>
        {/if}
        {#if i?.nature === 'valeurPerso'}
          <input type="text" bind:value={n.valeur} placeholder="nom" aria-label="Nom de la valeur personnalisée" class="court" />
          <input type="text" bind:value={n.valeur2} placeholder="texte" aria-label="Texte cherché" data-testid="valeur-critere" />
        {:else if o && o.valeurs >= 1}
          <input type="text" bind:value={n.valeur} placeholder={indication(n)} aria-label="Valeur" data-testid="valeur-critere"
            class:court={i?.nature !== 'texte' && i?.nature !== 'expression'} />
          {#if o.valeurs >= 2}
            <span class="discret">et</span>
            <input type="text" bind:value={n.valeur2} placeholder={indication(n)} aria-label="Seconde valeur" class="court" data-testid="valeur2-critere" />
          {/if}
          {#if o.unite ?? i?.unite}<span class="discret">{o.unite ?? i?.unite}</span>{/if}
        {/if}
        {#if i?.texte}
          <label class="casse" title="Ignorer la différence entre majuscules et minuscules"><input type="checkbox" bind:checked={n.ignorerCasse} /> Aa</label>
        {/if}
        {#if i && !i.evalue}
          <span class="non-evalue" title="ComicRust ne sait pas encore évaluer ce critère (ComicRack, oui)">⚠</span>
        {/if}
        <span class="actions">
          <button type="button" class="petit" disabled={k === 0} onclick={() => deplacer(k, -1)} title="Monter" aria-label="Monter le critère">↑</button>
          <button type="button" class="petit" disabled={k === noeuds.length - 1} onclick={() => deplacer(k, 1)} title="Descendre" aria-label="Descendre le critère">↓</button>
          <button type="button" class="petit" onclick={() => noeuds.splice(k, 1)} title="Supprimer le critère" aria-label="Supprimer le critère">✖</button>
        </span>
      </div>
    {/if}
  {/each}
  <div class="ajouts">
    <button type="button" onclick={() => noeuds.push(critereVide())} title="Nouveau critère" data-testid="ajouter-critere">➕ Critère</button>
    <button type="button" onclick={() => noeuds.push(groupeVide())} title="Nouveau groupe de critères" data-testid="ajouter-groupe">➕ Groupe</button>
  </div>
</div>

<style>
  .noeuds { display: grid; gap: 4px; }
  .critere { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 3px 0; }
  .critere input[type='text'] { flex: 1; min-width: 120px; }
  .critere input.court { flex: 0 1 110px; min-width: 70px; }
  .critere.brut { color: var(--cr-texte-discret); font-style: italic; }
  .groupe { margin: 4px 0; padding: 4px 8px 8px; border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); background: var(--cr-fond); }
  .groupe legend { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 0 4px; color: var(--cr-texte-discret); }
  .non, .casse { display: inline-flex; align-items: center; gap: 3px; color: var(--cr-texte-discret); font-size: 12px; }
  .actions { margin-left: auto; display: inline-flex; gap: 2px; }
  .petit { padding: 1px 6px; font-size: 12px; }
  .ajouts { display: flex; gap: 6px; margin-top: 2px; }
  .ajouts button { font-size: 12px; padding: 2px 8px; }
  .discret { color: var(--cr-texte-discret); font-size: 12px; margin: 0; }
  .non-evalue { color: var(--cr-erreur); cursor: help; }
  .imbrique .critere { padding-left: 4px; }
</style>
