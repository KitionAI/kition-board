// The board editor's flow in a real browser, through the development host: node tests/flow.mjs [chromium|webkit]
// Starts the Vite dev server itself, opens dev.html (Sample.kboard), and prints PASS or FAIL per step:
// the engine's fragments on the canvas, engine text widths against the browser's, draw, drag (and
// the flicker probe), connect (page-derived ends against the engine's), type, the input-method Enter
// in both browsers' event orders, undo, save and reopen, the 5,000-element pan budget and the 20-shape commit budget (spike S2).
// Run with the proxy variables unset: the dev server is on loopback.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
// The commit budget (spike S2-1: 16 ms, one frame at 60 Hz) is a product target measured on a
// maintainer's machine, where the spike's commits took 3–8 ms (p95). A slower machine multiplies it by
// KITION_BUDGET_SCALE (a number ≥ 1; default 1, and 2 on GitHub's hosted runners, where 4 shared
// vCPUs without a GPU measured a 22 ms median). The pan budgets are not scaled: frames there are
// paced by the display, not by the processor.
const budgetScale = Number(process.env.KITION_BUDGET_SCALE ?? (process.env.GITHUB_ACTIONS === 'true' ? 2 : 1))
if (!Number.isFinite(budgetScale) || budgetScale < 1) throw new Error(`KITION_BUDGET_SCALE must be a number ≥ 1, not ${process.env.KITION_BUDGET_SCALE}`)
const commitBudget = 16 * budgetScale

const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
// Headless Chromium on Linux hints glyphs and rounds every advance to a whole pixel, so its text is
// not the unhinted text the engine lays out (and macOS draws). Without hinting its advances are the
// font's own, as on macOS.
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
page.on('console', (message) => {
  if (message.text().includes('[vite]')) lifecycle.push(message.text().slice(0, 120))
})
const warnings = []
page.on('console', (message) => {
  if (message.type() === 'warning' || message.type() === 'error') warnings.push(message.text() + ' ' + JSON.stringify(message.args().length))
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
const copy = async (ids) => (await ai([{ command: 'clipboard.copy', params: { ids } }])).completed[0].result
const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)]
const p95 = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length * 0.95)]

