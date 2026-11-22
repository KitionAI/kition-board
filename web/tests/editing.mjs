// Editing on the canvas in a real browser, through the development host: node tests/editing.mjs [chromium|webkit]
// Starts the Vite dev server itself, opens dev.html (Sample.kboard) in Classic, goes to a page of its
// own at a known view, and prints PASS or FAIL per step: Cmd/Ctrl+C then V pastes one copy (selected,
// one undo step), a second paste steps on from the first, cut and paste, a connector pasted with the
// two shapes it joins stays bound to the copies, Alt pressed during a drag leaves the original and
// makes a copy at the drop, the canvas menu's order rows, its empty-canvas rows, a locked element's
// rows (case board.editing/clipboard); a pasted PNG becomes one picture with a blob, one undo takes
// it away, and a dropped PNG lands centred on the drop point (case board.editing/pictures); and a
// read-only board neither cuts nor pastes. Then, on a page of its own: the style panel's Shape
// section turns a rectangle into a document with its box, label and connector kept, and one undo
// brings the rectangle back (case board.editing/change-shape); Cmd/Ctrl+B on a selected word stores
// a bold mark that the engine draws bold, a stored mark shows pressed and survives an edit
// elsewhere, Cmd/Ctrl+K stores a link with its address, and a Markdown label shows no marks bar
// (case board.editing/marks). Then, on a page of its own, the review fixes: a mark made on the last
// of a run of identical characters stays on it when the same character is typed before it
// (regressions/issue-mark-offsets); a link typed with HTTPS:// is stored, and so are the marks after
// it (regressions/issue-link-scheme); Cmd/Ctrl+K pressed again while the link field is open, then a
// click on the canvas, closes the box (regressions/issue-link-field-blur); Cmd/Ctrl+X and Delete on
// a locked shape keep it and its selection (regressions/issue-locked-cut-key); a dropped text file
// gets a notice, inserts nothing and leaves the frame on the board (regressions/issue-file-drop). The
// harness is web/tests/flow.mjs's.
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
const copy = async (ids) => (await ai([{ command: 'clipboard.copy', params: { ids } }])).completed[0].result

/** A 2 × 2 PNG (as in flow.mjs's picture step). */
const PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEUlEQVR4nGMIc73yH4QZYAwAUnYJuZK5ts8AAAAASUVORK5CYII='

