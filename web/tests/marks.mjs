// The text box's marks, without a browser: node tests/marks.mjs. While a label is edited, its stored
// and waiting marks follow the typing (web/src/board/marks.ts `follow`): the edit is placed at the
// box's caret, so typing inside a run of identical characters (or of emoji that share their first
// UTF-16 unit) moves a mark by exactly what was typed before it, and no mark boundary lands inside
// a surrogate pair or past the text's end (case regressions/issue-mark-offsets). The browser half
// is web/tests/editing.mjs step 15.
import { follow } from '../src/board/marks.ts'

let failures = 0
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
}
const bold = (start, end) => ({ start, end, mark: 'bold', value: true })
const link = (start, end) => ({ start, end, mark: 'link', value: { href: 'https://example.com' } })
const ranges = (marks) => JSON.stringify(marks.map((item) => [item.mark, item.start, item.end]))
const expect = (name, marks, before, after, caret, wanted) => {
  const got = follow(marks, before, after, caret)
  step(name, ranges(got) === JSON.stringify(wanted), `${JSON.stringify(before)} → ${JSON.stringify(after)} caret ${caret}: ${ranges(got)}, wanted ${JSON.stringify(wanted)}`)
}

// "xx" with the second x marked; the caret between them; an x typed: the new x is the second one.
expect('typing an x between two x leaves the bold on the old second x', [bold(1, 2)], 'xx', 'xxx', 2, [['bold', 2, 3]])
expect('typing an x between two x does not link the typed x', [link(1, 2)], 'xx', 'xxx', 2, [['link', 2, 3]])
expect('typing an x before a linked run of x moves the link by one', [link(0, 2)], 'xx', 'xxx', 1, [['link', 1, 3]])
// At a mark's end, bold grows with the typing (expand: after) and a link does not.
expect('typing at the end of a bold run makes the typed text bold', [bold(0, 2)], 'ab', 'abc', 3, [['bold', 0, 3]])
expect('typing at the end of a link does not grow it', [link(0, 2)], 'ab', 'abc', 3, [['link', 0, 2]])
expect('typing at the end of a bold run of x grows it by one', [bold(0, 2)], 'xx', 'xxx', 3, [['bold', 0, 3]])
// Deleting inside a run: Backspace with the caret after the second of three x removes that one.
expect('Backspace inside a run of x keeps the bold on the last x', [bold(2, 3)], 'xxx', 'xx', 1, [['bold', 1, 2]])
expect('Delete inside a run of x keeps the bold on the first x', [bold(0, 1)], 'xxx', 'xx', 1, [['bold', 0, 1]])
// Emoji share their high surrogate (😀 is D83D DE00, 😁 is D83D DE01).
expect('typing an emoji before a bold emoji moves the bold past it', [bold(0, 2)], '😀', '😁😀', 2, [['bold', 2, 4]])
expect('typing an emoji between two emoji leaves the bold on the second', [bold(2, 4)], '😀😀', '😀😀😀', 4, [['bold', 4, 6]])
expect('typing an emoji after a link on an emoji does not link it', [link(0, 2)], '😀', '😀😀', 4, [['link', 0, 2]])
// A selection replaced by typing.
expect('a replaced selection shortens the bold over it', [bold(0, 5)], 'hello', 'hao', 2, [['bold', 0, 3]])
expect('a mark inside a deleted selection goes', [bold(1, 3)], 'hello', 'ho', 1, [])
// Two emoji replaced by a third that shares their first unit: the common start is one unit, inside
// the first pair; the edit takes the whole pair, and the bold over the first emoji goes with it.
expect('a mark over replaced emoji never ends inside the new one', [bold(0, 2)], '😀😀', '😁', 2, [])
expect('a mark after replaced emoji moves by whole emoji', [bold(4, 5)], '😀😀x', '😁x', 2, [['bold', 2, 3]])
// No change, or a caret that does not fit the change (an undo of the box's own): never a boundary
// past the end or inside a surrogate pair.
expect('the same text keeps the marks', [bold(1, 2)], 'xx', 'xx', 1, [['bold', 1, 2]])

/** Whether every boundary of `marks` is inside `text` and not between the two units of a pair. */
const sound = (marks, text) =>
  marks.every((item) =>
    [item.start, item.end].every((at) => at >= 0 && at <= text.length && !(at > 0 && at < text.length && /[\uD800-\uDBFF]/.test(text[at - 1]) && /[\uDC00-\uDFFF]/.test(text[at]))) && item.start < item.end,
  )
// Random edits of random texts of x, emoji and letters, at random carets (some of them wrong).
const pieces = ['x', 'x', 'y', '😀', '😁', '🎉']
let seed = 7
const random = (n) => {
  seed = (seed * 1103515245 + 12345) % 2147483648
  return seed % n
}
const textOf = (n) => Array.from({ length: n }, () => pieces[random(pieces.length)]).join('')
/** A boundary that does not split a pair in `text`. */
const whole = (text, at) => (at > 0 && at < text.length && /[\uDC00-\uDFFF]/.test(text[at]) ? at - 1 : at)
/** The code-point boundaries of `text`. */
const bounds = (text) => {
  const out = [0]
  let at = 0
  for (const ch of text) out.push((at += ch.length))
  return out
}
let unsound = null
for (let round = 0; round < 3000 && !unsound; round += 1) {
  const before = textOf(1 + random(6))
  const a = whole(before, random(before.length + 1))
  const b = whole(before, a + random(before.length - a + 1))
  const typed = random(3) ? textOf(random(3)) : ''
  const after = before.slice(0, a) + typed + before.slice(b)
  const caret = random(10) ? a + typed.length : random(after.length + 3) - 1
  // A bold mark over every code-point range of the old text, and a link over the whole of it.
  const at = bounds(before)
  const marks = [...at.flatMap((s) => at.filter((e) => e > s).map((e) => bold(s, e))), link(0, before.length)]
  const got = follow(marks, before, after, caret)
  if (!sound(got, after)) unsound = `${JSON.stringify(before)} → ${JSON.stringify(after)} caret ${caret}: ${ranges(marks)} → ${ranges(got)}`
}
step('no boundary inside a surrogate pair or past the end, after 3000 random edits', unsound == null, unsound ?? '')

console.log(failures ? `marks: ${failures} failed` : 'marks: all passed')
process.exit(failures ? 1 : 0)
