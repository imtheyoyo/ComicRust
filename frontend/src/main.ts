import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { installerDialogues } from './lib/dialogue';

// Dialogues natifs (Ctrl+O, « Retrouver le fichier »…) : branchés seulement si un dialogue existe.
installerDialogues();

export default mount(App, { target: document.getElementById('app')! });
