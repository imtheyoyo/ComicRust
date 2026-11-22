<script lang="ts">
  // Lot L7, écart 18 : « 📋 Copier les livres » (rapport des livres affichés pour comparer avec ComicRack,
  // `shown_books_report` de la version egui) et « ⟳ Relire » (relit toute la bibliothèque depuis sa source,
  // `load_library`). Deux boutons autonomes pour ne presque pas toucher VueBibliotheque.
  import { messageErreur, rapportLivres, relireBibliotheque, type RequeteLivres } from '../lib/api';

  interface Props {
    /** `copier` : bouton de l'en-tête d'une liste ; `relire` : bouton de la barre d'outils. */
    type: 'copier' | 'relire';
    /** Requête de la vue affichée (recherche, tri, liste, pile ouverte) ; `debut` et `nombre` sont ignorés. */
    requete?: RequeteLivres;
    /** Nom de la liste affichée (titre du rapport). */
    titre?: string;
    /** Relecture terminée : la vue se recharge. */
    onrelu?: () => void;
  }
  let { type, requete, titre = '', onrelu }: Props = $props();

  let etat = $state<'repos' | 'occupe' | 'ok' | 'erreur'>('repos');
  let message = $state('');
  let minuterie: ReturnType<typeof setTimeout> | undefined;

  function signaler(e: 'ok' | 'erreur', m: string) {
    etat = e;
    message = m;
    clearTimeout(minuterie);
    minuterie = setTimeout(() => { etat = 'repos'; message = ''; }, e === 'ok' ? 2500 : 6000);
  }

  async function copier() {
    if (!requete || etat === 'occupe') return;
    etat = 'occupe';
    try {
      const texte = await rapportLivres({ ...requete, debut: 0, nombre: 0 }, titre);
      await navigator.clipboard.writeText(texte);
      signaler('ok', 'Liste copiée ✓');
    } catch (e) {
      signaler('erreur', `Copie impossible : ${messageErreur(e)}`);
    }
  }

  async function relire() {
    if (etat === 'occupe') return;
    etat = 'occupe';
    try {
      await relireBibliotheque();
      signaler('ok', 'Relue ✓');
      onrelu?.();
    } catch (e) {
      signaler('erreur', messageErreur(e));
    }
  }
</script>

{#if type === 'copier'}
  <button type="button" onclick={() => void copier()} disabled={etat === 'occupe'}
    title="Liste des livres affichés (série, numéro, fichier), pour comparer avec ComicRack" data-testid="copier-livres">
    📋 Copier les livres
  </button>
{:else}
  <button type="button" onclick={() => void relire()} disabled={etat === 'occupe'}
    title="Relire toute la bibliothèque depuis sa source" data-testid="relire">
    ⟳ Relire
  </button>
{/if}
{#if message}<span class={etat === 'erreur' ? 'erreur' : 'ok'} role={etat === 'erreur' ? 'alert' : 'status'} data-testid="{type}-message">{message}</span>{/if}

<style>
  .ok { color: var(--cr-accent); font-size: 12px; }
  .erreur { color: var(--cr-erreur); font-size: 12px; }
</style>
