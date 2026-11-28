/**
 * Select and move. A press on an element selects it at once (Shift, Cmd or Ctrl adds it, or takes a
 * selected one out), so a drag can start straight away; past the threshold the selection's unlocked
 * elements move, snapping their box to the edges and centres of the elements near them, and the
 * frames they would drop into light up. The release runs one `element.move { reparent }` (a drop
 * into a frame makes it the parent, a drop out of one frees it).
 *
 * Alt pressed during the drag makes copies at the drop instead (one `element.duplicate`): the
 * originals stay drawn where they are and a dashed box shows the copies; releasing Alt before the
 * drop makes it a move again. Alt held from the press moves without snapping (web/tests/flow.mjs
 * relies on it; plan section 5, item 1).
 */
import { center, offset, type Point } from '../../board/geometry'
import type { Override } from '../../board/layer'
import { frameAt } from '../../board/model'
import { translationSnap, type SnapGuide } from '../../board/snap'
import type { Gesture, GestureContext, PointerInput } from './types'

const SNAP_KINDS = ['shape', 'text', 'image', 'card', 'frame']

/** The live route of a connector whose bound targets are being moved by `delta`. */
export function liveRoute(ctx: GestureContext, id: string, moving: ReadonlySet<string>, delta: Point): Point[] {
  const view = ctx.sync.view
  const element = view.elements.get(id)
  if (!element || element.kind !== 'connector') return []
  const endOf = (end: typeof element.start) => {
    if (!end) return null
    const target = end.binding ? view.elements.get(end.binding.target) : null
    const box = target ? view.rect(target) : null
    if (end.binding && target && box) {
      const moved = moving.has(target.id) ? offset(box, delta.x, delta.y) : box
      return view.boundPoint(target, moved, end.binding.anchor)
    }
    return moving.has(id) ? { x: end.point.x + delta.x, y: end.point.y + delta.y } : end.point
  }
  const shift = moving.has(id) ? delta : { x: 0, y: 0 }
  return [endOf(element.start), ...element.waypoints.map((p) => ({ x: p.x + shift.x, y: p.y + shift.y })), endOf(element.end)].filter((p): p is Point => p != null)
}

/** The press on element `hit` with the select tool: selects, and answers the move (null: nothing to move). */
export function downMove(ctx: GestureContext, input: PointerInput, hit: string): Gesture | null {
  const view = ctx.sync.view
  const id = view.selectable(hit)
  const selection = ctx.selection()
  const additive = input.shift || input.command
  let next: string[]
  if (additive) next = selection.includes(id) ? selection.filter((item) => item !== id) : [...selection, id]
  else next = selection.includes(id) ? [...selection] : [id]
  ctx.select(next)
  if (ctx.readOnly() || (additive && !next.includes(id))) return null
  const roots = next.filter((item) => !view.elements.get(item)?.locked)
  if (!roots.length) return null
  const moving = new Set(roots.flatMap((root) => view.withDescendants(root)))
  const base = ctx.selectionBox([...moving])
  if (!base) return null
  const layer = ctx.layer()
  const shownPage = view.page(layer?.pageId ?? view.pages[0]?.id)
  // Where each root's centre is, for the frame a drop lands it in (the engine's rule).
  const frames = shownPage ? view.framesOn(shownPage, moving) : []
  const centers: Point[] = frames.length
    ? roots.flatMap((root) => {
        const element = view.elements.get(root)
        const box = element ? view.bounds(element) : null
        return box ? [center(box)] : []
      })
    : []
  const connectors = view.connectorsOf(moving).filter((item) => !moving.has(item))
  const start = input.p
  const altAtStart = input.alt
  let delta: Point = { x: 0, y: 0 }
  return {
    type: 'move',
    threshold: true,
    cursor: 'move',
    move(ctx, input) {
      const z = ctx.view().zoom
      delta = { x: input.p.x - start.x, y: input.p.y - start.y }
      let guides: SnapGuide[] = []
      if (!input.alt) {
        const moved = offset(base, delta.x, delta.y)
        const margin = Math.max(128, 512 / z)
        const layer = ctx.layer()
        const near = layer?.query({ x0: moved.x0 - margin, y0: moved.y0 - margin, x1: moved.x1 + margin, y1: moved.y1 + margin }) ?? []
        const targets = near.filter((item) => !moving.has(item) && SNAP_KINDS.includes(ctx.sync.view.elements.get(item)?.kind ?? '')).flatMap((item) => layer?.worldBox(item) ?? [])
        const snap = translationSnap(moved, targets, 6 / z)
        delta = { x: delta.x + snap.adjustment.x, y: delta.y + snap.adjustment.y }
        guides = snap.guides
      }
      if (!altAtStart && input.alt) {
        // The originals stay where they are; a dashed box shows where the copies land.
        ctx.setOverrides(new Map())
        ctx.setHidden([])
        ctx.setPreview(null)
        ctx.setDraft({ type: 'rect', box: offset(base, delta.x, delta.y), dashed: true })
        ctx.setGuides(guides)
        ctx.setDrops([])
        return
      }
      const overrides = new Map<string, Override>()
      for (const item of moving) overrides.set(item, { dx: delta.x, dy: delta.y })
      ctx.setOverrides(overrides)
      ctx.setHidden(connectors)
      ctx.setPreview({ type: 'move', dx: delta.x, dy: delta.y })
      ctx.setDraft({ type: 'lines', lines: connectors.map((item) => liveRoute(ctx, item, moving, delta)), target: null })
      ctx.setGuides(guides)
      ctx.setDrops([...new Set(centers.flatMap((c) => frameAt(frames, { x: c.x + delta.x, y: c.y + delta.y })?.id ?? []))])
    },
    up(ctx, input, dragged) {
      ctx.setGuides([])
      ctx.setDrops([])
      if (!dragged) {
        ctx.clearPreview()
        return
      }
      if (!altAtStart && input.alt) {
        // Copies at the drop, the originals untouched: one element.duplicate, one undo step.
        ctx.duplicate(roots, delta, ctx.clearPreview)
        return
      }
      void ctx.commit('element.move', { ids: roots, delta: [delta.x, delta.y], reparent: true }, ctx.clearPreview)
    },
    cancel() {},
  }
}
