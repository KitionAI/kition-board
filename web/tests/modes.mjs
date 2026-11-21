// The board editor in each display mode (kit-editor's EditorFrame: Menus, Single line, Classic, and
// Focus), through the development host: node tests/modes.mjs [chromium|webkit]
// kition-sdk's mode smoke (`@kition/sdk/dev-host/mode-smoke`). Run it with no HTTP proxy set.
// Single line opens on Edit: its four rows, Undo and Redo disabled on a board just opened.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'
import { runModeSmoke } from '@kition/sdk/dev-host/mode-smoke'

const line = { tab: 'Edit', primary: ['board-cmd-element.duplicate', 'board-cmd-element.delete'], min: 2 }
await runModeSmoke({ kit: 'kit.board', line, config: new URL('../vite.config.ts', import.meta.url).pathname, browser: process.argv[2] === 'webkit' ? webkit : chromium, createServer })
