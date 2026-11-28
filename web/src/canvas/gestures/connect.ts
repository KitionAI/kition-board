/**
 * Connect: from the element under the press (the connector tool) or from a selected shape's
 * connection handle, past the threshold a live line follows the pointer and the element under it
 * lights up; the release runs one `connector.add` between the two ends (a click makes nothing). A
 * connector drawn from a handle is an elbow whatever the default route.
 */
import { distance, type Point } from '../../board/geometry'
import { styleFor } from '../../board/style'
import { createdOf, endParams, MIN_DRAW, type EndTarget, type Gesture, type GestureContext } from './types'

/** A polyline for a connector being drawn: straight, or the elbow the engine routes. */
function elbowOr(from: Point, to: Point, elbow: boolean): Point[] {
  if (!elbow) return [from, to]
  const middle = (from.x + to.x) / 2
  return [from, { x: middle, y: from.y }, { x: middle, y: to.y }, to]
}

export function downConnect(_ctx: GestureContext, from: EndTarget, at: Point, elbow: boolean): Gesture {
  let current: EndTarget = { element: null, point: at }
  return {
    type: 'connect',
    threshold: true,
    cursor: null,
    move(ctx, input) {
      current = ctx.endAt(input.p, input, from.element)
      ctx.setDraft({ type: 'lines', lines: [elbowOr(from.point, current.point, elbow)], target: current.element })
    },
    up(ctx, _input, dragged) {
      ctx.setDraft(null)
      if (!dragged || distance(from.point, current.point) * ctx.view().zoom < MIN_DRAW) return
      const params: Record<string, unknown> = { from: endParams(from), to: endParams(current) }
      const style = styleFor('connector', ctx.drawStyle())
      if (elbow) {
        delete style.connectorType
        params.type = 'elbow'
      }
      if (Object.keys(style).length) params.style = style
      void ctx.commit('connector.add', params).then((answer) => {
        const created = createdOf(answer)
        if (created.length) ctx.select(created)
        ctx.toolDone()
      })
    },
    cancel() {},
  }
}
