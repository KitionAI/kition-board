// Diagrams by hand (plan "Board usable by hand", stages 2 to 6): node tests/hand/diagram.mjs [chromium|webkit] [scenario ...]
// Real mouse and keyboard events only. Stage 2: the selection bar shows next to a selected shape
// without covering it or its handles (below it when there is no room above, or when above would
// cover a neighbour); a red fill from the bar is one element.style that one Undo takes back, and the
// keys are the canvas's again; a selected connector becomes an elbow and takes another arrowhead; a
// selection of three is aligned left; the bar is gone while a shape is dragged and while a text box
// is open; the style sections come before the folded library when something is selected or a
// drawing tool is in hand, and the library is open on top when nothing is; a library cell dragged
// onto the canvas makes that shape where it was let go; the hint on an empty page; the shortcuts
// (? and the header's button); tool lock (Q). Stage 3, connectors: near a shape's edge the select
// tool shows its four side midpoints, and the connector tool over a shape outlines it with its own
// outline; an end within 16 px of a midpoint snaps to it; the line being drawn is the engine's own
// connector with its arrowhead and is the route it then commits; a connector drawn from a handle
// takes the default route and, let go on the empty canvas, ends free there; a re-attached end
// keeps the label; a bend added from a segment's middle point and removed by a double-click; an
// elbow's segment moved sideways; a connector with two free ends dragged whole; the connectors of a
// moving shape drawn live on the route they keep after the drop; a deleted shape leaves its
// connector's end free where it was. Later stages add their scenarios to this file.
// The cases under cases/board.editor.* and cases/regressions/issue-hand-* name these scenarios.
import { open } from './harness.mjs'

const which = process.argv[2] ?? 'chromium'
const h = await open(which)
const { page, kit, mod, step, until, board, ai, screen, boxOf, centre, testid, selection, tool, editorOpen, storedOne, near, fmt, fresh, drag } = h

/** A connector made by the agent on the person's page; answers its id once the canvas shows it. */
const connectorOf = async (params) => {
  const made = await ai([{ command: 'connector.add', params: { page: await h.personPage(), ...params } }])
  const id = made.completed[0].result.created[0]
  await until(async () => (await h.shown()).includes(id))
  return id
}
/** The engine's route of a stored connector (its corners). */
const routeOf = async (id) => (await board((id) => window.kitBoard.routes({ connectors: [{ id }] }), id))[0] ?? null
const hint = () => board(() => window.kitBoard.connectHint())
const draft = () => board(() => window.kitBoard.draft())
const pts = (points) => JSON.stringify((points ?? []).map((p) => [Math.round(p.x * 10) / 10, Math.round(p.y * 10) / 10]))
const samePoints = (a, b, by = 0.5) => a != null && b != null && a.length === b.length && a.every((p, i) => near(p.x, b[i].x, by) && near(p.y, b[i].y, by))
/** Presses at `from`, moves in steps to `to` and holds there until the preview has the last position; answers the draft, then releases. */
const holdAndRelease = async (from, to, { steps = 12, settled } = {}) => {
  await page.mouse.move(from.x, from.y)
  await page.mouse.down()
  for (let i = 1; i <= steps; i += 1) await page.mouse.move(from.x + ((to.x - from.x) * i) / steps, from.y + ((to.y - from.y) * i) / steps)
  let last = null
  await until(async () => {
    const now = await draft()
    const ok = now != null && (!settled || (await settled(now))) && JSON.stringify(now) === JSON.stringify(last)
    last = now
    return ok
  })
  const held = { draft: last, hint: await hint(), live: await kit.getByTestId('board-live-line').count(), target: await kit.getByTestId('board-connect-target').count() }
  await page.mouse.up()
  return held
}
/** The connector made by the last gesture: the newest connector the canvas shows. */
const newConnector = async (before) => {
  let found = null
  await until(async () => {
    const now = await h.shown()
    for (const id of now) if (!before.includes(id) && (await storedOne(id))?.kind === 'connector') found = id
    return found != null
  })
  return found
}

