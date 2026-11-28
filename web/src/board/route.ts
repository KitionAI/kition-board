/**
 * The engine's `connector.getRoute` (design note 5.1) as the page uses it: how connectors would be
 * drawn, without writing anything. The connect, re-attach and bend gestures and the move of shapes
 * with connectors bound to them all draw their preview from its answer, so a preview is the line
 * the command at the end of the gesture leaves (plan "Board usable by hand", rule 11). The page has
 * no route code of its own.
 */
import type { Point, Rect } from './geometry'
import type { TextFrame } from './sync'

/** A connector end as `connector.add` takes it. */
export type EndParam = { element: string; anchor?: [number, number] } | { point: [number, number] }

/** One connector to draw: a stored one (`id`) with what replaces its fields, or a new one. */
export type RouteEntry = {
  id?: string
  from?: EndParam
  to?: EndParam
  type?: string
  waypoints?: Array<[number, number]>
  label?: string
  style?: Record<string, unknown>
}

export type RouteRequest = { connectors: RouteEntry[]; move?: { ids: string[]; delta: [number, number] } }

/** An end as it would be stored: where it is drawn, and what it is attached to. */
export type RouteEnd = { point: Point; target: string | null; anchor: [number, number] | null }

/** An arrowhead as drawn. */
export type RouteHead = { end: 'start' | 'end'; kind: string; tip: Point; direction: Point; length: number; halfWidth: number }

/** One connector as the engine would draw it. */
export type Route = {
  id: string | null
  /** The corners the route turns at, start and end included. */
  points: Point[]
  /** SVG path data of the line through its route. */
  path: string
  start: RouteEnd | null
  end: RouteEnd | null
  heads: RouteHead[]
  /** The connector drawn as the board draws it, in world coordinates. */
  svg: string
  box: Rect
  text: TextFrame | null
}

const finite = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value)
const point = (value: unknown): Point | null => (Array.isArray(value) && value.length === 2 && finite(value[0]) && finite(value[1]) ? { x: value[0], y: value[1] } : null)

function end(value: unknown): RouteEnd | null {
  const record = value as { point?: unknown; binding?: { target?: unknown; anchor?: unknown } } | null
  const at = point(record?.point)
  if (!at) return null
  const target = typeof record?.binding?.target === 'string' ? record.binding.target : null
  const anchor = point(record?.binding?.anchor)
  return { point: at, target, anchor: anchor ? [anchor.x, anchor.y] : null }
}

/** `connector.getRoute`'s answer, read defensively (an entry it cannot read is left out). */
export function parseRoutes(answer: unknown): Route[] {
  const items = (answer as { connectors?: unknown } | null)?.connectors
  if (!Array.isArray(items)) return []
  return items.flatMap((item): Route[] => {
    const record = item as Record<string, unknown>
    const points = Array.isArray(record.points) ? record.points.flatMap((p) => point(p) ?? []) : []
    const box = Array.isArray(record.box) && record.box.length === 4 && record.box.every(finite) ? { x0: record.box[0], y0: record.box[1], x1: record.box[2], y1: record.box[3] } : null
    if (!box || typeof record.svg !== 'string') return []
    const heads = Array.isArray(record.heads)
      ? record.heads.flatMap((head): RouteHead[] => {
          const h = head as Record<string, unknown>
          const tip = point(h.tip)
          const direction = point(h.direction)
          return tip && direction && (h.end === 'start' || h.end === 'end') && typeof h.kind === 'string' && finite(h.length) && finite(h.halfWidth)
            ? [{ end: h.end, kind: h.kind, tip, direction, length: h.length, halfWidth: h.halfWidth }]
            : []
        })
      : []
    const text = record.text as (Omit<TextFrame, 'area'> & { area: [number, number, number, number] }) | undefined
    return [
      {
        id: typeof record.id === 'string' ? record.id : null,
        points,
        path: typeof record.path === 'string' ? record.path : '',
        start: end(record.start),
        end: end(record.end),
        heads,
        svg: record.svg,
        box,
        text: text && Array.isArray(text.area) ? { ...text, area: { x0: text.area[0], y0: text.area[1], x1: text.area[2], y1: text.area[3] } } : null,
      },
    ]
  })
}

/**
 * Asks the engine for routes while a gesture moves: one question in flight at a time, and only the
 * latest one waiting behind it (a pointer that moves faster than the engine answers skips the
 * positions in between). `cancel` drops whatever is still to come, so an answer that arrives after
 * the gesture ended draws nothing.
 */
export class RouteQuery {
  private waiting: RouteRequest | null = null
  private busy = false
  private done = false

  constructor(
    private readonly ask: (request: RouteRequest) => Promise<Route[]>,
    private readonly answer: (routes: Route[]) => void,
  ) {}

  request(request: RouteRequest) {
    if (this.done) return
    if (this.busy) {
      this.waiting = request
      return
    }
    this.busy = true
    void this.ask(request)
      .then((routes) => {
        if (!this.done) this.answer(routes)
      })
      .catch(() => undefined)
      .finally(() => {
        this.busy = false
        const next = this.waiting
        this.waiting = null
        if (next) this.request(next)
      })
  }

  cancel() {
    this.done = true
    this.waiting = null
  }
}
