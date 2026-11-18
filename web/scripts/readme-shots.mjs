// Takes the README's pictures of the Board editor (assets/readme/*.png at the repository root) on
// kition-sdk's shared runner (tools/readme-shots/shots.mjs): the development host opens a showcase
// sample, and each shot shows one ability. Not a test; re-run it by hand when the editor changes:
//
//   cd web && pnpm install && sh scripts/build-engine.sh   # the engine and fonts in public/
//   node scripts/readme-shots.mjs [name ...]                # every shot, or only the named ones
//
// The samples come from a kition-showcase checkout next to this repository (or the one
// KITION_SHOWCASE names). Run it with no HTTP proxy set. Shrink the pictures afterwards with
// pngquant --quality 70-90 (a README picture is at most 400 KB).
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const web = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const sdk = path.resolve(process.env.KITION_SDK ?? path.join(web, '..', '..', 'kition-sdk'))
const showcase = path.resolve(process.env.KITION_SHOWCASE ?? path.join(web, '..', '..', 'kition-showcase'))
const { takeShots, openFile } = await import(pathToFileURL(path.join(sdk, 'tools/readme-shots/shots.mjs')).href)

/** The showcase sample (board/<name>/<name>.kboard) each shot opens. */
const SAMPLES = {
  hero: 'cafe-launch-mind-map',
  'see-1': 'cafe-launch-mind-map',
  'see-2': 'web-shop-architecture',
  'see-3': 'start-stop-continue-retro',
  'see-4': 'bike-coop-swot-analysis',
}
const openSample = (page, name) => openFile(page, { name: `${name}.kboard`, bytes: fs.readFileSync(path.join(showcase, 'board', name, `${name}.kboard`)) })

/** The board's element whose own text is `text` (the smallest one holding it), as a locator. */
async function element(kit, text) {
  const all = kit.locator('[data-testid="board-elements"] [data-id]').filter({ hasText: text })
  let best = null
  for (const candidate of await all.all()) {
    const box = await candidate.boundingBox()
    if (box && (!best || box.width * box.height < best.area)) best = { candidate, area: box.width * box.height, box }
  }
  if (!best) throw new Error(`no element with the text ${JSON.stringify(text)}`)
  return best
}
/** Clicks an element by its text, as a person selects it. */
async function select(page, kit, text) {
  const { box } = await element(kit, text)
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2)
  await page.waitForTimeout(500)
}
/** Draws a freehand stroke through `points` (pairs of fractions of the box of the element with `text`). */
async function stroke(page, kit, text, points) {
  const { box } = await element(kit, text)
  const at = ([fx, fy]) => ({ x: box.x + box.width * fx, y: box.y + box.height * fy })
  const [first, ...rest] = points.map(at)
  await page.mouse.move(first.x, first.y)
  await page.mouse.down()
  for (const point of rest) await page.mouse.move(point.x, point.y, { steps: 8 })
  await page.mouse.up()
  await page.waitForTimeout(500)
}

await takeShots({
  web,
  sample: (page, shot) => openSample(page, SAMPLES[shot.name]),
  ready: '[data-testid="board-elements"] [data-id]',
  query: { modes: 'classic,line,menus,focus' },
  shots: [
    // A café launch as a two-sided mind map, its centre topic selected.
    { name: 'hero', steps: (page, { kit }) => select(page, kit, 'Corner Café') },
    {
      // A mind map branch selected: the buttons to add a child or a sibling and to collapse it, and
      // the Mind map section of the Style panel.
      name: 'see-1',
      steps: (page, { kit }) => select(page, kit, 'Marketing'),
    },
    {
      // A shape in a system diagram selected, with its handles; the connectors stay bound to it.
      name: 'see-2',
      steps: (page, { kit }) => select(page, kit, 'API gateway'),
    },
    {
      // Sticky notes in a retrospective, and a freehand underline drawn with the pen (P) under a column's heading.
      name: 'see-3',
      steps: async (page, { kit }) => {
        await select(page, kit, 'Continue')
        await page.keyboard.press('Escape')
        await page.keyboard.press('p')
        await stroke(page, kit, 'Continue', [[0, 1.25], [0.3, 1.4], [0.6, 1.2], [0.9, 1.35], [1.15, 1.2]])
      },
    },
    {
      // A SWOT board of four frames holding sticky notes, one frame selected (a click on its empty part).
      name: 'see-4',
      steps: async (page, { kit }) => {
        const { box: label } = await element(kit, 'Opportunities')
        for (const frame of await kit.locator('[data-testid="board-elements"] [data-kind="frame"]').all()) {
          const box = await frame.boundingBox()
          if (!box || label.x < box.x || label.x > box.x + box.width || label.y < box.y || label.y > box.y + box.height) continue
          await page.mouse.click(box.x + box.width * 0.5, box.y + box.height - 18)
          await page.waitForTimeout(500)
          return
        }
        throw new Error('no frame around Opportunities')
      },
    },
  ],
})
