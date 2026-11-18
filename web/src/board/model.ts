/**
 * The page's view of the board, read from the page replica (design note 4.1): pages in order,
 * each element where it is listed first, bound connector ends derived from their targets (1.4).
 * The editor reads geometry, hierarchy and culling bounds here and never writes the replica:
 * every change is an engine command.
 *
 * The read side of Kition's `packages/board-editor/src/kdm/boardKdmRead.ts`, for schema v2 and the
 * rules of `crates/kdm/kdm-model/src/view.rs`.
 */
import type { Replica } from '@kition/kdm'
import { anchorPoint, rect, rectOfPoints, rotatedBounds, union, type Point, type Rect } from './geometry'
import { hiddenByCollapse, isMindNode } from './mindmap'

export type Kind = 'shape' | 'text' | 'stroke' | 'connector' | 'image' | 'card' | 'frame' | 'group'
export type Binding = { target: string; anchor: [number, number] }
export type End = { point: Point; binding?: Binding }

export type BoardElement = {
  id: string
  kind: Kind
  position: Point | null
  size: { w: number; h: number } | null
  rotation: number
  locked: boolean
  parent: string | null
  text: string
  points: Point[]
  start: End | null
  end: End | null
  waypoints: Point[]
  shapeType: string | null
  shapeStyle: string | null
  fontSize: number | null
  fontFamily: string | null
  textAlign: string | null
  connectorType: string | null
  image: { hash: string; mime: string } | null
  /** A card's target: a workspace file (`path`) or a web address (`url`). */
  card: { kind: string; path: string | null; url: string | null; title: string | null } | null
  mindMapDirection: string | null
  mindMapBranchSide: string | null
  mindMapCollapsed: boolean
}

export type PageView = { id: string; name: string; elements: string[] }

export const ROOT_ID = 'board'

const KINDS = new Set<Kind>(['shape', 'text', 'stroke', 'connector', 'image', 'card', 'frame', 'group'])

const finite = (value: unknown): number | null => (typeof value === 'number' && Number.isFinite(value) ? value : null)
const string = (value: unknown): string | null => (typeof value === 'string' ? value : null)

function pair(value: unknown): Point | null {
  if (!Array.isArray(value) || value.length !== 2) return null
  const [x, y] = [finite(value[0]), finite(value[1])]
  return x == null || y == null ? null : { x, y }
}

function pairs(value: unknown): Point[] {
  return Array.isArray(value) ? value.flatMap((item) => pair(item) ?? []) : []
}

function end(value: unknown): End | null {
  if (!value || typeof value !== 'object') return null
  const record = value as Record<string, unknown>
  const point = pair(record.point)
  if (!point) return null
  const binding = record.binding as Record<string, unknown> | undefined
  const target = binding ? string(binding.target) : null
  if (!binding || !target) return { point }
  const anchor = pair(binding.anchor ?? [0.5, 0.5]) ?? { x: 0.5, y: 0.5 }
  return { point, binding: { target, anchor: [anchor.x, anchor.y] } }
}

function cardOf(value: unknown): BoardElement['card'] {
  if (!value || typeof value !== 'object') return null
  const card = value as Record<string, unknown>
  const target = card.target as Record<string, unknown> | undefined
  return { kind: string(card.kind) ?? 'file', path: target ? string(target.path) : null, url: string(card.url), title: string(card.title) }
}

