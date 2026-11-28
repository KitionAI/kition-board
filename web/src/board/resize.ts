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

/** A box resized by a handle, and whether it was dragged over its opposite side (so it flipped) across and down. */
export type Resized = { box: Rect; flipX: boolean; flipY: boolean }

/**
 * The box a handle dragged to `p` makes, the opposite side fixed (rule 8): dragged past the opposite
 * side the box goes on over it, flipped. Shift keeps the aspect (a side handle's other side follows,
 * growing from the top or left edge).
 */
export function resizedBox(box: Rect, handle: Handle, p: Point, keepAspect: boolean): Resized {
  const sx = handle.includes('east') ? 1 : handle.includes('west') ? -1 : 0
  const sy = handle.includes('south') ? 1 : handle.includes('north') ? -1 : 0
  const w0 = box.x1 - box.x0
  const h0 = box.y1 - box.y0
  const fixedX = sx > 0 ? box.x0 : box.x1
  const fixedY = sy > 0 ? box.y0 : box.y1
  const atLeastOne = (v: number) => (Math.abs(v) >= 1 ? v : v < 0 ? -1 : 1)
  // Signed sizes along the handle's direction: negative once it is past the opposite side.
  let w = sx ? atLeastOne((p.x - fixedX) * sx) : w0
  let h = sy ? atLeastOne((p.y - fixedY) * sy) : h0
  const ratio = w0 / Math.max(1e-6, h0)
  if (keepAspect && sx && sy) {
    if (Math.abs(w) / Math.abs(h) > ratio) h = Math.sign(h) * (Math.abs(w) / ratio)
    else w = Math.sign(w) * Math.abs(h) * ratio
  } else if (keepAspect && sx) h = Math.abs(w) / ratio
  else if (keepAspect && sy) w = Math.abs(h) * ratio
  const xs = sx ? [fixedX, fixedX + sx * w] : [box.x0, box.x0 + w]
  const ys = sy ? [fixedY, fixedY + sy * h] : [box.y0, box.y0 + h]
  return { box: { x0: Math.min(...xs), y0: Math.min(...ys), x1: Math.max(...xs), y1: Math.max(...ys) }, flipX: sx !== 0 && w < 0, flipY: sy !== 0 && h < 0 }
}

/** The box a handle dragged to `p` makes (`resizedBox` without the flips). */
export function resized(box: Rect, handle: Handle, p: Point, keepAspect: boolean): Rect {
  return resizedBox(box, handle, p, keepAspect).box
}

/**
 * The unrotated box of an element rotated by `rotation` about its centre whose `handle` is
 * dragged to world point `p`: the box resizes in the element's own frame and moves so the
 * opposite handle stays where it is on the board (also once it has flipped over it).
 */
export function resizedRotated(box: Rect, rotation: number, handle: Handle, p: Point, keepAspect: boolean): Resized {
  if (!rotation) return resizedBox(box, handle, p, keepAspect)
  const c = center(box)
  const next = resizedBox(box, handle, rotate(p, c, -rotation), keepAspect)
  const [fx, fy] = HANDLE_AT[handle]
  const fixedLocal = at(box, [1 - fx, 1 - fy])
  const fixed = rotate(fixedLocal, c, rotation)
  const moved = rotate(fixedLocal, center(next.box), rotation)
  const dx = fixed.x - moved.x
  const dy = fixed.y - moved.y
  return { ...next, box: { x0: next.box.x0 + dx, y0: next.box.y0 + dy, x1: next.box.x1 + dx, y1: next.box.y1 + dy } }
}

/** The handle a dragged handle has become once the box flipped over its opposite side (for its cursor). */
export function flippedHandle(handle: Handle, flipX: boolean, flipY: boolean): Handle {
  let out: string = handle
  if (flipX) out = out.includes('east') ? out.replace('east', 'west') : out.replace('west', 'east')
  if (flipY) out = out.includes('north') ? out.replace('north', 'south') : out.replace('south', 'north')
  return out as Handle
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
