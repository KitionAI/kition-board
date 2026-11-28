/**
 * The canvas's pointer gestures (plan "Board usable by hand", rule 1): each gesture is one file with
 * a `down` entry point that starts it (or answers null) and an object with `move`, `up` and
 * `cancel`. `BoardCanvas` only dispatches: on a press it picks the gesture, holds it "pressed" until
 * the pointer has moved `DRAG_THRESHOLD` screen pixels (a gesture with `threshold: false` drags at
 * once), then sends it every move; on release `up` hears whether it dragged (a release before is a
 * click). Escape, or the browser taking the pointer, calls `cancel`, which puts back what the gesture
 * changed; the canvas then drops the preview and lets the pointer go.
 *
 * During a gesture only the editor's own view changes (R2c, design note 4.3): fragment overrides in
 * the layer, the draft the overlay draws, snap guides, drop targets and the selection overlay's
 * preview. `up` sends the gesture's one engine command; the preview stays until its change has
 * arrived (`commit`'s `done`).
 */
import type { Strings } from '@kition/sdk/editor'
import type { Point, Rect } from '../../board/geometry'
import type { ElementLayer, Override, Viewport } from '../../board/layer'
import type { SnapGuide } from '../../board/snap'
import type { Style } from '../../board/style'
import type { EndParam, Route, RouteQuery } from '../../board/route'
import type { BoardSync } from '../../board/sync'

/** How far (screen pixels) the pointer goes before a press becomes a drag. */
export const DRAG_THRESHOLD = 4
/** The smallest drawn box (screen pixels); a smaller one is a click's default box. */
export const MIN_DRAW = 6
/** How near (screen pixels) a moved or resized box snaps to the edges and centres around it (rule 7). */
export const SNAP_THRESHOLD = 8
/** A move snaps only while the pointer is slower than this (screen pixels per millisecond, rule 7). */
export const SNAP_SPEED = 0.5

/**
 * Where a connector end goes: an element (and where on it; `side` when it snapped to one of its
 * four side midpoints, no anchor when it binds to the centre and the engine faces it to the other
 * end) or a free point.
 */
export type EndTarget = { element: string; anchor: [number, number] | null; point: Point; side: string | null } | { element: null; point: Point }

/**
 * What the overlay draws for the gesture under way. `routes` are connectors as the engine would
 * draw them (`connector.getRoute`): the connector being drawn, re-attached or bent, or those
 * carried by a move.
 */
export type Draft =
  | { type: 'rect'; box: Rect; dashed: boolean }
  | { type: 'routes'; routes: Route[] }
  | { type: 'pen'; points: Point[]; highlight: boolean }
  /** A shape being drawn: the engine's picture of it (`shape.previews`), stretched to `box`. */
  | { type: 'shape'; box: Rect; shape: string; picture: { viewBox: string; body: string } | null }

/**
 * The shape a connector end would go to, shown with its four side midpoints (rule 11): `target`
 * while a connector is drawn or re-attached, or the connector tool hovers (outlined by its own
 * outline, `side` the midpoint it snaps to); `hover` with the select tool near a shape's edge (the
 * midpoints start a connector when pressed).
 */
export type ConnectHint = { element: string; side: string | null; mode: 'target' | 'hover' }

/** Where the selection overlay is drawn while a gesture previews its change (rule 4: it follows). */
export type SelectionPreview =
  | { type: 'move'; dx: number; dy: number }
  | { type: 'resize'; box: Rect; rotation: number }
  | { type: 'scale'; box: Rect }
  | { type: 'rotate'; origin: Point; degrees: number }

/** One pointer event as the gestures read it: the board point, the client point and the modifiers. */
export type PointerInput = {
  p: Point
  clientX: number
  clientY: number
  shift: boolean
  alt: boolean
  /** Cmd or Ctrl. */
  command: boolean
  /** How fast the pointer moves (screen pixels per millisecond, from the browser's event times). */
  speed: number
  /** The browser's event (its coalesced samples, for the eraser). */
  native: PointerEvent
}