/** The element `id` of the replica; `null` when it is not a live element. */
export function readElement(replica: Replica, id: string): BoardElement | null {
  if (!replica.exists(id) || replica.objectType(id) !== 'Element') return null
  const get = (field: string) => replica.get(id, field)
  const kind = string(get('kind')) as Kind | null
  if (!kind || !KINDS.has(kind)) return null
  const position = get('position') as Record<string, unknown> | null
  const size = get('size') as Record<string, unknown> | null
  const px = finite(position?.x)
  const py = finite(position?.y)
  const sw = finite(size?.w)
  const sh = finite(size?.h)
  const image = get('image') as Record<string, unknown> | null
  const shapeStyle = string(get('shapeStyle'))
  // Only a mind-map node has the mind-map fields worth reading (a commit re-reads its elements).
  const mind = kind === 'shape' && shapeStyle === 'mind-node'
  return {
    id,
    kind,
    position: px != null && py != null ? { x: px, y: py } : null,
    size: sw != null && sh != null ? { w: Math.max(0, sw), h: Math.max(0, sh) } : null,
    rotation: finite(get('rotation')) ?? 0,
    locked: get('locked') === true,
    parent: string(get('parent')),
    text: replica.text(id, 'text'),
    points: pairs(get('points')),
    start: end(get('start')),
    end: end(get('end')),
    waypoints: pairs(get('waypoints')),
    shapeType: string(get('shapeType')),
    shapeStyle,
    fontSize: finite(get('fontSize')),
    fontFamily: string(get('fontFamily')),
    textAlign: string(get('textAlign')),
    connectorType: string(get('connectorType')),
    image: image && typeof image.hash === 'string' ? { hash: image.hash, mime: string(image.mime) ?? 'application/octet-stream' } : null,
    card: kind === 'card' ? cardOf(get('card')) : null,
    mindMapDirection: mind ? string(get('mindMapDirection')) : null,
    mindMapBranchSide: mind ? string(get('mindMapBranchSide')) : null,
    mindMapCollapsed: mind && get('mindMapCollapsed') === true,
  }
}

export const isBoxed = (kind: Kind) => kind === 'shape' || kind === 'text' || kind === 'image' || kind === 'card' || kind === 'frame'
export const isBindable = isBoxed
export const canContain = (kind: Kind) => kind === 'frame' || kind === 'group'

function add(index: Map<string, Set<string>>, key: string, id: string) {
  const ids = index.get(key)
  if (ids) ids.add(id)
  else index.set(key, new Set([id]))
}

function drop(index: Map<string, Set<string>>, key: string, id: string) {
  const ids = index.get(key)
  if (!ids) return
  ids.delete(id)
  if (!ids.size) index.delete(key)
}

function targetsOf(element: BoardElement): string[] {
  const out: string[] = []
  if (element.start?.binding) out.push(element.start.binding.target)
  if (element.end?.binding) out.push(element.end.binding.target)
  return out
}

/** The materialised page view. `measured` answers the engine's size of a `text` element without one. */
export class BoardView {
  title = ''
  pages: PageView[] = []
  elements = new Map<string, BoardElement>()
  /** Each shown element's page index. */
  placement = new Map<string, number>()
  /** Moves each time the pages or their lists are re-read (a patch in place keeps it). */
  layout = 0
  /** What collapsed mind-map nodes hide (the engine's `mindmap::hidden`), on every page. */
  hidden: ReadonlySet<string> = new Set()
  /** The collapsed mind-map nodes: while there are none, nothing is hidden and nothing is computed. */
  private collapsed = new Set<string>()
  /** Each shown element's position in its page's list. */
  private order = new Map<string, number>()
  private childrenOf = new Map<string, Set<string>>()
  /** The connectors with an end bound to the key. */
  private boundTo = new Map<string, Set<string>>()

  constructor(private readonly measured: (id: string) => { w: number; h: number } | null) {}

  /**
   * Re-reads the board; with `changed`, only the elements that changed or were not shown before.
   * Answers the ids whose element or page placement changed.
   */
  refresh(replica: Replica, changed: ReadonlySet<string> | null): Set<string> {
    const touched = (changed && this.patch(replica, changed)) ?? this.rebuild(replica, changed)
    // A collapsed node hides what is below it; showing or hiding redraws those elements.
    const before = this.hidden
    const mapChanged = this.collapsed.size > 0 || before.size > 0
    if (mapChanged) {
      this.hidden = this.collapsed.size ? hiddenByCollapse(this) : new Set()
      for (const id of before) if (!this.hidden.has(id)) touched.add(id)
      for (const id of this.hidden) if (!before.has(id)) touched.add(id)
    }
    return touched
  }

