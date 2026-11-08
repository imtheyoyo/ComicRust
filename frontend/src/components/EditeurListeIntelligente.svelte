<script lang="ts" module>
  import type { CatalogueCriteres } from '../lib/api';
  /** Catalogue des critères, demandé une fois au backend. */
  let catalogueMemorise: CatalogueCriteres | null = null;
</script>

<script lang="ts">
  // Éditeur de liste intelligente (« Liste intelligente » de ComicRack, smart_editor.rs de la version
  // egui ; docs/tauri/ipc-listes.md) : nom, description, « correspond à tous / au moins un des
  // critères suivants, dans : [liste de départ] » et « pas dans cette liste », critères et groupes
  // imbriqués, limite, livres retirés à la main dans ComicRack, aperçu du nombre de livres (calculé
  // par le backend 250 ms après la dernière modification). Consultation seule en lecture seule.
  // Échap ne ferme pas s'il y a des modifications (Annuler pour fermer).
  import { onDestroy, onMount } from 'svelte';
  import {
    apercuCriteres, catalogueCriteres, enregistrerCriteres, lireCriteres, messageErreur,
    type Apercu, type CriteresListe, type ListeDepart,
  } from '../lib/api';
  import type { EtatListes } from '../lib/listes.svelte';
  import { modale } from '../lib/modale';
  import NoeudsCriteres from './NoeudsCriteres.svelte';

  interface Props {
    /** Liste existante ; absent : nouvelle liste, à la place `place`. */
    id?: string;
    place?: { parent?: string; position?: number; apresListe?: string };
    listes: EtatListes;
    onfermer: () => void;
  }
  let { id: idInitial, place, listes, onfermer }: Props = $props();

  const DELAI_APERCU = 250;

  /** Identifiant de la liste (une nouvelle liste en reçoit un au premier « Appliquer »). */
  let id = $state<string | undefined>(undefined);
  let catalogue = $state<CatalogueCriteres | null>(null);
  let criteres = $state<CriteresListe | null>(null);
  let listesDepart = $state<ListeDepart[]>([]);
  let lectureSeule = $state<string | undefined>(undefined);
  let initial = $state('');
  let erreur = $state('');
  let apercu = $state<Apercu | null>(null);
  let calcul = $state(false);
  let enregistrement = $state(false);
  let erreurEnregistrement = $state('');
  let minuterie: ReturnType<typeof setTimeout> | undefined;
  let sequence = 0;

  async function lire(cible?: string) {
    const e = await lireCriteres(cible);
    id = e.id;
    criteres = e.criteres;
    listesDepart = e.listesDepart;
    lectureSeule = e.lectureSeule;
    initial = JSON.stringify(e.criteres);
  }

  onMount(() => {
    id = idInitial;
    void (async () => {
      try {
        catalogueMemorise ??= await catalogueCriteres();
        catalogue = catalogueMemorise;
        await lire(idInitial);
      } catch (e) {
        erreur = messageErreur(e);
      }
    })();
  });
  onDestroy(() => clearTimeout(minuterie));

  const cle = $derived(criteres ? JSON.stringify(criteres) : '');
  const modifie = $derived(!!criteres && cle !== initial);

  // Aperçu recalculé 250 ms après la dernière modification.
  $effect(() => {
    const k = cle;
    if (!k) return;
    clearTimeout(minuterie);
    const n = ++sequence;
    calcul = true;
    const copie = JSON.parse(k) as CriteresListe;
    const cible = id;
    minuterie = setTimeout(async () => {
      try {
        const r = await apercuCriteres(copie, cible);
        if (n === sequence) apercu = r;
      } catch (e) {
        if (n === sequence) apercu = { inconnu: messageErreur(e), erreurs: [] };
      } finally {
        if (n === sequence) calcul = false;
      }
    }, DELAI_APERCU);
  });

  const erreurs = $derived(apercu?.erreurs ?? []);
  const peutEnregistrer = $derived(!!criteres && !lectureSeule && !enregistrement && !!criteres.nom.trim() && erreurs.length === 0 && !calcul);
  const raisonRefus = $derived(
    lectureSeule ? lectureSeule : enregistrement ? 'Enregistrement en cours…' : !criteres?.nom.trim() ? 'Nom vide' : erreurs.length ? "Corrige d'abord les erreurs signalées" : '',
  );

  /** OK (fermer après succès) ou Appliquer (garder la fenêtre, relue depuis le backend). */
  async function enregistrer(fermer: boolean) {
    if (!criteres || !peutEnregistrer) return;
    enregistrement = true;
    erreurEnregistrement = '';
    const c = $state.snapshot(criteres) as CriteresListe;
    const cible = id;
    const nom = c.nom.trim();
    const r = await listes.executer(cible ? `Modification de « ${nom} »` : `Création de « ${nom} »`,
      () => enregistrerCriteres(c, cible ? { id: cible } : { ...place }));
    enregistrement = false;
    if (!r) {
      erreurEnregistrement = listes.info?.texte ?? 'Enregistrement refusé.';
      return;
    }
    if (fermer) {
      onfermer();
      return;
    }
    try {
      await lire(r.creee ?? cible);
    } catch (e) {
      erreurEnregistrement = messageErreur(e);
    }
  }

  function basculerLimite(actif: boolean) {
    if (!criteres) return;
    criteres.limite = actif ? { valeur: '25', unite: 'livres', selection: 'hasard' } : null;
  }

  const pluriel = (n: number) => (n > 1 ? 's' : '');
  /** Échap : seulement sans modification et hors enregistrement (comme « Infos du livre »). */
  const peutFermer = () => !modifie && !enregistrement;
