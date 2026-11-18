/**
 * Mind maps as the page reads them (the engine's `crates/kdm/kdm-model/src/mindmap.rs`): nodes are
 * shapes with `shapeStyle: mind-node`; a connector bound from one node to another makes the second
 * the first's child (the first such parent in page order); a collapsed node that is not a root
 * hides what is below it. And where the quick controls of a selected node go.
 *
 * Ported from Kition's `packages/board-editor/src/lib/whiteboardMindMap.ts` and
 * `lib/whiteboardMindMapControls.ts`.
 */
import type { Point, Rect } from './geometry'
import type { BoardElement, BoardView } from './model'

export const isMindNode = (element: BoardElement | undefined): element is BoardElement => element?.kind === 'shape' && element.shapeStyle === 'mind-node'

export type Edge = { connector: string; parent: string; child: string }
export type Topology = { edges: Edge[]; parent: Map<string, string>; children: Map<string, string[]> }

/** The mind maps of the page holding `ids` (in page order). */
export function topology(view: BoardView, ids: readonly string[]): Topology {
  const out: Topology = { edges: [], parent: new Map(), children: new Map() }
  for (const id of ids) {
    const connector = view.elements.get(id)
    if (connector?.kind !== 'connector') continue
    const parent = connector.start?.binding?.target
    const child = connector.end?.binding?.target
    if (!parent || !child || parent === child || out.parent.has(child)) continue
    if (!isMindNode(view.elements.get(parent)) || !isMindNode(view.elements.get(child))) continue
    out.parent.set(child, parent)
    out.children.set(parent, [...(out.children.get(parent) ?? []), child])
    out.edges.push({ connector: id, parent, child })
  }
  return out
}

export function rootOf(map: Topology, id: string): string {
  let at = id
  const seen = new Set<string>()
  while (map.parent.has(at) && !seen.has(at)) {
    seen.add(at)
    at = map.parent.get(at)!
  }
  return at
}

/** What collapsed nodes hide on every page: their descendants and the branches to them. */
export function hiddenByCollapse(view: BoardView): Set<string> {
  const out = new Set<string>()
  for (const page of view.pages) {
    const map = topology(view, page.elements)
    const branches = new Map<string, Edge[]>()
    for (const edge of map.edges) branches.set(edge.parent, [...(branches.get(edge.parent) ?? []), edge])
    const roots = page.elements.filter((id) => isMindNode(view.elements.get(id)) && !map.parent.has(id))
    const stack: Array<[string, boolean]> = roots.map((id) => [id, false])
    const seen = new Set<string>()
    while (stack.length) {
      const [id, above] = stack.pop()!
      if (seen.has(id)) continue
      seen.add(id)
      if (above) out.add(id)
      const hides = above || (map.parent.has(id) && view.elements.get(id)?.mindMapCollapsed === true)
      for (const edge of branches.get(id) ?? []) {
        if (hides) out.add(edge.connector)
        stack.push([edge.child, hides])
      }
    }
  }
  return out
}

export type Direction = 'both' | 'right' | 'left' | 'down'
export type Growth = 'left' | 'right' | 'down'

/** One map around a node: its root, direction, and the node's place in it. */
export type MindGraph = { root: string; direction: Direction; map: Topology; nodes: string[] }

/** The map `node` is in (v1's `getWhiteboardMindMapGraph`), or null when it is no node. */
export function graphOf(view: BoardView, node: string): MindGraph | null {
  if (!isMindNode(view.elements.get(node))) return null
  const page = view.pageOf(node)
  if (!page) return null
  const map = topology(view, page.elements)
  const root = rootOf(map, node)
  const nodes: string[] = []
  const queue = [root]
  while (queue.length) {
    const id = queue.shift()!
    if (nodes.includes(id)) continue
    nodes.push(id)
    queue.push(...(map.children.get(id) ?? []))
  }
  const stored = view.elements.get(root)?.mindMapDirection as Direction | null
  return { root, direction: stored ?? inferDirection(view, root, nodes), map, nodes }
}

