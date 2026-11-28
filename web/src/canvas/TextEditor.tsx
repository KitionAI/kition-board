/**
 * The label and text box (UI-K6): a focused `<textarea>` over the element. Typing stays local;
 * Enter (without Shift) or leaving the box commits once, Escape cancels. An Enter that belongs to
 * an input method's composition never commits (`composingEnter`, the IME fix of spike S2).
 *
 * Over a shape's label or a text (not a Markdown one), a small bar above the box sets bold
 * (Cmd/Ctrl+B), italic (Cmd/Ctrl+I) and a link (Cmd/Ctrl+K, a field for the address; an empty one
 * removes it) on the box's selection. The box stays a plain text box: the marks wait beside the
 * text as `{ start, end, mark, value }` in UTF-16 offsets of the current text (`board/marks.ts`),
 * move with the typing before them (placed at the caret), and are sent after the text when the box
 * commits (`text.mark`, design note 4.3); the engine's fragment shows them then. A bar button is pressed when the selection lies
 * inside a mark of its kind, stored or waiting.
 *
 * Ported from Kition's `packages/board-editor/src/components/WhiteboardTextEditor.tsx`.
 */
import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react'
import { ICON_STROKE, isMac } from '@kition/sdk/editor'
import { BoldIcon, ItalicIcon, LinkIcon, type IconComponent } from '@kition/tessera/icons/react'
import { composingEnter } from '../board/ime'
import { address, follow, markOver, type MarkName, type TextMark } from '../board/marks'

export type TextEditing = {
  /** The element whose text this is; `null` for a new text element at `world`. */
  id: string | null
  kind: 'shape' | 'text' | 'frame' | 'connector' | 'card' | 'new-text'
  value: string
  /** Screen box of the editor. */
  left: number
  top: number
  width: number
  height: number
  fontSize: number
  align: 'left' | 'center'
  /** The marks the text has, when the box offers the marks bar; `null`: no bar (Markdown, titles, captions). */
  marks: readonly TextMark[] | null
  /**
   * Opened by a typed character (`value` is that character): the box takes the focus at once, so
   * the characters typed after it land in it, with the caret after the text (no selection).
   */
  typed?: boolean
}

const ICONS: Record<MarkName, IconComponent> = { bold: BoldIcon, italic: ItalicIcon, link: LinkIcon }
const KEYS: Record<MarkName, string> = { bold: 'B', italic: 'I', link: 'K' }

/** Whether `event` is the editor's own shortcut key with `letter` (Cmd on a Mac, else Ctrl). */
function shortcut(event: ReactKeyboardEvent, letter: string) {
  const primary = isMac() ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey
  return primary && !event.altKey && !event.shiftKey && (event.key.toLowerCase() === letter || event.code === `Key${letter.toUpperCase()}`)
}

export function TextEditor({
  editing,
  placeholder,
  labels,
  onCommit,
  onCancel,
  commitRef,
}: {
  editing: TextEditing
  placeholder: string
  /** The bar's names: the bar itself, bold, italic, link and the link's address field. */
  labels: { bar: string; bold: string; italic: string; link: string; address: string }
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
  const [value, setValue] = useState(editing.value)
  const [ignored, setIgnored] = useState(0)
  // The stored marks and the waiting ones, both following the typing; the box's selection.
  const marks = useRef<{ stored: TextMark[]; pending: TextMark[] }>({ stored: [...(editing.marks ?? [])], pending: [] })
  const [selection, setSelection] = useState<{ start: number; end: number }>(editing.typed ? { start: editing.value.length, end: editing.value.length } : { start: 0, end: editing.value.length })
  const [linking, setLinking] = useState<{ start: number; end: number; text: string } | null>(null)
  const [, setTick] = useState(0)
  // The focus moving between the text box and the link field is not leaving the editor, whatever
  // the browser says the next focus is. Set only when the text box has the focus and is about to
  // hand it to the field, and cleared when the field takes it, so it never swallows a later blur.
  const openingLink = useRef(false)
  const returning = useRef(false)
  const rich = editing.marks != null

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
      field.current?.focus()
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
    field.current?.focus()
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
  // Above the box, or under it when the box is at the canvas's top.
  const barTop = editing.top >= 40 ? editing.top - 38 : editing.top + Math.max(editing.height, 34) + 4
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

  return (
    <>
      {rich ? (
        <div ref={bar} className="board-text-marks" role="toolbar" aria-label={labels.bar} style={{ left: editing.left, top: barTop }} data-testid="board-text-marks" onPointerDown={(event) => event.stopPropagation()}>
          {markButton('bold', () => toggle('bold'))}
          {markButton('italic', () => toggle('italic'))}
          {markButton('link', openLink)}
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
      <textarea
        ref={field}
        className="board-text-editor"
        style={{
          left: editing.left,
          top: editing.top,
          width: editing.width,
          minHeight: editing.height,
          fontSize: editing.fontSize,
          textAlign: editing.align,
        }}
        value={value}
        rows={1}
        placeholder={placeholder}
        aria-label={placeholder}
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
    </>
  )
}
