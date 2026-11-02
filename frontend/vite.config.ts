import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Tauri sert `frontend/dist` (build) ou http://localhost:5173 (dev, voir tauri.conf.json).
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  // fs.allow : le mode simulé lit les thèmes livrés (crates/comicrust-tauri/themes) hors de frontend/.
  server: { port: 5173, strictPort: true, fs: { allow: ['..'] } },
  // WebView2 (Chromium récent) : pas besoin de cibler d'anciens navigateurs.
  build: { target: 'es2022', outDir: 'dist', emptyOutDir: true },
});
