/**
 * Re-attach a selected connector's end: its handle dragged past the threshold hides the connector,
 * draws a live line from the other end to the pointer and lights up the element under it; the
 * release runs one `connector.bind`. A click on the handle changes nothing.
 */
import { endParams, type EndTarget, type Gesture, type GestureContext, type PointerInput } from './types'

export function downTerminal(ctx: GestureContext, input: PointerInput, connector: string, terminal: 'start' | 'end'): Gesture {
  let current: EndTarget = { element: null, point: input.p }
  ctx.setHidden([connector])
  return {
    type: 'terminal',
    threshold: true,
    cursor: null,
    move(ctx, input) {
      const element = ctx.sync.view.elements.get(connector)
      current = ctx.endAt(input.p, input, connector)
      if (!element) return
      const route = ctx.sync.view.route(element)
      const other = terminal === 'start' ? route[route.length - 1] : route[0]
      if (other) ctx.setDraft({ type: 'lines', lines: [terminal === 'start' ? [current.point, other] : [other, current.point]], target: current.element })
    },
    up(ctx, _input, dragged) {
      if (!dragged) {
        ctx.clearPreview()
        return
      }
      void ctx.commit('connector.bind', { id: connector, terminal, to: endParams(current) }, ctx.clearPreview)
    },
    cancel() {},
  }
}
