import { fileURLToPath } from 'node:url'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// The kit package is served from wherever a host puts it, so every asset path is relative.
export default defineConfig({
  base: './',
  plugins: [react()],
  resolve: {
    // @kition/sdk may be linked from a kition-sdk checkout (KITION_LINK=1, .pnpmfile.cjs); one React for the page.
    dedupe: ['react', 'react-dom'],
    // The editor's page replica (and the kit editor's engine worker) read with @kition/sdk/kdm. loro-crdt's
    // browser entry loads its WebAssembly itself; the development entry Vite's server would pick
    // imports it as a module, which needs a plugin.
    alias: [{ find: /^loro-crdt$/, replacement: 'loro-crdt/browser' }],
  },
  // loro-crdt stays unbundled, so its WebAssembly is found next to it.
  // The kit editor's own CommonJS dependency (React's useSyncExternalStore shim) is pre-bundled, or the
  // development server hands the page a module without its named export.
  optimizeDeps: {
    exclude: ['@kition/sdk', 'loro-crdt'],
    include: ['@kition/sdk > use-sync-external-store/shim'],
  },
  worker: { format: 'es' },
  server: {
    port: 5191,
    fs: { allow: [fileURLToPath(new URL('..', import.meta.url)), fileURLToPath(new URL('../../kition-sdk', import.meta.url)), fileURLToPath(new URL('../../kition-tessera', import.meta.url))] },
  },
  build: {
    outDir: 'dist',
    rollupOptions: { input: { index: fileURLToPath(new URL('index.html', import.meta.url)), dev: fileURLToPath(new URL('dev.html', import.meta.url)) } },
  },
})