</script>

<dialog class="editeur" aria-labelledby="titre-editeur" use:modale={{ peutFermer, fermer: onfermer, ouvrirAuMontage: true }}
  onkeydown={(e) => { if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); void enregistrer(true); } }}
  data-testid="editeur-liste">
  <h2 id="titre-editeur">{id ? `Liste intelligente — ${criteres?.nom ?? '…'}` : 'Nouvelle liste intelligente'}</h2>
  {#if erreur}
    <p class="erreur" role="alert">{erreur}</p>
    <div class="pied"><button onclick={onfermer}>Fermer</button></div>
  {:else if !criteres || !catalogue}
    <p class="discret">Chargement…</p>
  {:else}
    {#if lectureSeule}<p class="lecture-seule" role="status">🔒 Consultation seulement : {lectureSeule}</p>{/if}
    <fieldset class="corps" disabled={!!lectureSeule}>
      <div class="entete">
        <label for="nom-liste">Nom</label>
        <input id="nom-liste" type="text" bind:value={criteres.nom} data-testid="nom-liste" />
        <label for="description-liste">Description</label>
        <input id="description-liste" type="text" bind:value={criteres.description} />
      </div>
      <div class="correspond">
        Correspond à
        <select bind:value={criteres.mode} aria-label="Combinaison des critères" data-testid="mode-liste">
          <option value="tous">tous</option>
          <option value="auMoinsUn">au moins un</option>
        </select>
        des critères suivants, dans :
        <select value={criteres.depart ?? ''} onchange={(e) => { if (criteres) criteres.depart = e.currentTarget.value || null; }} aria-label="Liste de départ" data-testid="base-liste">
          <option value="">Toute la bibliothèque</option>
          {#each listesDepart as b (b.id)}<option value={b.id}>{'  '.repeat(b.profondeur)}{b.type === 'dossier' ? '📁 ' : ''}{b.nom}</option>{/each}
        </select>
        <label class="case" title="Livres de toute la bibliothèque qui ne sont pas dans la liste de départ">
          <input type="checkbox" bind:checked={criteres.pasDansDepart} disabled={!criteres.depart} /> pas dans cette liste
        </label>
      </div>
      <div class="criteres">
        <NoeudsCriteres noeuds={criteres.noeuds} {catalogue} onerreur={(m) => (erreurEnregistrement = m)} />
      </div>
      <div class="limite">
        <label class="case"><input type="checkbox" checked={!!criteres.limite} onchange={(e) => basculerLimite(e.currentTarget.checked)} data-testid="limite-active" /> Limiter à</label>
        {#if criteres.limite}
          <input type="text" inputmode="numeric" bind:value={criteres.limite.valeur} aria-label="Valeur de la limite" class="nombre" data-testid="limite-valeur" />
          <select bind:value={criteres.limite.unite} aria-label="Unité de la limite">
            <option value="livres">livres</option>
            <option value="mo">Mo</option>
            <option value="go">Go</option>
          </select>
          sélectionnés
          <select bind:value={criteres.limite.selection} aria-label="Sélection">
            <option value="hasard">au hasard</option>
            <option value="position">par position</option>
            <option value="serie">par série</option>
          </select>
        {/if}
      </div>
      {#if criteres.retires > 0}
        <div class="retires">
          {criteres.retires} livre{pluriel(criteres.retires)} retiré{pluriel(criteres.retires)} à la main dans ComicRack
          <label class="case"><input type="checkbox" bind:checked={criteres.reinitialiserRetires} data-testid="reinitialiser-retires" /> Réinitialiser (les remettre dans la liste)</label>
        </div>
      {/if}
    </fieldset>

    <div class="apercu" aria-live="polite" data-testid="apercu-liste">
      {#if erreurs.length}
        <p class="erreur">Aperçu : corrige les critères signalés.</p>
        <ul class="erreurs">{#each erreurs as e, k (k)}<li>{e}</li>{/each}</ul>
      {:else if apercu?.inconnu}
        <p class="erreur">Aperçu impossible : {apercu.inconnu}</p>
      {:else if apercu?.nbLivres !== undefined}
        <p class:calcul>Aperçu : <strong>{apercu.nbLivres.toLocaleString('fr-FR')}</strong> livre{pluriel(apercu.nbLivres)} trouvé{pluriel(apercu.nbLivres)}{calcul ? ' (mise à jour…)' : ''}</p>
      {:else}
        <p class="discret">Calcul de l'aperçu…</p>
      {/if}
      {#if criteres.limite?.selection === 'hasard'}
        <p class="discret">Sélection « au hasard » : le tirage de ComicRust peut différer de celui de ComicRack.</p>
      {/if}
    </div>
    {#if erreurEnregistrement}<p class="erreur" role="alert">{erreurEnregistrement}</p>{/if}

    <div class="pied">
      {#if lectureSeule}
        <button class="principal" onclick={onfermer}>Fermer</button>
      {:else}
        <button onclick={onfermer} disabled={enregistrement}>Annuler</button>
        <button disabled={!peutEnregistrer || !modifie} title={raisonRefus} onclick={() => void enregistrer(false)} data-testid="appliquer-liste">Appliquer</button>
        <button class="principal" disabled={!peutEnregistrer} title={raisonRefus} onclick={() => void enregistrer(true)} data-testid="enregistrer-liste">
          {enregistrement ? 'Enregistrement…' : 'OK'}</button>
      {/if}
    </div>
  {/if}
</dialog>

<style>
  .editeur { width: min(900px, calc(100vw - 32px)); }
  .corps { border: 0; padding: 0; margin: 0; min-width: 0; display: grid; gap: 10px; }
  .entete { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 6px 10px; align-items: center; }
  .entete label { color: var(--cr-texte-discret); }
  .correspond, .limite, .retires { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .retires { color: var(--cr-texte-discret); font-size: 12px; }
  .case { display: inline-flex; align-items: center; gap: 4px; }
  .criteres { max-height: 46vh; overflow: auto; padding: 6px 8px; border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); background: var(--cr-fond-panneau); }
  .nombre { width: 80px; }
  .apercu { margin-top: 10px; min-height: 22px; }
  .apercu p { margin: 0; }
  .apercu .calcul { opacity: 0.7; }
  .erreurs { margin: 4px 0 0; padding-left: 18px; color: var(--cr-erreur); font-size: 12px; }
  .lecture-seule { margin: 0 0 10px; padding: 6px 8px; border-radius: var(--cr-rayon); background: var(--cr-fond-survol); color: var(--cr-texte-discret); }
  .erreur { color: var(--cr-erreur); white-space: pre-line; }
  .discret { color: var(--cr-texte-discret); font-size: 12px; }
  .pied { display: flex; justify-content: flex-end; gap: 8px; margin-top: 12px; }
  button.principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); font-weight: 600; }
</style>
