/**
 * The shape library: every shape of `shapeType` in the library's four groups (basic, flowchart,
 * arrows, marks), with a search by name. The groups and each shape's picture are the engine's
 * (`shape.previews`: the renderer's own outline of each shape), so the panel has no shape table or
 * drawing of its own and cannot disagree with the canvas. A click makes the shape the shape tool's;
 * the rail's shape slot lights for it (`railId`).
 */
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { PanelSection, useKitEditor } from '@kition/sdk/editor'

/** `shape.previews`'s answer. */
type Previews = { groups: { id: string; shapes: { shape: string; svg: string }[] }[] }

/** How often the panel asks again while the engine has not opened the board yet (every 200 ms). */
const TRIES = 100

function isPreviews(value: unknown): value is Previews {
  const groups = (value as { groups?: unknown } | null)?.groups
  return Array.isArray(groups) && groups.every((group) => typeof group?.id === 'string' && Array.isArray(group.shapes))
}

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

export function ShapeLibrary({ active, onPick }: { active: string | null; onPick: (shape: string) => void }) {
  const { t, query } = useKitEditor()
  const [previews, setPreviews] = useState<Previews | null>(null)
  const [tries, setTries] = useState(0)
  const [search, setSearch] = useState('')

  // Asked once; again only while the engine has not answered (the board is still opening).
  useEffect(() => {
    if (previews) return undefined
    let current = true
    let timer: number | undefined
    void query<Previews>('shape.previews').then((answer) => {
      if (!current) return
      if (isPreviews(answer)) setPreviews(answer)
      else if (tries < TRIES) timer = window.setTimeout(() => setTries((n) => n + 1), 200)
    })
    return () => {
      current = false
      if (timer !== undefined) window.clearTimeout(timer)
    }
  }, [previews, query, tries])

  // The groups with the shapes whose name holds the search, in the person's language.
  const shown = useMemo(() => {
    const wanted = search.trim().toLocaleLowerCase()
    return (previews?.groups ?? [])
      .map((group) => ({ id: group.id, shapes: group.shapes.filter(({ shape }) => !wanted || t(`shape.${shape}`).toLocaleLowerCase().includes(wanted)) }))
      .filter((group) => group.shapes.length > 0)
  }, [previews, search, t])

  const body = useMemo(
    () => (
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
            <div className="board-library-grid">
              {group.shapes.map(({ shape, svg }) => (
                <button
                  key={shape}
                  type="button"
                  className="board-library-shape"
                  aria-pressed={active === shape}
                  aria-label={t(`shape.${shape}`)}
                  title={t(`shape.${shape}`)}
                  onClick={() => onPick(shape)}
                  data-testid={`board-library-${shape}`}
                >
                  <Picture svg={svg} />
                </button>
              ))}
            </div>
          </div>
        ))}
      </div>
    ),
    [active, onPick, previews, search, shown, t],
  )

  return (
    <PanelSection id="panel:shapes" title={t('shapeLibrary')} testId="board-shape-library">
      {body}
    </PanelSection>
  )
}
