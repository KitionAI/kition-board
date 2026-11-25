// The shape library panel in a real browser, through the development host: node tests/shapes.mjs [chromium|webkit]
// Starts the Vite dev server itself, opens dev.html (Sample.kboard) in Classic, goes to a page of its
// own at a known view, and prints PASS or FAIL per step: the library's four groups and 44 shapes,
// each pictured by the engine (`shape.previews`); the search; five library shapes drawn on the
// canvas, each stored with its shapeType and drawn by the engine; the rail's shape slot lit for a
// shape it does not hold; and the five shapes after a save through the host and a reopen
// (case board.shapes/library-panel); then a connector end dropped on a diamond, derived by the page as
// the engine derives it and on the diamond's outline, not its box (case board.connectors/outline-ends-page).
// The harness is web/tests/flow.mjs's.
// Run with the proxy variables unset: the dev server is on loopback.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
// Headless Chromium on Linux hints glyphs unless told not to (as in flow.mjs).
const browser = await (which === 'webkit' ? webkit.launch() : chromium.launch({ args: process.platform === 'linux' ? ['--font-render-hinting=none'] : [] }))
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })
const page = await context.newPage()
// The editor's shortcuts take Cmd where the page says it runs on a Mac (kit-editor reads
// navigator.platform), else Ctrl. Playwright's ControlOrMeta follows the machine running the test
// instead, and its WebKit says Mac on Linux too, so the test asks the page.
const mod = (await page.evaluate(() => /Mac|iPhone|iPad/i.test(navigator.platform))) ? 'Meta' : 'Control'
// A missing element fails its step in seconds, not after Playwright's half minute per try.
context.setDefaultTimeout(10000)
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
// What reloaded or crashed, for a run that loses the editor's frame.
const lifecycle = []
page.on('framenavigated', (navigated) => lifecycle.push(`navigated ${navigated === page.mainFrame() ? 'page' : 'frame'} ${navigated.url().replace(/\?.*$/, '')}`))
page.on('crash', () => lifecycle.push('page crashed'))
const warnings = []
page.on('console', (message) => {
  if (message.type() === 'warning' || message.type() === 'error') warnings.push(message.text())
})
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
}
const until = async (check, tries = 100) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
const ai = (steps) => page.evaluate(async (steps) => window.devHost.connection().ai.run(steps, false), steps)
const inspect = () => page.evaluate(async () => (await window.devHost.connection().ai.inspect()).details ?? (await window.devHost.connection().ai.inspect()))
const elementCount = async () => (await inspect()).elements
const copy = async (ids) => (await ai([{ command: 'edit.copy', params: { ids } }])).completed[0].result

/** The library's shapes, with the English names the search matches (strings.ts, en-US). */
const PICKED = ['document', 'cylinder', 'callout', 'pentagon', 'brace-left']

