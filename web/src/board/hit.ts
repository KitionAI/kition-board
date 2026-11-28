/**
 * What a press hits with the select tool (plan "Board usable by hand", rule 2). The browser's own
 * SVG hit testing finds what is painted under the pointer (the engine's fragments are real DOM: a
 * filled shape's inside, a hollow shape's line, any text, a connector's or a stroke's wide invisible
 * hit path); this adds what painting alone misses:
 *
 * - a margin of 4.5 screen pixels around a boxed element's outline (a filled shape just outside its
 *   edge, a hollow shape near its thin line);
 * - inside hollow shapes, where nothing of theirs is painted, the hollow shape with the smallest
 *   area (one larger than the view is skipped, so a big outline does not swallow every press);
 * - a frame: only its line (with the margin) and its title count, not its inside (a drag there is a
 *   marquee; a click there still selects the frame, `gestures/brush.ts`);
 * - locked elements count as the empty canvas (a click on one still selects it, `gestures/brush.ts`;
 *   the canvas menu takes them, `locked: true`).
 *
 * Connectors and strokes are hit by their painted hit paths, which are wider than the margin.
 */
import { center, rotate, type Point, type Rect } from './geometry'
import { isBoxed, type BoardView } from './model'
import type { Outline } from './outline'

/** The hit margin in screen pixels (divided by the zoom). */
export const HIT_MARGIN = 4.5

/** How many points stand for an ellipse. */
const ELLIPSE_STEPS = 48

/** The outline a shape is hit by, as a polygon on its unrotated box: the engine's outline (an ellipse sampled), else the box. */
export function outlinePolygon(outline: Outline | null | undefined, r: Rect): Point[] {
  const w = r.x1 - r.x0
  const h = r.y1 - r.y0
  if (outline === 'ellipse') {
    const c = center(r)
    return Array.from({ length: ELLIPSE_STEPS }, (_, at) => {
      const angle = (at / ELLIPSE_STEPS) * Math.PI * 2
      return { x: c.x + (Math.cos(angle) * w) / 2, y: c.y + (Math.sin(angle) * h) / 2 }
    })
  }
  if (outline && 'slant' in outline) {
    const inset = Math.min(w * outline.slant[0], h * outline.slant[1])
    return [
      { x: r.x0 + inset, y: r.y0 },
      { x: r.x1, y: r.y0 },
      { x: r.x1 - inset, y: r.y1 },
      { x: r.x0, y: r.y1 },
    ]
  }
  if (outline) return outline.map(([fx, fy]) => ({ x: r.x0 + w * fx, y: r.y0 + h * fy }))
  return [
    { x: r.x0, y: r.y0 },
    { x: r.x1, y: r.y0 },
    { x: r.x1, y: r.y1 },
    { x: r.x0, y: r.y1 },
  ]
}

function segmentDistance(p: Point, a: Point, b: Point): number {
  const dx = b.x - a.x
  const dy = b.y - a.y
  const length = dx * dx + dy * dy
  const t = length > 0 ? Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / length)) : 0
  return Math.hypot(p.x - (a.x + dx * t), p.y - (a.y + dy * t))
}

/** How far `p` is from the polygon's edges. */
export function polygonDistance(polygon: readonly Point[], p: Point): number {
  let best = Infinity
  for (let at = 0; at < polygon.length; at += 1) best = Math.min(best, segmentDistance(p, polygon[at], polygon[(at + 1) % polygon.length]))
  return best
}

/** Whether `p` is inside the polygon (even-odd). */
export function insidePolygon(polygon: readonly Point[], p: Point): boolean {
  let inside = false
  for (let at = 0, before = polygon.length - 1; at < polygon.length; before = at, at += 1) {
    const a = polygon[at]
    const b = polygon[before]
    if (a.y > p.y !== b.y > p.y && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) inside = !inside
  }
  return inside
}

