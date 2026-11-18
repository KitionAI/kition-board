/**
 * The label and text box (UI-K6): a focused `<textarea>` over the element. Typing stays local;
 * Enter (without Shift) or leaving the box commits once, Escape cancels. An Enter that belongs to
 * an input method's composition never commits (`composingEnter`, the IME fix of spike S2).
 *
 * Ported from Kition's `packages/board-editor/src/components/WhiteboardTextEditor.tsx`.
 */
import { useEffect, useRef, useState } from 'react'
import { composingEnter } from '../board/ime'

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
}

export function TextEditor({ editing, placeholder, onCommit, onCancel }: { editing: TextEditing; placeholder: string; onCommit: (value: string) => void; onCancel: () => void }) {
  const field = useRef<HTMLTextAreaElement>(null)
  const composing = useRef(false)
  const done = useRef(false)
  const [value, setValue] = useState(editing.value)
  const [ignored, setIgnored] = useState(0)

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
    if (commit) onCommit(field.current?.value ?? value)
    else onCancel()
  }

  return (
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
      onChange={(event) => setValue(event.target.value)}
      onPointerDown={(event) => event.stopPropagation()}
      onCompositionStart={() => {
        composing.current = true
      }}
      onCompositionEnd={() => {
        composing.current = false
      }}
      onBlur={() => finish(true)}
      onKeyDown={(event) => {
        event.stopPropagation()
        if (event.key === 'Escape') {
          event.preventDefault()
          finish(false)
          return
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
  )
}
