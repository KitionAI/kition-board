/**
 * The text box (UI-K6; plan "Board usable by hand", stage 1): a focused `<textarea>` laid over the
 * element exactly where the engine lays its text out. Its geometry is the fragment's text frame
 * (`TextFrame`: the area the lines wrap in, font, size, weight, line height, alignment, top or
 * middle), never constants of the page's own: the box is placed in the element's own frame, in
 * board units, turned with the element and scaled by the zoom, so it wraps where the engine wraps
 * whatever the zoom. It grows with what is typed and never scrolls: a text box grows downward; a
 * label stays centred in its shape while it fits and then grows downward past the shape (the shape
 * does not grow). An auto-width text grows to the right and wraps only at line breaks. While the box
 * is open the element's own text is hidden and its outline and fill stay (`ElementLayer.setEditing`).
 *
 * Typing stays local; Enter (without Shift) or leaving the box commits once, Escape cancels. An Enter
 * that belongs to an input method's composition never commits (`composingEnter`, the IME fix of
 * spike S2); the text being composed is drawn in the box like any other.
 *
 * Over a shape's label or a text (not a Markdown one), a bar floats above the element, outside it
 * (below it when there is no room above): bold (Cmd/Ctrl+B), italic (Cmd/Ctrl+I) and a link
 * (Cmd/Ctrl+K, a field for the address; an empty one removes it) on the box's selection, then the
 * text size (smaller, larger) and the text colour, which go to the element through `element.style`
 * at once (a new text takes them when it is made). The box stays a plain text box: the marks wait
 * beside the text as `{ start, end, mark, value }` in UTF-16 offsets of the current text
 * (`board/marks.ts`), move with the typing before them (placed at the caret), and are sent after the
 * text when the box commits (`text.mark`, design note 4.3); the engine's fragment shows them then. A
 * bar button is pressed when the selection lies inside a mark of its kind, stored or waiting.
 *
 * Ported from Kition's `packages/board-editor/src/components/WhiteboardTextEditor.tsx`.
 */
import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type KeyboardEvent as ReactKeyboardEvent } from 'react'
import { ICON_STROKE, isMac } from '@kition/sdk/editor'
import { AArrowDownIcon, AArrowUpIcon, BaselineIcon, BoldIcon, ItalicIcon, LinkIcon, type IconComponent } from '@kition/tessera/icons/react'
import { rotate, type Point, type Rect } from '../board/geometry'
import { composingEnter } from '../board/ime'
import { address, follow, markOver, type MarkName, type TextMark } from '../board/marks'
import type { Style, Swatch } from '../board/style'
import type { TextFrame } from '../board/sync'
import type { ViewportStore } from './Minimap'

export type TextEditing = {
  /** A new number for each box opened (the box keeps its state while it stays open). */
  session: number
  /** The element whose text this is; `null` for a new text element at `at`. */
  id: string | null
  kind: 'shape' | 'text' | 'frame' | 'connector' | 'card' | 'new-text'
  value: string
  /** The marks the text has, when the box offers the marks bar; `null`: no bar (Markdown, titles, captions). */
  marks: readonly TextMark[] | null
  /**
   * Opened by a typed character (`value` is that character): the box takes the focus at once, so
   * the characters typed after it land in it, with the caret after the text (no selection).
   */
  typed?: boolean
  /** A new text: its top-left corner on the board. */
  at?: Point
  /** A new text's fixed width (board units); null or absent: auto width. */
  width?: number | null
  /** The style a new text is made with (the drawing tools' style, then what the bar picked). */
  style?: Style
}

/** Where the box goes, in board units: the element's unrotated box, its rotation, and its text frame (local to the box). */
export type EditFrame = { box: Rect; rotation: number; text: TextFrame }

/** The bar's text size and colour: the values now, the palette, and whether the colour is also the lines'. */
export type TextStyleBar = { fontSize: number; color: string | null; palette: readonly Swatch[]; colorIsLine: boolean }

export type TextEditorLabels = { bar: string; bold: string; italic: string; link: string; address: string; size: string; smaller: string; larger: string; color: string; lineColor: string; swatch: (token: string) => string }

const ICONS: Record<MarkName, IconComponent> = { bold: BoldIcon, italic: ItalicIcon, link: LinkIcon }
const KEYS: Record<MarkName, string> = { bold: 'B', italic: 'I', link: 'K' }
/** The text sizes the bar steps through (board units). */
export const TEXT_SIZES = [8, 10, 12, 14, 16, 18, 20, 24, 28, 32, 36, 40, 48, 56, 64, 72, 80, 96, 120] as const
/** How far the bar stays from the element (screen pixels), and from the canvas's edges. */
const BAR_GAP = 8
const EDGE = 4

