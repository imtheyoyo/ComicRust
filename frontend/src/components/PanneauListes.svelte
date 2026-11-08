<script lang="ts">
  // Panneau des listes ComicRack (étape 5, docs/tauri/ipc-listes.md), comme celui de la version
  // egui (lists_ui.rs) : arbre (dossiers, listes de lecture, listes intelligentes) avec compteurs,
  // clavier (↑ ↓ changent la liste affichée, → déplie / entre, ← replie / remonte, F2, Suppr, Menu),
  // menu contextuel, renommage dans l'arbre, glisser-déposer des listes, cible des livres glissés,
  // import / export .cbl, éditeur de listes intelligentes, messages lecture seule / ComicRack lancé.
  import { onDestroy, tick } from 'svelte';
  import {
    analyserCbl, bilanListes, convertirEnLecture, creerListe, decalerListe, deplacerListe, deposerListe, destinationsListe,
    dupliquerListe, exporterCbl, importerCbl, infosConversion, infosSuppression, messageErreur, modifierListe, placeDepotListe,
    renommerListe, supprimerListe, type Combinaison, type DestinationListe, type InfosSuppression, type NoeudListe, type PlaceListe,
    type RapportCbl,
  } from '../lib/api';
  import { glypheListe, infobulleListe, type EtatListes, type LigneArbre } from '../lib/listes.svelte';
  import { modale } from '../lib/modale';
  import EditeurListeIntelligente from './EditeurListeIntelligente.svelte';
  import MenuContextuel, { type EntreeMenu } from './MenuContextuel.svelte';

  interface Props {
    listes: EtatListes;
    /** Liste choisie (clic, flèches) : la vue affiche ses livres (null : toute la bibliothèque). */
    onafficher: (id: string | null) => void;
  }
  let { listes, onafficher }: Props = $props();

  type Place = { parent?: string; position?: number; apresListe?: string };

  let arbreEl = $state<HTMLDivElement>();
  /** Menu contextuel : sur une liste, ou dans le vide (null). */
  let menu = $state<{ x: number; y: number; id: string | null } | null>(null);
  /** Éditeur de liste intelligente : existante (id) ou nouvelle (place). */
  let editeur = $state<{ id?: string; place?: Place } | null>(null);
  let suppression = $state<(InfosSuppression & { id: string }) | null>(null);
  let conversion = $state<{ id: string; nom: string; nbLivres: number } | null>(null);
  let modification = $state<{ id: string; nom: string; description: string; combinaison?: Combinaison; initial: string } | null>(null);
  let destination = $state<{ id: string; nom: string; choix: string; liste: DestinationListe[] } | null>(null);
  let importCbl = $state<{ place: Place; chemin: string; nom: string; rapport: RapportCbl | null; importe: boolean; erreur: string; cours: boolean } | null>(null);
  let exportCbl = $state<{ id: string; nom: string; chemin: string; bilan: string; erreur: string } | null>(null);

  const lignes = $derived(listes.lignes);
  const raison = $derived(listes.raison);
  const etat = $derived(listes.arbre?.etat);
  const lectureSeule = $derived(etat && !etat.modifiable ? (etat.lectureSeule ?? etat.erreur) : undefined);

  // ───── Choix et clavier ─────

  function choisir(id: string, focus = false) {
    onafficher(id);
    void tick().then(() => {
      document.getElementById(`liste-${id}`)?.scrollIntoView({ block: 'nearest' });
      if (focus) arbreEl?.focus({ preventScroll: true });
    });
  }

  function basculer(n: NoeudListe) {
    if (listes.ouverts.has(n.id)) listes.ouverts.delete(n.id);
    else listes.ouverts.add(n.id);
  }

  /** Flèches comme la version egui (keynav.rs, tree_move) ; Début / Fin ; Page préc./suiv. */
  function clavier(e: KeyboardEvent) {
    if (listes.renommage) return;
    const k = lignes.findIndex((l) => l.noeud.id === listes.courante);
    const ligne: LigneArbre | undefined = lignes[k];
    const page = Math.max(1, Math.floor((arbreEl?.clientHeight ?? 300) / 28) - 1);
    let cible: number | null = null;
    switch (e.key) {
      case 'ArrowDown': cible = k < 0 ? 0 : Math.min(lignes.length - 1, k + 1); break;
      case 'ArrowUp': cible = k < 0 ? 0 : Math.max(0, k - 1); break;
      case 'PageDown': cible = k < 0 ? 0 : Math.min(lignes.length - 1, k + page); break;
      case 'PageUp': cible = k < 0 ? 0 : Math.max(0, k - page); break;
      case 'Home': cible = 0; break;
      case 'End': cible = lignes.length - 1; break;
      case 'ArrowRight':
        if (!ligne || ligne.noeud.type !== 'dossier') break;
        if (!listes.ouverts.has(ligne.noeud.id)) listes.ouverts.add(ligne.noeud.id);
        else if (lignes[k + 1]?.profondeur > ligne.profondeur) cible = k + 1;
        break;
      case 'ArrowLeft':
        if (!ligne) break;
        if (ligne.noeud.type === 'dossier' && listes.ouverts.has(ligne.noeud.id)) listes.ouverts.delete(ligne.noeud.id);
        else if (ligne.parent) cible = lignes.findIndex((l) => l.noeud.id === ligne.parent!.id);
        break;
      case 'F2':
        if (ligne) commencerRenommage(ligne.noeud);
        break;
      case 'Delete':
        if (ligne) void demanderSuppression(ligne.noeud);
        break;
      case 'Enter':
        if (ligne?.noeud.type === 'dossier') basculer(ligne.noeud);
        break;
      case 'ContextMenu':
      case 'F10':
        if (e.key === 'F10' && !e.shiftKey) return;
        if (ligne) {
          const r = document.getElementById(`liste-${ligne.noeud.id}`)?.getBoundingClientRect();
          menu = { x: (r?.left ?? 40) + 30, y: (r?.top ?? 40) + 20, id: ligne.noeud.id };
        }
        break;
      default:
        return;
    }
    e.preventDefault();
    if (cible !== null && cible >= 0 && lignes[cible]) choisir(lignes[cible].noeud.id);
  }

  /** Commande refusée en lecture seule : la raison sous le panneau. */
  function refuser(): boolean {
    if (!raison) return false;
    listes.info = { texte: raison, erreur: true };
    return true;
  }

  // ───── Renommage dans l'arbre ─────

  function commencerRenommage(n: NoeudListe) {
    if (refuser()) return;
    listes.renommage = { id: n.id, texte: n.nom };
  }

  function champRenommage(el: HTMLInputElement) {
    el.focus();
    el.select(); // tout le texte sélectionné, comme dans ComicRack
  }

  let finRenommage = false;
  async function validerRenommage(annuler: boolean) {
    const r = listes.renommage;
    if (!r || finRenommage) return;
    finRenommage = true;
    const n = listes.noeud(r.id);
    const nom = r.texte.trim();
    listes.renommage = null;
    if (!annuler && n && nom && nom !== n.nom) await listes.executer(`Renommage en « ${nom} »`, () => renommerListe(r.id, nom));
    finRenommage = false;
    await tick();
    arbreEl?.focus({ preventScroll: true });
  }

  function clavierRenommage(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Enter') {
      e.preventDefault();
      void validerRenommage(false);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      void validerRenommage(true);
    }
  }

  // ───── Création, suppression, déplacement ─────

  /** Place d'une nouvelle liste : dans le dossier visé (à la fin), ou juste après la liste visée. */
  function placeNouvelle(id: string | null): Place {
    const n = listes.noeud(id ?? listes.courante);
    if (!n) return {};
    if (n.type === 'dossier' && id) return { parent: n.id };
    return { apresListe: n.id };
  }

  async function nouvelle(type: 'lecture' | 'dossier' | 'intelligente', id: string | null) {
    const place = placeNouvelle(id);
    if (type === 'intelligente') {
      editeur = { place };
      return;
    }
    // Le backend sélectionne la nouvelle liste et la passe en renommage (ResultatListes).
    await listes.executer(type === 'dossier' ? 'Création du dossier' : 'Création de la liste', () => creerListe({ type, ...place }));
    await tick();
    if (listes.courante) document.getElementById(`liste-${listes.courante}`)?.scrollIntoView({ block: 'nearest' });
  }

  async function demanderSuppression(n: NoeudListe) {
    if (refuser()) return;
    try {
      suppression = { ...(await infosSuppression(n.id)), id: n.id };
    } catch (e) {
      listes.info = { texte: messageErreur(e), erreur: true };
    }
  }

  async function confirmerSuppression() {
    if (!suppression) return;
    const { id, nom } = suppression;
    suppression = null;
    await listes.executer(`Suppression de « ${nom} »`, () => supprimerListe(id, listes.courante ?? undefined));
    arbreEl?.focus({ preventScroll: true });
  }

  async function demanderConversion(n: NoeudListe) {
    if (refuser()) return;
    try {
      conversion = { ...(await infosConversion(n.id)), id: n.id };
    } catch (e) {
      listes.info = { texte: messageErreur(e), erreur: true };
    }
  }

  async function demanderDestination(n: NoeudListe) {
    if (refuser()) return;
    try {
      const liste = await destinationsListe(n.id);
      const libre = liste.find((d) => !d.actuel);
      destination = { id: n.id, nom: n.nom, liste, choix: libre ? String(libre.id) : '' };
    } catch (e) {
      listes.info = { texte: messageErreur(e), erreur: true };
    }
  }

  async function validerDestination() {
    if (!destination) return;
    const { id, nom, choix } = destination;
    destination = null;
    const parent = choix === 'null' ? null : choix;
    const r = await listes.executer(`Déplacement de « ${nom} »`, () => deplacerListe(id, parent));
    if (r) listes.devoiler(id);
  }

  async function validerModification() {
    const m = modification;
    if (!m) return;
    modification = null;
    const n = listes.noeud(m.id);
    if (!n) return;
    const champs: { nom?: string; description?: string; combinaison?: Combinaison } = {};
    if (m.nom.trim() && m.nom.trim() !== n.nom) champs.nom = m.nom.trim();
    if (m.description.trim() !== (n.description ?? '')) champs.description = m.description.trim();
    if (m.combinaison && m.combinaison !== n.combinaison) champs.combinaison = m.combinaison;
    if (Object.keys(champs).length) await listes.executer(`Modification de « ${n.nom} »`, () => modifierListe(m.id, champs));
  }

  async function copierBilan() {
    try {
      const t = await bilanListes();
      await navigator.clipboard?.writeText(t);
      listes.info = { texte: 'Bilan des listes copié (ComicRust / ComicRack, « ≠ » : différent).', erreur: false };
    } catch (e) {
      listes.info = { texte: messageErreur(e), erreur: true };
    }
  }

  // ───── Menu contextuel ─────

  const entreesMenu = $derived.by<EntreeMenu[]>(() => {
    if (!menu) return [];
    const n = listes.noeud(menu.id);
    const aide = raison;
    const ro = !!raison;
    const nouvelles: EntreeMenu[] = [
      { id: 'n-lecture', libelle: '📄 Nouvelle liste de lecture', desactive: ro, aide },
      { id: 'n-intelligente', libelle: '🔍 Nouvelle liste intelligente…', desactive: ro, aide },
      { id: 'n-dossier', libelle: '📁 Nouveau dossier', desactive: ro, aide },
      { id: 'importer', libelle: 'Importer une liste .cbl…', desactive: ro, aide },
    ];
    if (!n) return [...nouvelles, { id: 'bilan', libelle: '📋 Copier le bilan des listes', separe: true, aide: 'Comptes ComicRust / ComicRack de chaque liste' }];
    const smart = n.type === 'intelligente';
    return [
      ...nouvelles,
      smart
        ? { id: 'criteres', libelle: '✏ Modifier les critères…', separe: true, aide: ro ? `${aide} (consultation)` : undefined }
        : { id: 'modifier', libelle: '✏ Modifier…', separe: true, desactive: ro, aide },
      { id: 'renommer', libelle: 'Renommer', raccourci: 'F2', desactive: ro, aide },
      { id: 'dupliquer', libelle: '⎘ Dupliquer', desactive: ro, aide },
      { id: 'supprimer', libelle: '🗑 Supprimer…', raccourci: 'Suppr', desactive: ro, aide },
      ...(smart ? [{ id: 'convertir', libelle: '📄 Convertir en liste de lecture…', separe: true, desactive: ro, aide }] : []),
      { id: 'monter', libelle: '⬆ Monter', desactive: !n.peutMonter, aide, separe: true },
      { id: 'descendre', libelle: '⬇ Descendre', desactive: !n.peutDescendre, aide },
      { id: 'deplacer', libelle: 'Déplacer dans…', desactive: ro, aide },
      ...(n.type !== 'dossier'
        ? [{ id: 'exporter', libelle: 'Exporter en .cbl…', separe: true, aide: 'Liste de lecture ComicRack (.cbl), possible en lecture seule' }]
        : []),
    ];
  });

  /** Liste visée par le dernier menu ouvert (le menu se ferme avant d'appeler `action`). */
  let cibleMenu: string | null = null;
  $effect(() => {
    if (menu) cibleMenu = menu.id;
  });

  function action(choix: string) {
    const id = cibleMenu;
    menu = null;
    const n = listes.noeud(id);
    switch (choix) {
      case 'n-lecture': void nouvelle('lecture', id); break;
      case 'n-intelligente': void nouvelle('intelligente', id); break;
      case 'n-dossier': void nouvelle('dossier', id); break;
      case 'importer': importCbl = { place: placeNouvelle(id), chemin: '', nom: '', rapport: null, importe: false, erreur: '', cours: false }; break;
      case 'bilan': void copierBilan(); break;
      case 'criteres': if (n) editeur = { id: n.id }; break;
      case 'modifier':
        if (n) {
          const m = { id: n.id, nom: n.nom, description: n.description ?? '', combinaison: n.combinaison };
          modification = { ...m, initial: JSON.stringify(m) };
        }
        break;
      case 'renommer': if (n) commencerRenommage(n); break;
      case 'dupliquer': if (n) void listes.executer(`Copie de « ${n.nom} »`, () => dupliquerListe(n.id)); break;
      case 'supprimer': if (n) void demanderSuppression(n); break;
      case 'convertir': if (n) void demanderConversion(n); break;
      case 'monter': if (n) void listes.executer(`Déplacement de « ${n.nom} »`, () => decalerListe(n.id, true)); break;
      case 'descendre': if (n) void listes.executer(`Déplacement de « ${n.nom} »`, () => decalerListe(n.id, false)); break;
      case 'deplacer': if (n) void demanderDestination(n); break;
      case 'exporter': if (n) exportCbl = { id: n.id, nom: n.nom, chemin: '', bilan: '', erreur: '' }; break;
    }
  }

  function menuSur(e: MouseEvent, id: string | null) {
    e.preventDefault();
    e.stopPropagation();
    if (id && id !== listes.courante) choisir(id);
    menu = { x: e.clientX, y: e.clientY, id };
  }

  // ───── Import / export .cbl ─────

  async function analyser() {
    if (!importCbl || !importCbl.chemin.trim()) return;
    importCbl.cours = true;
    importCbl.erreur = '';
    try {
      const r = await analyserCbl(importCbl.chemin.trim());
      if (!importCbl) return;
      importCbl.rapport = r;
      importCbl.nom = r.nom;
    } catch (e) {
      if (importCbl) importCbl.erreur = messageErreur(e);
    } finally {
      if (importCbl) importCbl.cours = false;
    }
  }

  async function importer() {
    const d = importCbl;
    if (!d?.rapport) return;
    d.cours = true;
    const chemin = d.chemin.trim();
    const nom = d.nom.trim() || undefined;
    const place = { ...d.place };
    const r = await listes.executer('Import de la liste .cbl', async () => {
      const x = await importerCbl(chemin, { nom, ...place });
      if (importCbl) importCbl.rapport = x.rapport;
      return x.resultat;
    });
    if (!importCbl) return;
    importCbl.cours = false;
    if (r) importCbl.importe = true;
    else importCbl.erreur = listes.info?.texte ?? 'Import impossible.';
  }

  async function exporter() {
    if (!exportCbl || !exportCbl.chemin.trim()) return;
    exportCbl.erreur = '';
    try {
      const r = await exporterCbl(exportCbl.id, exportCbl.chemin.trim());
      exportCbl.bilan = `${r.livres} livre${r.livres > 1 ? 's' : ''} exporté${r.livres > 1 ? 's' : ''} dans ${r.chemin}`;
      exportCbl.chemin = '';
      listes.info = { texte: `✔ ${exportCbl.bilan}`, erreur: false };
    } catch (e) {
      exportCbl.erreur = messageErreur(e);
    }
  }

  const libelleManquant = (m: RapportCbl['manquants'][number]) =>
    `#${m.rang} — ${m.serie}${m.volume ? ` (${m.volume})` : ''} n° ${m.numero}${m.annee ? `, ${m.annee}` : ''}`;

  // ───── Glisser-déposer des listes dans l'arbre ─────

  let glisseListe = $state<{ id: string; x0: number; y0: number; actif: boolean; cible: string | null; place: PlaceListe } | null>(null);
  /** Réponses de place_depot_liste (« cible/place » → possible ?). */
  let places = $state<Record<string, boolean>>({});

  function appuiListe(e: PointerEvent, n: NoeudListe) {
    if (e.button !== 0 || raison || listes.renommage) return;
    glisseListe = { id: n.id, x0: e.clientX, y0: e.clientY, actif: false, cible: null, place: 'avant' };
    places = {};
    window.addEventListener('pointermove', bougerListe);
    window.addEventListener('pointerup', lacherListe);
    window.addEventListener('pointercancel', annulerGlisseListe);
    window.addEventListener('keydown', echapGlisseListe, true);
  }

  function finGlisseListe() {
    window.removeEventListener('pointermove', bougerListe);
    window.removeEventListener('pointerup', lacherListe);
    window.removeEventListener('pointercancel', annulerGlisseListe);
    window.removeEventListener('keydown', echapGlisseListe, true);
  }

  onDestroy(finGlisseListe);

  /** Glisser interrompu (pointeur perdu) ou Échap : rien n'est déplacé. */
  function annulerGlisseListe() {
    finGlisseListe();
    glisseListe = null;
  }

  function echapGlisseListe(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !glisseListe?.actif) return;
    e.preventDefault();
    e.stopPropagation();
    annulerGlisseListe();
  }

  function bougerListe(e: PointerEvent) {
    const g = glisseListe;
    if (!g) return;
    if (!g.actif) {
      if (Math.hypot(e.clientX - g.x0, e.clientY - g.y0) < 6) return;
      g.actif = true;
    }
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>('[data-liste-id]');
    if (!el || !arbreEl?.contains(el)) {
      g.cible = null;
      return;
    }
    const cible = el.dataset.listeId!;
    const r = el.getBoundingClientRect();
    const t = (e.clientY - r.top) / Math.max(1, r.height);
    // Zones du contrat : dossier = quart haut avant, quart bas après, milieu dans ; liste = moitiés.
    g.place = el.dataset.type === 'dossier' ? (t < 0.25 ? 'avant' : t > 0.75 ? 'apres' : 'dans') : t < 0.5 ? 'avant' : 'apres';
    g.cible = cible;
    const k = `${cible}/${g.place}`;
    if (!(k in places) && cible !== g.id) {
      const glissee = g.id;
      const place = g.place;
      places[k] = false;
      void placeDepotListe(glissee, cible, place).then((ok) => (places[k] = ok), () => (places[k] = false));
    }
  }

  function lacherListe() {
    finGlisseListe();
    const g = glisseListe;
    glisseListe = null;
    if (!g?.actif) return;
    const bloquer = (e: MouseEvent) => { e.stopPropagation(); e.preventDefault(); };
    window.addEventListener('click', bloquer, { capture: true, once: true });
    setTimeout(() => window.removeEventListener('click', bloquer, { capture: true }), 0);
    if (g.cible && places[`${g.cible}/${g.place}`]) {
      const cible = g.cible;
      const nom = listes.noeud(g.id)?.nom ?? '';
      void listes.executer(`Déplacement de « ${nom} »`, () => deposerListe(g.id, cible, g.place)).then((r) => {
        if (r) listes.devoiler(g.id);
      });
    }
  }

  const nombre = (n: NoeudListe) => (n.nbLivres === undefined ? '?' : n.nbLivres.toLocaleString('fr-FR'));
