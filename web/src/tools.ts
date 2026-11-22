/**
 * The editor's tools (the tool rail and their letters): select, hand, the 18 shapes in one slot,
 * sticky note, text, connector, pen, highlighter and eraser in one slot, frame, and picture. All 44
 * shapes are in the library panel (`panels/ShapeLibrary.tsx`), whose groups come from the engine.
 *
 * Ported from Kition's `packages/board-editor/src/components/WhiteboardToolbar.tsx` (`TOOLS`, the
 * shape button and its palette, the picture button) and `WhiteboardShapePalette.tsx`
 * (`WHITEBOARD_PALETTE_SHAPE_TYPES`), on kit-editor's `ToolRail` instead of a floating bar: a slot's
 * flyout replaces the shape palette.
 */
import { ArrowDownIcon as ArrowDown, ArrowLeftIcon as ArrowLeft, ArrowRightIcon as ArrowRight, ArrowUpIcon as ArrowUp, CheckIcon as Check, CircleIcon as Circle, CloudIcon as Cloud, DiamondIcon as Diamond, EraserIcon as Eraser, FileSymlinkIcon as FileSymlink, FrameIcon as Frame, HandIcon as Hand, HeartIcon as Heart, HexagonIcon as Hexagon, HighlighterIcon as Highlighter, ImageIcon, Link2Icon as Link2, MinusIcon as Minus, MousePointer2Icon as MousePointer2, MoveUpRightIcon as MoveUpRight, NetworkIcon as Network, ParallelogramIcon as Parallelogram, PencilIcon as Pencil, PillIcon as Pill, SquareIcon as Square, SquareCheckIcon as SquareCheck, SquareXIcon as SquareX, StarIcon as Star, StickyNoteIcon as StickyNote, TriangleIcon as Triangle, TypeIcon as Type, type IconComponent } from '@kition/tessera/icons/react'

/** What a pointer gesture on the canvas does. */
export type Tool = 'select' | 'hand' | 'shape' | 'note' | 'mindmap' | 'text' | 'connector' | 'pen' | 'highlight' | 'eraser' | 'frame'

/** The 18 shapes of `shapeType`, in the palette's order (the engine's `SHAPE_TYPES`). */
export const SHAPES: ReadonlyArray<{ shape: string; icon: IconComponent }> = [
  { shape: 'rectangle', icon: Square },
  { shape: 'ellipse', icon: Circle },
  { shape: 'triangle', icon: Triangle },
  { shape: 'diamond', icon: Diamond },
  { shape: 'hexagon', icon: Hexagon },
  { shape: 'pill', icon: Pill },
  { shape: 'parallelogram', icon: Parallelogram },
  { shape: 'star', icon: Star },
  { shape: 'cloud', icon: Cloud },
  { shape: 'heart', icon: Heart },
  { shape: 'x-box', icon: SquareX },
  { shape: 'check-box', icon: SquareCheck },
  { shape: 'check', icon: Check },
  { shape: 'arrow-left', icon: ArrowLeft },
  { shape: 'arrow-right', icon: ArrowRight },
  { shape: 'arrow-up', icon: ArrowUp },
  { shape: 'arrow-down', icon: ArrowDown },
  { shape: 'line', icon: Minus },
]

/** A rail entry: `id` is the rail's (`shape:<type>` for a shape), `tool` what the canvas does. */
export type BoardTool = { id: string; label: string; icon: IconComponent; shortcut: string | null; group?: number; tool: Tool | 'image' | 'card' | 'link' }

/** The rail, top to bottom; `label` is a key of the editor's strings. */
export const TOOLS: readonly BoardTool[] = [
  { id: 'select', label: 'toolSelect', icon: MousePointer2, shortcut: 'V', tool: 'select' },
  { id: 'hand', label: 'toolHand', icon: Hand, shortcut: 'H', tool: 'hand' },
  ...SHAPES.map(({ shape, icon }) => ({ id: `shape:${shape}`, label: `shape.${shape}`, icon, shortcut: 'R', group: 1, tool: 'shape' as const })),
  { id: 'note', label: 'toolNote', icon: StickyNote, shortcut: 'N', tool: 'note' },
  { id: 'mindmap', label: 'toolMindMap', icon: Network, shortcut: 'M', tool: 'mindmap' },
  { id: 'text', label: 'toolText', icon: Type, shortcut: 'T', tool: 'text' },
  { id: 'connector', label: 'toolConnector', icon: MoveUpRight, shortcut: 'A', tool: 'connector' },
  { id: 'pen', label: 'toolPen', icon: Pencil, shortcut: 'P', group: 2, tool: 'pen' },
  { id: 'highlight', label: 'toolHighlight', icon: Highlighter, shortcut: 'L', group: 2, tool: 'highlight' },
  { id: 'eraser', label: 'toolEraser', icon: Eraser, shortcut: 'E', group: 2, tool: 'eraser' },
  { id: 'frame', label: 'toolFrame', icon: Frame, shortcut: 'F', tool: 'frame' },
  { id: 'image', label: 'toolImage', icon: ImageIcon, shortcut: null, tool: 'image' },
  { id: 'card', label: 'toolCard', icon: FileSymlink, shortcut: null, group: 3, tool: 'card' },
  { id: 'link', label: 'toolLink', icon: Link2, shortcut: null, group: 3, tool: 'link' },
]

/** The tool a letter picks (the shape letter picks the last shape used). */
export const TOOL_KEYS: Readonly<Record<string, Tool>> = { v: 'select', h: 'hand', r: 'shape', n: 'note', m: 'mindmap', t: 'text', a: 'connector', p: 'pen', l: 'highlight', e: 'eraser', f: 'frame' }

const RAIL_SHAPES = new Set(SHAPES.map(({ shape }) => shape))

/**
 * The rail id of what the canvas does now. A shape from the library panel that is not one of the
 * rail's 18 lights the shape slot (as its rectangle).
 */
export function railId(tool: Tool, shapeType: string) {
  return tool === 'shape' ? `shape:${RAIL_SHAPES.has(shapeType) ? shapeType : 'rectangle'}` : tool
}

/** A highlighter stroke's look (v1 `DEFAULT_WHITEBOARD_HIGHLIGHT_STYLE`). */
export const HIGHLIGHT_STYLE = { strokeColor: 'yellow', strokeSize: 'xl', opacity: 0.45 } as const

/** The pictures the picture tool takes. */
export const PICTURE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg']
