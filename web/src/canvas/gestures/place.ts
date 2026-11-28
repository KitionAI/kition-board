/**
 * The tools that place something on the press itself, without a drag: a mind map's root centred on
 * the pointer, ready to type into, and a text (an existing text under the press opens instead).
 */
import type { Point } from '../../board/geometry'
import { createdOf, type GestureContext } from './types'

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

export function placeText(ctx: GestureContext, p: Point, hit: string | null) {
  if (hit && ctx.sync.view.elements.get(hit)?.kind === 'text') ctx.openText(hit)
  else ctx.openText(null, p)
  if (!ctx.readOnly()) ctx.setTool('select')
}
