// An agent's draft and a restored version in the editor (Kit UI protocol sections 7d and 7m),
// through the development host with the replica and checkpoints kept (`?replica=1&checkpoints=1`)
// and the sample board (case board.agent.draft/editor):
// a. the person adds a page; the agent drafts a shape on the first page; the draft's comparison
//    lists the shape and the board does not show it; the person renames the new page meanwhile; the
//    person accepts the draft: the canvas draws the shape and the tab bar shows the new name;
// b. one Undo (the frame's Undo button) takes back the shape only; Redo brings it back;
// d. a checkpoint saved with a label, a page added after it, the checkpoint restored through the
//    host: the board has the checkpoint's pages; one Undo brings the later page back.
// node tests/drafts.mjs [chromium|webkit]
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const FILE = 'Sample.kboard'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })
context.setDefaultTimeout(10000)
const page = await context.newPage()
// The editor's shortcuts take Cmd where the page says it runs on a Mac, else Ctrl.
const mod = (await page.evaluate(() => /Mac|iPhone|iPad/i.test(navigator.platform))) ? 'Meta' : 'Control'
const kit = page.frameLocator('#kit')
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
const until = async (check, message) => {
  for (let n = 0; n < 200; n += 1) {
    if (await check().catch(() => false)) return
    await page.waitForTimeout(100)
  }
  assert.fail(typeof message === 'function' ? await message() : message)
}
/** The board as the editor answers it (`ai/inspect`), or (`draft`) the agent's draft of it. */
const inspect = (draft = false) =>
  page.evaluate(async (draft) => {
    const answer = await window.devHost.connection().ai.inspect(draft ? { draft: true } : undefined)
    return answer.details ?? answer
  }, draft)
const pageOf = async (id, draft = false) => (await inspect(draft)).pages.find((listed) => listed.id === id)
/** What the canvas draws now: the ids of the shown page's elements. */
const shown = async () => (await (await page.$('#kit')).contentFrame()).evaluate(() => window.kitBoard?.shown() ?? [])
/** The page tabs as the person sees them. */
const tabs = () => kit.getByTestId('board-page-tabs').evaluate((strip) => [...strip.querySelectorAll('[role=tab]')].map((tab) => ({ id: tab.dataset.page, name: tab.textContent })))
const showPage = async (id) => {
  await kit.getByTestId(`board-page-tab-${id}`).click()
  await until(async () => (await inspect()).page === id, `the editor shows page ${id}`)
}
const addPage = async () => {
  const count = (await inspect()).pages.length
  await kit.getByTestId('board-page-add').click()
  await until(async () => (await inspect()).pages.length === count + 1 && (await tabs()).length === count + 1, 'Add page makes a page')
}
const undo = () => kit.getByTestId('board-undo').click()
const redo = () => kit.getByTestId('board-redo').click()
let failures = 0

