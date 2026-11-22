/**
 * The canvas's right-click menu (design note 4.3): on elements Copy, Cut, Paste, Duplicate | Bring
 * to front, Bring forward, Send backward, Send to back | Group, Ungroup | Lock or Unlock | Delete;
 * on the empty canvas Paste and Select all; in a read-only board Copy only. Each row is one engine
 * command on the elements the menu opened on (or a copy, which changes nothing), and a row that
 * cannot apply is disabled. kit-editor's `ContextMenu`, as the page tabs' menu.
 */
import { ContextMenu, MenuItem, MenuSeparator, type ContextMenuPoint, type Strings } from '@kition/sdk/editor'

export type CanvasAction = 'copy' | 'cut' | 'paste' | 'duplicate' | 'front' | 'forward' | 'backward' | 'back' | 'group' | 'ungroup' | 'lock' | 'unlock' | 'delete' | 'select-all'

/** Where the menu is and what its rows can do. */
export type CanvasMenuState = {
  at: ContextMenuPoint
  /** On elements (the selection, which the right-click made when it was on another element), or on the empty canvas. */
  on: 'elements' | 'canvas'
  readOnly: boolean
  /** A copy here, or a system clipboard the editor may read. */
  canPaste: boolean
  /** Some element, or something inside one, is locked: Cut, Delete and Group cannot apply. */
  locked: boolean
  /** Every element is locked: the row unlocks them. */
  allLocked: boolean
  /** Two or more elements. */
  several: boolean
  /** A group among them. */
  groups: boolean
}

export function CanvasMenu({ menu, t, onAction, onClose }: { menu: CanvasMenuState | null; t: Strings; onAction: (action: CanvasAction) => void; onClose: () => void }) {
  const row = (action: CanvasAction, label: string, disabled = false) => (
    <MenuItem key={action} label={t(label)} disabled={disabled} onSelect={() => onAction(action)} testId={`board-canvas-menu-${action}`} />
  )
  const rows = () => {
    if (!menu) return null
    if (menu.readOnly) return menu.on === 'elements' ? row('copy', 'menuCopy') : null
    if (menu.on === 'canvas') return [row('paste', 'menuPaste', !menu.canPaste), row('select-all', 'menuSelectAll')]
    return [
      row('copy', 'menuCopy'),
      row('cut', 'menuCut', menu.locked),
      row('paste', 'menuPaste', !menu.canPaste),
      row('duplicate', 'menuDuplicate'),
      <MenuSeparator key="before-order" />,
      row('front', 'menuFront'),
      row('forward', 'menuForward'),
      row('backward', 'menuBackward'),
      row('back', 'menuBack'),
      <MenuSeparator key="before-group" />,
      row('group', 'menuGroup', !menu.several || menu.locked),
      row('ungroup', 'menuUngroup', !menu.groups || menu.locked),
      <MenuSeparator key="before-lock" />,
      menu.allLocked ? row('unlock', 'menuUnlock') : row('lock', 'menuLock'),
      <MenuSeparator key="before-delete" />,
      row('delete', 'menuDelete', menu.locked),
    ]
  }
  return (
    <ContextMenu at={menu?.at ?? null} onClose={onClose} label={t('canvasMenu')} testId="board-canvas-menu">
      {rows()}
    </ContextMenu>
  )
}
