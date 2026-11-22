/**
 * The board canvas (R2c, design note 4): the page view of the board drawn with the engine's SVG
 * fragments (`ElementLayer`), and the interaction on top of it: select, move (dropping into
 * frames), resize (one box, rotated or not, or a selection), rotate, marquee, draw shapes, frames,
 * text, strokes and highlights, erase, connect, re-attach connector ends, grow mind maps, pan and
 * zoom, and the text box. During a gesture only the editor's own view changes (transforms of cached
 * fragments, a live line for the connectors of moved shapes); when it ends, one engine command
 * makes the change, one undo step. The gesture's preview stays until that command's change has
 * arrived through the page replica, and goes in the same frame its result is shown (no flicker).
 *
 * Ported from Kition's `packages/board-editor/src/components/WhiteboardCanvas.tsx`,
 * `WhiteboardSelectionOverlay.tsx`, `hooks/useWhiteboardInteractionMachine.ts`,
 * `useWhiteboardCamera.ts`, `useWhiteboardKeyboard.ts` and `lib/boardInteractionMachine.ts`,
 * `whiteboardConnectionHandles.ts` and `WhiteboardMindMapQuickControls.tsx`.
 * Not ported: the selection toolbar (its commands are in the Arrange menu and the side panel) and
 * agent previews.
 */
import { useCallback, useEffect, useImperativeHandle, useMemo, useRef, useState, type Ref } from 'react'
import { FrameChrome, toast, type KitCanvasHandle, type Strings } from '@kition/sdk/editor'
import * as clipboard from '../board/clipboard'
import { anchorPoint, center, distance, nearestEdgeAnchor, offset, rect, rotate, union, type Point, type Rect } from '../board/geometry'
import { rayAnchor } from '../board/outline'
import { ElementLayer, type Override, type Viewport } from '../board/layer'
import { controlLanes, graphOf, isMindNode } from '../board/mindmap'
import { frameAt, isBoxed, type BoardElement } from '../board/model'
import { resizeSnap, translationSnap, type Handle, type SnapGuide } from '../board/snap'
import { simplifyStroke } from '../board/freehand'
import type { BoardSync } from '../board/sync'
import { handleCursor, handlePoint, resized, resizedRotated, scaleOverrides } from '../board/resize'
import { styleFor, type Style } from '../board/style'
import { TOOL_KEYS, type Tool } from '../tools'
import { CanvasMenu, type CanvasAction, type CanvasMenuState } from './CanvasMenu'
import { Minimap, type DocumentArea, type ViewportStore } from './Minimap'
import { TextEditor, type TextEditing } from './TextEditor'

export type { Tool }

export type BoardCanvasApi = {
  /** The editor's keys; answers whether it handled the key. */
  keyDown: (event: KeyboardEvent) => boolean
  /** Adds a mind-map branch to `node` (or a sibling beside it), selected and ready to type into. */
  grow: (node: string, sibling: boolean) => void
  /** Opens the text box on an element (a card's caption, a label). */
  edit: (id: string) => void
  /** Gives the keys back to the canvas (after a dialog or a panel placed something). */
  focus: () => void
}

type Props = {
  sync: BoardSync
  page: string | null
  run: (command: string, params: Record<string, unknown>) => Promise<string | null>
  /** A reading command (`clipboard.copy`), also in a read-only board; answers parsed JSON or null. */
  query: (command: string, params: Record<string, unknown>) => Promise<unknown>
  tool: Tool
  shapeType: string
  /** What the drawing tools give new elements, and the highlighter its strokes. */
  drawStyle: Style
  highlightStyle: Style
  onTool: (tool: Tool) => void
  readOnly: boolean
  selection: readonly string[]
  onSelection: (ids: string[]) => void
  onZoom: (zoom: number) => void
  area: DocumentArea | null
  revision: number
  render: (x: number, y: number, scale: number, width: number, height: number) => Promise<Uint8Array>
  t: Strings
  handle: Ref<KitCanvasHandle>
  api: Ref<BoardCanvasApi>
}

const MIN_ZOOM = 0.05
const MAX_ZOOM = 4
const MIN_DRAW = 6
const HANDLES: readonly Handle[] = ['north-west', 'north', 'north-east', 'east', 'south-east', 'south', 'south-west', 'west']
const CONNECTION_HANDLES: Array<{ side: string; anchor: [number, number]; normal: Point }> = [
  { side: 'north', anchor: [0.5, 0], normal: { x: 0, y: -1 } },
  { side: 'east', anchor: [1, 0.5], normal: { x: 1, y: 0 } },
  { side: 'south', anchor: [0.5, 1], normal: { x: 0, y: 1 } },
  { side: 'west', anchor: [0, 0.5], normal: { x: -1, y: 0 } },
]

/** Where a connector end goes: an element (and where on it) or a free point. */
type EndTarget = { element: string; anchor: [number, number] | null; point: Point } | { element: null; point: Point }

type Gesture =
  | { type: 'pan'; start: Point; from: Viewport }
  /**
   * Alt pressed during the drag makes copies at the drop instead (one element.duplicate); Alt held
   * from the press (`altAtStart`) moves without snapping, as before.
   */
  | { type: 'move'; start: Point; roots: string[]; moving: Set<string>; base: Rect; connectors: string[]; delta: Point; moved: boolean; frames: Array<{ id: string; box: Rect }>; centers: Point[]; altAtStart: boolean }
  | { type: 'resize'; id: string; handle: Handle; box: Rect; rotation: number; current: Rect }
  | { type: 'scale'; roots: string[]; moving: string[]; handle: Handle; box: Rect; current: Rect }
  | { type: 'rotate'; ids: string[]; origin: Point; start: number; degrees: number }
  | { type: 'brush'; start: Point; current: Point; additive: boolean; initial: string[] }
  | { type: 'draw'; tool: Tool; start: Point; current: Point }
  | { type: 'connect'; from: EndTarget; current: EndTarget; elbow: boolean }
  | { type: 'terminal'; connector: string; terminal: 'start' | 'end'; current: EndTarget }
  | { type: 'pen'; points: Point[]; highlight: boolean }
  | { type: 'erase'; ids: Set<string> }

/** What the overlay draws for the gesture under way. */
type Draft =
  | { type: 'rect'; box: Rect; dashed: boolean }
  | { type: 'lines'; lines: Point[][]; target: string | null }
  | { type: 'pen'; points: Point[]; highlight: boolean }

const errorText = (error: unknown) => (error instanceof Error ? error.message : String(error))

function selectionBox(sync: BoardSync, layer: ElementLayer | null, ids: readonly string[]): Rect | null {
  let out: Rect | null = null
  for (const id of ids) {
    const element = sync.view.elements.get(id)
    // A selection may name an element whose change has not reached the page yet.
    if (element) out = union(out, layer?.worldBox(id) ?? sync.view.bounds(element))
  }
  return out
}

/** The element ids a command's answer names as made or changed (`created`, `moved`, `updated`). */
function touchedBy(answer: Record<string, unknown> | null): string[] {
  if (!answer) return []
  return ['created', 'moved', 'updated'].flatMap((key) => {
    const ids = answer[key]
    return Array.isArray(ids) ? ids.filter((id): id is string => typeof id === 'string') : []
  })
}

/** The ids a command's answer names as made. */
function createdOf(answer: Record<string, unknown> | null): string[] {
  const ids = answer?.created
  return Array.isArray(ids) ? ids.filter((id): id is string => typeof id === 'string') : []
}

/** Whether `target` is a field that keeps its own clipboard. */
function typingIn(target: EventTarget | null): boolean {
  return (target as Element | null)?.closest?.('input, textarea, select, [contenteditable]:not([contenteditable="false"])') != null
}

