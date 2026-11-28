/** Pan: the middle button, the hand tool or Space held drags the view; Escape puts it back. */
import type { Gesture, GestureContext, PointerInput } from './types'

export function downPan(ctx: GestureContext, input: PointerInput): Gesture {
  const start = { x: input.clientX, y: input.clientY }
  const from = { ...ctx.view() }
  return {
    type: 'pan',
    threshold: false,
    cursor: 'grabbing',
    move(ctx, input) {
      const z = ctx.view().zoom
      ctx.applyView({ x: from.x - (input.clientX - start.x) / z, y: from.y - (input.clientY - start.y) / z })
    },
    up() {},
    cancel(ctx) {
      ctx.applyView({ x: from.x, y: from.y })
    },
  }
}
