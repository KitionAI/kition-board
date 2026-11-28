/**
 * The selection bar (plan "Board usable by hand", stage 2; findings F11.1, F11.3, F11.5, F6.11): a
 * bar that floats over the canvas next to the selection with the styles the selected elements
 * take, so nothing needs the side panel for everyday changes. It sits above the selection, clear of
 * its handles (the rotate and connection handles), centred on it, and goes below when there is no
 * room above or when above it would cover another element and below would not. The canvas hides
 * it while a gesture drags (move, resize, rotate, draw) and while a text box is open (the text box
 * has its own bar).
 *
 * The controls follow the selection, as `element.getStyle` answers its fields: the fill colour; the
 * line colour with the dashes (a shape's text colour is its line colour, `strokeColor`, so a shape
 * shows one "text and line colour"; a text shows its text colour); the text size (smaller, larger,
 * through the text box's sizes); a connector's route (straight, elbow, curved) and its two
 * arrowheads; with several elements, align and distribute; and last a "more" menu with Duplicate,
 * Lock or Unlock, the stacking order and Delete. Each control is one engine command, one undo step.
 * A locked selection shows only the menu. After a choice the keys go back to the canvas.
 *
 * Its place and contents are the behaviour of Drawnix's popup toolbar (MIT, `popup-toolbar.tsx`;
 * plan's gap-drawnix.md section 2), placed above the selection as the plan says; no code is taken
 * from it. The controls are kit-editor's (`MenuButton`, `MenuItem`) and the board's swatches.
 */
import { useLayoutEffect, useRef, useState, useSyncExternalStore, type ReactNode } from 'react'
import { ICON_STROKE, MenuButton, MenuItem, MenuSeparator, type Strings } from '@kition/sdk/editor'
import {
  AArrowDownIcon,
  AArrowUpIcon,
  AlignCenterHorizontalIcon,
  AlignCenterVerticalIcon,
  AlignEndHorizontalIcon,
  AlignEndVerticalIcon,
  AlignHorizontalJustifyCenterIcon,
  AlignStartHorizontalIcon,
  AlignStartVerticalIcon,
  AlignVerticalJustifyCenterIcon,
  ArrowDownIcon,
  ArrowLeftIcon,
  ArrowRightIcon,
  ArrowUpIcon,
  BaselineIcon,
  BringToFrontIcon,
  CopyPlusIcon,
  CornerDownRightIcon,
  DashDashedIcon,
  DashDottedIcon,
  DashSolidIcon,
  EllipsisIcon,
  LockIcon,
  LockOpenIcon,
  MoveUpRightIcon,
  PaintBucketIcon,
  PenLineIcon,
  SendToBackIcon,
  SplineIcon,
  TrashIcon,
  type IconComponent,
} from '@kition/tessera/icons/react'
import type { Rect } from '../board/geometry'
import { ARROWHEADS, CONNECTOR_TYPES, DASH_STYLES, type Style, type StyleAnswer, type StyleValue, type Swatch } from '../board/style'
import type { ViewportStore } from './Minimap'
import { stepSize, strokeCss } from './TextEditor'

/** How far the bar stays from what it clears (screen pixels), and from the canvas's edges. */
const BAR_GAP = 8
const EDGE = 4

/** What the selection is, for the bar's controls. */
export type BarSelection = {
  ids: readonly string[]
  /** The selected elements' kinds, as many as there are elements. */
  kinds: readonly string[]
  /** Something in the selection, or inside it, is locked. */
  locked: boolean
  /** Every selected element is locked. */
  allLocked: boolean
}

export type BarActions = {
  style: (style: Style) => void
  align: (edge: (typeof ALIGN_EDGES)[number]) => void
  distribute: (axis: 'horizontal' | 'vertical') => void
  duplicate: () => void
  lock: (locked: boolean) => void
  order: (placement: 'front' | 'forward' | 'backward' | 'back') => void
  remove: () => void
}

export const ALIGN_EDGES = ['left', 'center', 'right', 'top', 'middle', 'bottom'] as const
const ALIGN_ICONS: Record<(typeof ALIGN_EDGES)[number], IconComponent> = {
  left: AlignStartVerticalIcon,
  center: AlignCenterVerticalIcon,
  right: AlignEndVerticalIcon,
  top: AlignStartHorizontalIcon,
  middle: AlignCenterHorizontalIcon,
  bottom: AlignEndHorizontalIcon,
}
const DASH_ICONS = { solid: DashSolidIcon, dashed: DashDashedIcon, dotted: DashDottedIcon } as const
const ROUTE_ICONS = { straight: MoveUpRightIcon, elbow: CornerDownRightIcon, curved: SplineIcon } as const
const ORDER = [
  ['front', BringToFrontIcon, 'menuFront'],
  ['forward', ArrowUpIcon, 'menuForward'],
  ['backward', ArrowDownIcon, 'menuBackward'],
  ['back', SendToBackIcon, 'menuBack'],
] as const

