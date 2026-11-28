// Typing by hand (plan "Board usable by hand", stage 1): node tests/hand/text.mjs [chromium|webkit] [scenario ...]
// Real mouse and keyboard events only. The text box is the element: while a long label is edited the
// whole text is visible (the box never scrolls), the box is the element's box, its lines break where
// the engine's do (before and after the commit), and the element's own text is hidden; the same for
// a sticky note, at 50% and 200% and on a rotated shape. The text tool: a click makes an auto-width
// text (Shift+Enter for a new line), a drag across a fixed-width one that wraps at that width; a
// side handle makes an auto-width text fixed-width. The bar floats above the element without
// covering it and sets the text size and colour. Input methods: both Enter orders still do not
// commit, and the text being composed is in the box.
// The cases under cases/board.editor.text-box, board.editor.text-tool, board.text and
// cases/regressions/issue-hand-f5-* name these scenarios.
import { open } from './harness.mjs'

const which = process.argv[2] ?? 'chromium'
const h = await open(which)
const { page, kit, step, until, board, ai, screen, centre, selection, tool, editorOpen, storedOne, near, sameBox, fmt, fresh, drag } = h

const LONG = 'A label far too long to fit inside this small shape, so it wraps onto many lines and runs past the bottom edge'
const RECT = { kind: 'shape', position: { x: 200, y: 160 }, size: { w: 160, h: 100 }, text: LONG }
const NOTE = { kind: 'shape', shapeStyle: 'sticky', position: { x: 520, y: 160 }, size: { w: 180, h: 140 }, text: `${LONG}, and further still on a note` }
const editor = kit.getByTestId('board-text-editor')

/** A page rectangle of the element with `testId`, as { x0, y0, x1, y1 }, or null. */
const rectOf = async (testId) => {
  const b = await kit.getByTestId(testId).first().boundingBox().catch(() => null)
  return b ? { x0: b.x, y0: b.y, x1: b.x + b.width, y1: b.y + b.height } : null
}
/** The stored box of an element on the page (unrotated), and its rotated bounds. */
const pageBox = async (id) => {
  const element = await storedOne(id)
  if (!element?.position || !element?.size) return null
  const { x, y } = element.position
  const { w, h: hh } = element.size
  const a = await screen(x, y)
  const b = await screen(x + w, y + hh)
  const r = ((element.rotation ?? 0) * Math.PI) / 180
  const cx = (a.x + b.x) / 2
  const cy = (a.y + b.y) / 2
  const corners = [
    [a.x, a.y],
    [b.x, a.y],
    [b.x, b.y],
    [a.x, b.y],
  ].map(([px, py]) => [cx + (px - cx) * Math.cos(r) - (py - cy) * Math.sin(r), cy + (px - cx) * Math.sin(r) + (py - cy) * Math.cos(r)])
  const xs = corners.map((p) => p[0])
  const ys = corners.map((p) => p[1])
  return { box: { x0: a.x, y0: a.y, x1: b.x, y1: b.y }, bounds: { x0: Math.min(...xs), y0: Math.min(...ys), x1: Math.max(...xs), y1: Math.max(...ys) } }
}
/** The text box's state: its value, whether it scrolls, its size in its own (board) units, its lines as the browser breaks them. */
const boxState = () =>
  board(() => {
    const field = document.querySelector('[data-testid="board-text-editor"]')
    if (!field) return null
    const cs = getComputedStyle(field)
    const mirror = document.createElement('div')
    for (const name of ['fontFamily', 'fontSize', 'fontWeight', 'fontStyle', 'lineHeight', 'letterSpacing', 'fontKerning', 'fontVariantLigatures', 'fontOpticalSizing', 'fontVariationSettings', 'whiteSpace', 'overflowWrap', 'wordBreak', 'textAlign', 'width', 'tabSize']) mirror.style[name] = cs[name]
    Object.assign(mirror.style, { position: 'absolute', left: '-20000px', top: '0', padding: '0', border: '0', margin: '0' })
    mirror.textContent = field.value || '​'
    document.body.appendChild(mirror)
    const node = mirror.firstChild
    const lines = []
    let current = ''
    let lastTop = null
    for (let i = 0; i < field.value.length; i += 1) {
      const ch = field.value[i]
      if (ch === '\n') {
        lines.push(current)
        current = ''
        lastTop = null
        continue
      }
      const range = document.createRange()
      range.setStart(node, i)
      range.setEnd(node, i + 1)
      const rects = [...range.getClientRects()].filter((rect) => rect.width > 0 || ch === ' ')
      const top = rects.length ? rects[rects.length - 1].top : lastTop
      if (lastTop != null && top != null && top > lastTop + 1 && ch !== ' ') {
        lines.push(current)
        current = ''
      }
      current += ch
      if (top != null && ch !== ' ') lastTop = top
    }
    lines.push(current)
    mirror.remove()
    return {
      value: field.value,
      scrolls: field.scrollHeight > field.clientHeight + 1 || field.scrollWidth > field.clientWidth + 1,
      width: field.offsetWidth,
      height: field.offsetHeight,
      lineHeight: parseFloat(cs.lineHeight),
      fontSize: parseFloat(cs.fontSize),
      lines: lines.map((line) => line.trimEnd()).filter((line) => line.length),
    }
  })
