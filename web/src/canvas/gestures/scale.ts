/**
 * Scale a selection of several (or one stroke or group) by a handle of its bounds: every element
 * inside is previewed the way the engine will map it, the selection overlay is drawn on the new
 * bounds, and the release runs one `element.resize { ids, box }`. Dragged past the opposite side,
 * the bounds flip over it (rule 8). Held at the canvas's edge, the view scrolls.
 */
import type { Rect } from '../../board/geometry'
import { flippedHandle, handleCursor, resizedBox, scaleOverrides } from '../../board/resize'
import type { Handle } from '../../board/snap'
import type { Gesture, GestureContext } from './types'

export function downScale(_ctx: GestureContext, roots: string[], moving: string[], handle: Handle, box: Rect): Gesture {
  let current = box
  let cursor = handleCursor(handle, 0)
  return {
    type: 'scale',
    threshold: true,
    get cursor() {
      return cursor
    },
    edgeScroll: true,
    move(ctx, input) {
      const next = resizedBox(box, handle, input.p, input.shift)
      current = next.box
      cursor = handleCursor(flippedHandle(handle, next.flipX, next.flipY), 0)
      ctx.setOverrides(scaleOverrides(ctx.sync.view, moving, box, current))
      // Connectors from outside the selection are drawn again once their targets have moved.
      const inside = new Set(moving)
      ctx.setHidden(ctx.sync.view.connectorsOf(inside).filter((id) => !inside.has(id)))
      ctx.setPreview({ type: 'scale', box: current })
    },
    up(ctx, _input, dragged) {
      const same = Math.abs(current.x0 - box.x0) + Math.abs(current.y0 - box.y0) + Math.abs(current.x1 - box.x1) + Math.abs(current.y1 - box.y1) < 0.5
      if (!dragged || same) {
        ctx.clearPreview()
        return
      }
      void ctx.commit('element.resize', { ids: roots, box: [current.x0, current.y0, current.x1, current.y1] }, ctx.clearPreview)
    },
    cancel() {},
  }
}