/** The next size up (`by` 1) or down (`by` -1) from `size` in `TEXT_SIZES`. */
export function stepSize(size: number, by: 1 | -1): number {
  if (by > 0) return TEXT_SIZES.find((step) => step > size + 0.01) ?? TEXT_SIZES[TEXT_SIZES.length - 1]
  return [...TEXT_SIZES].reverse().find((step) => step < size - 0.01) ?? TEXT_SIZES[0]
}

/** The CSS colour of a token or `#rrggbb` as the engine paints it for lines and text (app.css's --kb-stroke-*). */
export function strokeCss(value: string, palette: readonly Swatch[]): string {
  if (value.startsWith('#')) return value
  const swatch = palette.find((item) => item.token === value)
  return `var(--kb-stroke-${value}, ${swatch?.stroke ?? 'currentColor'})`
}

/** Whether `event` is the editor's own shortcut key with `letter` (Cmd on a Mac, else Ctrl). */
function shortcut(event: ReactKeyboardEvent, letter: string) {
  const primary = isMac() ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey
  return primary && !event.altKey && !event.shiftKey && (event.key.toLowerCase() === letter || event.code === `Key${letter.toUpperCase()}`)
}

let measurer: CanvasRenderingContext2D | null = null
/** The width of `text` in `font` (a placeholder's width; the typed text is measured by the box itself). */
function textWidth(text: string, font: string) {
  measurer ??= document.createElement('canvas').getContext('2d')
  if (!measurer) return 0
  measurer.font = font
  return measurer.measureText(text).width
}

