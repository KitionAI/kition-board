/**
 * The board canvas (R2c, design note 4): the page view of the board drawn with the engine's SVG
 * fragments (`ElementLayer`), and the interaction on top of it. The pointer gestures (select and
 * move, resize, scale, rotate, marquee, draw shapes, notes and frames, connect, re-attach connector
 * ends, pen and highlighter, erase, pan) are one file each in `./gestures`; this component only
 * dispatches the pointer to them (a press stays "pressed" until the pointer has moved 4 screen
 * pixels, Escape cancels any of them) and draws the overlay: the hover outline, the selection with
 * its handles (following a gesture's preview), the gesture's draft, snap guides and drop targets.
 * It also keeps the keys (tools, typing into the selected element, the view keys), the text box,
 * the clipboard, the canvas menu, pan and zoom. During a gesture only the editor's own view changes;
 * when it ends, one engine command makes the change, one undo step. The gesture's preview stays
 * until that command's change has arrived through the page replica, and goes in the same frame its
 * result is shown (no flicker).
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
import { anchorPoint, center, distance, nearestEdgeAnchor, offset, rotate, union, type Point, type Rect } from '../board/geometry'
import { rayAnchor } from '../board/outline'
import { ElementLayer, type Viewport } from '../board/layer'
import { controlLanes, graphOf, isMindNode } from '../board/mindmap'
import { frameAt, isBoxed, type BoardElement } from '../board/model'
import type { Handle, SnapGuide } from '../board/snap'
import type { BoardSync } from '../board/sync'
import { handleCursor, handlePoint } from '../board/resize'
import { styleFor, type Style } from '../board/style'
import { TOOL_KEYS, type Tool } from '../tools'
import { CanvasMenu, type CanvasAction, type CanvasMenuState } from './CanvasMenu'
import { Minimap, type DocumentArea, type ViewportStore } from './Minimap'
import { marksOf, sendMarks, type TextMark } from '../board/marks'
import { TextEditor, type TextEditing } from './TextEditor'
import { downBrush } from './gestures/brush'
import { downConnect } from './gestures/connect'
import { downDraw } from './gestures/draw'
import { downErase } from './gestures/erase'
import { downMove } from './gestures/move'
import { downPan } from './gestures/pan'
import { downPen } from './gestures/pen'
import { placeMindMap, placeText } from './gestures/place'
import { downResize } from './gestures/resize'
import { downRotate, ROTATE_CURSOR } from './gestures/rotate'
import { downScale } from './gestures/scale'
import { downTerminal } from './gestures/terminal'
import { createdOf, DRAG_THRESHOLD, type Draft, type EndTarget, type Gesture, type GestureContext, type PointerInput, type SelectionPreview } from './gestures/types'

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
  /** The element that holds the keys when nothing else does (the frame's `keyboardHome`). */
  element: () => HTMLElement | null
}

