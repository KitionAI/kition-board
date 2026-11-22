/**
 * Element styles as the editor offers them: the choices of each style field, which fields each
 * kind takes (the engine's `crates/engine/model/src/fields.rs`, for the styles the editor gives new
 * elements), and the engine's `element.getStyle` answer.
 *
 * Ported from Kition's `packages/board-editor/src/lib/whiteboardStyle.ts` (the token, fill, dash
 * and size lists, `DEFAULT_WHITEBOARD_STYLE`); the colours behind the tokens come from the engine.
 */
import type { Kind } from './model'

export type StyleValue = string | number
/** Style fields and their values (`element.style`'s `style`). */
export type Style = Partial<Record<StyleField, StyleValue>>

export const STYLE_FIELDS = [
  'strokeColor',
  'fillColor',
  'fillStyle',
  'dashStyle',
  'strokeSize',
  'opacity',
  'fontSize',
  'fontFamily',
  'textAlign',
  'connectorType',
  'startArrowhead',
  'endArrowhead',
] as const
export type StyleField = (typeof STYLE_FIELDS)[number]

export const FILL_STYLES = ['none', 'solid', 'semi', 'pattern'] as const
export const DASH_STYLES = ['solid', 'dashed', 'dotted'] as const
export const STROKE_SIZES = ['s', 'm', 'l', 'xl'] as const
export const FONT_FAMILIES = ['sans', 'serif', 'mono'] as const
export const TEXT_ALIGNS = ['left', 'center', 'right'] as const
export const CONNECTOR_TYPES = ['straight', 'elbow', 'curved'] as const
export const ARROWHEADS = ['none', 'arrow', 'dot', 'triangle', 'triangle-open', 'diamond', 'diamond-open', 'circle-open', 'bar'] as const

/** The style fields each kind takes (fields.rs `kind_fields`, style fields only). */
const USES: Record<Kind, readonly StyleField[]> = {
  shape: ['strokeColor', 'fillColor', 'fillStyle', 'dashStyle', 'strokeSize', 'opacity', 'fontSize', 'fontFamily', 'textAlign'],
  text: ['fontSize', 'fontFamily', 'textAlign', 'strokeColor', 'opacity'],
  stroke: ['strokeColor', 'strokeSize', 'opacity'],
  connector: ['connectorType', 'startArrowhead', 'endArrowhead', 'dashStyle', 'strokeSize', 'strokeColor', 'opacity'],
  image: ['opacity'],
  card: ['fillColor', 'strokeColor'],
  frame: ['fillColor', 'strokeColor'],
  group: [],
}

/** The fields of `style` an element of `kind` takes. */
export function styleFor(kind: Kind, style: Style): Style {
  const out: Style = {}
  for (const field of USES[kind]) if (style[field] != null) out[field] = style[field]
  return out
}

/** One colour token and the colours it stands for (from the engine). */
export type Swatch = { token: string; stroke: string; fill: string }

/** `element.getStyle`'s answer: the fields used, each one's value (`null`: the elements differ). */
export type StyleAnswer = { fields: StyleField[]; style: Partial<Record<StyleField, StyleValue | null>>; palette: Swatch[] }
