<script lang="ts">
  // Fenêtre « Infos du livre » (comme ui/info_editor.rs de la version egui, d'après ComicBookDialog
  // et MultipleComicBooksDialog de ComicRack). Contrat : docs/tauri/ipc-edition.md.
  // - Un livre : Précédent / Suivant dans la vue (modifications enregistrées avant de changer).
  // - Plusieurs livres : un champ aux valeurs différentes affiche « (valeurs multiples) » et n'est
  //   envoyé que s'il est saisi ou vidé explicitement (✖) ; seuls les champs touchés partent.
  // - OK : enregistre puis ferme (rien à enregistrer : ferme) ; Appliquer : enregistre, garde la
  //   fenêtre et relit les fiches (un OK ensuite n'écrit donc rien de plus) ; Annuler : ferme.
  // Aucune logique métier ici : valeurs, validation et suggestions viennent du backend.
  import { onDestroy, onMount, tick } from 'svelte';
  import {
    ajouterValeurPerso, appliquerModifications, annulerOperation, ecouter, lireFiches, messageErreur, verifierChamps,
    type BilanOperation, type ChampFiche, type ChampModifie, type FicheEdition, type PageLivre, type ProgressionEdition,
  } from '../lib/api';
  import { connexion } from '../lib/connexion.svelte';
  import { modale } from '../lib/modale';
  import ChampSuggestions from './ChampSuggestions.svelte';
  import OngletPages from './OngletPages.svelte';
  import ResultatsLot, { pourcent } from './ResultatsLot.svelte';
  import MenuContextuel, { type EntreeMenu } from './MenuContextuel.svelte';
  import { scripts } from '../lib/scripts.svelte';
  import { texteValeur } from '../lib/formulaireScript';

  interface Props {
    ids: string[];
    /** Livre voisin dans la vue (un seul livre) : rend son identifiant (la sélection suit), ou null. */
    onvoisin?: (pas: -1 | 1) => Promise<string | null>;
    /** Existe-t-il un voisin (pour griser Précédent / Suivant) ? */
    avoisin?: (pas: -1 | 1) => boolean;
    onfermer: () => void;
  }
  let { ids: idsInitiaux, onvoisin, avoisin, onfermer }: Props = $props();

  const MULTIPLE = '(valeurs multiples)';
  const CHOIX_MULTIPLE = '\u0000multiple';

  let ids = $state<string[]>([]);
  let fiche = $state<FicheEdition | null>(null);
  /** Texte courant de chaque champ. */
  let textes = $state<Record<string, string>>({});
  /** Champs multiples vidés explicitement (✖ « vider pour tous les livres »). */
  let vides = $state<Record<string, boolean>>({});
  let erreurs = $state<Record<string, string>>({});
  let onglet = $state(0);
  let ecrireFichier = $state(false);
  let chargement = $state(true);
  let enregistrement = $state(false);
  let progression = $state<ProgressionEdition | null>(null);
  let bilan = $state<BilanOperation | null>(null);
  let message = $state<{ texte: string; erreur: boolean } | null>(null);
  /** Garde-fou : message du backend et suite à donner une fois confirmé. */
  let confirmation = $state<{ message: string; suite: 'fermer' | 'rester' | -1 | 1 } | null>(null);
  let nouvelleCle = $state('');
  /** Onglet Pages (étape 4, un seul livre) : monté à la première visite puis gardé. */
  let ongletPages = $state<ReturnType<typeof OngletPages>>();
  let pagesVues = $state(false);
  let pagesModifiees = $state(false);
  let pagesOccupe = $state(false);
  /** Scripts `Editor` (tauri-v0.9.0) : menu ouvert et script en cours. */
  let menuScripts = $state<{ x: number; y: number } | null>(null);
  let scriptEnCours = $state(false);
  let minuterieVerif: ReturnType<typeof setTimeout> | undefined;
  let sequenceVerif = 0;
  let desabonner: (() => void) | undefined;

  onMount(() => {
    void charger(idsInitiaux);
    void ecouter<ProgressionEdition>('edition-progression', (p) => {
      if (enregistrement && p.operation === 'modification') progression = p;
    }).then((f) => (desabonner = f));
  });
  onDestroy(() => {
    desabonner?.();
    clearTimeout(minuterieVerif);
  });

  async function charger(cible: string[], garderOnglet = true) {
    chargement = true;
    try {
      const f = await lireFiches(cible);
      fiche = f;
      ids = f.ids;
      textes = Object.fromEntries(f.champs.map((c) => [c.nom, c.valeur]));
      vides = {};
      erreurs = {};
      ecrireFichier = f.ecrireDansFichier && f.ecrireDansFichierPossible;
      const pagesPossibles = !f.multiple && !!f.livres[0]?.fichier;
      if (!garderOnglet || onglet > f.onglets.length || (onglet === f.onglets.length && !pagesPossibles)) onglet = 0;
    } catch (e) {
      message = { texte: messageErreur(e), erreur: true };
    } finally {
      chargement = false;
    }
  }

  const champsParNom = $derived(new Map((fiche?.champs ?? []).map((c) => [c.nom, c])));

  /** Un champ est modifié (règles du contrat). */
  function modifie(c: ChampFiche): boolean {
    const t = textes[c.nom] ?? '';
    if (!c.multiple) return t !== c.valeur;
    return !!vides[c.nom] || t !== '';
  }

  const modifications = $derived.by<ChampModifie[]>(() =>
    (fiche?.champs ?? []).filter(modifie).map((c) => ({ nom: c.nom, valeur: vides[c.nom] ? '' : (textes[c.nom] ?? '') })),
  );
  /** Onglet Pages : un seul livre ayant un fichier (comme la version egui et ComicRack). */
  const avecPages = $derived(!!fiche && !fiche.multiple && !!fiche.livres[0]?.fichier);
  const indicePages = $derived(fiche?.onglets.length ?? 0);
  const surPages = $derived(avecPages && onglet === indicePages);
  $effect(() => { if (surPages) pagesVues = true; });
  const sale = $derived(modifications.length > 0 || (avecPages && pagesModifiees));
  // Base injoignable : saisies gardées, enregistrement désactivé jusqu'au retour de la base.
  const lectureSeule = $derived(fiche?.lectureSeule ?? connexion.motifEcriture);
  const modifiable = $derived(!!fiche && !lectureSeule && !enregistrement && !confirmation);
  const nbErreurs = $derived(Object.keys(erreurs).length);

  // Validation par le backend pendant la saisie (~300 ms après la dernière frappe).
  $effect(() => {
    const champs = modifications;
    clearTimeout(minuterieVerif);
    const n = ++sequenceVerif;
    if (champs.length === 0) { erreurs = {}; return; }
    minuterieVerif = setTimeout(async () => {
      try {
        const r = await verifierChamps(champs);
        if (n === sequenceVerif) erreurs = Object.fromEntries(r.map((e) => [e.nom, e.message]));
      } catch {
        /* vérification indisponible : appliquer_modifications refusera de toute façon */
      }
    }, 300);
  });

  function saisir(c: ChampFiche, v: string) {
    textes[c.nom] = v;
    if (c.multiple && v !== '') vides[c.nom] = false;
    bilan = null;
  }

  function revenir(c: ChampFiche) {
    textes[c.nom] = c.valeur;
    vides[c.nom] = false;
    delete erreurs[c.nom];
  }

  function viderPourTous(c: ChampFiche) {
    textes[c.nom] = '';
    vides[c.nom] = true;
  }

  function choisirListe(c: ChampFiche, v: string) {
    if (v === CHOIX_MULTIPLE) { revenir(c); return; }
    textes[c.nom] = v;
    // « Inconnu » choisi sur un champ multiple : le vider pour tous.
    vides[c.nom] = c.multiple && v === '';
  }

  const valeurListe = (c: ChampFiche) => (c.multiple && !modifie(c) ? CHOIX_MULTIPLE : (textes[c.nom] ?? ''));

  function etoiles(c: ChampFiche): number {
    if (c.multiple && !modifie(c)) return 0;
    const n = Number((textes[c.nom] ?? '').replace(',', '.'));
    return Number.isFinite(n) ? n : 0;
  }

  const indication = (c: ChampFiche) => (c.multiple ? MULTIPLE : (c.indication ?? ''));

  // ───── Enregistrement ─────

  async function enregistrer(suite: 'fermer' | 'rester' | -1 | 1, confirme = false) {
    if (!fiche) return;
    // Pages lues après la fin des opérations en file (Suppr puis Ctrl+Entrée aussitôt : `sale`
    // n'en tient pas encore compte, la modification serait perdue à la fermeture).
    let pages: PageLivre[] | undefined;
    try {
      pages = avecPages && pagesVues ? await ongletPages?.pagesModifiees() : undefined;
    } catch {
      pages = undefined;
    }
    if (modifications.length === 0 && !pages) { await apres(suite); return; }
    enregistrement = true;
    progression = null;
    message = null;
    bilan = null;
    confirmation = null;
    try {
      const b = await appliquerModifications({ ids, champs: modifications, pages, ecrireDansFichier: ecrireFichier, confirme });
      if (b.confirmationRequise) {
        confirmation = { message: b.confirmationRequise.message, suite };
        return;
      }
      if (!b.complet) {
        // Pas tout enregistré : la fenêtre reste, avec le bilan ; les champs restent saisis.
        bilan = b;
        message = { texte: `Pas tout enregistré (${b.resume}).`, erreur: true };
        return;
      }
      if (suite === 'rester') {
        // Fiche relue, et vignettes relues du fichier (réécrit si la case est cochée) : nouvelle séance.
        await Promise.all([charger(ids), pagesVues ? ongletPages?.charger() : undefined]);
        bilan = b;
        message = { texte: `Enregistré : ${b.resume}.`, erreur: false };
      } else await apres(suite);
    } catch (e) {
      message = { texte: messageErreur(e), erreur: true };
    } finally {
      enregistrement = false;
      progression = null;
    }
  }

  async function apres(suite: 'fermer' | 'rester' | -1 | 1) {
    if (suite === 'fermer') { fermer(); return; }
    if (suite === 'rester') return;
    const voisin = await onvoisin?.(suite);
    if (voisin) {
      message = null;
      bilan = null;
      await charger([voisin]);
    }
  }

  /** Le parent retire la fenêtre ({#if}) : pas de dialog.close() ici (voir lib/modale.ts). */
  function fermer() {
    onfermer();
  }

  function annuler() {
    if (enregistrement) return; // le bilan doit arriver
    fermer();
  }

  /** Échap : ferme seulement si rien n'est modifié (sinon Annuler, explicitement), comme egui. */
  const peutFermerParEchap = () => !sale && !pagesOccupe && !enregistrement && !confirmation && !scriptEnCours;

  // ───── Scripts `#@Hook Editor` (tauri-v0.9.0) ─────
  // Le script reçoit les valeurs affichées ; à la fin (script:fin.champs), ses propositions
  // remplissent les champs SANS enregistrer : l'utilisateur vérifie puis OK ou Appliquer.
  const scriptsEditeur = $derived(scripts.scriptsDe('Editor'));
  const entreesScripts = $derived<EntreeMenu[]>(scriptsEditeur.map((s) => ({
    id: s.id, libelle: s.nom, desactive: !!scripts.raisonIndisponible, aide: scripts.raisonIndisponible ?? s.description,
  })));

  function ouvrirMenuScripts(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menuScripts = { x: r.left, y: Math.max(4, r.top - 8 - 32 * Math.max(1, scriptsEditeur.length)) };
  }

  async function lancerScriptEditeur(id: string) {
    const s = scriptsEditeur.find((x) => x.id === id);
    if (!s || !fiche || fiche.multiple || !ids[0]) return;
    message = null;
    scriptEnCours = true;
    const err = await scripts.lancerEditeur(s, ids[0], { ...textes }, (champs) => {
      scriptEnCours = false;
      if (!champs) {
        message = { texte: 'Script arrêté ou en erreur : rien n\'a été repris (voir la fenêtre du script).', erreur: true };
        return;
      }
      const remplis: string[] = [];
      const ignores: string[] = [];
      for (const [nom, v] of Object.entries(champs)) {
        const c = champsParNom.get(nom);
        if (!c) { ignores.push(nom); continue; }
        saisir(c, texteValeur(v));
        remplis.push(c.libelle);
      }
      message = {
        texte: (remplis.length === 0 ? 'Script terminé : aucun champ rempli.'
          : `Script terminé : ${remplis.length} champ(s) rempli(s) (${remplis.join(', ')}). Vérifie, puis OK ou Appliquer pour enregistrer.`)
          + (ignores.length ? `\nIgnoré(s) : ${ignores.join(', ')}.` : ''),
        erreur: false,
      };
    });
    if (err) {
      scriptEnCours = false;
      message = { texte: `Script : ${err}`, erreur: true };
    }
  }

  function clavier(e: KeyboardEvent) {
    // Ctrl+Entrée : OK (comme le bouton par défaut d'une fenêtre Windows).
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && peutOk) {
      e.preventDefault();
      void enregistrer('fermer');
    }
  }

  async function ajouterPerso() {
    if (!fiche || !nouvelleCle.trim()) return;
    try {
      const existantes = fiche.champs.filter((c) => c.nom.startsWith('CustomValue:')).map((c) => c.nom.slice(12));
      const c = await ajouterValeurPerso(nouvelleCle.trim(), existantes);
      fiche.champs.push(c);
      fiche.onglets[fiche.onglets.length - 1].champs.push(c.nom);
      textes[c.nom] = c.valeur;
      nouvelleCle = '';
      message = null;
      await tick();
      document.querySelector<HTMLElement>(`[data-champ="${CSS.escape(c.nom)}"]`)?.focus();
    } catch (e) {
      message = { texte: `Valeur personnalisée : ${messageErreur(e)}`, erreur: true };
    }
  }

  const peutEnregistrer = $derived(modifiable && sale && nbErreurs === 0);
  const peutOk = $derived(!enregistrement && !confirmation && !chargement && (!sale || peutEnregistrer));
  const raisonBlocage = $derived(
    nbErreurs > 0
      ? `Valeur(s) refusée(s) : ${Object.keys(erreurs).map((n) => champsParNom.get(n)?.libelle ?? n).join(', ')}`
      : lectureSeule ? 'Lecture seule' : 'Rien à enregistrer',
  );

  const livres = $derived(fiche?.livres ?? []);
  const taille = (o: number) => (o >= 1048576 ? `${(o / 1048576).toFixed(1).replace('.', ',')} Mo` : `${Math.round(o / 1024)} Ko`);
  const faits = $derived.by(() => {
    const l = livres[0];
    if (!l || livres.length > 1) return '';
    const f: string[] = [];
    if (l.pages > 0) f.push(`${l.pages} pages`);
    if (l.tailleFichier) f.push(taille(l.tailleFichier));
    if (l.progression) f.push(`lu à ${l.progression} %`);
    if (l.infosAEcrire) f.push('infos pas encore écrites dans le fichier');
    return f.join(' · ');
  });
  const titreFenetre = $derived(
    !fiche ? 'Infos du livre' : fiche.multiple ? `Infos de ${livres.length} livres` : `Infos — ${livres[0]?.titre ?? ''}`,
  );
  const nbPerso = $derived(fiche?.onglets[fiche.onglets.length - 1]?.champs.length ?? 0);
  const nomOnglet = (k: number, nom: string) =>
    fiche && k === fiche.onglets.length - 1 && nom === 'Personnalisé' && nbPerso > 0 ? `Personnalisé (${nbPerso})` : nom;
  const estPerso = $derived(!!fiche && fiche.onglets[onglet]?.nom === 'Personnalisé');
