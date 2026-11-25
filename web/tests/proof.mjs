// Export, undo and redo, and templates end to end in a real browser, through the development host:
// node tests/proof.mjs [chromium|webkit]
// Starts the Vite dev server itself, opens dev.html (Sample.kboard) in Classic and prints PASS or
// FAIL per step: the export dialog writes a PNG, an SVG and a PDF of the shown page through the host,
// and the SVG draws a library shape (a cylinder) with its curves (case board.export/editor); a shape
// drawn, moved, filled and deleted by the person comes back state by state with four undos (the
// keys and the frame's Undo button in turn) and goes forward again with four redos, and the board
// saved after one more undo reopens with the shape moved and filled (case board.history/editor);
// each of the six templates is placed by the template gallery as one undo step, comes back with
// redo, and the board saved after all six reopens with every element (case board.templates/editor).
// The harness is web/tests/editing.mjs's. Run with the proxy variables unset: the dev server is on
// loopback.
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
// navigator.platform), else Ctrl (as in editing.mjs).
const mod = (await page.evaluate(() => /Mac|iPhone|iPad/i.test(navigator.platform))) ? 'Meta' : 'Control'
// A missing element fails its step in seconds, not after Playwright's half minute per try.
context.setDefaultTimeout(10000)
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
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

/** A stored element's box, whichever form its position and size take. */
const boxOf = (element) => {
  const [x, y] = Array.isArray(element?.position) ? element.position : [element?.position?.x, element?.position?.y]
  const [w, h] = Array.isArray(element?.size) ? element.size : [element?.size?.w, element?.size?.h]
  return [x, y, w, h].every(Number.isFinite) ? { x, y, w, h } : null
}

