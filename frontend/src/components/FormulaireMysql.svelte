<script module lang="ts">
  // Formulaire de connexion MySQL (serveur, port, utilisateur, mot de passe masqué, base) avec le
  // bouton « Tester ». Porté de `MySqlForm` de la version egui. Le mot de passe saisi n'est envoyé
  // qu'au backend : il n'est jamais relu (seulement « défini oui / non »).
  import type { ParametresMysql } from '../lib/api';

  /** Valeurs du formulaire (le port reste un texte tant qu'il est saisi). */
  export interface ValeursMysql {
    serveur: string;
    port: string;
    utilisateur: string;
    motDePasse: string;
    base: string;
  }

  /** Formulaire vide (port par défaut 3306). */
  export const mysqlVide = (): ValeursMysql => ({ serveur: '', port: '3306', utilisateur: '', motDePasse: '', base: '' });

  /** Paramètres à envoyer, ou le texte de l'erreur de saisie. */
  export function versParametres(v: ValeursMysql, memorise = false): ParametresMysql | string {
    const port = Number(v.port.trim());
    if (!v.serveur.trim() || !v.utilisateur.trim() || !v.base.trim()) return 'Serveur, utilisateur et base sont nécessaires.';
    if (!Number.isInteger(port) || port < 1 || port > 65535) return 'Port invalide (1 à 65535).';
    return {
      serveur: v.serveur.trim(), port, utilisateur: v.utilisateur.trim(), base: v.base.trim(),
      // Champ vide avec un mot de passe déjà enregistré : le backend garde l'ancien.
      ...(v.motDePasse === '' && memorise ? {} : { motDePasse: v.motDePasse }),
    };
  }
</script>

<script lang="ts">
  import { messageErreur, testerConnexionMysql, type CibleTestMysql } from '../lib/api';

  interface Props {
    valeurs: ValeursMysql;
    cible: CibleTestMysql;
    /** Un mot de passe est déjà enregistré pour cette connexion (champ laissé vide = on le garde). */
    memorise?: boolean;
    /** Préfixe des `data-testid`. */
    id: string;
  }
  let { valeurs = $bindable(), cible, memorise = false, id }: Props = $props();

  let occupe = $state(false);
  let resultat = $state<{ ok: boolean; texte: string } | null>(null);

  async function tester() {
    const p = versParametres(valeurs, memorise);
    if (typeof p === 'string') {
      resultat = { ok: false, texte: p };
      return;
    }
    occupe = true;
    resultat = null;
    try {
      const r = await testerConnexionMysql(cible, p);
      resultat = { ok: true, texte: `Connexion réussie : ${r.description} (${r.nbLivres} livre${r.nbLivres > 1 ? 's' : ''}).` };
    } catch (e) {
      resultat = { ok: false, texte: messageErreur(e) };
    } finally {
      occupe = false;
    }
  }
</script>

<div class="mysql" data-testid={id}>
  <label><span>Serveur</span>
    <input type="text" bind:value={valeurs.serveur} autocomplete="off" spellcheck="false" data-testid="{id}-serveur" oninput={() => (resultat = null)} /></label>
  <label><span>Port</span>
    <input type="text" inputmode="numeric" bind:value={valeurs.port} autocomplete="off" class="court" data-testid="{id}-port" oninput={() => (resultat = null)} /></label>
  <label><span>Utilisateur</span>
    <input type="text" bind:value={valeurs.utilisateur} autocomplete="off" spellcheck="false" data-testid="{id}-utilisateur" oninput={() => (resultat = null)} /></label>
  <label><span>Mot de passe</span>
    <input type="password" bind:value={valeurs.motDePasse} autocomplete="off"
      placeholder={memorise ? 'Enregistré (laisser vide pour le garder)' : ''} data-testid="{id}-mot-de-passe" oninput={() => (resultat = null)} /></label>
  <label><span>Base</span>
    <input type="text" bind:value={valeurs.base} autocomplete="off" spellcheck="false" data-testid="{id}-base" oninput={() => (resultat = null)} /></label>
  <div class="actions">
    <button type="button" class="secondaire" disabled={occupe} onclick={() => void tester()} data-testid="{id}-tester">
      {occupe ? 'Test…' : 'Tester'}
    </button>
    {#if resultat}
      <span class={resultat.ok ? 'ok' : 'echec'} role={resultat.ok ? 'status' : 'alert'} data-testid="{id}-resultat">{resultat.texte}</span>
    {/if}
  </div>
</div>

<style>
  .mysql { display: grid; gap: 8px; padding: 12px; border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon); }
  label { display: grid; grid-template-columns: 110px 1fr; align-items: center; gap: 8px; }
  input {
    font: inherit; color: var(--cr-texte); background: var(--cr-fond); border: 1px solid var(--cr-bordure);
    border-radius: var(--cr-rayon); padding: 6px 10px; min-width: 0;
  }
  input.court { width: 90px; }
  input:focus-visible, button:focus-visible { outline: 2px solid var(--cr-accent); outline-offset: 1px; }
  .actions { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .secondaire {
    font: inherit; cursor: pointer; padding: 6px 18px; color: var(--cr-texte); background: var(--cr-fond-panneau);
    border: 1px solid var(--cr-bordure); border-radius: var(--cr-rayon);
  }
  .secondaire:hover:not(:disabled) { background: var(--cr-fond-survol); }
  .secondaire:disabled { opacity: 0.5; cursor: default; }
  .ok { color: var(--cr-succes, var(--cr-texte)); }
  .echec { color: var(--cr-erreur); white-space: pre-line; }
</style>