type Props = {
  sync: BoardSync
  page: string | null
  run: (command: string, params: Record<string, unknown>) => Promise<string | null>
  /** A reading command (`edit.copy`), also in a read-only board; answers parsed JSON or null. */
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
/** How far one Ctrl/Cmd+wheel event zooms at most (rule 15): 10%. */
const WHEEL_ZOOM_STEP = 0.1
const HANDLES: readonly Handle[] = ['north-west', 'north', 'north-east', 'east', 'south-east', 'south', 'south-west', 'west']
const CONNECTION_HANDLES: Array<{ side: string; anchor: [number, number]; normal: Point }> = [
  { side: 'north', anchor: [0.5, 0], normal: { x: 0, y: -1 } },
  { side: 'east', anchor: [1, 0.5], normal: { x: 1, y: 0 } },
  { side: 'south', anchor: [0.5, 1], normal: { x: 0, y: 1 } },
  { side: 'west', anchor: [0, 0.5], normal: { x: -1, y: 0 } },
]

/** The gesture under way: the pointer it holds, where it was pressed, and whether it has started to drag. */
type Active = { gesture: Gesture; pointerId: number; start: Point; dragging: boolean; svg: Element }

/** Where the overlay draws the selection's frame and the hover outline (board coordinates), for the tests' `overlay()`. */
type OverlayInfo = { selection: { box: Rect; rotation: number } | null; hover: { id: string; box: Rect } | null; handles: number }

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

/** Whether `target` is a field that keeps its own clipboard. */
function typingIn(target: EventTarget | null): boolean {
  return (target as Element | null)?.closest?.('input, textarea, select, [contenteditable]:not([contenteditable="false"])') != null
}

/**
 * Which way a typed character goes into the single selected element (rule 10): a sticky note, a
 * text and a mind-map node take any printable character; a plain shape takes any but a lower-case
 * tool letter, which still picks its tool (web/tests/flow.mjs presses tool letters with a shape
 * selected; plan section 5). Null: the element takes no typing (a frame, a connector, a picture).
 */
function typingTarget(element: BoardElement | undefined): 'shape' | 'any' | null {
  if (!element || element.locked) return null
  if (element.kind === 'text') return 'any'
  if (element.kind !== 'shape') return null
  return element.shapeStyle === 'sticky' || element.shapeStyle === 'mind-node' ? 'any' : 'shape'
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
  /** Where a gesture under way previews the selection (the overlay follows it). */
  const [preview, setPreview] = useState<SelectionPreview | null>(null)
  /** The element under the pointer with the select tool (rule 3). */
  const [hover, setHover] = useState<string | null>(null)
  const hoverNow = useRef(hover)
  hoverNow.current = hover
  /** The cursor of the gesture that is dragging (null: the tool's). */
  const [gestureCursor, setGestureCursor] = useState<string | null>(null)
  const [spaceHeld, setSpaceHeld] = useState(false)
  const [editing, setEditing] = useState<TextEditing | null>(null)
  const [tick, setTick] = useState(0)
  const active = useRef<Active | null>(null)
  const overlayInfo = useRef<OverlayInfo>({ selection: null, hover: null, handles: 0 })
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
  /**
   * The element whose text box just closed (Enter, Escape, a press elsewhere): while it stays the
   * selection, a typed letter is a tool key again, so a note just written and a tool letter pressed
   * next (N for the next note) do not replace what was written. A press on the canvas or another
   * selection ends it.
   */
  const afterEdit = useRef<string | null>(null)
  /** Commits and closes the open text box (`TextEditor`'s `commitRef`). */
  const commitEditor = useRef<(() => void) | null>(null)
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


  /** Shows `area` whole in the view, zoomed in no further than `max`. */
  const show = useCallback(
    (area: Rect, max = 1) => {
      const v = viewport.current
      if (!v.width || !v.height) return
      const w = Math.max(1, area.x1 - area.x0)
      const h = Math.max(1, area.y1 - area.y0)
      const z = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.min((v.width - 80) / w, (v.height - 80) / h, max)))
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

  /** Gives the keys to the canvas when nothing in the editor holds them (rule 17). */
  const takeKeys = useCallback(() => {
    const focused = document.activeElement
    if (!focused || focused === document.body || focused === document.documentElement) container.current?.focus({ preventScroll: true })
  }, [])

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
    // The board just opened: its keys work at once, nothing clicked first (rule 17).
    box?.focus({ preventScroll: true })
    const observer = new ResizeObserver(size)
    if (box) observer.observe(box)
    const stop = sync.subscribe((changed) => {
      layer.refresh(changed)
      // A selected element another writer deleted leaves the selection.
      const kept = latest.current.selection.filter((id) => sync.view.elements.has(id))
      if (kept.length !== latest.current.selection.length) latest.current.onSelection(kept)
      setTick((value) => value + 1)
      // An agent's change (or an undo from a panel) may have taken away what held the keys: the
      // canvas takes them, unless something outside the editor's page has them.
      if (document.hasFocus() && !editingNow.current) takeKeys()
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
      hover: () => hoverNow.current,
      gesture: () => (active.current ? { type: active.current.gesture.type, phase: active.current.dragging ? 'dragging' : 'pressed' } : null),
      overlay: () => structuredClone(overlayInfo.current),
    }
    return () => {
      stop()
      observer.disconnect()
      layerRef.current = null
      delete (window as BoardWindow).kitBoard
    }
  }, [applyView, contentBox, show, sync, takeKeys])

  // Another selection ends the "just edited" state of the element whose text box closed.
  useEffect(() => {
    if (afterEdit.current && !(selection.length === 1 && selection[0] === afterEdit.current)) afterEdit.current = null
  }, [selection])

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

  /**
   * Runs one engine command, then shows its change; `done` drops the gesture's preview in the same
   * frame. A refused command answers null; `refused` hears the engine's reason.
   */
  const commit = useCallback(
    async (command: string, params: Record<string, unknown>, done?: () => void, refused?: (reason: string) => void) => {
      let result: string | null = null
      const started = performance.now()
      try {
        result = await latest.current.run(command, params)
        note(`${command} ran in ${Math.round(performance.now() - started)} ms`)
      } catch (error) {
        done?.()
        console.warn(`[board] ${command} was refused: ${errorText(error)}`)
        refused?.(errorText(error))
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
    setPreview(null)
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

  const openText = useCallback(
    /** Opens the text box on `id` (or a new text at `at`); `typed` replaces its text with a typed character, the caret after it. */
    (id: string | null, at?: Point, typed?: string) => {
      if (latest.current.readOnly) return
      const v = viewport.current
      if (!id) {
        if (!at) return
        const s = toScreen(at)
        // The first line of the new text is centred on the pointer.
        setEditing({ id: null, kind: 'new-text', value: '', left: s.x, top: s.y - 17, width: Math.max(160, 240 * v.zoom), height: 34, fontSize: Math.max(12, 20 * v.zoom), align: 'left', marks: [] })
        return
      }
      const element = sync.view.elements.get(id)
      if (!element || element.locked) return
      if (element.kind === 'connector') {
        const route = sync.view.route(element)
        const middle = route.length ? route[Math.floor(route.length / 2) - (route.length % 2 ? 0 : 1)] : { x: 0, y: 0 }
        const s = toScreen(middle)
        setEditing({ id, kind: 'connector', value: element.text, left: s.x - 90, top: s.y - 17, width: 180, height: 34, fontSize: Math.max(12, 14 * v.zoom), align: 'center', marks: null })
        return
      }
      const box = sync.view.rect(element)
      if (!box || !['shape', 'text', 'frame', 'card'].includes(element.kind)) return
      const s = toScreen({ x: box.x0, y: box.y0 })
      const w = (box.x1 - box.x0) * v.zoom
      const h = (box.y1 - box.y0) * v.zoom
      const size = (element.fontSize ?? (element.kind === 'text' ? 20 : 16)) * v.zoom
      // A shape's label and a text take bold, italic and links; a Markdown one writes them as Markdown.
      // A typed character replaces the text, so the stored marks have nothing left to stand on.
      const rich = (element.kind === 'shape' || element.kind === 'text') && sync.textFormat(id) !== 'markdown'
      const marks = rich ? (typed == null ? marksOf(sync.textRuns(id)) : []) : null
      const value = typed ?? element.text
      const start = typed != null
      if (element.kind === 'shape') setEditing({ id, kind: 'shape', value, left: s.x + 8, top: s.y + h / 2 - size, width: Math.max(80, w - 16), height: size * 2, fontSize: size, align: 'center', marks, typed: start })
      else if (element.kind === 'frame') setEditing({ id, kind: 'frame', value, left: s.x, top: s.y - 30, width: Math.max(160, w), height: 28, fontSize: 14 * v.zoom, align: 'left', marks: null, typed: start })
      else if (element.kind === 'card') setEditing({ id, kind: 'card', value, left: s.x, top: s.y + h + 4, width: Math.max(160, w), height: 28, fontSize: 13 * v.zoom, align: 'center', marks: null, typed: start })
      else setEditing({ id, kind: 'text', value, left: s.x, top: s.y, width: Math.max(160, w + 24), height: Math.max(34, h), fontSize: size, align: 'left', marks, typed: start })
    },
    [sync, toScreen],
  )

  /**
   * The marks the box kept, after the text: one `text.mark` each, in the order they were made, in
   * UTF-16 offsets of the committed text (the engine has no command that writes a text and its
   * marks together, so each is an undo step of its own). A refused mark does not stop the others;
   * the person is told what was refused.
   */
  const commitMarks = useCallback(
    async (id: string, text: string, marks: readonly TextMark[]) => {
      const refused = await sendMarks(id, text, marks, async (params) => {
        let reason: string | null = null
        const answer = await commit('text.mark', params, undefined, (why) => {
          reason = why
        })
        return answer ? null : (reason ?? 'text.mark was refused')
      })
      if (refused.length) toast('error', latest.current.t('marksFailed'), refused.join('; '))
    },
    [commit],
  )

  const commitText = useCallback(
    (value: string, marks: readonly TextMark[]) => {
      const current = editing
      setEditing(null)
      afterEdit.current = current?.id ?? null
      // The keys go back to the canvas (Undo, Delete) once the box closes.
      container.current?.focus({ preventScroll: true })
      if (!current) return
      if (current.id == null) {
        if (!value.trim()) return
        const at = toWorldFromScreen(current.left, current.top, viewport.current)
        const parent = frameUnder(at)
        void commit('element.add', { elements: [{ kind: 'text', position: [at.x, at.y], text: value, ...styleFor('text', latest.current.drawStyle), ...(parent ? { parent } : {}) }] }).then(async (answer) => {
          const created = (answer?.created as string[] | undefined) ?? []
          if (created.length === 1) afterEdit.current = created[0]
          if (created.length) latest.current.onSelection(created)
          if (created.length === 1 && marks.length) await commitMarks(created[0], value, marks)
        })
        return
      }
      const element = sync.view.elements.get(current.id)
      if (!element) return
      const id = current.id
      if (element.text === value) {
        if (marks.length) void commitMarks(id, value, marks)
        return
      }
      if (current.kind === 'connector') void commit('connector.setLabel', { id, text: value })
      else
        void commit('element.setText', { id, text: value }).then((answer) => {
          if (answer && marks.length) return commitMarks(id, value, marks)
        })
    },
    [commit, commitMarks, editing, sync],
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
  /** Whether something among `ids`, or inside them, is locked: Cut and Delete do not apply (the canvas menu has them off). */
  const lockedIn = (ids: readonly string[]) => ids.some((id) => sync.view.withDescendants(id).some((inside) => sync.view.elements.get(inside)?.locked))
  /** Cut: nothing at all (no copy, the selection kept) when something in it is locked. */
  const cutIds = (ids: readonly string[]) => {
    if (lockedIn(ids)) return
    void clipboard.copy(latest.current.query, ids).then((copied) => {
      if (!copied || latest.current.readOnly) return
      latest.current.onSelection([])
      void commit('element.delete', { ids: [...ids] })
    })
  }
  /**
   * Pastes the board payload `from` holds (or the copy kept here) with its top-left at `at`; selects
   * the copies. Pictures copied on another board whose files this board does not have are left out
   * by the engine (`skipped`), and the person is told.
   */
  const pasteBoard = async (from: clipboard.Carried | null, at: Point) => {
    const payload = await clipboard.payloadFor(from)
    if (!payload || latest.current.readOnly) return
    const place = clipboard.pastePlace(at)
    const t = latest.current.t
    const answer = await commit('edit.paste', { ...onPage(), payload, at: [place.x, place.y] }, undefined, (reason) => toast('error', t('pasteFailed'), reason))
    if (Array.isArray(answer?.skipped) && answer.skipped.length) toast('warning', t('picturesLeftOut'), t('picturesLeftOutHint'))
    selectMade(createdOf(answer))
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
      const others = files.filter((file) => clipboard.pictureMime(file) == null)
      if (others.length) toast('warning', latest.current.t('dropUnsupported'), others.map((file) => file.name).join(', '))
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
    // Files dragged over the canvas are always taken, so the browser never opens a dropped file in
    // the editor's frame (its default for a drop nobody takes); a read-only board takes none of them.
    const carriesFiles = (event: DragEvent) => Array.from(event.dataTransfer?.types ?? []).includes('Files') || (event.dataTransfer?.files.length ?? 0) > 0
    const onDragOver = (event: DragEvent) => {
      if (!carriesFiles(event)) return
      event.preventDefault()
      if (event.dataTransfer) event.dataTransfer.dropEffect = latest.current.readOnly ? 'none' : 'copy'
    }
    const onDrop = (event: DragEvent) => {
      if (!carriesFiles(event)) return
      event.preventDefault()
      if (latest.current.readOnly) return
      // Pictures are placed; any other file gets a notice (clip.current.drop).
      clip.current.drop(Array.from(event.dataTransfer?.files ?? []), event.clientX, event.clientY)
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
    if (lockedIn(ids)) return
    latest.current.onSelection([])
    void commit('element.delete', { ids: [...ids] })
  }
  const groupIds = (ids: readonly string[]) => {
    void commit('arrange.group', { ids: [...ids] }).then((answer) => {
      if (typeof answer?.group === 'string') latest.current.onSelection([answer.group])
    })
  }
  const ungroupIds = (ids: readonly string[]) => {
    const groups = ids.filter((id) => sync.view.elements.get(id)?.kind === 'group')
    if (groups.length) void commit('arrange.ungroup', { groupIds: groups })
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
    if (editing || active.current) return
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

  // The gestures' view of the canvas (gestures/types.ts): reads go through refs, so one object serves every render.
  const openTextRef = useRef(openText)
  openTextRef.current = openText
  const ctx = useMemo<GestureContext>(
    () => ({
      sync,
      layer: () => layerRef.current,
      view: () => viewport.current,
      applyView,
      readOnly: () => latest.current.readOnly,
      selection: () => latest.current.selection,
      select: (ids) => latest.current.onSelection(ids),
      setTool: (next) => latest.current.onTool(next),
      shapeType: () => latest.current.shapeType,
      drawStyle: () => latest.current.drawStyle,
      highlightStyle: () => latest.current.highlightStyle,
      t: () => latest.current.t,
      setDraft,
      setGuides,
      setDrops: (frames) => setDrops((now) => (now.length === frames.length && now.every((id, at) => id === frames[at]) ? now : frames)),
      setPreview,
      setOverrides: (overrides) => layerRef.current?.setOverrides(overrides),
      setHidden: (ids) => layerRef.current?.setHidden(ids),
      clearPreview,
      commit: (command, params, done, refused) => commitRef.current(command, params, done, refused),
      duplicate: (ids, by, done) => duplicateRef.current(ids, by, done),
      openText: (id, at) => openTextRef.current(id, at),
      selectionBox: (ids) => selectionBox(sync, layerRef.current, ids),
      frameUnder: (p) => frameUnderRef.current(p),
      elementUnder: (clientX, clientY, excluding) => elementUnderRef.current(clientX, clientY, excluding),
      endAt: (p, input, excluding) => endAtRef.current(p, input, excluding),
    }),
    [applyView, clearPreview, sync],
  )
  const duplicateRef = useRef(duplicateIds)
  duplicateRef.current = duplicateIds
  const frameUnderRef = useRef(frameUnder)
  frameUnderRef.current = frameUnder
  const elementUnderRef = useRef(elementUnder)
  elementUnderRef.current = elementUnder
  const endAtRef = useRef(endAt)
  endAtRef.current = endAt

  const inputOf = (event: React.PointerEvent<SVGSVGElement>): PointerInput => ({
    p: toWorld(event.clientX, event.clientY),
    clientX: event.clientX,
    clientY: event.clientY,
    shift: event.shiftKey,
    alt: event.altKey,
    command: event.metaKey || event.ctrlKey,
    native: event.nativeEvent,
  })

  /** Lets the gesture's pointer go and forgets the gesture. */
  const release = () => {
    const now = active.current
    active.current = null
    setGestureCursor(null)
    if (now && now.svg.hasPointerCapture?.(now.pointerId)) now.svg.releasePointerCapture(now.pointerId)
  }

  /** Escape, or the browser taking the pointer: the gesture puts back what it changed, the preview goes, the pointer is let go (rule 1). */
  const cancelGesture = () => {
    const now = active.current
    if (!now) return false
    note(`cancel ${now.gesture.type}`)
    release()
    now.gesture.cancel(ctx)
    clearPreview()
    return true
  }

  /** The press: picks the gesture (the handle under it, else the tool and what it hit) and holds it. */
  const startGesture = (event: React.PointerEvent<SVGSVGElement>, input: PointerInput): Gesture | null => {
    const v = sync.view
    if (event.button === 1 || tool === 'hand' || space.current) return downPan(ctx, input)
    if (event.button !== 0) return null
    const handle = (event.target as Element).closest?.('[data-handle]')?.getAttribute('data-handle') ?? null
    if (handle && !readOnly) {
      const [kind, value] = handle.split(':')
      if (kind === 'resize') {
        const element = selection.length === 1 ? v.elements.get(selection[0]) : null
        const box = element && isBoxed(element.kind) ? v.rect(element) : null
        if (element && box) return downResize(ctx, element.id, value as Handle, box, element.rotation)
        // A selection (or a stroke or group) scales as a whole, by the bounds the engine maps from.
        const roots = selection.filter((id) => v.elements.has(id))
        const bounds = roots.reduce<Rect | null>((all, id) => {
          const element = v.elements.get(id)
          return element ? union(all, v.bounds(element)) : all
        }, null)
        return roots.length && bounds ? downScale(ctx, roots, [...new Set(roots.flatMap((id) => v.withDescendants(id)))], value as Handle, bounds) : null
      }
      if (kind === 'rotate') {
        const box = selectionBox(sync, layerRef.current, selection)
        return box ? downRotate(ctx, input, [...selection], center(box)) : null
      }
      if (kind === 'terminal') return selection[0] ? downTerminal(ctx, input, selection[0], value as 'start' | 'end') : null
      if (kind === 'connect') {
        const element = v.elements.get(selection[0])
        const box = element ? v.rect(element) : null
        const side = CONNECTION_HANDLES.find((item) => item.side === value)
        if (!element || !box || !side) return null
        return downConnect(ctx, { element: element.id, anchor: side.anchor, point: v.boundPoint(element, box, side.anchor) }, input.p, true)
      }
    }
    const hit = elementAt(event.target)
    if (tool === 'select') return hit ? downMove(ctx, input, hit) : downBrush(ctx, input)
    if (readOnly) return null
    if (tool === 'text') {
      placeText(ctx, input.p, hit)
      return null
    }
    if (tool === 'mindmap') {
      placeMindMap(ctx, input.p)
      return null
    }
    if (tool === 'connector') return downConnect(ctx, endAt(input.p, input, null), input.p, false)
    if (tool === 'pen' || tool === 'highlight') return downPen(ctx, input, tool === 'highlight')
    if (tool === 'eraser') return downErase(ctx, input)
    return downDraw(ctx, input, tool)
  }

  const onPointerDown = (event: React.PointerEvent<SVGSVGElement>) => {
    note(`down ${tool}${editing ? ' editing' : ''} ${event.button}`)
    pointer.current = { x: event.clientX, y: event.clientY }
    // A press on the canvas closes the text box, committing it (whatever has the focus).
    if (editing) {
      commitEditor.current?.()
      return
    }
    // The canvas takes the keys (tool letters, Delete, Undo) once it is used.
    container.current?.focus({ preventScroll: true })
    afterEdit.current = null
    // A press while another is held (a second button) ends the first.
    if (active.current) cancelGesture()
    setHover(null)
    const gesture = startGesture(event, inputOf(event))
    if (!gesture) return
    const svg = event.currentTarget
    svg.setPointerCapture?.(event.pointerId)
    active.current = { gesture, pointerId: event.pointerId, start: { x: event.clientX, y: event.clientY }, dragging: !gesture.threshold, svg }
    if (!gesture.threshold) setGestureCursor(gesture.cursor)
  }

  /** The element the pointer is over, for the hover outline (select tool, no gesture). */
  const hoverAt = (target: EventTarget | null): string | null => {
    if (tool !== 'select' || editing) return null
    const hit = elementAt(target)
    return hit ? sync.view.selectable(hit) : null
  }

  const onPointerMove = (event: React.PointerEvent<SVGSVGElement>) => {
    pointer.current = { x: event.clientX, y: event.clientY }
    const now = active.current
    if (!now || now.pointerId !== event.pointerId) {
      const over = hoverAt(event.target)
      if (over !== hoverNow.current) setHover(over)
      return
    }
    if (!now.dragging) {
      if (distance(now.start, { x: event.clientX, y: event.clientY }) < DRAG_THRESHOLD) return
      now.dragging = true
      setGestureCursor(now.gesture.cursor)
    }
    now.gesture.move(ctx, inputOf(event))
  }

  const onPointerUp = (event: React.PointerEvent<SVGSVGElement>) => {
    const now = active.current
    note(`${event.type} ${now?.gesture.type ?? 'none'}`)
    if (!now || now.pointerId !== event.pointerId) return
    release()
    now.gesture.up(ctx, inputOf(event), now.dragging)
  }

  const onDoubleClick = (event: React.MouseEvent<SVGSVGElement>) => {
    // A captured pointer's clicks target the canvas: find what is under it.
    const hit = elementUnder(event.clientX, event.clientY, null)
    if (hit) openText(hit)
    // On the empty canvas, a new text there (rule 10).
    else if (tool === 'select' && !readOnly) openText(null, toWorld(event.clientX, event.clientY))
  }

  const onWheel = useCallback(
    (event: WheelEvent) => {
      event.preventDefault()
      const box = container.current?.getBoundingClientRect()
      if (event.ctrlKey || event.metaKey) {
        // At most 10% per event, however large its delta (a mouse notch, a trackpad pinch).
        const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? 100 : 1
        const step = Math.max(-WHEEL_ZOOM_STEP, Math.min(WHEEL_ZOOM_STEP, (event.deltaY * unit) / 100))
        zoomAt(step < 0 ? 1 - step : 1 / (1 + step), { x: event.clientX - (box?.left ?? 0), y: event.clientY - (box?.top ?? 0) })
      } else {
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

  /** Shift+2: the selection at 100%, centred; already at 100%, the selection fitted. */
  const showSelection = () => {
    const box = selectionBox(sync, layerRef.current, latest.current.selection)
    if (!box) return
    const v = viewport.current
    if (v.zoom !== 1) {
      const c = center(box)
      applyView({ zoom: 1, x: c.x - v.width / 2, y: c.y - v.height / 2 })
    } else show(box, MAX_ZOOM)
  }

  // The editor's keys (the frame runs the engine's own shortcuts, such as Undo).
  useImperativeHandle(
    props.api,
    () => ({
      grow: (node: string, sibling: boolean) => growMap(node, sibling),
      edit: (id: string) => openText(id),
      focus: () => container.current?.focus({ preventScroll: true }),
      element: () => container.current,
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
          setSpaceHeld(true)
          return true
        }
        // Escape cancels the gesture, else clears the selection, else returns to Select; with
        // nothing of these it is not taken, so in Focus it leaves Focus (KitPointer 4.4).
        if (event.key === 'Escape') {
          if (cancelGesture()) return true
          if (latest.current.selection.length) latest.current.onSelection([])
          else if (latest.current.tool !== 'select') onTool('select')
          else return false
          return true
        }
        // The view (rule 15): Shift+0 100%, Shift+1 everything, Shift+2 the selection.
        if (event.shiftKey && !command && !event.altKey && (event.code === 'Digit0' || event.code === 'Digit1' || event.code === 'Digit2')) {
          if (event.code === 'Digit0') zoomAt(1 / viewport.current.zoom)
          else if (event.code === 'Digit1') {
            const box = contentBox()
            if (box) show(box)
          } else showSelection()
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
        // A character typed with one element selected that takes text starts its text with it (rule 10).
        if (latest.current.tool === 'select' && [...event.key].length === 1 && event.key !== ' ' && !event.isComposing && ids.length === 1 && ids[0] !== afterEdit.current) {
          const target = typingTarget(sync.view.elements.get(ids[0]))
          if (target === 'any' || (target === 'shape' && !TOOL_KEYS[event.key])) {
            openText(ids[0], undefined, event.key)
            return true
          }
        }
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
      if (event.key !== ' ') return
      space.current = false
      setSpaceHeld(false)
    }
    // A Space released while the page had no focus never comes back as a keyup.
    const lost = () => {
      space.current = false
      setSpaceHeld(false)
    }
    window.addEventListener('keyup', up)
    window.addEventListener('blur', lost)
    return () => {
      window.removeEventListener('keyup', up)
      window.removeEventListener('blur', lost)
    }
  }, [])

  // The overlay: hover, selection, handles, the gesture's draft, snap guides.
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
  // While a gesture drags, the handles that start other gestures (connect, mind-map controls) wait.
  const dragging = gestureCursor != null || preview != null
  const connectable = !readOnly && single?.kind === 'shape' && singleBox && !single.locked && tool === 'select' && !dragging
  // A selected mind-map node's quick controls (v1's `WhiteboardMindMapQuickControls`).
  const mindGraph = useMemo(() => (single && isMindNode(single) && !single.locked ? graphOf(view, single.id) : null), [single, view, tick])
  const lanes = !readOnly && tool === 'select' && single && singleBox && mindGraph && !dragging ? controlLanes(view, mindGraph, single.id, singleBox, zoom) : []
  // The selection as the gesture previews it (rule 4): moved, resized, scaled or turned with it.
  const shownSingle = preview?.type === 'resize' ? preview.box : singleBox
  const shownScale = preview?.type === 'scale' ? preview.box : scaleBox
  const selectionTransform = preview?.type === 'move' ? `translate(${preview.dx} ${preview.dy})` : preview?.type === 'rotate' ? `rotate(${preview.degrees} ${preview.origin.x} ${preview.origin.y})` : undefined
  const turned = preview?.type === 'rotate' ? preview.degrees : 0
  const shift = preview?.type === 'move' ? { x: preview.dx, y: preview.dy } : { x: 0, y: 0 }
  // The rotate handle: outside the selection's top-right corner, clear of the connection handle above the top edge.
  const rotateAt = (() => {
    if (readOnly || !selBox || !selected.some((element) => isBoxed(element.kind) && !element.locked)) return null
    if (resizable && single && shownSingle) {
      const corner = anchorPoint(shownSingle, single.rotation, [1, 0])
      const out = rotate({ x: 16 * px, y: -16 * px }, { x: 0, y: 0 }, single.rotation)
      return { x: corner.x + out.x, y: corner.y + out.y }
    }
    const box = shownScale ?? selBox
    return { x: box.x1 + 16 * px, y: box.y0 - 16 * px }
  })()
  const hovered = hover && tool === 'select' && !dragging && !selection.includes(hover) ? view.elements.get(hover) : undefined
  const hoverBox = hovered ? layerRef.current?.worldBox(hovered.id) ?? null : null
  const hoverMovable = hover != null && tool === 'select' && !readOnly && !view.elements.get(hover)?.locked
  const cursor = gestureCursor ?? (tool === 'hand' || spaceHeld ? 'grab' : tool === 'select' ? (hoverMovable ? 'move' : 'default') : tool === 'eraser' ? 'cell' : 'crosshair')
  // What the tests read (`kitBoard.overlay()`): the selection's frame and the hover outline as drawn.
  const frameOf = (): OverlayInfo['selection'] => {
    if (single && resizable && shownSingle) {
      const box = single.rotation || preview?.type === 'resize' ? shownSingle : (layerRef.current?.worldBox(single.id) ?? shownSingle)
      return { box: offset(box, shift.x, shift.y), rotation: single.rotation + turned }
    }
    const box = shownScale ?? selBox
    return box ? { box: offset(box, shift.x, shift.y), rotation: turned } : null
  }
  overlayInfo.current = {
    selection: selected.length ? frameOf() : null,
    hover: hovered && hoverBox ? { id: hovered.id, box: hoverBox } : null,
    handles: resizable || shownScale ? HANDLES.length : 0,
  }
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
  const markLabels = useMemo(() => ({ bar: t('marks'), bold: t('markBold'), italic: t('markItalic'), link: t('markLink'), address: t('markLinkAddress') }), [t])

  return (
    <div ref={container} className="board-canvas" tabIndex={0} data-testid="board-canvas" data-tool={tool} style={{ cursor }}>
      <svg
        className="board-svg"
        data-testid="board-svg"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={() => {
          cancelGesture()
        }}
        onPointerLeave={() => {
          pointer.current = null
          if (!active.current) setHover(null)
        }}
        onDoubleClick={onDoubleClick}
        onContextMenu={onContextMenu}
      >
        <g ref={world}>
          <g ref={elementsRoot} data-testid="board-elements" />
          <g className="board-overlay" data-testid="board-overlay">
            {hovered && hoverBox ? (
              <g data-testid="board-hover" data-hover={hovered.id}>
                {hovered.kind === 'connector' ? (
                  <polyline className="board-hover" points={view.route(hovered).map((q) => `${q.x},${q.y}`).join(' ')} strokeWidth={1.5 * px} />
                ) : hovered.kind === 'stroke' && hovered.points.length > 1 ? (
                  <polyline className="board-hover" points={hovered.points.map((q) => `${q.x},${q.y}`).join(' ')} strokeWidth={1.5 * px} />
                ) : (() => {
                  const own = isBoxed(hovered.kind) && hovered.rotation ? view.rect(hovered) : null
                  const box = own ?? hoverBox
                  const c = center(box)
                  return <rect className="board-hover" x={box.x0} y={box.y0} width={box.x1 - box.x0} height={box.y1 - box.y0} strokeWidth={1.5 * px} transform={own ? `rotate(${hovered.rotation} ${c.x} ${c.y})` : undefined} />
                })()}
              </g>
            ) : null}
            <g data-testid="board-selection" transform={selectionTransform}>
              {preview?.type === 'scale'
                ? null
                : selected.map((element) => {
                    // One rotated box is outlined in its own frame (and a resize on its new box).
                    const own = single && (single.rotation || preview?.type === 'resize') && shownSingle ? shownSingle : null
                    const box = own ?? layerRef.current?.worldBox(element.id)
                    if (!box) return null
                    const c = center(box)
                    return <rect key={element.id} className="board-selected" x={box.x0} y={box.y0} width={box.x1 - box.x0} height={box.y1 - box.y0} strokeWidth={1.5 * px} data-selected={element.id} data-testid="board-selection-frame" transform={own && single!.rotation ? `rotate(${single!.rotation} ${c.x} ${c.y})` : undefined} />
                  })}
              {selBox && selected.length > 1 ? (() => {
                const box = shownScale ?? selBox
                return <rect className="board-selection-box" data-testid="board-selection-box" x={box.x0 - 6 * px} y={box.y0 - 6 * px} width={box.x1 - box.x0 + 12 * px} height={box.y1 - box.y0 + 12 * px} strokeWidth={px} />
              })() : null}
              {resizable && single && shownSingle
                ? HANDLES.map((handle) => {
                    const point = handlePoint(shownSingle, single.rotation, handle)
                    return <rect key={handle} className="board-handle" data-handle={`resize:${handle}`} data-testid={`board-handle-${handle}`} x={point.x - 4 * px} y={point.y - 4 * px} width={8 * px} height={8 * px} strokeWidth={1.5 * px} transform={single.rotation ? `rotate(${single.rotation} ${point.x} ${point.y})` : undefined} style={{ cursor: handleCursor(handle, single.rotation) }} />
                  })
                : null}
              {shownScale
                ? HANDLES.map((handle) => {
                    const point = handlePoint(shownScale, 0, handle)
                    return <rect key={handle} className="board-handle" data-handle={`resize:${handle}`} data-testid={`board-handle-${handle}`} x={point.x - 4 * px} y={point.y - 4 * px} width={8 * px} height={8 * px} strokeWidth={1.5 * px} style={{ cursor: handleCursor(handle, 0) }} />
                  })
                : null}
              {rotateAt ? <circle className="board-handle" data-handle="rotate:0" data-testid="board-handle-rotate" cx={rotateAt.x} cy={rotateAt.y} r={5 * px} strokeWidth={1.5 * px} style={{ cursor: ROTATE_CURSOR }} /> : null}
              {connector && connector.length >= 2
                ? (['start', 'end'] as const).map((terminal) => {
                    const point = terminal === 'start' ? connector[0] : connector[connector.length - 1]
                    return <circle key={terminal} className="board-handle" data-handle={`terminal:${terminal}`} data-testid={`board-connector-${terminal}`} cx={point.x} cy={point.y} r={6 * px} strokeWidth={1.5 * px} style={{ cursor: 'crosshair' }} />
                  })
                : null}
            </g>
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
      {editing ? <TextEditor key={`${editing.id}:${editing.left}:${editing.top}`} editing={editing} placeholder={t('textPlaceholder')} labels={markLabels} commitRef={commitEditor} onCommit={commitText} onCancel={() => {
            afterEdit.current = editing.id
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
    /** The element under the pointer that the hover outline marks (select tool), or null. */
    hover: () => string | null
    /** The gesture under way: its kind, and whether it is still a press or already a drag. */
    gesture: () => { type: string; phase: 'pressed' | 'dragging' } | null
    /** Where the overlay draws the selection's frame (board coordinates, unrotated box and its rotation) and the hover outline. */
    overlay: () => OverlayInfo
  }
}
