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
import type { BoardSync } from '../../board/sync'
import type { Tool } from '../../tools'

/** How far (screen pixels) the pointer goes before a press becomes a drag. */
export const DRAG_THRESHOLD = 4
/** The smallest drawn box (screen pixels); a smaller one is a click's default box. */
export const MIN_DRAW = 6

/** Where a connector end goes: an element (and where on it) or a free point. */
export type EndTarget = { element: string; anchor: [number, number] | null; point: Point } | { element: null; point: Point }

/** What the overlay draws for the gesture under way. */
export type Draft =
  | { type: 'rect'; box: Rect; dashed: boolean }
  | { type: 'lines'; lines: Point[][]; target: string | null }
  | { type: 'pen'; points: Point[]; highlight: boolean }

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
  setTool: (tool: Tool) => void
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
  /** A connector end over `p`. */
  endAt: (p: Point, input: { clientX: number; clientY: number }, excluding: string | null) => EndTarget
}

export type GestureType = 'pan' | 'move' | 'resize' | 'scale' | 'rotate' | 'brush' | 'draw' | 'connect' | 'terminal' | 'pen' | 'erase' | 'text'

export interface Gesture {
  readonly type: GestureType
  /** Whether the press waits for `DRAG_THRESHOLD` before it drags (false: it drags at once). */
  readonly threshold: boolean
  /** The canvas's cursor while it drags (null: the tool's). */
  readonly cursor: string | null
  move(ctx: GestureContext, input: PointerInput): void
  /** The release; `dragged` is false for a release before the threshold (a click). */
  up(ctx: GestureContext, input: PointerInput, dragged: boolean): void
  /** Puts back what the gesture changed (the canvas drops the preview after). */
  cancel(ctx: GestureContext): void
}

/** A connector end as `connector.add` and `connector.bind` take it. */
export const endParams = (end: EndTarget) => (end.element ? (end.anchor ? { element: end.element, anchor: end.anchor } : { element: end.element }) : { point: [end.point.x, end.point.y] })

/** The element ids a command's answer names as made. */
export function createdOf(answer: Record<string, unknown> | null): string[] {
  const ids = answer?.created
  return Array.isArray(ids) ? ids.filter((id): id is string => typeof id === 'string') : []
}
