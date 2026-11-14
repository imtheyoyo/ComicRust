<script lang="ts">
  // Réglages : choix du thème (fichiers CSS de data\themes : aperçu en direct, OK / Annuler, import,
  // export, suppression, réinitialisation des thèmes livrés), dossiers du mode portable et, en mode
  // lié à ComicRack, l'option « Lecture et écriture » (section `edition` de data\comicrust-tauri.json).
  import { fermerModale, modale } from '../lib/modale';
  import {
    enregistrerReglagesEdition, exporterTheme, importerTheme, infos, lireReglagesEdition, listerThemes, messageErreur,
    reinitialiserThemesLivres, revelerDossierThemes, supprimerTheme, type InfosApplication, type ModeBiblio, type ThemeInfo,
  } from '../lib/api';
  import {
    appliquerTheme, apercuTheme, messageTheme, rechargerThemeCourant, restaurerTheme, THEME_DEFAUT, themeApplique, themePersiste,
  } from '../lib/theme';

  interface Props {
    themeActif: string;
    onTheme: (id: string) => void;
    /** Mode de la bibliothèque : l'option d'écriture ComicRack n'est proposée qu'en mode lié. */
    mode?: ModeBiblio;
    /** L'option « Lecture et écriture » a changé (les listes relisent leur état). */
    onecriture?: () => void;
    /** « Transférer la bibliothèque ComicRack… » : ferme les réglages et ouvre la fenêtre du transfert. */
    ontransfert?: () => void;
    /** « Rechercher une mise à jour… » : ferme les réglages et ouvre la fenêtre « Mise à jour ». */
    onmaj?: () => void;
  }
  let { themeActif, onTheme, mode = 'autonome', onecriture, ontransfert, onmaj }: Props = $props();
  let ecritureComicrack = $state(false);
  let confirmerEcriture = $state(false);
  let enregistre = $state('');

  let dialogue = $state<HTMLDialogElement>();
  let themes = $state<ThemeInfo[]>([]);
  let info = $state<InfosApplication | null>(null);
  let erreur = $state('');
  /** Thème choisi dans la liste, pas encore enregistré (OK l'enregistre, Annuler l'abandonne). */
  let choix = $state(THEME_DEFAUT);
  /** Le thème choisi a un CSS inutilisable : l'affichage reste sur le dernier thème valide. */
  let invalide = $state(false);
  let messageInfo = $state('');
  let confirmation = $state<'supprimer' | 'reinitialiser' | null>(null);
  let champFichier = $state<HTMLInputElement>();
  let ouvert = $state(false);

  const themeChoisi = $derived(themes.find((t) => t.id === choix));
  /** Avertissements du thème choisi (@import, url(http…) ignorés…). */
  const avertissements = $derived(themeChoisi?.avertissements ?? []);

  export async function ouvrir() {
    choix = themeActif || themePersiste();
    invalide = false;
    erreur = '';
    messageInfo = '';
    confirmation = null;
    ouvert = true;
    dialogue?.showModal();
    await actualiser();
  }

  /** Relit la liste des thèmes et le CSS du thème affiché (fichier modifié à la main, thème supprimé). */
  async function actualiser() {
    try {
      themes = await listerThemes();
      const avant = themeApplique();
      const id = await rechargerThemeCourant();
      if (id !== avant) {
        // Le thème affiché a disparu : repli sur le thème par défaut (enregistré aussi si c'était lui).
        choix = id;
        if (themePersiste() === id) onTheme(id);
        messageInfo = 'Le thème affiché n\'existe plus : retour au thème par défaut.';
      }
      invalide = false;
      if (!themes.some((t) => t.id === choix)) choix = THEME_DEFAUT;
      info = await infos();
      if (mode === 'comicrack') ecritureComicrack = (await lireReglagesEdition()).ecritureComicrack;
      erreur = '';
    } catch (e) {
      erreur = messageTheme(e);
    }
  }

  /** Retour du focus sur la fenêtre : relit les thèmes (un fichier a pu être ajouté ou modifié). */
  function auFocus() {
    if (ouvert) void actualiser();
    else void rechargerThemeCourant().then((id) => { if (id !== themeActif) onTheme(id); }).catch(() => { /* garde le thème affiché */ });
  }
  $effect(() => {
    window.addEventListener('focus', auFocus);
    return () => window.removeEventListener('focus', auFocus);
  });

  /** Choisit un thème (clic, flèches, « Thème par défaut ») : aperçu en direct, enregistré par OK. */
  async function selectionner(id: string) {
    choix = id;
    messageInfo = '';
    try {
      await apercuTheme(id);
      invalide = false;
      erreur = '';
    } catch (e) {
      invalide = true;
      erreur = `Le thème « ${themes.find((t) => t.id === id)?.nom ?? id} » est inutilisable : ${messageTheme(e)} L'affichage reste sur le dernier thème valide.`;
    }
  }

  /** Survol : applique temporairement le thème sous la souris ; la sortie de la liste revient au choix. */
  function survoler(id: string | null) {
    if (!ouvert || confirmation) return;
    if (id === null && invalide) return;
    void apercuTheme(id ?? choix).catch(() => { /* aperçu impossible : l'affichage ne change pas */ });
  }

  async function valider() {
    try {
      onTheme(await appliquerTheme(choix, { persister: true }));
      fermerModale(dialogue);
    } catch (e) {
      invalide = true;
      erreur = messageTheme(e);
    }
  }

  /** Fermeture (OK, Annuler, Échap) : tout aperçu non validé est annulé. */
  function auClose() {
    ouvert = false;
    confirmation = null;
    if (themeApplique() !== themePersiste()) void restaurerTheme().catch((e) => { erreur = messageTheme(e); });
  }

  async function importer(e: Event & { currentTarget: HTMLInputElement }) {
    const fichier = e.currentTarget.files?.[0];
    e.currentTarget.value = '';
    if (!fichier) return;
    erreur = '';
    messageInfo = '';
    try {
      if (fichier.size > 1024 * 1024) throw new Error('fichier trop volumineux pour un thème (1 Mo maximum).');
      const css = await fichier.text();
      const t = await importerTheme(fichier.name.replace(/\.css$/i, ''), css);
      themes = await listerThemes();
      await selectionner(t.id);
      if (invalide) return;
      messageInfo = `Thème « ${t.nom} » importé${t.avertissements.length ? ' avec avertissements (voir ci-dessous)' : ''}. Cliquez sur OK pour l'adopter.`;
    } catch (err) {
      erreur = `Import refusé : ${messageErreur(err)}`;
    }
  }

  async function copierCss() {
    erreur = '';
    try {
      await navigator.clipboard.writeText(await exporterTheme(choix));
      messageInfo = `CSS du thème « ${themeChoisi?.nom ?? choix} » copié dans le presse-papiers.`;
    } catch (err) {
      erreur = `Copie impossible : ${messageErreur(err)}`;
    }
  }

  async function ouvrirDossier() {
    erreur = '';
    try {
      await revelerDossierThemes();
    } catch (err) {
      erreur = messageErreur(err);
    }
  }

  async function supprimer() {
    const t = themeChoisi;
    confirmation = null;
    if (!t || t.livre) return;
    try {
      await supprimerTheme(t.id);
      themes = await listerThemes();
      if (themePersiste() === t.id) onTheme(await appliquerTheme(THEME_DEFAUT));
      await selectionner(THEME_DEFAUT);
      messageInfo = `Thème « ${t.nom} » supprimé.`;
    } catch (err) {
      erreur = messageErreur(err);
    }
  }

  async function reinitialiser() {
    confirmation = null;
    try {
      const ids = await reinitialiserThemesLivres();
      await actualiser();
      messageInfo = `${ids.length} thème(s) livré(s) recopié(s) ; les anciens fichiers sont conservés en « .css.avant ».`;
    } catch (err) {
      erreur = messageErreur(err);
    }
  }

  /** Cocher demande d'abord de lire l'avertissement ; décocher (retour à la lecture seule) est immédiat. */
  function basculerEcriture(e: Event & { currentTarget: HTMLInputElement }) {
    if (e.currentTarget.checked && !ecritureComicrack) {
      e.currentTarget.checked = false;
      confirmerEcriture = true;
      return;
    }
    void changerEcriture(e.currentTarget.checked);
  }

  async function changerEcriture(actif: boolean) {
    confirmerEcriture = false;
    try {
      ecritureComicrack = (await enregistrerReglagesEdition({ ecritureComicrack: actif })).ecritureComicrack;
      enregistre = ecritureComicrack ? 'Lecture et écriture activée.' : 'Lecture seule rétablie.';
      onecriture?.();
    } catch (err) {
      erreur = messageErreur(err);
    }
  }
