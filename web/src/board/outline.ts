/**
 * Where a bound connector end meets its target: the page's mirror of the engine's
 * `outline_point` (`crates/kdm/kdm-model/src/outline.rs`, design note 1.4). A bound end is the
 * point where the ray from the target box's centre through the anchor's point leaves the target's
 * outline. The outlines are the engine's: `shape.previews` answers each shape's `outline`, which
 * the page reads once at open (`board/sync.ts`), so the page has no shape table of its own. Until
 * the answer arrives, and for every shape it gives no outline for, an end is on the box
 * (`anchorPoint`), as the engine puts it on rectangles. `web/tests/flow.mjs` and
 * `web/tests/shapes.mjs` check the page's derived ends against the engine's.
 */
import { anchorPoint, center, rotate, type Point, type Rect } from './geometry'

/**
 * What a bound end meets, as `shape.previews` answers it: the inscribed ellipse, a polygon's corners
 * as `[fx, fy]` fractions of the box, or a parallelogram leaning right whose inset is
 * `min(w * fx, h * fy)`. A shape without one meets its box.
 */
export type Outline = 'ellipse' | ReadonlyArray<readonly [number, number]> | { slant: readonly [number, number] }

const finite = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value)
const pair = (value: unknown): value is [number, number] => Array.isArray(value) && value.length === 2 && finite(value[0]) && finite(value[1])

/** One shape's `outline` from the engine's answer; `null` when it is absent or not one. */
export function parseOutline(value: unknown): Outline | null {
  if (value === 'ellipse') return 'ellipse'
  if (Array.isArray(value)) return value.length >= 3 && value.every(pair) ? (value as Array<[number, number]>) : null
  const slant = (value as { slant?: unknown } | null)?.slant
  return pair(slant) ? { slant } : null
}

/** Each shape's outline from `shape.previews`'s answer (`{ groups: [{ shapes: [{ shape, outline? }] }] }`). */
export function readOutlines(answer: unknown): Map<string, Outline> {
  const out = new Map<string, Outline>()
  const groups = (answer as { groups?: unknown } | null)?.groups
  if (!Array.isArray(groups)) return out
  for (const group of groups) {
    const shapes = (group as { shapes?: unknown } | null)?.shapes
    if (!Array.isArray(shapes)) continue
    for (const item of shapes) {
      const { shape, outline } = (item ?? {}) as { shape?: unknown; outline?: unknown }
      const parsed = parseOutline(outline)
      if (typeof shape === 'string' && parsed) out.set(shape, parsed)
    }
  }
  return out
}

const clamp01 = (v: number) => (Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : 0.5)

/** How far along `(dx, dy)` from the centre of a `w` × `h` box the ray leaves `outline`; `null` when it does not cross it. */
function leave(outline: Outline, w: number, h: number, dx: number, dy: number): number | null {
  if (outline === 'ellipse') {
    const ex = dx / (w / 2)
    const ey = dy / (h / 2)
    return 1 / Math.sqrt(ex * ex + ey * ey)
  }
  let corners: Array<readonly [number, number]>
  if ('slant' in outline) {
    const inset = Math.min(w * outline.slant[0], h * outline.slant[1])
    corners = [
      [inset, 0],
      [w, 0],
      [w - inset, h],
      [0, h],
    ]
  } else {
    corners = outline.map(([fx, fy]) => [w * fx, h * fy] as const)
  }
  const cx = w / 2
  const cy = h / 2
  // The outermost crossing, as the engine takes it (concave outlines such as `cross`).
  let best: number | null = null
  for (let at = 0; at < corners.length; at += 1) {
    const [px, py] = corners[at]
    const [qx, qy] = corners[(at + 1) % corners.length]
    const ex = qx - px
    const ey = qy - py
    const denominator = dx * ey - dy * ex
    if (Math.abs(denominator) <= Number.EPSILON * (Math.abs(dx) + Math.abs(dy)) * (Math.abs(ex) + Math.abs(ey))) continue
    const ox = px - cx
    const oy = py - cy
    const t = (ox * ey - oy * ex) / denominator
    const s = (ox * dy - oy * dx) / denominator
    if (t >= 0 && s >= -1e-9 && s <= 1 + 1e-9 && (best == null || t > best)) best = t
  }
  return best != null && Number.isFinite(best) ? best : null
}

/**
 * The world point of a bound end: where the ray from the centre of the unrotated box `r` through
 * `anchor`'s point leaves `outline`, turned by `rotation` about the centre. No outline (the box),
 * an anchor at the centre, a box with a zero side or anything not finite: `anchorPoint`.
 */
export function outlinePoint(outline: Outline | null | undefined, r: Rect, rotation: number, anchor: readonly [number, number]): Point {
  if (!outline) return anchorPoint(r, rotation, anchor)
  const w = r.x1 - r.x0
  const h = r.y1 - r.y0
  const c = center(r)
  const dx = r.x0 + w * clamp01(anchor[0]) - c.x
  const dy = r.y0 + h * clamp01(anchor[1]) - c.y
  const usable = w > 0 && h > 0 && Number.isFinite(w) && Number.isFinite(h) && (dx !== 0 || dy !== 0)
  const t = usable ? leave(outline, w, h, dx, dy) : null
  if (t == null) return anchorPoint(r, rotation, anchor)
  const local = { x: c.x + dx * t, y: c.y + dy * t }
  return Number.isFinite(local.x) && Number.isFinite(local.y) ? rotate(local, c, rotation) : anchorPoint(r, rotation, anchor)
}

/**
 * The anchor on the box `r` (rotated by `rotation`) where the ray from its centre through the world
 * point `p` leaves it: an end pinned there meets the outline on that ray, under the pointer. `null`
 * when `p` is the centre.
 */
export function rayAnchor(r: Rect, rotation: number, p: Point): [number, number] | null {
  const c = center(r)
  const local = rotate(p, c, -rotation)
  const dx = local.x - c.x
  const dy = local.y - c.y
  const w = r.x1 - r.x0
  const h = r.y1 - r.y0
  if ((dx === 0 && dy === 0) || !(w > 0) || !(h > 0)) return null
  const t = Math.min(dx !== 0 ? w / 2 / Math.abs(dx) : Infinity, dy !== 0 ? h / 2 / Math.abs(dy) : Infinity)
  if (!Number.isFinite(t)) return null
  return [clamp01((c.x + dx * t - r.x0) / w), clamp01((c.y + dy * t - r.y0) / h)]
}
