/**
 * Eraser: the unlocked elements the pointer passes are hidden at once and deleted together on
 * release (one element.delete, one undo step); Escape shows them again.
 */
import type { Gesture, GestureContext, PointerInput } from './types'

export function downErase(ctx: GestureContext, input: PointerInput): Gesture {
  const ids = new Set<string>()
  const at = (ctx: GestureContext, clientX: number, clientY: number) => {
    const hit = ctx.elementUnder(clientX, clientY, null)
    const element = hit ? ctx.sync.view.elements.get(hit) : null
    if (!element || element.locked || ids.has(element.id)) return
    ids.add(element.id)
    ctx.setHidden([...ids])
  }
  at(ctx, input.clientX, input.clientY)
  return {
    type: 'erase',
    threshold: false,
    cursor: null,
    move(ctx, input) {
      // Every coalesced point, so a quick stroke does not skip a thin line.
      for (const sample of input.native.getCoalescedEvents?.() ?? [input.native]) at(ctx, sample.clientX, sample.clientY)
    },
    up(ctx) {
      if (!ids.size) return
      ctx.select(ctx.selection().filter((id) => !ids.has(id)))
      void ctx.commit('element.delete', { ids: [...ids] }, ctx.clearPreview)
    },
    cancel() {},
  }
}