</script>

<dialog class="infos" class:large={surPages} aria-labelledby="titre-infos" use:modale={{ peutFermer: peutFermerParEchap, fermer, ouvrirAuMontage: true }} onkeydown={clavier} data-testid="dialogue-infos">
  <h2 id="titre-infos">{titreFenetre}</h2>

  {#if fiche}
    <header class="entete">
      <div class="couvertures" class:pile={livres.length > 1} aria-hidden="true">
        {#each livres.slice(0, 3).reverse() as l, k (l.id)}
          {@const rang = Math.min(3, livres.length) - 1 - k}
          <img src={l.urlCouverture} alt="" draggable="false" style:--decalage="{rang * 12}px" style:z-index={3 - rang} />
        {/each}
      </div>
      <div class="identite">
        {#if fiche.multiple}
          <strong class="nom">{livres.length} livres</strong>
          <span class="discret">
            {livres.slice(0, 4).map((l) => l.titre).join(' · ')}{livres.length > 4 ? ` et ${livres.length - 4} autres` : ''}
          </span>
          <span class="discret petit">Seuls les champs modifiés sont écrits ; « {MULTIPLE} » : les livres diffèrent.</span>
        {:else if livres[0]}
          <strong class="nom">{livres[0].titre}</strong>
          {#if faits}<span class="discret">{faits}</span>{/if}
          {#if livres[0].fichier}<span class="discret petit chemin">{livres[0].fichier}</span>{/if}
        {/if}
      </div>
    </header>

    {#if lectureSeule}
      <div class="lecture-seule" role="note">
        <strong>{connexion.enLigne ? '🔒 Consultation seulement : cette bibliothèque est en lecture seule.' : '⚠ Base injoignable : consultation seulement pour le moment (tes saisies sont gardées).'}</strong>
        <span>{lectureSeule}</span>
      </div>
    {/if}

    <div class="onglets" role="tablist" aria-label="Onglets des infos">
      {#each fiche.onglets as o, k (o.nom)}
        <button role="tab" id="onglet-infos-{k}" aria-selected={onglet === k} aria-controls="panneau-infos" class:actif={onglet === k}
          onclick={() => (onglet = k)}>{nomOnglet(k, o.nom)}</button>
      {/each}
      {#if avecPages}
        <button role="tab" id="onglet-infos-{indicePages}" aria-selected={surPages} aria-controls="panneau-infos" class:actif={surPages}
          class:touche-onglet={pagesModifiees} onclick={() => (onglet = indicePages)} data-testid="onglet-pages-bouton"
          title={pagesModifiees ? 'Pages modifiées : enregistrées par OK ou Appliquer' : 'Ordre, type, rotation des pages'}>{pagesModifiees ? '● ' : ''}Pages</button>
      {/if}
    </div>

    {#if avecPages && pagesVues}
      <div class="panneau-pages" hidden={!surPages} role="tabpanel" id={surPages ? 'panneau-infos' : undefined} aria-labelledby="onglet-infos-{indicePages}">
        <OngletPages bind:this={ongletPages} bind:modifie={pagesModifiees} bind:occupe={pagesOccupe} id={ids[0]} fichier={livres[0]?.fichier}
          actif={!enregistrement && !confirmation && !chargement} />
      </div>
    {/if}
    <div class="panneau-champs" id={surPages ? undefined : 'panneau-infos'} role="tabpanel" aria-labelledby="onglet-infos-{onglet}" hidden={surPages}>
      {#if estPerso}
        <p class="discret petit">Valeurs personnalisées (« Personnalisé » de ComicRack), utilisées par les scripts et les listes intelligentes.</p>
        {#if fiche.onglets[onglet].champs.length === 0}<p class="discret">Aucune valeur personnalisée.</p>{/if}
      {/if}
      <div class="grille-champs">
        {#each fiche.onglets[onglet]?.champs ?? [] as nom (nom)}
          {@const c = champsParNom.get(nom)}
          {#if c}
            {@const m = modifie(c)}
            {@const idc = `champ-${nom.replace(/[^\w-]/g, '_')}`}
            <label for={idc} class="libelle" class:touche={m} title={m ? 'Modifié : sera enregistré' : undefined}>{m ? '● ' : ''}{c.libelle}</label>
            <div class="controle">
              <div class="ligne-controle">
                {#if c.type === 'texteLong'}
                  <textarea id={idc} rows="5" value={textes[nom]} placeholder={indication(c)} disabled={!modifiable}
                    oninput={(e) => saisir(c, e.currentTarget.value)} data-champ={nom}></textarea>
                {:else if c.choix}
                  <select id={idc} value={valeurListe(c)} disabled={!modifiable} onchange={(e) => choisirListe(c, e.currentTarget.value)} data-champ={nom}>
                    {#if c.multiple && !m}<option value={CHOIX_MULTIPLE}>{MULTIPLE}</option>{/if}
                    {#each c.choix as ch (ch.valeur)}<option value={ch.valeur}>{ch.libelle}</option>{/each}
                  </select>
                {:else if c.type === 'note'}
                  <span class="etoiles" role="group" aria-label="{c.libelle} en étoiles">
                    {#each [1, 2, 3, 4, 5] as s (s)}
                      <button type="button" class="etoile" disabled={!modifiable} aria-label="{s} sur 5" title="{s} / 5"
                        onclick={() => saisir(c, String(s))}>{etoiles(c) >= s - 0.25 ? '★' : '☆'}</button>
                    {/each}
                  </span>
                  <input id={idc} type="text" class="court" value={textes[nom]} placeholder={indication(c)} disabled={!modifiable}
                    title="Note de 0 à 5 (décimales permises : 3,5)" oninput={(e) => saisir(c, e.currentTarget.value)} data-champ={nom} />
                {:else if c.type === 'entier' || c.type === 'prix' || c.type === 'date'}
                  <input id={idc} type="text" class:court={!c.multiple} class:moyen={c.multiple || c.type === 'date'} value={textes[nom]}
                    placeholder={indication(c)} disabled={!modifiable} inputmode={c.type === 'date' ? 'numeric' : 'decimal'}
                    oninput={(e) => saisir(c, e.currentTarget.value)} data-champ={nom} />
                {:else}
                  <ChampSuggestions id={idc} champ={nom} valeur={textes[nom] ?? ''} suggestions={c.suggestions} libelle={c.libelle}
                    placeholder={indication(c)} disabled={!modifiable} onsaisie={(v) => saisir(c, v)} />
                {/if}
                {#if m}
                  <button type="button" class="mini" disabled={!modifiable} onclick={() => revenir(c)} title="Revenir à la valeur enregistrée"
                    aria-label="Revenir à la valeur enregistrée de {c.libelle}">↺</button>
                {:else if c.multiple || (estPerso && c.valeur !== '')}
                  <button type="button" class="mini" disabled={!modifiable} onclick={() => viderPourTous(c)}
                    title={estPerso && !c.multiple ? 'Supprimer cette valeur' : 'Vider ce champ pour tous les livres'}
                    aria-label="Vider {c.libelle}">✖</button>
                {/if}
              </div>
              {#if erreurs[nom]}<span class="erreur-champ" role="alert">⚠ {erreurs[nom]}</span>{/if}
            </div>
          {/if}
        {/each}
      </div>
      {#if estPerso}
        <form class="nouvelle" onsubmit={(e) => { e.preventDefault(); void ajouterPerso(); }}>
          <input type="text" bind:value={nouvelleCle} placeholder="Nom de la nouvelle valeur" disabled={!modifiable} aria-label="Nom de la nouvelle valeur personnalisée" />
          <button type="submit" disabled={!modifiable || !nouvelleCle.trim()}>➕ Ajouter</button>
        </form>
      {/if}
    </div>
  {:else if chargement}
    <p class="discret">Chargement des fiches…</p>
  {/if}

  {#if confirmation}
    <div class="confirmation" role="alertdialog" aria-label="Confirmation" data-testid="confirmation">
      <p>{confirmation.message}</p>
      <div class="boutons">
        <button onclick={() => (confirmation = null)}>Annuler</button>
        <button class="principal" onclick={() => { const s = confirmation!.suite; void enregistrer(s, true); }}>Confirmer</button>
      </div>
    </div>
  {/if}
  {#if message}<p class="message" class:erreur={message.erreur} role={message.erreur ? 'alert' : 'status'} data-testid="message-infos">{message.texte}</p>{/if}
  {#if bilan && (fiche?.multiple || !bilan.complet)}<ResultatsLot {bilan} replie={bilan.complet} />{/if}

  <footer class="pied">
    <div class="rang">
      {#if fiche && !fiche.multiple && onvoisin}
        {@const indice = sale ? ' (les modifications sont enregistrées)' : ''}
        <button disabled={!peutOk || !avoisin?.(-1)} onclick={() => void enregistrer(-1)} title="Livre précédent de la vue{indice}">◀ Précédent</button>
        <button disabled={!peutOk || !avoisin?.(1)} onclick={() => void enregistrer(1)} title="Livre suivant de la vue{indice}">Suivant ▶</button>
        <span class="separateur"></span>
      {/if}
      <label class="case" title={fiche?.ecrireDansFichierPossible
        ? "À l'enregistrement, écrire aussi le ComicInfo.xml dans le fichier (CBZ seulement), comme « Écrire les infos dans le fichier »"
        : 'Seuls les CBZ reçoivent les infos (convertir d\'abord en CBZ)'}>
        <input type="checkbox" bind:checked={ecrireFichier} disabled={!modifiable || !fiche?.ecrireDansFichierPossible} data-testid="ecrire-fichier" />
        Écrire aussi les infos dans le fichier (ComicInfo.xml)
      </label>
    </div>
    <div class="rang">
      <span class="etat" aria-live="polite">
        {#if enregistrement}
          <span>Enregistrement…{progression ? ` ${progression.fait} / ${progression.total}${progression.livre ? ' — ' + progression.livre : ''}` : ''}</span>
          <span class="barre-progression petite" role="progressbar" aria-label="Progression de l'enregistrement" aria-valuemin="0" aria-valuemax="100" aria-valuenow={pourcent(progression)}
            ><div style:width="{pourcent(progression)}%"></div></span>
          {#if ids.length > 1}<button class="mini" onclick={() => void annulerOperation()} title="Arrêter entre deux livres">Arrêter</button>{/if}
        {:else if sale}
          <span class="touche">● {[modifications.length ? `${modifications.length} champ(s) modifié(s)` : '', avecPages && pagesModifiees ? 'pages modifiées' : ''].filter(Boolean).join(', ')}</span>
        {/if}
      </span>
      <span class="espace"></span>
      {#if fiche && !fiche.multiple && scriptsEditeur.length}
        <button disabled={!modifiable || scriptEnCours} onclick={ouvrirMenuScripts} aria-haspopup="menu"
          title={scriptEnCours ? 'Script en cours' : 'Lancer un script qui remplit les champs (rien n\'est enregistré avant OK)'} data-testid="infos-scripts">📜 Scripts ▾</button>
      {/if}
      <button class="principal" disabled={!peutOk} title={peutOk ? 'Enregistrer et fermer (Ctrl+Entrée)' : raisonBlocage}
        onclick={() => void enregistrer('fermer')} data-testid="infos-ok">OK</button>
      <button disabled={enregistrement} onclick={annuler} data-testid="infos-annuler">Annuler</button>
      <button disabled={!peutEnregistrer} title={peutEnregistrer ? 'Enregistrer sans fermer' : raisonBlocage}
        onclick={() => void enregistrer('rester')} data-testid="infos-appliquer">Appliquer</button>
    </div>
  </footer>
  {#if menuScripts}
    <MenuContextuel x={menuScripts.x} y={menuScripts.y} entrees={entreesScripts} onchoix={(id) => void lancerScriptEditeur(id)} onfermer={() => (menuScripts = null)} />
  {/if}
</dialog>

<style>
  .infos { width: min(760px, calc(100vw - 32px)); max-height: calc(100vh - 32px); display: flex; flex-direction: column; gap: 10px; padding: 16px 18px; }
  .infos:not([open]) { display: none; }
  .infos.large { width: min(1040px, calc(100vw - 32px)); height: calc(100vh - 32px); }
  .panneau-pages { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .panneau-pages[hidden], .panneau-champs[hidden] { display: none; }
  .onglets button.touche-onglet:not(.actif) { color: var(--cr-accent); }
  .infos h2 { margin: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .entete { display: flex; gap: 14px; align-items: flex-start; }
  .couvertures { position: relative; width: 96px; height: 140px; flex: none; }
  .couvertures img {
    position: absolute; left: 0; top: 0; width: 96px; height: 140px; object-fit: cover; border: 1px solid var(--cr-bordure);
    border-radius: var(--cr-rayon); background: var(--cr-fond-survol); box-shadow: var(--cr-ombre);
  }
  .couvertures.pile img { width: 72px; height: 116px; transform: translate(var(--decalage), calc(24px - var(--decalage))); }
  .identite { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .nom { font-size: 16px; overflow-wrap: anywhere; }
  .discret { color: var(--cr-texte-discret); }
  .petit { font-size: 12px; }
  .chemin { word-break: break-all; }
  .lecture-seule { display: flex; flex-direction: column; gap: 4px; padding: 8px 10px; border: 1px solid var(--cr-erreur); border-radius: var(--cr-rayon); }
  .lecture-seule strong { color: var(--cr-erreur); }
  .onglets { display: flex; flex-wrap: wrap; gap: 4px; border-bottom: 1px solid var(--cr-bordure); padding-bottom: 6px; }
  .onglets button.actif { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); }
  .panneau-champs { flex: 1; min-height: 160px; overflow-y: auto; padding-right: 4px; }
  .grille-champs { display: grid; grid-template-columns: minmax(140px, max-content) minmax(0, 1fr); gap: 6px 12px; align-items: start; }
  .libelle { padding-top: 6px; color: var(--cr-texte-discret); }
  .libelle.touche, .touche { color: var(--cr-accent); }
  .controle { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .ligne-controle { display: flex; align-items: center; gap: 4px; min-width: 0; }
  .ligne-controle > input[type='text'] { flex: 1; min-width: 0; }
  .ligne-controle > input.court { flex: none; width: 90px; }
  .ligne-controle > input.moyen { flex: none; width: 160px; }
  .ligne-controle select { min-width: 220px; }
  textarea {
    flex: 1; min-width: 0; resize: vertical; font: inherit; color: var(--cr-texte); background: var(--cr-fond-panneau);
    border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); padding: 5px 8px;
  }
  textarea:focus-visible { outline: 2px solid var(--cr-accent); outline-offset: 1px; }
  .etoiles { display: inline-flex; }
  .etoile { border: 0; background: none; padding: 0 2px; font-size: 18px; color: var(--cr-accent); }
  .etoile:hover:not(:disabled) { background: none; filter: brightness(1.2); }
  .mini { padding: 2px 7px; flex: none; }
  .erreur-champ { font-size: 12px; color: var(--cr-erreur); }
  .nouvelle { display: flex; gap: 8px; margin-top: 10px; }
  .nouvelle input { width: 240px; }
  .confirmation { border: 1px solid var(--cr-accent); border-radius: var(--cr-rayon); padding: 8px 10px; background: var(--cr-fond); }
  .confirmation p { margin: 0 0 8px; }
  .boutons { display: flex; justify-content: flex-end; gap: 8px; }
  .message { margin: 0; color: var(--cr-accent); white-space: pre-line; max-height: 80px; overflow-y: auto; }
  .message.erreur { color: var(--cr-erreur); }
  .pied { display: flex; flex-direction: column; gap: 8px; border-top: 1px solid var(--cr-bordure); padding-top: 8px; }
  .rang { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .separateur { width: 1px; align-self: stretch; background: var(--cr-bordure); }
  .case { display: flex; align-items: center; gap: 6px; }
  .case input { accent-color: var(--cr-accent); }
  .etat { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .espace { flex: 1; }
  .petite { display: inline-block; width: 120px; }
  button.principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); min-width: 80px; font-weight: 600; }
  button.principal:hover:not(:disabled) { background: var(--cr-accent); filter: brightness(1.1); }
</style>
