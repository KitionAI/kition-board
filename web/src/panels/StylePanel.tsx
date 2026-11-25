/**
 * The style panel: line and fill colour, fill, dashes, line weight, opacity, text (size, typeface,
 * alignment) and connector route and arrowheads, for the selection or, while a drawing tool is
 * active, for what it draws next. Only the fields the elements take are shown; each one's value
 * is the engine's (`element.getStyle`: what is set, else the kind's default; none when the
 * elements differ). A change to the selection is one `element.style` (one undo step); opacity and
 * text size are sent when the slider or field is let go. When every selected element is a plain or
 * flow-chart shape, a Shape section changes their `shapeType` to any of the library's shapes, in
 * its groups (`shape.previews`, the answer the board asked for once): one `element.set` per
 * shape, so box, label, style and bound connectors stay, and the connectors' ends move to the new
 * outline in the same command (design note 1.4).
 *
 * Ported from Kition's `packages/board-editor/src/components/WhiteboardStylePanel.tsx`, on kit-editor's
 * panel and controls; the swatches' colours come from the engine's palette.
 */
import { useEffect, useState } from 'react'
import { BanIcon as Ban, CornerDownRightIcon as CornerDownRight, DashDashedIcon as DashDashed, DashDottedIcon as DashDotted, DashSolidIcon as DashSolid, Grid3x3Icon as Grid3X3, MoveUpRightIcon as MoveUpRight, SplineIcon as Spline, SquareIcon as Square, SunMediumIcon as SunMedium, TextAlignCenterIcon as TextAlignCenter, TextAlignEndIcon as TextAlignEnd, TextAlignStartIcon as TextAlignStart } from '@kition/tessera/icons/react'
import { Choice, NumberField, PanelSection, Segmented, useKitEditor, type Segment } from '@kition/sdk/editor'
import type { Kind } from '../board/model'
import { ARROWHEADS, CONNECTOR_TYPES, DASH_STYLES, FONT_FAMILIES, STROKE_SIZES, type Style, type StyleAnswer, type StyleField, type StyleValue, type Swatch } from '../board/style'

/** What the panel styles: the selected elements, or the next element a tool draws (`defaults`). */
export type StyleTarget = { ids: readonly string[] } | { kind: Kind; shapeStyle?: string; defaults: Style }

/**
 * The Shape section's state: the library's groups (each a group id and its shapeType values, in the
 * library's order) and the selected shapes' common shapeType (`null` when they differ).
 */
export type ShapeChoice = { groups: readonly { id: string; shapes: readonly string[] }[]; value: string | null }

/** A drop-down of the library's shapes in their groups (kit-editor's `Choice` markup, with groups). */
function ShapeSection({ choice, onShape }: { choice: ShapeChoice; onShape: (shape: string) => void }) {
  const { t } = useKitEditor()
  return (
    <PanelSection title={t('styleShape')} testId="board-style-shape">
      <label className="kui-choice">
        <span>{t('styleShapeType')}</span>
        <select className="kui-field" value={choice.value ?? ''} aria-label={t('styleShapeType')} onChange={(event) => event.target.value && onShape(event.target.value)} data-testid="board-style-shapeType">
          {choice.value == null ? (
            <option value="" disabled>
              {t('styleMixed')}
            </option>
          ) : null}
          {choice.groups.map((group) => (
            <optgroup key={group.id} label={t(`shapeGroup.${group.id}`)} data-group={group.id}>
              {group.shapes.map((shape) => (
                <option key={shape} value={shape}>
                  {t(`shape.${shape}`)}
                </option>
              ))}
            </optgroup>
          ))}
        </select>
      </label>
    </PanelSection>
  )
}

const FILL_ICONS = { none: Ban, solid: Square, semi: SunMedium, pattern: Grid3X3 } as const
const DASH_ICONS = { solid: DashSolid, dashed: DashDashed, dotted: DashDotted } as const
const ALIGN_ICONS = { left: TextAlignStart, center: TextAlignCenter, right: TextAlignEnd } as const
const ROUTE_ICONS = { straight: MoveUpRight, elbow: CornerDownRight, curved: Spline } as const

const key = (target: StyleTarget) => ('ids' in target ? `ids:${target.ids.join(',')}` : `kind:${target.kind}:${target.shapeStyle ?? ''}`)

