/**
 * The keyboard shortcuts at a glance (plan "Board usable by hand", stage 2; finding F11.6): the
 * header's Keyboard shortcuts button or the `?` key opens it. It lists the keys the canvas takes
 * (`canvas/BoardCanvas.tsx`, its `keyDown`) and the engine's own shortcuts the frame runs (Undo,
 * Redo, Duplicate, Group); Cmd on a Mac, Ctrl elsewhere, as kit-editor's shortcuts.
 */
import { Button, Dialog, isMac, useKitEditor } from '@kition/sdk/editor'

/** The rows of each group: the keys (each a key cap) and the string key of what they do. */
const GROUPS: ReadonlyArray<{ title: string; rows: ReadonlyArray<{ keys: readonly string[]; does: string }> }> = [
  {
    title: 'keysTools',
    rows: [
      { keys: ['V'], does: 'toolSelect' },
      { keys: ['H'], does: 'toolHand' },
      { keys: ['R'], does: 'keysShape' },
      { keys: ['N'], does: 'toolNote' },
      { keys: ['M'], does: 'toolMindMap' },
      { keys: ['T'], does: 'toolText' },
      { keys: ['A'], does: 'toolConnector' },
      { keys: ['P'], does: 'toolPen' },
      { keys: ['L'], does: 'toolHighlight' },
      { keys: ['E'], does: 'toolEraser' },
      { keys: ['F'], does: 'toolFrame' },
      { keys: ['Q'], does: 'toolLock' },
      { keys: ['Esc'], does: 'keysEscape' },
    ],
  },
  {
    title: 'keysEdit',
    rows: [
      { keys: ['Mod', 'Z'], does: 'keysUndo' },
      { keys: ['Mod', 'Shift', 'Z'], does: 'keysRedo' },
      { keys: ['Mod', 'C'], does: 'menuCopy' },
      { keys: ['Mod', 'X'], does: 'menuCut' },
      { keys: ['Mod', 'V'], does: 'menuPaste' },
      { keys: ['Mod', 'D'], does: 'menuDuplicate' },
      { keys: ['Delete'], does: 'menuDelete' },
      { keys: ['Mod', 'A'], does: 'menuSelectAll' },
      { keys: ['Mod', 'G'], does: 'menuGroup' },
      { keys: ['Mod', 'Shift', 'G'], does: 'menuUngroup' },
      { keys: ['←', '↑', '→', '↓'], does: 'keysNudge' },
      { keys: ['Shift', '←'], does: 'keysNudgeFar' },
    ],
  },
  {
    title: 'keysText',
    rows: [
      { keys: ['Enter'], does: 'keysEditText' },
      { keys: ['Shift', 'Enter'], does: 'keysNewLine' },
      { keys: ['Esc'], does: 'keysDiscard' },
      { keys: ['Mod', 'B'], does: 'markBold' },
      { keys: ['Mod', 'I'], does: 'markItalic' },
      { keys: ['Mod', 'K'], does: 'markLink' },
      { keys: ['Tab'], does: 'mindAddChild' },
      { keys: ['Enter'], does: 'mindAddSibling' },
    ],
  },
  {
    title: 'keysView',
    rows: [
      { keys: ['Space'], does: 'keysPan' },
      { keys: ['Mod', 'Wheel'], does: 'keysZoom' },
      { keys: ['Shift', '0'], does: 'keysZoom100' },
      { keys: ['Shift', '1'], does: 'keysFitAll' },
      { keys: ['Shift', '2'], does: 'keysFitSelection' },
      { keys: ['?'], does: 'shortcuts' },
    ],
  },
]

export function Shortcuts({ onClose }: { onClose: () => void }) {
  const { t } = useKitEditor()
  const mac = isMac()
  const cap = (key: string) => (key === 'Mod' ? (mac ? '⌘' : 'Ctrl') : key === 'Shift' && mac ? '⇧' : key === 'Wheel' ? t('keysWheel') : key)
  return (
    <Dialog
      title={t('shortcuts')}
      onClose={onClose}
      testId="board-shortcuts"
      actions={
        <Button variant="ghost" onClick={onClose}>
          {t('close')}
        </Button>
      }
    >
      <div className="board-shortcuts">
        {GROUPS.map((group) => (
          <section key={group.title} className="board-shortcuts-group" aria-label={t(group.title)}>
            <h3 className="board-shortcuts-title">{t(group.title)}</h3>
            <dl>
              {group.rows.map((row) => (
                <div key={`${row.keys.join('+')}-${row.does}`} className="board-shortcuts-row">
                  <dt>
                    {row.keys.map((key) => (
                      <kbd key={key}>{cap(key)}</kbd>
                    ))}
                  </dt>
                  <dd>{t(row.does)}</dd>
                </div>
              ))}
            </dl>
          </section>
        ))}
      </div>
    </Dialog>
  )
}
