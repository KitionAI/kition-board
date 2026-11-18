/**
 * Board geometry in board units (1 unit = 1 CSS pixel at 100%): boxes, rotation about a box's
 * centre, and the derived point of a bound connector end. These are the Rust rules of
 * `crates/kdm/kdm-model/src/geometry.rs` (design note 1.4) in TypeScript, for the page view;
 * `web/tests/flow.mjs` checks the page's derived ends against the engine's.
 *
 * Ported from Kition's `packages/board-editor/src/lib/whiteboardGeometry.ts` and
 * `boardBindingEngine.ts`.
 */

export type Point = { x: number; y: number }
/** An axis-aligned box `x0 ≤ x1`, `y0 ≤ y1`. */
export type Rect = { x0: number; y0: number; x1: number; y1: number }

export function rect(ax: number, ay: number, bx: number, by: number): Rect {
  return { x0: Math.min(ax, bx), y0: Math.min(ay, by), x1: Math.max(ax, bx), y1: Math.max(ay, by) }
}

export function rectOfPoints(points: readonly Point[]): Rect | null {
  if (!points.length) return null
  let out = rect(points[0].x, points[0].y, points[0].x, points[0].y)
  for (const point of points) out = { x0: Math.min(out.x0, point.x), y0: Math.min(out.y0, point.y), x1: Math.max(out.x1, point.x), y1: Math.max(out.y1, point.y) }
  return out
}

export function union(a: Rect | null, b: Rect | null): Rect | null {
  if (!a) return b
  if (!b) return a
  return { x0: Math.min(a.x0, b.x0), y0: Math.min(a.y0, b.y0), x1: Math.max(a.x1, b.x1), y1: Math.max(a.y1, b.y1) }
}

export function intersects(a: Rect, b: Rect) {
  return a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1
}

export function contains(r: Rect, p: Point) {
  return p.x >= r.x0 && p.x <= r.x1 && p.y >= r.y0 && p.y <= r.y1
}

export function grow(r: Rect, by: number): Rect {
  return { x0: r.x0 - by, y0: r.y0 - by, x1: r.x1 + by, y1: r.y1 + by }
}

export function center(r: Rect): Point {
  return { x: (r.x0 + r.x1) / 2, y: (r.y0 + r.y1) / 2 }
}

export function offset(r: Rect, dx: number, dy: number): Rect {
  return { x0: r.x0 + dx, y0: r.y0 + dy, x1: r.x1 + dx, y1: r.y1 + dy }
}

/** `p` rotated by `degrees` (clockwise on screen, y down) about `origin`. */
export function rotate(p: Point, origin: Point, degrees: number): Point {
  if (!degrees || !Number.isFinite(degrees)) return p
  const radians = (degrees * Math.PI) / 180
  const sine = Math.sin(radians)
  const cosine = Math.cos(radians)
  const x = p.x - origin.x
  const y = p.y - origin.y
  return { x: origin.x + x * cosine - y * sine, y: origin.y + x * sine + y * cosine }
}

/** The axis-aligned box around `r` rotated by `degrees` about its centre. */
export function rotatedBounds(r: Rect, degrees: number): Rect {
  if (!degrees || !Number.isFinite(degrees)) return r
  const c = center(r)
  return rectOfPoints([
    rotate({ x: r.x0, y: r.y0 }, c, degrees),
    rotate({ x: r.x1, y: r.y0 }, c, degrees),
    rotate({ x: r.x1, y: r.y1 }, c, degrees),
    rotate({ x: r.x0, y: r.y1 }, c, degrees),
  ])!
}

const clamp01 = (v: number) => (Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : 0.5)

/** The world point of `anchor` (normalised to the unrotated box) on a box rotated by `rotation`. */
export function anchorPoint(r: Rect, rotation: number, anchor: readonly [number, number]): Point {
  const local = { x: r.x0 + (r.x1 - r.x0) * clamp01(anchor[0]), y: r.y0 + (r.y1 - r.y0) * clamp01(anchor[1]) }
  return rotate(local, center(r), rotation)
}

/**
 * Where a connector toward `p` binds on the box: outside, where the line from the centre to `p`
 * leaves the box; inside, the nearest edge. Answers the anchor and its world point.
 */
export function nearestEdgeAnchor(r: Rect, rotation: number, p: Point): { anchor: [number, number]; point: Point } {
  const c = center(r)
  const local = rotate(p, c, -rotation)
  const dx = local.x - c.x
  const dy = local.y - c.y
  const halfW = (r.x1 - r.x0) / 2
  const halfH = (r.y1 - r.y0) / 2
  const w = r.x1 - r.x0
  const h = r.y1 - r.y0
  let edge: Point
  if (!contains(r, local) && (dx !== 0 || dy !== 0)) {
    const tx = dx !== 0 ? halfW / Math.abs(dx) : Infinity
    const ty = dy !== 0 ? halfH / Math.abs(dy) : Infinity
    const t = Math.min(tx, ty)
    edge = { x: c.x + dx * t, y: c.y + dy * t }
  } else {
    const cx = Math.min(r.x1, Math.max(r.x0, local.x))
    const cy = Math.min(r.y1, Math.max(r.y0, local.y))
    const candidates: Array<[number, Point]> = [
      [Math.abs(local.x - r.x1), { x: r.x1, y: cy }],
      [Math.abs(local.x - r.x0), { x: r.x0, y: cy }],
      [Math.abs(local.y - r.y1), { x: cx, y: r.y1 }],
      [Math.abs(local.y - r.y0), { x: cx, y: r.y0 }],
    ]
    edge = candidates.reduce((best, next) => (next[0] < best[0] ? next : best))[1]
  }
  const anchor: [number, number] = [clamp01(w > 0 ? (edge.x - r.x0) / w : 0.5), clamp01(h > 0 ? (edge.y - r.y0) / h : 0.5)]
  return { anchor, point: rotate(edge, c, rotation) }
}

export function distance(a: Point, b: Point) {
  return Math.hypot(b.x - a.x, b.y - a.y)
}