const RECT = { kind: 'shape', position: { x: 300, y: 300 }, size: { w: 160, h: 100 } }
const bar = kit.getByTestId('board-selection-bar')
const barCount = () => bar.count()
/** A page rectangle of the first element with `testId`, or null. */
const rectOf = async (testId) => {
  const b = await kit.getByTestId(testId).first().boundingBox().catch(() => null)
  return b ? { x0: b.x, y0: b.y, x1: b.x + b.width, y1: b.y + b.height } : null
}
/** An element's board box as a page rectangle. */
const pageRect = async (id) => {
  const b = await boxOf(id)
  if (!b) return null
  const a = await screen(b.x0, b.y0)
  const c = await screen(b.x1, b.y1)
  return { x0: a.x, y0: a.y, x1: c.x, y1: c.y }
}
const intersects = (a, b) => a != null && b != null && a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1
/** Selects `id` with a click on its middle; answers once the bar is shown. */
const select = async (id) => {
  const at = await centre(id)
  await page.mouse.click(at.x, at.y)
  await until(async () => JSON.stringify(await selection()) === JSON.stringify([id]))
  return until(async () => (await barCount()) === 1, 40)
}
/** Clicks the element with `testId` in the editor's frame by real mouse events. */
const press = async (testId) => {
  const at = await testid(testId)
  if (!at) return false
  await page.mouse.click(at.x, at.y)
  return true
}
const focused = () => board(() => document.activeElement?.getAttribute('data-testid') ?? document.activeElement?.tagName ?? null)

