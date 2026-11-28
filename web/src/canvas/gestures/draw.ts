/**
 * Draw a shape, a sticky note or a frame: past the threshold the dragged box is previewed and made
 * on release; Shift makes it square (rule 9). A shape's preview is the engine's own picture of that
 * shape (`shape.previews`, stretched to the box; its lines keep their width), not a rectangle. A
 * click (or a box smaller than `MIN_DRAW` screen pixels on a side) makes the default size centred on
 * the press. One `element.add` (`frame.add` for a frame); the new element is selected, the tool goes
 * back to Select (unless tool lock is on), and a sticky note opens its text.
 */
import { center, rect, type Point, type Rect } from '../../board/geometry'
import { styleFor } from '../../board/style'
import { isPreviews } from '../../panels/ShapeLibrary'
import type { Tool } from '../../tools'
import { MIN_DRAW, type Draft, type Gesture, type GestureContext, type PointerInput } from './types'

/** The size a click makes for each drawing tool. */
export function defaultSize(tool: Tool): [number, number] {
  return tool === 'note' ? [180, 140] : tool === 'frame' ? [640, 400] : [160, 100]
}

/** The default box of `tool` centred on `p`. */
export function clickBox(tool: Tool, p: Point): Rect {
  const [w, h] = defaultSize(tool)
  return { x0: p.x - w / 2, y0: p.y - h / 2, x1: p.x + w / 2, y1: p.y + h / 2 }
}

/** The box dragged from `start` to `p`; Shift makes it square (as wide as the longer side, toward the pointer). */
export function dragBox(start: Point, p: Point, square: boolean): Rect {
  if (!square) return rect(start.x, start.y, p.x, p.y)
  const side = Math.max(Math.abs(p.x - start.x), Math.abs(p.y - start.y))
  return rect(start.x, start.y, start.x + (p.x < start.x ? -side : side), start.y + (p.y < start.y ? -side : side))
}

/**
 * The engine's picture of `shape` from `shape.previews`'s answer: its content and the box it is drawn
 * in (the picture's own size, without the margin its viewBox adds), null when the answer has none.
 */
export function shapePicture(previews: unknown, shape: string): { viewBox: string; body: string } | null {
  if (!isPreviews(previews)) return null
  const svg = previews.groups.flatMap((group) => group.shapes).find((item) => item.shape === shape)?.svg
  const open = svg ? /^<svg\b[^>]*>/.exec(svg) : null
  if (!svg || !open || !svg.endsWith('</svg>')) return null
  const width = Number(/\swidth="([\d.]+)"/.exec(open[0])?.[1])
  const height = Number(/\sheight="([\d.]+)"/.exec(open[0])?.[1])
  if (!(width > 0) || !(height > 0)) return null
  return { viewBox: `0 0 ${width} ${height}`, body: svg.slice(open[0].length, -'</svg>'.length) }
}

export function downDraw(ctx: GestureContext, input: PointerInput, tool: Tool): Gesture {
  const start = input.p
  const shape = tool === 'shape' ? ctx.shapeType() : null
  const picture = shape ? shapePicture(ctx.sync.previews, shape) : null
  return {
    type: 'draw',
    threshold: true,
    cursor: null,
    move(ctx, input) {
      const box = dragBox(start, input.p, input.shift)
      const draft: Draft = shape ? { type: 'shape', box, shape, picture } : { type: 'rect', box, dashed: false }
      ctx.setDraft(draft)
    },
    up(ctx, input, dragged) {
      ctx.setDraft(null)
      const z = ctx.view().zoom
      let box = dragBox(start, input.p, input.shift)
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
