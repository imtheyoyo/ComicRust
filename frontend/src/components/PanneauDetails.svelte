<script lang="ts">
  // Panneau de détails du livre sélectionné : résumé immédiat, puis détail complet (detail_livre).
  import { detailLivre, type LivreDetail, type LivreResume, messageErreur } from '../lib/api';

  interface Props {
    livre: LivreResume | undefined;
    /** Largeur en px (réglable, mémorisée). */
    largeur?: number;
    onouvrir: () => void;
    /** Ouvre « Infos du livre » (Ctrl+I). */
    onmodifier?: () => void;
    /** Nombre de livres sélectionnés (au-delà de 1 : rappel de la sélection multiple). En vue
     *  empilée : total des livres des piles sélectionnées. */
    nbSelection?: number;
    /** Vue empilée par série : résumé des piles sélectionnées (remplace le détail d'un livre, qui
     *  serait trompeur : une pile n'est pas un livre). `pile` : la pile quand il n'y en a qu'une. */
    piles?: { nbPiles: number; pile?: { serie: string; nbLivres: number; nbLus: number; urlCouverture: string } };
    /** Incrémenté quand les fiches ont changé (édition) : le détail est relu. */
    rafraichir?: number;
  }
  let { livre, largeur = 320, onouvrir, onmodifier, nbSelection = 0, piles, rafraichir = 0 }: Props = $props();

  const pluriel = (n: number, mot: string) => `${n.toLocaleString('fr-FR')} ${mot}${n > 1 ? 's' : ''}`;

  let detail = $state<LivreDetail | null>(null);
  let erreur = $state('');
  let sequence = 0;

  // Dérivé par valeur : un rechargement des pages (nouveaux objets, même id) ne relance pas l'appel.
  const id = $derived(livre?.id);

  let dernierId: string | undefined;
  $effect(() => {
    const cible = id;
    rafraichir;
    const n = ++sequence;
    // Même livre relu après une modification : l'ancien détail reste affiché jusqu'à la réponse.
    if (cible !== dernierId) detail = null;
    dernierId = cible;
    erreur = '';
    if (cible === undefined) return;
    detailLivre(cible)
      .then((d) => { if (n === sequence) detail = d; })
      .catch((e) => { if (n === sequence) erreur = messageErreur(e); });
  });

  const MOIS = ['janv.', 'févr.', 'mars', 'avr.', 'mai', 'juin', 'juil.', 'août', 'sept.', 'oct.', 'nov.', 'déc.'];
  const taille = (o: number) => (o >= 1048576 ? `${(o / 1048576).toFixed(1)} Mo` : `${Math.round(o / 1024)} Ko`);

  /** Lecture affichée : `null` tant que le détail n'est pas là (le résumé de la liste sert alors). */
  const lecture = $derived.by(() => {
    const d = detail;
    if (!d) return null;
    const pct = Math.round(d.progression ?? 0);
    const lu = d.lu || pct >= 100;
    const origine = d.sourceLecture === 'comicrust' ? ' dans ComicRust' : '';
    // Relecture d'un livre lu : il reste « Lu », la barre suit la relecture (C-06, tauri-v0.7.3).
    const r = d.relecture;
    if (lu && r) {
      return { pct: r.pourcentage, texte: `Lu${origine} — relecture en cours : ${r.pourcentage} % (page ${r.page} / ${r.total})`, lu: true };
    }
    if (lu) return { pct: 100, texte: `Lu${origine}`, lu: true };
    if (pct <= 0) return { pct: 0, texte: 'Non lu', lu: false };
    const page = d.pageLue ? ` (page ${d.pageLue}${d.pages ? ` / ${d.pages}` : ''})` : '';
    return { pct, texte: `Lu${origine} : ${pct} %${page}`, lu: false };
  });

  /** Note ComicRack 0–5, arrondie au demi (les demi-étoiles existent dans la fiche). */
  const note = $derived(detail?.note ? Math.min(5, Math.max(0, Math.round(detail.note * 2) / 2)) : 0);
  const texteNote = $derived(`Note : ${note.toLocaleString('fr-FR')} sur 5`);

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
      ['ISBN', d.isbn],
    ];
    return lignes.filter((l): l is [string, string] => !!l[1]);
  });
</script>

