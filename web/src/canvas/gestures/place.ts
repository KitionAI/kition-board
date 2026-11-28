/**
 * The tools that place something where they are pressed: a mind map's root centred on the
 * pointer, ready to type into, and a text (an existing text under the press opens instead).
 */
import type { Point } from '../../board/geometry'
import { createdOf, type Gesture, type GestureContext, type PointerInput } from './types'

export function placeMindMap(ctx: GestureContext, p: Point) {
  const root: Record<string, unknown> = { kind: 'shape', shapeStyle: 'mind-node', mindMapDirection: 'right', position: [p.x - 80, p.y - 30], size: [160, 60] }
  const parent = ctx.frameUnder(p)
  if (parent) root.parent = parent
  void ctx.commit('element.add', { elements: [root] }).then((answer) => {
    const created = createdOf(answer)
    ctx.setTool('select')
    if (!created.length) return
    ctx.select(created)
    window.setTimeout(() => ctx.openText(created[0]), 0)
  })
}

/**
 * The text tool (rule 10): a click makes an auto-width text (no wrapping; Shift+Enter makes a new
 * line), its first line centred on the pointer; a drag across makes a fixed-width text as wide as
 * the drag, which wraps at that width. A press on an existing text opens it instead. The tool goes
 * back to Select either way.
 */
export function downText(ctx: GestureContext, input: PointerInput, hit: string | null): Gesture | null {
  if (hit && ctx.sync.view.elements.get(hit)?.kind === 'text') {
    ctx.openText(hit)
    ctx.setTool('select')
    return null
  }
  const start = input.p
  let width: number | null = null
  return {
    type: 'text',
    threshold: true,
    cursor: null,
    move(ctx, input) {
      const z = ctx.view().zoom
      const dx = input.p.x - start.x
      // Narrower than a few letters is still a click (tldraw's 24 pixels: six times the drag threshold).
      width = Math.abs(dx) * z >= TEXT_DRAG ? Math.abs(dx) : null
      const line = ctx.textLineHeight()
      ctx.setDraft(width == null ? null : { type: 'rect', box: { x0: Math.min(start.x, input.p.x), y0: start.y - line / 2, x1: Math.max(start.x, input.p.x), y1: start.y + line / 2 }, dashed: true })
    },
    up(ctx, input, dragged) {
      ctx.setDraft(null)
      ctx.setTool('select')
      if (dragged && width != null) ctx.openNewText(Math.min(start.x, input.p.x), start.y, width)
      else ctx.openNewText(start.x, start.y, null)
    },
    cancel(ctx) {
      ctx.setDraft(null)
    },
  }
}

/** How far across (screen pixels) the text tool is dragged before the text gets a fixed width. */
export const TEXT_DRAG = 24