function ColorRow({ field, role, value, palette, onPick, label }: { field: StyleField; role: 'stroke' | 'fill'; value: StyleValue | null | undefined; palette: readonly Swatch[]; onPick: (value: string) => void; label: string }) {
  const { t } = useKitEditor()
  const custom = typeof value === 'string' && value.startsWith('#') ? value : null
  return (
    <div className="board-swatches" role="group" aria-label={label}>
      {palette.map((swatch) => (
        <button
          key={swatch.token}
          type="button"
          className="board-swatch"
          // The token's colour in the host's scheme (app.css's --kb-* properties), the engine's light one behind it.
          style={{ background: `var(--kb-${role}-${swatch.token}, ${role === 'stroke' ? swatch.stroke : swatch.fill})` }}
          aria-pressed={value === swatch.token}
          aria-label={t(`color.${swatch.token}`)}
          title={t(`color.${swatch.token}`)}
          onClick={() => onPick(swatch.token)}
          data-testid={`board-style-${field}-${swatch.token}`}
        />
      ))}
      <label className="board-swatch board-swatch-custom" title={t('customColor')} aria-pressed={custom != null} style={custom ? { background: custom } : undefined}>
        <input type="color" value={custom ?? '#5645d4'} aria-label={t('customColor')} onChange={(event) => onPick(event.target.value)} data-testid={`board-style-${field}-custom`} />
      </label>
    </div>
  )
}

/** Text buttons of which one is pressed (line weights). */
function Chips<V extends string>({ label, value, choices, onChange, testId }: { label: string; value: StyleValue | null | undefined; choices: readonly (readonly [V, string])[]; onChange: (value: V) => void; testId: string }) {
  return (
    <span className="board-chips" role="group" aria-label={label}>
      {choices.map(([choice, text]) => (
        <button key={choice} type="button" className="board-chip" aria-pressed={value === choice} onClick={() => onChange(choice)} data-testid={`${testId}-${choice}`}>
          {text}
        </button>
      ))}
    </span>
  )
}

