<script lang="ts">
  // Menu « 🕘 Récents » de la bibliothèque : les 15 derniers fichiers ouverts (bibliothèque, lecteur,
  // fichier passé en argument ou déposé), comme le menu de la barre d'outils de la version egui
  // (`Settings::recent`). La liste est tenue par le backend (data\comicrust-tauri.json, bloc
  // `recents`) ; chaque entrée est vérifiée à l'ouverture du menu : un fichier absent est grisé.
  import { onMount } from 'svelte';
  import { recents } from '../lib/recents.svelte';
  import MenuContextuel, { type EntreeMenu } from './MenuContextuel.svelte';

  interface Props {
    /** Lire ce fichier (ou dossier d'images) dans le lecteur. */
    onouvrir: (chemin: string) => void | Promise<void>;
  }
  let { onouvrir }: Props = $props();

  let position = $state<{ x: number; y: number } | null>(null);
  let bouton = $state<HTMLButtonElement>();

  onMount(() => void recents.recharger());

  /** Le menu se ferme dès le `pointerdown` ailleurs (donc aussi sur ce bouton) : le clic qui suit
   *  ne doit pas le rouvrir, sinon le bouton ne servirait jamais à le refermer. */
  let etaitOuvert = false;
  const fermer = () => (position = null);

  function ouvrirMenu() {
    if (etaitOuvert) {
      etaitOuvert = false;
      return;
    }
    if (position) return;
    const r = bouton!.getBoundingClientRect();
    position = { x: r.left, y: r.bottom + 2 };
    void recents.recharger(); // l'existence des fichiers est revérifiée à chaque ouverture
  }

  const PREFIXE = 'r:';
  const entrees = $derived.by((): EntreeMenu[] => {
    if (recents.liste.length === 0) return [{ id: 'aucun', libelle: 'Aucun fichier récent', desactive: true }];
    return [
      ...recents.liste.map((r) => ({
        id: PREFIXE + r.chemin,
        libelle: r.existe ? r.nom : `${r.nom} (absent)`,
        desactive: !r.existe,
        aide: r.existe ? r.chemin : `Fichier introuvable : ${r.chemin}`,
      })),
      { id: 'vider', libelle: 'Vider la liste', separe: true },
    ];
  });

  function choisir(id: string) {
    if (id === 'vider') void recents.vider();
    else if (id.startsWith(PREFIXE)) void onouvrir(id.slice(PREFIXE.length));
  }
</script>

<button
  bind:this={bouton}
  onpointerdown={() => (etaitOuvert = !!position)}
  onclick={ouvrirMenu}
  aria-haspopup="menu"
  aria-expanded={!!position}
  title="Derniers fichiers ouverts (15 au plus)"
  data-testid="menu-recents"
>🕘 Récents ▾</button>

{#if position}
  <MenuContextuel x={position.x} y={position.y} {entrees} onchoix={choisir} onfermer={fermer} />
{/if}