/** A screen rectangle. */
type Box = { x0: number; y0: number; x1: number; y1: number }

const overlap = (a: Box, b: Box) => Math.max(0, Math.min(a.x1, b.x1) - Math.max(a.x0, b.x0)) * Math.max(0, Math.min(a.y1, b.y1) - Math.max(a.y0, b.y0))

/**
 * Where the bar goes (screen pixels, the canvas's corner at 0, 0): centred on the selection `sel`,
 * `clear` pixels plus a gap above it, else below it. Above is taken when it fits in the canvas and
 * covers none of `others`, else below when that fits and covers none; else above or below what it
 * would cover (moved up past those elements, or down past them, a few times at most), else whichever
 * place that fits covers least (above on a tie), else above held inside the canvas.
 */
export function placeBar(sel: Box, clear: number, bar: { w: number; h: number }, canvas: { w: number; h: number }, others: (area: Box) => readonly Box[]): { left: number; top: number; side: 'above' | 'below' } {
  const left = Math.max(EDGE, Math.min((sel.x0 + sel.x1) / 2 - bar.w / 2, canvas.w - bar.w - EDGE))
  const boxAt = (top: number) => ({ x0: left, y0: top, x1: left + bar.w, y1: top + bar.h })
  const fits = (top: number) => top >= EDGE && top + bar.h <= canvas.h - EDGE
  const covering = (top: number) => others(boxAt(top)).filter((other) => overlap(boxAt(top), other) > 0)
  const above = sel.y0 - clear - BAR_GAP - bar.h
  const below = sel.y1 + clear + BAR_GAP
  /** From `top`, past the boxes it covers, upward (`-1`) or downward (`1`), until it covers none. */
  const past = (top: number, way: -1 | 1) => {
    let at = top
    for (let tries = 0; tries < 4 && fits(at); tries += 1) {
      const covered = covering(at)
      if (!covered.length) return at
      at = way < 0 ? Math.min(...covered.map((other) => other.y0)) - BAR_GAP - bar.h : Math.max(...covered.map((other) => other.y1)) + BAR_GAP
    }
    return fits(at) && !covering(at).length ? at : null
  }
  const plain = [
    { side: 'above' as const, top: above },
    { side: 'below' as const, top: below },
  ].filter((place) => fits(place.top))
  const clean = plain.find((place) => !covering(place.top).length)
  if (clean) return { left, ...clean }
  const up = past(above, -1)
  if (up != null) return { left, top: up, side: 'above' }
  const down = past(below, 1)
  if (down != null) return { left, top: down, side: 'below' }
  const area = (top: number) => covering(top).reduce((sum, other) => sum + overlap(boxAt(top), other), 0)
  const least = plain.reduce<(typeof plain)[number] | null>((best, place) => (best == null || area(place.top) < area(best.top) ? place : best), null)
  return least ? { left, ...least } : { left, top: Math.max(EDGE, above), side: 'above' }
}

/** The bar's own buttons keep the canvas's focus (its keys, the selection) when pressed. */
const keep = (event: { preventDefault: () => void }) => event.preventDefault()

function BarButton({ icon: Icon, label, pressed, onClick, testId, children }: { icon: IconComponent; label: string; pressed?: boolean; onClick: () => void; testId: string; children?: ReactNode }) {
  return (
    <button type="button" className="board-text-mark" aria-label={label} title={label} aria-pressed={pressed} onMouseDown={keep} onClick={onClick} data-testid={testId}>
      <Icon size={16} strokeWidth={ICON_STROKE} aria-hidden="true" />
      {children}
    </button>
  )
}

/** A bar button that opens a popover (kit-editor's `MenuButton`) of swatches or menu rows. */
function BarMenu({ icon: Icon, label, testId, color, children }: { icon: IconComponent; label: string; testId: string; color?: string | null; children: ReactNode }) {
  return (
    <MenuButton
      placement="auto"
      trigger={({ open, toggle }) => (
        <button type="button" className="board-text-mark board-bar-menu" aria-label={label} title={label} aria-haspopup="true" aria-expanded={open} onClick={toggle} data-testid={testId}>
          <Icon size={16} strokeWidth={ICON_STROKE} aria-hidden="true" />
          {color !== undefined ? <span className="board-text-color-bar" style={{ background: color ?? 'transparent' }} /> : null}
        </button>
      )}
    >
      {children}
    </MenuButton>
  )
}

