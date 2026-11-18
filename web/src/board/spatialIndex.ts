/**
 * A grid of 512-unit cells over the board, for culling and hit candidates: which elements' boxes
 * meet an area, back to front.
 *
 * Ported from Kition's `packages/board-editor/src/lib/boardSpatialIndex.ts`. Entries are ids with a
 * box (the page view's bounds), not v1 elements.
 */
import { intersects, type Point, type Rect } from './geometry'

const CELL = 512
const MAX_CELLS_PER_ENTRY = 256
const MAX_QUERY_CELLS = 4096

type Entry = { box: Rect; cells: string[]; large: boolean; order: number }
type Span = { startX: number; startY: number; endX: number; endY: number; count: number }

export class SpatialIndex {
  private readonly cells = new Map<string, Set<string>>()
  private readonly entries = new Map<string, Entry>()
  private readonly large = new Set<string>()

  /** Sets the entries to `items` (in back-to-front order). */
  sync(items: ReadonlyArray<readonly [string, Rect]>) {
    const present = new Set(items.map(([id]) => id))
    for (const id of [...this.entries.keys()]) if (!present.has(id)) this.remove(id)
    items.forEach(([id, box], order) => this.upsert(id, box, order))
  }

  upsert(id: string, box: Rect, order = this.entries.size) {
    const current = this.entries.get(id)
    if (current && same(current.box, box)) {
      current.order = order
      return
    }
    if (current) this.remove(id)
    const span = spanOf(box)
    const large = span.count > MAX_CELLS_PER_ENTRY
    const cells = large ? [] : keysOf(span)
    this.entries.set(id, { box, cells, large, order })
    if (large) {
      this.large.add(id)
      return
    }
    for (const key of cells) {
      const ids = this.cells.get(key)
      if (ids) ids.add(id)
      else this.cells.set(key, new Set([id]))
    }
  }

  remove(id: string) {
    const entry = this.entries.get(id)
    if (!entry) return
    this.entries.delete(id)
    this.large.delete(id)
    for (const key of entry.cells) {
      const ids = this.cells.get(key)
      if (!ids) continue
      ids.delete(id)
      if (!ids.size) this.cells.delete(key)
    }
  }

  /** An entry's back-to-front position. */
  order(id: string): number | null {
    return this.entries.get(id)?.order ?? null
  }

  box(id: string): Rect | null {
    return this.entries.get(id)?.box ?? null
  }

  /** The ids whose boxes meet `area`, back to front. */
  query(area: Rect): string[] {
    const span = spanOf(area)
    const candidates = new Set<string>(this.large)
    if (span.count > MAX_QUERY_CELLS) for (const id of this.entries.keys()) candidates.add(id)
    else for (const key of keysOf(span)) for (const id of this.cells.get(key) ?? []) candidates.add(id)
    const hits: Array<[string, number]> = []
    for (const id of candidates) {
      const entry = this.entries.get(id)
      if (entry && intersects(area, entry.box)) hits.push([id, entry.order])
    }
    return hits.sort((a, b) => a[1] - b[1]).map(([id]) => id)
  }

  queryPoint(p: Point, tolerance = 0): string[] {
    return this.query({ x0: p.x - tolerance, y0: p.y - tolerance, x1: p.x + tolerance + 1e-9, y1: p.y + tolerance + 1e-9 })
  }

  get size() {
    return this.entries.size
  }
}

function spanOf(box: Rect): Span {
  const startX = Math.floor(box.x0 / CELL)
  const startY = Math.floor(box.y0 / CELL)
  const endX = Math.floor(Math.max(box.x0, box.x1) / CELL)
  const endY = Math.floor(Math.max(box.y0, box.y1) / CELL)
  return { startX, startY, endX, endY, count: (endX - startX + 1) * (endY - startY + 1) }
}

function keysOf(span: Span): string[] {
  const keys: string[] = []
  for (let x = span.startX; x <= span.endX; x += 1) for (let y = span.startY; y <= span.endY; y += 1) keys.push(`${x}:${y}`)
  return keys
}

function same(a: Rect, b: Rect) {
  return a.x0 === b.x0 && a.y0 === b.y0 && a.x1 === b.x1 && a.y1 === b.y1
}
