/**
 * The label and text box (UI-K6): a focused `<textarea>` over the element. Typing stays local;
 * Enter (without Shift) or leaving the box commits once, Escape cancels. An Enter that belongs to
 * an input method's composition never commits (`composingEnter`, the IME fix of spike S2).
 *
 * Over a shape's label or a text (not a Markdown one), a small bar above the box sets bold
 * (Cmd/Ctrl+B), italic (Cmd/Ctrl+I) and a link (Cmd/Ctrl+K, a field for the address; an empty one
 * removes it) on the box's selection. The box stays a plain text box: the marks wait beside the
 * text as `{ start, end, mark, value }` in UTF-16 offsets of the current text, move with the
 * typing before them, and are sent after the text when the box commits (`text.mark`, design note
 * 4.3); the engine's fragment shows them then. A bar button is pressed when the selection lies
 * inside a mark of its kind, stored or waiting.
 *
 * Ported from Kition's `packages/board-editor/src/components/WhiteboardTextEditor.tsx`.
 */
import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react'
import { ICON_STROKE, isMac } from '@kition/sdk/editor'
import { BoldIcon, ItalicIcon, LinkIcon, type IconComponent } from '@kition/tessera/icons/react'
import type { TextRun } from '@kition/sdk/kdm'
import { composingEnter } from '../board/ime'

export type MarkName = 'bold' | 'italic' | 'link'
/** A mark's value: `true` (bold, italic), a link's address, or `null` (removed). */
export type MarkValue = true | { href: string } | null
/** A mark over the UTF-16 range `start..end` of the box's text. */
export type TextMark = { start: number; end: number; mark: MarkName; value: MarkValue }

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
}

const MARKS: readonly MarkName[] = ['bold', 'italic', 'link']
const ICONS: Record<MarkName, IconComponent> = { bold: BoldIcon, italic: ItalicIcon, link: LinkIcon }
const KEYS: Record<MarkName, string> = { bold: 'B', italic: 'I', link: 'K' }

/** The stored marks of a text, from its runs (`Replica.textDelta`, UTF-16 lengths). */
export function marksOf(runs: readonly TextRun[]): TextMark[] {
  const out: TextMark[] = []
  let at = 0
  for (const run of runs) {
    const end = at + run.insert.length
    for (const mark of MARKS) {
      const raw = run.attributes?.[mark]
      const value: MarkValue = mark === 'link' ? (raw && typeof raw === 'object' && typeof (raw as { href?: unknown }).href === 'string' ? { href: (raw as { href: string }).href } : null) : raw === true ? true : null
      if (value == null) continue
      let last: TextMark | undefined
      for (const item of out) if (item.mark === mark) last = item
      if (last && last.end === at && JSON.stringify(last.value) === JSON.stringify(value)) last.end = end
      else out.push({ start: at, end, mark, value })
    }
    at = end
  }
  return out
}

/**
 * Where offset `x` of the old text is in the new one after `delete` units at `at` gave way to
 * `insert` units. A start inside the replaced part goes after the new text; an end inside it goes
 * to its start. Typing at a range's end takes the mark only when `expand` (bold and italic, as the
 * board's schema says: `expand: after`; a link never grows).
 */
function shifted(x: number, side: 'start' | 'end', at: number, removed: number, inserted: number, expand: boolean) {
  if (side === 'start') {
    if (x < at) return x
    return x >= at + removed ? x + inserted - removed : at + inserted
  }
  if (x < at || (x === at && !(expand && removed === 0))) return x
  if (x === at) return x + inserted
  return x >= at + removed ? x + inserted - removed : at
}