</script>

<section class="panneau" aria-label="Listes">
  <div class="tete">
    <span class="titre">Listes</span>
    {#if lectureSeule}<span class="cadenas" title={lectureSeule} aria-hidden="true">🔒</span>{/if}
    <button class="plus" title="Nouvelle liste, nouveau dossier, import .cbl" aria-label="Nouvelle liste"
      onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); menu = { x: r.left, y: r.bottom + 2, id: null }; }}
      data-testid="listes-plus">＋</button>
  </div>
  {#if lectureSeule}
    <p class="lecture-seule" class:comicrack={etat?.comicrackLance} role="status" data-testid="listes-lecture-seule">🔒 {lectureSeule}</p>
  {/if}
  {#if etat?.enCours}<p class="discret" role="status">{etat.enCours}…</p>{/if}

  {#if listes.erreur}
    <p class="erreur" role="alert">{listes.erreur}</p>
  {:else if !listes.arbre}
    <p class="discret">Chargement des listes…</p>
  {:else}
    <!-- Le clavier est géré par l'arbre (aria-activedescendant). -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="arbre" bind:this={arbreEl} role="tree" tabindex="0" aria-label="Listes de la bibliothèque"
      aria-activedescendant={listes.courante && listes.noeud(listes.courante) ? `liste-${listes.courante}` : undefined}
      onkeydown={clavier} oncontextmenu={(e) => menuSur(e, null)} data-testid="arbre-listes">
      {#if lignes.length === 0}
        <p class="discret">Aucune liste : « ＋ » pour en créer une.</p>
      {/if}
      {#each lignes as l (l.noeud.id)}
        {@const n = l.noeud}
        {@const ouvert = listes.ouverts.has(n.id)}
        {@const cible = glisseListe?.actif && glisseListe.cible === n.id && glisseListe.id !== n.id ? glisseListe : null}
        {@const placeOk = cible ? places[`${n.id}/${cible.place}`] : undefined}
        {@const depot = listes.survolDepot?.id === n.id ? listes.survolDepot : null}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          id="liste-{n.id}"
          class="ligne"
          class:courante={listes.courante === n.id}
          class:cible-avant={placeOk && cible?.place === 'avant'}
          class:cible-apres={placeOk && cible?.place === 'apres'}
          class:cible-dans={placeOk && cible?.place === 'dans'}
          class:cible-refus={cible && placeOk === false}
          class:depot-ok={depot?.ok}
          class:depot-refus={depot && !depot.ok}
          style:padding-left="{6 + l.profondeur * 16}px"
          role="treeitem"
          aria-selected={listes.courante === n.id}
          aria-expanded={n.type === 'dossier' ? ouvert : undefined}
          aria-level={l.profondeur + 1}
          tabindex="-1"
          title={infobulleListe(n)}
          data-liste-id={n.id}
          data-type={n.type}
          onclick={() => choisir(n.id, true)}
          ondblclick={() => { if (n.type === 'dossier') basculer(n); else if (n.type === 'intelligente') editeur = { id: n.id }; }}
          oncontextmenu={(e) => menuSur(e, n.id)}
          onpointerdown={(e) => appuiListe(e, n)}
        >
          {#if n.type === 'dossier'}
            <button class="triangle" tabindex="-1" aria-label={ouvert ? 'Replier' : 'Déplier'}
              onclick={(e) => { e.stopPropagation(); basculer(n); }}>{ouvert ? '▾' : '▸'}</button>
          {:else}
            <span class="triangle"></span>
          {/if}
          <span class="glyphe" aria-hidden="true">{glypheListe(n, ouvert)}</span>
          {#if listes.renommage?.id === n.id}
            <input class="renommage" type="text" bind:value={listes.renommage.texte} {@attach champRenommage}
              onkeydown={clavierRenommage} onblur={() => void validerRenommage(false)} onclick={(e) => e.stopPropagation()}
              onpointerdown={(e) => e.stopPropagation()} aria-label="Nouveau nom" spellcheck="false" data-testid="renommage-liste" />
          {:else}
            <span class="nom">{n.nom}</span>
            <span class="compte" class:inconnu={n.nbLivres === undefined}>{nombre(n)}</span>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  {#if listes.info}
    <p class="info" class:erreur={listes.info.erreur} role={listes.info.erreur ? 'alert' : 'status'} data-testid="listes-info">
      <span>{listes.info.erreur ? '⚠ ' : ''}{listes.info.texte}</span>
      <button class="fermer-info" aria-label="Effacer le message" onclick={() => (listes.info = null)}>✖</button>
    </p>
  {/if}
</section>

{#if menu}
  <MenuContextuel x={menu.x} y={menu.y} entrees={entreesMenu} onchoix={action} onfermer={() => (menu = null)} />
{/if}

{#if editeur}
  <EditeurListeIntelligente
    id={editeur.id}
    place={editeur.place}
    {listes}
    onfermer={() => { editeur = null; void tick().then(() => arbreEl?.focus({ preventScroll: true })); }}
  />
{/if}

{#if suppression}
  <dialog use:modale={{ ouvrirAuMontage: true, fermer: () => (suppression = null) }} aria-labelledby="titre-suppression" data-testid="dialogue-suppression-liste">
    <h2 id="titre-suppression">Supprimer la liste</h2>
    <p>Supprimer « {suppression.nom} » ?</p>
    {#if suppression.sousListes > 0}
      <p class="avertissement">Ce dossier contient {suppression.sousListes} sous-liste{suppression.sousListes > 1 ? 's' : ''} : {suppression.sousListes > 1 ? 'elles seront supprimées' : 'elle sera supprimée'} avec lui.</p>
    {/if}
    {#if suppression.dependantes.length > 0}
      <p class="avertissement">Ces listes intelligentes partent de cette liste ; elles partiront de toute la bibliothèque : {suppression.dependantes.map((d) => `« ${d} »`).join(', ')}.</p>
    {/if}
    <p class="discret">Les livres eux-mêmes ne sont pas supprimés.</p>
    <div class="pied">
      <button onclick={() => (suppression = null)}>Annuler</button>
      <button class="danger" onclick={() => void confirmerSuppression()} data-testid="confirmer-suppression">Supprimer</button>
    </div>
  </dialog>
{/if}

{#if conversion}
  <dialog use:modale={{ ouvrirAuMontage: true, fermer: () => (conversion = null) }} aria-labelledby="titre-conversion" data-testid="dialogue-conversion">
    <h2 id="titre-conversion">Convertir en liste de lecture</h2>
    <p>« {conversion.nom} » devient une liste de lecture contenant ses {conversion.nbLivres} livre{conversion.nbLivres > 1 ? 's' : ''} actuel{conversion.nbLivres > 1 ? 's' : ''} ; ses critères sont abandonnés.</p>
    <p class="discret">Les listes intelligentes qui en partent continuent de fonctionner.</p>
    <div class="pied">
      <button onclick={() => (conversion = null)}>Annuler</button>
      <button class="principal" data-testid="confirmer-conversion"
        onclick={() => { const c = conversion!; conversion = null; void listes.executer(`Conversion de « ${c.nom} »`, () => convertirEnLecture(c.id)); }}>Convertir</button>
    </div>
  </dialog>
{/if}

{#if modification}
  <dialog use:modale={{ ouvrirAuMontage: true, peutFermer: () => !!modification && JSON.stringify({ id: modification.id, nom: modification.nom, description: modification.description, combinaison: modification.combinaison }) === modification.initial, fermer: () => (modification = null) }}
    aria-labelledby="titre-modification" data-testid="dialogue-modifier-liste">
    <h2 id="titre-modification">{modification.combinaison ? 'Modifier le dossier' : 'Modifier la liste'}</h2>
    <form class="formulaire" onsubmit={(e) => { e.preventDefault(); void validerModification(); }}>
      <label for="mod-nom">Nom :</label>
      <input id="mod-nom" type="text" bind:value={modification.nom} spellcheck="false" />
      <label for="mod-description">Description :</label>
      <input id="mod-description" type="text" bind:value={modification.description} />
      {#if modification.combinaison}
        <label for="mod-combinaison">Livres du dossier :</label>
        <select id="mod-combinaison" bind:value={modification.combinaison}>
          <option value="ou">réunion des sous-listes (au moins une)</option>
          <option value="et">intersection des sous-listes (toutes)</option>
          <option value="vide">aucun livre (simple rangement)</option>
        </select>
      {/if}
      <div class="pied">
        <button type="button" onclick={() => (modification = null)}>Annuler</button>
        <button type="submit" class="principal" disabled={!modification.nom.trim()}>OK</button>
      </div>
    </form>
  </dialog>
{/if}

{#if destination}
  <dialog use:modale={{ ouvrirAuMontage: true, fermer: () => (destination = null) }} aria-labelledby="titre-destination" data-testid="dialogue-deplacer-liste">
    <h2 id="titre-destination">Déplacer « {destination.nom} » dans</h2>
    <form class="formulaire" onsubmit={(e) => { e.preventDefault(); void validerDestination(); }}>
      <select bind:value={destination.choix} size={Math.min(10, destination.liste.length)} aria-label="Dossier de destination">
        {#each destination.liste as d (String(d.id))}
          <option value={String(d.id)} disabled={d.actuel}>{'\u00a0\u00a0'.repeat(d.profondeur)}{d.id ? '📁 ' : ''}{d.nom}{d.actuel ? ' (actuel)' : ''}</option>
        {/each}
      </select>
      <div class="pied">
        <button type="button" onclick={() => (destination = null)}>Annuler</button>
        <button type="submit" class="principal" disabled={!destination.choix}>Déplacer</button>
      </div>
    </form>
  </dialog>
{/if}

{#if importCbl}
  <dialog use:modale={{ ouvrirAuMontage: true, peutFermer: () => !importCbl?.cours && (!!importCbl?.importe || !importCbl?.chemin.trim()), fermer: () => (importCbl = null) }}
    aria-labelledby="titre-import" data-testid="dialogue-import-cbl">
    <h2 id="titre-import">Importer une liste de lecture (.cbl)</h2>
    <form class="formulaire" onsubmit={(e) => { e.preventDefault(); void (importCbl?.rapport ? importer() : analyser()); }}>
      <label for="chemin-cbl">Chemin du fichier .cbl (ComicRack, Comic Book Reading Lists…) :</label>
      <input id="chemin-cbl" type="text" bind:value={importCbl.chemin} placeholder="D:\Listes\Thanos.cbl" spellcheck="false" autocomplete="off"
        disabled={importCbl.importe} oninput={() => { if (importCbl) importCbl.rapport = null; }} data-testid="chemin-cbl" />
      {#if importCbl.rapport}
        {@const r = importCbl.rapport}
        <div class="rapport" data-testid="rapport-cbl">
          <p><strong>{r.trouves} / {r.total}</strong> numéros trouvés dans la bibliothèque
            ({r.parIdentifiant} par identifiant, {r.parSerie} par série et numéro{r.doublons ? `, ${r.doublons} doublon${r.doublons > 1 ? 's' : ''}` : ''}){r.total > r.trouves ? `, ${r.total - r.trouves} manquant${r.total - r.trouves > 1 ? 's' : ''}` : ''}.</p>
          {#if r.manquants.length}
            <details open>
              <summary>Numéros manquants</summary>
              <ul class="manquants">{#each r.manquants as m, k (k)}<li>{libelleManquant(m)}</li>{/each}</ul>
              {#if r.total - r.trouves > r.manquants.length}<p class="discret">… et {r.total - r.trouves - r.manquants.length} autres (bilan complet : « Copier le bilan »).</p>{/if}
            </details>
          {/if}
          <button type="button" onclick={() => void navigator.clipboard?.writeText(r.texte)}>📋 Copier le bilan</button>
        </div>
        {#if !importCbl.importe}
          <label for="nom-cbl">Nom de la nouvelle liste :</label>
          <input id="nom-cbl" type="text" bind:value={importCbl.nom} spellcheck="false" data-testid="nom-cbl" />
        {:else}
          <p class="ok" role="status">✔ Liste « {importCbl.nom || r.nom} » créée avec {r.trouves} livre{r.trouves > 1 ? 's' : ''}.</p>
        {/if}
      {/if}
      {#if importCbl.erreur}<p class="erreur" role="alert">{importCbl.erreur}</p>{/if}
      <div class="pied">
        {#if importCbl.importe}
          <button type="button" class="principal" onclick={() => (importCbl = null)} data-testid="fermer-import">Fermer</button>
        {:else}
          <button type="button" onclick={() => (importCbl = null)}>Annuler</button>
          {#if importCbl.rapport}
            <button type="submit" class="principal" disabled={importCbl.cours} data-testid="importer-cbl">{importCbl.cours ? 'Import…' : 'Importer'}</button>
          {:else}
            <button type="submit" class="principal" disabled={!importCbl.chemin.trim() || importCbl.cours} data-testid="analyser-cbl">{importCbl.cours ? 'Analyse…' : 'Analyser'}</button>
          {/if}
        {/if}
      </div>
    </form>
  </dialog>
{/if}

{#if exportCbl}
  <dialog use:modale={{ ouvrirAuMontage: true, peutFermer: () => !exportCbl?.chemin.trim(), fermer: () => (exportCbl = null) }}
    aria-labelledby="titre-export" data-testid="dialogue-export-cbl">
    <h2 id="titre-export">Exporter « {exportCbl.nom} » en .cbl</h2>
    <form class="formulaire" onsubmit={(e) => { e.preventDefault(); void exporter(); }}>
      <label for="chemin-export">Fichier à écrire (l'extension .cbl est ajoutée si besoin) :</label>
      <input id="chemin-export" type="text" bind:value={exportCbl.chemin} placeholder={`D:\\Listes\\${exportCbl.nom}.cbl`} spellcheck="false" autocomplete="off" data-testid="chemin-export" />
      {#if exportCbl.erreur}<p class="erreur" role="alert">{exportCbl.erreur}</p>{/if}
      {#if exportCbl.bilan}<p class="ok" role="status">✔ {exportCbl.bilan}</p>{/if}
      <div class="pied">
        <button type="button" onclick={() => (exportCbl = null)}>{exportCbl.bilan ? 'Fermer' : 'Annuler'}</button>
        <button type="submit" class="principal" disabled={!exportCbl.chemin.trim()} data-testid="exporter-cbl">Exporter</button>
      </div>
    </form>
  </dialog>
{/if}

<style>
  .panneau { display: flex; flex-direction: column; min-height: 0; flex: 1; margin-top: 10px; border-top: 1px solid var(--cr-bordure); padding-top: 6px; }
  .tete { display: flex; align-items: center; gap: 6px; padding: 2px 4px 4px 10px; }
  .titre { flex: 1; font-size: 12px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.04em; color: var(--cr-texte-discret); }
  .cadenas { font-size: 12px; }
  .plus { padding: 0 8px; line-height: 22px; }
  .lecture-seule { margin: 0 4px 6px; padding: 6px 8px; font-size: 12px; border-radius: var(--cr-rayon); background: var(--cr-fond-survol); color: var(--cr-texte-discret); }
  .lecture-seule.comicrack { color: var(--cr-erreur); border: 1px solid var(--cr-erreur); }
  .arbre { flex: 1; min-height: 60px; overflow: auto; outline: none; padding: 2px 0; border-radius: var(--cr-rayon); }
  .arbre:focus-visible { box-shadow: inset 0 0 0 2px var(--cr-accent); }
  .ligne {
    display: flex; align-items: center; gap: 4px; height: 26px; padding-right: 6px; cursor: pointer; user-select: none;
    border-radius: var(--cr-rayon); border: 1px solid transparent; position: relative;
  }
  .ligne:hover { background: var(--cr-fond-survol); }
  .ligne.courante { background: var(--cr-accent); color: var(--cr-accent-texte); }
  .ligne.courante .compte { color: inherit; }
  .ligne.cible-avant { box-shadow: inset 0 2px 0 var(--cr-accent); }
  .ligne.cible-apres { box-shadow: inset 0 -2px 0 var(--cr-accent); }
  .ligne.cible-dans, .ligne.depot-ok { border-color: var(--cr-accent); background: color-mix(in srgb, var(--cr-accent) 22%, transparent); }
  .ligne.cible-refus, .ligne.depot-refus { border-color: var(--cr-erreur); cursor: not-allowed; }
  .triangle { width: 16px; flex: none; padding: 0; border: 0; background: none; color: inherit; font-size: 11px; line-height: 1; }
  .glyphe { width: 18px; flex: none; text-align: center; font-size: 13px; }
  .nom { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .compte { font-size: 11px; color: var(--cr-texte-discret); font-variant-numeric: tabular-nums; }
  .compte.inconnu { font-weight: 700; }
  .renommage { flex: 1; min-width: 0; padding: 1px 4px; }
  .info { display: flex; gap: 6px; align-items: flex-start; margin: 6px 4px 0; padding: 6px 8px; font-size: 12px; border-radius: var(--cr-rayon); background: var(--cr-fond-survol); white-space: pre-line; }
  .info span { flex: 1; overflow-wrap: anywhere; }
  .info.erreur, .erreur, .avertissement { color: var(--cr-erreur); }
  .fermer-info { padding: 0 4px; border: 0; background: none; font-size: 11px; }
  .discret { color: var(--cr-texte-discret); font-size: 12px; padding: 4px 10px; margin: 0; }
  .formulaire { display: grid; gap: 8px; }
  .rapport { padding: 8px; border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); background: var(--cr-fond); }
  .rapport p { margin: 0 0 6px; }
  .pied { display: flex; justify-content: flex-end; gap: 8px; margin-top: 12px; }
  .manquants { max-height: 30vh; overflow: auto; margin: 6px 0; font-size: 12px; }
  .ok { color: var(--cr-accent); }
  button.principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); font-weight: 600; }
  button.danger { background: var(--cr-erreur); color: var(--cr-accent-texte); border-color: var(--cr-erreur); font-weight: 600; }
</style>