/** The lines the engine drew for an element (its fragment's tspans). */
const engineLines = (id) => board((id) => [...document.querySelectorAll(`[data-testid="board-elements"] [data-id="${id}"] tspan`)].map((span) => span.textContent.trimEnd()).filter((line) => line.length), id)
/** The engine's text frame of an element's fragment, read through the editor's frame. */
const textHidden = (id) =>
  board((id) => {
    const node = document.querySelector(`[data-testid="board-elements"] [data-id="${id}"]`)
    const text = node?.querySelector('text')
    return { editing: node?.classList.contains('board-editing') ?? false, hidden: text ? getComputedStyle(text).visibility === 'hidden' : null, fill: node?.querySelector('path, rect, ellipse, polygon') ? getComputedStyle(node.querySelector('path, rect, ellipse, polygon')).visibility : null }
  }, id)
const intersects = (a, b) => a && b && a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1
const inCanvas = async (r) => {
  const svg = await h.svgBox()
  return r && r.x0 >= svg.x - 1 && r.y0 >= svg.y - 1 && r.x1 <= svg.x + svg.width + 1 && r.y1 <= svg.y + svg.height + 1
}
const openOn = async (id) => {
  const at = await centre(id)
  await page.mouse.dblclick(at.x, at.y)
  return until(async () => (await editorOpen()) && (await editor.evaluate((field) => field.ownerDocument.activeElement === field)), 40)
}
const close = async () => {
  await page.keyboard.press('Escape')
  await until(async () => !(await editorOpen()), 20)
}
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b)

/**
 * A long label opened: the box is the element's box, the text box is as tall as its lines and as
 * wide as the engine's wrap width (in board units, whatever the zoom), nothing scrolls, everything
 * is inside the canvas, the lines break where the engine broke them, and the element's own text is
 * hidden while its outline stays.
 */
async function checkLong(label, id, wrapWidth) {
  const opened = await openOn(id)
  const state = await boxState()
  const box = await rectOf('board-text-box')
  const element = await pageBox(id)
  const field = await rectOf('board-text-editor')
  const zoom = (await board(() => window.kitBoard.viewport())).zoom
  step(`${label}: the box is the element's box`, opened && sameBox(box, element?.box, 1.5), `box ${fmt(box)} vs element ${fmt(element?.box)}`)
  step(
    `${label}: the whole text shows, nothing scrolls`,
    state && !state.scrolls && near(state.height, state.lines.length * state.lineHeight, 1) && (await inCanvas(field)),
    `${JSON.stringify(state && { scrolls: state.scrolls, height: state.height, lines: state.lines.length, lineHeight: state.lineHeight })} field ${fmt(field)}`,
  )
  step(`${label}: it wraps at the engine's width`, state && near(state.width, wrapWidth, 0.5) && field && near(field.x1 - field.x0, wrapWidth * zoom, 1.5), `width ${state?.width} (board units), ${field && field.x1 - field.x0} px at ${zoom}`)
  const drawn = await engineLines(id)
  step(`${label}: its lines break where the engine's do`, state && drawn.length > 2 && same(state.lines, drawn), `box ${JSON.stringify(state?.lines)} engine ${JSON.stringify(drawn)}`)
  const hidden = await textHidden(id)
  step(`${label}: the element's own text is hidden, its outline stays`, hidden.editing && hidden.hidden === true && hidden.fill === 'visible', JSON.stringify(hidden))
  return state
}

