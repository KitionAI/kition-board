// The shared start-up of the by-hand tests (web/tests/hand/*.mjs): the Vite dev server, dev.html in
// Chromium or WebKit, real mouse and keyboard events on the page, the board prepared through the
// development host's agent connection (never instead of a gesture under test), and the editor's
// read-only hooks (`window.kitBoard`). Coordinates come from the board's boxes and the view, never
// written in. A test runs named scenarios and prints PASS or FAIL per check; the arguments after the
// browser pick scenarios by name. No check reads a clock.
// Run with the proxy variables unset: the dev server is on loopback.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

export async function open(which = 'chromium') {
  const server = await createServer({ configFile: new URL('../../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
  await server.listen()
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`
  const browser = await (which === 'webkit' ? webkit.launch() : chromium.launch({ args: process.platform === 'linux' ? ['--font-render-hinting=none'] : [] }))
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })
  context.setDefaultTimeout(10000)
  const page = await context.newPage()
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  // The editor's shortcuts take Cmd where the page says it runs on a Mac, else Ctrl (web/tests/flow.mjs).
  const mod = (await page.evaluate(() => /Mac|iPhone|iPad/i.test(navigator.platform))) ? 'Meta' : 'Control'
  let failures = 0
  const checks = []
  const step = (name, ok, detail = '') => {
    if (!ok) failures += 1
    checks.push({ name, ok })
    console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
  }
  const until = async (check, tries = 100) => {
    for (let i = 0; i < tries; i += 1) {
      if (await check().catch(() => false)) return true
      await page.waitForTimeout(50)
    }
    return false
  }

  await page.goto(`${origin}/dev.html?modes=classic`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90000 })
  const kit = page.frameLocator('#kit')
  const frame = await (await page.$('#kit')).contentFrame()
  await frame.waitForFunction(() => window.kitBoard != null, null, { timeout: 60000 })
  await frame.evaluate(() => document.fonts.ready)

  const board = (fn, arg) => frame.evaluate(fn, arg)
  const ai = (steps) => page.evaluate(async (steps) => window.devHost.connection().ai.run(steps, false), steps)
  const inspect = async () => {
    const answer = await page.evaluate(async () => window.devHost.connection().ai.inspect())
    return answer.details ?? answer
  }
  const svgBox = () => kit.getByTestId('board-svg').boundingBox()
  /** The page point of board point (x, y). */
  const screen = async (x, y) => {
    const box = await svgBox()
    const v = await board(() => window.kitBoard.viewport())
    return { x: box.x + (x - v.x) * v.zoom, y: box.y + (y - v.y) * v.zoom }
  }
  /** A page rectangle as a board rectangle. */
  const toBoard = async (r) => {
    const box = await svgBox()
    const v = await board(() => window.kitBoard.viewport())
    return { x0: v.x + (r.x0 - box.x) / v.zoom, y0: v.y + (r.y0 - box.y) / v.zoom, x1: v.x + (r.x1 - box.x) / v.zoom, y1: v.y + (r.y1 - box.y) / v.zoom }
  }
  const boxOf = (id) => board((id) => window.kitBoard.worldBox(id), id)
  const centre = async (id) => {
    const b = await boxOf(id)
    return screen((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
  }
  /** Where an element is drawn now (its node's page rectangle, a gesture's preview included), as a board rectangle. */
  const drawnBox = async (id) => {
    const r = await board((id) => {
      const node = document.querySelector(`[data-testid="board-elements"] [data-id="${id}"]`)
      const b = node?.getBoundingClientRect()
      return b ? { x0: b.left, y0: b.top, x1: b.right, y1: b.bottom } : null
    }, id)
    if (!r) return null
    const frameBox = await (await page.$('#kit')).boundingBox()
    return toBoard({ x0: r.x0 + frameBox.x, y0: r.y0 + frameBox.y, x1: r.x1 + frameBox.x, y1: r.y1 + frameBox.y })
  }
  /** The page point of the middle of the element with `testId` in the editor's frame, or null. */
  const testid = async (testId) => {
    const box = await kit.getByTestId(testId).first().boundingBox().catch(() => null)
    return box ? { x: box.x + box.width / 2, y: box.y + box.height / 2 } : null
  }
  /** The computed cursor at a page point. */
  const cursorAt = async (p) => {
    const frameBox = await (await page.$('#kit')).boundingBox()
    return board(([x, y]) => {
      const node = document.elementFromPoint(x, y)
      return node ? getComputedStyle(node).cursor : null
    }, [p.x - frameBox.x, p.y - frameBox.y])
  }
  const selection = () => board(() => window.kitBoard.selection())
  const shown = () => board(() => window.kitBoard.shown())
  const tool = () => kit.getByTestId('board-canvas').getAttribute('data-tool')
  const editorOpen = async () => (await kit.getByTestId('board-text-editor').count()) === 1
  /** The stored elements of `ids` (an agent's copy reads them). */
  const stored = async (ids) => {
    const answer = await ai([{ command: 'edit.copy', params: { ids } }])
    const payload = answer.completed[0].result
    return payload.payload?.elements ?? payload.elements ?? []
  }
  const storedOne = async (id) => (await stored([id])).find((element) => element.ref === id) ?? null
  const near = (a, b, by = 1.5) => a != null && b != null && Math.abs(a - b) <= by
  const sameBox = (a, b, by = 2) => a != null && b != null && near(a.x0, b.x0, by) && near(a.y0, b.y0, by) && near(a.x1, b.x1, by) && near(a.y1, b.y1, by)
  const fmt = (r) => (r ? `[${[r.x0, r.y0, r.x1, r.y1].map((n) => Math.round(n * 10) / 10).join(', ')}]` : 'null')

  /** The person's page: what the editor shows. */
  const personPage = async () => (await inspect()).page
  /** The person's New page (View tab), at a known view: 100%, the board's origin at the canvas corner. */
  const newPage = async () => {
    const before = await personPage()
    await kit.getByTestId('ribbon-tab-View').click()
    await kit.getByTestId('board-cmd-page.add').click()
    await kit.getByTestId('ribbon-tab-Edit').click()
    await until(async () => (await personPage()) !== before && (await shown()).length === 0)
    await board(() => window.kitBoard.setViewport({ x: 0, y: 0, zoom: 1 }))
    return personPage()
  }
  /**
   * Empties the person's page and has the agent put `elements` on it; answers their ids in order,
   * once the canvas shows them, with nothing selected and the view at 100% from the origin.
   */
  const fresh = async (elements) => {
    const pageId = await personPage()
    const old = await shown()
    if (old.length) await ai([{ command: 'element.delete', params: { ids: old } }])
    await until(async () => (await shown()).length === 0)
    await board(() => window.kitBoard.setViewport({ x: 0, y: 0, zoom: 1 }))
    if (!elements.length) return []
    const answer = await ai([{ command: 'element.add', params: { page: pageId, elements } }])
    const ids = answer.completed[0].result.created
    await until(async () => {
      const now = await shown()
      return ids.every((id) => now.includes(id))
    })
    await kit.getByTestId('board-canvas').focus()
    await page.keyboard.press('Escape')
    await page.keyboard.press('Escape')
    await until(async () => (await selection()).length === 0)
    return ids
  }
  /** A drag in steps by real mouse events; `mid` runs halfway (still pressed). */
  const drag = async (from, to, { steps = 10, mid, before } = {}) => {
    await page.mouse.move(from.x, from.y)
    await page.mouse.down()
    if (before) await before()
    for (let i = 1; i <= steps; i += 1) {
      await page.mouse.move(from.x + ((to.x - from.x) * i) / steps, from.y + ((to.y - from.y) * i) / steps)
      if (mid && i === Math.ceil(steps / 2)) await mid()
    }
    await page.mouse.up()
  }

  /** Runs the scenarios named on the command line (all when none is named), in order. */
  const run = async (scenarios) => {
    const wanted = process.argv.slice(3)
    for (const [name, body] of scenarios) {
      if (wanted.length && !wanted.includes(name)) continue
      console.log(`# ${name}`)
      try {
        await body()
      } catch (error) {
        step(`${name} ran to the end`, false, error instanceof Error ? error.stack : String(error))
        // Leave nothing pressed for the next scenario.
        await page.mouse.up().catch(() => undefined)
        for (const key of ['Shift', 'Alt', 'Control', 'Meta', 'Space']) await page.keyboard.up(key).catch(() => undefined)
      }
    }
    step('no page errors', errors.length === 0, errors.join(' | '))
    await browser.close()
    await server.close()
    console.log(failures ? `${which}: ${failures} failed` : `${which}: all passed`)
    process.exit(failures ? 1 : 0)
  }

  return { page, kit, frame, mod, step, until, board, ai, inspect, svgBox, screen, toBoard, boxOf, centre, drawnBox, testid, cursorAt, selection, shown, tool, editorOpen, stored, storedOne, near, sameBox, fmt, personPage, newPage, fresh, drag, run }
}
