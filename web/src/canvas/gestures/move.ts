/**
 * Select and move. A press on an element outside the selection selects it at once (Shift, Cmd or
 * Ctrl adds it), so a drag can start straight away (rule 5); a press on a selected element keeps the
 * selection, and a release without a drag then takes it out (with Shift, Cmd or Ctrl) or narrows a
 * selection of several to it. Past the threshold the selection's unlocked elements move, snapping
 * their box within 8 screen pixels to the edges and centres of the elements near them while the
 * pointer is slower than 0.5 px/ms (Cmd or Ctrl held: no snapping); Shift keeps the move on the
 * axis it has gone further along (rule 7). The frames they would drop into light up. The release
 * runs one `element.move { reparent }` (a drop into a frame makes it the parent, a drop out of one
 * frees it). Held at the canvas's edge, the view scrolls (`edgeScroll`).
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
import { SNAP_SPEED, SNAP_THRESHOLD, type Gesture, type GestureContext, type PointerInput } from './types'

const SNAP_KINDS = ['shape', 'text', 'image', 'card', 'frame']

/** The press on element `hit` with the select tool: selects, and answers the move (null: nothing to move). */
export function downMove(ctx: GestureContext, input: PointerInput, hit: string): Gesture | null {
  const view = ctx.sync.view
  const id = ctx.selectable(hit)
  const selection = [...ctx.selection()]
  const additive = input.shift || input.command
  const was = selection.includes(id)
  // A release without a drag on a selected element: Shift, Cmd or Ctrl takes it out, a plain click
  // narrows a selection of several to it (rule 5).
  const clicked = was ? (additive ? selection.filter((item) => item !== id) : selection.length > 1 ? [id] : null) : null
  const next = was ? selection : additive ? [...selection, id] : [id]
  if (!was) ctx.select(next)
  if (ctx.readOnly()) {
    if (clicked) ctx.select(clicked)
    return null
  }
  const roots = next.filter((item) => !view.elements.get(item)?.locked)
  const moving = new Set(roots.flatMap((root) => view.withDescendants(root)))
  const base = roots.length ? ctx.selectionBox([...moving]) : null
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
    edgeScroll: true,
    move(ctx, input) {
      if (!base) return
      const z = ctx.view().zoom
      delta = { x: input.p.x - start.x, y: input.p.y - start.y }
      // Shift: only along the axis the pointer has gone further along.
      const axis = input.shift ? (Math.abs(delta.x) >= Math.abs(delta.y) ? 'x' : 'y') : null
      if (axis === 'x') delta = { x: delta.x, y: 0 }
      if (axis === 'y') delta = { x: 0, y: delta.y }
      let guides: SnapGuide[] = []
      if (!input.alt && !input.command && input.speed <= SNAP_SPEED) {
        const moved = offset(base, delta.x, delta.y)
        const margin = Math.max(128, 512 / z)
        const layer = ctx.layer()
        const near = layer?.query({ x0: moved.x0 - margin, y0: moved.y0 - margin, x1: moved.x1 + margin, y1: moved.y1 + margin }) ?? []
        const targets = near.filter((item) => !moving.has(item) && SNAP_KINDS.includes(ctx.sync.view.elements.get(item)?.kind ?? '')).flatMap((item) => layer?.worldBox(item) ?? [])
        const snap = translationSnap(moved, targets, SNAP_THRESHOLD / z)
        // A locked axis does not snap.
        delta = { x: delta.x + (axis === 'y' ? 0 : snap.adjustment.x), y: delta.y + (axis === 'x' ? 0 : snap.adjustment.y) }
        guides = snap.guides.filter((guide) => axis == null || guide.axis === axis)
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
      if (!dragged || !base) {
        ctx.clearPreview()
        if (!dragged && clicked) ctx.select(clicked)
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
