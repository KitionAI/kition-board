/**
 * Select and move. A press on an element selects it at once (Shift, Cmd or Ctrl adds it, or takes a
 * selected one out), so a drag can start straight away; past the threshold the selection's unlocked
 * elements move, snapping their box to the edges and centres of the elements near them, and the
 * frames they would drop into light up. The release runs one `element.move { reparent }` (a drop
 * into a frame makes it the parent, a drop out of one frees it).
 *
 * The connectors bound to what moves (and a moving connector with a bound end) are drawn as the
 * engine would draw them after the move, asked of it as the pointer goes (`connector.getRoute` with
 * `move`, every connector in one question): their real route, arrowheads and labels, the result of
 * the release's own command.
 *
 * Alt pressed during the drag makes copies at the drop instead (one `element.duplicate`): the
 * originals stay drawn where they are and a dashed box shows the copies; releasing Alt before the
 * drop makes it a move again. Alt held from the press moves without snapping (web/tests/flow.mjs
 * relies on it; plan section 5, item 1).
 */
import { center, offset, type Point } from '../../board/geometry'
import type { Override } from '../../board/layer'
import { frameAt, type BoardElement } from '../../board/model'
import { translationSnap, type SnapGuide } from '../../board/snap'
import type { Gesture, GestureContext, PointerInput } from './types'

const SNAP_KINDS = ['shape', 'text', 'image', 'card', 'frame']

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
  // A moving connector with an end bound to something that stays is not simply shifted.
  const bound = (element: BoardElement | undefined) => element?.kind === 'connector' && [element.start, element.end].some((end) => end?.binding)
  const connectors = [...new Set([...view.connectorsOf(moving), ...[...moving].filter((item) => bound(view.elements.get(item)))])]
  const routed = new Set(connectors)
  const start = input.p
  const altAtStart = input.alt
  let delta: Point = { x: 0, y: 0 }
  let copying = false
  const query = connectors.length
    ? ctx.routeQuery((routes) => {
        if (copying) return
        ctx.setHidden(connectors)
        ctx.setDraft({ type: 'routes', routes })
      })
    : null
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
      copying = !altAtStart && input.alt
      if (copying) {
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
      for (const item of moving) if (!routed.has(item)) overrides.set(item, { dx: delta.x, dy: delta.y })
      ctx.setOverrides(overrides)
      ctx.setPreview({ type: 'move', dx: delta.x, dy: delta.y })
      query?.request({ connectors: connectors.map((id) => ({ id })), move: { ids: roots, delta: [delta.x, delta.y] } })
      ctx.setGuides(guides)
      ctx.setDrops([...new Set(centers.flatMap((c) => frameAt(frames, { x: c.x + delta.x, y: c.y + delta.y })?.id ?? []))])
    },
    up(ctx, input, dragged) {
      query?.cancel()
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
    cancel() {
      query?.cancel()
    },
  }
}