try {
  await page.goto(`${origin}/dev.html?modes=classic`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90000 })
  const kit = page.frameLocator('#kit')
  const frame = await (await page.$('#kit')).contentFrame()
  await frame.waitForFunction(() => window.kitBoard != null, null, { timeout: 60000 })
  await frame.evaluate(() => document.fonts.ready)
  const board = (fn, arg) => frame.evaluate(fn, arg)
  const svgBox = async () => kit.getByTestId('board-svg').boundingBox()
  /** The screen point of world point (x, y). */
  const screen = async (x, y) => {
    const box = await svgBox()
    const v = await board(() => window.kitBoard.viewport())
    return { x: box.x + (x - v.x) * v.zoom, y: box.y + (y - v.y) * v.zoom }
  }
  const drag = async (from, to, steps = 12) => {
    await page.mouse.move(from.x, from.y)
    await page.mouse.down()
    await page.mouse.move(to.x, to.y, { steps })
    await page.mouse.up()
  }
  const personPage = async () => (await inspect()).page
  /** The person's New page, from the View tab. */
  const newPage = async () => {
    await kit.getByTestId('ribbon-tab-View').click()
    await kit.getByTestId('board-cmd-page.add').click()
    await kit.getByTestId('ribbon-tab-Edit').click()
  }
  /** Copies `ids` and answers their stored elements. */
  const storedOf = async (ids) => {
    const payload = await copy(ids)
    return payload.payload?.elements ?? payload.elements ?? []
  }
  /** An attribute of the first element with `testId`, null when there is none (without waiting for one). */
  const attr = (testId, name) => kit.getByTestId(testId).evaluateAll((nodes, name) => nodes[0]?.getAttribute(name) ?? null, name)
  /** The board's element count, noting what the canvas shows now (for `newest`). */
  let shownBefore = []
  const mark = async () => {
    shownBefore = await board(() => window.kitBoard.shown())
    return elementCount()
  }
  /** Once the board holds `count` elements, the one the canvas shows that it did not at `mark`. */
  const newest = async (count) => {
    await until(async () => (await elementCount()) === count)
    let made
    await until(async () => {
      made = (await board(() => window.kitBoard.shown())).filter((id) => !shownBefore.includes(id)).at(-1)
      return made != null
    })
    return made
  }

  // The library's own page, at a known view (100%, world origin at the canvas corner).
  const pageBefore = await personPage()
  await newPage()
  await until(async () => (await personPage()) !== pageBefore && (await board(() => window.kitBoard.shown().length)) === 0)
  await board(() => window.kitBoard.setViewport({ x: 0, y: 0, zoom: 1 }))

  // 1. Four groups and 44 shapes, each pictured by the engine. The engine answers once the board is open.
  const buttons = kit.locator('[data-testid^="board-library-"]')
  await until(async () => (await buttons.count()) === 44, 200)
  const groups = await kit.locator('[data-testid^="board-shape-group-"]').evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-testid')))
  const headings = await kit.locator('[data-testid="board-shape-library"] .board-library-heading').allTextContents()
  const shapeCount = await buttons.count()
  const pictured = await buttons.evaluateAll((nodes) => nodes.filter((node) => node.querySelector('svg path, svg polygon, svg rect, svg ellipse, svg line') != null).length)
  step(
    'the library shows four groups and 44 shapes',
    JSON.stringify(groups) === JSON.stringify(['board-shape-group-basic', 'board-shape-group-flowchart', 'board-shape-group-arrows', 'board-shape-group-marks']) &&
      JSON.stringify(headings) === JSON.stringify(['Basic', 'Flowchart', 'Arrows', 'Marks']) &&
      shapeCount === 44 &&
      pictured === 44,
    `${JSON.stringify(groups)} ${JSON.stringify(headings)} ${shapeCount} shapes, ${pictured} pictured`,
  )

  // 2. The search keeps the shapes whose name holds it (any case), and all of them again when cleared.
  const search = kit.getByTestId('board-shape-search')
  await search.fill('DATA')
  await until(async () => (await buttons.count()) === 2)
  const found = (await buttons.evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-testid')))).sort()
  const foundGroups = await kit.locator('[data-testid^="board-shape-group-"]').count()
  await search.fill('')
  await until(async () => (await buttons.count()) === 44)
  const cleared = await buttons.count()
  step(
    'search narrows the library',
    JSON.stringify(found) === JSON.stringify(['board-library-cylinder', 'board-library-stored-data']) && foundGroups === 1 && cleared === 44,
    `${JSON.stringify(found)} in ${foundGroups} group(s); ${cleared} after clearing`,
  )

  // 3 and 4. Each picked shape becomes the shape tool's; a drag draws it, one element.add.
  const made = {}
  for (const [at, shape] of PICKED.entries()) {
    const button = kit.getByTestId(`board-library-${shape}`)
    await button.click()
    await until(async () => (await button.getAttribute('aria-pressed')) === 'true' && (await attr('board-canvas', 'data-tool')) === 'shape')
    if (shape === 'document') {
      // The rail's 18 do not hold the document: its slot lights as the rectangle.
      const lit = await until(async () => (await kit.locator('.kui-tool-rail [data-testid="board-tool-shape:rectangle"][aria-pressed=true]').count()) === 1)
      step("the rail's shape slot is lit for a library shape", lit && (await button.getAttribute('aria-pressed')) === 'true' && (await attr('board-canvas', 'data-tool')) === 'shape', `${await attr('board-canvas', 'data-tool')} ${await button.getAttribute('aria-pressed')}`)
    }
    const count = await mark()
    // Two rows of three slots, 240 × 200 apart, clear of each other and of the canvas's edges.
    const x0 = 80 + (at % 3) * 240
    const y0 = 80 + Math.floor(at / 3) * 200
    await drag(await screen(x0, y0), await screen(x0 + 140, y0 + 90))
    const id = await newest(count + 1)
    made[shape] = id
    const stored = id ? (await storedOf([id])).find((element) => element.kind === 'shape') : null
    const drawn = id ? await until(async () => board((id) => document.querySelector(`[data-id="${id}"]`)?.querySelector('path, polygon, rect, ellipse, line') != null, id), 50) : false
    step(`draws the chosen library shape: ${shape}`, stored?.shapeType === shape && drawn, `${id} ${JSON.stringify(stored ?? null).slice(0, 200)} drawn ${drawn} ${warnings.slice(-3).join(' | ')}`)
  }

  // 5. Saved through the host and opened again, the five keep their shapes.
  await kit.getByTestId('board-canvas').focus()
  await page.keyboard.press(`${mod}+s`)
  await until(async () => (await page.evaluate(() => window.devHost.files.get('Sample.kboard')?.version ?? 0)) > 1)
  const saved = (await page.evaluate(() => window.devHost.files.get('Sample.kboard')?.version ?? 0)) > 1
  await page.evaluate(async () => window.devHost.open('Sample.kboard', window.devHost.files.get('Sample.kboard').bytes))
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await frame.waitForFunction(() => window.kitBoard != null, null, { timeout: 60000 })
  const ids = PICKED.map((shape) => made[shape]).filter(Boolean)
  let reopened = []
  await until(async () => {
    reopened = await storedOf(ids)
    return reopened.length === PICKED.length
  })
  const kept = PICKED.every((shape) => reopened.some((element) => element.ref === made[shape] && element.shapeType === shape))
  step(
    'a library shape survives save and reopen',
    saved && ids.length === PICKED.length && kept,
    `saved ${saved}; ${JSON.stringify(reopened.map((element) => [element.ref, element.shapeType]))}`,
  )

  // 6 and 7. A connector end dropped on a diamond meets the diamond's outline, not its box, and the
  // page derives it as the engine does (design note 1.4; case board.connectors/outline-ends-page).
  // On a page of its own at a known view, a diamond and a rectangle drawn from the library, then the
  // connector tool from the rectangle's centre to just inside the diamond's upper-right side.
  const outlinePageBefore = await personPage()
  await newPage()
  await until(async () => (await personPage()) !== outlinePageBefore && (await board(() => window.kitBoard.shown().length)) === 0)
  await board(() => window.kitBoard.setViewport({ x: 0, y: 0, zoom: 1 }))
  /** Draws `shape` from the library by a drag over (x0, y0, w, h); answers its id. */
  const drawShape = async (shape, x0, y0, w, h) => {
    const button = kit.getByTestId(`board-library-${shape}`)
    await button.click()
    await until(async () => (await button.getAttribute('aria-pressed')) === 'true' && (await attr('board-canvas', 'data-tool')) === 'shape')
    const count = await mark()
    await drag(await screen(x0, y0), await screen(x0 + w, y0 + h))
    return newest(count + 1)
  }
  /** A stored element's box, whichever form its position and size take. */
  const boxOf = (element) => {
    const [x, y] = Array.isArray(element?.position) ? element.position : [element?.position?.x, element?.position?.y]
    const [w, h] = Array.isArray(element?.size) ? element.size : [element?.size?.w, element?.size?.h]
    return [x, y, w, h].every(Number.isFinite) ? { cx: x + w / 2, cy: y + h / 2, hw: w / 2, hh: h / 2 } : null
  }
  const diamond = await drawShape('diamond', 420, 120, 200, 120)
  const rectangle = await drawShape('rectangle', 80, 320, 140, 90)
  const shapesStored = diamond && rectangle ? await storedOf([diamond, rectangle]) : []
  const diamondBox = boxOf(shapesStored.find((element) => element.ref === diamond))
  const rectangleBox = boxOf(shapesStored.find((element) => element.ref === rectangle))
  let connector = null
  if (diamondBox && rectangleBox) {
    await kit.locator('.kui-tool-rail [data-testid="board-tool-connector"]').click()
    await until(async () => (await attr('board-canvas', 'data-tool')) === 'connector')
    // The middle of the diamond's upper-right side, pulled 15% toward its centre: on the diamond,
    // within the 16-unit reach that pins an end where the pointer is.
    const { cx, cy, hw, hh } = diamondBox
    const drop = { x: cx + 0.85 * (hw / 2), y: cy - 0.85 * (hh / 2) }
    const count = await mark()
    await drag(await screen(rectangleBox.cx, rectangleBox.cy), await screen(drop.x, drop.y))
    connector = await newest(count + 1)
  }
  const stored = connector ? (await storedOf([connector])).find((element) => element.kind === 'connector') : null
  const derived = connector ? await board((id) => window.kitBoard.endPoints(id), connector) : []
  const engineEnds = [stored?.start?.point, stored?.end?.point]
  const off =
    derived.length >= 2 && engineEnds.every(Array.isArray)
      ? Math.max(Math.hypot(derived[0].x - engineEnds[0][0], derived[0].y - engineEnds[0][1]), Math.hypot(derived.at(-1).x - engineEnds[1][0], derived.at(-1).y - engineEnds[1][1]))
      : Infinity
  step(
    "the page's derived end on a diamond matches the engine's",
    off < 0.01 && stored?.start?.binding?.target === rectangle && stored?.end?.binding?.target === diamond,
    `${off} ${connector} ${JSON.stringify(stored ?? null).slice(0, 300)} ${JSON.stringify(derived)}`,
  )
  const end = derived.at(-1)
  const onOutline = end && diamondBox ? Math.abs(end.x - diamondBox.cx) / diamondBox.hw + Math.abs(end.y - diamondBox.cy) / diamondBox.hh : NaN
  step(
    "the end is on the diamond's outline, not its box",
    Math.abs(onOutline - 1) <= 0.02 && end.x > diamondBox.cx && end.y < diamondBox.cy,
    `${JSON.stringify(end ?? null)} at ${onOutline} of the half-diagonals; diamond ${JSON.stringify(diamondBox)}`,
  )

  step('no page errors', errors.length === 0, errors.join(' | '))
} catch (error) {
  step('shapes ran to the end', false, `${error instanceof Error ? error.stack : String(error)}${errors.length ? `\npage errors: ${errors.join(' | ')}` : ''}${warnings.length ? `\nwarnings: ${warnings.slice(-5).join(' | ').slice(0, 3000)}` : ''}\nlifecycle: ${lifecycle.slice(-8).join(' | ')}`)
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${which}: ${failures} failed` : `${which}: all passed`)
process.exit(failures ? 1 : 0)
