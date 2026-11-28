/**
 * Marquee: a press on the empty canvas clears the selection (Shift, Cmd or Ctrl keeps it and adds
 * to it); past the threshold a dashed box selects what it meets on release. A click without a drag
 * on a frame's empty inside selects the frame. Escape puts back the selection from before the press.
 */
import { rect } from '../../board/geometry'
import { frameAt } from '../../board/model'
import type { Gesture, GestureContext, PointerInput } from './types'

export function downBrush(ctx: GestureContext, input: PointerInput): Gesture {
  const before = [...ctx.selection()]
  const additive = input.shift || input.command
  if (!additive) ctx.select([])
  const start = input.p
  return {
    type: 'brush',
    threshold: true,
    cursor: null,
    move(ctx, input) {
      ctx.setDraft({ type: 'rect', box: rect(start.x, start.y, input.p.x, input.p.y), dashed: true })
    },
    up(ctx, input, dragged) {
      ctx.setDraft(null)
      const view = ctx.sync.view
      if (!dragged) {
        // A click on a frame's empty inside selects the frame (a drag there still draws a marquee).
        const shownPage = view.page(ctx.layer()?.pageId ?? view.pages[0]?.id)
        const frame = shownPage && !additive ? frameAt(view.framesOn(shownPage, view.hidden), input.p) : null
        if (frame) ctx.select([frame.id])
        return
      }
      const box = rect(start.x, start.y, input.p.x, input.p.y)
      const hits = (ctx.layer()?.query(box) ?? []).filter((id) => !view.hidden.has(id)).map((id) => view.selectable(id))
      ctx.select([...new Set([...(additive ? before : []), ...hits])])
    },
    cancel(ctx) {
      ctx.select(before)
    },
  }
}
