/**
 * Rotate the selection by its rotate handle: the angle from the selection's centre to the pointer
 * (Shift: 15° steps). One element turns about its own centre; several turn together about the
 * selection's centre, as `element.rotate { about: "selection" }` will turn them. The fragments and
 * the selection overlay turn as the pointer goes; the release runs one `element.rotate`.
 */
import { center, rotate, type Point } from '../../board/geometry'
import type { Override } from '../../board/layer'
import type { Gesture, GestureContext, PointerInput } from './types'

/** The rotate handle's cursor: a turning arrow (black with a white edge, as the system's cursors), the grab hand where images are not taken. */
export const ROTATE_CURSOR =
  `url("data:image/svg+xml,${encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><g fill="none" stroke-linecap="round" stroke-linejoin="round"><path d="M18 8a7 7 0 1 0 1 6" stroke="white" stroke-width="5"/><path d="M19 3v5h-5" stroke="white" stroke-width="5"/><path d="M18 8a7 7 0 1 0 1 6" stroke="black" stroke-width="2"/><path d="M19 3v5h-5" stroke="black" stroke-width="2"/></g></svg>')}") 12 12, grab`

export function downRotate(_ctx: GestureContext, input: PointerInput, ids: string[], origin: Point): Gesture {
  const start = Math.atan2(input.p.y - origin.y, input.p.x - origin.x)
  let degrees = 0
  return {
    type: 'rotate',
    threshold: true,
    cursor: ROTATE_CURSOR,
    move(ctx, input) {
      degrees = ((Math.atan2(input.p.y - origin.y, input.p.x - origin.x) - start) * 180) / Math.PI
      if (input.shift) degrees = Math.round(degrees / 15) * 15
      const view = ctx.sync.view
      const overrides = new Map<string, Override>()
      for (const id of ids) {
        const element = view.elements.get(id)
        const box = element ? view.rect(element) : null
        if (ids.length > 1 && box) {
          // Several turn about the selection's centre: each box's centre goes round it too.
          const c = center(box)
          const moved = rotate(c, origin, degrees)
          overrides.set(id, { dx: moved.x - c.x, dy: moved.y - c.y, rotate: degrees, about: origin })
        } else overrides.set(id, { dx: 0, dy: 0, rotate: degrees, about: origin })
      }
      ctx.setOverrides(overrides)
      ctx.setHidden(view.connectorsOf(new Set(ids)))
      ctx.setPreview({ type: 'rotate', origin, degrees })
    },
    up(ctx, _input, dragged) {
      if (!dragged || Math.abs(degrees) < 0.5) {
        ctx.clearPreview()
        return
      }
      void ctx.commit('element.rotate', { ids, degrees, about: ids.length > 1 ? 'selection' : 'each' }, ctx.clearPreview)
    },
    cancel() {},
  }
}
