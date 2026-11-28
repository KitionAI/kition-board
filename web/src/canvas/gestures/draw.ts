/**
 * Draw a shape, a sticky note or a frame: past the threshold the dragged box is previewed and made
 * on release; a click (or a box smaller than `MIN_DRAW` screen pixels on a side) makes the default
 * size centred on the press (rule 9). One `element.add` (`frame.add` for a frame); the new element
 * is selected, the tool goes back to Select (unless tool lock is on), and a sticky note opens its text.
 */
import { center, rect, type Point, type Rect } from '../../board/geometry'
import { styleFor } from '../../board/style'
import type { Tool } from '../../tools'
import { MIN_DRAW, type Gesture, type GestureContext, type PointerInput } from './types'

/** The size a click makes for each drawing tool. */
export function defaultSize(tool: Tool): [number, number] {
  return tool === 'note' ? [180, 140] : tool === 'frame' ? [640, 400] : [160, 100]
}

/** The default box of `tool` centred on `p`. */
export function clickBox(tool: Tool, p: Point): Rect {
  const [w, h] = defaultSize(tool)
  return { x0: p.x - w / 2, y0: p.y - h / 2, x1: p.x + w / 2, y1: p.y + h / 2 }
}

export function downDraw(_ctx: GestureContext, input: PointerInput, tool: Tool): Gesture {
  const start = input.p
  return {
    type: 'draw',
    threshold: true,
    cursor: null,
    move(ctx, input) {
      ctx.setDraft({ type: 'rect', box: rect(start.x, start.y, input.p.x, input.p.y), dashed: false })
    },
    up(ctx, input, dragged) {
      ctx.setDraft(null)
      const z = ctx.view().zoom
      let box = rect(start.x, start.y, input.p.x, input.p.y)
      const small = !dragged || (box.x1 - box.x0) * z < MIN_DRAW || (box.y1 - box.y0) * z < MIN_DRAW
      if (small) box = clickBox(tool, start)
      const done = (answer: Record<string, unknown> | null) => {
        const created = (answer?.created as string[] | undefined) ?? (answer?.frame ? [answer.frame as string] : [])
        if (created.length) ctx.select(created)
        ctx.toolDone()
        if (tool === 'note' && created[0]) window.setTimeout(() => ctx.openText(created[0]), 0)
      }
      if (tool === 'frame') {
        void ctx.commit('frame.add', { box: [box.x0, box.y0, box.x1, box.y1] }).then(done)
        return
      }
      const element: Record<string, unknown> = { kind: 'shape', position: [box.x0, box.y0], size: [box.x1 - box.x0, box.y1 - box.y0], ...styleFor('shape', ctx.drawStyle()) }
      const parent = ctx.frameUnder(center(box))
      if (parent) element.parent = parent
      if (tool === 'note') element.shapeStyle = 'sticky'
      else element.shapeType = ctx.shapeType()
      void ctx.commit('element.add', { elements: [element] }).then(done)
    },
    cancel() {},
  }
}
