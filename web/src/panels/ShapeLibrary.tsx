/**
 * The shape library: every shape of `shapeType` in the library's four groups (basic, flowchart,
 * arrows, marks), with a search by name. The groups and each shape's picture are the engine's
 * (`shape.previews`: the renderer's own outline of each shape, asked for once when the board opens,
 * `board/sync.ts`), so the panel has no shape table or drawing of its own and cannot disagree with
 * the canvas. A click makes the shape the shape tool's; the rail's shape slot lights for it
 * (`railId`). A cell dragged out of the library and let go over the canvas makes that shape where
 * it was let go (plan "Board usable by hand", rule 9): the canvas previews the box while the
 * pointer is over it (`onDrag`).
 *
 * Folded (plan stage 2: something is selected or a drawing tool is in hand, so the style sections
 * come first), the library is one row of the same cells in the groups' order, scrolled sideways,
 * without the search; its header's button opens it whole again until the selection or tool changes.
 */
import { useLayoutEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react'
import { createPortal } from 'react-dom'
import { ICON_STROKE, PanelSection, useKitEditor } from '@kition/sdk/editor'
import { ChevronDownIcon, ChevronUpIcon } from '@kition/tessera/icons/react'

/** `shape.previews`'s answer. */
export type Previews = { groups: { id: string; shapes: { shape: string; svg: string }[] }[] }

/** A library cell dragged over the page: where the pointer is (client coordinates), or its end. */
export type ShapeDrag = { shape: string; clientX: number; clientY: number; phase: 'move' | 'drop' | 'cancel' }

export function isPreviews(value: unknown): value is Previews {
  const groups = (value as { groups?: unknown } | null)?.groups
  return Array.isArray(groups) && groups.every((group) => typeof group?.id === 'string' && Array.isArray(group.shapes))
}

/** How far (screen pixels) a pressed cell moves before it is dragged rather than clicked. */
const DRAG_THRESHOLD = 4

/**
 * A shape's picture: the engine's SVG, put into the page as the canvas puts the engine's fragments
 * (`board/layer.ts`). The string comes only from the engine's answer.
 */
function Picture({ svg }: { svg: string }) {
  const node = useRef<HTMLSpanElement>(null)
  useLayoutEffect(() => {
    if (node.current) node.current.innerHTML = svg
  }, [svg])
  return <span ref={node} className="board-library-picture" aria-hidden="true" />
}

export function ShapeLibrary({ previews, active, folded, onPick, onDrag }: { previews: Previews | null; active: string | null; folded: boolean; onPick: (shape: string) => void; onDrag: (drag: ShapeDrag) => void }) {
  const { t } = useKitEditor()
  const [search, setSearch] = useState('')
  // Opened whole from the folded row; folded again when the reason to fold changes.
  const [opened, setOpened] = useState(false)
  const [foldedBefore, setFoldedBefore] = useState(folded)
  if (foldedBefore !== folded) {
    setFoldedBefore(folded)
    setOpened(false)
  }
  const row = folded && !opened
  const rowRef = useRef<HTMLDivElement>(null)
  // The press on a cell, until it is let go: a drag once it moved far enough.
  const press = useRef<{ shape: string; svg: string; x: number; y: number; pointerId: number; dragging: boolean } | null>(null)
  const dragged = useRef(false)
  const [ghost, setGhost] = useState<{ svg: string; x: number; y: number } | null>(null)

  // The groups with the shapes whose name holds the search, in the person's language.
  const shown = useMemo(() => {
    const wanted = row ? '' : search.trim().toLocaleLowerCase()
    return (previews?.groups ?? [])
      .map((group) => ({ id: group.id, shapes: group.shapes.filter(({ shape }) => !wanted || t(`shape.${shape}`).toLocaleLowerCase().includes(wanted)) }))
      .filter((group) => group.shapes.length > 0)
  }, [previews, row, search, t])

  // The folded row shows the active shape.
  useLayoutEffect(() => {
    const strip = rowRef.current
    const cell = strip?.querySelector<HTMLElement>('[aria-pressed="true"]')
    if (!strip || !cell) return
    if (cell.offsetLeft < strip.scrollLeft || cell.offsetLeft + cell.offsetWidth > strip.scrollLeft + strip.clientWidth) strip.scrollLeft = cell.offsetLeft - strip.clientWidth / 2 + cell.offsetWidth / 2
  }, [active, row])

  const onPointerDown = (event: ReactPointerEvent<HTMLButtonElement>, shape: string, svg: string) => {
    if (event.button !== 0) return
    press.current = { shape, svg, x: event.clientX, y: event.clientY, pointerId: event.pointerId, dragging: false }
    event.currentTarget.setPointerCapture?.(event.pointerId)
  }
  const onPointerMove = (event: ReactPointerEvent<HTMLButtonElement>) => {
    const now = press.current
    if (!now || now.pointerId !== event.pointerId) return
    if (!now.dragging && Math.hypot(event.clientX - now.x, event.clientY - now.y) < DRAG_THRESHOLD) return
    now.dragging = true
    setGhost({ svg: now.svg, x: event.clientX, y: event.clientY })
    onDrag({ shape: now.shape, clientX: event.clientX, clientY: event.clientY, phase: 'move' })
  }
  const end = (event: ReactPointerEvent<HTMLButtonElement>, phase: 'drop' | 'cancel') => {
    const now = press.current
    if (!now || now.pointerId !== event.pointerId) return
    press.current = null
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId)
    if (!now.dragging) return
    // The click that follows the release is not a pick.
    dragged.current = true
    setGhost(null)
    onDrag({ shape: now.shape, clientX: event.clientX, clientY: event.clientY, phase })
  }

  const cell = (shape: string, svg: string) => (
    <button
      key={shape}
      type="button"
      className="board-library-shape"
      aria-pressed={active === shape}
      aria-label={t(`shape.${shape}`)}
      title={t(`shape.${shape}`)}
      onClick={() => {
        if (dragged.current) {
          dragged.current = false
          return
        }
        onPick(shape)
      }}
      onPointerDown={(event) => onPointerDown(event, shape, svg)}
      onPointerMove={onPointerMove}
      onPointerUp={(event) => end(event, 'drop')}
      onPointerCancel={(event) => end(event, 'cancel')}
      onDragStart={(event) => event.preventDefault()}
      data-testid={`board-library-${shape}`}
    >
      <Picture svg={svg} />
    </button>
  )

  const body = row ? (
    <div ref={rowRef} className="board-library-row" role="group" aria-label={t('shapeLibrary')} data-testid="board-shape-library-row">
      {shown.flatMap((group) => group.shapes.map(({ shape, svg }) => cell(shape, svg)))}
    </div>
  ) : (
    <div className="board-library" data-testid="board-shape-library-body">
      <input
        type="search"
        className="kui-field board-shape-search"
        value={search}
        placeholder={t('shapeSearch')}
        aria-label={t('shapeSearch')}
        onChange={(event) => setSearch(event.target.value)}
        data-testid="board-shape-search"
      />
      {previews && !shown.length ? <div className="board-library-none kui-muted">{t('shapeNone')}</div> : null}
      {shown.map((group) => (
        <div key={group.id} className="board-library-group" role="group" aria-label={t(`shapeGroup.${group.id}`)} data-testid={`board-shape-group-${group.id}`}>
          <div className="board-library-heading kui-muted">{t(`shapeGroup.${group.id}`)}</div>
          <div className="board-library-grid">{group.shapes.map(({ shape, svg }) => cell(shape, svg))}</div>
        </div>
      ))}
    </div>
  )

  const toggle = folded ? (
    <button type="button" className="board-library-toggle" aria-label={opened ? t('shapeLibraryFold') : t('shapeLibraryOpen')} title={opened ? t('shapeLibraryFold') : t('shapeLibraryOpen')} aria-expanded={opened} onClick={() => setOpened((now) => !now)} data-testid="board-shapes-toggle">
      {opened ? <ChevronUpIcon size={14} strokeWidth={ICON_STROKE} aria-hidden="true" /> : <ChevronDownIcon size={14} strokeWidth={ICON_STROKE} aria-hidden="true" />}
    </button>
  ) : null

  return (
    <>
      <PanelSection id="panel:shapes" title={t('shapeLibrary')} testId="board-shape-library" actions={toggle}>
        <div className="board-library-frame" data-folded={row ? 'true' : 'false'} data-testid="board-shape-library-state">
          {body}
        </div>
      </PanelSection>
      {ghost
        ? createPortal(
            <div className="board-library-ghost" style={{ left: ghost.x, top: ghost.y }} aria-hidden="true">
              <Picture svg={ghost.svg} />
            </div>,
            document.body,
          )
        : null}
    </>
  )
}
