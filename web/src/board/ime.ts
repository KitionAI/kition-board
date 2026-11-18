/**
 * Whether an Enter belongs to an input method's composition and must not commit the text box.
 *
 * Kition's v1 board committed on any Enter, so a Chinese or Japanese input method's Enter (which
 * picks a candidate) committed the half-typed text (board evaluation, gap 9). `isComposing` alone
 * is not enough: Chromium sends the Enter during the composition (`isComposing: true`), but WebKit,
 * the desktop app's engine, ends the composition first and then sends the Enter with
 * `isComposing: false` and `keyCode: 229` (spike S2, 3.5). So the composition is also tracked with
 * `compositionstart`/`compositionend` and `keyCode` 229 counts as composing.
 */
export function composingEnter(event: { key: string; isComposing?: boolean; keyCode?: number }, composing: boolean) {
  return event.key === 'Enter' && (event.isComposing === true || event.keyCode === 229 || composing)
}
