/** Pen and highlighter: the stroke drawn as the pointer goes, one stroke.add on release. */
import { distance, type Point } from '../../board/geometry'
import { simplifyStroke } from '../../board/freehand'
import { styleFor } from '../../board/style'
import type { Gesture, GestureContext, PointerInput } from './types'

export function downPen(_ctx: GestureContext, input: PointerInput, highlight: boolean): Gesture {
  const points: Point[] = [input.p]
  return {
    type: 'pen',
    threshold: false,
    cursor: null,
    move(ctx, input) {
      const last = points[points.length - 1]
      if (!last || distance(last, input.p) >= 1.5 / ctx.view().zoom) {
        points.push(input.p)
        ctx.setDraft({ type: 'pen', points: [...points], highlight })
      }
    },
    up(ctx) {
      ctx.setDraft(null)
      if (points.length < 2) return
      const simplified = simplifyStroke(points, Math.max(0.5, 1.25 / ctx.view().zoom)).map((q) => [q.x, q.y])
      const params: Record<string, unknown> = { points: simplified }
      const style = styleFor('stroke', highlight ? ctx.highlightStyle() : ctx.drawStyle())
      if (Object.keys(style).length) params.style = style
      void ctx.commit('stroke.add', params, () => ctx.setDraft(null))
    },
    cancel() {},
  }
}
