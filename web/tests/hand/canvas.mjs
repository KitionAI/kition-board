// The canvas by hand (plan "Board usable by hand", stage 0): node tests/hand/canvas.mjs [chromium|webkit] [scenario ...]
// Real mouse and keyboard events only: the keys right after opening, after an agent's edit, after a
// panel button and a dialog (none of them clicks the canvas first); hover and cursors; the 4-pixel
// drag threshold and Escape during a drag; the selection overlay following a move, a resize and a
// rotation; typing into the selected element; a double-click on the empty canvas; a click-created
// shape centred on the pointer; Ctrl+wheel steps and the view keys (Shift+0, Shift+1, Shift+2).
// Stage 4, selection and transforms: the hit rules (a margin round the outline, hollow shapes from
// inside, locked elements as the empty canvas), the look of a selection of several, marquee modes
// (Cmd/Ctrl encloses, Shift adds, frames only whole, no mind-map branches) and edge scrolling, the
// move rules (Shift locks the axis, snapping within 8 px only while slow and not with Cmd/Ctrl, edge
// scrolling, a run of arrow nudges as one undo step), a resize flipping over its opposite side, the
// hover outline of a connector on its real route, a shape's own preview while drawn and Shift for a
// square, groups drilled into by double-click and left by Escape, and a frame's title (move cursor,
// drag, double-click to name, also untitled) with a marquee on its empty inside.
// The cases under cases/board.editor.* and cases/regressions/issue-hand-* name these scenarios.
import { open } from './harness.mjs'

const which = process.argv[2] ?? 'chromium'
const h = await open(which)
const { page, kit, mod, step, until, board, ai, screen, boxOf, centre, drawnBox, testid, cursorAt, selection, tool, editorOpen, storedOne, near, sameBox, fmt, fresh, drag } = h

