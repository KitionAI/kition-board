/**
 * Nudging with the arrow keys (rule 7): 1 unit, 10 with Shift. A run of nudges (a key held down and
 * repeating, or several arrows held together) is previewed as a move (the fragments, the selection
 * overlay and the bound connectors on their real routes, as `move.ts` draws them) and lands as one
 * `element.move` when the last arrow key is let go, so the run is one undo step. The canvas ends a
 * run early, committing it, when anything else happens (another key, a press, the page losing focus).
 */
import type { Point } from '../../board/geometry'
import type { Override } from '../../board/layer'
import type { BoardElement } from '../../board/model'
import type { GestureContext } from './types'

export class NudgeRun {
  private delta: Point = { x: 0, y: 0 }
  private readonly moving: Set<string>
  private readonly connectors: string[]
  private readonly query: ReturnType<GestureContext['routeQuery']> | null

  constructor(
    ctx: GestureContext,
    /** The elements the run moves (the selection, as element.move takes it). */
    readonly ids: readonly string[],
  ) {
    const view = ctx.sync.view
    const roots = ids.filter((id) => !view.elements.get(id)?.locked)
    this.moving = new Set(roots.flatMap((root) => view.withDescendants(root)))
    const bound = (element: BoardElement | undefined) => element?.kind === 'connector' && [element.start, element.end].some((end) => end?.binding)
    this.connectors = [...new Set([...view.connectorsOf(this.moving), ...[...this.moving].filter((id) => bound(view.elements.get(id)))])]
    this.query = this.connectors.length
      ? ctx.routeQuery((routes) => {
          ctx.setHidden(this.connectors)
          ctx.setDraft({ type: 'routes', routes })
        })
      : null
  }

  /** Whether the run is over the same elements as `ids`. */
  same(ids: readonly string[]) {
    return ids.length === this.ids.length && ids.every((id, at) => id === this.ids[at])
  }

  /** One more nudge by (dx, dy), previewed. */
  add(ctx: GestureContext, dx: number, dy: number) {
    this.delta = { x: this.delta.x + dx, y: this.delta.y + dy }
    const { x, y } = this.delta
    const routed = new Set(this.connectors)
    const overrides = new Map<string, Override>()
    for (const id of this.moving) if (!routed.has(id)) overrides.set(id, { dx: x, dy: y })
    ctx.setOverrides(overrides)
    ctx.setPreview({ type: 'move', dx: x, dy: y })
    this.query?.request({ connectors: this.connectors.map((id) => ({ id })), move: { ids: [...this.ids], delta: [x, y] } })
  }

  /** The run lands: one element.move by everything it nudged (nothing when it came back to where it was). */
  end(ctx: GestureContext) {
    this.query?.cancel()
    const { x, y } = this.delta
    if (!x && !y) {
      ctx.clearPreview()
      return
    }
    void ctx.commit('element.move', { ids: [...this.ids], delta: [x, y] }, ctx.clearPreview)
  }
}
