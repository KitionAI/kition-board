/**
 * Connect: from the element under the press (the connector tool), from a selected shape's
 * connection handle, or from one of a shape's side midpoints the select tool shows near its edge.
 * Past the threshold the connector as the engine would draw it follows the pointer
 * (`connector.getRoute`: its real route, arrowheads and style), and the shape under the pointer
 * shows its outline and its four side midpoints (within 16 px of one the end snaps to it); the
 * release runs one `connector.add` between the two ends with the drawing style's route (a click
 * makes nothing; from a side midpoint the select tool showed, a click selects the shape). Let go
 * over the empty canvas, the end stays free where it was let go.
 */
import { distance, type Point } from '../../board/geometry'
import { styleFor } from '../../board/style'
import { createdOf, endParams, MIN_DRAW, targetHint, type EndTarget, type Gesture, type GestureContext } from './types'

export function downConnect(ctx: GestureContext, from: EndTarget, at: Point, clickSelects = false): Gesture {
  let current: EndTarget = { element: null, point: at }
  const style = styleFor('connector', ctx.drawStyle())
  const query = ctx.routeQuery((routes) => ctx.setDraft({ type: 'routes', routes }))
  const entry = () => ({ from: endParams(from), to: endParams(current), ...(Object.keys(style).length ? { style } : {}) })
  const end = () => {
    query.cancel()
    ctx.setDraft(null)
    ctx.setConnectHint(null)
  }
  return {
    type: 'connect',
    threshold: true,
    cursor: null,
    move(ctx, input) {
      current = ctx.endAt(input.p, input, from.element ? [from.element] : [])
      ctx.setConnectHint(targetHint(current))
      if (current.element || distance(from.point, current.point) * ctx.view().zoom >= MIN_DRAW) query.request({ connectors: [entry()] })
    },
    up(ctx, _input, dragged) {
      end()
      if (!dragged && clickSelects && from.element) ctx.select([from.element])
      if (!dragged || distance(from.point, current.point) * ctx.view().zoom < MIN_DRAW) return
      void ctx.commit('connector.add', entry()).then((answer) => {
        const created = createdOf(answer)
        if (created.length) ctx.select(created)
        ctx.toolDone()
      })
    },
    cancel() {
      end()
    },
  }
}
