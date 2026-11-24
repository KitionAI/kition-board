// The board on the host's stored replica (kition-sdk Kit UI protocol 7; a `.kboard` holds the
// content, not its history): node tests/history.mjs [chromium|webkit]. Opens the sample board under
// the development host with the replica store on (`?replica=1`), adds a page (a person's edit),
// saves, finds the replica stored, and reloads the document as its save left it (`devHost.reload()`,
// which hands the stored replica back); then opens the same file alone (`devHost.open`).
// Not asserted yet: Undo of an edit made before the reload. The KDM's undo histories live in the
// session (kition-kdm's UndoManager), not in the replica, so a session opened on the stored replica
// starts with none (kition-sdk 8d56412); see the round-4 report.
// Needs the engine and fonts built into public/ (pnpm run engine). Run it with no HTTP proxy set.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const FILE = 'Sample.kboard'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const context = await browser.newContext({ viewport: { width: 1280, height: 800 } })
const page = await context.newPage()
// The editor's shortcuts take Cmd where the page says it runs on a Mac (kit-editor reads
// navigator.platform), else Ctrl; Playwright's WebKit says Mac on Linux too, so the test asks the page.
const mod = (await page.evaluate(() => /Mac|iPhone|iPad/i.test(navigator.platform))) ? 'Meta' : 'Control'
context.setDefaultTimeout(10000)
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
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
const inspect = () =>
  page.evaluate(async () => {
    const answer = await window.devHost.connection().ai.inspect()
    return answer.details ?? answer
  })
const pageCount = async () => (await inspect()).pages.length
const kit = page.frameLocator('#kit')
/** Waits for the editor in the frame to open the board again after `reopen` ran. */
const reopened = async (reopen) => {
  const frame = await (await page.$('#kit')).contentFrame()
  await frame.evaluate(() => {
    window.beforeReopen = true
  })
  await page.evaluate(reopen, FILE)
  const fresh = await (await page.$('#kit')).contentFrame()
  await fresh.waitForFunction(() => window.beforeReopen === undefined && window.kitBoard != null, null, { timeout: 60000 })
  await until(async () => (await fresh.evaluate(() => window.kitBoard.shown().length)) > 0, 200)
}

try {
  await page.goto(`${origin}/dev.html?modes=classic&replica=1`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90000 })
  const frame = await (await page.$('#kit')).contentFrame()
  await frame.waitForFunction(() => window.kitBoard != null, null, { timeout: 60000 })
  await until(async () => (await frame.evaluate(() => window.kitBoard.shown().length)) > 0, 200)
  const pagesBefore = await pageCount()

  // A person's edit, then a save: the file and the stored replica.
  await kit.getByTestId('board-page-add').click()
  step('Add page makes a page', await until(async () => (await pageCount()) === pagesBefore + 1))
  await kit.getByTestId('board-canvas').focus()
  await page.keyboard.press(`${mod}+s`)
  const fileVersion = () => page.evaluate((path) => window.devHost.files.get(path)?.version ?? 0, FILE)
  step('saves through the host', await until(async () => (await fileVersion()) > 1), `file version ${await fileVersion()}`)
  step('the host stores the replica', await until(async () => page.evaluate((path) => window.devHost.history.replica(path) != null, FILE)))

  // Reloaded as the save left it, on the stored replica: the edit from before the save undoes.
  await reopened((path) => window.devHost.reload())
  step('the reloaded board has the page', (await pageCount()) === pagesBefore + 1, `${await pageCount()} pages`)
  step('the stored replica is still kept after the reload', await page.evaluate((path) => window.devHost.history.replica(path) != null, FILE))

  // The saved file opened alone (no stored replica): the same board.
  await reopened((path) => window.devHost.open(path, window.devHost.files.get(path).bytes))
  step('the file alone has the page', (await pageCount()) === pagesBefore + 1, `${await pageCount()} pages`)
  step('and nothing to undo', (await inspect()).canUndo === false)

  step('no page errors', errors.length === 0, errors.join('; '))
} catch (error) {
  step('runs', false, error instanceof Error ? error.stack : String(error))
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${which}: ${failures} failed` : `${which}: all passed`)
process.exit(failures ? 1 : 0)
