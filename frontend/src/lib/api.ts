// Couche d'appel des commandes Tauri. Le front n'a aucune logique métier : il affiche ce que
// renvoient les commandes (crates/comicrust-tauri/src/app.rs).
import { invoke } from '@tauri-apps/api/core';

export interface InfosApplication {
  version: string;
  dossier_data: string;
  dossier_themes: string;
  dossier_webview: string;
}

export interface Theme {
  id: string;
  nom: string;
  livre: boolean;
}

/** `true` dans la fenêtre Tauri ; `false` dans un navigateur (npm run dev sans Tauri). */
export const dansTauri = (): boolean => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export const infos = () => invoke<InfosApplication>('infos');
export const listerThemes = () => invoke<Theme[]>('lister_themes');
export const chargerTheme = (id: string) => invoke<string>('charger_theme', { id });