  /**
   * Re-reads `changed` in place when every one is a shown element that is still live: no page or
   * list changed (their owners are not in `changed`), so placement and order stay (as the
   * engine's view does, `crates/kdm/kdm-model/src/view.rs`). Answers null, having changed
   * nothing, when the lists must be re-read.
   */
  private patch(replica: Replica, changed: ReadonlySet<string>): Set<string> | null {
    const fresh: BoardElement[] = []
    for (const id of changed) {
      if (!this.elements.has(id)) return null
      const element = readElement(replica, id)
      if (!element) return null
      fresh.push(element)
    }
    for (const element of fresh) {
      const old = this.elements.get(element.id)
      if (old) this.unindex(old)
      this.index(element)
      this.elements.set(element.id, element)
    }
    return new Set(changed)
  }

  private index(element: BoardElement) {
    if (element.mindMapCollapsed && isMindNode(element)) this.collapsed.add(element.id)
    if (element.parent) add(this.childrenOf, element.parent, element.id)
    if (element.kind === 'connector') for (const target of targetsOf(element)) add(this.boundTo, target, element.id)
  }

  private unindex(element: BoardElement) {
    this.collapsed.delete(element.id)
    if (element.parent) drop(this.childrenOf, element.parent, element.id)
    for (const target of targetsOf(element)) drop(this.boundTo, target, element.id)
  }

  private rebuild(replica: Replica, changed: ReadonlySet<string> | null): Set<string> {
    this.layout += 1
    const old = this.elements
    const oldPlacement = this.placement
    this.elements = new Map()
    this.placement = new Map()
    this.order = new Map()
    this.pages = []
    const touched = new Set<string>()
    this.title = replica.exists(ROOT_ID) ? (string(replica.get(ROOT_ID, 'title')) ?? '') : ''
    const pageIds = replica.exists(ROOT_ID) ? replica.list(ROOT_ID, 'pages') : []
    const seenPages = new Set<string>()
    for (const pageId of pageIds) {
      if (seenPages.has(pageId) || !replica.exists(pageId) || replica.objectType(pageId) !== 'Page') continue
      seenPages.add(pageId)
      const index = this.pages.length
      const shown: string[] = []
      for (const id of replica.list(pageId, 'elements')) {
        if (this.placement.has(id)) continue
        const previous = old.get(id)
        const element = previous && changed && !changed.has(id) ? previous : readElement(replica, id)
        if (!element) continue
        if (element !== previous || oldPlacement.get(id) !== index) touched.add(id)
        this.elements.set(id, element)
        this.placement.set(id, index)
        this.order.set(id, shown.length)
        shown.push(id)
      }
      this.pages.push({ id: pageId, name: string(replica.get(pageId, 'name')) ?? '', elements: shown })
    }
    for (const id of old.keys()) if (!this.elements.has(id)) touched.add(id)
    this.childrenOf = new Map()
    this.boundTo = new Map()
    this.collapsed = new Set()
    for (const element of this.elements.values()) this.index(element)
    return touched
  }

  page(id: string | null | undefined): PageView | null {
    return this.pages.find((page) => page.id === id) ?? null
  }

  pageOf(id: string): PageView | null {
    const index = this.placement.get(id)
    return index == null ? null : (this.pages[index] ?? null)
  }

  /** The elements on `id`'s page whose `parent` is `id`, in page order. */
  children(id: string): BoardElement[] {
    return this.inPageOrder(this.childrenOf.get(id), id)
  }

  /** The ids in `ids` shown on `of`'s page, back to front. */
  private inPageOrder(ids: ReadonlySet<string> | undefined, of: string): BoardElement[] {
    const page = this.placement.get(of)
    if (!ids?.size || page == null) return []
    return [...ids]
      .filter((id) => this.placement.get(id) === page)
      .sort((x, y) => (this.order.get(x) ?? 0) - (this.order.get(y) ?? 0))
      .flatMap((id) => this.elements.get(id) ?? [])
  }