export function StylePanel({ target, onApply, shape, onShape }: { target: StyleTarget | null; onApply: (style: Style) => void; shape?: ShapeChoice | null; onShape?: (shape: string) => void }) {
  const { t, query, session } = useKitEditor()
  const revision = (session as { change?: { revision?: number } }).change?.revision ?? 0
  const [answer, setAnswer] = useState<{ key: string; value: StyleAnswer } | null>(null)
  const targetKey = target ? key(target) : null
  // The engine's view of the target, again after every change of the board.
  useEffect(() => {
    if (!target || !targetKey) return undefined
    let current = true
    const params = 'ids' in target ? { ids: [...target.ids] } : { kind: target.kind, ...(target.shapeStyle ? { shapeStyle: target.shapeStyle } : {}) }
    void query<StyleAnswer>('element.getStyle', params).then((value) => {
      if (current && value) setAnswer({ key: targetKey, value })
    })
    return () => {
      current = false
    }
    // `target` is read through its key.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, revision, targetKey])
  const known = answer && answer.key === targetKey ? answer.value : null
  const [opacity, setOpacity] = useState<number | null>(null)
  const [fontSize, setFontSize] = useState<number | null>(null)
  useEffect(() => {
    setOpacity(null)
    setFontSize(null)
  }, [targetKey, revision])

  if (!target) {
    return (
      <div className="board-style-empty kui-muted" data-testid="board-style-empty">
        {t('styleNothing')}
      </div>
    )
  }
  if (!known) return null
  const defaults = 'ids' in target ? null : target.defaults
  const value = (field: StyleField) => (defaults && defaults[field] != null ? defaults[field] : known.style[field])
  const has = (field: StyleField) => known.fields.includes(field)
  const set = (field: StyleField, next: StyleValue) => onApply({ [field]: next })
  const segments = <V extends string>(values: readonly V[], icons: Record<V, Segment<V>['icon']>, prefix: string): Segment<V>[] => values.map((item) => ({ value: item, icon: icons[item], label: t(`${prefix}.${item}`) }))
  const opacityShown = opacity ?? Math.round(Number(value('opacity') ?? 1) * 100)
  const fontShown = fontSize ?? Number(value('fontSize') ?? 16)

  return (
    <div className="board-style" data-testid="board-style-panel" data-target={defaults ? 'defaults' : 'selection'}>
      <div className="board-style-scope kui-muted">{defaults ? t('styleDefaults') : t('styleSelection')}</div>
      {shape && onShape && !defaults ? <ShapeSection choice={shape} onShape={onShape} /> : null}
      {has('strokeColor') ? (
        <PanelSection title={t('styleStroke')} testId="board-style-stroke">
          <ColorRow field="strokeColor" role="stroke" value={value('strokeColor')} palette={known.palette} onPick={(color) => set('strokeColor', color)} label={t('styleStroke')} />
        </PanelSection>
      ) : null}
      {has('fillColor') ? (
        <PanelSection title={t('styleFill')} testId="board-style-fill">
          <ColorRow field="fillColor" role="fill" value={value('fillColor')} palette={known.palette} onPick={(color) => set('fillColor', color)} label={t('styleFill')} />
          {has('fillStyle') ? (
            <Segmented label={t('styleFillStyle')} value={String(value('fillStyle') ?? '')} segments={segments(['none', 'solid', 'semi', 'pattern'] as const, FILL_ICONS, 'fillStyle')} onChange={(next) => set('fillStyle', next)} testId="board-style-fillStyle" />
          ) : null}
        </PanelSection>
      ) : null}
      {has('dashStyle') || has('strokeSize') || has('opacity') ? (
        <PanelSection title={t('styleLine')} testId="board-style-line">
          {has('dashStyle') ? <Segmented label={t('styleDash')} value={String(value('dashStyle') ?? '')} segments={segments(DASH_STYLES, DASH_ICONS, 'dashStyle')} onChange={(next) => set('dashStyle', next)} testId="board-style-dashStyle" /> : null}
          {has('strokeSize') ? <Chips label={t('styleSize')} value={value('strokeSize')} choices={STROKE_SIZES.map((size) => [size, size.toUpperCase()] as const)} onChange={(next) => set('strokeSize', next)} testId="board-style-strokeSize" /> : null}
          {has('opacity') ? (
            <NumberField
              label={t('styleOpacity')}
              value={opacityShown}
              min={5}
              max={100}
              step={5}
              unit="%"
              track
              onChange={(next, final) => {
                setOpacity(next)
                if (final) set('opacity', Math.round(next) / 100)
              }}
              testId="board-style-opacity"
            />
          ) : null}
        </PanelSection>
      ) : null}
      {has('fontSize') || has('fontFamily') || has('textAlign') ? (
        <PanelSection title={t('styleText')} testId="board-style-text">
          {has('fontSize') ? (
            <NumberField
              label={t('styleFontSize')}
              value={fontShown}
              min={6}
              max={400}
              step={1}
              onChange={(next, final) => {
                setFontSize(next)
                if (final) set('fontSize', Math.round(next))
              }}
              testId="board-style-fontSize"
            />
          ) : null}
          {has('fontFamily') ? <Choice label={t('styleFont')} value={String(value('fontFamily') ?? 'sans')} choices={FONT_FAMILIES.map((family) => [family, t(`font.${family}`)] as const)} onChange={(next) => set('fontFamily', next)} testId="board-style-fontFamily" /> : null}
          {has('textAlign') ? <Segmented label={t('styleAlign')} value={String(value('textAlign') ?? '')} segments={segments(['left', 'center', 'right'] as const, ALIGN_ICONS, 'align')} onChange={(next) => set('textAlign', next)} testId="board-style-textAlign" /> : null}
        </PanelSection>
      ) : null}
      {has('connectorType') || has('startArrowhead') ? (
        <PanelSection title={t('styleConnector')} testId="board-style-connector">
          {has('connectorType') ? <Segmented label={t('styleRoute')} value={String(value('connectorType') ?? '')} segments={segments(CONNECTOR_TYPES, ROUTE_ICONS, 'route')} onChange={(next) => set('connectorType', next)} testId="board-style-connectorType" /> : null}
          {has('startArrowhead') ? <Choice label={t('styleStartArrowhead')} value={String(value('startArrowhead') ?? 'none')} choices={ARROWHEADS.map((head) => [head, t(`arrowhead.${head}`)] as const)} onChange={(next) => set('startArrowhead', next)} testId="board-style-startArrowhead" /> : null}
          {has('endArrowhead') ? <Choice label={t('styleEndArrowhead')} value={String(value('endArrowhead') ?? 'arrow')} choices={ARROWHEADS.map((head) => [head, t(`arrowhead.${head}`)] as const)} onChange={(next) => set('endArrowhead', next)} testId="board-style-endArrowhead" /> : null}
        </PanelSection>
      ) : null}
    </div>
  )
}
