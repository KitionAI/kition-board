/**
 * Re-attach a selected connector's end: its handle dragged past the threshold draws the connector
 * as the engine would draw it with that end at the pointer (`connector.getRoute`: its route,
 * arrowheads, style and label) in place of the connector, and the shape under the pointer shows its
 * outline and side midpoints; the release runs one `connector.bind`, which keeps the label and the
 * style. A click on the handle changes nothing.
 */
import { endParams, targetHint, type EndTarget, type Gesture, type GestureContext, type PointerInput } from './types'

export function downTerminal(ctx: GestureContext, input: PointerInput, connector: string, terminal: 'start' | 'end'): Gesture {
  let current: EndTarget = { element: null, point: input.p }
  const element = ctx.sync.view.elements.get(connector)
  // The element the other end is on cannot take this end too (the engine refuses both ends on one).
  const other = (terminal === 'start' ? element?.end : element?.start)?.binding?.target
  const excluding = other ? [connector, other] : [connector]
  const query = ctx.routeQuery((routes) => {
    ctx.setHidden([connector])
    ctx.setDraft({ type: 'routes', routes })
  })
  return {
    type: 'terminal',
    threshold: true,
    cursor: null,
    move(ctx, input) {
      current = ctx.endAt(input.p, input, excluding)
      ctx.setConnectHint(targetHint(current))
      query.request({ connectors: [{ id: connector, [terminal === 'start' ? 'from' : 'to']: endParams(current) }] })
    },
    up(ctx, _input, dragged) {
      query.cancel()
      ctx.setConnectHint(null)
      if (!dragged) {
        ctx.clearPreview()
        return
      }
      void ctx.commit('connector.bind', { id: connector, terminal, to: endParams(current) }, ctx.clearPreview)
    },
    cancel(ctx) {
      query.cancel()
      ctx.setConnectHint(null)
    },
  }
}