function centerOf(view: BoardView, id: string): Point | null {
  const element = view.elements.get(id)
  const box = element ? view.rect(element) : null
  return box ? { x: (box.x0 + box.x1) / 2, y: (box.y0 + box.y1) / 2 } : null
}

function inferDirection(view: BoardView, root: string, nodes: readonly string[]): Direction {
  const c = centerOf(view, root)
  const others = nodes.filter((id) => id !== root).flatMap((id) => centerOf(view, id) ?? [])
  if (!c || !others.length) return 'right'
  const left = others.filter((p) => p.x < c.x - 20).length
  const right = others.filter((p) => p.x > c.x + 20).length
  const below = others.filter((p) => p.y > c.y + 20).length
  if (left > 0 && right > 0) return 'both'
  if (below > Math.max(left, right)) return 'down'
  return left > right ? 'left' : 'right'
}

/** The side of the root a node's branch grows on (v1's `getWhiteboardMindMapBranchSide`). */
export function branchSide(view: BoardView, graph: MindGraph, id: string): 'left' | 'right' | null {
  if (id === graph.root) return null
  let first = id
  for (let steps = 0; graph.map.parent.has(first) && graph.map.parent.get(first) !== graph.root && steps < graph.nodes.length; steps += 1) first = graph.map.parent.get(first)!
  const stored = view.elements.get(first)?.mindMapBranchSide
  if (stored === 'left' || stored === 'right') return stored
  const a = centerOf(view, first)
  const r = centerOf(view, graph.root)
  return a && r ? (a.x < r.x ? 'left' : 'right') : null
}

/** How many nodes are below `id`. */
export function descendants(graph: MindGraph, id: string): number {
  let count = 0
  const queue = [...(graph.map.children.get(id) ?? [])]
  const seen = new Set<string>()
  while (queue.length) {
    const next = queue.shift()!
    if (seen.has(next)) continue
    seen.add(next)
    count += 1
    queue.push(...(graph.map.children.get(next) ?? []))
  }
  return count
}

export type QuickControl = { kind: 'add' | 'collapse' | 'expand'; center: Point; direction: Growth; count?: number }
export type ControlLane = { direction: Growth; edge: Point; controls: QuickControl[] }

const DIAMETER = 22
const GAP = 9
const LEAD = 10

/** The quick controls beside a selected node (v1's `getWhiteboardMindMapControlLanes`). */
export function controlLanes(view: BoardView, graph: MindGraph, id: string, box: Rect, zoom: number): ControlLane[] {
  const z = Math.max(0.1, zoom)
  const growth: Growth[] =
    id === graph.root && graph.direction === 'both' ? ['left', 'right'] : graph.direction === 'down' ? ['down'] : graph.direction === 'left' || graph.direction === 'right' ? [graph.direction] : [branchSide(view, graph, id) ?? 'right']
  const kids = graph.map.children.get(id)?.length ?? 0
  const isRoot = id === graph.root
  const collapsed = view.elements.get(id)?.mindMapCollapsed === true
  return growth.map((direction) => {
    const vector = direction === 'left' ? { x: -1, y: 0 } : direction === 'right' ? { x: 1, y: 0 } : { x: 0, y: 1 }
    const edge = direction === 'left' ? { x: box.x0, y: (box.y0 + box.y1) / 2 } : direction === 'right' ? { x: box.x1, y: (box.y0 + box.y1) / 2 } : { x: (box.x0 + box.x1) / 2, y: box.y1 }
    const along = (p: Point, by: number) => ({ x: p.x + vector.x * by, y: p.y + vector.y * by })
    const first = along(edge, (LEAD + DIAMETER / 2) / z)
    const controls: QuickControl[] = []
    if (!isRoot && kids > 0 && collapsed) controls.push({ kind: 'expand', center: first, direction, count: descendants(graph, id) })
    else if (!isRoot && kids > 0) controls.push({ kind: 'collapse', center: first, direction }, { kind: 'add', center: along(first, (DIAMETER + GAP) / z), direction })
    else controls.push({ kind: 'add', center: first, direction })
    return { direction, edge, controls }
  })
}