export function TextEditor({
  editing,
  frame,
  viewport: store,
  placeholder,
  labels,
  styleBar,
  onStyle,
  onCommit,
  onCancel,
  commitRef,
}: {
  editing: TextEditing
  frame: EditFrame
  /** The view, followed so the box stays on the element when the view pans or zooms. */
  viewport: ViewportStore
  placeholder: string
  /** The bar's names. */
  labels: TextEditorLabels
  /** The text size and colour in the bar (null: not offered). */
  styleBar: TextStyleBar | null
  /** A size or colour picked in the bar. */
  onStyle: (style: Style) => void
  onCommit: (value: string, marks: readonly TextMark[]) => void
  onCancel: () => void
  /** Set to a function that commits the box and closes it (a press on the canvas). */
  commitRef?: { current: (() => void) | null }
}) {
  const field = useRef<HTMLTextAreaElement>(null)
  const bar = useRef<HTMLDivElement>(null)
  const linkInput = useRef<HTMLInputElement>(null)
  const composing = useRef(false)
  const done = useRef(false)
  const view = useSyncExternalStore(store.subscribe, store.get)
  const [value, setValue] = useState(editing.value)
  const [ignored, setIgnored] = useState(0)
  // The stored marks and the waiting ones, both following the typing; the box's selection.
  const marks = useRef<{ stored: TextMark[]; pending: TextMark[] }>({ stored: [...(editing.marks ?? [])], pending: [] })
  const [selection, setSelection] = useState<{ start: number; end: number }>(editing.typed ? { start: editing.value.length, end: editing.value.length } : { start: 0, end: editing.value.length })
  const [linking, setLinking] = useState<{ start: number; end: number; text: string } | null>(null)
  const [colors, setColors] = useState(false)
  const [, setTick] = useState(0)
  // What the box's text needs (board units): its lines' height, and its width when it sets its own.
  const [content, setContent] = useState<{ w: number; h: number }>({ w: frame.text.area.x1 - frame.text.area.x0, h: frame.text.lineHeight })
  const [barSize, setBarSize] = useState<{ w: number; h: number }>({ w: 0, h: 34 })
  // The focus moving between the text box and the link field is not leaving the editor, whatever
  // the browser says the next focus is. Set only when the text box has the focus and is about to
  // hand it to the field, and cleared when the field takes it, so it never swallows a later blur.
  const openingLink = useRef(false)
  const returning = useRef(false)
  const rich = editing.marks != null

  const text = frame.text
  const areaWidth = text.area.x1 - text.area.x0
  const fontCss = `${text.weight} ${text.size}px ${text.family}`

  // The box grows to what it holds: its height is its lines', its width an auto-width text's
  // widest line (measured with the box itself, so it is the browser's own layout).
  useLayoutEffect(() => {
    const box = field.current
    if (!box) return
    const before = { height: box.style.height, width: box.style.width }
    box.style.height = '0px'
    if (!text.wrap) box.style.width = '0px'
    const h = Math.max(box.scrollHeight, text.lineHeight)
    // An empty auto-width box is as wide as its placeholder; the caret takes a pixel.
    const w = text.wrap ? areaWidth : Math.max(box.scrollWidth, value ? 0 : textWidth(placeholder, fontCss)) + 1
    box.style.height = before.height
    box.style.width = before.width
    if (Math.abs(h - content.h) > 0.01 || Math.abs(w - content.w) > 0.01) setContent({ w, h })
  })

  useLayoutEffect(() => {
    const node = bar.current
    if (!node) return
    const w = node.offsetWidth
    const h = node.offsetHeight
    if (w !== barSize.w || h !== barSize.h) setBarSize({ w, h })
  })

  // Focused once the pointer press that opened the box is over: the Text tool opens a new box on
  // the press, and the press's own default action then focuses the canvas; a box focused at once
  // would lose focus to it and close empty (leaving the box commits).
  // A box opened by a key has no press to wait for: it takes the focus before the next key.
  useLayoutEffect(() => {
    if (!editing.typed) return
    const box = field.current
    box?.focus({ preventScroll: true })
    box?.setSelectionRange(box.value.length, box.value.length)
  }, [editing.typed])
  useEffect(() => {
    if (editing.typed) return undefined
    const id = window.setTimeout(() => {
      field.current?.focus({ preventScroll: true })
      field.current?.select()
    }, 0)
    return () => window.clearTimeout(id)
  }, [editing.typed])

  const finish = (commit: boolean) => {
    if (done.current) return
    done.current = true
    if (commit) onCommit(field.current?.value ?? value, marks.current.pending)
    else onCancel()
  }

  // A press on the canvas commits the box whatever has the focus.
  useEffect(() => {
    if (!commitRef) return undefined
    commitRef.current = () => finish(true)
    return () => {
      commitRef.current = null
    }
  })

  const readSelection = () => {
    const box = field.current
    if (box) setSelection({ start: box.selectionStart, end: box.selectionEnd })
  }

  /** Bold or italic over the selection: off where all of it has the mark, else on. */
  const toggle = (mark: 'bold' | 'italic') => {
    const box = field.current
    if (!box || box.selectionStart >= box.selectionEnd) return
    const { selectionStart: start, selectionEnd: end } = box
    const on = markOver(marks.current.stored, marks.current.pending, mark, start, end) != null
    marks.current.pending.push({ start, end, mark, value: on ? null : true })
    setSelection({ start, end })
    setTick((n) => n + 1)
  }

  /** Opens the link field over the selection, with the address it has. */
  const openLink = () => {
    const box = field.current
    if (!box || box.selectionStart >= box.selectionEnd) return
    const { selectionStart: start, selectionEnd: end } = box
    const current = markOver(marks.current.stored, marks.current.pending, 'link', start, end)
    // Only a focused text box gives the focus to the field (a Link button pressed from the field
    // leaves it there, and then no blur follows).
    const fromText = box.ownerDocument.activeElement === box
    openingLink.current = fromText
    setLinking({ start, end, text: current && current !== true ? current.href : '' })
    // A field already open is not mounted again, so its autoFocus does not take the focus.
    if (linking && fromText) linkInput.current?.focus()
  }

  /**
   * Back to the text box with the selection the bar worked on, before the link field goes (the
   * focus moves inside the editor, so nothing commits).
   */
  const backToText = (start: number, end: number) => {
    returning.current = true
    field.current?.focus({ preventScroll: true })
    returning.current = false
    field.current?.setSelectionRange(start, end)
    readSelection()
    setLinking(null)
  }

  const applyLink = () => {
    if (!linking) return
    const href = address(linking.text)
    marks.current.pending.push({ start: linking.start, end: linking.end, mark: 'link', value: href ? { href } : null })
    backToText(linking.start, linking.end)
  }

  /** Focus that stays inside the box or its bar is not leaving it. */
  const leaving = (next: EventTarget | null) => !(next instanceof Node && (next === field.current || bar.current?.contains(next)))

  const pressed = (mark: MarkName) => markOver(marks.current.stored, marks.current.pending, mark, selection.start, selection.end) != null
  // The text box keeps the focus when a bar button is pressed (leaving the box commits).
  const keep = (event: { preventDefault: () => void }) => event.preventDefault()
  const markButton = (mark: MarkName, onClick: () => void) => {
    const Icon = ICONS[mark]
    return (
      <button
        key={mark}
        type="button"
        className="board-text-mark"
        aria-pressed={pressed(mark)}
        aria-label={labels[mark]}
        title={`${labels[mark]} (${isMac() ? '⌘' : 'Ctrl+'}${KEYS[mark]})`}
        onMouseDown={keep}
        onClick={onClick}
        data-testid={`board-text-${mark}`}
      >
        <Icon size={16} strokeWidth={ICON_STROKE} aria-hidden="true" />
      </button>
    )
  }

  // The box in the element's frame: board units, turned about the element's centre, scaled by the zoom.
  const z = view?.zoom ?? 1
  const vx = view?.x ?? 0
  const vy = view?.y ?? 0
  const { box: elementBox, rotation } = frame
  const bw = elementBox.x1 - elementBox.x0
  const bh = elementBox.y1 - elementBox.y0
  const areaHeight = text.area.y1 - text.area.y0
  // A label centred in its area while it fits, then from the area's top, growing downward.
  const top = text.valign === 'middle' ? text.area.y0 + Math.max(0, (areaHeight - content.h) / 2) : text.area.y0
  const width = text.wrap ? areaWidth : content.w
  // An auto-width text grows from its aligned edge as the engine's box does: from the left.
  const left = text.area.x0
  const placement = `translate(${(elementBox.x0 - vx) * z}px, ${(elementBox.y0 - vy) * z}px) scale(${z})${rotation ? ` translate(${bw / 2}px, ${bh / 2}px) rotate(${rotation}deg) translate(${-bw / 2}px, ${-bh / 2}px)` : ''}`

  // The bar: above everything the box and the element cover on screen, else below; inside the canvas.
  const c = { x: bw / 2, y: bh / 2 }
  const local = [
    { x: Math.min(0, left), y: Math.min(0, top) },
    { x: Math.max(bw, left + width), y: Math.min(0, top) },
    { x: Math.max(bw, left + width), y: Math.max(bh, top + content.h) },
    { x: Math.min(0, left), y: Math.max(bh, top + content.h) },
  ].map((p) => rotate(p, c, rotation))
  const screenOf = (p: Point) => ({ x: (elementBox.x0 + p.x - vx) * z, y: (elementBox.y0 + p.y - vy) * z })
  const corners = local.map(screenOf)
  const covered = { x0: Math.min(...corners.map((p) => p.x)), y0: Math.min(...corners.map((p) => p.y)), x1: Math.max(...corners.map((p) => p.x)), y1: Math.max(...corners.map((p) => p.y)) }
  const canvasW = view?.width ?? 0
  const canvasH = view?.height ?? 0
  const above = covered.y0 - BAR_GAP - barSize.h
  const below = covered.y1 + BAR_GAP
  const barTop = above >= EDGE || below + barSize.h > canvasH - EDGE ? Math.max(EDGE, above) : below
  const barLeft = Math.max(EDGE, Math.min(covered.x0, canvasW - barSize.w - EDGE))

  const sizeButton = (by: 1 | -1) => {
    const Icon = by > 0 ? AArrowUpIcon : AArrowDownIcon
    const label = by > 0 ? labels.larger : labels.smaller
    return (
      <button type="button" className="board-text-mark" aria-label={label} title={label} onMouseDown={keep} onClick={() => styleBar && onStyle({ fontSize: stepSize(styleBar.fontSize, by) })} data-testid={by > 0 ? 'board-text-size-up' : 'board-text-size-down'}>
        <Icon size={16} strokeWidth={ICON_STROKE} aria-hidden="true" />
      </button>
    )
  }
  const colorLabel = styleBar?.colorIsLine ? labels.lineColor : labels.color

  return (
    <>
      {rich ? (
        <div ref={bar} className="board-text-marks" role="toolbar" aria-label={labels.bar} style={{ left: barLeft, top: barTop }} data-testid="board-text-marks" onPointerDown={(event) => event.stopPropagation()}>
          {markButton('bold', () => toggle('bold'))}
          {markButton('italic', () => toggle('italic'))}
          {markButton('link', openLink)}
          {styleBar ? (
            <>
              <span className="board-text-sep" aria-hidden="true" />
              {sizeButton(-1)}
              <span className="board-text-size" aria-label={labels.size} title={labels.size} data-testid="board-text-size">
                {Math.round(styleBar.fontSize)}
              </span>
              {sizeButton(1)}
              <span className="board-text-sep" aria-hidden="true" />
              <button type="button" className="board-text-mark" aria-label={colorLabel} title={colorLabel} aria-expanded={colors} onMouseDown={keep} onClick={() => setColors((open) => !open)} data-testid="board-text-color">
                <BaselineIcon size={16} strokeWidth={ICON_STROKE} aria-hidden="true" />
                <span className="board-text-color-bar" style={{ background: styleBar.color ? strokeCss(styleBar.color, styleBar.palette) : text.color }} />
              </button>
              {colors ? (
                <span className="board-text-colors" role="group" aria-label={colorLabel}>
                  {styleBar.palette.map((swatch) => (
                    <button
                      key={swatch.token}
                      type="button"
                      className="board-swatch"
                      style={{ background: strokeCss(swatch.token, styleBar.palette) }}
                      aria-pressed={styleBar.color === swatch.token}
                      aria-label={labels.swatch(swatch.token)}
                      title={labels.swatch(swatch.token)}
                      onMouseDown={keep}
                      onClick={() => {
                        setColors(false)
                        onStyle({ strokeColor: swatch.token })
                      }}
                      data-testid={`board-text-color-${swatch.token}`}
                    />
                  ))}
                </span>
              ) : null}
            </>
          ) : null}
          {linking ? (
            <input
              ref={linkInput}
              type="url"
              className="kui-field board-text-link-field"
              value={linking.text}
              placeholder="https://"
              aria-label={labels.address}
              autoFocus
              onChange={(event) => setLinking({ ...linking, text: event.target.value })}
              onFocus={() => {
                openingLink.current = false
              }}
              onBlur={(event) => {
                if (returning.current || !leaving(event.relatedTarget)) return
                // A browser may name no next focus (a click back into the text box): decide once
                // the focus has moved.
                window.setTimeout(() => {
                  if (leaving(field.current?.ownerDocument.activeElement ?? null)) finish(true)
                }, 0)
              }}
              onKeyDown={(event) => {
                event.stopPropagation()
                if (event.key === 'Enter' && !composingEnter(event.nativeEvent, false)) {
                  event.preventDefault()
                  applyLink()
                } else if (event.key === 'Escape') {
                  event.preventDefault()
                  backToText(linking.start, linking.end)
                }
              }}
              data-testid="board-text-link-url"
            />
          ) : null}
        </div>
      ) : null}
      <div
        className={editing.id == null ? 'board-text-box board-text-new' : 'board-text-box'}
        style={{ width: bw, height: bh, transform: placement, transformOrigin: '0 0' }}
        data-testid="board-text-box"
        data-rotation={rotation}
        data-zoom={z}
      >
        <textarea
          ref={field}
          className="board-text-editor"
          style={{
            left,
            top,
            width,
            height: content.h,
            fontFamily: text.family,
            fontSize: text.size,
            fontWeight: text.weight,
            lineHeight: `${text.lineHeight}px`,
            textAlign: text.align,
            color: text.color,
            whiteSpace: text.wrap ? 'pre-wrap' : 'pre',
          }}
          wrap={text.wrap ? 'soft' : 'off'}
          value={value}
          rows={1}
          placeholder={placeholder}
          aria-label={placeholder}
          spellCheck={false}
          data-testid="board-text-editor"
          data-ime-ignored={ignored}
          onChange={(event) => {
            const next = event.target.value
            if (rich) {
              // The edit ends at the caret (typing inside a run of identical characters).
              const caret = event.target.selectionEnd
              marks.current.stored = follow(marks.current.stored, value, next, caret)
              marks.current.pending = follow(marks.current.pending, value, next, caret)
            }
            setValue(next)
            readSelection()
          }}
          onSelect={readSelection}
          onPointerDown={(event) => event.stopPropagation()}
          onScroll={(event) => {
            // The box is as large as its text; a scroll the browser starts (a caret moved into view) is put back.
            event.currentTarget.scrollTop = 0
            event.currentTarget.scrollLeft = 0
          }}
          onCompositionStart={() => {
            composing.current = true
          }}
          onCompositionEnd={() => {
            composing.current = false
          }}
          onBlur={(event) => {
            if (openingLink.current) {
              openingLink.current = false
              return
            }
            if (leaving(event.relatedTarget)) finish(true)
          }}
          onKeyDown={(event) => {
            event.stopPropagation()
            if (event.key === 'Escape') {
              event.preventDefault()
              finish(false)
              return
            }
            if (rich && !composing.current && !event.nativeEvent.isComposing) {
              if (shortcut(event, 'b') || shortcut(event, 'i')) {
                event.preventDefault()
                toggle(shortcut(event, 'b') ? 'bold' : 'italic')
                return
              }
              if (shortcut(event, 'k')) {
                event.preventDefault()
                openLink()
                return
              }
            }
            if (event.key !== 'Enter' || event.shiftKey) return
            if (composingEnter(event.nativeEvent, composing.current)) {
              setIgnored((count) => count + 1)
              return
            }
            event.preventDefault()
            finish(true)
          }}
        />
      </div>
    </>
  )
}
