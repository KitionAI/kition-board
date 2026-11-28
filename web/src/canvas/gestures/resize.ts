/**
 * Resize one element by a handle: a rotated box resizes in its own frame, its opposite handle kept
 * where it is on the board (no snapping to the page's axes); an unrotated one snaps the dragged edges
 * to the boxes near it (Alt: no snapping, Shift: keep the aspect). The fragment is scaled as a
 * preview, the selection overlay drawn on the new box, and the release runs one `element.resize`.
 *
 * A text's left or right handle (`text`) sets its width (rule 10): an auto-width text becomes a
 * fixed-width one that wraps there. The box stored is the new width and `text.height` (one line for
 * a text that sized itself, else its stored height); the engine makes the box as tall as the
 * wrapped lines. The text is not stretched while the handle moves: only the new box is shown.
 */
import type { Rect } from '../../board/geometry'
import { resizedRotated, handleCursor } from '../../board/resize'
import { resizeSnap, type Handle } from '../../board/snap'
import type { Gesture, GestureContext } from './types'

export function downResize(_ctx: GestureContext, id: string, handle: Handle, box: Rect, rotation: number, text?: { height: number }): Gesture {
  let current = box
  return {
    type: 'resize',
    threshold: true,
    cursor: handleCursor(handle, rotation),
    move(ctx, input) {
      const z = ctx.view().zoom
      const layer = ctx.layer()
      const snapped = input.alt || rotation ? { point: input.p, guides: [] } : resizeSnap(handle, input.p, box, (layer?.query(box) ?? []).filter((item) => item !== id).flatMap((item) => layer?.worldBox(item) ?? []), 6 / z)
      current = resizedRotated(box, rotation, handle, snapped.point, input.shift && !text)
      if (text) {
        ctx.setPreview({ type: 'resize', box: current, rotation })
        ctx.setGuides(snapped.guides)
        return
      }
      const sx = (current.x1 - current.x0) / Math.max(1e-6, box.x1 - box.x0)
      const sy = (current.y1 - current.y0) / Math.max(1e-6, box.y1 - box.y0)
      ctx.setOverrides(new Map([[id, { dx: current.x0 - box.x0, dy: current.y0 - box.y0, sx, sy }]]))
      ctx.setHidden(ctx.sync.view.connectorsOf(new Set([id])))
      ctx.setPreview({ type: 'resize', box: current, rotation })
      ctx.setGuides(snapped.guides)
    },
    up(ctx, _input, dragged) {
      if (!dragged || current.x1 - current.x0 < 1 || current.y1 - current.y0 < 1) {
        ctx.clearPreview()
        return
      }
      const y1 = text ? current.y0 + text.height : current.y1
      void ctx.commit('element.resize', { id, box: [current.x0, current.y0, current.x1, y1] }, ctx.clearPreview)
    },
    cancel() {},
  }
}