/** A stored element's box, whichever form its position and size take. */
const boxOf = (element) => {
  const [x, y] = Array.isArray(element?.position) ? element.position : [element?.position?.x, element?.position?.y]
  const [w, h] = Array.isArray(element?.size) ? element.size : [element?.size?.w, element?.size?.h]
  return [x, y, w, h].every(Number.isFinite) ? { x, y, w, h } : null
}
const near = (a, b, tolerance = 1) => Number.isFinite(a) && Math.abs(a - b) <= tolerance

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
  const click = async (x, y, options) => {
    const at = await screen(x, y)
    await page.mouse.click(at.x, at.y, options)
  }
  const personPage = async () => (await inspect()).page
  /** The person's New page, from the View tab. */
  const newPage = async () => {
    await kit.getByTestId('ribbon-tab-View').click()
    await kit.getByTestId('board-cmd-page.create').click()
    await kit.getByTestId('ribbon-tab-Edit').click()
  }
  /** Copies `ids` and answers their stored elements, in the page's order. */
  const storedOf = async (ids) => {
    const payload = await copy(ids)
    return payload.payload?.elements ?? payload.elements ?? []
  }
  const storedOne = async (id) => (await storedOf([id])).find((element) => element.ref === id) ?? null
  const selected = () => board(() => window.kitBoard.selection())
  const shown = () => board(() => window.kitBoard.shown())
  /** The board's element count, noting what the canvas shows now (for `newest`). */
  let shownBefore = []
  const mark = async () => {
    shownBefore = await shown()
    return elementCount()
  }
  /** Once the board holds `count` elements, the ones the canvas shows that it did not at `mark`. */
  const made = async (count) => {
    await until(async () => (await elementCount()) === count)
    let out = []
    await until(async () => {
      out = (await shown()).filter((id) => !shownBefore.includes(id))
      return out.length > 0
    })
    return out
  }
  const menuRow = (action) => kit.getByTestId(`board-canvas-menu-${action}`)
  const menuOpen = () => until(async () => (await kit.getByTestId('board-canvas-menu').count()) === 1)
  const menuRows = () => kit.locator('[data-testid="board-canvas-menu"] [role^="menuitem"]').evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-testid')?.replace('board-canvas-menu-', '')))

  // A page of its own at a known view (100%, world origin at the canvas corner), with a shape A, two
  // shapes B and C joined by a connector, and a shape D, made by the agent (not the person's undo).
  const pageBefore = await personPage()
  await newPage()
  await until(async () => (await personPage()) !== pageBefore && (await board(() => window.kitBoard.shown().length)) === 0)
  await board(() => window.kitBoard.setViewport({ x: 0, y: 0, zoom: 1 }))
  const shownPage = await personPage()
  const setup = await ai([
    {
      command: 'element.create',
      params: {
        page: shownPage,
        elements: [
          { kind: 'shape', position: { x: 60, y: 60 }, size: { w: 140, h: 90 }, text: 'A' },
          { kind: 'shape', position: { x: 60, y: 300 }, size: { w: 120, h: 80 }, text: 'B' },
          { kind: 'shape', position: { x: 300, y: 300 }, size: { w: 120, h: 80 }, text: 'C' },
          { kind: 'shape', position: { x: 60, y: 480 }, size: { w: 120, h: 80 }, text: 'D' },
        ],
      },
    },
  ])
  const [a, b, c, d] = setup.completed[0].result.created
  const joined = await ai([{ command: 'connector.create', params: { page: shownPage, from: { element: b }, to: { element: c } } }])
  const connector = joined.completed[0].result.created[0]
  await until(async () => (await shown()).length === 5)

  // 1. Cmd/Ctrl+C then V: one copy, selected, as one command; one undo takes it away.
  await click(80, 80)
  await until(async () => JSON.stringify(await selected()) === JSON.stringify([a]))
  let count = await mark()
  await page.keyboard.press(`${mod}+c`)
  await page.keyboard.press(`${mod}+v`)
  const [first] = await made(count + 1)
  await until(async () => JSON.stringify(await selected()) === JSON.stringify([first]))
  const firstStored = first ? await storedOne(first) : null
  const firstSelected = await selected()
  const countPasted = await elementCount()
  await page.keyboard.press(`${mod}+z`)
  await until(async () => (await elementCount()) === count && !(await shown()).includes(first))
  step(
    'Cmd/Ctrl+C then V pastes one copy, selected, as one command',
    countPasted === count + 1 && firstStored?.kind === 'shape' && firstStored?.text === 'A' && JSON.stringify(firstSelected) === JSON.stringify([first]) && (await elementCount()) === count && !(await shown()).includes(first),
    `${count} → ${countPasted} → ${await elementCount()}; selected ${JSON.stringify(firstSelected)}; ${JSON.stringify(firstStored ?? null).slice(0, 160)} ${warnings.slice(-3).join(' | ')}`,
  )

  // 2. Two pastes in a row, the pointer still: the second 16 units on from the first.
  count = await mark()
  await page.keyboard.press(`${mod}+v`)
  const [second] = await made(count + 1)
  await until(async () => JSON.stringify(await selected()) === JSON.stringify([second]))
  count = await mark()
  await page.keyboard.press(`${mod}+v`)
  const [third] = await made(count + 1)
  await until(async () => JSON.stringify(await selected()) === JSON.stringify([third]))
  const secondBox = boxOf(second ? await storedOne(second) : null)
  const thirdBox = boxOf(third ? await storedOne(third) : null)
  step(
    'a second paste is offset from the first',
    Boolean(secondBox && thirdBox) && near(thirdBox.x - secondBox.x, 16) && near(thirdBox.y - secondBox.y, 16) && JSON.stringify(await selected()) === JSON.stringify([third]),
    `${JSON.stringify(secondBox)} then ${JSON.stringify(thirdBox)}`,
  )

  // 3. Cut the selected copy, then paste it back.
  count = await elementCount()
  await page.keyboard.press(`${mod}+x`)
  const cutGone = await until(async () => (await elementCount()) === count - 1 && !(await shown()).includes(third))
  const cutSelection = await selected()
  count = await mark()
  await page.keyboard.press(`${mod}+v`)
  const [back] = await made(count + 1)
  await until(async () => JSON.stringify(await selected()) === JSON.stringify([back]))
  const backStored = back ? await storedOne(back) : null
  step(
    'cut removes the selection and paste brings it back',
    cutGone && cutSelection.length === 0 && backStored?.kind === 'shape' && backStored?.text === 'A' && JSON.stringify(await selected()) === JSON.stringify([back]),
    `gone ${cutGone}, selection after the cut ${JSON.stringify(cutSelection)}; ${JSON.stringify(backStored ?? null).slice(0, 160)}`,
  )

  // 4. B, C and their connector, chosen by a marquee, copied and pasted at an empty place: the copied
  // connector joins the copied shapes.
  await page.keyboard.press('Escape')
  await drag(await screen(40, 270), await screen(440, 400))
  await until(async () => (await selected()).length === 3)
  const chosen = (await selected()).slice().sort()
  await page.keyboard.press(`${mod}+c`)
  const empty = await screen(600, 60)
  await page.mouse.move(empty.x, empty.y)
  count = await mark()
  await page.keyboard.press(`${mod}+v`)
  await made(count + 3)
  await until(async () => (await selected()).length === 3 && !(await selected()).some((id) => chosen.includes(id)))
  const pasted = await selected()
  const pastedStored = await storedOf(pasted)
  const pastedConnector = pastedStored.find((element) => element.kind === 'connector')
  const pastedShapes = pastedStored.filter((element) => element.kind === 'shape').map((element) => element.ref)
  const ends = [pastedConnector?.start?.binding?.target, pastedConnector?.end?.binding?.target]
  step(
    'paste keeps a connector bound between two copied shapes',
    JSON.stringify(chosen) === JSON.stringify([b, c, connector].sort()) && pastedShapes.length === 2 && ends.every((target) => pastedShapes.includes(target)) && ends[0] !== ends[1],
    `chose ${JSON.stringify(chosen)}; pasted ${JSON.stringify(pastedStored.map((element) => [element.ref, element.kind, element.start?.binding?.target, element.end?.binding?.target]))}`,
  )

  // 5. Alt pressed during a drag of D: D stays, a copy lands at the drop (no snapping with Alt),
  // one element.duplicate; one undo takes the copy away.
  await page.keyboard.press('Escape')
  const dBefore = boxOf(await storedOne(d))
  const grab = await screen(120, 520)
  const drawnLeft = () => board((id) => document.querySelector(`[data-id="${id}"]`)?.getBoundingClientRect().left ?? null, d)
  const leftBefore = await drawnLeft()
  count = await mark()
  await page.mouse.move(grab.x, grab.y)
  await page.mouse.down()
  await page.mouse.move(grab.x + 20, grab.y + 5, { steps: 4 })
  await page.keyboard.down('Alt')
  await page.mouse.move(grab.x + 200, grab.y + 40, { steps: 10 })
  // During the drag the original is drawn where it is (the preview is a dashed box at the drop).
  const leftDuring = await drawnLeft()
  const originalStays = leftBefore != null && near(leftDuring, leftBefore)
  await page.mouse.up()
  await page.keyboard.up('Alt')
  const [dCopy] = await made(count + 1)
  const dAfter = boxOf(await storedOne(d))
  const copyBox = boxOf(dCopy ? await storedOne(dCopy) : null)
  const copySelected = await until(async () => JSON.stringify(await selected()) === JSON.stringify([dCopy]))
  const countCopied = await elementCount()
  await page.keyboard.press(`${mod}+z`)
  const undone = await until(async () => (await elementCount()) === count && !(await shown()).includes(dCopy))
  step(
    'Alt-drag leaves the original and makes a copy at the drop',
    originalStays && countCopied === count + 1 && JSON.stringify(dAfter) === JSON.stringify(dBefore) && near(copyBox?.x, dBefore.x + 200) && near(copyBox?.y, dBefore.y + 40) && copySelected && undone,
    `D ${JSON.stringify(dBefore)} → ${JSON.stringify(dAfter)}, drawn at ${leftBefore} then ${leftDuring} during the drag; copy ${dCopy} ${JSON.stringify(copyBox)}; ${count} → ${countCopied}, undone ${undone}`,
  )

  // 6. The canvas menu on A (not selected: the right-click selects it): Send to back, then Bring to front.
  await page.keyboard.press('Escape')
  const order = async () => (await storedOf(await shown())).map((element) => element.ref)
  await click(68, 68, { button: 'right' })
  const opened = await menuOpen()
  const selectedByMenu = JSON.stringify(await selected()) === JSON.stringify([a])
  await menuRow('back').click()
  const atBack = await until(async () => (await order())[0] === a)
  await click(68, 68, { button: 'right' })
  await menuOpen()
  await menuRow('front').click()
  const atFront = await until(async () => (await order()).at(-1) === a)
  step(
    'the canvas menu on an element: Send to back, then Bring to front, change the order',
    opened && selectedByMenu && atBack && atFront,
    `opened ${opened}, selected by the right-click ${selectedByMenu}, back ${atBack}, front ${atFront}: ${JSON.stringify(await order())}`,
  )

  // 7. The canvas menu on the empty canvas: Paste and Select all.
  await click(700, 400, { button: 'right' })
  await menuOpen()
  const emptyRows = await menuRows()
  const pasteOn = !(await menuRow('paste').isDisabled())
  await menuRow('select-all').click()
  const everything = await until(async () => (await selected()).length === (await shown()).length && (await shown()).length > 0)
  step(
    'the canvas menu on empty canvas offers Paste and Select all',
    JSON.stringify(emptyRows) === JSON.stringify(['paste', 'select-all']) && pasteOn && everything,
    `${JSON.stringify(emptyRows)}, paste enabled ${pasteOn}; ${(await selected()).length} of ${(await shown()).length} selected`,
  )

  // 8. Lock D from its menu (element.lock); once the change is on the page its menu offers Unlock,
  // and Delete and Cut are off; Unlock gives the Lock row back. (The clipboard payload does not carry
  // `locked`, so the page's rows are what the step reads.)
  await page.keyboard.press('Escape')
  const menuOnD = async () => {
    await page.keyboard.press('Escape')
    await click(70, 490, { button: 'right' })
    return menuOpen()
  }
  await menuOnD()
  const lockRow = (await menuRow('lock').count()) === 1
  await menuRow('lock').click()
  const unlockOffered = await until(async () => (await menuOnD()) && (await menuRow('unlock').count()) === 1, 30)
  const deleteOff = unlockOffered && (await menuRow('delete').isDisabled())
  const cutOff = unlockOffered && (await menuRow('cut').isDisabled())
  if (unlockOffered) await menuRow('unlock').click()
  const unlocked = await until(async () => (await menuOnD()) && (await menuRow('lock').count()) === 1 && !(await menuRow('delete').isDisabled()), 30)
  await page.keyboard.press('Escape')
  step(
    "a locked element's menu offers Unlock, and Delete is disabled",
    lockRow && unlockOffered && deleteOff && cutOff && unlocked,
    `lock row ${lockRow}, unlock offered ${unlockOffered}, delete off ${deleteOff}, cut off ${cutOff}, unlocked again ${unlocked}`,
  )

  // Pictures (case board.editing/pictures). A paste event carrying a PNG file, on the canvas with the
  // pointer at an empty place; where the browser cannot construct one, the canvas's own paste hook.
  await page.keyboard.press('Escape')
  await click(760, 450)
  await page.waitForTimeout(1200)
  count = await mark()
  const pasteWay = await board((png) => {
    const bytes = Uint8Array.from(atob(png), (ch) => ch.charCodeAt(0))
    const file = new File([bytes], 'pasted.png', { type: 'image/png' })
    let data = null
    try {
      data = new DataTransfer()
      data.items.add(file)
    } catch {
      data = null
    }
    let event = null
    try {
      if (data && data.files.length + data.items.length > 0) event = new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true })
    } catch {
      event = null
    }
    if (event?.clipboardData && event.clipboardData.files.length + event.clipboardData.items.length > 0) {
      document.querySelector('[data-testid="board-canvas"]').dispatchEvent(event)
      return 'a ClipboardEvent'
    }
    window.kitBoard.paste([file])
    return 'kitBoard.paste (no ClipboardEvent with files in this browser)'
  }, PNG)
  const [picture] = await made(count + 1)
  const pictureStored = picture ? await storedOne(picture) : null
  const drawn = picture ? await until(async () => board((id) => Boolean(document.querySelector(`[data-id="${id}"] image[data-blob]`)?.getAttribute('href')), picture), 50) : false
  step(
    'a pasted PNG becomes one image element with a blob',
    (await elementCount()) === count + 1 && pictureStored?.kind === 'image' && typeof pictureStored?.image?.hash === 'string' && pictureStored.image.hash.length > 0 && drawn,
    `through ${pasteWay}; ${picture} ${JSON.stringify(pictureStored ?? null).slice(0, 200)} drawn ${drawn} ${warnings.slice(-3).join(' | ')}`,
  )
  await page.keyboard.press(`${mod}+z`)
  const pictureGone = await until(async () => (await elementCount()) === count && !(await shown()).includes(picture))
  step('one undo removes the pasted picture', pictureGone, `${await elementCount()} elements, shown ${(await shown()).includes(picture)}`)

  // A drop of a PNG file at an empty place: centred on the drop point at its natural size (2 × 2).
  count = await mark()
  const dropWay = await board(
    ({ png, x, y }) => {
      const bytes = Uint8Array.from(atob(png), (ch) => ch.charCodeAt(0))
      const file = new File([bytes], 'dropped.png', { type: 'image/png' })
      const svg = document.querySelector('[data-testid="board-svg"]')
      const box = svg.getBoundingClientRect()
      const v = window.kitBoard.viewport()
      const clientX = box.left + (x - v.x) * v.zoom
      const clientY = box.top + (y - v.y) * v.zoom
      let data = null
      try {
        data = new DataTransfer()
        data.items.add(file)
      } catch {
        data = null
      }
      let event = null
      try {
        if (data && data.files.length > 0) event = new DragEvent('drop', { dataTransfer: data, clientX, clientY, bubbles: true, cancelable: true })
      } catch {
        event = null
      }
      if (event?.dataTransfer && event.dataTransfer.files.length > 0) {
        svg.dispatchEvent(event)
        return 'a DragEvent'
      }
      window.kitBoard.drop([file], clientX, clientY)
      return 'kitBoard.drop (no DragEvent with files in this browser)'
    },
    { png: PNG, x: 900, y: 300 },
  )
  const [dropped] = await made(count + 1)
  const droppedStored = dropped ? await storedOne(dropped) : null
  const droppedBox = boxOf(droppedStored)
  step(
    'a dropped PNG lands at the drop point',
    droppedStored?.kind === 'image' && near(droppedBox?.w, 2, 0.01) && near(droppedBox?.x + droppedBox?.w / 2, 900, 0.5) && near(droppedBox?.y + droppedBox?.h / 2, 300, 0.5),
    `through ${dropWay}; ${dropped} ${JSON.stringify(droppedStored ?? null).slice(0, 200)}`,
  )
  step('no page errors', errors.length === 0, errors.join(' | '))

  // 9. Read-only: the person may select and copy, but neither cut nor paste; the menu offers Copy only.
  const viewer = await context.newPage()
  const viewerErrors = []
  viewer.on('pageerror', (error) => viewerErrors.push(error.message))
  await viewer.goto(`${origin}/dev.html?modes=classic&role=viewer`)
  await viewer.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90000 })
  const viewerKit = viewer.frameLocator('#kit')
  const viewerFrame = await (await viewer.$('#kit')).contentFrame()
  await viewerFrame.waitForFunction(() => window.kitBoard != null && window.kitBoard.shown().length > 0, null, { timeout: 60000 })
  const target = await viewerFrame.evaluate(() => document.querySelector('[data-kind=shape]')?.getAttribute('data-id') ?? null)
  const at = await viewerFrame.evaluate((id) => {
    const box = window.kitBoard.worldBox(id)
    const svg = document.querySelector('[data-testid="board-svg"]').getBoundingClientRect()
    const v = window.kitBoard.viewport()
    return box ? { x: svg.left + ((box.x0 + box.x1) / 2 - v.x) * v.zoom, y: svg.top + ((box.y0 + box.y1) / 2 - v.y) * v.zoom } : null
  }, target)
  const kitFrame = await viewer.locator('#kit').boundingBox()
  const point = at && kitFrame ? { x: kitFrame.x + at.x, y: kitFrame.y + at.y } : null
  let readOnlyHeld = false
  let readOnlyMenu = []
  let readOnlyDetail = `no shape (${target}) or no point`
  if (point) {
    await viewer.mouse.click(point.x, point.y)
    const picked = await until(async () => JSON.stringify(await viewerFrame.evaluate(() => window.kitBoard.selection())) === JSON.stringify([target]))
    const before = await viewerFrame.evaluate(() => window.kitBoard.shown())
    await viewer.keyboard.press(`${mod}+c`)
    await viewer.keyboard.press(`${mod}+x`)
    await viewer.keyboard.press(`${mod}+v`)
    await viewer.waitForTimeout(1500)
    const after = await viewerFrame.evaluate(() => window.kitBoard.shown())
    readOnlyHeld = picked && JSON.stringify(after) === JSON.stringify(before) && after.includes(target)
    await viewer.mouse.click(point.x, point.y, { button: 'right' })
    await until(async () => (await viewerKit.getByTestId('board-canvas-menu').count()) === 1)
    readOnlyMenu = await viewerKit.locator('[data-testid="board-canvas-menu"] [role^="menuitem"]').evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-testid')?.replace('board-canvas-menu-', '')))
    readOnlyDetail = `selected ${picked}; ${before.length} shown before, ${after.length} after; menu ${JSON.stringify(readOnlyMenu)}`
  }
  step('read-only: no paste, no cut', readOnlyHeld && JSON.stringify(readOnlyMenu) === JSON.stringify(['copy']) && viewerErrors.length === 0, `${readOnlyDetail} ${viewerErrors.join(' | ')}`)

  // Changing a shape and marks in a label (cases board.editing/change-shape and board.editing/marks),
  // on a page of its own at a known view: a rectangle R joined to a shape T by a connector, a label
  // L to mark, and a Markdown label M, made by the agent (not the person's undo).
  await viewer.close()
  await page.bringToFront()
  await page.keyboard.press('Escape')
  const pageBeforeMarks = await personPage()
  await newPage()
  await until(async () => (await personPage()) !== pageBeforeMarks && (await shown()).length === 0)
  await board(() => window.kitBoard.setViewport({ x: 0, y: 0, zoom: 1 }))
  const marksPage = await personPage()
  const madeForMarks = await ai([
    {
      command: 'element.create',
      params: {
        page: marksPage,
        elements: [
          { kind: 'shape', position: { x: 80, y: 120 }, size: { w: 200, h: 120 }, text: 'Report' },
          { kind: 'shape', position: { x: 480, y: 120 }, size: { w: 160, h: 100 }, text: 'Archive' },
          { kind: 'shape', position: { x: 80, y: 400 }, size: { w: 360, h: 120 }, text: 'Ship the plan' },
          { kind: 'shape', position: { x: 520, y: 400 }, size: { w: 240, h: 120 }, text: '**Notes**', textFormat: 'markdown' },
        ],
      },
    },
  ])
  const [rectangle, archive, labelled, markdown] = madeForMarks.completed[0].result.created
  const joinedForShape = await ai([{ command: 'connector.create', params: { page: marksPage, from: { element: rectangle }, to: { element: archive } } }])
  const shapeConnector = joinedForShape.completed[0].result.created[0]
  await until(async () => (await shown()).length === 5)
  const shapeType = kit.getByTestId('board-style-shapeType')

  // 10. The style panel's Shape section offers the library's 44 shapes in its four groups; choosing
  // the document changes R's shapeType and nothing else, and the connector stays bound to it.
  await click(100, 140)
  await until(async () => JSON.stringify(await selected()) === JSON.stringify([rectangle]))
  const sectionShown = await until(async () => (await kit.getByTestId('board-style-shape').count()) === 1 && (await shapeType.inputValue()) === 'rectangle', 50)
  const offered = sectionShown
    ? await shapeType.evaluate((select) => ({ groups: [...select.querySelectorAll('optgroup')].map((group) => group.dataset.group), options: select.querySelectorAll('optgroup option').length }))
    : null
  const rectangleBefore = await storedOne(rectangle)
  if (sectionShown) await shapeType.selectOption('document')
  const changedShape = await until(async () => (await storedOne(rectangle))?.shapeType === 'document', 50)
  const rectangleAfter = await storedOne(rectangle)
  const connectorAfter = await storedOne(shapeConnector)
  step(
    'changing a rectangle to a document keeps its box, label and connector',
    sectionShown &&
      offered?.options === 44 &&
      JSON.stringify(offered?.groups) === JSON.stringify(['basic', 'flowchart', 'arrows', 'marks']) &&
      changedShape &&
      JSON.stringify(boxOf(rectangleAfter)) === JSON.stringify(boxOf(rectangleBefore)) &&
      rectangleAfter?.text === 'Report' &&
      connectorAfter?.start?.binding?.target === rectangle &&
      connectorAfter?.end?.binding?.target === archive,
    `section ${sectionShown}, offered ${JSON.stringify(offered)}; ${JSON.stringify(rectangleBefore ?? null).slice(0, 160)} → ${JSON.stringify(rectangleAfter ?? null).slice(0, 160)}; connector ${JSON.stringify([connectorAfter?.start?.binding, connectorAfter?.end?.binding])}`,
  )

  // 11. One undo (the frame's Undo button) brings the rectangle back; the panel follows.
  await kit.getByTestId('board-undo').click()
  const shapeUndone = await until(async () => (await storedOne(rectangle))?.shapeType === 'rectangle', 50)
  const panelFollows = await until(async () => (await shapeType.inputValue()) === 'rectangle', 30)
  const connectorUndone = await storedOne(shapeConnector)
  const stillBound = connectorUndone?.start?.binding?.target === rectangle && connectorUndone?.end?.binding?.target === archive
  step(
    'one undo brings the rectangle back',
    shapeUndone && panelFollows && stillBound,
    `shape back ${shapeUndone}, panel follows ${panelFollows}, connector bound ${stillBound}; now shapeType ${(await storedOne(rectangle))?.shapeType}, panel ${await shapeType.inputValue().catch(() => 'gone')}, connector ${JSON.stringify([connectorUndone?.start, connectorUndone?.end])}`,
  )

  // 12. Cmd/Ctrl+B on "the" in L's label: the bar's Bold is pressed, the commit stores a bold mark
  // over the word, and the engine's fragment draws it bold.
  const editor = kit.getByTestId('board-text-editor')
  const openLabel = async (x, y) => {
    await page.keyboard.press('Escape')
    const at = await screen(x, y)
    await page.mouse.dblclick(at.x, at.y)
    return until(async () => (await editor.count()) === 1 && (await editor.evaluate((field) => field.ownerDocument.activeElement === field)), 50)
  }
  const runsOf = async (id) => (await storedOne(id))?.runs ?? []
  const runWith = (runs, mark) => runs.find((run) => run.attributes?.[mark] != null) ?? null
  const labelOpened = await openLabel(260, 460)
  const barShown = labelOpened && (await kit.getByTestId('board-text-marks').count()) === 1
  await editor.evaluate((field) => field.setSelectionRange(5, 8))
  await page.keyboard.press(`${mod}+b`)
  const pressedIs = (mark, value) => until(async () => (await kit.getByTestId(`board-text-${mark}`).getAttribute('aria-pressed')) === value, 20)
  const boldPressed = await pressedIs('bold', 'true')
  await page.keyboard.press('Enter')
  const boldStored = await until(async () => runWith(await runsOf(labelled), 'bold')?.insert === 'the', 50)
  const boldDrawn = await until(
    async () =>
      board(
        (id) => [...document.querySelectorAll(`[data-id="${id}"] tspan`)].some((span) => Number(span.getAttribute('font-weight')) >= 700 && span.textContent === 'the'),
        labelled,
      ),
    50,
  )
  step(
    'bold on a selected word is stored as a mark and drawn bold',
    barShown && boldPressed && boldStored && boldDrawn,
    `bar ${barShown}, pressed ${boldPressed}, stored ${boldStored}, drawn ${boldDrawn}: ${JSON.stringify(await runsOf(labelled))} ${warnings.slice(-3).join(' | ')}`,
  )

  // 13. The stored bold shows pressed with the caret inside "the"; Cmd/Ctrl+K on "plan" takes an
  // address in the bar's field, then "!" typed at the end: the commit keeps the bold, stores the
  // link with its address over "plan" only, and the text.
  const reopened = await openLabel(260, 460)
  await editor.evaluate((field) => field.setSelectionRange(0, 0))
  for (let i = 0; i < 6; i += 1) await page.keyboard.press('ArrowRight')
  const storedPressed = await pressedIs('bold', 'true')
  const italicOff = await pressedIs('italic', 'false')
  await editor.evaluate((field) => field.setSelectionRange(9, 13))
  await page.keyboard.press(`${mod}+k`)
  const linkField = kit.getByTestId('board-text-link-url')
  const fieldShown = await until(async () => (await linkField.count()) === 1, 30)
  if (fieldShown) {
    await linkField.fill('https://kition.ai/plan')
    await linkField.press('Enter')
  }
  const backInText = await until(async () => (await linkField.count()) === 0 && (await editor.count()) === 1 && (await editor.evaluate((field) => field.ownerDocument.activeElement === field)), 30)
  const linkPressed = await pressedIs('link', 'true')
  await editor.evaluate((field) => field.setSelectionRange(field.value.length, field.value.length))
  await page.keyboard.type('!')
  await page.keyboard.press('Enter')
  const linked = await until(async () => {
    const element = await storedOne(labelled)
    return element?.text === 'Ship the plan!' && runWith(element.runs ?? [], 'link')?.insert === 'plan'
  }, 50)
  const finalRuns = await runsOf(labelled)
  const linkRun = runWith(finalRuns, 'link')
  step(
    'a link is stored with its address',
    reopened &&
      storedPressed &&
      italicOff &&
      fieldShown &&
      backInText &&
      linkPressed &&
      linked &&
      linkRun?.attributes?.link?.href === 'https://kition.ai/plan' &&
      runWith(finalRuns, 'bold')?.insert === 'the' &&
      !finalRuns.some((run) => run.insert.includes('!') && run.attributes != null),
    `opened ${reopened}, stored bold pressed ${storedPressed}, italic off ${italicOff}, field ${fieldShown}, back ${backInText}, link pressed ${linkPressed}: ${JSON.stringify(finalRuns)} ${warnings.slice(-3).join(' | ')}`,
  )

  // 14. A Markdown label opens its text box without the bar.
  const markdownOpened = await openLabel(640, 460)
  const markdownBar = await kit.getByTestId('board-text-marks').count()
  await page.keyboard.press('Escape')
  const markdownClosed = await until(async () => (await editor.count()) === 0, 30)
  const markdownKept = (await storedOne(markdown))?.text === '**Notes**'
  step('a Markdown label shows no mark bar', markdownOpened && markdownBar === 0 && markdownClosed && markdownKept, `opened ${markdownOpened}, bars ${markdownBar}, closed ${markdownClosed}, text kept ${markdownKept}`)
  step('no page errors while changing shapes and marking labels', errors.length === 0, errors.join(' | '))

  // Review fixes (the regression cases named at each step), on a page of their own at a known view,
  // with labels made by the agent (not the person's undo).
  await page.keyboard.press('Escape')
  const pageBeforeFixes = await personPage()
  await newPage()
  await until(async () => (await personPage()) !== pageBeforeFixes && (await shown()).length === 0)
  await board(() => window.kitBoard.setViewport({ x: 0, y: 0, zoom: 1 }))
  const fixesPage = await personPage()
  const madeForFixes = await ai([
    {
      command: 'element.create',
      params: {
        page: fixesPage,
        elements: [
          { kind: 'shape', position: { x: 60, y: 80 }, size: { w: 200, h: 100 }, text: 'xx' },
          { kind: 'shape', position: { x: 320, y: 80 }, size: { w: 200, h: 100 }, text: 'yy' },
          { kind: 'shape', position: { x: 580, y: 80 }, size: { w: 200, h: 100 }, text: '😀😀' },
        ],
      },
    },
  ])
  const [runX, runY, runEmoji] = madeForFixes.completed[0].result.created
  await until(async () => (await shown()).length === 3)
  /**
   * Opens the label at world point (x, y), selects its last character with the keys, marks it with
   * `markKey` (Cmd/Ctrl+B, or Cmd/Ctrl+K and `href` in the link field), puts the caret before it
   * (ArrowLeft collapses the selection to its start) and types `typed`, then commits with Enter.
   */
  const markLastThenTypeBefore = async (x, y, markKey, href, typed) => {
    const opened = await openLabel(x, y)
    // The box opens with its text selected: ArrowRight goes to the end, Shift+ArrowLeft takes the last character.
    await page.keyboard.press('ArrowRight')
    await page.keyboard.press('Shift+ArrowLeft')
    const chosen = opened ? await editor.evaluate((field) => [field.selectionStart, field.selectionEnd, field.value.length]) : null
    await page.keyboard.press(`${mod}+${markKey}`)
    let fieldUsed = null
    if (href) {
      fieldUsed = await until(async () => (await linkField.count()) === 1, 30)
      if (fieldUsed) {
        await linkField.fill(href)
        await linkField.press('Enter')
      }
      fieldUsed = fieldUsed && (await until(async () => (await linkField.count()) === 0 && (await editor.evaluate((field) => field.ownerDocument.activeElement === field)), 30))
    }
    await page.keyboard.press('ArrowLeft')
    const caret = opened ? await editor.evaluate((field) => [field.selectionStart, field.selectionEnd]) : null
    await page.keyboard.type(typed)
    const typedValue = opened ? await editor.inputValue().catch(() => null) : null
    await page.keyboard.press('Enter')
    const closed = await until(async () => (await editor.count()) === 0, 30)
    return { opened, chosen, fieldUsed, caret, typedValue, closed }
  }
  /** `runs` as [text, mark names] pairs, for the step details. */
  const runList = (runs) => JSON.stringify(runs.map((run) => [run.insert, Object.keys(run.attributes ?? {})]))
  const plain = (run) => run != null && (run.attributes == null || Object.keys(run.attributes).length === 0)
  const said = (name, done) => `${name}: opened ${done.opened}, chose ${JSON.stringify(done.chosen)}, field ${done.fieldUsed}, caret ${JSON.stringify(done.caret)}, typed ${JSON.stringify(done.typedValue)}, closed ${done.closed}`

  // 15. Typing inside a run of identical characters (regressions/issue-mark-offsets): in "xx" the
  // second x made bold, the caret put between the two and an x typed: the bold goes to the old
  // second x (now the third) and the typed x stays plain; the same with a link on the second of
  // "yy" (the typed y is not linked), and with emoji that share their first UTF-16 unit.
  const xDone = await markLastThenTypeBefore(160, 100, 'b', null, 'x')
  const xStored = await until(async () => {
    const runs = await runsOf(runX)
    return (await storedOne(runX))?.text === 'xxx' && runs.length === 2 && plain(runs[0]) && runs[0].insert === 'xx' && runs[1].insert === 'x' && runs[1].attributes?.bold === true
  }, 50)
  const yDone = await markLastThenTypeBefore(420, 100, 'k', 'https://kition.ai/y', 'y')
  const yStored = await until(async () => {
    const runs = await runsOf(runY)
    return (await storedOne(runY))?.text === 'yyy' && runs.length === 2 && plain(runs[0]) && runs[0].insert === 'yy' && runs[1].insert === 'y' && runs[1].attributes?.link?.href === 'https://kition.ai/y'
  }, 50)
  const emojiDone = await markLastThenTypeBefore(680, 100, 'b', null, '😀')
  const emojiStored = await until(async () => {
    const runs = await runsOf(runEmoji)
    return (await storedOne(runEmoji))?.text === '😀😀😀' && runs.length === 2 && plain(runs[0]) && runs[0].insert === '😀😀' && runs[1].insert === '😀' && runs[1].attributes?.bold === true
  }, 50)
  step(
    'typing inside a run of identical characters keeps each mark on its own character',
    xDone.opened && xDone.closed && xStored && yDone.opened && yDone.fieldUsed && yDone.closed && yStored && emojiDone.opened && emojiDone.closed && emojiStored,
    `${said('xx', xDone)}, stored ${xStored} ${runList(await runsOf(runX))}; ${said('yy', yDone)}, stored ${yStored} ${runList(await runsOf(runY))}; ` +
      `${said('emoji', emojiDone)}, stored ${emojiStored} ${runList(await runsOf(runEmoji))} ${warnings.slice(-3).join(' | ')}`,
  )

  /** The texts of the frame's notices of `level` (kit-editor's toasts). */
  const notices = (level) => board((level) => [...document.querySelectorAll(`.kui-toast[data-level=${level}]`)].map((node) => node.textContent ?? ''), level)

  // 16. A link address with its scheme in capitals (regressions/issue-link-scheme): "docs" in "Read
  // the docs" linked to HTTPS://Example.com/Docs, then "Read" made bold, one commit: the link is
  // stored with https in lower case and the bold made after it is stored too, with no notice.
  const madeForScheme = await ai([{ command: 'element.create', params: { page: fixesPage, elements: [{ kind: 'shape', position: { x: 60, y: 260 }, size: { w: 300, h: 100 }, text: 'Read the docs' }] } }])
  const [schemed] = madeForScheme.completed[0].result.created
  await until(async () => (await shown()).includes(schemed))
  const schemeOpened = await openLabel(210, 280)
  await editor.evaluate((field) => field.setSelectionRange(9, 13))
  await page.keyboard.press(`${mod}+k`)
  const schemeField = await until(async () => (await linkField.count()) === 1, 30)
  if (schemeField) {
    await linkField.fill('HTTPS://Example.com/Docs')
    await linkField.press('Enter')
  }
  const schemeBack = await until(async () => (await linkField.count()) === 0 && (await editor.evaluate((field) => field.ownerDocument.activeElement === field)), 30)
  await editor.evaluate((field) => field.setSelectionRange(0, 4))
  await page.keyboard.press(`${mod}+b`)
  const schemeBold = await pressedIs('bold', 'true')
  await page.keyboard.press('Enter')
  const schemeClosed = await until(async () => (await editor.count()) === 0, 30)
  const schemeStored = await until(async () => {
    const runs = await runsOf(schemed)
    return runWith(runs, 'link')?.insert === 'docs' && runWith(runs, 'bold')?.insert === 'Read'
  }, 50)
  const schemeRuns = await runsOf(schemed)
  const schemeNotices = await notices('error')
  step(
    'a link address with its scheme in capitals is stored, and the marks after it too',
    schemeOpened &&
      schemeField &&
      schemeBack &&
      schemeBold &&
      schemeClosed &&
      schemeStored &&
      runWith(schemeRuns, 'link')?.attributes?.link?.href === 'https://Example.com/Docs' &&
      (await storedOne(schemed))?.text === 'Read the docs' &&
      !schemeNotices.some((text) => text.includes('marks')),
    `opened ${schemeOpened}, field ${schemeField}, back ${schemeBack}, bold pressed ${schemeBold}, closed ${schemeClosed}, stored ${schemeStored}: ${JSON.stringify(schemeRuns)}; notices ${JSON.stringify(schemeNotices)} ${warnings.slice(-3).join(' | ')}`,
  )

  // 17. Cmd/Ctrl+K pressed again while the link field is open (regressions/issue-link-field-blur):
  // with the field open on "Close", a click back into the text box and Cmd/Ctrl+K on "box" move the
  // focus to the field; a click on the empty canvas then commits and closes the box (nothing linked,
  // the text kept), and the next click on the canvas selects the shape under it.
  const madeForField = await ai([{ command: 'element.create', params: { page: fixesPage, elements: [{ kind: 'shape', position: { x: 420, y: 260 }, size: { w: 300, h: 100 }, text: 'Close the box' }] } }])
  const [closing] = madeForField.completed[0].result.created
  await until(async () => (await shown()).includes(closing))
  const fieldOpened = await openLabel(570, 280)
  await editor.evaluate((field) => field.setSelectionRange(0, 5))
  await page.keyboard.press(`${mod}+k`)
  const firstField = await until(async () => (await linkField.count()) === 1, 30)
  await editor.click()
  const clickedBack = await until(async () => (await editor.count()) === 1 && (await linkField.count()) === 1 && (await editor.evaluate((field) => field.ownerDocument.activeElement === field)), 30)
  await editor.evaluate((field) => field.setSelectionRange(10, 13))
  await page.keyboard.press(`${mod}+k`)
  const fieldFocused = await until(async () => (await linkField.count()) === 1 && (await linkField.evaluate((input) => input.ownerDocument.activeElement === input)), 30)
  const fieldAddress = fieldFocused ? await linkField.inputValue() : null
  await click(760, 450)
  const fieldClosed = await until(async () => (await editor.count()) === 0 && (await linkField.count()) === 0, 30)
  const closingRuns = await runsOf(closing)
  const closingText = (await storedOne(closing))?.text
  await click(560, 340)
  const pressWorks = await until(async () => JSON.stringify(await selected()) === JSON.stringify([closing]), 30)
  step(
    'Cmd/Ctrl+K again with the link field open, then a click on the canvas, commits and closes the box',
    fieldOpened && firstField && clickedBack && fieldFocused && fieldAddress === '' && fieldClosed && closingText === 'Close the box' && runWith(closingRuns, 'link') == null && pressWorks,
    `opened ${fieldOpened}, field ${firstField}, clicked back ${clickedBack}, field focused again ${fieldFocused} (address ${JSON.stringify(fieldAddress)}), closed ${fieldClosed}, text ${JSON.stringify(closingText)} ${JSON.stringify(closingRuns)}, next press selects ${pressWorks} (${JSON.stringify(await selected())}) ${warnings.slice(-3).join(' | ')}`,
  )

  // 18. Cmd/Ctrl+X and Delete on a locked shape (regressions/issue-locked-cut-key): as the canvas
  // menu has Cut and Delete off, the keys do nothing: the shape stays and stays selected.
  await page.keyboard.press('Escape')
  const madeLocked = await ai([{ command: 'element.create', params: { page: fixesPage, elements: [{ kind: 'shape', position: { x: 60, y: 420 }, size: { w: 160, h: 80 }, text: 'Kept' }] } }])
  const [kept] = madeLocked.completed[0].result.created
  await until(async () => (await shown()).includes(kept))
  await ai([{ command: 'element.lock', params: { ids: [kept], locked: true } }])
  // Once the lock is on the page, the shape's menu offers Unlock.
  const lockShown = await until(async () => {
    await page.keyboard.press('Escape')
    await click(80, 440, { button: 'right' })
    return (await menuOpen()) && (await menuRow('unlock').count()) === 1
  }, 30)
  await page.keyboard.press('Escape')
  await click(80, 440)
  const lockedSelected = await until(async () => JSON.stringify(await selected()) === JSON.stringify([kept]), 30)
  const countLocked = await elementCount()
  await page.keyboard.press(`${mod}+x`)
  await page.waitForTimeout(1500)
  const afterCut = { selection: await selected(), shown: (await shown()).includes(kept), count: await elementCount() }
  await page.keyboard.press('Delete')
  await page.waitForTimeout(1500)
  const afterDelete = { selection: await selected(), shown: (await shown()).includes(kept), count: await elementCount() }
  await ai([{ command: 'element.lock', params: { ids: [kept], locked: false } }])
  step(
    'Cmd/Ctrl+X and Delete on a locked shape do nothing and keep the selection',
    lockShown &&
      lockedSelected &&
      JSON.stringify(afterCut.selection) === JSON.stringify([kept]) &&
      afterCut.shown &&
      afterCut.count === countLocked &&
      JSON.stringify(afterDelete.selection) === JSON.stringify([kept]) &&
      afterDelete.shown &&
      afterDelete.count === countLocked,
    `lock on the page ${lockShown}, selected ${lockedSelected}; ${countLocked} elements; after the cut ${JSON.stringify(afterCut)}; after Delete ${JSON.stringify(afterDelete)} ${warnings.slice(-3).join(' | ')}`,
  )

  // 19. A text file dropped on the canvas (regressions/issue-file-drop): the canvas takes the
  // dragover and the drop (their default, opening the file in the frame, is prevented), nothing is
  // inserted, a notice says only pictures can be dropped, and the frame still shows this board.
  await page.keyboard.press('Escape')
  const frameUrl = frame.url()
  const shownBeforeDrop = await shown()
  const countBeforeDrop = await elementCount()
  const textDrop = await board(
    ({ x, y }) => {
      const file = new File(['Plain words, not a picture.'], 'notes.txt', { type: 'text/plain' })
      const svg = document.querySelector('[data-testid="board-svg"]')
      const box = svg.getBoundingClientRect()
      const v = window.kitBoard.viewport()
      const clientX = box.left + (x - v.x) * v.zoom
      const clientY = box.top + (y - v.y) * v.zoom
      let data = null
      try {
        data = new DataTransfer()
        data.items.add(file)
      } catch {
        data = null
      }
      const made = (type) => {
        try {
          return data && data.files.length > 0 ? new DragEvent(type, { dataTransfer: data, clientX, clientY, bubbles: true, cancelable: true }) : null
        } catch {
          return null
        }
      }
      const over = made('dragover')
      const drop = made('drop')
      if (over?.dataTransfer?.files.length && drop?.dataTransfer?.files.length) {
        const overTaken = !svg.dispatchEvent(over)
        const dropTaken = !svg.dispatchEvent(drop)
        return { way: 'DragEvents', overTaken, dropTaken }
      }
      window.kitBoard.drop([file], clientX, clientY)
      return { way: 'kitBoard.drop (no DragEvent with files in this browser; the defaults are not checked)', overTaken: null, dropTaken: null }
    },
    { x: 760, y: 450 },
  )
  const dropNotice = await until(async () => (await notices('warning')).some((text) => text.includes('Only pictures can be dropped')), 30)
  await page.waitForTimeout(1000)
  const stillHere = frame.url() === frameUrl && !frame.isDetached() && (await board(() => window.kitBoard != null))
  const shownAfterDrop = await shown()
  const countAfterDrop = await elementCount()
  const defaultsTaken = textDrop.way === 'DragEvents' ? textDrop.overTaken === true && textDrop.dropTaken === true : true
  step(
    'a dropped text file is refused with a notice and the frame keeps the board',
    defaultsTaken && dropNotice && stillHere && countAfterDrop === countBeforeDrop && JSON.stringify(shownAfterDrop) === JSON.stringify(shownBeforeDrop) && shownAfterDrop.length > 0,
    `through ${textDrop.way}: dragover taken ${textDrop.overTaken}, drop taken ${textDrop.dropTaken}; notice ${dropNotice} ${JSON.stringify(await notices('warning'))}; frame kept ${stillHere}; ${countBeforeDrop} → ${countAfterDrop} elements, ${shownBeforeDrop.length} → ${shownAfterDrop.length} shown; lifecycle ${lifecycle.slice(-3).join(' | ')}`,
  )
  step('no page errors in the review fixes', errors.length === 0, errors.join(' | '))
} catch (error) {
  step('editing ran to the end', false, `${error instanceof Error ? error.stack : String(error)}${errors.length ? `\npage errors: ${errors.join(' | ')}` : ''}${warnings.length ? `\nwarnings: ${warnings.slice(-5).join(' | ').slice(0, 3000)}` : ''}\nlifecycle: ${lifecycle.slice(-8).join(' | ')}`)
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${which}: ${failures} failed` : `${which}: all passed`)
process.exit(failures ? 1 : 0)
