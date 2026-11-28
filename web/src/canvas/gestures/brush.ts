/**
 * Marquee (rule 6): a press on the empty canvas clears the selection (Shift keeps it and adds to
 * it); past the threshold a dashed box selects, as it goes, what it touches, or with Cmd or Ctrl held
 * (read at every move, so it can change mid-drag) only what it encloses whole. A frame is selected
 * only when enclosed whole; a mind-map branch (a connector between two mind-map nodes) and a locked
 * element never are; an element inside a group selects the group, and what is inside a selected
 * frame or group is not selected besides it. Held at the canvas's edge, the view scrolls. Escape
 * puts back the selection from before the press.
 *
 * A click without a drag selects what a click on the empty canvas there means: the topmost locked
 * element under it (`clicked`; locked elements count as the empty canvas for a press, rule 2), else
 * the frame whose empty inside it is on.
 */
import { contains, intersects, rect, type Rect } from '../../board/geometry'
import { isMindNode } from '../../board/mindmap'
import { frameAt } from '../../board/model'
import type { Gesture, GestureContext, PointerInput } from './types'

/** Whether box `inner` lies inside `outer`. */
const encloses = (outer: Rect, inner: Rect) => contains(outer, { x: inner.x0, y: inner.y0 }) && contains(outer, { x: inner.x1, y: inner.y1 })

/** What a marquee over `box` selects (`whole`: only what it encloses whole). */
export function brushed(ctx: GestureContext, box: Rect, whole: boolean): string[] {
  const view = ctx.sync.view
  const layer = ctx.layer()
  const roots = new Set<string>()
  for (const id of layer?.query(box) ?? []) {
    const element = view.elements.get(id)
    if (!element || view.hidden.has(id) || element.locked) continue
    // A mind-map branch is part of its map, not something to select on its own.
    if (element.kind === 'connector' && isMindNode(view.elements.get(element.start?.binding?.target ?? '')) && isMindNode(view.elements.get(element.end?.binding?.target ?? ''))) continue
    roots.add(ctx.selectable(id))
  }
  const picked = [...roots].filter((id) => {
    const element = view.elements.get(id)
    if (!element || element.locked) return false
    const own = element.kind === 'group' ? view.bounds(element) : (layer?.worldBox(id) ?? view.bounds(element))
    if (!own) return false
    return whole || element.kind === 'frame' ? encloses(box, own) : intersects(box, own) || encloses(box, own)
  })
  // What is inside a picked frame or group goes with it.
  const chosen = new Set(picked)
  const inside = (id: string) => {
    let parent = view.elements.get(id)?.parent ?? null
    for (let depth = 0; parent && depth < 64; depth += 1) {
      if (chosen.has(parent)) return true
      parent = view.elements.get(parent)?.parent ?? null
    }
    return false
  }
  return picked.filter((id) => !inside(id))
}

export function downBrush(ctx: GestureContext, input: PointerInput, clicked: string | null): Gesture {
  const before = [...ctx.selection()]
  const additive = input.shift
  if (!additive) ctx.select([])
  const start = input.p
  const pick = (input: PointerInput) => {
    const box = rect(start.x, start.y, input.p.x, input.p.y)
    ctx.setDraft({ type: 'rect', box, dashed: true })
    const next = [...new Set([...(additive ? before : []), ...brushed(ctx, box, input.command)])]
    const now = ctx.selection()
    if (next.length !== now.length || next.some((id, at) => id !== now[at])) ctx.select(next)
  }
  return {
    type: 'brush',
    threshold: true,
    cursor: null,
    edgeScroll: true,
    move(ctx, input) {
      pick(input)
    },
    up(ctx, input, dragged) {
      if (dragged) {
        pick(input)
        ctx.setDraft(null)
        return
      }
      ctx.setDraft(null)
      if (additive) return
      if (clicked) {
        ctx.select([clicked])
        return
      }
      // A click on a frame's empty inside selects the frame (a drag there still draws a marquee).
      const view = ctx.sync.view
      const shownPage = view.page(ctx.layer()?.pageId ?? view.pages[0]?.id)
      const frame = shownPage ? frameAt(view.framesOn(shownPage, view.hidden), input.p) : null
      if (frame) ctx.select([frame.id])
    },
    cancel(ctx) {
      ctx.select(before)
    },
  }
}