await h.run([
  ['setup', async () => {
    await h.newPage()
  }],
  // F11.3: a bar next to the selection; it covers neither the shape nor its handles.
  ['bar-shows', async () => {
    const [shape] = await fresh([RECT])
    const shown = await select(shape)
    const barBox = await rectOf('board-selection-bar')
    const shapeBox = await pageRect(shape)
    const handles = await Promise.all(['board-handle-rotate', 'board-connect-north', 'board-handle-north', 'board-handle-north-east'].map(rectOf))
    step('selecting a shape shows the selection bar above it', shown && barBox && shapeBox && barBox.y1 <= shapeBox.y0 && (await bar.getAttribute('data-side')) === 'above', `bar ${fmt(barBox)} shape ${fmt(shapeBox)}`)
    step('the bar covers neither the shape nor its handles', !intersects(barBox, shapeBox) && handles.every((handle) => handle && !intersects(barBox, handle)), `bar ${fmt(barBox)} handles ${handles.map(fmt).join(' ')}`)
    const middle = (barBox.x0 + barBox.x1) / 2
    step('the bar is centred on the selection', near(middle, (shapeBox.x0 + shapeBox.x1) / 2, 2), `${middle} vs ${(shapeBox.x0 + shapeBox.x1) / 2}`)
    const names = await bar.evaluate((node) => [...node.querySelectorAll('[data-testid]')].map((item) => item.getAttribute('data-testid')))
    step('a shape gets fill, text and line colour, text size and more', ['board-bar-fill', 'board-bar-stroke', 'board-bar-size-down', 'board-bar-size-up', 'board-bar-more'].every((name) => names.includes(name)) && !names.includes('board-bar-route-elbow') && !names.includes('board-bar-align'), JSON.stringify(names))
    await page.keyboard.press('Escape')
    step('Escape clears the selection and the bar goes', await until(async () => (await barCount()) === 0, 20))
    // With no room above the shape, the bar goes below it.
    const [top] = await fresh([{ ...RECT, position: { x: 300, y: 10 } }])
    await select(top)
    const under = await rectOf('board-selection-bar')
    const topBox = await pageRect(top)
    step('with no room above, the bar is below the shape', under && topBox && under.y0 >= topBox.y1 && (await bar.getAttribute('data-side')) === 'below', `bar ${fmt(under)} shape ${fmt(topBox)}`)
  }],
  // Above would cover the shape just over the selection: the bar goes below (a mind map's branches).
  ['bar-avoids', async () => {
    const [, lower] = await fresh([{ ...RECT, position: { x: 300, y: 200 }, size: { w: 160, h: 60 } }, { ...RECT, position: { x: 300, y: 290 }, size: { w: 160, h: 60 } }])
    await select(lower)
    const barBox = await rectOf('board-selection-bar')
    const upper = (await h.shown()).find((id) => id !== lower)
    step('the bar does not cover the shape above the selection', !intersects(barBox, await pageRect(upper)) && (await bar.getAttribute('data-side')) === 'below', `bar ${fmt(barBox)} upper ${fmt(await pageRect(upper))}`)
    const upperAt = await centre(upper)
    await page.mouse.click(upperAt.x, upperAt.y)
    step('so a click on that shape selects it', await until(async () => JSON.stringify(await selection()) === JSON.stringify([upper]), 20), JSON.stringify(await selection()))
  }],
  // F11.1: a red fill from the bar, one element.style; one Undo takes it back; the keys stay the canvas's.
  ['bar-fill', async () => {
    const [shape] = await fresh([RECT])
    await select(shape)
    await press('board-bar-fill')
    const swatch = await until(async () => (await kit.getByTestId('board-bar-fill-red').count()) === 1, 20)
    await press('board-bar-fill-red')
    const red = await until(async () => (await storedOne(shape))?.fillColor === 'red')
    const drawn = await until(async () => board((id) => document.querySelector(`[data-testid="board-elements"] [data-id="${id}"]`)?.innerHTML.includes('--kb-fill-red') ?? false, shape), 40)
    step('a red fill from the bar turns the shape red, drawn by the engine', swatch && red && drawn, `${JSON.stringify(await storedOne(shape))?.slice(0, 160)}`)
    step('the keys are the canvas\'s again after the choice', (await focused()) === 'board-canvas', `${await focused()}`)
    await page.keyboard.press(`${mod}+z`)
    const undone = await until(async () => (await storedOne(shape))?.fillColor == null)
    step('one Undo takes the fill back', undone && JSON.stringify(await selection()) === JSON.stringify([shape]), JSON.stringify(await storedOne(shape))?.slice(0, 160))
    await press('board-bar-size-up')
    const larger = await until(async () => ((await storedOne(shape))?.fontSize ?? 0) > 16)
    step('the bar\'s larger text is one element.style', larger, JSON.stringify((await storedOne(shape))?.fontSize))
  }],
  // F6.11: a selected connector becomes an elbow from the bar, and takes another arrowhead.
  ['bar-connector', async () => {
    const pageId = await h.personPage()
    await fresh([])
    const made = await ai([{ command: 'connector.add', params: { page: pageId, from: { point: [200, 300] }, to: { point: [600, 300] }, type: 'straight' } }])
    const connector = made.completed[0].result.created[0]
    await until(async () => (await h.shown()).includes(connector))
    const mid = await screen(400, 300)
    await page.mouse.click(mid.x, mid.y)
    await until(async () => JSON.stringify(await selection()) === JSON.stringify([connector]))
    const shown = await until(async () => (await barCount()) === 1 && (await kit.getByTestId('board-bar-route-elbow').count()) === 1, 40)
    step('a selected connector shows its route and arrowheads on the bar', shown && (await kit.getByTestId('board-bar-fill').count()) === 0 && (await kit.getByTestId('board-bar-end').count()) === 1)
    await press('board-bar-route-elbow')
    const elbow = await until(async () => (await storedOne(connector))?.connectorType === 'elbow')
    step('the bar makes the connector an elbow', elbow && (await kit.getByTestId('board-bar-route-elbow').getAttribute('aria-pressed')) === 'true', JSON.stringify(await storedOne(connector))?.slice(0, 200))
    await press('board-bar-end')
    await until(async () => (await kit.getByTestId('board-bar-end-dot').count()) === 1, 20)
    await press('board-bar-end-dot')
    step('and gives its end a dot', await until(async () => (await storedOne(connector))?.endArrowhead === 'dot'), JSON.stringify((await storedOne(connector))?.endArrowhead))
  }],
  // F11.5: several selected, align left from the bar; one Undo.
  ['bar-align', async () => {
    const ids = await fresh([
      { ...RECT, position: { x: 200, y: 160 }, size: { w: 120, h: 70 } },
      { ...RECT, position: { x: 420, y: 280 }, size: { w: 120, h: 70 } },
      { ...RECT, position: { x: 300, y: 400 }, size: { w: 120, h: 70 } },
    ])
    await drag(await screen(150, 120), await screen(620, 520))
    await until(async () => (await selection()).length === 3)
    const shown = await until(async () => (await barCount()) === 1, 40)
    await press('board-bar-align')
    await until(async () => (await kit.getByTestId('board-bar-align-left').count()) === 1, 20)
    await press('board-bar-align-left')
    const aligned = await until(async () => (await Promise.all(ids.map(storedOne))).every((element) => near(element?.position?.x, 200, 0.01)))
    step('several selected, the bar aligns their left edges', shown && aligned, JSON.stringify((await Promise.all(ids.map(storedOne))).map((element) => element?.position)))
    await page.keyboard.press(`${mod}+z`)
    const back = await until(async () => near((await storedOne(ids[1]))?.position?.x, 420, 0.01))
    step('one Undo puts them back', back, JSON.stringify((await storedOne(ids[1]))?.position))
  }],
  // The bar is not there while the shape is dragged, nor while its text box is open.
  ['bar-hidden', async () => {
    const [shape] = await fresh([RECT])
    await select(shape)
    const at = await centre(shape)
    let during = null
    await drag(at, { x: at.x + 160, y: at.y + 80 }, { mid: async () => (during = await barCount()) })
    await until(async () => ((await boxOf(shape))?.x0 ?? 0) > 400)
    const after = await until(async () => (await barCount()) === 1, 40)
    step('the bar is absent during a drag and back after the drop', during === 0 && after, `during ${during}, after ${after}`)
    await page.mouse.dblclick((await centre(shape)).x, (await centre(shape)).y)
    await until(editorOpen, 40)
    step('while the text box is open, its own bar shows and the selection bar does not', (await barCount()) === 0 && (await kit.getByTestId('board-text-marks').count()) === 1)
    await page.keyboard.press('Escape')
    await until(async () => !(await editorOpen()), 20)
  }],
  // F11.1, F11.2: with something selected the style sections come first and the library is one row.
  ['panel-order', async () => {
    const [shape] = await fresh([RECT])
    const state = kit.getByTestId('board-shape-library-state')
    const order = () => kit.getByTestId('board-panel-order').getAttribute('data-first')
    step('with nothing selected the library is open on top', (await order()) === 'library' && (await state.getAttribute('data-folded')) === 'false' && (await kit.getByTestId('board-shape-search').count()) === 1)
    await select(shape)
    await until(async () => (await order()) === 'style', 20)
    const style = await rectOf('board-style-panel')
    const library = await rectOf('board-shape-library')
    const fill = await rectOf('board-style-fillColor-white')
    step('with a shape selected the style sections are above the library', style && library && style.y0 < library.y0 && (await state.getAttribute('data-folded')) === 'true', `style ${fmt(style)} library ${fmt(library)}`)
    step('and the fill swatches are on the first screen without scrolling', fill && fill.y1 <= 900, fmt(fill))
    const folded = await rectOf('board-shape-library-row')
    step('the folded library is one row', folded && folded.y1 - folded.y0 < 56, fmt(folded))
    await page.keyboard.press('Escape')
    await until(async () => (await order()) === 'library', 20)
    await page.keyboard.press('p')
    const drawing = await until(async () => (await order()) === 'style' && (await kit.getByTestId('board-style-panel').getAttribute('data-target')) === 'defaults', 20)
    step('with a drawing tool in hand the style sections come first too', drawing, `${await order()}`)
    await page.keyboard.press('Escape')
    await until(async () => (await tool()) === 'select', 20)
  }],
  // F4.7: a library cell dragged onto the canvas makes that shape where it is let go.
  ['library-drag', async () => {
    await fresh([])
    const cell = await testid('board-library-diamond')
    const to = await screen(520, 380)
    await page.mouse.move(cell.x, cell.y)
    await page.mouse.down()
    const steps = 12
    for (let i = 1; i <= steps; i += 1) await page.mouse.move(cell.x + ((to.x - cell.x) * i) / steps, cell.y + ((to.y - cell.y) * i) / steps)
    const preview = await kit.locator('[data-testid="board-overlay"] .board-brush').count()
    await page.mouse.up()
    await until(async () => (await h.shown()).length === 1)
    const [made] = await h.shown()
    const element = made ? await storedOne(made) : null
    const box = made ? await boxOf(made) : null
    step(
      'a library cell dragged to the canvas makes that shape centred where it was let go',
      element?.shapeType === 'diamond' && box && near((box.x0 + box.x1) / 2, 520, 2) && near((box.y0 + box.y1) / 2, 380, 2) && preview === 1,
      `${JSON.stringify(element)?.slice(0, 160)} ${fmt(box)} preview ${preview}`,
    )
    step('the new shape is selected and the tool is still Select', JSON.stringify(await selection()) === JSON.stringify([made]) && (await tool()) === 'select', `${JSON.stringify(await selection())} ${await tool()}`)
    // A click on a cell still picks the shape tool.
    await kit.getByTestId('board-library-cylinder').click()
    step('a click on a cell still makes it the shape tool', await until(async () => (await tool()) === 'shape', 20), `${await tool()}`)
    await kit.getByTestId('board-canvas').focus()
    await page.keyboard.press('Escape')
    await page.keyboard.press('Escape')
  }],
  // F1.4: three sentences on an empty page, gone as soon as an element exists; presses go through.
  ['empty-hint', async () => {
    await fresh([])
    const hint = kit.getByTestId('board-empty-hint')
    const sentences = await hint.locator('p').count()
    const through = await hint.evaluate((node) => getComputedStyle(node).pointerEvents).catch(() => null)
    step('an empty page shows a hint of three sentences that presses go through', sentences === 3 && through === 'none', `${sentences} ${through}`)
    await page.keyboard.press('r')
    const at = await screen(400, 300)
    await page.mouse.click(at.x, at.y)
    const gone = await until(async () => (await h.shown()).length === 1 && (await hint.count()) === 0, 40)
    step('the hint goes once the page holds an element', gone, `${(await h.shown()).length} ${await hint.count()}`)
    await page.keyboard.press('Escape')
  }],
  // F11.6: ? and the header's button show the shortcuts; the keys come back to the canvas.
  ['shortcuts', async () => {
    await fresh([RECT])
    await page.keyboard.press('?')
    const opened = await until(async () => (await kit.getByTestId('board-shortcuts').count()) === 1, 20)
    const text = opened ? await kit.getByTestId('board-shortcuts').textContent() : ''
    step('? shows the keyboard shortcuts', opened && text.includes('Undo') && text.includes('Tool lock'), text.slice(0, 120))
    await page.keyboard.press('Escape')
    await until(async () => (await kit.getByTestId('board-shortcuts').count()) === 0, 20)
    await page.keyboard.press('n')
    step('closed, the keys go to the canvas again', await until(async () => (await tool()) === 'note', 20), `${await tool()}`)
    await page.keyboard.press('Escape')
    await kit.getByTestId('board-shortcuts-open').click()
    step("the header's button shows them too", await until(async () => (await kit.getByTestId('board-shortcuts').count()) === 1, 20))
    await page.keyboard.press('Escape')
    await until(async () => (await kit.getByTestId('board-shortcuts').count()) === 0, 20)
  }],
  // F4.4: tool lock (Q) keeps the drawing tool after each shape.
  ['tool-lock', async () => {
    await fresh([])
    await page.keyboard.press('q')
    const on = await until(async () => (await kit.getByTestId('board-tool-lock').count()) === 1, 20)
    await page.keyboard.press('r')
    await drag(await screen(100, 100), await screen(220, 180))
    await until(async () => (await h.shown()).length === 1)
    const kept = (await tool()) === 'shape'
    await drag(await screen(300, 100), await screen(420, 180))
    await until(async () => (await h.shown()).length === 2)
    step('with tool lock on, the shape tool stays after each shape', on && kept && (await tool()) === 'shape' && (await h.shown()).length === 2, `lock ${on}, tool ${await tool()}, ${(await h.shown()).length} shapes`)
    await page.keyboard.press('q')
    const off = await until(async () => (await kit.getByTestId('board-tool-lock').count()) === 0, 20)
    await drag(await screen(500, 100), await screen(620, 180))
    await until(async () => (await h.shown()).length === 3)
    step('Q again turns it off: back to Select after the next shape', off && (await until(async () => (await tool()) === 'select', 20)), `tool ${await tool()}`)
    await page.keyboard.press('Escape')
  }],
  // F6.1, F6.4: near a shape's edge the select tool shows its four side midpoints; the connector tool
  // over a shape outlines it with its own outline (an ellipse's, not a rectangle) and shows them too.
  ['connect-points', async () => {
    const [shape, oval] = await fresh([RECT, { ...RECT, shapeType: 'ellipse', position: { x: 600, y: 300 } }])
    const edge = await screen(300 + 6, 330)
    await page.mouse.move(edge.x - 40, edge.y)
    await page.mouse.move(edge.x, edge.y, { steps: 3 })
    const shown = await until(async () => (await hint())?.element === shape, 20)
    const now = await hint()
    const expected = { north: [380, 300], east: [460, 350], south: [380, 400], west: [300, 350] }
    const placed = now?.points?.every((p) => near(p.x, expected[p.side][0], 0.5) && near(p.y, expected[p.side][1], 0.5))
    step('near a shape\'s edge the select tool shows its four side midpoints', shown && now.mode === 'hover' && now.points.length === 4 && placed && (await kit.locator('[data-testid^="board-connect-point-"]').count()) === 4, JSON.stringify(now))
    const middle = await centre(shape)
    await page.mouse.move(middle.x, middle.y, { steps: 3 })
    step('deep inside the shape they go (the shape is still outlined to move)', await until(async () => (await hint()) == null, 20) && (await board(() => window.kitBoard.hover())) === shape)
    // Pressed and dragged, a midpoint starts a connector from that side.
    const west = await screen(300, 350)
    await page.mouse.move(west.x - 30, west.y, { steps: 2 })
    await page.mouse.move(west.x, west.y, { steps: 2 })
    await until(async () => (await hint())?.side === 'west', 20)
    const before = await h.shown()
    await drag(west, await screen(150, 150))
    const made = await newConnector(before)
    const stored = made ? await storedOne(made) : null
    step('a midpoint dragged out starts a connector from that side', stored?.start?.binding?.target === shape && JSON.stringify(stored?.start?.binding?.anchor) === '[0,0.5]' && stored?.end?.binding == null, JSON.stringify(stored)?.slice(0, 240))
    await page.keyboard.press('Escape')
    await page.keyboard.press('a')
    const ovalAt = await centre(oval)
    await page.mouse.move(ovalAt.x - 20, ovalAt.y - 10, { steps: 3 })
    const target = await until(async () => (await hint())?.element === oval && (await kit.getByTestId('board-connect-target').count()) === 1, 20)
    const tag = await kit.getByTestId('board-connect-target').evaluate((node) => `${node.tagName}:${(node.getAttribute('d') ?? '').includes('A')}`).catch(() => null)
    step('the connector tool over an ellipse outlines it with its own outline and shows its four points', target && tag === 'path:true' && (await kit.locator('[data-testid^="board-connect-point-"]').count()) === 4, `${tag} ${JSON.stringify(await hint())}`)
    await page.keyboard.press('Escape')
    await until(async () => (await tool()) === 'select', 20)
  }],
  // F6.12: within 16 px of a side midpoint the end snaps to it.
  ['connect-snap', async () => {
    const [a, b] = await fresh([{ ...RECT, position: { x: 100, y: 300 } }, { ...RECT, position: { x: 600, y: 200 } }])
    await page.keyboard.press('a')
    const before = await h.shown()
    // 10 units left of and 7 below B's west midpoint (600, 250): outside B, near the point.
    const held = await holdAndRelease(await centre(a), await screen(590, 257), { settled: async () => (await hint())?.side === 'west' })
    const made = await newConnector(before)
    const stored = made ? await storedOne(made) : null
    step('within 16 px of a side midpoint the end snaps to it', held.hint?.element === b && held.hint?.side === 'west' && stored?.end?.binding?.target === b && JSON.stringify(stored?.end?.binding?.anchor) === '[0,0.5]', `${JSON.stringify(held.hint)} ${JSON.stringify(stored?.end)}`)
    step('the snapped midpoint is marked while the pointer is there', held.draft?.[0]?.end?.target === b && near(held.draft?.[0]?.end?.point?.x, 600, 0.01) && near(held.draft?.[0]?.end?.point?.y, 250, 0.01), JSON.stringify(held.draft?.[0]?.end))
    await page.keyboard.press('Escape')
    // Pressed deep inside a shape that lies over another, near that one's edge: the end is the top shape's.
    const [under, top, far] = await fresh([{ ...RECT, position: { x: 100, y: 200 } }, { ...RECT, position: { x: 140, y: 240 } }, { ...RECT, position: { x: 600, y: 200 } }])
    await page.keyboard.press('a')
    const others = await h.shown()
    await drag(await screen(220, 292), await centre(far))
    const over = await newConnector(others)
    const overStored = over ? await storedOne(over) : null
    step('over a shape, an edge of one under it does not take the end', overStored?.start?.binding?.target === top && overStored?.start?.binding?.target !== under, JSON.stringify(overStored?.start))
    await page.keyboard.press('Escape')
  }],
  // F6.3, F3.2: the line being drawn is the engine's connector, with its arrowhead and the target
  // outlined, and it is the route the release commits. A connector from a shape's handle takes the
  // drawing style's route (straight by default), not a forced elbow.
  ['connect-draft', async () => {
    const [a, b] = await fresh([{ ...RECT, position: { x: 100, y: 300 } }, { ...RECT, shapeType: 'diamond', position: { x: 600, y: 120 } }])
    await page.keyboard.press('a')
    const before = await h.shown()
    const held = await holdAndRelease(await centre(a), await centre(b))
    const made = await newConnector(before)
    const committed = made ? await routeOf(made) : null
    const route = held.draft?.[0]
    step('the line being drawn has its arrowhead, and the target is outlined', held.live === 1 && route?.heads?.length === 1 && route.heads[0].kind === 'arrow' && held.target === 1 && held.hint?.element === b, `${held.live} ${JSON.stringify(route?.heads)} ${held.target}`)
    step('the line being drawn is the route the release commits', samePoints(route?.points, committed?.points) && route?.path === committed?.path, `${pts(route?.points)} vs ${pts(committed?.points)}`)
    await page.keyboard.press('Escape')
    await page.keyboard.press('Escape')
    const at = await centre(a)
    await page.mouse.click(at.x, at.y)
    await until(async () => (await kit.getByTestId('board-connect-east').count()) === 1, 20)
    const handle = await testid('board-connect-east')
    const others = await h.shown()
    await drag(handle, await centre(b))
    const fromHandle = await newConnector(others)
    const stored = fromHandle ? await storedOne(fromHandle) : null
    step('a connector from a handle takes the default route, not an elbow', stored != null && (stored.connectorType ?? 'straight') === 'straight' && stored.end?.binding?.target === b, JSON.stringify(stored)?.slice(0, 240))
  }],
  // F6.6: from a handle to the empty canvas, a straight dangling end where it was let go.
  ['connect-empty', async () => {
    const [a] = await fresh([{ ...RECT, position: { x: 200, y: 200 } }])
    const at = await centre(a)
    await page.mouse.click(at.x, at.y)
    await until(async () => (await kit.getByTestId('board-connect-east').count()) === 1, 20)
    const before = await h.shown()
    await drag(await testid('board-connect-east'), await screen(560, 520))
    const made = await newConnector(before)
    const stored = made ? await storedOne(made) : null
    const route = made ? await routeOf(made) : null
    const end = Array.isArray(stored?.end?.point) ? { x: stored.end.point[0], y: stored.end.point[1] } : stored?.end?.point
    step('let go on the empty canvas, the end is free where it was let go', stored?.end?.binding == null && near(end?.x, 560, 1) && near(end?.y, 520, 1) && stored?.start?.binding?.target === a, JSON.stringify(stored)?.slice(0, 240))
    step('and the line is straight, not looping back through the shape', route?.points?.length === 2, pts(route?.points))
  }],
  // F6.8: an end moved to another shape keeps the label (shown while dragged and after).
  ['rebind-label', async () => {
    const [a, b, c] = await fresh([{ ...RECT, position: { x: 100, y: 200 } }, { ...RECT, position: { x: 600, y: 200 } }, { ...RECT, shapeType: 'ellipse', position: { x: 600, y: 500 } }])
    const connector = await connectorOf({ from: { element: a }, to: { element: b }, label: 'yes' })
    const on = await screen(380, 250)
    await page.mouse.click(on.x - 60, on.y)
    await until(async () => JSON.stringify(await selection()) === JSON.stringify([connector]))
    await until(async () => (await kit.getByTestId('board-connector-end').count()) === 1, 20)
    const held = await holdAndRelease(await testid('board-connector-end'), await centre(c), { settled: async (now) => now?.[0]?.end?.target === c })
    step('while the end is dragged the label is drawn with the line', held.draft?.[0]?.text != null && held.hint?.element === c, JSON.stringify(held.draft?.[0]?.text))
    const after = await until(async () => (await storedOne(connector))?.end?.binding?.target === c)
    const drawn = await until(async () => board((id) => document.querySelector(`[data-testid="board-elements"] [data-id="${id}"]`)?.textContent?.includes('yes') ?? false, connector), 40)
    step('re-attached to another shape, the connector keeps its label', after && (await storedOne(connector))?.text === 'yes' && drawn, JSON.stringify(await storedOne(connector))?.slice(0, 240))
  }],
  // F6.10, F2.5: a selected connector is highlighted along its line (no box); a segment's middle
  // point dragged adds a bend, a double-click on the bend removes it; an elbow's segment moves sideways.
  ['bends', async () => {
    await fresh([])
    const line = await connectorOf({ from: { point: [200, 300] }, to: { point: [600, 300] }, type: 'straight' })
    const on = await screen(300, 300)
    await page.mouse.click(on.x, on.y)
    await until(async () => JSON.stringify(await selection()) === JSON.stringify([line]))
    const ready = await until(async () => (await kit.getByTestId('board-connector-mid-0').count()) === 1, 20)
    step('a selected connector is highlighted along its line, not boxed', (await kit.getByTestId('board-connector-highlight').count()) === 1 && (await kit.getByTestId('board-selection-frame').count()) === 0 && ready)
    const middle = await testid('board-connector-mid-0')
    const there = await screen(400, 300)
    step('the middle point is in the middle of the segment', near(middle?.x, there.x, 1.5) && near(middle?.y, there.y, 1.5), `${JSON.stringify(middle)} vs ${JSON.stringify(there)}`)
    await drag(middle, await screen(400, 180))
    const bent = await until(async () => {
      const w = (await storedOne(line))?.waypoints
      return Array.isArray(w) && w.length === 1 && near(w[0][0] ?? w[0].x, 400, 1) && near(w[0][1] ?? w[0].y, 180, 1)
    })
    step('dragging the middle point adds a bend there (connector.setWaypoints)', bent, JSON.stringify((await storedOne(line))?.waypoints))
    await until(async () => (await kit.getByTestId('board-connector-waypoint-0').count()) === 1, 20)
    const bend = await testid('board-connector-waypoint-0')
    await page.mouse.dblclick(bend.x, bend.y)
    const removed = await until(async () => ((await storedOne(line))?.waypoints ?? []).length === 0)
    step('a double-click on the bend removes it', removed && !(await editorOpen()), JSON.stringify((await storedOne(line))?.waypoints))
    await page.keyboard.press('Escape')
    // An elbow's inner segment moves sideways and the route keeps its right angles.
    const [a, b] = await fresh([{ ...RECT, position: { x: 100, y: 200 } }, { ...RECT, position: { x: 600, y: 450 } }])
    const elbow = await connectorOf({ from: { element: a, anchor: [1, 0.5] }, to: { element: b, anchor: [0, 0.5] }, type: 'elbow' })
    const start = await screen(300, 250)
    await page.mouse.click(start.x, start.y)
    await until(async () => JSON.stringify(await selection()) === JSON.stringify([elbow]))
    await until(async () => (await kit.getByTestId('board-connector-mid-1').count()) === 1, 20)
    const inner = await testid('board-connector-mid-1')
    await drag(inner, { x: inner.x + 60, y: inner.y + 20 })
    const moved = await until(async () => ((await storedOne(elbow))?.waypoints ?? []).length === 2)
    const corners = (await routeOf(elbow))?.points ?? []
    const square = corners.length >= 3 && corners.slice(1).every((p, i) => near(p.x, corners[i].x, 0.01) || near(p.y, corners[i].y, 0.01))
    step("an elbow's middle segment moves sideways and keeps its right angles", moved && square && near(corners[1]?.x, 490, 1), pts(corners))
  }],
  // A connector with both ends free is dragged as a whole.
  ['free-drag', async () => {
    await fresh([])
    const line = await connectorOf({ from: { point: [200, 300] }, to: { point: [500, 400] } })
    const on = await screen(275, 325)
    await page.mouse.click(on.x, on.y)
    await until(async () => JSON.stringify(await selection()) === JSON.stringify([line]))
    await drag(on, { x: on.x + 100, y: on.y - 60 })
    const point = (end) => (Array.isArray(end?.point) ? { x: end.point[0], y: end.point[1] } : end?.point)
    const moved = await until(async () => near(point((await storedOne(line))?.start)?.x, 300, 1))
    const stored = await storedOne(line)
    step('a connector with two free ends is dragged whole', moved && near(point(stored.start).y, 240, 1) && near(point(stored.end).x, 600, 1) && near(point(stored.end).y, 340, 1), JSON.stringify([stored?.start, stored?.end]))
  }],
  // F3.2: while a shape moves, its connectors are drawn live on the route they have after the drop,
  // with their arrowhead and label.
  ['move-live', async () => {
    const [a, b] = await fresh([{ ...RECT, position: { x: 100, y: 200 } }, { ...RECT, position: { x: 600, y: 450 } }])
    const connector = await connectorOf({ from: { element: a, anchor: [1, 0.5] }, to: { element: b, anchor: [0, 0.5] }, type: 'elbow', label: 'next' })
    const grab = await screen(140, 230)
    const held = await holdAndRelease(grab, { x: grab.x + 90, y: grab.y + 150 }, { settled: async (now) => (now?.[0]?.start?.point?.y ?? 0) > 300 })
    const route = held.draft?.find((item) => item.id === connector)
    await until(async () => ((await boxOf(a))?.y0 ?? 0) > 300)
    const committed = await routeOf(connector)
    step('while a shape moves, its connector is drawn on its real route with arrowhead and label', route != null && held.live === 1 && route.heads.length === 1 && route.text != null && route.points.length === 4, `${held.live} ${pts(route?.points)}`)
    step('the live route is the route after the drop', samePoints(route?.points, committed?.points), `${pts(route?.points)} vs ${pts(committed?.points)}`)
  }],
  // F6.13: a deleted shape leaves its connector, that end free where it was.
  ['delete-shape', async () => {
    const [a, b] = await fresh([{ ...RECT, position: { x: 100, y: 200 } }, { ...RECT, position: { x: 600, y: 200 } }])
    const connector = await connectorOf({ from: { element: a }, to: { element: b } })
    const before = await routeOf(connector)
    const at = await centre(b)
    await page.mouse.click(at.x, at.y)
    await until(async () => JSON.stringify(await selection()) === JSON.stringify([b]))
    await page.keyboard.press('Delete')
    await until(async () => !(await h.shown()).includes(b))
    const stored = await storedOne(connector)
    const after = await routeOf(connector)
    step('deleting a shape leaves its connector, that end free where it was', stored != null && stored.end?.binding == null && stored.start?.binding?.target === a && samePoints(after?.points, before?.points, 0.01), `${JSON.stringify(stored)?.slice(0, 200)} ${pts(after?.points)}`)
  }],
])