const RECT = { kind: 'shape', position: { x: 200, y: 200 }, size: { w: 160, h: 100 } }
const NOTE = { kind: 'shape', shapeStyle: 'sticky', position: { x: 520, y: 200 }, size: { w: 180, h: 140 } }
const gestureNow = () => board(() => window.kitBoard.gesture())
const overlayNow = () => board(() => window.kitBoard.overlay())
const transformOf = (id) => board((id) => document.querySelector(`[data-testid="board-elements"] [data-id="${id}"]`)?.getAttribute('transform') ?? '', id)
const rotationIn = (transform) => Number(/rotate\(([-\d.e]+)/.exec(transform)?.[1] ?? 0)
const same = (a, b) => JSON.stringify([...a].sort()) === JSON.stringify([...b].sort())
const positionOf = async (id) => (await storedOne(id))?.position ?? null
/** Clicks the board point (x, y). */
const clickAt = async (x, y) => {
  const at = await screen(x, y)
  await page.mouse.click(at.x, at.y)
}
/** A marquee from board point a to b by real mouse events; `mid` runs before the last step (still pressed). */
const marquee = async (a, b, { mid } = {}) => drag(await screen(a.x, a.y), await screen(b.x, b.y), { mid })
/** The person's page id, for the agent's steps. */
const pageNow = () => h.personPage()

await h.run([
  // F1.2: the editor takes the keys as soon as the board is open, nothing clicked.
  ['focus-open', async () => {
    await page.keyboard.press('r')
    const picked = await until(async () => (await tool()) === 'shape', 20)
    step('right after opening, R picks the shape tool without a click', picked, `tool ${await tool()}`)
    await page.keyboard.press('Escape')
    step('Escape returns to Select', await until(async () => (await tool()) === 'select', 20))
  }],
  // F1.3: after an agent's edit, the keys still reach the canvas (the kit's own focus let go of first).
  ['focus-agent', async () => {
    await board(() => (document.activeElement instanceof HTMLElement ? document.activeElement.blur() : undefined))
    const pageId = (await h.inspect()).page
    await ai([{ command: 'element.add', params: { page: pageId, elements: [{ kind: 'shape', position: { x: 4000, y: 4000 }, size: { w: 80, h: 60 } }] } }])
    await page.waitForTimeout(300)
    await page.keyboard.press('a')
    step("after an agent's edit, A picks the connector without a click", await until(async () => (await tool()) === 'connector', 20), `tool ${await tool()}`)
    await page.keyboard.press('Escape')
  }],
  // Rule 17: after a ribbon tab and after a dialog, the keys go back to the canvas.
  ['focus-panel', async () => {
    await kit.getByTestId('ribbon-tab-Arrange').click()
    await page.keyboard.press('n')
    step('after a ribbon tab, N picks the sticky note', await until(async () => (await tool()) === 'note', 20), `tool ${await tool()}`)
    await page.keyboard.press('Escape')
    await kit.getByTestId('ribbon-tab-Edit').click()
    await kit.getByTestId('board-templates-open').click()
    await until(async () => (await kit.getByTestId('board-templates').count()) === 1)
    await page.keyboard.press('Escape')
    await until(async () => (await kit.getByTestId('board-templates').count()) === 0)
    await page.keyboard.press('p')
    step('after a dialog closes, P picks the pen', await until(async () => (await tool()) === 'pen', 20), `tool ${await tool()}`)
    await page.keyboard.press('Escape')
  }],
  ['setup', async () => {
    await h.newPage()
  }],
  // F2.1, F10.4: hover outline and cursors.
  ['hover', async () => {
    const [shape] = await fresh([RECT])
    const at = await centre(shape)
    await page.mouse.move(at.x - 30, at.y - 10)
    await page.mouse.move(at.x, at.y, { steps: 3 })
    const hovered = await until(async () => (await board(() => window.kitBoard.hover())) === shape, 20)
    const outline = await kit.getByTestId('board-hover').count()
    const hoverBox = (await overlayNow())?.hover?.box ?? null
    step('the pointer over a shape outlines it', hovered && outline === 1 && sameBox(hoverBox, await boxOf(shape)), `hover ${await board(() => window.kitBoard.hover())}, outline ${outline}, box ${fmt(hoverBox)} vs ${fmt(await boxOf(shape))}`)
    step('the cursor over a shape is move', (await cursorAt(at)) === 'move', `${await cursorAt(at)}`)
    const empty = await screen(700, 600)
    await page.mouse.move(empty.x, empty.y, { steps: 3 })
    await until(async () => (await board(() => window.kitBoard.hover())) == null, 20)
    step('off the shape: no outline, the default cursor', (await board(() => window.kitBoard.hover())) == null && (await kit.getByTestId('board-hover').count()) === 0 && (await cursorAt(empty)) === 'default', `${await cursorAt(empty)}`)
    await page.mouse.click(at.x, at.y)
    await until(async () => (await selection())[0] === shape)
    const corner = await testid('board-handle-south-east')
    const rotateAt = await testid('board-handle-rotate')
    const cornerCursor = corner ? await cursorAt(corner) : null
    const rotateCursor = rotateAt ? await cursorAt(rotateAt) : null
    step('a corner handle shows its resize cursor', cornerCursor === 'nwse-resize', `${cornerCursor}`)
    step('the rotate handle shows a rotate cursor', rotateCursor != null && rotateCursor.includes('url('), `${rotateCursor}`)
    await kit.getByTestId('board-canvas').focus()
    await page.mouse.move(empty.x, empty.y)
    await page.keyboard.down('Space')
    const grab = await until(async () => (await cursorAt(empty)) === 'grab', 20)
    await page.mouse.down()
    await page.mouse.move(empty.x + 30, empty.y + 20, { steps: 3 })
    const grabbing = await cursorAt({ x: empty.x + 30, y: empty.y + 20 })
    await page.mouse.up()
    await page.keyboard.up('Space')
    step('Space held shows the grab cursor, grabbing while panning', grab && grabbing === 'grabbing', `grabbing ${grabbing}`)
    await page.keyboard.press('h')
    step('the hand tool shows the grab cursor', await until(async () => (await cursorAt(empty)) === 'grab', 20))
    await page.keyboard.press('v')
  }],
  // Rule 1: nothing moves until the pointer has gone 4 screen pixels; Escape puts everything back.
  ['threshold', async () => {
    const [shape] = await fresh([RECT])
    const before = await boxOf(shape)
    const at = await centre(shape)
    await page.mouse.move(at.x, at.y)
    await page.mouse.down()
    await page.mouse.move(at.x + 2, at.y + 2, { steps: 2 })
    const pressed = await gestureNow()
    await page.mouse.up()
    await page.waitForTimeout(300)
    step('under 4 pixels the press is still a press, and nothing moves', pressed?.type === 'move' && pressed?.phase === 'pressed' && sameBox(await boxOf(shape), before, 0.01), `${JSON.stringify(pressed)} ${fmt(await boxOf(shape))}`)
    await page.mouse.move(at.x, at.y)
    await page.mouse.down()
    await page.mouse.move(at.x + 6, at.y, { steps: 2 })
    const dragging = await gestureNow()
    await page.keyboard.press('Escape')
    step('past 4 pixels it is a drag', dragging?.type === 'move' && dragging?.phase === 'dragging', JSON.stringify(dragging))
    await page.mouse.up()
  }],
  ['escape', async () => {
    const [shape] = await fresh([RECT])
    const before = await boxOf(shape)
    const at = await centre(shape)
    await page.mouse.move(at.x, at.y)
    await page.mouse.down()
    await page.mouse.move(at.x + 120, at.y + 60, { steps: 8 })
    const during = await drawnBox(shape)
    await page.keyboard.press('Escape')
    const back = await drawnBox(shape)
    const state = await board(() => ({ gesture: window.kitBoard.gesture(), captured: document.querySelector('[data-testid="board-svg"]').hasPointerCapture?.(1) ?? false }))
    await page.mouse.move(at.x + 200, at.y + 80, { steps: 4 })
    await page.mouse.up()
    await page.waitForTimeout(400)
    const stored = await storedOne(shape)
    step(
      'Escape during a drag puts the shape back, ends the gesture and lets the pointer go',
      during && during.x0 > before.x0 + 100 && back && near(back.x0, during.x0 - 120, 3) && state.gesture == null && !state.captured && near(stored?.position?.x, 200, 0.01) && near(stored?.position?.y, 200, 0.01),
      `during ${fmt(during)}, after Escape ${fmt(back)}, ${JSON.stringify(state)}, stored ${JSON.stringify(stored?.position)}`,
    )
    const [, other] = await fresh([RECT, { kind: 'shape', position: { x: 600, y: 200 }, size: { w: 120, h: 80 } }])
    await page.mouse.click((await centre(other)).x, (await centre(other)).y)
    await until(async () => (await selection())[0] === other)
    const empty = await screen(100, 600)
    await page.mouse.move(empty.x, empty.y)
    await page.mouse.down()
    await page.mouse.move(empty.x + 400, empty.y - 450, { steps: 8 })
    await page.keyboard.press('Escape')
    await page.mouse.up()
    await page.waitForTimeout(200)
    step('Escape during a marquee keeps the selection from before it', JSON.stringify(await selection()) === JSON.stringify([other]), JSON.stringify(await selection()))
  }],
  // F3.1: the selection overlay follows a move (rule 4).
  ['move-overlay', async () => {
    const [shape] = await fresh([RECT])
    const at = await centre(shape)
    await page.mouse.click(at.x, at.y)
    await until(async () => (await selection())[0] === shape)
    let mid = null
    await drag(at, { x: at.x + 150, y: at.y + 90 }, {
      mid: async () => {
        mid = { overlay: await overlayNow(), drawn: await drawnBox(shape), stored: await boxOf(shape) }
      },
    })
    const box = mid?.overlay?.selection?.box ?? null
    step(
      'mid-drag the selection box is where the shape is drawn, not where it was',
      box && mid.drawn && near(box.x0, mid.drawn.x0, 3) && near(box.y0, mid.drawn.y0, 3) && near(box.x1, mid.drawn.x1, 3) && near(box.y1, mid.drawn.y1, 3) && box.x0 > mid.stored.x0 + 40,
      `overlay ${fmt(box)}, drawn ${fmt(mid?.drawn)}, stored ${fmt(mid?.stored)}`,
    )
    await until(async () => (await boxOf(shape)).x0 > 300)
    const after = (await overlayNow())?.selection?.box ?? null
    step('after the drop the selection box is on the moved shape', sameBox(after, await boxOf(shape), 3), `${fmt(after)} vs ${fmt(await boxOf(shape))}`)
  }],
  ['resize-overlay', async () => {
    const [shape] = await fresh([RECT])
    const at = await centre(shape)
    await page.mouse.click(at.x, at.y)
    await until(async () => (await selection())[0] === shape)
    const corner = await testid('board-handle-south-east')
    let mid = null
    await drag(corner, { x: corner.x + 80, y: corner.y + 60 }, {
      before: () => page.keyboard.down('Alt'),
      mid: async () => {
        mid = { overlay: await overlayNow(), drawn: await drawnBox(shape) }
      },
    })
    await page.keyboard.up('Alt')
    const box = mid?.overlay?.selection?.box ?? null
    step('mid-resize the selection box is the drawn box', box && mid.drawn && sameBox(box, mid.drawn, 3) && box.x1 > 380, `overlay ${fmt(box)}, drawn ${fmt(mid?.drawn)}`)
    await until(async () => (await boxOf(shape)).x1 > 400)
    step('the resize lands as one element.resize', near((await boxOf(shape)).x1, 440, 3), fmt(await boxOf(shape)))
  }],
  // F3.10: the shape turns while the rotate handle is dragged, and the overlay turns with it.
  ['rotate', async () => {
    const [shape] = await fresh([RECT])
    const at = await centre(shape)
    await page.mouse.click(at.x, at.y)
    await until(async () => (await selection())[0] === shape)
    const handle = await testid('board-handle-rotate')
    const samples = []
    // From the handle at the top right corner round to below the centre: about 120° in all.
    await drag(handle, { x: at.x, y: at.y + 150 }, {
      steps: 12,
      mid: async () => {
        samples.push({ transform: await transformOf(shape), overlay: await overlayNow() })
      },
    })
    const turned = rotationIn(samples[0]?.transform ?? '')
    const overlayTurn = samples[0]?.overlay?.selection?.rotation ?? null
    step('mid-rotation the shape is drawn turned', Math.abs(turned) > 30, `transform "${samples[0]?.transform}"`)
    step('and the selection overlay turns with it', overlayTurn != null && near(overlayTurn, turned, 1), `overlay ${overlayTurn} vs ${turned}`)
    const landed = await until(async () => Math.abs((await storedOne(shape))?.rotation ?? 0) > 20)
    step('the rotation lands as one element.rotate', landed, JSON.stringify((await storedOne(shape))?.rotation))
  }],
  // F5.5: typing with one text-capable element selected writes into it.
  ['type-to-edit', async () => {
    const [shape, note] = await fresh([RECT, NOTE])
    await page.mouse.click((await centre(note)).x, (await centre(note)).y)
    await until(async () => (await selection())[0] === note)
    await page.keyboard.type('hello')
    const opened = await until(editorOpen, 20)
    const value = opened ? await kit.getByTestId('board-text-editor').inputValue() : null
    await page.keyboard.press('Enter')
    await until(async () => (await storedOne(note))?.text === 'hello')
    step('a selected sticky note takes what is typed, tool keys included', value === 'hello' && (await storedOne(note))?.text === 'hello' && (await tool()) === 'select', `box ${JSON.stringify(value)}, stored ${JSON.stringify((await storedOne(note))?.text)}, tool ${await tool()}`)
    // Just written, the note stays selected and the next letter is a tool key again (N for the next note).
    await page.keyboard.press('n')
    const noteTool = await until(async () => (await tool()) === 'note', 20)
    step('right after its text closed, a letter picks a tool and leaves the text alone', noteTool && !(await editorOpen()) && (await storedOne(note))?.text === 'hello', `tool ${await tool()}, editor ${await editorOpen()}, stored ${JSON.stringify((await storedOne(note))?.text)}`)
    await page.keyboard.press('v')
    await page.mouse.click((await centre(shape)).x, (await centre(shape)).y)
    await until(async () => (await selection())[0] === shape)
    await page.keyboard.type('Hello')
    const shapeOpened = await until(editorOpen, 20)
    const shapeValue = shapeOpened ? await kit.getByTestId('board-text-editor').inputValue() : null
    await page.keyboard.press('Enter')
    await until(async () => (await storedOne(shape))?.text === 'Hello')
    step('a selected shape takes "Hello" and the tool stays Select', shapeValue === 'Hello' && (await storedOne(shape))?.text === 'Hello' && (await tool()) === 'select', `box ${JSON.stringify(shapeValue)}, stored ${JSON.stringify((await storedOne(shape))?.text)}, tool ${await tool()}`)
    await page.mouse.click((await centre(shape)).x, (await centre(shape)).y)
    await page.keyboard.press('2')
    const digit = await until(editorOpen, 20)
    const digitValue = digit ? await kit.getByTestId('board-text-editor').inputValue() : null
    await page.keyboard.press('Escape')
    await until(async () => !(await editorOpen()))
    step('a digit starts editing too, and Escape keeps the old text', digitValue === '2' && (await storedOne(shape))?.text === 'Hello', `box ${JSON.stringify(digitValue)}, stored ${JSON.stringify((await storedOne(shape))?.text)}`)
    await page.keyboard.press('r')
    step('a lower-case tool letter with a shape selected still picks its tool', await until(async () => (await tool()) === 'shape', 20) && !(await editorOpen()), `tool ${await tool()}`)
    await page.keyboard.press('Escape')
    await page.keyboard.press('Escape')
  }],
  // F5.7: a double-click on the empty canvas makes a text there.
  ['double-click-text', async () => {
    await fresh([])
    const at = await screen(400, 300)
    await page.mouse.dblclick(at.x, at.y)
    const opened = await until(editorOpen, 20)
    await until(async () => kit.getByTestId('board-text-editor').evaluate((field) => field.ownerDocument.activeElement === field), 20)
    await page.keyboard.type('Note')
    await page.keyboard.press('Enter')
    await until(async () => (await selection()).length === 1)
    const made = (await selection())[0]
    const element = made ? await storedOne(made) : null
    const box = made ? await boxOf(made) : null
    step('a double-click on the empty canvas makes a text where it was', opened && element?.kind === 'text' && element.text === 'Note' && box && near(box.x0, 400, 2) && box.y0 < 300 && box.y1 > 300, `${JSON.stringify(element)?.slice(0, 160)} ${fmt(box)}`)
  }],
  // F4.3: a click with a shape tool makes the default box centred on the pointer.
  ['click-create', async () => {
    await fresh([])
    await page.keyboard.press('r')
    const at = await screen(400, 400)
    await page.mouse.click(at.x, at.y)
    await until(async () => (await selection()).length === 1)
    const shape = (await selection())[0]
    const box = shape ? await boxOf(shape) : null
    step('a click with the shape tool centres the new shape on the pointer', box && near((box.x0 + box.x1) / 2, 400) && near((box.y0 + box.y1) / 2, 400), fmt(box))
    await page.keyboard.press('Escape')
    await page.keyboard.press('n')
    const noteAt = await screen(800, 400)
    await page.mouse.click(noteAt.x, noteAt.y)
    await until(editorOpen, 20)
    await page.keyboard.press('Escape')
    await until(async () => (await selection()).length === 1 && (await selection())[0] !== shape)
    const note = (await selection())[0]
    const noteBox = note ? await boxOf(note) : null
    step('a click with the sticky note tool centres the note on the pointer', noteBox && near((noteBox.x0 + noteBox.x1) / 2, 800) && near((noteBox.y0 + noteBox.y1) / 2, 400), fmt(noteBox))
    await page.keyboard.press('Escape')
  }],
  // F10.2: one Ctrl+wheel event zooms by 10% at most.
  ['zoom-wheel', async () => {
    await fresh([RECT])
    const at = await screen(400, 300)
    await page.mouse.move(at.x, at.y)
    const zoom = () => board(() => window.kitBoard.viewport().zoom)
    const wheel = async (dy) => {
      const before = await zoom()
      await page.keyboard.down('Control')
      await page.mouse.wheel(0, dy)
      await page.keyboard.up('Control')
      await until(async () => (await zoom()) !== before, 20)
      return (await zoom()) / before
    }
    const notch = await wheel(-100)
    const big = await wheel(-600)
    const back = await wheel(100)
    step('Ctrl+wheel zooms in by at most 10% per event', notch > 1 && notch <= 1.1 + 1e-9 && big > 1 && big <= 1.1 + 1e-9, `one notch × ${notch.toFixed(4)}, a large delta × ${big.toFixed(4)}`)
    step('and out by at most 10%', back < 1 && back >= 1 / 1.1 - 1e-9, `× ${back.toFixed(4)}`)
    const pointed = await board(([x, y]) => {
      const v = window.kitBoard.viewport()
      const box = document.querySelector('[data-testid="board-svg"]').getBoundingClientRect()
      return { x: v.x + (x - box.left) / v.zoom, y: v.y + (y - box.top) / v.zoom }
    }, [at.x - (await (await page.$('#kit')).boundingBox()).x, at.y - (await (await page.$('#kit')).boundingBox()).y])
    step('the board point under the pointer stays under it', near(pointed.x, 400, 0.5) && near(pointed.y, 300, 0.5), JSON.stringify(pointed))
  }],
  // F10.5: Shift+0 is 100%, Shift+1 fits everything, Shift+2 the selection.
  ['zoom-keys', async () => {
    const [a, b] = await fresh([RECT, { kind: 'shape', position: { x: 2600, y: 1800 }, size: { w: 160, h: 100 } }])
    await board(() => window.kitBoard.setViewport({ x: 0, y: 0, zoom: 2 }))
    await page.keyboard.press('Shift+Digit0')
    step('Shift+0 zooms to 100%', await until(async () => (await board(() => window.kitBoard.viewport().zoom)) === 1, 20), JSON.stringify(await board(() => window.kitBoard.viewport())))
    await page.keyboard.press('Shift+Digit1')
    await page.waitForTimeout(200)
    const v = await board(() => window.kitBoard.viewport())
    const inView = (r) => r.x0 >= v.x && r.y0 >= v.y && r.x1 <= v.x + v.width / v.zoom && r.y1 <= v.y + v.height / v.zoom
    step('Shift+1 fits everything in view', inView(await boxOf(a)) && inView(await boxOf(b)), JSON.stringify(v))
    // The first shape is at the view's top left, clear of the overview in its bottom right corner.
    const aAt = await centre(a)
    await page.mouse.click(aAt.x, aAt.y)
    await until(async () => (await selection())[0] === a)
    await page.keyboard.press('Shift+Digit2')
    await page.waitForTimeout(200)
    const v2 = await board(() => window.kitBoard.viewport())
    const mid = { x: v2.x + v2.width / v2.zoom / 2, y: v2.y + v2.height / v2.zoom / 2 }
    const atHundred = v2.zoom === 1 && near(mid.x, 280, 2) && near(mid.y, 250, 2)
    step('Shift+2 shows the selection at 100%, centred', atHundred, `${JSON.stringify(await selection())} ${JSON.stringify(v2)}`)
    await page.keyboard.press('Shift+Digit2')
    await page.waitForTimeout(200)
    const v3 = await board(() => window.kitBoard.viewport())
    step('Shift+2 again, already at 100%, fits the selection', atHundred && v3.zoom > 1, JSON.stringify(v3))
    await page.keyboard.press('Escape')
  }],
  // F2.8, rule 2: a margin of 4.5 px round a shape's outline; inside hollow shapes the smallest one;
  // locked elements are the empty canvas for a press (a click still selects one).
  ['hit-rules', async () => {
    const [filled, outer, inner, locked, other] = await fresh([
      RECT,
      { kind: 'shape', position: { x: 500, y: 150 }, size: { w: 300, h: 260 }, fillStyle: 'none' },
      { kind: 'shape', position: { x: 560, y: 200 }, size: { w: 120, h: 100 }, fillStyle: 'none' },
      { kind: 'shape', position: { x: 200, y: 450 }, size: { w: 200, h: 120 } },
      { kind: 'shape', position: { x: 260, y: 640 }, size: { w: 80, h: 60 } },
    ])
    await ai([{ command: 'element.lock', params: { ids: [locked], locked: true } }])
    await page.waitForTimeout(300)
    await clickAt(363, 215)
    const justOutside = await until(async () => same(await selection(), [filled]), 20)
    step('a press 3 px outside a filled shape\'s edge selects it', justOutside, JSON.stringify(await selection()))
    await page.keyboard.press('Escape')
    await clickAt(369, 215)
    await page.waitForTimeout(200)
    step('9 px outside it is the empty canvas', (await selection()).length === 0, JSON.stringify(await selection()))
    await clickAt(740, 370)
    const hollowHit = await until(async () => same(await selection(), [outer]), 20)
    step('a press inside a hollow shape, away from its line, selects it', hollowHit, JSON.stringify(await selection()))
    await clickAt(620, 250)
    const smallest = await until(async () => same(await selection(), [inner]), 20)
    step('inside two hollow shapes the smaller one is selected', smallest, JSON.stringify(await selection()))
    await page.keyboard.press('Escape')
    const lockedBefore = await positionOf(locked)
    await marquee({ x: 300, y: 510 }, { x: 420, y: 720 })
    await page.waitForTimeout(300)
    step('a drag from a locked shape draws a marquee: it selects what it touches, not the locked shape, which stays', same(await selection(), [other]) && JSON.stringify(await positionOf(locked)) === JSON.stringify(lockedBefore), `${JSON.stringify(await selection())} ${JSON.stringify(await positionOf(locked))}`)
    await clickAt(300, 510)
    step('a click on a locked shape still selects it', await until(async () => same(await selection(), [locked]), 20), JSON.stringify(await selection()))
    await page.keyboard.press('Escape')
    // The next scenario empties the page, which a locked shape would refuse.
    await ai([{ command: 'element.lock', params: { ids: [locked], locked: false } }])
  }],
  // F2.4, rule 4: a selection of several is one frame with the handles and a thin outline of each
  // member in its own shape; a marquee does not take mind-map branches.
  ['multi-look', async () => {
    const [a, b, n1, n2] = await fresh([
      RECT,
      { kind: 'shape', shapeType: 'ellipse', position: { x: 500, y: 200 }, size: { w: 160, h: 100 } },
      { kind: 'shape', shapeStyle: 'mind-node', mindMapDirection: 'right', position: { x: 200, y: 450 }, size: { w: 160, h: 60 } },
      { kind: 'shape', shapeStyle: 'mind-node', position: { x: 460, y: 450 }, size: { w: 160, h: 60 } },
    ])
    const made = await ai([
      { command: 'connector.add', params: { page: await pageNow(), from: { element: a }, to: { element: b } } },
      { command: 'connector.add', params: { page: await pageNow(), from: { element: n1 }, to: { element: n2 } } },
    ])
    const line = made.completed[0].result.created[0]
    const branch = made.completed[1].result.created[0]
    await until(async () => (await h.shown()).includes(line) && (await h.shown()).includes(branch))
    await marquee({ x: 150, y: 150 }, { x: 700, y: 350 })
    await until(async () => (await selection()).length === 3)
    await until(async () => (await kit.getByTestId('board-selection-member').count()) === 3, 20)
    const look = await board(() => {
      const box = document.querySelector('[data-testid="board-selection-box"]')
      const members = [...document.querySelectorAll('[data-testid="board-selection-member"]')].map((node) => ({ id: node.getAttribute('data-selected'), tag: node.tagName, d: node.getAttribute('d') ?? '' }))
      return {
        boxes: document.querySelectorAll('[data-testid="board-selection-box"]').length,
        box: box ? { x0: Number(box.getAttribute('x')), y0: Number(box.getAttribute('y')), x1: Number(box.getAttribute('x')) + Number(box.getAttribute('width')), y1: Number(box.getAttribute('y')) + Number(box.getAttribute('height')) } : null,
        frames: document.querySelectorAll('[data-testid="board-selection-frame"]').length,
        handles: document.querySelectorAll('[data-handle^="resize:"]').length,
        members,
      }
    })
    const frame = (await overlayNow())?.selection?.box ?? null
    const ellipse = look.members.find((member) => member.id === b)
    step(
      'several selected: one frame with eight handles, a thin outline per member, no box per element',
      same(await selection(), [a, b, line]) && look.boxes === 1 && sameBox(look.box, frame, 0.5) && look.handles === 8 && look.frames === 0 && look.members.length === 3,
      JSON.stringify({ ...look, frame }),
    )
    step('each member is outlined in its own shape (an ellipse by its ellipse, the connector along its line)', ellipse?.tag === 'path' && ellipse.d.includes('A') && look.members.find((member) => member.id === line)?.tag === 'path', JSON.stringify(look.members))
    await page.keyboard.press('Escape')
    await marquee({ x: 150, y: 420 }, { x: 700, y: 560 })
    await until(async () => (await selection()).length >= 2)
    step('a marquee over a mind map takes its nodes, not its branches', same(await selection(), [n1, n2]), JSON.stringify(await selection()))
    await page.keyboard.press('Escape')
  }],
  // F2.12, rule 6: Cmd/Ctrl encloses (also switched mid-drag), Shift adds, a frame only whole; a drag on
  // a frame's empty inside is a marquee.
  ['marquee-modes', async () => {
    const [a, b, c] = await fresh([RECT, { ...RECT, position: { x: 500, y: 200 } }, { kind: 'shape', position: { x: 260, y: 470 }, size: { w: 120, h: 80 } }])
    const framed = await ai([{ command: 'frame.add', params: { page: await pageNow(), box: [200, 420, 640, 640], title: 'Box', children: [c] } }])
    const frame = framed.completed[0].result.frame
    await until(async () => (await h.shown()).includes(frame))
    let touching = null
    const from = await screen(150, 150)
    const to = await screen(560, 330)
    await page.mouse.move(from.x, from.y)
    await page.mouse.down()
    for (let i = 1; i <= 8; i += 1) await page.mouse.move(from.x + ((to.x - from.x) * i) / 8, from.y + ((to.y - from.y) * i) / 8)
    await until(async () => (await selection()).length === 2, 20)
    touching = await selection()
    await page.keyboard.down(mod)
    await page.mouse.move(to.x + 1, to.y + 1)
    const enclosing = await until(async () => same(await selection(), [a]), 20)
    await page.mouse.up()
    await page.keyboard.up(mod)
    step('a marquee selects what it touches; with Cmd/Ctrl held (even mid-drag) only what it encloses', same(touching, [a, b]) && enclosing && same(await selection(), [a]), `${JSON.stringify(touching)} then ${JSON.stringify(await selection())}`)
    await page.keyboard.down('Shift')
    await marquee({ x: 480, y: 180 }, { x: 700, y: 330 })
    await page.keyboard.up('Shift')
    step('Shift adds the marquee to the selection', await until(async () => same(await selection(), [a, b]), 20), JSON.stringify(await selection()))
    await page.keyboard.press('Escape')
    const frameBefore = await boxOf(frame)
    await marquee({ x: 230, y: 450 }, { x: 420, y: 600 })
    await page.waitForTimeout(200)
    step('a drag on a frame\'s empty inside is a marquee: it takes the shape it touches, not the frame, which stays', same(await selection(), [c]) && sameBox(await boxOf(frame), frameBefore, 0.01), `${JSON.stringify(await selection())} ${fmt(await boxOf(frame))}`)
    await page.keyboard.press('Escape')
    await marquee({ x: 120, y: 380 }, { x: 700, y: 700 })
    step('a frame enclosed whole is selected alone (what it holds goes with it)', await until(async () => same(await selection(), [frame]), 20), JSON.stringify(await selection()))
    await page.keyboard.press('Escape')
  }],
  // F3.4: a marquee held at the canvas's edge scrolls the view.
  ['marquee-edge-scroll', async () => {
    await fresh([RECT])
    const svg = await h.svgBox()
    const start = await screen(700, 450)
    const before = await board(() => window.kitBoard.viewport())
    await page.mouse.move(start.x, start.y)
    await page.mouse.down()
    await page.mouse.move(svg.x + 200, start.y, { steps: 6 })
    await page.mouse.move(svg.x + 3, start.y, { steps: 3 })
    const scrolled = await until(async () => (await board(() => window.kitBoard.viewport().x)) < before.x - 40, 60)
    const gesture = await gestureNow()
    await page.mouse.up()
    step('a marquee held at the left edge scrolls the view left', scrolled && gesture?.type === 'brush', `${JSON.stringify(before)} → ${JSON.stringify(await board(() => window.kitBoard.viewport()))}`)
    await page.keyboard.press('Escape')
  }],
  // F3.6, rule 7: Shift keeps a move on one axis.
  ['move-axis', async () => {
    const [shape] = await fresh([RECT])
    const at = await centre(shape)
    await page.keyboard.down('Shift')
    await drag(at, { x: at.x + 150, y: at.y + 35 })
    await page.keyboard.up('Shift')
    await until(async () => ((await positionOf(shape))?.x ?? 0) > 300)
    const p = await positionOf(shape)
    step('Shift held, a move mostly across stays on its row', near(p?.x, 350, 1) && p?.y === 200, JSON.stringify(p))
    await page.keyboard.press('Escape')
  }],
  // Rule 7: snapping within 8 px while the pointer is slow; not when it is fast, not with Cmd/Ctrl.
  ['move-snap', async () => {
    const [a] = await fresh([RECT, { ...RECT, position: { x: 500, y: 450 } }])
    const at = await centre(a)
    /** Moves A by 293 across (7 px short of B's left edge): `slow` ends with slow steps, else one jump. */
    const move = async ({ slow, command }) => {
      await page.mouse.move(at.x, at.y)
      if (command) await page.keyboard.down(mod)
      await page.mouse.down()
      await page.mouse.move(at.x + 6, at.y)
      if (slow) {
        await page.mouse.move(at.x + 287, at.y, { steps: 6 })
        for (let x = 288; x <= 293; x += 1) {
          await page.waitForTimeout(40)
          await page.mouse.move(at.x + x, at.y)
        }
      } else await page.mouse.move(at.x + 293, at.y)
      const guides = await board(() => document.querySelectorAll('.board-guide').length)
      await page.mouse.up()
      if (command) await page.keyboard.up(mod)
      await until(async () => ((await positionOf(a))?.x ?? 0) > 400)
      const x = (await positionOf(a))?.x
      await page.keyboard.press(`${mod}+z`)
      await until(async () => (await positionOf(a))?.x === 200)
      return { x, guides }
    }
    const slow = await move({ slow: true })
    step('a slow move snaps within 8 px to the edge of the shape near it', slow.x === 500 && slow.guides > 0, JSON.stringify(slow))
    const fast = await move({ slow: false })
    step('a fast move does not snap', near(fast.x, 493, 0.5), JSON.stringify(fast))
    const command = await move({ slow: true, command: true })
    step('Cmd/Ctrl held, a slow move does not snap', near(command.x, 493, 0.5) && command.guides === 0, JSON.stringify(command))
    await page.keyboard.press('Escape')
  }],
  // F3.4, rule 7: a move held at the canvas's edge scrolls the view, the shape going along.
  ['move-edge-scroll', async () => {
    const [shape] = await fresh([RECT])
    const svg = await h.svgBox()
    const at = await centre(shape)
    const edgeX = svg.x + svg.width - 3
    await page.mouse.move(at.x, at.y)
    await page.mouse.down()
    await page.mouse.move(edgeX - 100, at.y, { steps: 8 })
    await page.mouse.move(edgeX, at.y, { steps: 2 })
    const scrolled = await until(async () => (await board(() => window.kitBoard.viewport().x)) > 60, 60)
    await page.mouse.up()
    const travel = edgeX - at.x
    await until(async () => ((await positionOf(shape))?.x ?? 0) > 200 + travel + 40)
    step('a shape held at the right edge scrolls the view and goes along with it', scrolled && (await positionOf(shape))?.x > 200 + travel + 40, `${JSON.stringify(await positionOf(shape))}, the pointer went ${travel}`)
    await board(() => window.kitBoard.setViewport({ x: 0, y: 0, zoom: 1 }))
    await page.keyboard.press('Escape')
  }],
  // Rule 7: a run of arrow nudges (a key held, repeating) is previewed and lands as one undo step.
  ['nudge-undo', async () => {
    const [shape] = await fresh([RECT])
    await clickAt(280, 250)
    await until(async () => same(await selection(), [shape]))
    for (let i = 0; i < 3; i += 1) await page.keyboard.down('ArrowRight')
    const during = { drawn: await drawnBox(shape), stored: await positionOf(shape) }
    for (let i = 0; i < 2; i += 1) await page.keyboard.down('ArrowRight')
    await page.keyboard.up('ArrowRight')
    const landed = await until(async () => (await positionOf(shape))?.x === 205)
    step('held, the arrow nudges the drawn shape before anything is written, and lands as one move', landed && during.drawn && near(during.drawn.x0 - 3, during.stored?.x ?? 0, 2) && during.stored?.x === 200, `${fmt(during.drawn)} stored ${JSON.stringify(during.stored)} → ${JSON.stringify(await positionOf(shape))}`)
    await page.keyboard.down('Shift')
    await page.keyboard.down('ArrowDown')
    await page.keyboard.down('ArrowDown')
    await page.keyboard.up('ArrowDown')
    await page.keyboard.up('Shift')
    await until(async () => (await positionOf(shape))?.y === 220)
    await page.keyboard.press(`${mod}+z`)
    const oneStep = await until(async () => (await positionOf(shape))?.y === 200)
    step('one undo takes back a whole run (Shift: 10 a step)', oneStep && (await positionOf(shape))?.x === 205, JSON.stringify(await positionOf(shape)))
    await page.keyboard.press(`${mod}+z`)
    step('and the run before it is the undo step before', await until(async () => (await positionOf(shape))?.x === 200), JSON.stringify(await positionOf(shape)))
    await page.keyboard.press('Escape')
  }],
  // Rule 8: a handle dragged past the opposite side flips the box over it, and its cursor with it.
  ['resize-flip', async () => {
    const [shape] = await fresh([RECT])
    await clickAt(280, 250)
    await until(async () => same(await selection(), [shape]))
    const corner = await testid('board-handle-south-east')
    const to = await screen(140, 340)
    let cursor = null
    await drag(corner, to, {
      mid: async () => {
        await page.mouse.move(to.x, to.y)
        cursor = await board(() => getComputedStyle(document.querySelector('[data-testid="board-canvas"]')).cursor)
      },
    })
    await until(async () => near((await boxOf(shape))?.x1, 200, 2))
    const box = await boxOf(shape)
    step('dragged past the opposite side, the box flips over it (its west side stays)', near(box?.x0, 140, 2) && near(box?.x1, 200, 2) && near(box?.y1, 340, 2), fmt(box))
    step('and the cursor turns with it', cursor === 'nesw-resize', `${cursor}`)
    await page.keyboard.press('Escape')
  }],
  // Rule 8: the hover outline of a connector follows its real route.
  ['hover-route', async () => {
    const [a, b] = await fresh([{ ...RECT, position: { x: 100, y: 200 } }, { ...RECT, position: { x: 600, y: 450 } }])
    const made = await ai([{ command: 'connector.add', params: { page: await pageNow(), from: { element: a, anchor: [1, 0.5] }, to: { element: b, anchor: [0, 0.5] }, type: 'elbow' } }])
    const elbow = made.completed[0].result.created[0]
    await until(async () => (await h.shown()).includes(elbow))
    const route = (await board((id) => window.kitBoard.routes({ connectors: [{ id }] }), elbow))[0]
    const corners = route?.points ?? []
    const mid = corners.length >= 3 ? { x: (corners[1].x + corners[2].x) / 2, y: (corners[1].y + corners[2].y) / 2 } : null
    const at = mid ? await screen(mid.x, mid.y) : null
    if (at) {
      await page.mouse.move(at.x - 30, at.y - 30)
      await page.mouse.move(at.x, at.y, { steps: 3 })
    }
    const shown = await until(async () => (await kit.getByTestId('board-hover-line').count()) === 1, 40)
    const d = shown ? await kit.getByTestId('board-hover-line').getAttribute('d') : null
    step('the hover outline of an elbow connector is its real route, not a straight line', shown && d === route?.path && corners.length >= 4, `${d} vs ${route?.path}`)
  }],
  // F4.2, F4.5, rule 9: while drawn, a shape is previewed as that shape; Shift draws it square.
  ['draw-preview', async () => {
    await fresh([])
    await kit.getByTestId('board-library-ellipse').click()
    await until(async () => (await tool()) === 'shape', 20)
    await kit.getByTestId('board-canvas').focus()
    let preview = null
    await drag(await screen(300, 300), await screen(500, 420), {
      mid: async () => {
        preview = await board(() => {
          const node = document.querySelector('[data-testid="board-draft-shape"]')
          return node ? { shape: node.getAttribute('data-shape'), ellipse: node.querySelectorAll('ellipse').length, rect: document.querySelectorAll('.board-overlay rect.board-draft').length } : null
        })
      },
    })
    await until(async () => (await selection()).length === 1)
    const made = await storedOne((await selection())[0])
    step('while an ellipse is drawn, the preview is an ellipse, not a rectangle', preview?.shape === 'ellipse' && preview.ellipse === 1 && preview.rect === 0 && made?.shapeType === 'ellipse', JSON.stringify(preview))
    await page.keyboard.press('Escape')
    await page.keyboard.press('r')
    await until(async () => (await tool()) === 'shape', 20)
    await page.keyboard.down('Shift')
    await drag(await screen(600, 300), await screen(760, 380))
    await page.keyboard.up('Shift')
    await until(async () => (await selection()).length === 1 && (await selection())[0] !== made?.ref)
    const square = await storedOne((await selection())[0])
    step('Shift held, the shape is drawn square', near(square?.size?.w, 160, 1) && near(square?.size?.h, 160, 1), JSON.stringify(square?.size))
    await page.keyboard.press('Escape')
  }],
  // F9.3, rule 14: a click selects the group, a double-click goes in and selects the member, the next
  // double-click edits it; Escape goes back up.
  ['group-drill', async () => {
    const [a, b] = await fresh([{ ...RECT, text: 'A' }, { ...RECT, position: { x: 450, y: 200 }, text: 'B' }])
    const grouped = await ai([{ command: 'arrange.group', params: { ids: [a, b] } }])
    const group = grouped.completed[0].result.group
    await until(async () => (await board((id) => window.kitBoard.parent(id), a)) === group)
    await clickAt(280, 250)
    const whole = await until(async () => same(await selection(), [group]), 20)
    const at = await screen(280, 250)
    await page.mouse.dblclick(at.x, at.y)
    const member = await until(async () => same(await selection(), [a]), 20)
    step('a click selects the group, a double-click goes in and selects the member', whole && member && !(await editorOpen()), JSON.stringify(await selection()))
    await page.keyboard.down('Alt')
    await drag(at, { x: at.x, y: at.y + 150 })
    await page.keyboard.up('Alt')
    await until(async () => (await positionOf(a))?.y === 350)
    step('the member moves on its own', (await positionOf(a))?.y === 350 && (await positionOf(b))?.y === 200, `${JSON.stringify(await positionOf(a))} ${JSON.stringify(await positionOf(b))}`)
    const moved = await screen(280, 400)
    await page.mouse.dblclick(moved.x, moved.y)
    const editing = await until(editorOpen, 20)
    const value = editing ? await kit.getByTestId('board-text-editor').inputValue() : null
    step('a double-click on the selected member opens its text', editing && value === 'A', JSON.stringify(value))
    await page.keyboard.press('Escape')
    await until(async () => !(await editorOpen()), 20)
    await page.keyboard.press('Escape')
    step('Escape goes back up to the group', await until(async () => same(await selection(), [group]), 20), JSON.stringify(await selection()))
    await page.keyboard.press('Escape')
    step('and Escape again clears the selection', await until(async () => (await selection()).length === 0, 20))
  }],
  // F9.1, F9.5, rule 14: a frame's title shows the move cursor and drags the frame; a double-click on
  // it renames the frame; an untitled frame says where its title goes and takes a name there.
  ['frame-title', async () => {
    const [c] = await fresh([{ kind: 'shape', position: { x: 260, y: 380 }, size: { w: 120, h: 80 } }])
    const framed = await ai([
      { command: 'frame.add', params: { page: await pageNow(), box: [200, 300, 600, 560], title: 'Plan', children: [c] } },
      { command: 'frame.add', params: { page: await pageNow(), box: [760, 300, 1060, 500] } },
    ])
    const frame = framed.completed[0].result.frame
    const untitled = framed.completed[1].result.frame
    await until(async () => (await h.shown()).includes(frame) && (await h.shown()).includes(untitled))
    const titleBox = await board((id) => {
      const text = document.querySelector(`[data-testid="board-elements"] [data-id="${id}"] text`)
      const b = text?.getBBox()
      return b ? { x0: b.x, y0: b.y, x1: b.x + b.width, y1: b.y + b.height } : null
    }, frame)
    const title = titleBox ? await screen(200 + (titleBox.x0 + titleBox.x1) / 2, 300 + (titleBox.y0 + titleBox.y1) / 2) : null
    await page.mouse.move(title.x - 40, title.y - 30)
    await page.mouse.move(title.x, title.y, { steps: 3 })
    const hovered = await until(async () => (await board(() => window.kitBoard.hover())) === frame, 20)
    step("the frame's title outlines the frame and shows the move cursor", hovered && (await cursorAt(title)) === 'move', `${await board(() => window.kitBoard.hover())} ${await cursorAt(title)}`)
    await page.keyboard.down('Alt')
    await drag(title, { x: title.x + 100, y: title.y + 40 })
    await page.keyboard.up('Alt')
    await until(async () => near((await boxOf(frame))?.x0, 300, 2))
    step('dragging the title moves the frame and what it holds', near((await boxOf(frame))?.x0, 300, 2) && near((await positionOf(c))?.x, 360, 0.5), `${fmt(await boxOf(frame))} ${JSON.stringify(await positionOf(c))}`)
    const titleNow = { x: title.x + 100, y: title.y + 40 }
    await page.mouse.dblclick(titleNow.x, titleNow.y)
    const renaming = await until(async () => (await editorOpen()) && (await kit.getByTestId('board-text-editor').inputValue()) === 'Plan', 20)
    step('a double-click on the title opens it to rename the frame', renaming)
    await page.keyboard.press('Escape')
    await until(async () => !(await editorOpen()), 20)
    await page.keyboard.press('Escape')
    // Where the placeholder is drawn, from its page rectangle in the editor's frame (Playwright's
    // boundingBox of an SVG text in a frame is off in WebKit).
    const drawnAt = await board((id) => {
      const node = document.querySelector(`[data-testid="board-frame-placeholder"][data-frame="${id}"]`)
      const r = node?.getBoundingClientRect()
      return r ? { x: r.left + r.width / 2, y: r.top + r.height / 2 } : null
    }, untitled)
    const frameAt = await (await page.$('#kit')).boundingBox()
    const placeholder = drawnAt ? { x: frameAt.x + drawnAt.x, y: frameAt.y + drawnAt.y } : null
    const above = await screen(760, 300)
    step('an untitled frame says where its title goes (above its top left corner)', placeholder != null && placeholder.y < above.y && placeholder.x > above.x && (await kit.getByTestId('board-frame-placeholder').count()) === 1, JSON.stringify(placeholder))
    // On the start of its title line (board units, whatever the font draws the placeholder with).
    const titleLine = await screen(800, 287)
    await page.mouse.move(titleLine.x - 20, titleLine.y - 20)
    await page.mouse.move(titleLine.x, titleLine.y, { steps: 3 })
    const hoverThere = await board(() => window.kitBoard.hover())
    await page.mouse.dblclick(titleLine.x, titleLine.y)
    const naming = await until(async () => (await editorOpen()) && (await kit.getByTestId('board-text-editor').inputValue()) === '', 40)
    const seen = { hover: hoverThere, editor: await editorOpen(), value: (await editorOpen()) ? await kit.getByTestId('board-text-editor').inputValue() : null, selection: await selection(), trace: await board(() => window.kitBoard.trace()) }
    if (naming) {
      await page.keyboard.type('Ideas')
      await page.keyboard.press('Enter')
    }
    step('a double-click there names it', naming && (await until(async () => (await storedOne(untitled))?.text === 'Ideas', 40)), `${JSON.stringify((await storedOne(untitled))?.text)} ${JSON.stringify(seen)} frame ${untitled}`)
    await page.keyboard.press('Escape')
  }],
])
