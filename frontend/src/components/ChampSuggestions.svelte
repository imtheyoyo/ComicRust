<script lang="ts">
  // Champ texte avec suggestions (comme `suggest_edit` de la version egui) : la saisie en cours
  // (dernière valeur d'une liste) est complétée par les valeurs de la bibliothèque, que calcule le
  // backend (proposer_valeurs) ; ↑/↓ puis Entrée, ou clic. « ▾ » montre toutes les valeurs.
  import { proposerValeurs, type Proposition } from '../lib/api';

  interface Props {
    /** Nom du champ (`Series`, `Writer`…). */
    champ: string;
    valeur: string;
    /** Le backend propose des valeurs pour ce champ. */
    suggestions: boolean;
    placeholder?: string;
    disabled?: boolean;
    id?: string;
    libelle: string;
    onsaisie: (v: string) => void;
  }
  let { champ, valeur, suggestions, placeholder = '', disabled = false, id, libelle, onsaisie }: Props = $props();

  let liste = $state<Proposition[]>([]);
  let ouverte = $state(false);
  let tout = $state(false);
  let choix = $state(0);
  let champTexte = $state<HTMLInputElement>();
  let minuterie: ReturnType<typeof setTimeout> | undefined;
  let sequence = 0;
  const idListe = `sugg-${Math.random().toString(36).slice(2, 9)}`;

  async function charger(texte: string, toutes: boolean) {
    const n = ++sequence;
    try {
      const r = await proposerValeurs(champ, texte, toutes);
      if (n !== sequence) return;
      liste = r;
      choix = 0;
      ouverte = r.length > 0;
      tout = toutes;
    } catch {
      if (n === sequence) { liste = []; ouverte = false; }
    }
  }

  function saisir(v: string) {
    onsaisie(v);
    if (!suggestions) return;
    clearTimeout(minuterie);
    minuterie = setTimeout(() => void charger(v, tout), 120);
  }

  function choisir(p: Proposition) {
    onsaisie(p.texte);
    fermer();
    champTexte?.focus();
    // Curseur en fin de texte (après le rendu de la nouvelle valeur).
    queueMicrotask(() => champTexte?.setSelectionRange(p.texte.length, p.texte.length));
  }

  function fermer() {
    ++sequence;
    clearTimeout(minuterie);
    ouverte = false;
    tout = false;
    liste = [];
  }

  function clavier(e: KeyboardEvent) {
    if (!ouverte || liste.length === 0) return;
    if (e.key === 'ArrowDown') choix = Math.min(liste.length - 1, choix + 1);
    else if (e.key === 'ArrowUp') choix = Math.max(0, choix - 1);
    else if (e.key === 'Enter') choisir(liste[choix]);
    else if (e.key === 'Escape') fermer();
    else return;
    e.preventDefault();
    e.stopPropagation(); // Échap ne ferme pas la fenêtre, Entrée ne valide pas
    document.getElementById(`${idListe}-${choix}`)?.scrollIntoView({ block: 'nearest' });
  }

  function basculerTout() {
    if (ouverte && tout) fermer();
    else void charger(valeur, true);
    champTexte?.focus();
  }
</script>

<div class="champ-sugg">
  <input
    bind:this={champTexte}
    {id}
    type="text"
    value={valeur}
    {placeholder}
    {disabled}
    autocomplete="off"
    spellcheck="false"
    aria-label={libelle}
    role={suggestions ? 'combobox' : undefined}
    aria-autocomplete={suggestions ? 'list' : undefined}
    aria-expanded={suggestions ? ouverte : undefined}
    aria-controls={suggestions ? idListe : undefined}
    aria-activedescendant={ouverte ? `${idListe}-${choix}` : undefined}
    oninput={(e) => saisir(e.currentTarget.value)}
    onkeydown={clavier}
    onblur={() => setTimeout(fermer, 120)}
    data-champ={champ}
  />
  {#if suggestions}
    <button type="button" class="tout" tabindex="-1" {disabled} onmousedown={(e) => e.preventDefault()} onclick={basculerTout}
      title="Valeurs déjà présentes dans la bibliothèque" aria-label="Valeurs de la bibliothèque pour {libelle}">▾</button>
  {/if}
  {#if ouverte}
    <ul class="liste" id={idListe} role="listbox" aria-label="Suggestions pour {libelle}" data-testid="suggestions">
      {#each liste as p, k (p.valeur)}
        <!-- Le clavier reste dans le champ (aria-activedescendant). -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <li id="{idListe}-{k}" role="option" aria-selected={k === choix} class:choisie={k === choix}
          onmousedown={(e) => e.preventDefault()} onclick={() => choisir(p)}>{p.valeur}</li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .champ-sugg { position: relative; display: flex; gap: 4px; flex: 1; min-width: 0; }
  .champ-sugg input { flex: 1; min-width: 0; }
  .tout { padding: 2px 7px; flex: none; }
  .liste {
    position: absolute; z-index: 20; left: 0; right: 30px; top: calc(100% + 2px); margin: 0; padding: 3px 0; list-style: none;
    max-height: 220px; overflow-y: auto; background: var(--cr-fond-panneau); border: 1px solid var(--cr-bordure);
    border-radius: var(--cr-rayon); box-shadow: var(--cr-ombre);
  }
  .liste li { padding: 4px 10px; cursor: pointer; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .liste li:hover { background: var(--cr-fond-survol); }
  .liste li.choisie { background: var(--cr-accent); color: var(--cr-accent-texte); }
</style>
