import type { KitModuleManifest } from '@kition/sdk/editor'

/**
 * The engine, from this package's own folder. Its fonts are the page's: Inter first (Droid Sans
 * Fallback has no Latin letters), then Droid Sans Fallback for Chinese and Japanese, so the
 * engine measures text with the faces the browser draws it in (`app.css` names the same files).
 * `scripts/build-engine.sh` copies them from kition-fonts.
 */
export function engineManifest(): KitModuleManifest {
  return {
    kitId: 'kit.board',
    base: new URL('./engine/', document.baseURI).href,
    module: 'kition_board_kit',
    documentClass: 'BoardDocument',
    fonts: [new URL('./fonts/Inter-Variable.ttf', document.baseURI).href, new URL('./fonts/DroidSansFallbackFull.ttf', document.baseURI).href],
  }
}