/** What a gesture may read and do: the board, the view, the preview, and one engine command. */
export type GestureContext = {
  sync: BoardSync
  layer: () => ElementLayer | null
  view: () => Viewport
  applyView: (next: Partial<Viewport>) => void
  readOnly: () => boolean
  selection: () => readonly string[]
  select: (ids: string[]) => void
  /** What a press on element `id` selects: its outermost group, or within the group drilled into, the member there (rule 14). */
  selectable: (id: string) => string
  /** A drawing tool made something: back to Select, unless tool lock (Q) is on. */
  toolDone: () => void
  shapeType: () => string
  drawStyle: () => Style
  highlightStyle: () => Style
  t: () => Strings
  setDraft: (draft: Draft | null) => void
  setGuides: (guides: SnapGuide[]) => void
  setDrops: (frames: string[]) => void
  setPreview: (preview: SelectionPreview | null) => void
  setOverrides: (overrides: ReadonlyMap<string, Override>) => void
  setHidden: (ids: Iterable<string>) => void
  /** Drops every preview (overrides, hidden elements, draft, guides, drop targets, the overlay's). */
  clearPreview: () => void
  /** Runs one engine command; `done` drops the preview once its change is shown (or it was refused). */
  commit: (command: string, params: Record<string, unknown>, done?: () => void, refused?: (reason: string) => void) => Promise<Record<string, unknown> | null>
  /** element.duplicate of `ids` by `offset`, selecting the copies. */
  duplicate: (ids: readonly string[], offset: Point, done: () => void) => void
  openText: (id: string | null, at?: Point) => void
  /** Opens a new text starting at `x` with its first line centred on `y`: auto width, or `width` wide (it wraps). */
  openNewText: (x: number, y: number, width: number | null) => void
  /** The line height of a new text (the drawing style's size, the engine's line height). */
  textLineHeight: () => number
  /** The bounds of `ids` as drawn (the fragments' boxes). */
  selectionBox: (ids: readonly string[]) => Rect | null
  /** The frame a new element at `p` goes into, if any. */
  frameUnder: (p: Point) => string | null
  /** The topmost element painted at a client point, other than `excluding`. */
  elementUnder: (clientX: number, clientY: number, excluding: string | null) => string | null
  /** A connector end over `p`, on none of the elements `excluding`. */
  endAt: (p: Point, input: { clientX: number; clientY: number }, excluding: readonly string[]) => EndTarget
  /** Shows the shape a connector end would go to, with its side midpoints (null: none). */
  setConnectHint: (hint: ConnectHint | null) => void
  /** A question at a time to the engine for routes (`connector.getRoute`); `answer` hears the latest. */
  routeQuery: (answer: (routes: Route[]) => void) => RouteQuery
}

export type GestureType = 'pan' | 'move' | 'resize' | 'scale' | 'rotate' | 'brush' | 'draw' | 'connect' | 'terminal' | 'bend' | 'pen' | 'erase' | 'text'

export interface Gesture {
  readonly type: GestureType
  /** Whether the press waits for `DRAG_THRESHOLD` before it drags (false: it drags at once). */
  readonly threshold: boolean
  /** The canvas's cursor while it drags (null: the tool's); read again after each move (a resize that flips turns it). */
  readonly cursor: string | null
  /** Whether the view scrolls while the pointer is held at the canvas's edge (move, marquee, resize). */
  readonly edgeScroll?: boolean
  move(ctx: GestureContext, input: PointerInput): void
  /** The release; `dragged` is false for a release before the threshold (a click). */
  up(ctx: GestureContext, input: PointerInput, dragged: boolean): void
  /** Puts back what the gesture changed (the canvas drops the preview after). */
  cancel(ctx: GestureContext): void
}

/** A connector end as `connector.add`, `connector.bind` and `connector.getRoute` take it. */
export const endParams = (end: EndTarget): EndParam =>
  end.element ? (end.anchor ? { element: end.element, anchor: end.anchor } : { element: end.element }) : { point: [end.point.x, end.point.y] }

/** The hint for an end over `end`: its element as the target, the midpoint it snapped to. */
export const targetHint = (end: EndTarget): ConnectHint | null => (end.element ? { element: end.element, side: end.side, mode: 'target' } : null)

/** The element ids a command's answer names as made. */
export function createdOf(answer: Record<string, unknown> | null): string[] {
  const ids = answer?.created
  return Array.isArray(ids) ? ids.filter((id): id is string => typeof id === 'string') : []
}
