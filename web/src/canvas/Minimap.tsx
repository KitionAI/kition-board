/**
 * The minimap: the engine's raster of the page (`render`, the same scene the canvas shows), drawn
 * once per change into a small canvas and kept, with the view's rectangle over it. A per-element
 * SVG minimap costs 20–28 ms per commit at 5,000 elements (spike S2, 3.2), so it is a cached raster.
 * A click centres the view there.
 *
 * Replaces Kition's `packages/board-editor/src/components/WhiteboardMinimap.tsx`.
 */
import { useEffect, useRef, useSyncExternalStore } from 'react'
import type { Viewport } from '../board/layer'

/** The view, followed without re-rendering the editor on every pan step. */
export type ViewportStore = { get: () => Viewport | null; subscribe: (listener: () => void) => () => void }

export type DocumentArea = { x: number; y: number; unit: number; width: number; height: number }

const WIDTH = 200
const HEIGHT = 128

export function Minimap({ area, page, revision, render, viewport: store, onCenter, label }: { area: DocumentArea | null; page: string | null; revision: number; render: (x: number, y: number, scale: number, width: number, height: number) => Promise<Uint8Array>; viewport: ViewportStore; onCenter: (x: number, y: number) => void; label: string }) {
  const canvas = useRef<HTMLCanvasElement>(null)
  // A new `render` is a new color scheme: drawn at once, with the canvas, not after a burst settles.
  const drawnWith = useRef(render)
  // Another page is drawn at once too: the minimap never shows the page left.
  const drawnPage = useRef(page)
  const viewport = useSyncExternalStore(store.subscribe, store.get)
  const scale = area ? Math.min(WIDTH / area.width, HEIGHT / area.height) : 1
  const width = area ? Math.max(1, Math.round(area.width * scale)) : WIDTH
  const height = area ? Math.max(1, Math.round(area.height * scale)) : HEIGHT

  useEffect(() => {
    if (!area) return undefined
    let current = true
    // Changes come in bursts (a drag's commit, an agent's steps): draw once they settle.
    const schemeChanged = drawnWith.current !== render
    drawnWith.current = render
    const pageChanged = drawnPage.current !== page
    drawnPage.current = page
    const timer = window.setTimeout(() => {
      void render(0, 0, scale, width, height)
        .then((rgba) => {
          const target = canvas.current
          if (!current || !target || rgba.length !== width * height * 4) return
          target.width = width
          target.height = height
          target.getContext('2d')?.putImageData(new ImageData(new Uint8ClampedArray(rgba), width, height), 0, 0)
        })
        .catch(() => undefined)
    }, schemeChanged || pageChanged ? 0 : 400)
    return () => {
      current = false
      window.clearTimeout(timer)
    }
  }, [area, height, page, render, revision, scale, width])

  if (!area) return null
  const toMap = (wx: number, wy: number) => ({ x: ((wx - area.x) / area.unit) * scale, y: ((wy - area.y) / area.unit) * scale })
  const view = viewport ? toMap(viewport.x, viewport.y) : null
  const viewWidth = viewport ? (viewport.width / viewport.zoom / area.unit) * scale : 0
  const viewHeight = viewport ? (viewport.height / viewport.zoom / area.unit) * scale : 0
  return (
    <div
      className="board-minimap"
      style={{ width, height }}
      aria-label={label}
      data-testid="board-minimap"
      onPointerDown={(event) => {
        event.stopPropagation()
        const box = event.currentTarget.getBoundingClientRect()
        onCenter(area.x + ((event.clientX - box.left) / scale) * area.unit, area.y + ((event.clientY - box.top) / scale) * area.unit)
      }}
    >
      <canvas ref={canvas} width={width} height={height} />
      {view ? <div className="board-minimap-view" style={{ left: view.x, top: view.y, width: viewWidth, height: viewHeight }} /> : null}
    </div>
  )
}
