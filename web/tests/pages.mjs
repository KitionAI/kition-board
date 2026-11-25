// The page tabs under the board (task board-page-switcher), through the development host:
// node tests/pages.mjs [chromium|webkit]. Opens the sample board and goes through the person's pages:
// add a page and get back, switch by click and by keyboard without dirtying the file, rename (double
// click, F2, Escape), duplicate and reach the copy, move, delete the shown page, Undo and Redo of
// each (with the page shown after them), the selection and the view per page, and an agent's run
// that never moves the person (and leaves them on a neighbour when it deletes their page).
// Needs the engine and fonts built into public/ (pnpm run engine). Run it with no HTTP proxy set.
// SHOT=<dir> writes screenshots of the tabs in light and dark.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const shots = process.env.SHOT ?? null
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const context = await browser.newContext({ viewport: { width: 1280, height: 800 } })
const page = await context.newPage()
// The editor's shortcuts take Cmd where the page says it runs on a Mac (kit-editor reads
// navigator.platform), else Ctrl. Playwright's ControlOrMeta follows the machine running the test
// instead, and its WebKit says Mac on Linux too, so the test asks the page.
const mod = (await page.evaluate(() => /Mac|iPhone|iPad/i.test(navigator.platform))) ? 'Meta' : 'Control'
context.setDefaultTimeout(10000)
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
}
const until = async (check, tries = 60) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
const ai = (steps) => page.evaluate(async (steps) => window.devHost.connection().ai.run(steps, false), steps)
const inspect = () => page.evaluate(async () => {
  const answer = await window.devHost.connection().ai.inspect()
  return answer.details ?? answer
})