</script>

<dialog bind:this={dialogue} use:modale aria-labelledby="titre-reglages" onclose={auClose}>
  <h2 id="titre-reglages">Réglages</h2>
  <!-- En tête pour rester visible même quand la fenêtre défile. -->
  {#if erreur}<p class="erreur" role="alert" data-testid="erreur-reglages">{erreur}</p>{/if}
  <fieldset class="themes" data-testid="reglage-theme">
    <legend>Thème</legend>
    <div class="ligne entete-themes">
      <button type="button" onclick={() => void selectionner(THEME_DEFAUT)} data-testid="theme-defaut"
        title="Revenir au thème Moderne clair (à valider par OK)">Thème par défaut</button>
      <button type="button" onclick={actualiser} data-testid="theme-recharger"
        title="Relire la liste et le fichier du thème (rechargement aussi automatique au retour dans la fenêtre)">Recharger</button>
    </div>
    <!-- Liste de boutons radio : flèches = aperçu en direct, survol = aperçu temporaire. -->
    <div class="liste-themes" role="group" aria-label="Thème" onmouseleave={() => survoler(null)} data-testid="liste-themes">
      {#each themes as t (t.id)}
        <label class="theme" class:choisi={choix === t.id} onmouseenter={() => survoler(t.id)} data-testid="theme-{t.id}">
          <input type="radio" name="theme" value={t.id} checked={choix === t.id} onchange={() => void selectionner(t.id)} />
          <span class="pastille" aria-hidden="true"
            style:background={t.couleurs?.fond ?? 'transparent'} style:border-color={t.couleurs?.texte ?? 'currentColor'}>
            <i style:background={t.couleurs?.accent ?? 'currentColor'}></i><b style:background={t.couleurs?.texte ?? 'currentColor'}></b>
          </span>
          <span class="nom-theme">{t.nom}</span>
          {#if !t.livre}<span class="badge">perso</span>{/if}
          {#if t.avertissements.length}<span class="attention" title="Ce thème contient des constructions ignorées" aria-label="avertissements">⚠</span>{/if}
        </label>
      {/each}
    </div>
    <div class="echantillon" aria-label="Échantillon du thème" data-testid="echantillon-theme">
      <button type="button" tabindex="-1">Bouton</button>
      <button type="button" tabindex="-1" class="principal">Principal</button>
      <input type="text" value="Champ de saisie" readonly tabindex="-1" aria-label="Exemple de champ" />
      <span class="carte-ex"><strong>Carte</strong><span class="discret">texte discret</span></span>
      <a href="#echantillon" onclick={(e) => e.preventDefault()} tabindex="-1">lien</a>
    </div>
    <div class="ligne actions-themes">
      <input bind:this={champFichier} type="file" accept=".css,text/css" class="cache" onchange={importer} data-testid="theme-fichier" tabindex="-1" />
      <button type="button" onclick={() => champFichier?.click()} data-testid="theme-importer">Importer un thème…</button>
      <button type="button" onclick={copierCss} disabled={!themeChoisi} data-testid="theme-copier">Copier le CSS</button>
      <button type="button" onclick={ouvrirDossier} data-testid="theme-dossier">Ouvrir le dossier des thèmes</button>
      <button type="button" onclick={() => (confirmation = 'supprimer')} disabled={!themeChoisi || themeChoisi.livre}
        title={themeChoisi?.livre ? 'Un thème livré ne se supprime pas' : 'Supprimer ce thème personnel'} data-testid="theme-supprimer">Supprimer</button>
      <button type="button" onclick={() => (confirmation = 'reinitialiser')} data-testid="theme-reinitialiser">Réinitialiser les thèmes livrés</button>
    </div>
    {#if confirmation === 'supprimer' && themeChoisi}
      <div class="avertissement" role="alertdialog" aria-label="Confirmer la suppression" data-testid="confirmer-suppression-theme">
        <p>Supprimer définitivement le thème « {themeChoisi.nom} » ?</p>
        <div class="boutons">
          <button type="button" onclick={() => (confirmation = null)}>Annuler</button>
          <button type="button" class="principal" onclick={() => void supprimer()} data-testid="theme-supprimer-ok">Supprimer</button>
        </div>
      </div>
    {:else if confirmation === 'reinitialiser'}
      <div class="avertissement" role="alertdialog" aria-label="Confirmer la réinitialisation" data-testid="confirmer-reinit-themes">
        <p>Recopier les thèmes livrés ? Vos modifications de ces fichiers sont conservées dans des copies « .css.avant » ; vos thèmes personnels ne sont pas touchés.</p>
        <div class="boutons">
          <button type="button" onclick={() => (confirmation = null)}>Annuler</button>
          <button type="button" class="principal" onclick={() => void reinitialiser()} data-testid="theme-reinit-ok">Réinitialiser</button>
        </div>
      </div>
    {/if}
    {#if avertissements.length}
      <div class="avis" role="status" data-testid="avertissements-theme">
        <strong>⚠ Avertissements pour ce thème</strong>
        <ul>{#each avertissements as a}<li>{a}</li>{/each}</ul>
      </div>
    {/if}
    {#if messageInfo}<p class="ok" role="status">{messageInfo}</p>{/if}
    <p class="discret petit">Survolez ou parcourez la liste aux flèches pour voir le thème ; « OK » l'enregistre, « Annuler » rétablit l'ancien.</p>
  </fieldset>
  {#if mode === 'comicrack'}
    <fieldset class="comicrack" data-testid="reglage-ecriture">
      <legend>Bibliothèque ComicRack</legend>
      <label class="case">
        <input type="checkbox" checked={ecritureComicrack} onchange={basculerEcriture} data-testid="ecriture-comicrack" />
        Lecture et écriture (lecture, fichiers convertis, infos écrites sont enregistrés dans la base de ComicRack ;
        listes modifiables, ComicRack fermé)
      </label>
      <p class="discret petit">
        {ecritureComicrack
          ? 'ComicRack lancé voit les modifications en quelques secondes. ComicRust ne réécrit que les éléments concernés de la fiche (page lue, date d\'ouverture, fichier…), jamais le reste. Les listes (dans ComicDb.xml) ne sont modifiables que ComicRack fermé.'
          : 'Lecture seule : la lecture faite dans ComicRust est gardée dans ses réglages ; ni la base ni les listes de ComicRack ne sont modifiées.'}
        Sans effet pour une bibliothèque ComicDb.xml (fiches toujours en lecture seule).
      </p>
      {#if confirmerEcriture}
        <div class="avertissement" role="alertdialog" aria-label="Avertissement" data-testid="avertissement-ecriture">
          <p><strong>⚠ ComicRust va écrire dans la base de ComicRack.</strong></p>
          <p>Faites d'abord une sauvegarde de la base (MySQL). Une fiche modifiée entre-temps dans ComicRack n'est jamais écrasée
            (conflit signalé), mais évitez de modifier les mêmes livres dans les deux programmes en même temps.</p>
          <div class="boutons">
            <button onclick={() => (confirmerEcriture = false)}>Annuler</button>
            <button class="principal" onclick={() => void changerEcriture(true)} data-testid="confirmer-ecriture">Activer l'écriture</button>
          </div>
        </div>
      {/if}
      {#if enregistre}<p class="ok" role="status">{enregistre}</p>{/if}
    </fieldset>
  {/if}
  {#if ontransfert}
    <fieldset class="comicrack" data-testid="reglage-transfert">
      <legend>Passer de ComicRack à ComicRust</legend>
      <p class="discret petit transfert">Copie les livres, la lecture, les listes et les dossiers de ComicRack dans la base de ComicRust
        (ComicRack n'est pas modifié ; le transfert peut être relancé sans doublon).</p>
      <button onclick={() => { fermerModale(dialogue); ontransfert(); }} data-testid="ouvrir-transfert">Transférer la bibliothèque ComicRack…</button>
    </fieldset>
  {/if}
  {#if onmaj}
    <fieldset class="comicrack" data-testid="reglage-maj">
      <legend>Mise à jour</legend>
      <p class="discret petit transfert">Cherche une nouvelle version de ComicRust sur GitHub ; le paquet est vérifié avant d'être installé.</p>
      <button onclick={() => { fermerModale(dialogue); onmaj(); }} data-testid="ouvrir-maj">Rechercher une mise à jour…</button>
    </fieldset>
  {/if}
  {#if info}
    <p class="discret">
      Les thèmes sont des fichiers <code>.css</code> dans <code>{info.dossier_themes}</code> : ajoutez ou modifiez-en un,
      puis « Recharger » : aucun redémarrage ni recompilation.
    </p>
    <dl>
      <dt>Version</dt><dd>{info.version}</dd>
      <dt>Données</dt><dd><code>{info.dossier_data}</code></dd>
      <dt>Profil WebView2</dt><dd><code>{info.dossier_webview}</code></dd>
    </dl>
  {/if}
  <div class="pied">
    <button type="button" onclick={() => fermerModale(dialogue)} data-testid="reglages-annuler">Annuler</button>
    <button type="button" class="principal" onclick={() => void valider()} disabled={invalide} data-testid="reglages-ok">OK</button>
  </div>
</dialog>

<style>
  .ligne { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .discret { color: var(--cr-texte-discret); }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 4px 12px; margin: 0 0 12px; }
  dt { color: var(--cr-texte-discret); }
  dd { margin: 0; word-break: break-all; }
  code { font-family: ui-monospace, Consolas, monospace; background: var(--cr-fond-survol); padding: 1px 5px; border-radius: 3px; }
  .erreur { color: var(--cr-erreur); margin: 0 0 10px; padding: 6px 10px; border: 1px solid var(--cr-erreur); border-radius: var(--cr-rayon); }
  .comicrack { border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); margin: 12px 0; padding: 8px 12px; }
  .comicrack legend { padding: 0 4px; font-weight: 600; }
  .case { display: flex; gap: 8px; align-items: flex-start; }
  .case input { margin-top: 3px; accent-color: var(--cr-accent); }
  .petit { font-size: 12px; margin: 6px 0 0 24px; }
  .petit.transfert { margin: 0 0 8px; }
  .avertissement { margin: 8px 0 0; padding: 8px 10px; border: 1px solid var(--cr-erreur); border-radius: var(--cr-rayon); }
  .avertissement p { margin: 0 0 6px; }
  .avertissement strong { color: var(--cr-erreur); }
  .boutons { display: flex; justify-content: flex-end; gap: 8px; }
  .ok { color: var(--cr-accent); margin: 6px 0 0; }
  button.principal { background: var(--cr-accent); color: var(--cr-accent-texte); border-color: var(--cr-accent); }
  /* OK / Annuler restent visibles quand la fenêtre défile (petit écran). */
  .pied {
    display: flex; justify-content: flex-end; gap: 8px; margin: 12px -20px -20px; padding: 10px 20px;
    position: sticky; bottom: -20px; background: var(--cr-fond-panneau); border-top: 1px solid var(--cr-bordure);
  }
  .themes { border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); margin: 0 0 12px; padding: 8px 12px; }
  .themes legend { padding: 0 4px; font-weight: 600; }
  .entete-themes { margin-bottom: 8px; }
  .liste-themes { display: grid; grid-template-columns: repeat(auto-fill, minmax(190px, 1fr)); gap: 4px; max-height: 190px; overflow-y: auto; padding: 2px; }
  .theme {
    display: flex; align-items: center; gap: 8px; padding: 5px 8px; cursor: pointer;
    border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon);
  }
  .theme:hover { background: var(--cr-fond-survol); }
  .theme.choisi { border-color: var(--cr-accent); background: var(--cr-fond-selection); }
  .theme:has(input:focus-visible) { outline: 2px solid var(--cr-focus); outline-offset: 1px; }
  .theme input { margin: 0; }
  .nom-theme { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* Pastille : fond du thème, point d'accent et trait de texte (couleurs lues dans le fichier CSS). */
  .pastille { position: relative; flex: none; width: 34px; height: 20px; border: 2px solid; border-radius: 6px; overflow: hidden; }
  .pastille i { position: absolute; right: 3px; top: 3px; width: 10px; height: 10px; border-radius: 50%; }
  .pastille b { position: absolute; left: 3px; bottom: 3px; width: 14px; height: 3px; border-radius: 2px; }
  .badge { flex: none; font-size: 11px; padding: 0 6px; border: 1px solid var(--cr-bordure); border-radius: 999px; color: var(--cr-texte-discret); }
  .attention { flex: none; color: var(--cr-avertissement); }
  .echantillon {
    display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: 8px 0; padding: 8px 10px;
    background: var(--cr-fond); border: 1px dashed var(--cr-bordure); border-radius: var(--cr-rayon);
  }
  .echantillon input { width: 140px; }
  .echantillon a { color: var(--cr-accent); }
  .carte-ex { display: inline-flex; flex-direction: column; padding: 4px 10px; background: var(--cr-fond-panneau); border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); box-shadow: var(--cr-ombre); }
  .actions-themes { gap: 6px; }
  .cache { position: absolute; width: 1px; height: 1px; opacity: 0; pointer-events: none; }
  .avis { margin: 8px 0 0; padding: 6px 10px; border: 1px solid var(--cr-avertissement); border-radius: var(--cr-rayon); }
  .avis strong { color: var(--cr-avertissement); }
  .avis ul { margin: 4px 0 0; padding-left: 20px; }
  .themes .ok { margin: 8px 0 0; }
</style>
