import { createRoot } from 'react-dom/client'
import { connectHost, createKitStrings, defineKitUi } from '@kition/kit-editor'
import '@kition/kit-editor/styles.css'
import './app.css'
import { App } from './App'
import { CATALOGS } from './strings'
import { TOOLS } from './tools'
import manifest from '../../kit.json'

/** Every element this editor draws besides engine commands and menus (Kit UI protocol section 9). */
const KIT_UI = defineKitUi({
  tools: [
    ...TOOLS.map((tool) => ({ id: `tool:${tool.id}`, label: CATALOGS['en-US'][tool.label] ?? tool.label })),
  ],
  panels: [{ id: 'panel:style', label: 'Style' }],
  tabs: [],
  groups: [],
  controls: [
    { id: 'control:templates', label: 'Templates' },
    { id: 'control:export', label: 'Export' },
  ],
})

/**
 * The faces the engine measures with (`engine.ts`), for the browser: the fragments name these
 * families, so the browser draws the advances the engine laid out. Loaded from the package's own
 * folder, wherever a host serves it.
 */
function loadFonts() {
  const faces = [
    new FontFace('Inter', `url(${new URL('./fonts/Inter-Variable.ttf', document.baseURI).href})`, { weight: '100 900' }),
    new FontFace('Droid Sans Fallback', `url(${new URL('./fonts/DroidSansFallbackFull.ttf', document.baseURI).href})`),
  ]
  for (const face of faces) {
    document.fonts.add(face)
    void face.load().catch(() => undefined)
  }
}

loadFonts()
const root = createRoot(document.getElementById('root')!)

// Opened outside a host, the page says where it belongs instead of waiting forever.
const alone = window.setTimeout(() => {
  const t = createKitStrings(CATALOGS, navigator.language)
  root.render(<div className="kui-alone">{t('noHost')}</div>)
}, 3000)

// The editor opens in Menus, a whiteboard's display mode (`ui.display`).
void connectHost({ id: 'kit.board', version: '0.1.0', capabilities: ['ai', 'replica'], ui: KIT_UI, display: manifest.ui.display }).then((host) => {
  window.clearTimeout(alone)
  // No StrictMode: its doubled effects would open the document twice, in two engine workers.
  root.render(<App host={host} />)
})