await h.run([
  ['setup', async () => {
    await h.newPage()
  }],
  // F5.1: a long label in a shape is all visible while it is edited, and wraps as the engine wraps it.
  ['label', async () => {
    const [shape] = await fresh([RECT])
    await checkLong('shape', shape, 160 - 24)
    // Typing more grows the box downward; the shape does not grow.
    await editor.evaluate((field) => field.setSelectionRange(field.value.length, field.value.length))
    await page.keyboard.type(' and a few more words typed by hand')
    const typed = await boxState()
    step('typing more grows the box, still without scrolling', typed && !typed.scrolls && typed.lines.length > 0, JSON.stringify(typed?.lines))
    await page.keyboard.press('Enter')
    await until(async () => (await storedOne(shape))?.text?.endsWith('by hand'))
    await until(async () => (await engineLines(shape)).join(' ').endsWith('by hand'))
    const after = await engineLines(shape)
    step('after the commit the lines break where they broke in the box', typed && same(typed.lines, after), `box ${JSON.stringify(typed?.lines)} engine ${JSON.stringify(after)}`)
    step('the shape kept its size', near((await storedOne(shape))?.size?.h, 100, 0.01), JSON.stringify((await storedOne(shape))?.size))
    const shown = await textHidden(shape)
    step('the text shows again once the box closes', !shown.editing && shown.hidden === false, JSON.stringify(shown))
  }],
  // F5.3: the same in a sticky note.
  ['note', async () => {
    const [, note] = await fresh([{ ...RECT, text: 'x' }, NOTE])
    await checkLong('sticky note', note, 180 - 24)
    await close()
  }],
  // F5.2: the bar floats above the element, off it (below it when there is no room above).
  ['bar', async () => {
    const [shape] = await fresh([{ ...RECT, text: 'Plan' }])
    await openOn(shape)
    const bar = await rectOf('board-text-marks')
    const element = await pageBox(shape)
    step('the bar is above the element and does not cover it', bar && element && !intersects(bar, element.bounds) && bar.y1 <= element.bounds.y0, `bar ${fmt(bar)} element ${fmt(element?.bounds)}`)
    await close()
    // At the top of the canvas there is no room above: the bar goes below, still off the element.
    const [top] = await fresh([{ kind: 'shape', position: { x: 300, y: 2 }, size: { w: 160, h: 80 }, text: 'Top' }])
    await openOn(top)
    const under = await rectOf('board-text-marks')
    const topBox = await pageBox(top)
    step('with no room above, the bar is below the element', under && topBox && !intersects(under, topBox.bounds) && under.y0 >= topBox.bounds.y1, `bar ${fmt(under)} element ${fmt(topBox?.bounds)}`)
    await close()
  }],
  // F5.10: the text size and colour from the bar, while typing.
  ['bar-style', async () => {
    const [shape] = await fresh([{ ...RECT, text: 'Plan' }])
    await openOn(shape)
    await editor.evaluate((field) => field.setSelectionRange(field.value.length, field.value.length))
    await page.keyboard.type(' ahead')
    const before = (await boxState())?.fontSize
    const up = await h.testid('board-text-size-up')
    await page.mouse.click(up.x, up.y)
    const grown = await until(async () => (await storedOne(shape))?.fontSize === 18)
    const bigger = await until(async () => (await boxState())?.fontSize === 18, 20)
    step('A+ makes the text larger: the element and the box', grown && bigger && before === 16 && (await editorOpen()) && (await editor.inputValue()) === 'Plan ahead', `stored ${(await storedOne(shape))?.fontSize}, box ${before} → ${(await boxState())?.fontSize}, value ${JSON.stringify(await editor.inputValue().catch(() => null))}`)
    step('the size shows in the bar', (await kit.getByTestId('board-text-size').textContent()) === '18', await kit.getByTestId('board-text-size').textContent())
    const color = await h.testid('board-text-color')
    await page.mouse.click(color.x, color.y)
    await until(async () => (await kit.getByTestId('board-text-color-red').count()) === 1, 20)
    const red = await h.testid('board-text-color-red')
    await page.mouse.click(red.x, red.y)
    const colored = await until(async () => (await storedOne(shape))?.strokeColor === 'red')
    step('a colour from the bar colours the text', colored && (await editorOpen()), JSON.stringify((await storedOne(shape))?.strokeColor))
    const barNow = await rectOf('board-text-marks')
    step('the bar still keeps off the element', !intersects(barNow, (await pageBox(shape))?.bounds), `bar ${fmt(barNow)}`)
    await page.keyboard.press('Enter')
    step('the text typed meanwhile commits', await until(async () => (await storedOne(shape))?.text === 'Plan ahead'), JSON.stringify((await storedOne(shape))?.text))
    // A new text takes the size picked before it exists.
    await fresh([])
    await page.keyboard.press('t')
    await until(async () => (await tool()) === 'text', 20)
    const at = await screen(300, 300)
    await page.mouse.click(at.x, at.y)
    await until(async () => (await editorOpen()) && (await editor.evaluate((field) => field.ownerDocument.activeElement === field)), 40)
    await page.keyboard.type('Big')
    const larger = await h.testid('board-text-size-up')
    await page.mouse.click(larger.x, larger.y)
    const boxLarger = await until(async () => (await boxState())?.fontSize === 24, 20)
    await page.keyboard.press('Enter')
    await until(async () => (await selection()).length === 1)
    const made = (await selection())[0]
    step('a new text takes the size picked in the bar', boxLarger && (await storedOne(made))?.fontSize === 24 && (await storedOne(made))?.text === 'Big', JSON.stringify(await storedOne(made)))
  }],
  // F5.4: the text tool's two ways.
  ['text-tool-click', async () => {
    await fresh([])
    await page.keyboard.press('t')
    await until(async () => (await tool()) === 'text', 20)
    const at = await screen(240, 200)
    await page.mouse.click(at.x, at.y)
    await until(async () => (await editorOpen()) && (await editor.evaluate((field) => field.ownerDocument.activeElement === field)), 40)
    const words = 'An auto width text does not wrap however long it gets on its line'
    await page.keyboard.type(words)
    const one = await boxState()
    step('a click makes an auto-width text: one line however long', one && one.lines.length === 1 && !one.scrolls && one.width > 400, JSON.stringify(one && { lines: one.lines, width: one.width }))
    await page.keyboard.press('Shift+Enter')
    await page.keyboard.type('second line')
    const two = await boxState()
    step('Shift+Enter makes a new line', two && same(two.lines, [words, 'second line']), JSON.stringify(two?.lines))
    await page.keyboard.press('Enter')
    await until(async () => (await selection()).length === 1)
    const made = (await selection())[0]
    const element = made ? await storedOne(made) : null
    await until(async () => (await engineLines(made)).length === 2)
    step('committed: an auto-width text (no stored size) with the same two lines', element?.kind === 'text' && element.size == null && same(await engineLines(made), two?.lines), `${JSON.stringify(element)?.slice(0, 200)} ${JSON.stringify(await engineLines(made))}`)
    step('the tool is back to Select', (await tool()) === 'select', await tool())
  }],
  ['text-tool-drag', async () => {
    await fresh([])
    await page.keyboard.press('t')
    await until(async () => (await tool()) === 'text', 20)
    const from = await screen(200, 200)
    const to = await screen(420, 204)
    await drag(from, to)
    await until(async () => (await editorOpen()) && (await editor.evaluate((field) => field.ownerDocument.activeElement === field)), 40)
    await page.keyboard.type('A fixed width text made by dragging across wraps at that width as it is typed')
    const state = await boxState()
    step('a drag across makes a box as wide as the drag that wraps', state && near(state.width, 220, 0.5) && state.lines.length >= 3 && !state.scrolls, JSON.stringify(state && { width: state.width, lines: state.lines }))
    await page.keyboard.press('Enter')
    await until(async () => (await selection()).length === 1)
    const made = (await selection())[0]
    await until(async () => (await engineLines(made)).length >= 3)
    const element = made ? await storedOne(made) : null
    const drawn = await engineLines(made)
    step('committed: a fixed-width text 220 wide, wrapped where the box wrapped', element?.kind === 'text' && near(element.size?.w, 220, 0.01) && same(drawn, state?.lines), `${JSON.stringify(element?.size)} box ${JSON.stringify(state?.lines)} engine ${JSON.stringify(drawn)}`)
    const box = await h.boxOf(made)
    // The drawn box has half the text's stroke width around it.
    step('its box is as tall as its lines', box && near(box.y1 - box.y0, drawn.length * 25, 3), `${fmt(box)} for ${drawn.length} lines`)
  }],
  // A side handle makes an auto-width text fixed-width.
  ['text-handle', async () => {
    const [text] = await fresh([{ kind: 'text', position: { x: 200, y: 200 }, text: 'An auto width text that a side handle narrows' }])
    const at = await centre(text)
    await page.mouse.click(at.x, at.y)
    await until(async () => (await selection())[0] === text)
    const east = await h.testid('board-handle-east')
    const target = await screen(360, 0)
    await drag(east, { x: target.x, y: east.y })
    const narrowed = await until(async () => near((await storedOne(text))?.size?.w, 160, 2))
    await until(async () => (await engineLines(text)).length > 1)
    const drawn = await engineLines(text)
    const box = await h.boxOf(text)
    step('dragging the right handle makes it fixed-width: it wraps there', narrowed && drawn.length > 1 && box && near(box.y1 - box.y0, drawn.length * 25, 3), `${JSON.stringify((await storedOne(text))?.size)} ${JSON.stringify(drawn)} ${fmt(box)}`)
    await openOn(text)
    const state = await boxState()
    step('and its box wraps the same way', state && near(state.width, (await storedOne(text))?.size?.w ?? 0, 0.5) && same(state.lines, drawn), JSON.stringify(state?.lines))
    await close()
  }],
  // F5.8: at 50% and at 200%.
  ['zoom', async () => {
    for (const zoom of [0.5, 2]) {
      const [shape] = await fresh([RECT])
      await board((zoom) => window.kitBoard.setViewport({ x: 100, y: 60, zoom }), zoom)
      await page.waitForTimeout(100)
      await checkLong(`at ${zoom * 100}%`, shape, 160 - 24)
      await close()
    }
  }],
  // A rotated shape: the box turns with it.
  ['rotated', async () => {
    const [shape] = await fresh([{ ...RECT, text: 'Turned label', rotation: 30 }])
    await openOn(shape)
    const box = await rectOf('board-text-box')
    const element = await pageBox(shape)
    const rotation = Number(await kit.getByTestId('board-text-box').getAttribute('data-rotation'))
    step('on a rotated shape the box turns with it', rotation === 30 && sameBox(box, element?.bounds, 2), `rotation ${rotation}, box ${fmt(box)} vs ${fmt(element?.bounds)}`)
    const field = await rectOf('board-text-editor')
    const c = element && { x: (element.box.x0 + element.box.x1) / 2, y: (element.box.y0 + element.box.y1) / 2 }
    step('its one line is centred on the shape', field && c && near((field.x0 + field.x1) / 2, c.x, 2) && near((field.y0 + field.y1) / 2, c.y, 2), `field ${fmt(field)} centre ${JSON.stringify(c)}`)
    const bar = await rectOf('board-text-marks')
    step('the bar keeps off the turned shape', !intersects(bar, element?.bounds), `bar ${fmt(bar)} element ${fmt(element?.bounds)}`)
    await close()
  }],
  // A connector's label and a frame's title open where the engine draws them.
  ['other-texts', async () => {
    const pageId = await h.personPage()
    await fresh([])
    const made = await ai([
      { command: 'connector.add', params: { page: pageId, from: { point: [200, 300] }, to: { point: [600, 300] }, label: 'Yes' } },
      { command: 'frame.add', params: { page: pageId, box: [200, 420, 600, 600], title: 'Plan' } },
    ])
    const connector = made.completed[0].result.created[0]
    const frame = made.completed[1].result.frame
    await until(async () => (await h.shown()).includes(connector) && (await h.shown()).includes(frame))
    const mid = await screen(400, 300)
    await page.mouse.dblclick(mid.x, mid.y)
    const opened = await until(async () => (await editorOpen()) && (await editor.inputValue()) === 'Yes', 40)
    const field = await rectOf('board-text-editor')
    step("a connector's label opens on its place along the route", opened && field && near((field.x0 + field.x1) / 2, mid.x, 2) && near((field.y0 + field.y1) / 2, mid.y, 2), `field ${fmt(field)} at ${JSON.stringify(mid)}`)
    await editor.evaluate((field) => field.setSelectionRange(field.value.length, field.value.length))
    await page.keyboard.type(', ship it')
    await page.keyboard.press('Enter')
    step('and commits', await until(async () => (await storedOne(connector))?.text === 'Yes, ship it'), JSON.stringify((await storedOne(connector))?.text))
    const title = await screen(220, 410)
    await page.mouse.dblclick(title.x, title.y)
    const titleOpened = await until(async () => (await editorOpen()) && (await editor.inputValue()) === 'Plan', 40)
    const titleField = await rectOf('board-text-editor')
    const frameTop = (await screen(200, 420)).y
    step("a frame's title opens above the frame", titleOpened && titleField && titleField.y1 <= frameTop + 0.5 && near(titleField.x0, (await screen(200, 420)).x, 1), `field ${fmt(titleField)} frame top ${frameTop}`)
    await close()
  }],
  // Input methods: both Enter orders hold in the new box; what is composed is in the box.
  ['ime', async () => {
    const [shape] = await fresh([{ ...RECT, text: 'Plan' }])
    await openOn(shape)
    const ime = await editor.evaluate((field) => {
      const key = (init) => field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true, ...init }))
      const composition = (type) => field.dispatchEvent(new CompositionEvent(type, { bubbles: true, data: 'ni' }))
      composition('compositionstart')
      key({ isComposing: true, keyCode: 229 })
      composition('compositionend')
      const afterChromium = document.contains(field)
      composition('compositionstart')
      composition('compositionend')
      key({ isComposing: false, keyCode: 229 })
      return { afterChromium, afterWebKit: document.contains(field) }
    })
    step('an input-method Enter does not commit, in either order', ime.afterChromium && ime.afterWebKit && (await editorOpen()), JSON.stringify(ime))
    if (which === 'chromium') {
      // A real composition through the browser's input-method path (Chromium's DevTools protocol).
      const session = await page.context().newCDPSession(page)
      await editor.evaluate((field) => field.setSelectionRange(field.value.length, field.value.length))
      const composed = '计划很长的一段正在输入的中文文字会换行'
      await session.send('Input.imeSetComposition', { text: composed, selectionStart: composed.length, selectionEnd: composed.length })
      const during = await boxState()
      step('the text being composed is in the box, which grows with it', during && during.value.endsWith(composed) && !during.scrolls && during.lines.length > 1, JSON.stringify(during && { value: during.value, lines: during.lines }))
      await session.send('Input.insertText', { text: composed })
      await page.keyboard.press('Enter')
      step('the composed text commits with a plain Enter', await until(async () => (await storedOne(shape))?.text === `Plan${composed}`), JSON.stringify((await storedOne(shape))?.text))
    } else {
      await close()
    }
  }],
])
