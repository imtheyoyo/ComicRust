// Fichiers récents (menu 🕘) : état partagé entre le lecteur (qui alimente la liste à chaque
// ouverture) et le menu de la bibliothèque. La liste, son plafond (15) et sa vérification
// (« fichier absent ») sont dans le backend (docs/tauri/ipc-lecteur.md) ; ici, seulement l'affichage.
import { ajouterRecent, lireRecents, retirerRecent, viderRecents, type RecentFichier } from './api';

class Recents {
  liste = $state<RecentFichier[]>([]);

  /** Relit la liste (et l'existence de chaque fichier) : à l'ouverture du menu. */
  async recharger(): Promise<void> {
    try {
      this.liste = await lireRecents();
    } catch (e) {
      console.warn('fichiers récents illisibles', e);
    }
  }

  /** Un fichier vient d'être ouvert : il passe en tête. Un échec n'empêche jamais la lecture. */
  async ajouter(chemin: string): Promise<void> {
    if (!chemin.trim()) return;
    try {
      this.liste = await ajouterRecent(chemin);
    } catch (e) {
      console.warn('fichier récent non mémorisé', e);
    }
  }

  /** Retire une entrée (fichier qui ne s'ouvre pas, ou choix de l'utilisateur). */
  async retirer(chemin: string): Promise<void> {
    try {
      this.liste = await retirerRecent(chemin);
    } catch (e) {
      console.warn('fichier récent non retiré', e);
    }
  }

  async vider(): Promise<void> {
    try {
      this.liste = await viderRecents();
    } catch (e) {
      console.warn('fichiers récents non vidés', e);
    }
  }
}

export const recents = new Recents();
