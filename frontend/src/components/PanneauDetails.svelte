<script lang="ts">
  // Panneau de détails du livre sélectionné : résumé immédiat, puis détail complet (detail_livre).
  import { detailLivre, type LivreDetail, type LivreResume } from '../lib/api';

  interface Props {
    livre: LivreResume | undefined;
    onouvrir: () => void;
  }
  let { livre, onouvrir }: Props = $props();

  let detail = $state<LivreDetail | null>(null);
  let erreur = $state('');
  let sequence = 0;

  // Dérivé par valeur : un rechargement des pages (nouveaux objets, même id) ne relance pas l'appel.
  const id = $derived(livre?.id);

  $effect(() => {
    const cible = id;
    const n = ++sequence;
    detail = null;
    erreur = '';
    if (cible === undefined) return;
    detailLivre(cible)
      .then((d) => { if (n === sequence) detail = d; })
      .catch((e) => { if (n === sequence) erreur = String(e); });
  });

  const MOIS = ['janv.', 'févr.', 'mars', 'avr.', 'mai', 'juin', 'juil.', 'août', 'sept.', 'oct.', 'nov.', 'déc.'];
  const taille = (o: number) => (o >= 1048576 ? `${(o / 1048576).toFixed(1)} Mo` : `${Math.round(o / 1024)} Ko`);

  const champs = $derived.by(() => {
    const d = detail;
    if (!d) return [] as Array<[string, string]>;
    const date = d.annee ? (d.mois ? `${MOIS[d.mois - 1]} ${d.annee}` : String(d.annee)) : '';
    const lignes: Array<[string, string | undefined]> = [
      ['Série', d.serie],
      ['Numéro', d.numero],
      ['Volume', d.volume !== undefined ? String(d.volume) : undefined],
      ['Collection', d.collection],
      ['Éditeur', d.editeur],
      ['Date', date],
      ['Scénario', d.scenariste],
      ['Dessin', d.dessinateur],
      ['Encrage', d.encreur],
      ['Couleurs', d.coloriste],
      ['Lettrage', d.lettreur],
      ['Arc narratif', d.arcNarratif],
      ['Genre', d.genre],
      ['Langue', d.langue],
      ['Format', d.format],
      ['Pages', String(d.pages)],
      ['Taille', d.tailleFichier ? taille(d.tailleFichier) : undefined],
      ['Lecture', d.progression !== undefined && d.progression > 0 ? `${Math.round(d.progression)} %` : undefined],
      ['Note', d.note ? `${d.note} / 5` : undefined],
      ['ISBN', d.isbn],
    ];
    return lignes.filter((l): l is [string, string] => !!l[1]);
  });
</script>

<aside class="panneau" aria-label="Détails du livre" data-testid="details">
  {#if !livre}
    <p class="vide">Sélectionnez un livre pour afficher ses détails.</p>
  {:else}
    <div class="couv"><img src={livre.urlCouverture} alt="Couverture de {livre.titre}" draggable="false" /></div>
    <h2>{livre.titre}</h2>
    <p class="etat">
      <span class:lu={livre.lu}>{livre.lu ? 'Lu' : 'Non lu'}</span>
      {#if detail && (detail.manquant || !detail.fichierPresent)}<span class="manquant">Fichier manquant</span>{/if}
    </p>
    <button class="ouvrir" onclick={onouvrir}>Lire</button>
    {#if erreur}
      <p class="erreur" role="alert">{erreur}</p>
    {:else if !detail}
      <p class="discret">Chargement…</p>
    {:else}
      <dl>
        {#each champs as [nom, valeur] (nom)}
          <dt>{nom}</dt><dd>{valeur}</dd>
        {/each}
      </dl>
      {#if detail.resume}<p class="resume">{detail.resume}</p>{/if}
      {#if detail.fichier}<p class="chemin" title={detail.fichier}>{detail.fichier}</p>{/if}
    {/if}
  {/if}
</aside>

<style>
  .panneau {
    width: 320px; flex: none; overflow-y: auto; padding: 16px; background: var(--cr-fond-panneau);
    border-left: 1px solid var(--cr-bordure);
  }
  .vide, .discret { color: var(--cr-texte-discret); }
  .couv { border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); box-shadow: var(--cr-ombre); overflow: hidden; background: var(--cr-fond-survol); aspect-ratio: 2 / 3; max-width: 200px; margin: 0 auto 12px; }
  .couv img { width: 100%; height: 100%; object-fit: cover; display: block; }
  h2 { margin: 0 0 4px; font-size: 17px; text-align: center; overflow-wrap: anywhere; }
  .etat { display: flex; justify-content: center; gap: 10px; margin: 0 0 10px; color: var(--cr-texte-discret); }
  .lu { color: var(--cr-accent); font-weight: 600; }
  .manquant, .erreur { color: var(--cr-erreur); }
  .ouvrir {
    display: block; margin: 0 auto 14px; font: inherit; cursor: pointer; padding: 6px 28px;
    background: var(--cr-accent); color: var(--cr-accent-texte); border: 1px solid var(--cr-accent); border-radius: var(--cr-rayon);
  }
  .ouvrir:hover { filter: brightness(1.1); }
  .ouvrir:focus-visible { outline: 2px solid var(--cr-accent); outline-offset: 2px; }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 4px 12px; margin: 0 0 12px; }
  dt { color: var(--cr-texte-discret); }
  dd { margin: 0; overflow-wrap: anywhere; }
  .resume { margin: 0 0 12px; line-height: 1.45; }
  .chemin { margin: 0; font-size: 12px; color: var(--cr-texte-discret); word-break: break-all; }
</style>
