/**
 * Resizing by the selection's handles: one element's box (rotated or not) or a selection's
 * bounds, and the preview of the elements while the handle is dragged. The engine makes the
 * change when the handle is let go (`element.resize { id, box }` or `{ ids, box }`); the preview
 * only transforms the cached fragments, the way the engine will map them.
 *
 * Ported from Kition's `packages/board-editor/src/lib/whiteboardGeometry.ts` (`getResizedBounds`,
 * `resizeWhiteboardElements`, `resizeWhiteboardElement`); the rotated box keeping its opposite
 * handle in place is new.
 */
import { center, rotate, type Point, type Rect } from './geometry'
import type { Override } from './layer'
import { isBoxed, type BoardView } from './model'
import type { Handle } from './snap'

/** Where each handle sits on the box (0 to 1 across, 0 to 1 down). */
export const HANDLE_AT: Record<Handle, [number, number]> = {
  'north-west': [0, 0],
  north: [0.5, 0],
  'north-east': [1, 0],
  east: [1, 0.5],
  'south-east': [1, 1],
  south: [0.5, 1],
  'south-west': [0, 1],
  west: [0, 0.5],
}

const at = (box: Rect, [fx, fy]: readonly [number, number]): Point => ({ x: box.x0 + (box.x1 - box.x0) * fx, y: box.y0 + (box.y1 - box.y0) * fy })

/** The box a handle dragged to `p` makes, the opposite side fixed (Shift keeps the aspect). */
export function resized(box: Rect, handle: Handle, p: Point, keepAspect: boolean): Rect {
  let { x0, y0, x1, y1 } = box
  if (handle.includes('west')) x0 = Math.min(p.x, x1 - 1)
  if (handle.includes('east')) x1 = Math.max(p.x, x0 + 1)
  if (handle.includes('north')) y0 = Math.min(p.y, y1 - 1)
  if (handle.includes('south')) y1 = Math.max(p.y, y0 + 1)
  if (keepAspect) {
    const ratio = (box.x1 - box.x0) / Math.max(1e-6, box.y1 - box.y0)
    const w = x1 - x0
    const h = y1 - y0
    if (w / Math.max(1e-6, h) > ratio) {
      const nh = w / ratio
      if (handle.includes('north')) y0 = y1 - nh
      else y1 = y0 + nh
    } else {
      const nw = h * ratio
      if (handle.includes('west')) x0 = x1 - nw
      else x1 = x0 + nw
    }
  }
  return { x0, y0, x1, y1 }
}

/**
 * The unrotated box of an element rotated by `rotation` about its centre whose `handle` is
 * dragged to world point `p`: the box resizes in the element's own frame and moves so the
 * opposite handle stays where it is on the board.
 */
export function resizedRotated(box: Rect, rotation: number, handle: Handle, p: Point, keepAspect: boolean): Rect {
  if (!rotation) return resized(box, handle, p, keepAspect)
  const c = center(box)
  const next = resized(box, handle, rotate(p, c, -rotation), keepAspect)
  const [fx, fy] = HANDLE_AT[handle]
  const opposite: [number, number] = [1 - fx, 1 - fy]
  const fixed = rotate(at(box, opposite), c, rotation)
  const moved = rotate(at(next, opposite), center(next), rotation)
  const dx = fixed.x - moved.x
  const dy = fixed.y - moved.y
  return { x0: next.x0 + dx, y0: next.y0 + dy, x1: next.x1 + dx, y1: next.y1 + dy }
}

/** A handle's position on the board for an element rotated by `rotation`. */
export function handlePoint(box: Rect, rotation: number, handle: Handle): Point {
  return rotate(at(box, HANDLE_AT[handle]), center(box), rotation)
}

const CURSORS = ['ns-resize', 'nesw-resize', 'ew-resize', 'nwse-resize']

/** The resize cursor along a handle's direction once the box is rotated. */
export function handleCursor(handle: Handle, rotation: number): string {
  const [fx, fy] = HANDLE_AT[handle]
  const degrees = (Math.atan2(fy - 0.5, fx - 0.5) * 180) / Math.PI + 90 + rotation
  const step = ((Math.round(degrees / 45) % 4) + 4) % 4
  return CURSORS[step]
}

/**
 * How the elements of `moving` look while a selection's bounds `from` become `to`, as the engine's
 * `element.resize { ids, box }` will map them: a box keeps its centre's place and scales (its
 * rotation stays), a text's letters scale with the area, strokes and connectors are mapped
 * point by point.
 */
export function scaleOverrides(view: BoardView, moving: readonly string[], from: Rect, to: Rect): Map<string, Override> {
  const sx = (to.x1 - to.x0) / Math.max(1e-6, from.x1 - from.x0)
  const sy = (to.y1 - to.y0) / Math.max(1e-6, from.y1 - from.y0)
  const map = (p: Point): Point => ({ x: to.x0 + (p.x - from.x0) * sx, y: to.y0 + (p.y - from.y0) * sy })
  const out = new Map<string, Override>()
  for (const id of moving) {
    const element = view.elements.get(id)
    if (!element || element.kind === 'group') continue
    if (!isBoxed(element.kind)) {
      // World fragments: x' = x·sx + (to.x0 − from.x0·sx).
      out.set(id, { dx: to.x0 - from.x0 * sx, dy: to.y0 - from.y0 * sy, sx, sy })
      continue
    }
    const box = view.rect(element)
    if (!box) continue
    if (element.kind === 'text') {
      const corner = map({ x: box.x0, y: box.y0 })
      const letters = Math.sqrt(Math.max(0.04, sx * sy))
      out.set(id, { dx: corner.x - box.x0, dy: corner.y - box.y0, sx: letters, sy: letters })
      continue
    }
    const c = map(center(box))
    const w = Math.max(1, (box.x1 - box.x0) * sx)
    const h = Math.max(1, (box.y1 - box.y0) * sy)
    out.set(id, { dx: c.x - w / 2 - box.x0, dy: c.y - h / 2 - box.y0, sx: w / Math.max(1e-6, box.x1 - box.x0), sy: h / Math.max(1e-6, box.y1 - box.y0) })
  }
  return out
}