/** The workspace's files and their versions (the host writes exports and saves there). */
const fileVersions = () => page.evaluate(() => Object.fromEntries([...window.devHost.files.entries()].map(([name, file]) => [name, file.version])))
/** A workspace file's first bytes as text, its size, and the whole of it as text when asked. */
const fileOf = (name, whole = false) =>
  page.evaluate(
    ({ name, whole }) => {
      const file = window.devHost.files.get(name)
      if (!file) return null
      const bytes = new Uint8Array(file.bytes)
      return { head: String.fromCharCode(...bytes.slice(0, 8)), size: bytes.length, text: whole ? new TextDecoder().decode(bytes) : null }
    },
    { name, whole },
  )

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
  const shown = () => board(() => window.kitBoard.shown())
  const selected = () => board(() => window.kitBoard.selection())
  /** The person's New page, from the View tab, at a known view (100%, world origin at the canvas corner). */
  const ownPage = async () => {
    const before = await personPage()
    await kit.getByTestId('ribbon-tab-View').click()
    await kit.getByTestId('board-cmd-page.add').click()
    await kit.getByTestId('ribbon-tab-Edit').click()
    await until(async () => (await personPage()) !== before && (await shown()).length === 0)
    await board(() => window.kitBoard.setViewport({ x: 0, y: 0, zoom: 1 }))
    return personPage()
  }
  const storedOne = async (id) => {
    const payload = await copy([id])
    return (payload.payload?.elements ?? payload.elements ?? []).find((element) => element.ref === id) ?? null
  }
  const canvas = kit.getByTestId('board-canvas')
  /** Saves with Cmd/Ctrl+S and opens the saved file again; answers whether both happened. */
  const saveAndReopen = async () => {
    const version = await page.evaluate(() => window.devHost.files.get('Sample.kboard')?.version ?? 0)
    await canvas.focus()
    await page.keyboard.press(`${mod}+s`)
    const saved = await until(async () => (await page.evaluate(() => window.devHost.files.get('Sample.kboard')?.version ?? 0)) > version)
    if (!saved) return false
    await page.evaluate(async () => window.devHost.open('Sample.kboard', window.devHost.files.get('Sample.kboard').bytes))
    await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
    await frame.waitForFunction(() => window.kitBoard != null, null, { timeout: 60000 })
    return true
  }

  // 1. Export (case board.export/editor): a cylinder alone on a page of its own, exported from the
  // export dialog as PNG, SVG and PDF; each file is written next to the board through the host.
  const exportPage = await ownPage()
  const cylinder = (await ai([{ command: 'element.add', params: { page: exportPage, elements: [{ kind: 'shape', shapeType: 'cylinder', position: { x: 100, y: 100 }, size: { w: 200, h: 150 } }] } }]))
    .completed[0].result.created[0]
  await until(async () => JSON.stringify(await shown()) === JSON.stringify([cylinder]))
  await page.keyboard.press('Escape')
  const written = {}
  const toasts = () => board(() => [...document.querySelectorAll('.kui-toast[data-level=error]')].map((node) => node.textContent).join(' | '))
  for (const format of ['png', 'svg', 'pdf']) {
    const before = await fileVersions()
    await kit.getByTestId('board-export').click()
    await until(async () => (await kit.getByTestId('board-export-dialog').count()) === 1)
    await kit.getByTestId('board-export-format').selectOption(format)
    await kit.getByTestId('board-export-run').click()
    let name = null
    await until(async () => {
      const now = await fileVersions()
      name = Object.keys(now).find((file) => file.toLowerCase().endsWith(`.${format}`) && now[file] !== before[file]) ?? null
      return name != null || (await toasts()) !== ''
    }, 300)
    written[format] = name ? { name, ...(await fileOf(name, format === 'svg')) } : { error: await toasts() }
    await until(async () => (await kit.getByTestId('board-export-dialog').count()) === 0, 30)
  }
  const png = written.png
  const svg = written.svg
  const pdf = written.pdf
  step(
    'export as PNG, SVG and PDF from the export dialog answers a file of that type',
    png?.head === '\x89PNG\r\n\x1a\n' && svg?.text?.trimStart().startsWith('<svg') && pdf?.head?.startsWith('%PDF-'),
    JSON.stringify({ png: { ...png, text: undefined }, svg: { ...svg, text: svg?.text?.slice(0, 80) }, pdf: { ...pdf, text: undefined } }),
  )
  // The SVG is the engine's scene as usvg writes it: arcs become cubic curves. The cylinder's body
  // and its top's front edge are two curved paths; drawn as its box, the page would have none.
  const curved = (svg?.text?.match(/<path\b[^>]*\bd="[^"]*[CQ][^"]*"/g) ?? []).length
  step('the exported SVG contains a library shape drawn as itself', curved >= 2, `${curved} curved paths in ${svg?.size} bytes`)

  // 2. Undo and redo (case board.history/editor) on a page of its own: the person draws a rectangle
  // (element.add), moves it with Alt held (element.move, no snapping), fills it green from the
  // style panel (element.style) and deletes it (element.delete).
  const historyPage = await ownPage()
  const pageElements = async () => (await inspect()).pages.find((listed) => listed.id === historyPage)?.elements ?? -1
  await page.mouse.click((await svgBox()).x + 5, (await svgBox()).y + 5)
  await page.keyboard.press('r')
  await drag(await screen(100, 100), await screen(260, 200))
  await until(async () => (await pageElements()) === 1 && (await selected()).length === 1)
  const drawn = (await selected())[0]
  /** What the history page holds: `none`, or the shape's box and fill. */
  const state = async () => {
    try {
      if ((await pageElements()) === 0) return 'none'
      const element = await storedOne(drawn)
      return JSON.stringify({ box: boxOf(element), fill: element?.fillColor ?? null })
    } catch (error) {
      return `unreadable: ${error instanceof Error ? error.message : String(error)}`
    }
  }
  const states = [await state()]
  const grab = await screen(180, 150)
  await page.keyboard.down('Alt')
  await drag(grab, { x: grab.x + 120, y: grab.y + 40 })
  await page.keyboard.up('Alt')
  await until(async () => (await state()) !== states[0])
  states.push(await state())
  await kit.getByTestId('board-style-fillColor-green').click()
  await until(async () => (await state()) !== states[1])
  states.push(await state())
  // Still selected; the keys go to the canvas again.
  await canvas.focus()
  await page.keyboard.press('Delete')
  await until(async () => (await state()) === 'none')
  states.push(await state())
  // states: drawn, moved, filled, deleted; before the drawing there was nothing.
  const timeline = ['none', ...states]
  const undoKeys = [() => page.keyboard.press(`${mod}+z`), () => kit.getByTestId('board-undo').click()]
  const redoKeys = [() => page.keyboard.press(`${mod}+Shift+z`), () => kit.getByTestId('board-redo').click()]
  const undone = []
  for (let i = 0; i < 4; i += 1) {
    await canvas.focus()
    await undoKeys[i % 2]()
    const wanted = timeline[3 - i]
    await until(async () => (await state()) === wanted, 50)
    undone.push((await state()) === wanted)
  }
  const redone = []
  for (let i = 0; i < 4; i += 1) {
    await canvas.focus()
    await redoKeys[i % 2]()
    const wanted = timeline[i + 1]
    await until(async () => (await state()) === wanted, 50)
    redone.push((await state()) === wanted)
  }
  // Nothing before the drawing and after the delete; three different states between.
  const distinct = timeline[0] === 'none' && timeline[4] === 'none' && new Set(states.slice(0, 3)).size === 3 && !states.slice(0, 3).includes('none')
  const filledMoved = states[2]
  step(
    'undo then redo of create, move, style and delete restore each state',
    distinct && undone.every(Boolean) && redone.every(Boolean) && filledMoved.includes('"fill":"green"'),
    `states distinct ${distinct}, filled green ${filledMoved.includes('"fill":"green"')}: ${JSON.stringify(timeline)}; undone ${JSON.stringify(undone)}, redone ${JSON.stringify(redone)}`,
  )
  // One more undo brings the filled shape back; the board saved then reopens with it.
  await kit.getByTestId('board-undo').click()
  const beforeSave = await until(async () => (await state()) === filledMoved, 50)
  const reopenedHistory = beforeSave && (await saveAndReopen())
  let afterReopen = null
  if (reopenedHistory) {
    await until(async () => {
      afterReopen = await storedOne(drawn)
      return afterReopen != null
    }, 100)
  }
  step(
    'the redone board survives save and reopen',
    reopenedHistory && JSON.stringify({ box: boxOf(afterReopen), fill: afterReopen?.fillColor ?? null }) === filledMoved,
    `before saving ${beforeSave}, reopened ${reopenedHistory}: ${JSON.stringify(afterReopen ?? null).slice(0, 200)} vs ${filledMoved}`,
  )

  // 3. Templates (case board.templates/editor): each of the six from the template gallery, on a page
  // of its own; one undo takes it away whole, one redo puts it back; the board saved after all six
  // reopens with every element.
  await ownPage()
  const placed = []
  for (const template of ['mindmap', 'flowchart', 'roadmap', 'kanban', 'retrospective', 'storyboard']) {
    const before = await elementCount()
    await kit.getByTestId('board-templates-open').click()
    await until(async () => (await kit.getByTestId('board-templates').count()) === 1)
    await kit.getByTestId(`board-template-${template}`).click()
    await until(async () => (await elementCount()) > before)
    // One command: the count does not move again.
    await page.waitForTimeout(300)
    const after = await elementCount()
    await kit.getByTestId('board-undo').click()
    const undoneWhole = await until(async () => (await elementCount()) === before, 50)
    await kit.getByTestId('board-redo').click()
    const back = await until(async () => (await elementCount()) === after, 50)
    placed.push({ template, made: after - before, undoneWhole, back })
  }
  const total = await elementCount()
  const everyOne = placed.length === 6 && placed.every((item) => item.made > 0 && item.undoneWhole && item.back)
  const reopenedTemplates = await saveAndReopen()
  const kept = reopenedTemplates && (await until(async () => (await elementCount()) === total, 100))
  step(
    'each of the six templates inserts, is one undo step, and survives save and reopen',
    everyOne && kept,
    `${JSON.stringify(placed)}; ${total} elements before saving, ${await elementCount()} after reopening (reopened ${reopenedTemplates})`,
  )
  step('no page errors', errors.length === 0, errors.join(' | '))
} catch (error) {
  step('proof ran to the end', false, `${error instanceof Error ? error.stack : String(error)}${errors.length ? `\npage errors: ${errors.join(' | ')}` : ''}${warnings.length ? `\nwarnings: ${warnings.slice(-5).join(' | ').slice(0, 3000)}` : ''}\nlifecycle: ${lifecycle.slice(-8).join(' | ')}`)
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${which}: ${failures} failed` : `${which}: all passed`)
process.exit(failures ? 1 : 0)