export function BoardCanvas(props: Props) {
  const { sync, page, run, tool, shapeType, onTool, readOnly, selection, onSelection, onZoom, t } = props
  const container = useRef<HTMLDivElement>(null)
  const world = useRef<SVGGElement>(null)
  const elementsRoot = useRef<SVGGElement>(null)
  const layerRef = useRef<ElementLayer | null>(null)
  const viewport = useRef<Viewport>({ x: 0, y: 0, zoom: 1, width: 0, height: 0 })
  const listeners = useRef(new Set<() => void>())
  const [zoom, setZoom] = useState(1)
  const [draft, setDraft] = useState<Draft | null>(null)
  const [guides, setGuides] = useState<SnapGuide[]>([])
  /** The frames a move under way drops its elements into. */
  const [drops, setDrops] = useState<string[]>([])
  const [editing, setEditing] = useState<TextEditing | null>(null)
  const [tick, setTick] = useState(0)
  const gesture = useRef<Gesture | null>(null)
  /** The last pointer events, for a failing flow test to show. */
  const trace = useRef<string[]>([])
  const note = (entry: string) => {
    trace.current.push(entry)
    if (trace.current.length > 24) trace.current.shift()
  }
  const space = useRef(false)
  const latest = useRef(props)
  latest.current = props
  const editingNow = useRef(editing)
  editingNow.current = editing
  /** Where the pointer is over the canvas (client coordinates), for a paste; null outside it. */
  const pointer = useRef<Point | null>(null)
  /** A Cmd/Ctrl+V waiting for the browser's paste event; `handled` once one of the two pasted. */
  const pasteKey = useRef<{ at: Point; handled: boolean } | null>(null)
  /** The canvas menu: where it opened (in the board too), and the elements it is on (null: the empty canvas). */
  const [menu, setMenu] = useState<{ at: Point; world: Point; ids: string[] | null } | null>(null)
  /** The paste and drop of this render, for the page's listeners and the test hooks. */
  const clip = useRef<{ paste: (from: clipboard.Carried, at: Point | null) => void; drop: (files: File[], clientX: number, clientY: number) => void }>({ paste: () => undefined, drop: () => undefined })

  const store = useMemo<ViewportStore>(
    () => ({
      get: () => {
        const v = viewport.current
        return v.width ? v : null
      },
      subscribe: (listener) => {
        listeners.current.add(listener)
        return () => listeners.current.delete(listener)
      },
    }),
    [],
  )

  /** Applies the view: one transform on the world group, culling in the layer; no React render. */
  const applyView = useCallback((next: Partial<Viewport>) => {
    const v = { ...viewport.current, ...next }
    v.zoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, v.zoom))
    const zoomChanged = v.zoom !== viewport.current.zoom
    viewport.current = { ...v }
    world.current?.setAttribute('transform', `scale(${v.zoom}) translate(${-v.x} ${-v.y})`)
    layerRef.current?.render(viewport.current)
    for (const listener of listeners.current) listener()
    if (zoomChanged) {
      setZoom(v.zoom)
      latest.current.onZoom(v.zoom)
    }
  }, [])

  const toWorld = useCallback((clientX: number, clientY: number): Point => {
    const box = container.current?.getBoundingClientRect()
    const v = viewport.current
    return { x: v.x + (clientX - (box?.left ?? 0)) / v.zoom, y: v.y + (clientY - (box?.top ?? 0)) / v.zoom }
  }, [])

  const toScreen = useCallback((p: Point): Point => {
    const v = viewport.current
    return { x: (p.x - v.x) * v.zoom, y: (p.y - v.y) * v.zoom }
  }, [])

  const contentBox = useCallback(() => {
    const pageView = sync.view.page(layerRef.current?.pageId ?? sync.view.pages[0]?.id)
    let out: Rect | null = null
    for (const id of pageView?.elements ?? []) out = union(out, layerRef.current?.worldBox(id) ?? null)
    return out
  }, [sync])

  const show = useCallback(
    (area: Rect) => {
      const v = viewport.current
      if (!v.width || !v.height) return
      const w = Math.max(1, area.x1 - area.x0)
      const h = Math.max(1, area.y1 - area.y0)
      const z = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.min((v.width - 80) / w, (v.height - 80) / h, 1)))
      applyView({ zoom: z, x: area.x0 + w / 2 - v.width / z / 2, y: area.y0 + h / 2 - v.height / z / 2 })
    },
    [applyView],
  )

  const zoomAt = useCallback(
    (factor: number, at?: Point) => {
      const v = viewport.current
      const screen = at ?? { x: v.width / 2, y: v.height / 2 }
      const before = { x: v.x + screen.x / v.zoom, y: v.y + screen.y / v.zoom }
      const z = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, v.zoom * factor))
      applyView({ zoom: z, x: before.x - screen.x / z, y: before.y - screen.y / z })
    },
    [applyView],
  )

  useImperativeHandle(
    props.handle,
    () => ({
      view: () => ({ ...viewport.current }),
      fit: () => {
        const box = contentBox()
        if (box) show(box)
      },
      show: (area) => show(area),
      zoomTo: (z) => zoomAt(z / viewport.current.zoom),
      zoomBy: (factor) => zoomAt(factor),
      panTo: (x, y) => applyView({ x, y }),
      retryRender: async () => true,
    }),
    [applyView, contentBox, show, zoomAt],
  )

  // The layer over the page replica; the engine's changes redraw what they touched.
  useEffect(() => {
    const root = elementsRoot.current
    if (!root) return undefined
    const layer = new ElementLayer(root, sync)
    layerRef.current = layer
    layer.showPage(latest.current.page)
    const box = container.current
    const size = () => {
      if (!box) return
      applyView({ width: box.clientWidth, height: box.clientHeight })
    }
    size()
    const first = contentBox()
    if (first) show(first)
    const observer = new ResizeObserver(size)
    if (box) observer.observe(box)
    const stop = sync.subscribe((changed) => {
      layer.refresh(changed)
      // A selected element another writer deleted leaves the selection.
      const kept = latest.current.selection.filter((id) => sync.view.elements.has(id))
      if (kept.length !== latest.current.selection.length) latest.current.onSelection(kept)
      setTick((value) => value + 1)
    })
    ;(window as BoardWindow).kitBoard = {
      viewport: () => ({ ...viewport.current }),
      setViewport: (next) => applyView(next),
      mounts: () => layer.mounts,
      resetMounts: () => {
        layer.mounts = 0
      },
      fragmentRequests: () => sync.fragments.requests,
      shown: () => [...root.children].map((node) => (node as SVGGElement).dataset.id ?? ''),
      worldBox: (id) => layer.worldBox(id),
      endPoints: (id) => {
        const element = sync.view.elements.get(id)
        return element?.kind === 'connector' ? sync.view.route(element) : []
      },
      selection: () => [...latest.current.selection],
      parent: (id) => sync.view.elements.get(id)?.parent ?? null,
      trace: () => trace.current.slice(-12),
      textMetrics: (request) => sync.textMetrics(request),
      move: async (ids, dx, dy) => {
        const started = performance.now()
        await commitRef.current('element.move', { ids, delta: [dx, dy] })
        return performance.now() - started
      },
      paste: (files, text) => clip.current.paste({ files, text: text ?? null, stale: false }, null),
      drop: (files, clientX, clientY) => clip.current.drop(files, clientX, clientY),
    }
    return () => {
      stop()
      observer.disconnect()
      layerRef.current = null
      delete (window as BoardWindow).kitBoard
    }
  }, [applyView, contentBox, show, sync])

  // Another page: the selection does not come along, and each page keeps its own view (where it
  // was left; a page not shown yet shows its content, and an empty one opens where the view is).
  const pageViews = useRef(new Map<string, Pick<Viewport, 'x' | 'y' | 'zoom'>>())
  useEffect(() => {
    const layer = layerRef.current
    if (layer && layer.pageId !== page) {
      const v = viewport.current
      if (layer.pageId) pageViews.current.set(layer.pageId, { x: v.x, y: v.y, zoom: v.zoom })
      layer.showPage(page)
      onSelection([])
      const kept = page ? pageViews.current.get(page) : undefined
      if (kept) applyView(kept)
      else {
        const box = contentBox()
        if (box) show(box)
      }
    }
  }, [applyView, contentBox, onSelection, page, show])

  /** Runs one engine command, then shows its change; `done` drops the gesture's preview in the same frame. */
  const commit = useCallback(
    async (command: string, params: Record<string, unknown>, done?: () => void) => {
      let result: string | null = null
      const started = performance.now()
      try {
        result = await latest.current.run(command, params)
        note(`${command} ran in ${Math.round(performance.now() - started)} ms`)
      } catch (error) {
        done?.()
        console.warn(`[board] ${command} was refused: ${errorText(error)}`)
        return null
      }
      let answer: Record<string, unknown> | null = null
      try {
        answer = result ? (JSON.parse(result) as Record<string, unknown>) : null
      } catch {
        answer = null
      }
      await sync.pull(touchedBy(answer))
      note(`${command} shown after ${Math.round(performance.now() - started)} ms`)
      done?.()
      return answer
    },
    [sync],
  )

  const commitRef = useRef(commit)
  commitRef.current = commit

  const clearPreview = useCallback(() => {
    layerRef.current?.setOverrides(new Map())
    layerRef.current?.setHidden([])
    setDraft(null)
    setGuides([])
    setDrops([])
  }, [])

  /** The frame a new element at `p` goes into (the smallest one under it), if any. */
  const frameUnder = (p: Point) => {
    const view = sync.view
    const shownPage = view.page(layerRef.current?.pageId ?? view.pages[0]?.id)
    return shownPage ? (frameAt(view.framesOn(shownPage, new Set()), p)?.id ?? null) : null
  }

  const elementAt = (target: EventTarget | null): string | null => {
    const node = (target as Element | null)?.closest?.('[data-id]')
    return node && elementsRoot.current?.contains(node) ? node.getAttribute('data-id') : null
  }

  /** The topmost element painted at a client point (a captured pointer's events all target the canvas). */
  const elementUnder = (clientX: number, clientY: number, excluding: string | null): string | null => {
    for (const node of document.elementsFromPoint(clientX, clientY)) {
      const id = elementAt(node)
      if (id && id !== excluding) return id
    }
    return null
  }

  /** A connector end over `p`: on the element under the pointer (not `excluding`), else free. */
  const endAt = (p: Point, client: { clientX: number; clientY: number }, excluding: string | null): EndTarget => {
    const hit = elementUnder(client.clientX, client.clientY, excluding)
    const element = hit && hit !== excluding ? sync.view.elements.get(hit) : null
    const box = element && isBoxed(element.kind) ? sync.view.rect(element) : null
    if (element && box) {
      // Near an edge the end goes where the pointer is; further in, the engine faces it toward the
      // other end (no anchor). A shape with an outline of its own (a diamond, an ellipse) is pinned
      // on the ray from its centre through the pointer, so the end meets the outline under the
      // pointer (design note 1.4); a box keeps the nearest edge.
      const outline = sync.view.outlineOf(element)
      const edge = nearestEdgeAnchor(box, element.rotation, p)
      const anchor = (outline && rayAnchor(box, element.rotation, p)) || edge.anchor
      const point = outline ? sync.view.boundPoint(element, box, anchor) : edge.point
      const near = distance(point, p) <= 16 / viewport.current.zoom
      return near ? { element: element.id, anchor, point } : { element: element.id, anchor: null, point: center(box) }
    }
    return { element: null, point: p }
  }

  const endParams = (end: EndTarget) => (end.element ? (end.anchor ? { element: end.element, anchor: end.anchor } : { element: end.element }) : { point: [end.point.x, end.point.y] })

  /** The live route of a connector whose bound targets are being moved by `delta`. */
  const liveRoute = (id: string, moving: ReadonlySet<string>, delta: Point): Point[] => {
    const element = sync.view.elements.get(id)
    if (!element || element.kind !== 'connector') return []
    const endOf = (end: BoardElement['start']) => {
      if (!end) return null
      const target = end.binding ? sync.view.elements.get(end.binding.target) : null
      const box = target ? sync.view.rect(target) : null
      if (end.binding && target && box) {
        const moved = moving.has(target.id) ? offset(box, delta.x, delta.y) : box
        return sync.view.boundPoint(target, moved, end.binding.anchor)
      }
      return moving.has(id) ? { x: end.point.x + delta.x, y: end.point.y + delta.y } : end.point
    }
    const shift = moving.has(id) ? delta : { x: 0, y: 0 }
    return [endOf(element.start), ...element.waypoints.map((p) => ({ x: p.x + shift.x, y: p.y + shift.y })), endOf(element.end)].filter((p): p is Point => p != null)
  }

  const openText = useCallback(
    (id: string | null, at?: Point) => {
      if (latest.current.readOnly) return
      const v = viewport.current
      if (!id) {
        if (!at) return
        const s = toScreen(at)
        setEditing({ id: null, kind: 'new-text', value: '', left: s.x, top: s.y, width: Math.max(160, 240 * v.zoom), height: 34, fontSize: Math.max(12, 20 * v.zoom), align: 'left' })
        return
      }
      const element = sync.view.elements.get(id)
      if (!element || element.locked) return
      if (element.kind === 'connector') {
        const route = sync.view.route(element)
        const middle = route.length ? route[Math.floor(route.length / 2) - (route.length % 2 ? 0 : 1)] : { x: 0, y: 0 }
        const s = toScreen(middle)
        setEditing({ id, kind: 'connector', value: element.text, left: s.x - 90, top: s.y - 17, width: 180, height: 34, fontSize: Math.max(12, 14 * v.zoom), align: 'center' })
        return
      }
      const box = sync.view.rect(element)
      if (!box || !['shape', 'text', 'frame', 'card'].includes(element.kind)) return
      const s = toScreen({ x: box.x0, y: box.y0 })
      const w = (box.x1 - box.x0) * v.zoom
      const h = (box.y1 - box.y0) * v.zoom
      const size = (element.fontSize ?? (element.kind === 'text' ? 20 : 16)) * v.zoom
      if (element.kind === 'shape') setEditing({ id, kind: 'shape', value: element.text, left: s.x + 8, top: s.y + h / 2 - size, width: Math.max(80, w - 16), height: size * 2, fontSize: size, align: 'center' })
      else if (element.kind === 'frame') setEditing({ id, kind: 'frame', value: element.text, left: s.x, top: s.y - 30, width: Math.max(160, w), height: 28, fontSize: 14 * v.zoom, align: 'left' })
      else if (element.kind === 'card') setEditing({ id, kind: 'card', value: element.text, left: s.x, top: s.y + h + 4, width: Math.max(160, w), height: 28, fontSize: 13 * v.zoom, align: 'center' })
      else setEditing({ id, kind: 'text', value: element.text, left: s.x, top: s.y, width: Math.max(160, w + 24), height: Math.max(34, h), fontSize: size, align: 'left' })
    },
    [sync, toScreen],
  )

  const commitText = useCallback(
    (value: string) => {
      const current = editing
      setEditing(null)
      // The keys go back to the canvas (Undo, Delete) once the box closes.
      container.current?.focus({ preventScroll: true })
      if (!current) return
      if (current.id == null) {
        if (!value.trim()) return
        const at = toWorldFromScreen(current.left, current.top, viewport.current)
        const parent = frameUnder(at)
        void commit('element.create', { elements: [{ kind: 'text', position: [at.x, at.y], text: value, ...styleFor('text', latest.current.drawStyle), ...(parent ? { parent } : {}) }] }).then((answer) => {
          const created = (answer?.created as string[] | undefined) ?? []
          if (created.length) latest.current.onSelection(created)
        })
        return
      }
      const element = sync.view.elements.get(current.id)
      if (!element || element.text === value) return
      if (current.kind === 'connector') void commit('connector.setLabel', { id: current.id, text: value })
      else void commit('element.setText', { id: current.id, text: value })
    },
    [commit, editing, sync],
  )

  /** Adds a branch to `node` (a child, or with `sibling` a sibling), selects it and opens its text. */
  const growMap = useCallback(
    (node: string, sibling: boolean, side?: 'left' | 'right') => {
      const params: Record<string, unknown> = { node }
      if (side) params.side = side
      void commit(sibling ? 'mindmap.addSibling' : 'mindmap.addChild', params).then((answer) => {
        const made = typeof answer?.node === 'string' ? answer.node : null
        if (!made) return
        latest.current.onSelection([made])
        window.setTimeout(() => openText(made), 0)
      })
    },
    [commit, openText],
  )

  /** Selects what a command made: its outermost elements (a frame's or a group's copy, not what is inside it). */
  const selectMade = (created: string[]) => {
    const made = new Set(created)
    const roots = created.filter((id) => !made.has(sync.view.elements.get(id)?.parent ?? ''))
    if (roots.length) latest.current.onSelection(roots)
  }

  /** The board point under the pointer, else the middle of the view. */
  const pointerWorld = (): Point => {
    const at = pointer.current
    if (at) return toWorld(at.x, at.y)
    const v = viewport.current
    return { x: v.x + v.width / v.zoom / 2, y: v.y + v.height / v.zoom / 2 }
  }

  /** The page commands that place something name: the one shown. */
  const onPage = (): Record<string, unknown> => (latest.current.page ? { page: latest.current.page } : {})

  // The clipboard (design note 4.3): every change is one engine command; a copy changes nothing.
  const copyIds = (ids: readonly string[]) => {
    void clipboard.copy(latest.current.query, ids)
  }
  const cutIds = (ids: readonly string[]) => {
    void clipboard.copy(latest.current.query, ids).then((copied) => {
      if (!copied || latest.current.readOnly) return
      latest.current.onSelection([])
      void commit('element.delete', { ids: [...ids] })
    })
  }
  /** Pastes the board payload `from` holds (or the copy kept here) with its top-left at `at`; selects the copies. */
  const pasteBoard = async (from: clipboard.Carried | null, at: Point) => {
    const payload = await clipboard.payloadFor(from)
    if (!payload || latest.current.readOnly) return
    const place = clipboard.pastePlace(at)
    selectMade(createdOf(await commit('element.paste', { ...onPage(), payload, at: [place.x, place.y] })))
  }
  /** One image.insert per picture, centred on `at` and each further one 24 units on; selects them. */
  const placePictures = async (files: File[], at: Point, named: boolean) => {
    const made: string[] = []
    for (const [index, file] of files.entries()) {
      const mime = clipboard.pictureMime(file)
      if (!mime) continue
      try {
        const bytes = new Uint8Array(await file.arrayBuffer())
        const centre = { x: at.x + index * clipboard.PICTURE_STEP, y: at.y + index * clipboard.PICTURE_STEP }
        const answer = await latest.current.run('image.insert', { ...onPage(), ...(await clipboard.pictureParams(bytes, mime, named ? file.name : '', centre)) })
        made.push(...createdOf(answer ? (JSON.parse(answer) as Record<string, unknown>) : null))
      } catch (error) {
        // Too large, or not a picture after all: the engine's reason, as the picture tool shows it.
        toast('error', latest.current.t('pictureFailed'), errorText(error))
      }
    }
    if (!made.length) return
    await sync.pull(made)
    latest.current.onSelection(made)
  }
  /** A paste's content: pictures win over board text; `at` is where the key was pressed (else the pointer). */
  const pasteContent = (from: clipboard.Carried, at: Point | null) => {
    if (latest.current.readOnly) return
    const place = at ?? pointerWorld()
    if (from.files.length) void placePictures(from.files, place, false)
    else void pasteBoard(from, place)
  }
  clip.current = {
    paste: pasteContent,
    drop: (files, clientX, clientY) => {
      if (latest.current.readOnly) return
      const pictures = files.filter((file) => clipboard.pictureMime(file) != null)
      if (pictures.length) void placePictures(pictures, toWorld(clientX, clientY), true)
    },
  }

  // The browser's paste, and pictures dropped from outside. A paste event (Cmd/Ctrl+V on the focused
  // canvas, or the host's Edit menu) carries the system clipboard without asking for permission;
  // where a browser sends none, the key's own fallback pastes the copy kept here.
  useEffect(() => {
    const box = container.current
    if (!box) return undefined
    const onPaste = (event: ClipboardEvent) => {
      if (latest.current.readOnly || editingNow.current || typingIn(event.target)) return
      const from = clipboard.carried(event.clipboardData)
      const key = pasteKey.current
      pasteKey.current = null
      event.preventDefault()
      if (key?.handled) return
      if (key) key.handled = true
      clip.current.paste(from, key?.at ?? null)
    }
    const onDragOver = (event: DragEvent) => {
      if (latest.current.readOnly || !Array.from(event.dataTransfer?.types ?? []).includes('Files')) return
      event.preventDefault()
      if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy'
    }
    const onDrop = (event: DragEvent) => {
      if (latest.current.readOnly) return
      const from = clipboard.carried(event.dataTransfer)
      if (!from.files.length) return
      event.preventDefault()
      clip.current.drop(from.files, event.clientX, event.clientY)
    }
    box.addEventListener('paste', onPaste)
    box.addEventListener('dragover', onDragOver)
    box.addEventListener('drop', onDrop)
    return () => {
      box.removeEventListener('paste', onPaste)
      box.removeEventListener('dragover', onDragOver)
      box.removeEventListener('drop', onDrop)
    }
  }, [])

  /** Cmd/Ctrl+V: the browser's paste event follows the key (and carries pictures); without one, the copy kept here. */
  const pasteFromKey = () => {
    const key = { at: pointerWorld(), handled: false }
    pasteKey.current = key
    window.setTimeout(() => {
      if (key.handled) return
      key.handled = true
      void pasteBoard(null, key.at)
      // A paste event later than this is the same key's: it is dropped, then the key is forgotten.
      window.setTimeout(() => {
        if (pasteKey.current === key) pasteKey.current = null
      }, 1000)
    }, 50)
  }

  const selectAll = () => {
    const pageView = sync.view.page(layerRef.current?.pageId ?? sync.view.pages[0]?.id)
    latest.current.onSelection([...new Set((pageView?.elements ?? []).map((id) => sync.view.selectable(id)))])
  }
  const duplicateIds = (ids: readonly string[], offset?: Point, done?: () => void) => {
    void commit('element.duplicate', { ids: [...ids], ...(offset ? { offset: [offset.x, offset.y] } : {}) }, done).then((answer) => selectMade(createdOf(answer)))
  }
  const deleteIds = (ids: readonly string[]) => {
    latest.current.onSelection([])
    void commit('element.delete', { ids: [...ids] })
  }
  const groupIds = (ids: readonly string[]) => {
    void commit('element.group', { ids: [...ids] }).then((answer) => {
      if (typeof answer?.group === 'string') latest.current.onSelection([answer.group])
    })
  }
  const ungroupIds = (ids: readonly string[]) => {
    const groups = ids.filter((id) => sync.view.elements.get(id)?.kind === 'group')
    if (groups.length) void commit('element.ungroup', { groupIds: groups })
  }

  /** A row of the canvas menu, on the elements the menu opened on. */
  const onMenu = (action: CanvasAction) => {
    const open = menu
    setMenu(null)
    if (!open) return
    const ids = open.ids ?? []
    switch (action) {
      case 'copy':
        copyIds(ids)
        break
      case 'cut':
        cutIds(ids)
        break
      case 'paste':
        void clipboard.systemText().then((from) => pasteBoard(from, open.world))
        break
      case 'duplicate':
        duplicateIds(ids)
        break
      case 'front':
      case 'forward':
      case 'backward':
      case 'back':
        void commit('element.reorder', { ids, placement: action })
        break
      case 'group':
        groupIds(ids)
        break
      case 'ungroup':
        ungroupIds(ids)
        break
      case 'lock':
      case 'unlock':
        void commit('element.lock', { ids, locked: action === 'lock' })
        break
      case 'delete':
        deleteIds(ids)
        break
      case 'select-all':
        selectAll()
        break
    }
  }

  const onContextMenu = (event: React.MouseEvent<SVGSVGElement>) => {
    event.preventDefault()
    if (editing || gesture.current) return
    const at = { x: event.clientX, y: event.clientY }
    const world = toWorld(event.clientX, event.clientY)
    const hit = elementUnder(event.clientX, event.clientY, null)
    if (hit) {
      // A right-click on an element outside the selection selects it first.
      const id = sync.view.selectable(hit)
      const ids = selection.includes(id) ? [...selection] : [id]
      if (!selection.includes(id)) onSelection(ids)
      setMenu({ at, world, ids })
    } else if (!readOnly) setMenu({ at, world, ids: null })
  }

  // Pointer handling.
  const onPointerDown = (event: React.PointerEvent<SVGSVGElement>) => {
    note(`down ${tool}${editing ? ' editing' : ''} ${event.button}`)
    pointer.current = { x: event.clientX, y: event.clientY }
    if (editing) return
    // The canvas takes the keys (tool letters, Delete, Undo) once it is used.
    container.current?.focus({ preventScroll: true })
    const p = toWorld(event.clientX, event.clientY)
    const screen = { x: event.clientX, y: event.clientY }
    const handle = (event.target as Element).closest?.('[data-handle]')?.getAttribute('data-handle') ?? null
    const svg = event.currentTarget
    const capture = () => svg.setPointerCapture?.(event.pointerId)
    if (event.button === 1 || tool === 'hand' || space.current) {
      gesture.current = { type: 'pan', start: screen, from: { ...viewport.current } }
      capture()
      return
    }
    if (event.button !== 0) return
    const v = sync.view
    if (handle && !readOnly) {
      const [kind, value] = handle.split(':')
      if (kind === 'resize') {
        const element = selection.length === 1 ? v.elements.get(selection[0]) : null
        const box = element && isBoxed(element.kind) ? v.rect(element) : null
        if (element && box) {
          gesture.current = { type: 'resize', id: element.id, handle: value as Handle, box, rotation: element.rotation, current: box }
          capture()
          return
        }
        // A selection (or a stroke or group) scales as a whole, by the bounds the engine maps from.
        const roots = selection.filter((id) => v.elements.has(id))
        const bounds = roots.reduce<Rect | null>((all, id) => union(all, v.bounds(v.elements.get(id)!)), null)
        if (roots.length && bounds) {
          gesture.current = { type: 'scale', roots, moving: [...new Set(roots.flatMap((id) => v.withDescendants(id)))], handle: value as Handle, box: bounds, current: bounds }
          capture()
        }
        return
      }
      if (kind === 'rotate') {
        const box = selectionBox(sync, layerRef.current, selection)
        if (box) {
          const origin = center(box)
          gesture.current = { type: 'rotate', ids: [...selection], origin, start: Math.atan2(p.y - origin.y, p.x - origin.x), degrees: 0 }
          capture()
        }
        return
      }
      if (kind === 'terminal') {
        const id = selection[0]
        gesture.current = { type: 'terminal', connector: id, terminal: value as 'start' | 'end', current: { element: null, point: p } }
        layerRef.current?.setHidden([id])
        capture()
        return
      }
      if (kind === 'connect') {
        const element = v.elements.get(selection[0])
        const box = element ? v.rect(element) : null
        const side = CONNECTION_HANDLES.find((item) => item.side === value)
        if (element && box && side) {
          const from: EndTarget = { element: element.id, anchor: side.anchor, point: v.boundPoint(element, box, side.anchor) }
          gesture.current = { type: 'connect', from, current: { element: null, point: p }, elbow: true }
          capture()
        }
        return
      }
    }
    const hit = elementAt(event.target)
    if (tool === 'select') {
      if (hit) {
        const id = v.selectable(hit)
        const additive = event.shiftKey || event.metaKey || event.ctrlKey
        let next: string[]
        if (additive) next = selection.includes(id) ? selection.filter((item) => item !== id) : [...selection, id]
        else next = selection.includes(id) ? [...selection] : [id]
        onSelection(next)
        if (readOnly || (additive && !next.includes(id))) return
        const roots = next.filter((item) => !v.elements.get(item)?.locked)
        if (!roots.length) return
        const moving = new Set(roots.flatMap((root) => v.withDescendants(root)))
        const base = selectionBox(sync, layerRef.current, [...moving])
        if (!base) return
        const shownPage = v.page(layerRef.current?.pageId ?? v.pages[0]?.id)
        // Where each root's centre is, for the frame a drop lands it in (the engine's rule).
        const frames = shownPage ? v.framesOn(shownPage, moving) : []
        const centers = frames.length ? roots.flatMap((id) => {
          const box = v.bounds(v.elements.get(id)!)
          return box ? [center(box)] : []
        }) : []
        gesture.current = { type: 'move', start: p, roots, moving, base, connectors: v.connectorsOf(moving).filter((item) => !moving.has(item)), delta: { x: 0, y: 0 }, moved: false, frames, centers, altAtStart: event.altKey }
        capture()
        return
      }
      const additive = event.shiftKey || event.metaKey || event.ctrlKey
      if (!additive) onSelection([])
      gesture.current = { type: 'brush', start: p, current: p, additive, initial: additive ? [...selection] : [] }
      capture()
      return
    }
    if (readOnly) return
    if (tool === 'text') {
      if (hit && v.elements.get(hit)?.kind === 'text') openText(hit)
      else openText(null, p)
      if (!latest.current.readOnly) onTool('select')
      return
    }
    if (tool === 'mindmap') {
      // A new mind map: its root where the pointer is, ready to type into.
      const root: Record<string, unknown> = { kind: 'shape', shapeStyle: 'mind-node', mindMapDirection: 'right', position: [p.x - 80, p.y - 30], size: [160, 60] }
      const parent = frameUnder(p)
      if (parent) root.parent = parent
      void commit('element.create', { elements: [root] }).then((answer) => {
        const created = (answer?.created as string[] | undefined) ?? []
        onTool('select')
        if (!created.length) return
        onSelection(created)
        window.setTimeout(() => openText(created[0]), 0)
      })
      return
    }
    if (tool === 'connector') {
      gesture.current = { type: 'connect', from: endAt(p, event, null), current: { element: null, point: p }, elbow: false }
      capture()
      return
    }
    if (tool === 'pen' || tool === 'highlight') {
      gesture.current = { type: 'pen', points: [p], highlight: tool === 'highlight' }
      capture()
      return
    }
    if (tool === 'eraser') {
      const erase: Gesture = { type: 'erase', ids: new Set() }
      gesture.current = erase
      eraseAt(erase, event.clientX, event.clientY)
      capture()
      return
    }
    gesture.current = { type: 'draw', tool, start: p, current: p }
    capture()
  }

  const onPointerMove = (event: React.PointerEvent<SVGSVGElement>) => {
    pointer.current = { x: event.clientX, y: event.clientY }
    const g = gesture.current
    if (!g) return
    const p = toWorld(event.clientX, event.clientY)
    const z = viewport.current.zoom
    switch (g.type) {
      case 'pan':
        applyView({ x: g.from.x - (event.clientX - g.start.x) / z, y: g.from.y - (event.clientY - g.start.y) / z })
        break
      case 'move': {
        let delta = { x: p.x - g.start.x, y: p.y - g.start.y }
        g.moved = g.moved || Math.abs(delta.x) * z > 2 || Math.abs(delta.y) * z > 2
        if (!g.moved) break
        let snapGuides: SnapGuide[] = []
        if (!event.altKey) {
          const moved = offset(g.base, delta.x, delta.y)
          const margin = Math.max(128, 512 / z)
          const near = layerRef.current?.query({ x0: moved.x0 - margin, y0: moved.y0 - margin, x1: moved.x1 + margin, y1: moved.y1 + margin }) ?? []
          const targets = near.filter((id) => !g.moving.has(id) && ['shape', 'text', 'image', 'card', 'frame'].includes(sync.view.elements.get(id)?.kind ?? '')).flatMap((id) => layerRef.current?.worldBox(id) ?? [])
          const snap = translationSnap(moved, targets, 6 / z)
          delta = { x: delta.x + snap.adjustment.x, y: delta.y + snap.adjustment.y }
          snapGuides = snap.guides
        }
        g.delta = delta
        if (!g.altAtStart && event.altKey) {
          // The originals stay where they are; a dashed box shows where the copies land.
          layerRef.current?.setOverrides(new Map())
          layerRef.current?.setHidden([])
          setDraft({ type: 'rect', box: offset(g.base, delta.x, delta.y), dashed: true })
          setGuides(snapGuides)
          setDrops((now) => (now.length ? [] : now))
          break
        }
        const overrides = new Map<string, Override>()
        for (const id of g.moving) overrides.set(id, { dx: delta.x, dy: delta.y })
        layerRef.current?.setOverrides(overrides)
        layerRef.current?.setHidden(g.connectors)
        setDraft({ type: 'lines', lines: g.connectors.map((id) => liveRoute(id, g.moving, delta)), target: null })
        setGuides(snapGuides)
        const into = [...new Set(g.centers.flatMap((c) => frameAt(g.frames, { x: c.x + delta.x, y: c.y + delta.y })?.id ?? []))]
        setDrops((now) => (now.length === into.length && now.every((id, at) => id === into[at]) ? now : into))
        break
      }
      case 'resize': {
        // A rotated box resizes in its own frame (no snapping to the page's axes).
        const snapped = event.altKey || g.rotation ? { point: p, guides: [] } : resizeSnap(g.handle, p, g.box, (layerRef.current?.query(g.box) ?? []).filter((id) => id !== g.id).flatMap((id) => layerRef.current?.worldBox(id) ?? []), 6 / z)
        g.current = resizedRotated(g.box, g.rotation, g.handle, snapped.point, event.shiftKey)
        const sx = (g.current.x1 - g.current.x0) / Math.max(1e-6, g.box.x1 - g.box.x0)
        const sy = (g.current.y1 - g.current.y0) / Math.max(1e-6, g.box.y1 - g.box.y0)
        layerRef.current?.setOverrides(new Map([[g.id, { dx: g.current.x0 - g.box.x0, dy: g.current.y0 - g.box.y0, sx, sy }]]))
        const connectors = sync.view.connectorsOf(new Set([g.id]))
        layerRef.current?.setHidden(connectors)
        if (!g.rotation) setDraft({ type: 'rect', box: g.current, dashed: false })
        setGuides(snapped.guides)
        break
      }
      case 'scale': {
        g.current = resized(g.box, g.handle, p, event.shiftKey)
        layerRef.current?.setOverrides(scaleOverrides(sync.view, g.moving, g.box, g.current))
        // Connectors from outside the selection are drawn again once their targets have moved.
        const inside = new Set(g.moving)
        layerRef.current?.setHidden(sync.view.connectorsOf(inside).filter((id) => !inside.has(id)))
        setDraft({ type: 'rect', box: g.current, dashed: true })
        break
      }
      case 'rotate': {
        let degrees = ((Math.atan2(p.y - g.origin.y, p.x - g.origin.x) - g.start) * 180) / Math.PI
        if (event.shiftKey) degrees = Math.round(degrees / 15) * 15
        g.degrees = degrees
        const overrides = new Map<string, Override>()
        for (const id of g.ids) overrides.set(id, { dx: 0, dy: 0, rotate: degrees })
        layerRef.current?.setOverrides(overrides)
        layerRef.current?.setHidden(sync.view.connectorsOf(new Set(g.ids)))
        break
      }
      case 'brush':
        g.current = p
        setDraft({ type: 'rect', box: rect(g.start.x, g.start.y, p.x, p.y), dashed: true })
        break
      case 'draw':
        g.current = p
        setDraft({ type: 'rect', box: rect(g.start.x, g.start.y, p.x, p.y), dashed: false })
        break
      case 'connect':
        g.current = endAt(p, event, g.from.element)
        setDraft({ type: 'lines', lines: [elbowOr(g.from.point, g.current.point, g.elbow)], target: g.current.element })
        break
      case 'terminal': {
        const connector = sync.view.elements.get(g.connector)
        g.current = endAt(p, event, g.connector)
        if (connector) {
          const route = sync.view.route(connector)
          const other = g.terminal === 'start' ? route[route.length - 1] : route[0]
          setDraft({ type: 'lines', lines: [g.terminal === 'start' ? [g.current.point, other] : [other, g.current.point]], target: g.current.element })
        }
        break
      }
      case 'pen': {
        const last = g.points[g.points.length - 1]
        if (!last || distance(last, p) >= 1.5 / z) {
          g.points.push(p)
          setDraft({ type: 'pen', points: [...g.points], highlight: g.highlight })
        }
        break
      }
      case 'erase':
        // Every coalesced point, so a quick stroke does not skip a thin line.
        for (const sample of event.nativeEvent.getCoalescedEvents?.() ?? [event.nativeEvent]) eraseAt(g, sample.clientX, sample.clientY)
        break
    }
  }

  /** The eraser passing a client point: the unlocked element painted there goes (hidden until the delete arrives). */
  const eraseAt = (g: Extract<Gesture, { type: 'erase' }>, clientX: number, clientY: number) => {
    const hit = elementUnder(clientX, clientY, null)
    const element = hit ? sync.view.elements.get(hit) : null
    if (!element || element.locked || g.ids.has(element.id)) return
    g.ids.add(element.id)
    layerRef.current?.setHidden([...g.ids])
  }

  const onPointerUp = (event: React.PointerEvent<SVGSVGElement>) => {
    const g = gesture.current
    note(`${event.type} ${g?.type ?? 'none'}${g?.type === 'pen' ? ` ${g.points.length}` : ''}`)
    gesture.current = null
    if (!g) return
    const p = toWorld(event.clientX, event.clientY)
    const z = viewport.current.zoom
    switch (g.type) {
      case 'pan':
        break
      case 'move':
        setGuides([])
        setDrops([])
        if (!g.moved) {
          clearPreview()
          break
        }
        if (!g.altAtStart && event.altKey) {
          // Copies at the drop, the originals untouched: one element.duplicate, one undo step.
          duplicateIds(g.roots, g.delta, clearPreview)
          break
        }
        // A drop into a frame makes the frame the parent, a drop out of one frees it: same command.
        void commit('element.move', { ids: g.roots, delta: [g.delta.x, g.delta.y], reparent: true }, clearPreview)
        break
      case 'resize': {
        const box = g.current
        if (box.x1 - box.x0 < 1 || box.y1 - box.y0 < 1) {
          clearPreview()
          break
        }
        void commit('element.resize', { id: g.id, box: [box.x0, box.y0, box.x1, box.y1] }, clearPreview)
        break
      }
      case 'scale': {
        const box = g.current
        const same = Math.abs(box.x0 - g.box.x0) + Math.abs(box.y0 - g.box.y0) + Math.abs(box.x1 - g.box.x1) + Math.abs(box.y1 - g.box.y1) < 0.5
        if (same) {
          clearPreview()
          break
        }
        void commit('element.resize', { ids: g.roots, box: [box.x0, box.y0, box.x1, box.y1] }, clearPreview)
        break
      }
      case 'rotate':
        if (Math.abs(g.degrees) < 0.5) {
          clearPreview()
          break
        }
        void commit('element.rotate', { ids: g.ids, degrees: g.degrees, about: g.ids.length > 1 ? 'selection' : 'each' }, clearPreview)
        break
      case 'brush': {
        setDraft(null)
        const box = rect(g.start.x, g.start.y, p.x, p.y)
        if ((box.x1 - box.x0) * z < 3 && (box.y1 - box.y0) * z < 3) {
          // A click on a frame's empty inside selects the frame (a drag there still draws a marquee).
          const shownPage = sync.view.page(layerRef.current?.pageId ?? sync.view.pages[0]?.id)
          const frame = shownPage && !g.additive ? frameAt(sync.view.framesOn(shownPage, sync.view.hidden), p) : null
          if (frame) onSelection([frame.id])
          break
        }
        const hits = (layerRef.current?.query(box) ?? []).filter((id) => !sync.view.hidden.has(id)).map((id) => sync.view.selectable(id))
        onSelection([...new Set([...g.initial, ...hits])])
        break
      }
      case 'draw': {
        setDraft(null)
        let box = rect(g.start.x, g.start.y, p.x, p.y)
        const small = (box.x1 - box.x0) * z < MIN_DRAW || (box.y1 - box.y0) * z < MIN_DRAW
        const size = g.tool === 'note' ? [180, 140] : g.tool === 'frame' ? [640, 400] : [160, 100]
        if (small) box = { x0: g.start.x, y0: g.start.y, x1: g.start.x + size[0], y1: g.start.y + size[1] }
        const done = (answer: Record<string, unknown> | null) => {
          const created = (answer?.created as string[] | undefined) ?? (answer?.frame ? [answer.frame as string] : [])
          if (created.length) onSelection(created)
          onTool('select')
          if (g.tool === 'note' && created[0]) window.setTimeout(() => openText(created[0]), 0)
        }
        if (g.tool === 'frame') void commit('frame.create', { box: [box.x0, box.y0, box.x1, box.y1] }).then(done)
        else {
          const element: Record<string, unknown> = { kind: 'shape', position: [box.x0, box.y0], size: [box.x1 - box.x0, box.y1 - box.y0], ...styleFor('shape', latest.current.drawStyle) }
          const parent = frameUnder(center(box))
          if (parent) element.parent = parent
          if (g.tool === 'note') element.shapeStyle = 'sticky'
          else element.shapeType = shapeType
          void commit('element.create', { elements: [element] }).then(done)
        }
        break
      }
      case 'connect': {
        setDraft(null)
        const end = g.current
        if (distance(g.from.point, end.point) * z < MIN_DRAW) break
        const params: Record<string, unknown> = { from: endParams(g.from), to: endParams(end) }
        const style = styleFor('connector', latest.current.drawStyle)
        // A connector drawn from a shape's handle is an elbow whatever the default route.
        if (g.elbow) {
          delete style.connectorType
          params.type = 'elbow'
        }
        if (Object.keys(style).length) params.style = style
        void commit('connector.create', params).then((answer) => {
          const created = (answer?.created as string[] | undefined) ?? []
          if (created.length) onSelection(created)
          onTool('select')
        })
        break
      }
      case 'terminal':
        void commit('connector.bind', { id: g.connector, terminal: g.terminal, to: endParams(g.current) }, clearPreview)
        break
      case 'pen': {
        setDraft(null)
        if (g.points.length < 2) break
        const points = simplifyStroke(g.points, Math.max(0.5, 1.25 / z)).map((q) => [q.x, q.y])
        const params: Record<string, unknown> = { points }
        const style = styleFor('stroke', g.highlight ? latest.current.highlightStyle : latest.current.drawStyle)
        if (Object.keys(style).length) params.style = style
        void commit('stroke.create', params, () => setDraft(null))
        break
      }
      case 'erase':
        if (!g.ids.size) break
        latest.current.onSelection(latest.current.selection.filter((id) => !g.ids.has(id)))
        // One delete for the whole pass: one undo step.
        void commit('element.delete', { ids: [...g.ids] }, clearPreview)
        break
    }
  }

  const onDoubleClick = (event: React.MouseEvent<SVGSVGElement>) => {
    // A captured pointer's clicks target the canvas: find what is under it.
    const hit = elementUnder(event.clientX, event.clientY, null)
    if (hit) openText(hit)
  }

  const onWheel = useCallback(
    (event: WheelEvent) => {
      event.preventDefault()
      const box = container.current?.getBoundingClientRect()
      if (event.ctrlKey || event.metaKey) zoomAt(Math.exp(-event.deltaY / 200), { x: event.clientX - (box?.left ?? 0), y: event.clientY - (box?.top ?? 0) })
      else {
        const v = viewport.current
        applyView({ x: v.x + event.deltaX / v.zoom, y: v.y + event.deltaY / v.zoom })
      }
    },
    [applyView, zoomAt],
  )

  useEffect(() => {
    const box = container.current
    if (!box) return undefined
    box.addEventListener('wheel', onWheel, { passive: false })
    return () => box.removeEventListener('wheel', onWheel)
  }, [onWheel])

  // The editor's keys (the frame runs the engine's own shortcuts, such as Undo).
  useImperativeHandle(
    props.api,
    () => ({
      grow: (node: string, sibling: boolean) => growMap(node, sibling),
      edit: (id: string) => openText(id),
      focus: () => container.current?.focus({ preventScroll: true }),
      keyDown: (event: KeyboardEvent) => {
        // The canvas menu's rows keep their keys (the arrows, Delete), and so does the text box.
        if (editing || (event.target as Element | null)?.closest?.('[data-testid="board-canvas-menu"]')) return false
        const command = event.metaKey || event.ctrlKey
        const ids = latest.current.selection
        const key = event.key.toLowerCase()
        const clipboardKey = command && !event.altKey && !event.shiftKey
        // Copy works in a read-only board too; with nothing selected the key goes on.
        if (clipboardKey && key === 'c') {
          if (!ids.length) return false
          copyIds(ids)
          return true
        }
        if (event.key === ' ' && !command) {
          space.current = true
          return true
        }
        // Escape cancels the gesture, else clears the selection, else returns to Select; with
        // nothing of these it is not taken, so in Focus it leaves Focus (KitPointer 4.4).
        if (event.key === 'Escape') {
          if (gesture.current) {
            gesture.current = null
            clearPreview()
          } else if (latest.current.selection.length) latest.current.onSelection([])
          else if (latest.current.tool !== 'select') onTool('select')
          else return false
          return true
        }
        if (command && key === 'a') {
          selectAll()
          return true
        }
        if (latest.current.readOnly) return false
        if (clipboardKey && key === 'x') {
          if (!ids.length) return false
          cutIds(ids)
          return true
        }
        // Not taken, so the browser's paste event follows (with what the system clipboard holds).
        if (clipboardKey && key === 'v') {
          pasteFromKey()
          return false
        }
        // These work on the selection; without one they do nothing (not the engine command's dialog).
        if (event.key === 'Delete' || event.key === 'Backspace') {
          if (!ids.length) return true
          deleteIds(ids)
          return true
        }
        if (command && key === 'd') {
          if (!ids.length) return true
          duplicateIds(ids)
          return true
        }
        if (command && key === 'g') {
          if (!ids.length) return true
          if (event.shiftKey) ungroupIds(ids)
          else if (ids.length > 1) groupIds(ids)
          return true
        }
        const plainKey = !command && !event.altKey && !event.shiftKey && !event.repeat
        const mindNode = ids.length === 1 && isMindNode(sync.view.elements.get(ids[0])) ? graphOf(sync.view, ids[0]) : null
        // Tab grows a branch from the selected node, Enter a sibling beside it (v1's keys).
        if (event.key === 'Tab' && plainKey && mindNode) {
          growMap(ids[0], false)
          return true
        }
        if (event.key === 'Enter' && plainKey && mindNode && mindNode.root !== ids[0]) {
          growMap(ids[0], true)
          return true
        }
        if (event.key === 'Enter' && ids.length === 1) {
          openText(ids[0])
          return true
        }
        const arrows: Record<string, Point> = { ArrowLeft: { x: -1, y: 0 }, ArrowRight: { x: 1, y: 0 }, ArrowUp: { x: 0, y: -1 }, ArrowDown: { x: 0, y: 1 } }
        const arrow = arrows[event.key]
        if (arrow && ids.length && !command) {
          const step = event.shiftKey ? 10 : 1
          void commit('element.move', { ids, delta: [arrow.x * step, arrow.y * step] })
          return true
        }
        if (command || event.altKey) return false
        const next = TOOL_KEYS[event.key.toLowerCase()]
        if (next) {
          onTool(next)
          return true
        }
        return false
      },
    }),
    [clearPreview, commit, editing, growMap, onSelection, onTool, openText, sync],
  )

  useEffect(() => {
    const up = (event: KeyboardEvent) => {
      if (event.key === ' ') space.current = false
    }
    window.addEventListener('keyup', up)
    return () => window.removeEventListener('keyup', up)
  }, [])

  // The overlay: selection, handles, the gesture's draft, snap guides.
  void tick
  const view = sync.view
  const selected = selection.flatMap((id) => view.elements.get(id) ?? [])
  const selBox = selectionBox(sync, layerRef.current, selection)
  const px = 1 / zoom
  const single = selected.length === 1 ? selected[0] : null
  const singleBox = single ? view.rect(single) : null
  const resizable = !readOnly && single && singleBox && !single.locked && isBoxed(single.kind)
  // A selection of several, or one stroke or group, scales by its bounds.
  const scalable = !readOnly && !resizable && selected.length > 0 && !selected.some((element) => element.locked) && !(single?.kind === 'connector')
  const scaleBox = scalable ? selected.reduce<Rect | null>((all, element) => union(all, view.bounds(element)), null) : null
  const connector = !readOnly && single?.kind === 'connector' && !single.locked ? view.route(single) : null
  const connectable = !readOnly && single?.kind === 'shape' && singleBox && !single.locked && tool === 'select'
  // A selected mind-map node's quick controls (v1's `WhiteboardMindMapQuickControls`).
  const mindGraph = useMemo(() => (single && isMindNode(single) && !single.locked ? graphOf(view, single.id) : null), [single, view, tick])
  const lanes = !readOnly && tool === 'select' && single && singleBox && mindGraph ? controlLanes(view, mindGraph, single.id, singleBox, zoom) : []
  const cursor = tool === 'hand' ? 'grab' : tool === 'select' ? 'default' : tool === 'eraser' ? 'cell' : 'crosshair'
  // What the canvas menu's rows can do, from the elements it opened on (and what is inside them).
  const menuState = useMemo<CanvasMenuState | null>(() => {
    if (!menu) return null
    const elements = (menu.ids ?? []).flatMap((id) => view.elements.get(id) ?? [])
    const locked = (menu.ids ?? []).some((id) => view.withDescendants(id).some((inside) => view.elements.get(inside)?.locked))
    return {
      at: menu.at,
      on: menu.ids ? 'elements' : 'canvas',
      readOnly,
      canPaste: clipboard.holding() || typeof navigator.clipboard?.readText === 'function',
      locked,
      allLocked: elements.length > 0 && elements.every((element) => element.locked),
      several: elements.length > 1,
      groups: elements.some((element) => element.kind === 'group'),
    }
  }, [menu, readOnly, view, tick])

  return (
    <div ref={container} className="board-canvas" tabIndex={0} data-testid="board-canvas" data-tool={tool} style={{ cursor }}>
      <svg
        className="board-svg"
        data-testid="board-svg"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        onPointerLeave={() => {
          pointer.current = null
        }}
        onDoubleClick={onDoubleClick}
        onContextMenu={onContextMenu}
      >
        <g ref={world}>
          <g ref={elementsRoot} data-testid="board-elements" />
          <g className="board-overlay" data-testid="board-overlay">
            {selected.map((element) => {
              // One rotated box is outlined in its own frame.
              const own = single && single.rotation && singleBox ? singleBox : null
              const box = own ?? layerRef.current?.worldBox(element.id)
              if (!box) return null
              const c = center(box)
              return <rect key={element.id} className="board-selected" x={box.x0} y={box.y0} width={box.x1 - box.x0} height={box.y1 - box.y0} strokeWidth={1.5 * px} data-selected={element.id} transform={own ? `rotate(${single!.rotation} ${c.x} ${c.y})` : undefined} />
            })}
            {selBox && selected.length > 1 ? (() => {
              const box = scaleBox ?? selBox
              return <rect className="board-selection-box" x={box.x0 - 6 * px} y={box.y0 - 6 * px} width={box.x1 - box.x0 + 12 * px} height={box.y1 - box.y0 + 12 * px} strokeWidth={px} />
            })() : null}
            {resizable && single && singleBox
              ? HANDLES.map((handle) => {
                  const point = handlePoint(singleBox, single.rotation, handle)
                  return <rect key={handle} className="board-handle" data-handle={`resize:${handle}`} data-testid={`board-handle-${handle}`} x={point.x - 4 * px} y={point.y - 4 * px} width={8 * px} height={8 * px} strokeWidth={1.5 * px} transform={single.rotation ? `rotate(${single.rotation} ${point.x} ${point.y})` : undefined} style={{ cursor: handleCursor(handle, single.rotation) }} />
                })
              : null}
            {scaleBox
              ? HANDLES.map((handle) => {
                  const point = handlePoint(scaleBox, 0, handle)
                  return <rect key={handle} className="board-handle" data-handle={`resize:${handle}`} data-testid={`board-handle-${handle}`} x={point.x - 4 * px} y={point.y - 4 * px} width={8 * px} height={8 * px} strokeWidth={1.5 * px} style={{ cursor: handleCursor(handle, 0) }} />
                })
              : null}
            {!readOnly && selBox && selected.some((element) => isBoxed(element.kind) && !element.locked) ? (
              <circle className="board-handle" data-handle="rotate:0" data-testid="board-handle-rotate" cx={(selBox.x0 + selBox.x1) / 2} cy={selBox.y0 - 24 * px} r={5 * px} strokeWidth={1.5 * px} style={{ cursor: 'grab' }} />
            ) : null}
            {connector && connector.length >= 2
              ? (['start', 'end'] as const).map((terminal) => {
                  const point = terminal === 'start' ? connector[0] : connector[connector.length - 1]
                  return <circle key={terminal} className="board-handle" data-handle={`terminal:${terminal}`} data-testid={`board-connector-${terminal}`} cx={point.x} cy={point.y} r={6 * px} strokeWidth={1.5 * px} style={{ cursor: 'crosshair' }} />
                })
              : null}
            {connectable && single && singleBox
              ? CONNECTION_HANDLES.map((item) => {
                  const at = anchorPoint(singleBox, single.rotation, item.anchor)
                  const normal = rotate(item.normal, { x: 0, y: 0 }, single.rotation)
                  return <circle key={item.side} className="board-connect-handle" data-handle={`connect:${item.side}`} data-testid={`board-connect-${item.side}`} cx={at.x + normal.x * 24 * px} cy={at.y + normal.y * 24 * px} r={5 * px} strokeWidth={1.5 * px} />
                })
              : null}
            {lanes.map((lane) => (
              <g key={lane.direction} className="board-mind-lane" data-testid={`board-mind-lane-${lane.direction}`}>
                <line className="board-mind-stem" x1={lane.edge.x} y1={lane.edge.y} x2={lane.controls[lane.controls.length - 1].center.x} y2={lane.controls[lane.controls.length - 1].center.y} strokeWidth={2 * px} />
                {lane.controls.map((control) => (
                  <g
                    key={control.kind}
                    className={`board-mind-control board-mind-${control.kind}`}
                    role="button"
                    aria-label={control.kind === 'add' ? t('mindAddChild') : control.kind === 'collapse' ? t('mindCollapse') : t('mindExpand', { count: control.count ?? 0 })}
                    data-testid={`board-mind-${control.kind}-${lane.direction}`}
                    onPointerDown={(event) => {
                      note(`down mind ${control.kind}`)
                      event.preventDefault()
                      event.stopPropagation()
                    }}
                    onPointerUp={(event) => {
                      note(`up mind ${control.kind}`)
                      event.stopPropagation()
                      if (!single) return
                      if (control.kind === 'add') growMap(single.id, false, lane.direction === 'down' ? undefined : lane.direction)
                      else void commit('mindmap.collapse', { node: single.id })
                    }}
                  >
                    <circle cx={control.center.x} cy={control.center.y} r={17 * px} fill="transparent" />
                    <circle className="board-mind-dot" cx={control.center.x} cy={control.center.y} r={11 * px} strokeWidth={2 * px} />
                    {control.kind === 'expand' ? (
                      <text x={control.center.x} y={control.center.y} fontSize={9.5 * px} textAnchor="middle" dominantBaseline="central">
                        {(control.count ?? 0) > 99 ? '99+' : String(control.count ?? 0)}
                      </text>
                    ) : (
                      <path d={`M${control.center.x - 4.5 * px} ${control.center.y}h${9 * px}${control.kind === 'add' ? `M${control.center.x} ${control.center.y - 4.5 * px}v${9 * px}` : ''}`} strokeWidth={2 * px} />
                    )}
                  </g>
                ))}
              </g>
            ))}
            {drops.map((id) => {
              const element = view.elements.get(id)
              const box = element ? view.rect(element) : null
              return box ? <rect key={id} className="board-drop-target" data-testid="board-drop-target" data-frame={id} x={box.x0} y={box.y0} width={box.x1 - box.x0} height={box.y1 - box.y0} strokeWidth={2 * px} /> : null
            })}
            {draft?.type === 'rect' ? <rect className={draft.dashed ? 'board-brush' : 'board-draft'} x={draft.box.x0} y={draft.box.y0} width={draft.box.x1 - draft.box.x0} height={draft.box.y1 - draft.box.y0} strokeWidth={px} /> : null}
            {draft?.type === 'lines'
              ? draft.lines.map((line, index) => <polyline key={index} className="board-live" data-testid="board-live-line" points={line.map((q) => `${q.x},${q.y}`).join(' ')} strokeWidth={2 * px} />)
              : null}
            {draft?.type === 'lines' && draft.target ? (() => {
              const element = view.elements.get(draft.target)
              const box = element ? view.rect(element) : null
              return box ? <rect className="board-target" x={box.x0} y={box.y0} width={box.x1 - box.x0} height={box.y1 - box.y0} strokeWidth={2 * px} /> : null
            })() : null}
            {draft?.type === 'pen' ? <polyline className={draft.highlight ? 'board-pen board-highlight' : 'board-pen'} data-testid="board-pen-draft" points={draft.points.map((q) => `${q.x},${q.y}`).join(' ')} strokeWidth={draft.highlight ? 16 : 2.5} /> : null}
            {guides.map((guide, index) =>
              guide.axis === 'x' ? (
                <line key={index} className="board-guide" x1={guide.position} y1={guide.start} x2={guide.position} y2={guide.end} strokeWidth={px} />
              ) : (
                <line key={index} className="board-guide" x1={guide.start} y1={guide.position} x2={guide.end} y2={guide.position} strokeWidth={px} />
              ),
            )}
          </g>
        </g>
      </svg>
      {editing ? <TextEditor key={`${editing.id}:${editing.left}:${editing.top}`} editing={editing} placeholder={t('textPlaceholder')} onCommit={commitText} onCancel={() => {
            setEditing(null)
            container.current?.focus({ preventScroll: true })
          }} /> : null}
      <CanvasMenu menu={menuState} t={t} onAction={onMenu} onClose={() => setMenu(null)} />
      {/* The overview is the frame's chrome: Focus shows only the board. */}
      <FrameChrome>
        <Minimap area={props.area} page={page} revision={props.revision} render={props.render} viewport={store} onCenter={(x, y) => applyView({ x: x - viewport.current.width / viewport.current.zoom / 2, y: y - viewport.current.height / viewport.current.zoom / 2 })} label={t('minimap')} />
      </FrameChrome>
    </div>
  )
}