/** How far `p` is from a boxed element's outline, and whether it is inside it (null: not a boxed element with a box). */
export function outlineHit(view: BoardView, id: string, p: Point): { distance: number; inside: boolean; area: number } | null {
  const element = view.elements.get(id)
  const box = element && isBoxed(element.kind) ? view.rect(element) : null
  if (!element || !box) return null
  const local = rotate(p, center(box), -element.rotation)
  const polygon = outlinePolygon(view.outlineOf(element), box)
  return { distance: polygonDistance(polygon, local), inside: insidePolygon(polygon, local), area: (box.x1 - box.x0) * (box.y1 - box.y0) }
}

/** What `pick` reads: the board, what the browser found painted under the pointer, and the elements near it. */
export type HitSource = {
  view: BoardView
  /** The element ids painted under the pointer, topmost first. */
  painted: readonly string[]
  /** The ids whose boxes meet the margin around the pointer, back to front (asked only when nothing painted counts). */
  near: () => readonly string[]
  /** Whether an element is drawn now (not culled, hidden by a gesture or collapsed away). */
  shown: (id: string) => boolean
  /** The margin in board units. */
  margin: number
  /** The view's area in board units: a hollow shape larger than it is not hit from inside. */
  viewArea: number
  /** A frame's title as drawn, in the frame's own unrotated frame (board units from its top-left corner), or null when it has none. */
  frameTitle: (id: string) => Rect | null
  /** Whether locked elements count (the canvas menu); otherwise they are the empty canvas. */
  locked: boolean
}

/** Whether `p` is on a frame's line (within the margin) or its title. */
function onFrame(source: HitSource, id: string, p: Point): number | null {
  const { view } = source
  const element = view.elements.get(id)
  const box = element ? view.rect(element) : null
  if (!element || !box) return null
  const local = rotate(p, center(box), -element.rotation)
  const title = source.frameTitle(id)
  const x = local.x - box.x0
  const y = local.y - box.y0
  if (title && x >= title.x0 - source.margin && x <= title.x1 + source.margin && y >= title.y0 - source.margin && y <= title.y1 + source.margin) return 0
  const d = polygonDistance(outlinePolygon(null, box), local)
  return d <= source.margin ? d : null
}

/** The element a press at board point `p` hits (rule 2), or null for the empty canvas. */
export function pick(source: HitSource, p: Point): string | null {
  const { view } = source
  const counts = (id: string) => {
    const element = view.elements.get(id)
    return element != null && (source.locked || !element.locked) && !view.hidden.has(id)
  }
  // What is painted under the pointer, topmost first: a frame only by its line or title.
  for (const id of source.painted) {
    if (!counts(id)) continue
    if (view.elements.get(id)?.kind === 'frame') {
      if (onFrame(source, id, p) != null) return id
      continue
    }
    return id
  }
  // Nothing painted: the nearest outline within the margin, else the smallest hollow shape around the pointer.
  let edge: { id: string; d: number } | null = null
  let hollow: { id: string; area: number } | null = null
  const near = source.near()
  for (let at = near.length - 1; at >= 0; at -= 1) {
    const id = near[at]
    if (!counts(id) || !source.shown(id)) continue
    const element = view.elements.get(id)
    if (!element || !isBoxed(element.kind)) continue
    if (element.kind === 'frame') {
      const d = onFrame(source, id, p)
      if (d != null && (!edge || d < edge.d)) edge = { id, d }
      continue
    }
    const hit = outlineHit(view, id, p)
    if (!hit) continue
    if (hit.distance <= source.margin) {
      if (!edge || hit.distance < edge.d) edge = { id, d: hit.distance }
    } else if (hit.inside && element.kind === 'shape' && !source.painted.includes(id) && hit.area <= source.viewArea && (!hollow || hit.area < hollow.area)) {
      hollow = { id, area: hit.area }
    }
  }
  return edge?.id ?? hollow?.id ?? null
}
