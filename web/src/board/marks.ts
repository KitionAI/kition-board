/**
 * The text box's marks (bold, italic, link; `web/src/canvas/TextEditor.tsx`), kept beside its text
 * as `{ start, end, mark, value }` in UTF-16 offsets of the current text: the stored ones read from
 * the text's runs, and the ones waiting to be sent. Both follow the typing. No DOM here, so
 * `web/tests/marks.mjs` runs it without a browser.
 */
import type { TextRun } from '@kition/sdk/kdm'

export type MarkName = 'bold' | 'italic' | 'link'
/** A mark's value: `true` (bold, italic), a link's address, or `null` (removed). */
export type MarkValue = true | { href: string } | null
/** A mark over the UTF-16 range `start..end` of the box's text. */
export type TextMark = { start: number; end: number; mark: MarkName; value: MarkValue }

export const MARKS: readonly MarkName[] = ['bold', 'italic', 'link']

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

const high = (code: number) => code >= 0xd800 && code <= 0xdbff
const low = (code: number) => code >= 0xdc00 && code <= 0xdfff

/** Whether offset `at` of `text` lies between the two UTF-16 units of a surrogate pair. */
function splits(text: string, at: number) {
  return at > 0 && at < text.length && high(text.charCodeAt(at - 1)) && low(text.charCodeAt(at))
}

/**
 * `marks` after one edit took the text from `before` to `after`, the box's caret then at `caret`
 * (its `selectionEnd`). The edit ends at the caret: what follows it followed the edit, and the
 * common start is no longer than what precedes it, so typing inside a run of identical units (`xx`,
 * or emoji sharing their first unit) is placed where it was typed, not at the run's end. A caret
 * that does not fit the change (an edit not made at the caret) falls back to the two texts' common
 * ends. Neither the edit nor any mark's boundary falls between the two units of a surrogate pair or
 * past the text's end.
 */
export function follow(marks: readonly TextMark[], before: string, after: string, caret: number): TextMark[] {
  if (before === after) return [...marks]
  const tail = after.length - caret
  const anchored = Number.isInteger(caret) && caret >= 0 && caret <= after.length && tail <= before.length && before.endsWith(after.slice(caret))
  let prefix = 0
  let suffix = 0
  if (anchored) {
    suffix = tail
    const most = Math.min(before.length - suffix, caret)
    while (prefix < most && before.charCodeAt(prefix) === after.charCodeAt(prefix)) prefix += 1
  } else {
    const most = Math.min(before.length, after.length)
    while (prefix < most && before.charCodeAt(prefix) === after.charCodeAt(prefix)) prefix += 1
    while (suffix < most - prefix && before.charCodeAt(before.length - 1 - suffix) === after.charCodeAt(after.length - 1 - suffix)) suffix += 1
  }
  if (splits(before, prefix) || splits(after, prefix)) prefix -= 1
  if (splits(before, before.length - suffix) || splits(after, after.length - suffix)) suffix -= 1
  const removed = before.length - prefix - suffix
  const inserted = after.length - prefix - suffix
  /** A boundary inside the new text, a pair kept whole (a start before it, an end after it). */
  const whole = (at: number, side: 'start' | 'end') => {
    const inside = Math.min(after.length, Math.max(0, at))
    return splits(after, inside) ? inside + (side === 'start' ? -1 : 1) : inside
  }
  return marks
    .map((item) => {
      const expand = item.mark !== 'link'
      return {
        ...item,
        start: whole(shifted(item.start, 'start', prefix, removed, inserted, expand), 'start'),
        end: whole(shifted(item.end, 'end', prefix, removed, inserted, expand), 'end'),
      }
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
export function markOver(stored: readonly TextMark[], pending: readonly TextMark[], mark: MarkName, start: number, end: number): MarkValue {
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
export function address(text: string): string | null {
  const trimmed = text.trim()
  if (!trimmed) return null
  return /^(https?:\/\/|mailto:)/i.test(trimmed) ? trimmed : `https://${trimmed}`
}