function toWorldFromScreen(left: number, top: number, v: Viewport): Point {
  return { x: v.x + left / v.zoom, y: v.y + top / v.zoom }
}

/** A polyline for a connector being drawn: straight, or the elbow the engine routes. */
function elbowOr(from: Point, to: Point, elbow: boolean): Point[] {
  if (!elbow) return [from, to]
  const middle = (from.x + to.x) / 2
  return [from, { x: middle, y: from.y }, { x: middle, y: to.y }, to]
}

/** What the flow tests and a maintainer's desktop check read from the page. */
type BoardWindow = Window & {
  kitBoard?: {
    viewport: () => Viewport
    setViewport: (next: Partial<Viewport>) => void
    mounts: () => number
    resetMounts: () => void
    fragmentRequests: () => number
    shown: () => string[]
    worldBox: (id: string) => Rect | null
    endPoints: (id: string) => Point[]
    selection: () => string[]
    parent: (id: string) => string | null
    trace: () => string[]
    textMetrics: (request: string) => Promise<string>
    /** Moves `ids` as a drag's commit does: command sent → change shown, in ms. */
    move: (ids: string[], dx: number, dy: number) => Promise<number>
    /**
     * What a paste event on the canvas does with this content, at the pointer (for a browser
     * whose test cannot construct a ClipboardEvent carrying files; web/tests/editing.mjs says which it used).
     */
    paste: (files: File[], text?: string | null) => void
    /** What a drop of `files` at a client point of the frame does (likewise for a DragEvent). */
    drop: (files: File[], clientX: number, clientY: number) => void
  }
}
