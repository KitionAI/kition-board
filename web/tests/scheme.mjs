// The board follows the host's color scheme, through the development host: node tests/scheme.mjs
// [chromium|webkit]. Opens the sample board in the dark scheme and checks that the canvas is the
// dark paper, the default ink is drawn light (and reads at WCAG AA on it), and the minimap is drawn
// dark, and the page tabs are dark and readable; then switches the host to light and back without a reload and checks that the canvas and
// the minimap repaint each time and no element moves. Needs the engine and fonts built into public/
// (pnpm run engine). Run it with no HTTP proxy set. SHOT=<dir> writes screenshots of both schemes.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const shots = process.env.SHOT ?? null
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const context = await browser.newContext({ viewport: { width: 1280, height: 900 } })
const page = await context.newPage()
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

// The engine's papers and inks (crates/render/scene/src/style.rs) and app.css's `--kb-*`.
const PAPER = { light: [255, 255, 255], dark: [30, 30, 30] }
const INK = { light: [30, 30, 30], dark: [232, 232, 230] }
const near = (a, b, by = 3) => a.length >= 3 && b.every((value, index) => Math.abs(value - a[index]) <= by)
const rgb = (text) => (text.match(/[\d.]+/g) ?? []).slice(0, 3).map(Number)
const luminance = ([r, g, b]) => {
  const c = (v) => (v / 255 <= 0.04045 ? v / 255 / 12.92 : ((v / 255 + 0.055) / 1.055) ** 2.4)
  return 0.2126 * c(r) + 0.7152 * c(g) + 0.0722 * c(b)
}
const contrast = (a, b) => (Math.max(luminance(a), luminance(b)) + 0.05) / (Math.min(luminance(a), luminance(b)) + 0.05)

try {
  await page.goto(`${origin}/dev.html?scheme=dark`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90000 })
  const frame = await (await page.$('#kit')).contentFrame()
  await frame.waitForFunction(() => window.kitBoard != null, null, { timeout: 60000 })
  await frame.evaluate(() => document.fonts.ready)
  step('opens the board with its elements', await until(() => frame.evaluate(() => document.querySelectorAll('[data-testid="board-svg"] [data-id]').length > 0), 300))

  /** The canvas's background, the computed colour of the first default-ink line, the minimap's middle pixel. */
  const colours = () =>
    frame.evaluate(() => {
      const canvas = document.querySelector('[data-testid="board-canvas"]')
      const ink = [...document.querySelectorAll('[data-testid="board-svg"] [stroke]')].find((node) => node.getAttribute('stroke').includes('--kb-stroke-ink'))
      const map = document.querySelector('[data-testid="board-minimap"] canvas')
      const context = map?.getContext('2d')
      const corner = context && map.width > 2 ? [...context.getImageData(1, 1, 1, 1).data.slice(0, 3)] : []
      return { background: getComputedStyle(canvas).backgroundColor, ink: ink ? getComputedStyle(ink).stroke : '', minimap: corner }
    })
  /** Where every element is on screen. */
  const places = () =>
    frame.evaluate(() =>
      [...document.querySelectorAll('[data-testid="board-svg"] [data-id]')].map((node) => {
        const box = node.getBoundingClientRect()
        return `${node.getAttribute('data-id')}:${box.x.toFixed(2)},${box.y.toFixed(2)},${box.width.toFixed(2)},${box.height.toFixed(2)}`
      }),
    )
  const paints = (scheme) =>
    until(async () => {
      const now = await colours()
      return near(rgb(now.background), PAPER[scheme]) && near(rgb(now.ink), INK[scheme]) && near(now.minimap, PAPER[scheme], 6)
    }, 80)

  step('the host marks the editor dark', (await frame.evaluate(() => document.documentElement.dataset.colorScheme)) === 'dark')
  step('the canvas, the default ink and the minimap are dark', await paints('dark'), JSON.stringify(await colours()))
  const dark = await colours()
  step('the default ink reads at WCAG AA on the dark paper', contrast(rgb(dark.ink), rgb(dark.background)) >= 4.5, contrast(rgb(dark.ink), rgb(dark.background)).toFixed(2))
  /** The page tabs' colours: the bar, and the shown tab's text on its background. */
  const tabs = () =>
    frame.evaluate(() => {
      const bar = document.querySelector('[data-testid="board-pages"]')
      const shown = document.querySelector('[data-testid="board-pages"] [role=tab][aria-selected=true]')
      return { bar: bar ? getComputedStyle(bar).backgroundColor : '', tab: shown ? getComputedStyle(shown).backgroundColor : '', text: shown ? getComputedStyle(shown).color : '' }
    })
  const darkTabs = await tabs()
  step(
    'the page tabs are dark and the shown page reads at WCAG AA',
    luminance(rgb(darkTabs.bar)) < 0.1 && rgb(darkTabs.tab).length === 3 && contrast(rgb(darkTabs.text), rgb(darkTabs.tab)) >= 4.5,
    JSON.stringify(darkTabs),
  )
  const placed = await places()
  if (shots) await page.screenshot({ path: `${shots}/board-dark-${which}.png` })

  await page.evaluate(() => window.devHost.setScheme('light'))
  step('switched live to light, the canvas, the ink and the minimap repaint light', await paints('light'), JSON.stringify(await colours()))
  step('nothing moves', JSON.stringify(await places()) === JSON.stringify(placed))
  const lightTabs = await tabs()
  step('the page tabs repaint light', luminance(rgb(lightTabs.bar)) > 0.8 && contrast(rgb(lightTabs.text), rgb(lightTabs.tab)) >= 4.5, JSON.stringify(lightTabs))
  if (shots) await page.screenshot({ path: `${shots}/board-light-${which}.png` })

  await page.evaluate(() => window.devHost.setScheme('dark'))
  step('and back to dark', await paints('dark'), JSON.stringify(await colours()))
  step('nothing moved', JSON.stringify(await places()) === JSON.stringify(placed))
  step('no page errors', errors.length === 0, errors.join('; '))
} catch (error) {
  step('runs', false, error instanceof Error ? error.stack : String(error))
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${failures} failed` : 'all passed')
process.exit(failures ? 1 : 0)