try {
  await page.goto(`${origin}/dev.html`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90000 })
  const kit = page.frameLocator('#kit')
  const frame = await (await page.$('#kit')).contentFrame()
  await frame.waitForFunction(() => window.kitBoard != null, null, { timeout: 60000 })
  const board = (fn, arg) => frame.evaluate(fn, arg)
  const shownCount = () => board(() => window.kitBoard.shown().length)
  /** The tabs as the person sees them: id, name, whether shown. */
  const tabs = () =>
    kit.getByTestId('board-page-tabs').evaluate((strip) =>
      [...strip.querySelectorAll('[role=tab]')].map((tab) => ({ id: tab.dataset.page, name: tab.textContent, selected: tab.getAttribute('aria-selected') === 'true' })),
    )
  const tab = (id) => kit.getByTestId(`board-page-tab-${id}`)
  const personPage = async () => (await inspect()).page
  const pageIds = async () => (await inspect()).pages.map((listed) => listed.id)
  /** The tabs match the engine's pages, in order, with the engine's shown page selected. */
  const inStep = async () => {
    const [listed, info] = [await tabs(), await inspect()]
    return listed.length === info.pages.length && listed.every((item, at) => item.id === info.pages[at].id && item.name === info.pages[at].name && item.selected === (item.id === info.page))
  }
  const settled = () => until(inStep)
  const undo = async () => {
    await kit.getByTestId('board-canvas').focus()
    await page.keyboard.press(`${mod}+z`)
  }
  const redo = async () => {
    await kit.getByTestId('board-canvas').focus()
    await page.keyboard.press(`${mod}+Shift+z`)
  }
  const menu = async (id, item) => {
    await tab(id).click({ button: 'right' })
    await kit.getByTestId(`board-page-menu-${item}`).click()
  }
  /** The host's last document/changed: revision and whether the board is unsaved. */
  const lastChange = () => page.evaluate(() => {
    const changes = window.devHost.changes
    const last = changes.at(-1)
    return { count: changes.length, revision: last?.revision ?? 0, dirty: last?.dirty ?? false }
  })

  await until(async () => (await shownCount()) > 0, 200)
  const first = await personPage()
  const firstShown = await shownCount()
  step('the tabs show the board\'s pages, the shown one selected', (await settled()) && (await tabs()).length === 1, JSON.stringify(await tabs()))
  step('the tab bar has accessible names', (await kit.getByRole('tablist', { name: 'Pages' }).count()) === 1 && (await kit.getByRole('button', { name: 'Add page' }).count()) === 1)
  const statusText = () => kit.getByTestId('board-status').textContent()
  step('the status bar counts one page in the singular', await until(async () => / · 1 page$/.test(await statusText())), await statusText())

  // Add page: a new tab, shown; the canvas is empty there.
  await kit.getByTestId('board-page-add').click()
  await until(async () => (await personPage()) !== first)
  const second = await personPage()
  step('Add page makes a page and shows it', (await settled()) && second !== first && (await pageIds()).length === 2 && (await until(async () => (await shownCount()) === 0)))
  step('and two pages in the plural', await until(async () => / · 2 pages$/.test(await statusText())), await statusText())

  // Undo of the new page takes the person back to the page they were on; Redo shows it again.
  await undo()
  step('Undo of Add page takes the person back', (await until(async () => (await pageIds()).length === 1 && (await personPage()) === first)) && (await settled()) && (await until(async () => (await shownCount()) === firstShown)))
  await redo()
  step('Redo of Add page shows it again', (await until(async () => (await personPage()) === second)) && (await settled()))

  // Saved, then switching back and forth: the file stays saved, the revision stays, no undo step.
  await kit.getByTestId('board-canvas').focus()
  await page.keyboard.press(`${mod}+s`)
  await until(async () => !(await lastChange()).dirty)
  const saved = await lastChange()
  step('the board is saved before switching', !saved.dirty, JSON.stringify(saved))
  const undoBefore = (await inspect()).canUndo
  await tab(first).click()
  step('a click on a tab shows its page', (await until(async () => (await personPage()) === first)) && (await settled()) && (await until(async () => (await shownCount()) === firstShown)))
  await tab(second).click()
  await until(async () => (await personPage()) === second)
  await tab(first).click()
  await until(async () => (await personPage()) === first)
  await page.waitForTimeout(300)
  const after = await lastChange()
  step('switching pages does not mark the file unsaved or change its revision', !after.dirty && after.revision === saved.revision, `${JSON.stringify(saved)} -> ${JSON.stringify(after)}`)
  step('switching pages adds no undo step', (await inspect()).canUndo === undoBefore)
  const status = await page.evaluate(() => document.getElementById('status')?.textContent ?? '')
  step('the host does not say unsaved', !status.includes('unsaved'), status)

  // Keyboard: the arrows move between the tabs, Enter shows one, Escape gives the keys back.
  await tab(first).focus()
  await page.keyboard.press('ArrowRight')
  step('ArrowRight moves the focus to the next tab', await until(() => tab(second).evaluate((node) => node === document.activeElement)))
  step('the focus alone does not switch', (await personPage()) === first)
  await page.keyboard.press('Enter')
  step('Enter shows the focused tab\'s page', (await until(async () => (await personPage()) === second)) && (await settled()))
  await page.keyboard.press('Home')
  await page.keyboard.press('Enter')
  step('Home and Enter go back to the first page', await until(async () => (await personPage()) === first))
  await page.keyboard.press('Escape')
  step('Escape gives the keys to the canvas', await until(() => kit.getByTestId('board-canvas').evaluate((node) => node === document.activeElement)))

  // The selection stays on its page, and each page keeps its own view.
  await kit.getByTestId('board-canvas').focus()
  await page.keyboard.press(`${mod}+a`)
  step('a selection on the first page', await until(async () => (await board(() => window.kitBoard.selection().length)) === firstShown))
  /** The minimap's picture, as a checksum of its pixels. */
  const minimap = () =>
    board(() => {
      const map = document.querySelector('[data-testid="board-minimap"] canvas')
      const data = map?.getContext('2d')?.getImageData(0, 0, map.width, map.height).data ?? []
      let sum = 0
      for (let at = 0; at < data.length; at += 1) sum = (sum * 31 + data[at]) % 1000000007
      return `${map?.width}x${map?.height}:${sum}`
    })
  await board(() => window.kitBoard.setViewport({ x: -137, y: -59, zoom: 0.75 }))
  await page.waitForTimeout(800)
  const firstMap = await minimap()
  await tab(second).click()
  step('the selection does not come to another page', (await until(async () => (await personPage()) === second)) && (await until(async () => (await board(() => window.kitBoard.selection().length)) === 0)))
  // At once, not after the 400 ms a burst of edits waits for.
  step('the minimap draws the other page at once', await until(async () => (await minimap()) !== firstMap, 3))
  await board(() => window.kitBoard.setViewport({ x: 400, y: 300, zoom: 1.5 }))
  await tab(first).click()
  await until(async () => (await personPage()) === first)
  const back = await board(() => window.kitBoard.viewport())
  step('and the first page again', await until(async () => (await minimap()) === firstMap, 3))
  step('the first page shows where it was left', Math.abs(back.x + 137) < 0.01 && Math.abs(back.y + 59) < 0.01 && back.zoom === 0.75, JSON.stringify(back))
  await tab(second).click()
  await until(async () => (await personPage()) === second)
  const there = await board(() => window.kitBoard.viewport())
  step('and the second page where it was left', there.x === 400 && there.y === 300 && there.zoom === 1.5, JSON.stringify(there))

  // Rename: a double-click opens the name, Enter keeps it; Escape (after F2) leaves it.
  const nameOf = async (id) => (await inspect()).pages.find((listed) => listed.id === id)?.name
  const oldName = await nameOf(second)
  await tab(second).dblclick()
  const field = kit.getByTestId('board-page-rename')
  step('a double-click opens the name to edit, labelled', (await field.count()) === 1 && (await field.getAttribute('aria-label')) === 'Page name')
  await page.keyboard.type('Ideas')
  await page.keyboard.press('Enter')
  step('Enter renames the page', (await until(async () => (await nameOf(second)) === 'Ideas')) && (await settled()))
  step('typing a name does not reach the canvas (no tool letter)', (await kit.getByTestId('board-canvas').getAttribute('data-tool')) === 'select')
  await tab(second).focus()
  await page.keyboard.press('F2')
  await page.keyboard.type('Nope')
  await page.keyboard.press('Escape')
  await page.waitForTimeout(200)
  step('F2 then Escape leaves the name', (await nameOf(second)) === 'Ideas' && (await field.count()) === 0)
  await undo()
  step('Undo of the rename', await until(async () => (await nameOf(second)) === oldName && (await settled())))
  await redo()
  step('Redo of the rename', await until(async () => (await nameOf(second)) === 'Ideas' && (await settled())))

  // Duplicate the first page from its menu: the copy is shown with the source's elements.
  await tab(first).click()
  await until(async () => (await personPage()) === first)
  await menu(first, 'duplicate')
  await until(async () => (await pageIds()).length === 3 && (await personPage()) !== first)
  const copy = await personPage()
  step('Duplicate makes a copy right after the page and shows it', (await pageIds())[1] === copy && (await settled()) && (await until(async () => (await shownCount()) === firstShown)), JSON.stringify(await tabs()))
  await tab(second).click()
  await until(async () => (await personPage()) === second)
  await tab(copy).click()
  step('the copy is reached from its tab', (await until(async () => (await personPage()) === copy)) && (await until(async () => (await shownCount()) === firstShown)))
  await undo()
  step('Undo of Duplicate removes the copy and shows its source', (await until(async () => (await pageIds()).length === 2 && (await personPage()) === first)) && (await settled()))
  await redo()
  step('Redo of Duplicate shows the copy again', (await until(async () => (await pageIds()).length === 3 && (await personPage()) === copy)) && (await settled()))

  // Move: the shown page one place right, and Undo.
  await menu(copy, 'right')
  step('Move right moves the page', (await until(async () => (await pageIds())[2] === copy)) && (await settled()) && (await personPage()) === copy)
  await undo()
  step('Undo of the move', (await until(async () => (await pageIds())[1] === copy)) && (await settled()) && (await personPage()) === copy)

  // Delete the shown page: the person lands on its neighbour; Undo brings it back, shown.
  await menu(copy, 'delete')
  step('Delete of the shown page shows its neighbour', (await until(async () => (await pageIds()).length === 2 && (await personPage()) === first)) && (await settled()))
  await undo()
  step('Undo of the delete shows the page again', (await until(async () => (await pageIds()).length === 3 && (await personPage()) === copy)) && (await settled()))
  await redo()
  step('Redo of the delete', (await until(async () => (await pageIds()).length === 2 && (await personPage()) === first)) && (await settled()))

  // An agent's run never moves the person's page, also when it makes and shows pages of its own.
  await tab(second).click()
  await until(async () => (await personPage()) === second)
  const run = await ai([
    { command: 'page.add', params: { name: 'Agent', activate: true } },
    { command: 'element.add', params: { elements: [{ kind: 'shape', position: [0, 0], text: 'by the agent' }] } },
  ])
  const agentPage = run.completed?.[0]?.result?.page
  await page.waitForTimeout(300)
  step("an agent's run does not move the person's page", Boolean(agentPage) && (await personPage()) === second && (await settled()) && (await tabs()).some((item) => item.id === agentPage && item.name === 'Agent'), JSON.stringify(run).slice(0, 300))
  // An agent deleting the person's page leaves them on its neighbour, not on nothing.
  await tab(agentPage).click()
  await until(async () => (await personPage()) === agentPage)
  const before = await pageIds()
  const neighbour = before[before.indexOf(agentPage) - 1]
  await ai([{ command: 'page.delete', params: { page: agentPage } }])
  step("an agent's delete of the person's page leaves them on its neighbour", (await until(async () => (await personPage()) === neighbour)) && (await settled()), `${await personPage()} vs ${neighbour}`)

  // The last page's Delete is off (the engine's rule).
  for (const id of (await pageIds()).slice(1)) await ai([{ command: 'page.delete', params: { page: id } }])
  await until(async () => (await pageIds()).length === 1)
  await tab(first).click({ button: 'right' })
  step("the last page's Delete is off", (await kit.getByTestId('board-page-menu-delete').isDisabled()) && (await kit.getByTestId('board-page-menu-delete').textContent()).includes('keeps one page'))
  await page.keyboard.press('Escape')

  if (shots) {
    await kit.getByTestId('board-page-add').click()
    await until(async () => (await pageIds()).length === 2)
    await tab(first).click()
    await until(async () => (await personPage()) === first && (await shownCount()) === firstShown)
    await page.waitForTimeout(300)
    await page.screenshot({ path: `${shots}/pages-light-${which}.png` })
    await page.evaluate(() => window.devHost.setScheme('dark'))
    await page.waitForTimeout(500)
    await page.screenshot({ path: `${shots}/pages-dark-${which}.png` })
    await kit.getByTestId('board-pages').screenshot({ path: `${shots}/pages-bar-dark-${which}.png` })
    await page.evaluate(() => window.devHost.setScheme('light'))
    await page.waitForTimeout(500)
    await kit.getByTestId('board-pages').screenshot({ path: `${shots}/pages-bar-light-${which}.png` })
  }
  step('no page errors', errors.length === 0, errors.join('; '))
} catch (error) {
  step('runs', false, error instanceof Error ? error.stack : String(error))
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${which}: ${failures} failed` : `${which}: all passed`)
process.exit(failures ? 1 : 0)
