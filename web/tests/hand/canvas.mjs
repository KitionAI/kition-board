// The canvas by hand (plan "Board usable by hand", stage 0): node tests/hand/canvas.mjs [chromium|webkit] [scenario ...]
// Real mouse and keyboard events only: the keys right after opening, after an agent's edit, after a
// panel button and a dialog (none of them clicks the canvas first); hover and cursors; the 4-pixel
// drag threshold and Escape during a drag; the selection overlay following a move, a resize and a
// rotation; typing into the selected element; a double-click on the empty canvas; a click-created
// shape centred on the pointer; Ctrl+wheel steps and the view keys (Shift+0, Shift+1, Shift+2).
// The cases under cases/board.editor.* and cases/regressions/issue-hand-* name these scenarios.
import { open } from './harness.mjs'

const which = process.argv[2] ?? 'chromium'
const h = await open(which)
const { page, kit, step, until, board, ai, screen, boxOf, centre, drawnBox, testid, cursorAt, selection, tool, editorOpen, storedOne, near, sameBox, fmt, fresh, drag } = h

const RECT = { kind: 'shape', position: { x: 200, y: 200 }, size: { w: 160, h: 100 } }
const NOTE = { kind: 'shape', shapeStyle: 'sticky', position: { x: 520, y: 200 }, size: { w: 180, h: 140 } }
const gestureNow = () => board(() => window.kitBoard.gesture())
const overlayNow = () => board(() => window.kitBoard.overlay())
const transformOf = (id) => board((id) => document.querySelector(`[data-testid="board-elements"] [data-id="${id}"]`)?.getAttribute('transform') ?? '', id)
const rotationIn = (transform) => Number(/rotate\(([-\d.e]+)/.exec(transform)?.[1] ?? 0)

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
])
