import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [
    tailwindcss(),
    sveltekit({
      // SvelteKit 3.x: config is passed inline.
      adapter: (await import('@sveltejs/adapter-node')).default(),
      extensions: ['.svelte'],
    }),
  ],
  server: {
    port: 5173,
    proxy: {
      // Forward API calls to the local patientspace-core Rust binary.
      '/fhir': 'http://127.0.0.1:8080',
      '/health': 'http://127.0.0.1:8080',
      '/psep': 'http://127.0.0.1:8080',
    },
  },
});