  /** `id` and everything below it through `parent`. */
  withDescendants(id: string): string[] {
    const out = [id]
    for (let at = 0; at < out.length && out.length <= this.elements.size; at += 1) {
      for (const child of this.childrenOf.get(out[at]) ?? []) if (!out.includes(child)) out.push(child)
    }
    return out
  }

  /** The unrotated box of a boxed element. */
  rect(element: BoardElement): Rect | null {
    if (!isBoxed(element.kind) || !element.position) return null
    const size = element.size ?? (element.kind === 'text' ? this.measured(element.id) : null) ?? { w: 0, h: 0 }
    return rect(element.position.x, element.position.y, element.position.x + size.w, element.position.y + size.h)
  }

  /** Where a connector end is: derived from a bound target on the same page, else its point. */
  endPoint(connector: string, value: End): Point {
    const binding = value.binding
    if (binding) {
      const target = this.elements.get(binding.target)
      if (target && isBindable(target.kind) && this.placement.get(binding.target) === this.placement.get(connector)) {
        const box = this.rect(target)
        if (box) return anchorPoint(box, target.rotation, binding.anchor)
      }
    }
    return value.point
  }

  route(connector: BoardElement): Point[] {
    const out: Point[] = []
    if (connector.start) out.push(this.endPoint(connector.id, connector.start))
    out.push(...connector.waypoints)
    if (connector.end) out.push(this.endPoint(connector.id, connector.end))
    return out
  }

  /** The world box an element covers (rotation included); a group's is its children's. */
  bounds(element: BoardElement, depth = 0): Rect | null {
    switch (element.kind) {
      case 'stroke':
        return rectOfPoints(element.points)
      case 'connector':
        return rectOfPoints(this.route(element))
      case 'group':
        if (depth > 64) return null
        return this.children(element.id).reduce<Rect | null>((all, child) => union(all, this.bounds(child, depth + 1)), null)
      default: {
        const box = this.rect(element)
        return box ? rotatedBounds(box, element.rotation) : null
      }
    }
  }

  /** The connectors bound to any of `ids` (from the binding index, not the whole board). */
  connectorsOf(ids: ReadonlySet<string>): string[] {
    const out = new Set<string>()
    for (const id of ids) for (const connector of this.boundTo.get(id) ?? []) if (this.elements.has(connector)) out.add(connector)
    return [...out]
  }

  /** The topmost selectable ancestor of `id` (a group's member selects the group). */
  selectable(id: string): string {
    let current = this.elements.get(id)
    for (let depth = 0; current?.parent && depth < 64; depth += 1) {
      const parent = this.elements.get(current.parent)
      if (!parent || parent.kind !== 'group') break
      current = parent
    }
    return current?.id ?? id
  }

  /** The frames on `page` (but `excluding`) with their boxes: where a drop can land. */
  framesOn(page: PageView, excluding: ReadonlySet<string>): Array<{ id: string; box: Rect }> {
    const out: Array<{ id: string; box: Rect }> = []
    for (const id of page.elements) {
      const element = this.elements.get(id)
      if (!element || element.kind !== 'frame' || excluding.has(id)) continue
      const box = this.rect(element)
      if (box) out.push({ id, box })
    }
    return out
  }
}

/**
 * The smallest of `frames` whose box holds `p`: the frame a drop at `p` lands in (the engine's
 * `frame_at`, v1's `getBoardFrameAtPoint`).
 */
export function frameAt(frames: ReadonlyArray<{ id: string; box: Rect }>, p: Point): { id: string; box: Rect } | null {
  let best: { id: string; box: Rect } | null = null
  for (const frame of frames) {
    const { box } = frame
    if (p.x < box.x0 || p.x > box.x1 || p.y < box.y0 || p.y > box.y1) continue
    if (!best || (box.x1 - box.x0) * (box.y1 - box.y0) < (best.box.x1 - best.box.x0) * (best.box.y1 - best.box.y0)) best = frame
  }
  return best
}