<aside class="panneau" aria-label="Détails du livre" data-testid="details" style:width="{largeur}px">
  {#if piles}
    {#if piles.nbPiles === 0}
      <p class="vide" data-testid="details-piles-vide">Sélectionnez une pile pour afficher ses détails.</p>
    {:else if piles.pile}
      {@const p = piles.pile}
      <div class="couv"><img src={p.urlCouverture} alt="Couverture de la série {p.serie}" draggable="false" /></div>
      <h2>{p.serie}</h2>
      <div class="etat" data-testid="details-pile">
        <span class="lecture">{pluriel(p.nbLivres, 'livre')}, {pluriel(p.nbLus, 'lu')}</span>
        <div class="barre-lecture" role="progressbar" aria-label="Livres lus de la série" aria-valuemin="0"
          aria-valuemax={p.nbLivres} aria-valuenow={p.nbLus}><div style:width="{p.nbLivres ? (p.nbLus / p.nbLivres) * 100 : 0}%"></div></div>
      </div>
      <div class="actions">
        <button class="ouvrir" onclick={onouvrir} data-testid="details-ouvrir-pile">Ouvrir la pile</button>
        {#if onmodifier}
          <button onclick={onmodifier} title="Modifier les infos (Ctrl+I)" data-testid="details-modifier"
            >✏ Modifier{nbSelection > 1 ? ` (${nbSelection})` : ''}</button>
        {/if}
      </div>
    {:else}
      <h2 data-testid="details-piles-multi">{pluriel(piles.nbPiles, 'pile')} sélectionnée{piles.nbPiles > 1 ? 's' : ''}</h2>
      <p class="discret multi">{pluriel(nbSelection, 'livre')} au total</p>
      <div class="actions">
        {#if onmodifier}
          <button onclick={onmodifier} title="Modifier les infos (Ctrl+I)" data-testid="details-modifier"
            >✏ Modifier{nbSelection > 1 ? ` (${nbSelection})` : ''}</button>
        {/if}
      </div>
    {/if}
  {:else if !livre}
    <p class="vide">Sélectionnez un livre pour afficher ses détails.</p>
  {:else}
    <div class="couv"><img src={livre.urlCouverture} alt="Couverture de {livre.titre}" draggable="false" /></div>
    <h2>{livre.titre}</h2>
    <div class="etat">
      {#if lecture}
        <span class="lecture" class:lu={lecture.lu} data-testid="details-lecture">{lecture.texte}</span>
        <div class="barre-lecture" role="progressbar" aria-label="Progression de lecture" aria-valuemin="0" aria-valuemax="100"
          aria-valuenow={lecture.pct} data-testid="details-barre"><div style:width="{lecture.pct}%"></div></div>
      {:else}
        <span class="lecture" class:lu={livre.lu}>{livre.lu ? 'Lu' : 'Non lu'}</span>
      {/if}
      {#if detail && (detail.manquant || !detail.fichierPresent)}<span class="manquant">Fichier manquant</span>{/if}
      {#if note > 0}
        <span class="etoiles" role="img" aria-label={texteNote} title={texteNote} data-testid="details-note">
          {#each [1, 2, 3, 4, 5] as rang (rang)}
            <span class="etoile" aria-hidden="true">★<span class="pleine" style:width="{Math.min(1, Math.max(0, note - rang + 1)) * 100}%">★</span></span>
          {/each}
        </span>
      {/if}
    </div>
    <div class="actions">
      <button class="ouvrir" onclick={onouvrir}>Lire</button>
      {#if onmodifier}
        <button onclick={onmodifier} title="Modifier les infos (Ctrl+I)" data-testid="details-modifier"
          >✏ Modifier{nbSelection > 1 ? ` (${nbSelection})` : ''}</button>
      {/if}
    </div>
    {#if nbSelection > 1}<p class="discret multi">{nbSelection.toLocaleString('fr-FR')} livres sélectionnés (détails du livre courant)</p>{/if}
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
    width: 320px; max-width: 60vw; flex: none; overflow-y: auto; padding: 16px; background: var(--cr-fond-panneau);
    border-left: 1px solid var(--cr-bordure);
  }
  .vide, .discret { color: var(--cr-texte-discret); }
  .couv { border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); box-shadow: var(--cr-ombre); overflow: hidden; background: var(--cr-fond-survol); aspect-ratio: 2 / 3; max-width: 200px; margin: 0 auto 12px; }
  .couv img { width: 100%; height: 100%; object-fit: cover; display: block; }
  h2 { margin: 0 0 4px; font-size: 17px; text-align: center; overflow-wrap: anywhere; }
  .etat { display: flex; flex-direction: column; align-items: center; gap: 6px; margin: 0 0 10px; color: var(--cr-texte-discret); }
  .lecture { text-align: center; }
  .lu { color: var(--cr-accent); font-weight: 600; }
  .barre-lecture { width: 70%; max-width: 200px; height: 6px; overflow: hidden; background: var(--cr-fond-survol); border: 1px solid var(--cr-bordure); border-radius: 4px; }
  .barre-lecture > div { height: 100%; background: var(--cr-accent); }
  /* Étoiles : grises (variable de bordure du thème) sous une étoile jaune rognée à la note. */
  .etoiles { display: inline-flex; font-size: 20px; line-height: 1; }
  .etoile { position: relative; display: inline-block; color: var(--cr-bordure); }
  .pleine { position: absolute; left: 0; top: 0; overflow: hidden; white-space: nowrap; color: var(--cr-etoile, #f2b01e); }
  .manquant, .erreur { color: var(--cr-erreur); }
  .actions { display: flex; justify-content: center; gap: 8px; margin: 0 0 14px; }
  .multi { text-align: center; font-size: 12px; margin: -6px 0 12px; }
  .ouvrir {
    display: block; font: inherit; cursor: pointer; padding: 6px 28px;
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