function Swatches({ role, value, palette, onPick, prefix, label, t }: { role: 'stroke' | 'fill'; value: StyleValue | null | undefined; palette: readonly Swatch[]; onPick: (token: string) => void; prefix: string; label: string; t: Strings }) {
  return (
    <div className="board-swatches board-bar-swatches" role="group" aria-label={label}>
      {palette.map((swatch) => (
        <button
          key={swatch.token}
          type="button"
          className="board-swatch"
          style={{ background: `var(--kb-${role}-${swatch.token}, ${role === 'stroke' ? swatch.stroke : swatch.fill})` }}
          aria-pressed={value === swatch.token}
          aria-label={t(`color.${swatch.token}`)}
          title={t(`color.${swatch.token}`)}
          onClick={() => onPick(swatch.token)}
          data-testid={`${prefix}-${swatch.token}`}
        />
      ))}
    </div>
  )
}

const fillCss = (value: StyleValue | null | undefined, palette: readonly Swatch[]) => {
  if (typeof value !== 'string') return null
  if (value.startsWith('#')) return value
  const swatch = palette.find((item) => item.token === value)
  return `var(--kb-fill-${value}, ${swatch?.fill ?? 'transparent'})`
}

export function SelectionBar({ selection, style, anchor, clear, viewport: store, others, actions, t }: {
  selection: BarSelection
  /** `element.getStyle` of the selection (null until it answers). */
  style: StyleAnswer | null
  /** The selection's bounds on the board. */
  anchor: Rect
  /** How far beyond the selection its handles reach (screen pixels): the bar keeps clear of them. */
  clear: number
  viewport: ViewportStore
  /** The other elements' boxes (board units) that meet a board area. */
  others: (area: Rect) => readonly Rect[]
  actions: BarActions
  t: Strings
}) {
  const view = useSyncExternalStore(store.subscribe, store.get)
  const bar = useRef<HTMLDivElement>(null)
  const [size, setSize] = useState({ w: 0, h: 34 })
  useLayoutEffect(() => {
    const node = bar.current
    if (!node) return
    if (node.offsetWidth !== size.w || node.offsetHeight !== size.h) setSize({ w: node.offsetWidth, h: node.offsetHeight })
  })
  if (!view) return null
  const z = view.zoom
  const toScreen = (r: Rect): Box => ({ x0: (r.x0 - view.x) * z, y0: (r.y0 - view.y) * z, x1: (r.x1 - view.x) * z, y1: (r.y1 - view.y) * z })
  const toBoard = (b: Box): Rect => ({ x0: view.x + b.x0 / z, y0: view.y + b.y0 / z, x1: view.x + b.x1 / z, y1: view.y + b.y1 / z })
  const place = placeBar(toScreen(anchor), clear, size, { w: view.width, h: view.height }, (area) => others(toBoard(area)).map(toScreen))

  const fields = new Set(style?.fields ?? [])
  const value = (field: keyof StyleAnswer['style']) => style?.style[field]
  const palette = style?.palette ?? []
  const kinds = new Set(selection.kinds)
  const several = selection.ids.length > 1
  const styled = !selection.locked && style != null
  const onlyText = kinds.size === 1 && kinds.has('text')
  const onlyShapes = kinds.size === 1 && kinds.has('shape')
  // A shape's text colour is its line colour; a text's line colour is its text colour.
  const strokeLabel = onlyShapes ? t('textLineColor') : onlyText ? t('textColor') : t('styleStroke')
  const strokeIcon = onlyShapes || onlyText ? BaselineIcon : PenLineIcon
  const fontSize = typeof value('fontSize') === 'number' ? (value('fontSize') as number) : null

  const groups: ReactNode[] = []
  if (styled && fields.has('fillColor')) {
    groups.push(
      <BarMenu key="fill" icon={PaintBucketIcon} label={t('styleFill')} testId="board-bar-fill" color={fillCss(value('fillColor'), palette)}>
        <Swatches role="fill" value={value('fillColor')} palette={palette} onPick={(token) => actions.style({ fillColor: token })} prefix="board-bar-fill" label={t('styleFill')} t={t} />
      </BarMenu>,
    )
  }
  if (styled && fields.has('strokeColor')) {
    const stroke = value('strokeColor')
    groups.push(
      <BarMenu key="stroke" icon={strokeIcon} label={strokeLabel} testId="board-bar-stroke" color={typeof stroke === 'string' ? strokeCss(stroke, palette) : null}>
        <Swatches role="stroke" value={stroke} palette={palette} onPick={(token) => actions.style({ strokeColor: token })} prefix="board-bar-stroke" label={strokeLabel} t={t} />
        {fields.has('dashStyle') ? (
          <div className="board-bar-row" role="group" aria-label={t('styleDash')}>
            {DASH_STYLES.map((dash) => (
              <BarButton key={dash} icon={DASH_ICONS[dash]} label={t(`dashStyle.${dash}`)} pressed={value('dashStyle') === dash} onClick={() => actions.style({ dashStyle: dash })} testId={`board-bar-dash-${dash}`} />
            ))}
          </div>
        ) : null}
      </BarMenu>,
    )
  }
  if (styled && fields.has('fontSize')) {
    const shown = fontSize ?? 16
    groups.push(
      <span key="size" className="board-bar-group" role="group" aria-label={t('textSize')}>
        <BarButton icon={AArrowDownIcon} label={t('textSmaller')} onClick={() => actions.style({ fontSize: stepSize(shown, -1) })} testId="board-bar-size-down" />
        <span className="board-text-size" title={t('textSize')} data-testid="board-bar-size">
          {fontSize == null ? '–' : Math.round(fontSize)}
        </span>
        <BarButton icon={AArrowUpIcon} label={t('textLarger')} onClick={() => actions.style({ fontSize: stepSize(shown, 1) })} testId="board-bar-size-up" />
      </span>,
    )
  }
  if (styled && fields.has('connectorType')) {
    groups.push(
      <span key="route" className="board-bar-group" role="group" aria-label={t('styleRoute')}>
        {CONNECTOR_TYPES.map((route) => (
          <BarButton key={route} icon={ROUTE_ICONS[route]} label={t(`route.${route}`)} pressed={value('connectorType') === route} onClick={() => actions.style({ connectorType: route })} testId={`board-bar-route-${route}`} />
        ))}
      </span>,
    )
  }
  if (styled && fields.has('startArrowhead')) {
    for (const [end, field, icon] of [
      ['start', 'startArrowhead', ArrowLeftIcon],
      ['end', 'endArrowhead', ArrowRightIcon],
    ] as const) {
      const label = t(end === 'start' ? 'barStartArrowhead' : 'barEndArrowhead')
      groups.push(
        <BarMenu key={end} icon={icon} label={label} testId={`board-bar-${end}`}>
          {ARROWHEADS.map((head) => (
            <MenuItem key={head} label={t(`arrowhead.${head}`)} checked={value(field) === head} onSelect={() => actions.style({ [field]: head })} testId={`board-bar-${end}-${head}`} />
          ))}
        </BarMenu>,
      )
    }
  }
  if (several && !selection.locked) {
    groups.push(
      <BarMenu key="align" icon={AlignStartVerticalIcon} label={t('barAlign')} testId="board-bar-align">
        {ALIGN_EDGES.map((edge) => (
          <MenuItem key={edge} icon={ALIGN_ICONS[edge]} label={t(`barAlign.${edge}`)} onSelect={() => actions.align(edge)} testId={`board-bar-align-${edge}`} />
        ))}
        <MenuSeparator />
        <MenuItem icon={AlignHorizontalJustifyCenterIcon} label={t('barDistribute.horizontal')} disabled={selection.ids.length < 3} onSelect={() => actions.distribute('horizontal')} testId="board-bar-distribute-horizontal" />
        <MenuItem icon={AlignVerticalJustifyCenterIcon} label={t('barDistribute.vertical')} disabled={selection.ids.length < 3} onSelect={() => actions.distribute('vertical')} testId="board-bar-distribute-vertical" />
      </BarMenu>,
    )
  }
  groups.push(
    <BarMenu key="more" icon={EllipsisIcon} label={t('barMore')} testId="board-bar-more">
      <MenuItem icon={CopyPlusIcon} label={t('menuDuplicate')} onSelect={actions.duplicate} testId="board-bar-duplicate" />
      {selection.allLocked ? (
        <MenuItem icon={LockOpenIcon} label={t('menuUnlock')} onSelect={() => actions.lock(false)} testId="board-bar-unlock" />
      ) : (
        <MenuItem icon={LockIcon} label={t('menuLock')} onSelect={() => actions.lock(true)} testId="board-bar-lock" />
      )}
      <MenuSeparator />
      {ORDER.map(([placement, icon, label]) => (
        <MenuItem key={placement} icon={icon} label={t(label)} onSelect={() => actions.order(placement)} testId={`board-bar-order-${placement}`} />
      ))}
      <MenuSeparator />
      <MenuItem icon={TrashIcon} label={t('menuDelete')} disabled={selection.locked} onSelect={actions.remove} testId="board-bar-delete" />
    </BarMenu>,
  )

  return (
    <div
      ref={bar}
      className="board-text-marks board-selection-bar"
      role="toolbar"
      aria-label={t('selectionBar')}
      style={{ left: place.left, top: place.top }}
      data-side={place.side}
      data-testid="board-selection-bar"
      onPointerDown={(event) => event.stopPropagation()}
      onDoubleClick={(event) => event.stopPropagation()}
    >
      {groups.flatMap((group, at) => (at ? [<span key={`sep-${at}`} className="board-text-sep" aria-hidden="true" />, group] : [group]))}
    </div>
  )
}
