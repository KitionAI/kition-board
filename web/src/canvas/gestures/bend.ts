/**
 * Bend a selected connector (rule 11): the small point in the middle of a segment, dragged, adds a
 * bend there (`connector.setWaypoints`); a bend's point, dragged, moves it. An elbow keeps its right
 * angles: the middle point of one of its inner segments moves that segment sideways, and the
 * elbow's corners become its bends. While the pointer moves, the connector is drawn as the engine
 * would draw it with those bends (`connector.getRoute`); the release runs one
 * `connector.setWaypoints`. A click changes nothing (a double-click on a bend removes it, in
 * `BoardCanvas`).
 */
import type { Point } from '../../board/geometry'
import type { Gesture, GestureContext, PointerInput } from './types'

/** What the press took: a segment's middle (`insert` a bend there, or `shift` an elbow's segment) or a bend (`move` it). */
export type BendHandle = { kind: 'insert' | 'shift'; segment: number } | { kind: 'move'; waypoint: number }

/**
 * The bends after dragging `handle` from `start` to `p`: `corners` are the route's corners (start
 * and end included), `waypoints` the stored bends.
 */
export function bentWaypoints(handle: BendHandle, corners: readonly Point[], waypoints: readonly Point[], start: Point, p: Point): Point[] {
  if (handle.kind === 'move') return waypoints.map((at, index) => (index === handle.waypoint ? p : at))
  if (handle.kind === 'insert') return [...waypoints.slice(0, handle.segment), p, ...waypoints.slice(handle.segment)]
  const a = corners[handle.segment]
  const b = corners[handle.segment + 1]
  if (!a || !b) return [...waypoints]
  const horizontal = a.y === b.y
  const moved = corners.map((at, index) => {
    if (index !== handle.segment && index !== handle.segment + 1) return at
    return horizontal ? { x: at.x, y: at.y + p.y - start.y } : { x: at.x + p.x - start.x, y: at.y }
  })
  return moved.slice(1, -1)
}

export function downBend(ctx: GestureContext, input: PointerInput, connector: string, handle: BendHandle, corners: readonly Point[]): Gesture {
  const element = ctx.sync.view.elements.get(connector)
  const waypoints = element?.waypoints ?? []
  const start = input.p
  let next: Point[] = [...waypoints]
  const query = ctx.routeQuery((routes) => {
    ctx.setHidden([connector])
    ctx.setDraft({ type: 'routes', routes })
  })
  const pairs = () => next.map((at) => [at.x, at.y] as [number, number])
  return {
    type: 'bend',
    threshold: true,
    cursor: null,
    move(ctx, input) {
      next = bentWaypoints(handle, corners, waypoints, start, input.p)
      query.request({ connectors: [{ id: connector, waypoints: pairs() }] })
    },
    up(ctx, _input, dragged) {
      query.cancel()
      if (!dragged) {
        ctx.clearPreview()
        return
      }
      void ctx.commit('connector.setWaypoints', { id: connector, waypoints: pairs() }, ctx.clearPreview)
    },
    cancel() {
      query.cancel()
    },
  }
}