/** `marks` after the text went from `before` to `after` (one edit: what lies between their common ends). */
function follow(marks: readonly TextMark[], before: string, after: string): TextMark[] {
  let prefix = 0
  const most = Math.min(before.length, after.length)
  while (prefix < most && before.charCodeAt(prefix) === after.charCodeAt(prefix)) prefix += 1
  let suffix = 0
  while (suffix < most - prefix && before.charCodeAt(before.length - 1 - suffix) === after.charCodeAt(after.length - 1 - suffix)) suffix += 1
  const removed = before.length - prefix - suffix
  const inserted = after.length - prefix - suffix
  if (!removed && !inserted) return [...marks]
  return marks
    .map((item) => {
      const expand = item.mark !== 'link'
      return { ...item, start: shifted(item.start, 'start', prefix, removed, inserted, expand), end: shifted(item.end, 'end', prefix, removed, inserted, expand) }
    })
    .filter((item) => item.end > item.start)
}

/** The value of `mark` at UTF-16 unit `at`: the stored marks, then the waiting ones in order. */
function valueAt(stored: readonly TextMark[], pending: readonly TextMark[], mark: MarkName, at: number): MarkValue {
  let value: MarkValue = null
  for (const item of [...stored, ...pending]) if (item.mark === mark && item.start <= at && at < item.end) value = item.value
  return value
}

/** Whether every unit of the selection (or the one before a caret) has `mark`; and its value there. */
function markOver(stored: readonly TextMark[], pending: readonly TextMark[], mark: MarkName, start: number, end: number): MarkValue {
  const from = start < end ? start : Math.max(0, start - 1)
  const to = start < end ? end : from + 1
  let first: MarkValue = null
  for (let at = from; at < to; at += 1) {
    const value = valueAt(stored, pending, mark, at)
    if (value == null) return null
    first ??= value
  }
  return first
}

/** An address as the engine takes it: http, https or mailto (a bare host gets `https://`). */
function address(text: string): string | null {
  const trimmed = text.trim()
  if (!trimmed) return null
  return /^(https?:\/\/|mailto:)/i.test(trimmed) ? trimmed : `https://${trimmed}`
}

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
}: {
  editing: TextEditing
  placeholder: string
  /** The bar's names: the bar itself, bold, italic, link and the link's address field. */
  labels: { bar: string; bold: string; italic: string; link: string; address: string }
  onCommit: (value: string, marks: readonly TextMark[]) => void
  onCancel: () => void
}) {
  const field = useRef<HTMLTextAreaElement>(null)
  const bar = useRef<HTMLDivElement>(null)
  const composing = useRef(false)
  const done = useRef(false)
  const [value, setValue] = useState(editing.value)
  const [ignored, setIgnored] = useState(0)
  // The stored marks and the waiting ones, both following the typing; the box's selection.
  const marks = useRef<{ stored: TextMark[]; pending: TextMark[] }>({ stored: [...(editing.marks ?? [])], pending: [] })
  const [selection, setSelection] = useState<{ start: number; end: number }>({ start: 0, end: editing.value.length })
  const [linking, setLinking] = useState<{ start: number; end: number; text: string } | null>(null)
  const [, setTick] = useState(0)
  // The focus moving between the text box and the link field is not leaving the editor, whatever
  // the browser says the next focus is.
  const openingLink = useRef(false)
  const returning = useRef(false)
  const rich = editing.marks != null

  // Focused once the pointer press that opened the box is over: the Text tool opens a new box on
  // the press, and the press's own default action then focuses the canvas; a box focused at once
  // would lose focus to it and close empty (leaving the box commits).
  useEffect(() => {
    const id = window.setTimeout(() => {
      field.current?.focus()
      field.current?.select()
    }, 0)
    return () => window.clearTimeout(id)
  }, [])

  const finish = (commit: boolean) => {
    if (done.current) return
    done.current = true
    if (commit) onCommit(field.current?.value ?? value, marks.current.pending)
    else onCancel()
  }

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
    openingLink.current = true
    setLinking({ start, end, text: current && current !== true ? current.href : '' })
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
              type="url"
              className="kui-field board-text-link-field"
              value={linking.text}
              placeholder="https://"
              aria-label={labels.address}
              autoFocus
              onChange={(event) => setLinking({ ...linking, text: event.target.value })}
              onBlur={(event) => {
                if (!returning.current && leaving(event.relatedTarget)) finish(true)
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
            marks.current.stored = follow(marks.current.stored, value, next)
            marks.current.pending = follow(marks.current.pending, value, next)
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