/** The 5,000-element board of spike S2: 70% labelled flow nodes, 20% connectors bound to a node, 10% strokes, 100 columns. */
function bigPayload() {
  const elements = []
  let last = null
  for (let i = 0; i < 5000; i += 1) {
    const c = i % 100
    const r = Math.floor(i / 100)
    const x = c * 220
    const y = r * 140
    const k = i % 10
    if ((k === 7 || k === 8) && last) elements.push({ ref: `e${i}`, kind: 'connector', start: { point: [0, 0], binding: { target: last, anchor: [1, 0.5] } }, end: { point: [x + 160, y + 100] } })
    else if (k >= 7) elements.push({ ref: `e${i}`, kind: 'stroke', points: Array.from({ length: 12 }, (_, j) => [x + 10 + j * 13, y + 30 + (j % 2) * 40]) })
    else {
      elements.push({ ref: `e${i}`, kind: 'shape', shapeStyle: 'flow-node', position: { x: x + 30, y: y + 30 }, size: { w: 160, h: 80 }, text: i % 3 ? `Node ${i}` : `Review step ${i}` })
      last = `e${i}`
    }
  }
  return { type: 'application/x-kition-board+json', version: 2, elements }
}

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

  // 1. The engine's fragments on the canvas.
  const opened = await elementCount()
  const shown = await board(() => window.kitBoard.shown().length)
  step('draws one engine fragment per element', opened === 4 && shown === 4, `${shown} of ${opened}`)
  const labels = await board(() => [...document.querySelectorAll('[data-testid=board-elements] tspan')].map((t) => t.textContent))
  step('labels are engine-laid tspans', ['Idea', 'Ship', 'Plan', 'build'].every((label) => labels.includes(label)), JSON.stringify(labels))
  step('the connector is a world fragment with an arrowhead', await board(() => {
    const node = document.querySelector('[data-kind=connector]')
    return Boolean(node && !node.getAttribute('transform') && node.querySelectorAll('path').length >= 3)
  }))

  // 2. Engine text widths against the browser's (spike S2, R-B1): same faces, kerning off.
  const texts = [
    { text: 'Review step 42', size: 16, weight: 500 },
    { text: 'Quarterly planning: Q4 roadmap', size: 16, weight: 600 },
    { text: 'Hello, world!', size: 20, weight: 400 },
    { text: '中文标签 混排 Mixed', size: 16, weight: 500 },
    { text: 'WAVE Typography AVATAR', size: 24, weight: 400 },
  ]
  const widths = await board(async (texts) => {
    const engine = JSON.parse(await window.kitBoard.textMetrics(JSON.stringify({ texts })))
    const ns = 'http://www.w3.org/2000/svg'
    const svg = document.createElementNS(ns, 'svg')
    document.body.appendChild(svg)
    const out = texts.map((item, index) => {
      const text = document.createElementNS(ns, 'text')
      text.setAttribute('font-family', "'Inter', 'Droid Sans Fallback', sans-serif")
      text.setAttribute('font-size', String(item.size))
      text.setAttribute('font-weight', String(item.weight))
      text.setAttribute('style', 'font-kerning:none;font-variant-ligatures:none;font-optical-sizing:none;white-space:pre')
      text.textContent = item.text
      svg.appendChild(text)
      return { text: item.text, engine: engine.results[index].width, browser: text.getComputedTextLength() }
    })
    svg.remove()
    return out
  }, texts)
  const worst = Math.max(...widths.map((w) => Math.abs(w.engine - w.browser)))
  step('engine text widths match the browser (≤ 0.5 px)', worst <= 0.5, `worst ${worst.toFixed(3)} px: ${widths.map((w) => `${w.engine.toFixed(2)}/${w.browser.toFixed(2)}`).join(' ')}`)

  // 3. Draw a rectangle with the shape tool.
  await page.mouse.click((await svgBox()).x + 5, (await svgBox()).y + 5)
  await page.keyboard.press('r')
  // A free spot in the view, below the sample's frame.
  const v0 = await board(() => window.kitBoard.viewport())
  const ox = Math.round(v0.x + 120)
  const oy = Math.round(v0.y + v0.height / v0.zoom - 260)
  const a = await screen(ox, oy)
  const b = await screen(ox + 160, oy + 100)
  await drag(a, b)
  await until(async () => (await elementCount()) === 5 && (await board(() => window.kitBoard.selection())).length === 1)
  step('draws a shape: one element.create', (await elementCount()) === 5)
  const drawn = (await board(() => window.kitBoard.selection()))[0]
  const drawnBox = await board((id) => window.kitBoard.worldBox(id), drawn)
  step('the new shape has the dragged box', drawnBox && Math.abs(drawnBox.x0 - ox) < 3 && Math.abs(drawnBox.x1 - ox - 160) < 3, JSON.stringify(drawnBox))

  // 4. Drag it; the flicker probe samples every frame from release to the commit.
  const center = await screen(ox + 80, oy + 50)
  // Alt: no snapping, so the move is exactly the drag.
  await page.keyboard.down('Alt')
  await page.mouse.move(center.x, center.y)
  await page.mouse.down()
  await page.mouse.move(center.x + 100, center.y + 40, { steps: 10 })
  const before = await board((id) => window.kitBoard.worldBox(id), drawn)
  await board((id) => {
    window.__flicker = []
    const sample = () => {
      const node = document.querySelector(`[data-id="${id}"]`)
      const box = node?.getBoundingClientRect()
      window.__flicker.push(box ? box.left : null)
      if (window.__flicker.length < 30) requestAnimationFrame(sample)
    }
    requestAnimationFrame(sample)
  }, drawn)
  await page.mouse.up()
  await page.keyboard.up('Alt')
  await until(async () => {
    const box = await board((id) => window.kitBoard.worldBox(id), drawn)
    return box && box.x0 > before.x0 + 90
  })
  const after = await board((id) => window.kitBoard.worldBox(id), drawn)
  step('drags a shape: one element.move', after && Math.abs(after.x0 - before.x0 - 100) < 4 && Math.abs(after.y0 - before.y0 - 40) < 4, `${JSON.stringify(before)} → ${JSON.stringify(after)}`)
  await page.waitForTimeout(600)
  const samples = await board(() => window.__flicker)
  const left0 = samples[0]
  const jumpedBack = samples.some((left) => left != null && left0 != null && left < left0 - 50)
  step('no flicker on commit (never back at the drag start)', !jumpedBack && samples.every((left) => left != null), JSON.stringify(samples.slice(0, 8)))

  // 5. Connect the new shape to "Ship" with the connector tool.
  const ship = await board(() => {
    const node = [...document.querySelectorAll('[data-kind=shape]')].find((n) => n.textContent.includes('Ship'))
    return node?.getAttribute('data-id')
  })
  const shipBox = await board((id) => window.kitBoard.worldBox(id), ship)
  await page.keyboard.press('a')
  const from = await screen((after.x0 + after.x1) / 2, (after.y0 + after.y1) / 2)
  const to = await screen((shipBox.x0 + shipBox.x1) / 2, (shipBox.y0 + shipBox.y1) / 2)
  await drag(from, to)
  // The engine has it first; the page selects it once the change arrived.
  await until(async () => (await elementCount()) === 6 && (await board(() => window.kitBoard.selection()))[0] !== drawn)
  const connector = (await board(() => window.kitBoard.selection()))[0]
  const payload = await copy([connector])
  const stored = payload.payload?.elements?.find((e) => e.kind === 'connector') ?? payload.elements?.find((e) => e.kind === 'connector')
  step('connects two shapes: one connector.create, bound at both ends', stored?.start?.binding?.target === drawn && stored?.end?.binding?.target === ship, `${connector} ${JSON.stringify(payload).slice(0, 400)} ${warnings.slice(-3).join(' | ')}`)
  const derived = await board((id) => window.kitBoard.endPoints(id), connector)
  const engineEnds = [stored?.start?.point, stored?.end?.point]
  const off = Math.max(Math.hypot(derived[0].x - engineEnds[0][0], derived[0].y - engineEnds[0][1]), Math.hypot(derived.at(-1).x - engineEnds[1][0], derived.at(-1).y - engineEnds[1][1]))
  step("the page's derived ends match the engine's", off < 0.01, `${off}`)

  // Moving the shape re-derives the connector in the same command.
  await board(() => window.kitBoard.resetMounts())
  await page.keyboard.press('Escape')
  const grab = await screen(after.x0 + 10, after.y0 + 10)
  await drag(grab, { x: grab.x, y: grab.y + 80 })
  await until(async () => {
    const ends = await board((id) => window.kitBoard.endPoints(id), connector)
    return ends[0] && Math.abs(ends[0].y - derived[0].y - 80) < 3
  })
  const moved = (await copy([connector])).payload?.elements?.find((e) => e.kind === 'connector') ?? (await copy([connector])).elements?.find((e) => e.kind === 'connector')
  const movedEnds = await board((id) => window.kitBoard.endPoints(id), connector)
  step('a bound end follows its moved target', Math.abs(movedEnds[0].y - moved.start.point[1]) < 0.01 && Math.abs(movedEnds[0].y - derived[0].y - 80) < 3, JSON.stringify(movedEnds[0]))

  // 6. Type into the shape.
  const label = await screen((after.x0 + after.x1) / 2, (after.y0 + after.y1) / 2 + 80)
  await page.mouse.dblclick(label.x, label.y)
  await until(async () => (await kit.getByTestId('board-text-editor').count()) === 1)
  await kit.getByTestId('board-text-editor').fill('')
  await page.keyboard.type('Hello board')
  await page.keyboard.press('Enter')
  await until(async () => (await board((id) => document.querySelector(`[data-id="${id}"]`)?.textContent ?? '', drawn)).includes('Hello board'))
  step('types a label: one element.setText', (await board((id) => document.querySelector(`[data-id="${id}"]`)?.textContent ?? '', drawn)).includes('Hello board'))

  // 7. Input methods: Enter during a composition does not commit, in either browser's order.
  await page.mouse.dblclick(label.x, label.y)
  await until(async () => (await kit.getByTestId('board-text-editor').count()) === 1)
  const ime = await kit.getByTestId('board-text-editor').evaluate((field) => {
    const key = (init) => field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true, ...init }))
    const composition = (type) => field.dispatchEvent(new CompositionEvent(type, { bubbles: true, data: 'ni' }))
    // Chromium: the Enter comes during the composition.
    composition('compositionstart')
    key({ isComposing: true, keyCode: 229 })
    composition('compositionend')
    const afterChromium = document.contains(field)
    // WebKit (the desktop app): the composition ends first, then Enter with keyCode 229.
    composition('compositionstart')
    composition('compositionend')
    key({ isComposing: false, keyCode: 229 })
    const afterWebKit = document.contains(field)
    return { afterChromium, afterWebKit }
  })
  await page.waitForTimeout(100)
  ime.ignored = Number(await kit.getByTestId('board-text-editor').getAttribute('data-ime-ignored'))
  step('an input-method Enter does not commit (Chromium order)', ime.afterChromium, JSON.stringify(ime))
  step('an input-method Enter does not commit (WebKit order)', ime.afterWebKit && ime.ignored === 2, JSON.stringify(ime))
  await page.keyboard.press('Enter')
  await until(async () => (await kit.getByTestId('board-text-editor').count()) === 0)
  step('a plain Enter commits', (await kit.getByTestId('board-text-editor').count()) === 0)

  // 8. Undo the label, then save and reopen.
  await page.keyboard.press(`${mod}+z`)
  await until(async () => !(await board((id) => document.querySelector(`[data-id="${id}"]`)?.textContent ?? '', drawn)).includes('Hello board'))
  step('undo takes back one gesture', !(await board((id) => document.querySelector(`[data-id="${id}"]`)?.textContent ?? '', drawn)).includes('Hello board'))
  await page.keyboard.press(`${mod}+s`)
  await until(async () => (await page.evaluate(() => window.devHost.files.get('Sample.kboard')?.version ?? 0)) > 1)
  step('saves through the host', (await page.evaluate(() => window.devHost.files.get('Sample.kboard')?.version ?? 0)) > 1)
  await page.evaluate(async () => window.devHost.open('Sample.kboard', window.devHost.files.get('Sample.kboard').bytes))
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await until(async () => (await frame.evaluate(() => window.kitBoard?.shown().length ?? 0)) === 6)
  step('the saved board reopens with every element', (await elementCount()) === 6 && (await board(() => window.kitBoard.shown().length)) === 6)

  // An agent's step runs in an editor state of its own and never moves the person's page (Kit UI
  // protocol 2.9): `page.create { activate }` makes the page and puts the run's next steps there.
  const personPage = async () => (await inspect()).page
  const pageBefore = await personPage()
  const agentRun = await ai([
    { command: 'page.create', params: { name: 'Agent', activate: true } },
    { command: 'element.create', params: { elements: [{ kind: 'shape', position: { x: 0, y: 0 }, size: { w: 40, h: 40 } }] } },
  ])
  const agentPage = agentRun.completed[0].result.page
  const agentMade = agentRun.completed[1].result.created[0]
  const agentPageElements = async () => (await inspect()).pages.find((listed) => listed.id === agentPage)?.elements
  await page.waitForTimeout(300)
  step(
    "an agent's page.create { activate } leaves the person's page, and its next step goes to the new page",
    (await personPage()) === pageBefore && (await board(() => window.kitBoard.shown().length)) === 6 && !(await board(() => window.kitBoard.shown())).includes(agentMade) && (await agentPageElements()) === 1,
    `${await personPage()} vs ${pageBefore}, ${await board(() => window.kitBoard.shown().length)} shown, ${await agentPageElements()} on the agent's page`,
  )
  // The editor's own UI, on a page of its own at a known view (100%, world origin at the canvas corner):
  // the person's New page (View) shows the page it makes.
  /** The person's New page, from the View tab. */
  const newPage = async () => {
    await kit.getByTestId('ribbon-tab-View').click()
    await kit.getByTestId('board-cmd-page.create').click()
    await kit.getByTestId('ribbon-tab-Edit').click()
  }
  await newPage()
  await until(async () => (await personPage()) !== pageBefore && (await board(() => window.kitBoard.shown().length)) === 0)
  step('New page shows the page it makes', ![pageBefore, agentPage].includes(await personPage()) && (await board(() => window.kitBoard.shown().length)) === 0, `${await personPage()}, ${await board(() => window.kitBoard.shown().length)} shown`)
  await board(() => window.kitBoard.setViewport({ x: 0, y: 0, zoom: 1 }))
  /** Copies `ids` and answers their stored elements. */
  const storedOf = async (ids) => {
    const payload = await copy(ids)
    return payload.payload?.elements ?? payload.elements ?? []
  }
  const selected = () => board(() => window.kitBoard.selection())
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

  // 9. The tool rail: every tool, the 18 shapes in one slot's flyout, the highlighter, the eraser, a picture.
  const rail = ['select', 'hand', 'shape:rectangle', 'note', 'text', 'connector', 'pen', 'frame', 'image']
  const railShown = await Promise.all(rail.map((id) => kit.locator(`.kui-tool-rail [data-testid="board-tool-${id}"]`).count()))
  step('the rail shows every tool slot', railShown.every((n) => n === 1), JSON.stringify(Object.fromEntries(rail.map((id, at) => [id, railShown[at]]))))
  await kit.locator('[data-testid="board-tool-shape:rectangle"]').first().click({ button: 'right' })
  const flyout = kit.locator('.kui-tool-flyout')
  await until(async () => (await flyout.count()) === 1)
  const flyoutShapes = await flyout.locator('[role=menuitemradio]').count()
  step('the shape slot holds the 18 shapes', flyoutShapes === 18, `${flyoutShapes}`)
  await flyout.locator('[data-testid="board-tool-shape:star"]').click()
  step('a shape from the flyout becomes the tool', await until(async () => (await kit.locator('[data-testid="board-tool-shape:star"][aria-pressed=true]').count()) === 1))
  let count = await mark()
  const toolBefore = await kit.getByTestId('board-canvas').getAttribute('data-tool')
  await drag(await screen(80, 80), await screen(200, 180))
  const star = await newest(count + 1)
  if (!star) console.log('DEBUG', toolBefore, await elementCount(), count, JSON.stringify(await board(() => window.kitBoard.viewport())), warnings.slice(-4).join(' | '))
  step('draws the chosen shape', (await storedOf([star])).find((e) => e.kind === 'shape')?.shapeType === 'star', JSON.stringify((await storedOf([star]))[0] ?? null))

  await page.keyboard.press('l')
  count = await mark()
  await drag(await screen(300, 120), await screen(520, 130), 16)
  const highlight = await newest(count + 1)
  const highlighted = (await storedOf([highlight]))[0]
  step('the highlighter draws a translucent yellow stroke', highlighted?.kind === 'stroke' && highlighted.strokeColor === 'yellow' && Math.abs(highlighted.opacity - 0.45) < 1e-6, `${JSON.stringify(highlighted).slice(0, 200)} ${JSON.stringify(await board(() => window.kitBoard.trace()))}`)

  await page.keyboard.press('e')
  step('E picks the eraser', (await kit.locator('[data-testid="board-canvas"][data-tool=eraser]').count()) === 1)
  count = await mark()
  await drag(await screen(410, 60), await screen(410, 200), 24)
  await until(async () => (await elementCount()) === count - 1)
  step('the eraser deletes what it passes, in one command', (await elementCount()) === count - 1 && !(await board(() => window.kitBoard.shown())).includes(highlight))
  await page.keyboard.press(`${mod}+z`)
  await until(async () => (await elementCount()) === count)
  step('one undo brings the erased stroke back', (await elementCount()) === count && (await board(() => window.kitBoard.shown())).includes(highlight))

  // Focus (kit-editor's EditorFrame): the rail goes, the Focus bar names the tool, letters still pick tools, Escape leaves.
  // The page pins Classic (`?modes=classic`); let the person enter Focus too.
  await page.evaluate(() => window.devHost.configure({ modes: { allowed: ['classic', 'focus'], default: 'classic' } }))
  await kit.getByTestId('board-canvas').focus()
  await page.keyboard.press(`${mod}+Backslash`)
  await until(async () => (await kit.getByTestId('board-focus-bar').count()) === 1)
  const focusRail = await kit.locator('.kui-tool-rail').count()
  const focusName = await kit.getByTestId('board-focus-tool').textContent()
  step('Focus hides the rail and names the active tool', focusRail === 0 && focusName.includes('Eraser'), `rail ${focusRail}, tool ${focusName}`)
  await page.keyboard.press('v')
  step('a tool letter in Focus changes the named tool', await until(async () => (await kit.getByTestId('board-focus-tool').textContent()).includes('Select')))
  // Escape cancels in the kit first (maintainer decision D1, KitPointer 4.4): it clears the selection
  // and Focus stays; the next Escape, with nothing left to cancel, leaves Focus.
  const focusSelected = (await board(() => window.kitBoard.selection())).length
  await page.keyboard.press('Escape')
  await until(async () => (await board(() => window.kitBoard.selection())).length === 0)
  step('in Focus, Escape first clears the selection and Focus stays', focusSelected > 0 && (await board(() => window.kitBoard.selection())).length === 0 && (await kit.getByTestId('board-focus-bar').count()) === 1, `selected before ${focusSelected}`)
  await page.keyboard.press('Escape')
  await until(async () => (await kit.getByTestId('board-focus-bar').count()) === 0)
  step('Escape leaves Focus and the rail comes back', (await kit.getByTestId('board-focus-bar').count()) === 0 && (await kit.locator('.kui-tool-rail').count()) === 1)

  // 10. The style panel: the selection's style, one element.style per change; a drawing tool's defaults.
  await page.keyboard.press('v')
  const starAt = await screen(140, 135)
  await page.mouse.click(starAt.x, starAt.y)
  await until(async () => (await selected())[0] === star)
  await until(async () => (await attr('board-style-panel', 'data-target')) === 'selection')
  step('the style panel styles the selection', (await attr('board-style-panel', 'data-target')) === 'selection' && (await attr('board-style-fillColor-white', 'aria-pressed')) === 'true')
  await kit.getByTestId('board-style-fillColor-purple').click()
  await until(async () => (await storedOf([star]))[0]?.fillColor === 'purple')
  const purpleDrawn = await board((id) => document.querySelector(`[data-id="${id}"]`)?.innerHTML.includes('--kb-fill-purple') ?? false, star)
  step('a fill colour is one element.style, drawn by the engine', (await storedOf([star]))[0]?.fillColor === 'purple' && purpleDrawn)
  await page.keyboard.press(`${mod}+z`)
  await until(async () => (await storedOf([star]))[0]?.fillColor == null)
  await until(async () => (await attr('board-style-fillColor-white', 'aria-pressed')) === 'true')
  step('one undo takes the colour back', (await storedOf([star]))[0]?.fillColor == null && (await attr('board-style-fillColor-white', 'aria-pressed')) === 'true')
  await kit.getByTestId('board-style-strokeSize-l').click()
  await kit.getByTestId('board-style-dashStyle-dotted').click()
  await kit.getByTestId('board-style-fillStyle-pattern').click()
  await kit.locator('[data-testid="board-style-opacity"] input').fill('50')
  await kit.locator('[data-testid="board-style-opacity"] input').press('Enter')
  await kit.locator('[data-testid="board-style-fontSize"] input').fill('24')
  await kit.locator('[data-testid="board-style-fontSize"] input').press('Enter')
  await kit.getByTestId('board-style-textAlign-left').click()
  await until(async () => (await storedOf([star]))[0]?.textAlign === 'left')
  const styled = (await storedOf([star]))[0]
  step('weight, dashes, fill, opacity, text size and alignment', styled.strokeSize === 'l' && styled.dashStyle === 'dotted' && styled.fillStyle === 'pattern' && Math.abs(styled.opacity - 0.5) < 1e-9 && styled.fontSize === 24 && styled.textAlign === 'left', JSON.stringify(styled))

  // A connector's route and arrowheads.
  await page.keyboard.press('a')
  count = await mark()
  await drag(await screen(300, 300), await screen(500, 360))
  const arrow = await newest(count + 1)
  await until(async () => (await selected())[0] === arrow)
  await until(async () => (await kit.getByTestId('board-style-endArrowhead').count()) === 1)
  step('a selected connector shows its route and arrowheads', (await kit.getByTestId('board-style-fillColor-white').count()) === 0 && (await kit.getByTestId('board-style-endArrowhead').inputValue()) === 'arrow')
  await kit.getByTestId('board-style-endArrowhead').selectOption('dot')
  await kit.getByTestId('board-style-startArrowhead').selectOption('arrow')
  await kit.getByTestId('board-style-connectorType-curved').click()
  await until(async () => (await storedOf([arrow]))[0]?.connectorType === 'curved')
  const routed = (await storedOf([arrow]))[0]
  step('arrowheads and route are set on the connector', routed.endArrowhead === 'dot' && routed.startArrowhead === 'arrow' && routed.connectorType === 'curved', JSON.stringify(routed).slice(0, 300))

  // A drawing tool's defaults: the pen draws in the colour chosen while it is active.
  await kit.getByTestId('board-canvas').focus()
  await page.keyboard.press('Escape')
  await page.keyboard.press('p')
  await until(async () => (await attr('board-style-panel', 'data-target')) === 'defaults')
  step('a drawing tool shows its defaults', (await attr('board-style-panel', 'data-target')) === 'defaults', `${await attr('board-style-panel', 'data-target')} ${await attr('board-canvas', 'data-tool')}`)
  await kit.getByTestId('board-style-strokeColor-red').click()
  await until(async () => (await attr('board-style-strokeColor-red', 'aria-pressed')) === 'true')
  count = await mark()
  await drag(await screen(600, 300), await screen(760, 380), 10)
  const inked = await newest(count + 1)
  step("a drawing tool's defaults go to what it draws", (await storedOf([inked]))[0]?.strokeColor === 'red', `${JSON.stringify((await storedOf([inked]))[0] ?? null).slice(0, 200)} ${await attr('board-canvas', 'data-tool')} ${warnings.slice(-3).join(' | ')} ${JSON.stringify(await board(() => window.kitBoard.trace()))}`)
  await page.keyboard.press('v')

  // 11. Resize handles: a selection of several scales by its bounds, a rotated box in its own frame.
  const rectangle = async (x0, y0, x1, y1) => {
    await page.keyboard.press('r')
    const before = await mark()
    await drag(await screen(x0, y0), await screen(x1, y1))
    const made = await newest(before + 1)
    await page.keyboard.press('Escape')
    return made
  }
  await kit.locator('.kui-tool-rail [data-testid^="board-tool-shape:"]').first().click({ button: 'right' })
  await kit.locator('.kui-tool-flyout [data-testid="board-tool-shape:rectangle"]').click()
  await kit.getByTestId('board-canvas').focus()
  await page.keyboard.press('v')
  const left = await rectangle(100, 450, 200, 520)
  const right = await rectangle(260, 450, 360, 520)
  await page.keyboard.press('v')
  await drag(await screen(80, 430), await screen(380, 545))
  await until(async () => (await selected()).length === 2)
  step('a marquee selects both boxes and shows the handles', (await selected()).length === 2 && (await kit.getByTestId('board-handle-south-east').count()) === 1, JSON.stringify(await selected()))
  await drag(await screen(360, 520), await screen(490, 590), 10)
  await until(async () => (await storedOf([right]))[0]?.size?.h > 100)
  const [l1, r1] = [(await storedOf([left]))[0], (await storedOf([right]))[0]]
  const sx = 390 / 260
  const near = (a, b) => Math.abs(a - b) < 1.5
  step(
    'dragging a handle of the selection scales both in one element.resize',
    near(l1.size.w, 100 * sx) && near(l1.size.h, 140) && near(r1.position.x, 100 + (310 - 100) * sx - (100 * sx) / 2) && near(r1.size.h, 140),
    `${JSON.stringify(l1.position)} ${JSON.stringify(l1.size)} ${JSON.stringify(r1.position)} ${JSON.stringify(r1.size)}`,
  )
  await page.keyboard.press(`${mod}+z`)
  await until(async () => near((await storedOf([right]))[0]?.size?.h ?? 0, 70))
  step('one undo takes the whole scale back', near((await storedOf([left]))[0].size.h, 70) && near((await storedOf([right]))[0].size.h, 70))

  await ai([{ command: 'element.rotate', params: { ids: [right], degrees: 90 } }])
  await page.keyboard.press('Escape')
  const rightCenter = await screen(310, 485)
  await page.mouse.click(rightCenter.x, rightCenter.y)
  await until(async () => (await selected()).length === 1 && (await selected())[0] === right)
  // Rotated 90°, the box's east handle sits under its centre: (310, 485 + 50).
  const eastAt = await screen(310, 535)
  const eastHandle = () => kit.getByTestId('board-handle-east').boundingBox()
  await until(async () => {
    const box = await eastHandle()
    return box && near(box.x + box.width / 2, eastAt.x) && near(box.y + box.height / 2, eastAt.y)
  })
  const east = await eastHandle()
  step('a rotated box shows its handles on its own edges', east && near(east.x + east.width / 2, eastAt.x) && near(east.y + east.height / 2, eastAt.y), `${JSON.stringify(east)} vs ${JSON.stringify(eastAt)}`)
  await drag(eastAt, await screen(310, 575), 8)
  await until(async () => near((await storedOf([right]))[0]?.size?.w ?? 0, 140))
  const r2 = (await storedOf([right]))[0]
  // The west handle stays on the board where it was: (310, 435).
  const c2 = { x: r2.position.x + r2.size.w / 2, y: r2.position.y + r2.size.h / 2 }
  const west = { x: c2.x, y: c2.y - r2.size.w / 2 }
  step('a rotated box resizes in its own frame, the opposite handle kept', near(r2.size.w, 140) && near(r2.size.h, 70) && r2.rotation === 90 && near(west.x, 310) && near(west.y, 435), `${JSON.stringify(r2.position)} ${JSON.stringify(r2.size)} west ${JSON.stringify(west)}`)
  await page.keyboard.press('Escape')

  // 12. Frames: a shape dropped into a frame becomes its child (one element.move), dropped out it is free again.
  await page.keyboard.press('f')
  count = await mark()
  await drag(await screen(620, 420), await screen(960, 640))
  const frameId = await newest(count + 1)
  await page.keyboard.press('Escape')
  const parentOf = (id) => board((id) => window.kitBoard.parent(id), id)
  const leftAt = await screen(150, 485)
  await page.keyboard.down('Alt')
  await page.mouse.move(leftAt.x, leftAt.y)
  await page.mouse.down()
  const intoFrame = await screen(780, 530)
  await page.mouse.move(intoFrame.x, intoFrame.y, { steps: 10 })
  const dropTargets = await board(() => [...document.querySelectorAll('[data-testid=board-drop-target]')].map((node) => node.getAttribute('data-frame')))
  await page.mouse.up()
  await page.keyboard.up('Alt')
  step('dragging over a frame shows it as the drop target', dropTargets.length === 1 && dropTargets[0] === frameId, JSON.stringify(dropTargets))
  await until(async () => (await parentOf(left)) === frameId)
  step('a shape dropped into a frame becomes its child', (await parentOf(left)) === frameId)
  await page.keyboard.press(`${mod}+z`)
  await until(async () => (await parentOf(left)) == null)
  const backAt = (await storedOf([left]))[0].position
  step('one undo takes back the move and the parent', (await parentOf(left)) == null && near(backAt.x, 100) && near(backAt.y, 450), JSON.stringify(backAt))
  await page.keyboard.press(`${mod}+Shift+z`)
  await until(async () => (await parentOf(left)) === frameId)
  // Moving the frame takes its child along.
  await ai([{ command: 'element.move', params: { ids: [frameId], delta: [20, 0] } }])
  await until(async () => near((await storedOf([left]))[0].position.x, 100 + 630 + 20))
  step('moving the frame moves its child', near((await storedOf([left]))[0].position.x, 750), JSON.stringify((await storedOf([left]))[0].position))
  await page.keyboard.press('Escape')
  await page.keyboard.down('Alt')
  await drag(await screen(800, 530), await screen(170, 485))
  await page.keyboard.up('Alt')
  await until(async () => (await parentOf(left)) == null)
  step('dropped outside every frame, the shape is free again', (await parentOf(left)) == null)
  await page.keyboard.press('r')
  count = await mark()
  await drag(await screen(820, 560), await screen(900, 610))
  const drawnInFrame = await newest(count + 1)
  step('a shape drawn inside a frame is its child', (await parentOf(drawnInFrame)) === frameId)
  await page.keyboard.press('Escape')

  // 13. Mind maps: a root, branches by Tab, the quick control and Enter, collapse, direction, one undo each.
  const typeLabel = async (text) => {
    // The box takes focus once the press that opened it is over (TextEditor); keys sent before
    // that would go to the canvas or the button instead.
    await until(async () => kit.getByTestId('board-text-editor').evaluate((element) => element.ownerDocument.activeElement === element))
    await page.keyboard.type(text)
    await page.keyboard.press('Enter')
    await until(async () => (await kit.getByTestId('board-text-editor').count()) === 0)
  }
  const boxOf = (id) => board((id) => window.kitBoard.worldBox(id), id)
  const clickOn = async (id) => {
    const box = await boxOf(id)
    const at = await screen((box.x0 + box.x1) / 2, (box.y0 + box.y1) / 2)
    await page.mouse.click(at.x, at.y)
    await until(async () => (await selected())[0] === id)
  }
  const labelOf = async (id, text) => {
    const read = () => board((id) => document.querySelector(`[data-id="${id}"]`)?.textContent ?? '', id)
    if (text) await until(async () => (await read()).includes(text), 30)
    return read()
  }
  await page.keyboard.press('m')
  count = await mark()
  const rootAt = await screen(640, 100)
  await page.mouse.click(rootAt.x, rootAt.y)
  const mapRoot = await newest(count + 1)
  await typeLabel('Plan')
  step('the mind-map tool makes a root to type into', (await labelOf(mapRoot, 'Plan')).includes('Plan'))
  count = await mark()
  await page.keyboard.press('Tab')
  const goals = await newest(count + 2)
  await typeLabel('Goals')
  const [rootBox, goalsBox] = [await boxOf(mapRoot), await boxOf(goals)]
  step('Tab grows a branch to the right, selected and typed into', (await labelOf(goals, 'Goals')).includes('Goals') && goalsBox.x0 > rootBox.x1 + 50, `${JSON.stringify(rootBox)} ${JSON.stringify(goalsBox)}`)
  await clickOn(mapRoot)
  await until(async () => (await kit.getByTestId('board-mind-add-right').count()) === 1)
  count = await mark()
  const plus = await kit.getByTestId('board-mind-add-right').boundingBox()
  await page.mouse.click(plus.x + plus.width / 2, plus.y + plus.height / 2)
  const risks = await newest(count + 2)
  await typeLabel('Risks')
  const [g2, r2box] = [await boxOf(goals), await boxOf(risks)]
  step("the node's + control adds a branch and the map lays itself out", (await labelOf(risks, 'Risks')).includes('Risks') && near(g2.x0, r2box.x0) && r2box.y0 > g2.y1, `${JSON.stringify(g2)} ${JSON.stringify(r2box)}`)
  await clickOn(goals)
  count = await mark()
  await page.keyboard.press('Enter')
  const team = await newest(count + 2)
  await typeLabel('Team')
  step('Enter on a branch adds a sibling', (await labelOf(team, 'Team')).includes('Team') && (await elementCount()) === count + 2, `${team} "${await labelOf(team)}" ${await elementCount()} vs ${count + 2} ${JSON.stringify(await board(() => window.kitBoard.trace()))}`)
  await page.keyboard.press(`${mod}+z`)
  await page.keyboard.press(`${mod}+z`)
  await until(async () => (await elementCount()) === count)
  step('undo takes the sibling (its label, then the node and branch) back', (await elementCount()) === count)

  await clickOn(goals)
  count = await mark()
  await page.keyboard.press('Tab')
  const q4 = await newest(count + 2)
  await typeLabel('Q4')
  await clickOn(goals)
  await until(async () => (await kit.getByTestId('board-mind-collapse-right').count()) === 1)
  const fold = await kit.getByTestId('board-mind-collapse-right').boundingBox()
  await page.mouse.click(fold.x + fold.width / 2, fold.y + fold.height / 2)
  await until(async () => !(await board(() => window.kitBoard.shown())).includes(q4))
  const expandCount = await kit.getByTestId('board-mind-expand-right').textContent().catch(() => '')
  step('collapsing hides the branch below, and the control counts it', !(await board(() => window.kitBoard.shown())).includes(q4) && expandCount === '1', `count "${expandCount}" shown ${(await board(() => window.kitBoard.shown())).includes(q4)} sel ${JSON.stringify(await selected())} goals ${goals} trace ${JSON.stringify(await board(() => window.kitBoard.trace()))} fold ${JSON.stringify(fold)}`)
  await kit.getByTestId('board-mind-panel-collapse').click()
  await until(async () => (await board(() => window.kitBoard.shown())).includes(q4))
  step('the panel expands it again', (await board(() => window.kitBoard.shown())).includes(q4))
  await clickOn(mapRoot)
  await kit.getByTestId('board-mind-direction-down').click()
  await until(async () => (await boxOf(goals)).y0 > (await boxOf(mapRoot)).y1)
  step('the panel turns the map to grow down', (await boxOf(goals)).y0 > (await boxOf(mapRoot)).y1 && (await boxOf(risks)).y0 > (await boxOf(mapRoot)).y1)
  await page.keyboard.press(`${mod}+z`)
  await until(async () => (await boxOf(goals)).x0 > (await boxOf(mapRoot)).x1)
  step('one undo turns it back', (await boxOf(goals)).x0 > (await boxOf(mapRoot)).x1)
  await page.keyboard.press('Escape')

  // A picture from the workspace (the development host's picker answers the first one).
  const png = 'iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEUlEQVR4nGMIc73yH4QZYAwAUnYJuZK5ts8AAAAASUVORK5CYII='
  await page.evaluate((png) => window.devHost.files.set('Picture.png', { bytes: Uint8Array.from(atob(png), (c) => c.charCodeAt(0)).buffer, version: 1 }), png)
  count = await mark()
  await kit.locator('[data-testid="board-tool-image"]').click()
  const picture = await newest(count + 1)
  step('the picture tool inserts a workspace picture', (await storedOf([picture]))[0]?.kind === 'image', JSON.stringify((await storedOf([picture]))[0] ?? null).slice(0, 200))

  // 14. Cards: a workspace file the host lets the person pick, its new target and caption, and a link card.
  await page.evaluate(() => {
    window.devHost.files.delete('Picture.png')
    window.devHost.files.set('Notes/Plan.md', { bytes: new TextEncoder().encode('# Plan\n\nShip it.').buffer, version: 1 })
  })
  await page.keyboard.press('Escape')
  count = await mark()
  await kit.locator('.kui-tool-rail [data-testid="board-tool-card"]').click()
  const noteCard = await newest(count + 1)
  const placed = (await storedOf([noteCard]))[0]
  step('the card tool places a card for a picked file', placed?.kind === 'card' && placed.card?.kind === 'note' && placed.card?.target?.path === 'Notes/Plan.md', JSON.stringify(placed).slice(0, 300))
  await until(async () => (await kit.getByTestId('board-card-target').count()) === 1)
  step('the panel shows the selected card\'s target', (await kit.getByTestId('board-card-target').textContent()).includes('Notes/Plan.md'))
  await page.evaluate(() => {
    window.devHost.files.delete('Notes/Plan.md')
    window.devHost.files.set('Notes/Other.md', { bytes: new TextEncoder().encode('# Other').buffer, version: 1 })
  })
  await kit.getByTestId('board-card-retarget').click()
  await until(async () => (await storedOf([noteCard]))[0]?.card?.target?.path === 'Notes/Other.md')
  step('Change file points the card at another file', (await storedOf([noteCard]))[0]?.card?.target?.path === 'Notes/Other.md')
  await kit.getByTestId('board-card-caption').click()
  await typeLabel('Read me first')
  step('the caption is typed under the card', (await labelOf(noteCard, 'Read me first')).includes('Read me first'))
  await kit.locator('.kui-tool-rail [data-testid="board-tool-card"]').click({ button: 'right' })
  await kit.locator('.kui-tool-flyout [data-testid="board-tool-link"]').click()
  await until(async () => (await kit.getByTestId('board-link-dialog').count()) === 1)
  await kit.getByTestId('board-link-url').fill('https://kition.ai/docs')
  await kit.getByTestId('board-link-title').fill('Docs')
  count = await mark()
  await kit.getByTestId('board-link-done').click()
  const linkCard = await newest(count + 1)
  const linked = (await storedOf([linkCard]))[0]
  step('a link card holds its address and title', linked?.card?.kind === 'link' && linked.card.url === 'https://kition.ai/docs' && linked.card.title === 'Docs' && (await labelOf(linkCard, 'Docs')).includes('Docs'), JSON.stringify(linked).slice(0, 300))
  await page.keyboard.press('Escape')

  // 15. Export: the selected frame as PNG, the page as SVG and PDF, written next to the board through the host.
  const written = (name) => page.evaluate((name) => {
    const file = window.devHost.files.get(name)
    if (!file) return null
    const bytes = new Uint8Array(file.bytes)
    const view = new DataView(file.bytes)
    return { head: String.fromCharCode(...bytes.slice(0, 5)), size: bytes.length, width: bytes.length > 24 ? view.getUint32(16) : 0, height: bytes.length > 24 ? view.getUint32(20) : 0 }
  }, name)
  const inFrame = await screen(950, 470)
  await page.mouse.click(inFrame.x, inFrame.y)
  await until(async () => (await selected())[0] === frameId)
  step("a click on a frame's empty inside selects the frame", (await selected()).length === 1 && (await selected())[0] === frameId)
  await kit.getByTestId('board-export').click()
  await until(async () => (await kit.getByTestId('board-export-dialog').count()) === 1)
  step('a selected frame is offered as what to export', (await kit.getByTestId('board-export-scope').inputValue()) === 'frame')
  await kit.getByTestId('board-export-scale').selectOption('1')
  await kit.getByTestId('board-export-run').click()
  await until(async () => (await written('Sample - Frame.png')) != null)
  const framePng = await written('Sample - Frame.png')
  step('the frame exports as a PNG of its box', framePng?.head.startsWith('\x89PNG') && framePng.width === 340 && framePng.height === 220, JSON.stringify(framePng))
  await kit.getByTestId('board-export').click()
  await until(async () => (await kit.getByTestId('board-export-dialog').count()) === 1)
  await kit.getByTestId('board-export-scope').selectOption('page')
  await kit.getByTestId('board-export-format').selectOption('svg')
  await kit.getByTestId('board-export-run').click()
  await until(async () => (await written('Sample.svg')) != null)
  step('the page exports as SVG', (await written('Sample.svg'))?.head === '<svg ', JSON.stringify(await written('Sample.svg')))
  await kit.getByTestId('board-export').click()
  await until(async () => (await kit.getByTestId('board-export-dialog').count()) === 1)
  await kit.getByTestId('board-export-scope').selectOption('page')
  await kit.getByTestId('board-export-format').selectOption('pdf')
  await kit.getByTestId('board-export-run').click()
  const pdfStarted = Date.now()
  const failedExport = () => board(() => [...document.querySelectorAll('.kui-toast[data-level=error]')].map((node) => node.textContent).join(' | '))
  await until(async () => (await written('Sample.pdf')) != null || (await failedExport()) !== '', 300)
  step('the page exports as PDF', (await written('Sample.pdf'))?.head === '%PDF-', `${JSON.stringify(await written('Sample.pdf'))} ${await failedExport()} after ${Date.now() - pdfStarted} ms; files ${await page.evaluate(() => [...window.devHost.files.keys()].join(','))}`)
  await page.keyboard.press('Escape')

  // 16. Templates: the kanban board in the middle of the view, its outer frame selected, one undo step.
  count = await elementCount()
  await kit.getByTestId('board-templates-open').click()
  await until(async () => (await kit.getByTestId('board-templates').count()) === 1)
  await kit.getByTestId('board-template-kanban').click()
  await until(async () => (await elementCount()) === count + 14)
  await until(async () => (await selected()).length === 1)
  const kanbanFrame = (await selected())[0]
  step('a template is placed whole, its outer frame selected', (await elementCount()) === count + 14 && (await board((id) => document.querySelector(`[data-id="${id}"]`)?.getAttribute('data-kind'), kanbanFrame)) === 'frame', `${(await elementCount()) - count} elements, selected ${JSON.stringify(await selected())}`)
  await page.keyboard.press(`${mod}+z`)
  await until(async () => (await elementCount()) === count)
  step('one undo takes the template away', (await elementCount()) === count)

  // 10. The pan budget: 5,000 elements on a new page, panned for 120 frames (spike S2, S2-2).
  const before5000 = await elementCount()
  // The person's New page; the agent's paste goes to the page the person shows.
  const toolsPage = await personPage()
  await newPage()
  await until(async () => (await personPage()) !== toolsPage && (await board(() => window.kitBoard.shown().length)) === 0)
  const t0 = Date.now()
  await ai([{ command: 'element.paste', params: { payload: bigPayload() } }])
  const pasteMs = Date.now() - t0
  await until(async () => (await board(() => window.kitBoard.shown().length)) > 10, 600)
  const showMs = Date.now() - t0 - pasteMs
  step('5,000 elements open on the canvas', (await elementCount()) === before5000 + 5000, `engine paste ${pasteMs} ms, then shown in ${showMs} ms`)
  const pan = async (zoom) => {
    await board((zoom) => window.kitBoard.setViewport({ x: 600, y: 0, zoom }), zoom)
    await page.waitForTimeout(zoom < 0.5 ? 3000 : 300)
    return board(async () => {
      const kb = window.kitBoard
      const fetchesBefore = kb.fragmentRequests()
      kb.resetMounts()
      const target = document.querySelector('[data-testid=board-canvas]')
      const frame = () => new Promise((resolve) => requestAnimationFrame(resolve))
      const gaps = []
      const work = []
      let last = await frame()
      for (let k = 0; k < 120; k += 1) {
        const started = performance.now()
        target.dispatchEvent(new WheelEvent('wheel', { bubbles: true, cancelable: true, deltaX: k < 60 ? 30 : -30, deltaY: k < 60 ? 20 : -20 }))
        work.push(performance.now() - started)
        const now = await frame()
        gaps.push(now - last)
        last = now
      }
      return { gaps, work, fetches: kb.fragmentRequests() - fetchesBefore, mounts: kb.mounts(), shown: kb.shown().length }
    })
  }
  const full = await pan(1)
  step(
    'pan at 5,000 elements, 100%: median frame ≤ 39 ms (v1) and no fragment fetch',
    median(full.gaps) <= 39 && full.fetches === 0,
    `frame median ${median(full.gaps).toFixed(1)} / p95 ${p95(full.gaps).toFixed(1)} ms, work p95 ${p95(full.work).toFixed(2)} ms, ${full.fetches} fetches, ${full.mounts} mounts, ${full.shown} shown`,
  )
  const low = await pan(0.25)
  step(
    'pan at 5,000 elements, 25% (low detail): median frame ≤ 39 ms and no fragment fetch',
    median(low.gaps) <= 39 && low.fetches === 0,
    `frame median ${median(low.gaps).toFixed(1)} / p95 ${p95(low.gaps).toFixed(1)} ms, work p95 ${p95(low.work).toFixed(2)} ms, ${low.fetches} fetches, ${low.mounts} mounts, ${low.shown} shown`,
  )

  // The commit of a 20-shape move at 5,000 elements: command sent → change shown, one engine
  // command and one undo step per move (spike S2-1: 16 ms). Two moves first warm the path up.
  await board(() => window.kitBoard.setViewport({ x: 600, y: 0, zoom: 1 }))
  await page.waitForTimeout(300)
  const commits = await board(async () => {
    const kb = window.kitBoard
    const ids = [...document.querySelectorAll('[data-testid=board-elements] [data-kind=shape]')].slice(0, 20).map((node) => node.getAttribute('data-id'))
    for (let k = 0; k < 2; k += 1) await kb.move(ids, k % 2 ? -6 : 6, 0)
    const times = []
    for (let k = 0; k < 20; k += 1) times.push(await kb.move(ids, k % 2 ? -6 : 6, 0))
    return times
  })
  step(
    `a 20-shape move at 5,000 elements commits: median ≤ ${commitBudget} ms (S2-1: 16 ms${budgetScale === 1 ? '' : `, × ${budgetScale} on this machine`})`,
    commits.length === 20 && median(commits) <= commitBudget,
    `median ${median(commits).toFixed(1)} / p95 ${p95(commits).toFixed(1)} ms`,
  )

  step('no page errors', errors.length === 0, errors.join(' | '))
} catch (error) {
  step('flow ran to the end', false, `${error instanceof Error ? error.stack : String(error)}${errors.length ? `\npage errors: ${errors.join(' | ')}` : ''}${warnings.length ? `\nwarnings: ${warnings.slice(-5).join(' | ').slice(0, 3000)}` : ''}\nlifecycle: ${lifecycle.slice(-8).join(' | ')}`)
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${which}: ${failures} failed` : `${which}: all passed`)
process.exit(failures ? 1 : 0)
