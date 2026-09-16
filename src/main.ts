import { mount } from 'svelte';
import './lib/styles/app.css';
import App from './App.svelte';
import { prefersDark } from './lib/store.svelte';

// Settled before the first paint; the stored preference takes over a moment
// later, once it has been read.
document.documentElement.dataset.theme = prefersDark() ? 'dark' : 'light';

const target = document.getElementById('app');
if (!target) throw new Error('missing #app root');

export default mount(App, { target });