try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?modes=classic&locale=en-US&replica=1&checkpoints=1`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90000 })
  await until(async () => (await shown()).length > 0, 'the canvas draws the sample board')
  const first = (await inspect()).page
  const elementsBefore = (await pageOf(first)).elements

  // a. The person adds a page.
  await addPage()
  const second = (await inspect()).pages.at(-1).id
  // The agent drafts a shape on the first page; the board does not show it.
  const ran = await page.evaluate(
    async (first) =>
      (await window.devHost.connection().ai.run([{ command: 'element.add', params: { page: first, elements: [{ kind: 'shape', position: { x: 40, y: 40 }, size: { w: 240, h: 100 }, text: 'From the draft' }] } }], false, { draft: true })).completed[0].result,
    first,
  )
  const drafted = ran.created?.[0]
  assert.ok(drafted, `the draft answers the shape's id: ${JSON.stringify(ran)}`)
  assert.equal((await pageOf(first, true)).elements, elementsBefore + 1, 'the draft holds the shape')
  // The comparison lists the shape and changes nothing in the board.
  const compared = await page.evaluate(() => window.devHost.connection().draft.compare({ maxSize: 256 }))
  assert.ok(compared.summary.created.some((change) => change.id === drafted), `the comparison lists the shape: ${JSON.stringify(compared.summary)}`)
  assert.equal((await pageOf(first)).elements, elementsBefore, 'the board does not hold the shape')
  // The person renames the new page meanwhile.
  await kit.getByTestId(`board-page-tab-${second}`).dblclick()
  await kit.getByTestId('board-page-rename').waitFor()
  await page.keyboard.type('Meanwhile')
  await page.keyboard.press('Enter')
  await until(async () => (await pageOf(second))?.name === 'Meanwhile', 'the person renames the page')
  // The person accepts the draft: both edits are in the board as the editor shows it.
  const merged = await page.evaluate(() => window.devHost.connection().draft.merge({ kind: 'agent', principal: 'agent:test' }))
  assert.equal(merged.merged, true, 'the draft merged')
  await showPage(first)
  await until(async () => (await shown()).includes(drafted), 'the canvas draws the merged shape')
  assert.equal((await pageOf(first)).elements, elementsBefore + 1)
  await until(async () => (await tabs()).some((tab) => tab.id === second && tab.name === 'Meanwhile'), 'the tab keeps the name given meanwhile')
  console.log(`PASS ${which}: a draft merged with the person's edits made meanwhile`)

  // b. One Undo takes back the draft's shape only; Redo brings it back.
  await undo()
  await until(async () => !(await shown()).includes(drafted), 'Undo takes the shape back')
  assert.equal((await pageOf(first)).elements, elementsBefore)
  assert.equal((await pageOf(second))?.name, 'Meanwhile', 'the person\'s rename stays')
  assert.equal((await inspect()).pages.length, 2, 'the person\'s page stays')
  await redo()
  await until(async () => (await shown()).includes(drafted), 'Redo brings the shape back')
  assert.equal((await pageOf(first)).elements, elementsBefore + 1)
  console.log(`PASS ${which}: one Undo takes back the draft only, Redo brings it back`)

  // d. A labelled checkpoint of the saved board; a page after it; the checkpoint restored; Undo.
  const version = () => page.evaluate((path) => window.devHost.files.get(path)?.version ?? 0, FILE)
  const saved = await version()
  await kit.getByTestId('board-canvas').focus()
  await page.keyboard.press(`${mod}+s`)
  await until(async () => (await version()) > saved, 'saved through the host')
  await until(() => page.evaluate((path) => window.devHost.history.replica(path) !== null, FILE), 'the replica is stored with the save')
  const checkpoint = await page.evaluate(() => window.devHost.recordCheckpoint({ origin: 'save', label: 'Before the third page' }))
  assert.ok(checkpoint?.id, 'a checkpoint is recorded')
  const listed = await page.evaluate((path) => window.devHost.history.checkpoints(path), FILE)
  assert.ok(listed.some((entry) => entry.id === checkpoint.id && entry.label === 'Before the third page'), `the checkpoint is listed with its label: ${JSON.stringify(listed)}`)
  await addPage()
  assert.equal((await inspect()).pages.length, 3)
  await page.evaluate((id) => window.devHost.restoreCheckpoint(id), checkpoint.id)
  await until(async () => (await inspect()).pages.length === 2 && (await tabs()).length === 2, async () => `the restore gives back the checkpoint's pages: ${JSON.stringify(await tabs())}`)
  assert.equal((await pageOf(first)).elements, elementsBefore + 1, 'the checkpoint holds the merged shape')
  assert.equal((await pageOf(second))?.name, 'Meanwhile')
  await undo()
  await until(async () => (await inspect()).pages.length === 3 && (await tabs()).length === 3, 'Undo brings the later page back')
  console.log(`PASS ${which}: a restored checkpoint is one step of the person's Undo`)
  assert.deepEqual(errors, [], 'no page errors')
} catch (error) {
  failures += 1
  console.log(`FAIL ${which}: ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`)
} finally {
  if (errors.length) console.log(`page errors:\n${errors.slice(0, 8).join('\n')}`)
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
