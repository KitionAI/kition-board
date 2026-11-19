/**
 * The board's pages under the canvas (task board-page-switcher): Add page, then one tab per page in
 * the board's order, the shown page marked. The design notes name no whiteboard the pages follow, so
 * the bar is the kit family's own part tabs (kition-sheet's sheet tabs, kit-editor's `FrameChrome`).
 *
 * A click or Enter shows a page (`page.activate`: editor state, not an edit). The arrows, Home and
 * End move between the tabs; F2 or a double-click renames; the context menu (right-click, the shown
 * tab's arrow, Shift+F10 or the menu key) renames, duplicates (and shows the copy), moves left or
 * right, and deletes (not the board's last page, the engine's rule). Escape gives the keys back to
 * the canvas. Every change is one engine command and one undo step; the engine decides which page
 * is shown after an undo or when the shown page goes. A viewer only switches pages.
 */
import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react'
import { ChevronDownIcon as ChevronDown, PlusIcon as Plus } from '@kition/tessera-icons/react'
import { ContextMenu, IconButton, MenuItem, MenuSeparator, useKitEditor, useReadOnly, type ContextMenuPoint } from '@kition/kit-editor'

export type PageTab = { id: string; name: string }

/** The most characters of a page name (the engine's rule, `crates/engine/model/src/pages.rs`). */
const MAX_NAME = 200

export function PageTabs({ pages, shown, onDone }: { pages: readonly PageTab[]; shown: string | null; onDone: () => void }) {
  const { run, t } = useKitEditor()
  const readOnly = useReadOnly()
  const current = shown ?? pages[0]?.id ?? null
  const [focused, setFocused] = useState<string | null>(current)
  const [renaming, setRenaming] = useState<string | null>(null)
  const [name, setName] = useState('')
  const [menu, setMenu] = useState<{ page: string; at: ContextMenuPoint } | null>(null)
  const tabs = useRef(new Map<string, HTMLDivElement>())
  const strip = useRef<HTMLDivElement>(null)

  // The roving focus follows the shown page when it changes (a switch, a new page, an undo).
  useEffect(() => setFocused(current), [current])
  const order = pages.map((page) => page.id).join(' ')
  // The shown tab stays in view.
  useEffect(() => {
    if (current) tabs.current.get(current)?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
  }, [current, order])
  const tabStop = pages.some((page) => page.id === focused) ? focused : current

  const show = (page: string) => {
    if (page !== current) void run('page.activate', { page })
  }
  const focusTab = (page: string | undefined) => {
    if (!page) return
    setFocused(page)
    tabs.current.get(page)?.focus()
  }
  const startRename = (page: string) => {
    if (readOnly) return
    setMenu(null)
    setName(pages.find((listed) => listed.id === page)?.name ?? '')
    setRenaming(page)
  }
  /** Ends a rename: `keep` sends a changed, non-blank name; either way the keys go back to the tab. */
  const endRename = (keep: boolean) => {
    const page = renaming
    if (!page) return
    setRenaming(null)
    const to = name.trim()
    const from = pages.find((listed) => listed.id === page)?.name
    if (keep && to && to !== from) void run('page.rename', { page, name: to.slice(0, MAX_NAME) })
    window.requestAnimationFrame(() => tabs.current.get(page)?.focus())
  }
  const duplicate = async (page: string) => {
    const answer = await run('page.duplicate', { page })
    const copy = answer ? (JSON.parse(answer) as { page?: unknown }).page : null
    if (typeof copy === 'string') await run('page.activate', { page: copy })
  }
  const openMenu = (page: string, at: ContextMenuPoint) => {
    if (readOnly) return
    setMenu({ page, at })
  }
  const menuAt = (page: string) => {
    const box = tabs.current.get(page)?.getBoundingClientRect()
    return box ? { x: box.left, y: box.top } : { x: 0, y: 0 }
  }

  const onKey = (page: string, event: ReactKeyboardEvent<HTMLDivElement>) => {
    // Undo, Redo, Save and the other shortcuts go on to the editor; the canvas's own keys (tool
    // letters, Delete) do not act on a page from here.
    if (event.metaKey || event.ctrlKey) return
    const at = pages.findIndex((listed) => listed.id === page)
    const handled = (() => {
      switch (event.key) {
        case 'ArrowLeft':
          focusTab(pages[(at - 1 + pages.length) % pages.length]?.id)
          return true
        case 'ArrowRight':
          focusTab(pages[(at + 1) % pages.length]?.id)
          return true
        case 'Home':
          focusTab(pages[0]?.id)
          return true
        case 'End':
          focusTab(pages.at(-1)?.id)
          return true
        case 'Enter':
        case ' ':
          show(page)
          return true
        case 'F2':
          startRename(page)
          return true
        case 'ContextMenu':
          openMenu(page, menuAt(page))
          return true
        case 'F10':
          if (!event.shiftKey) return false
          openMenu(page, menuAt(page))
          return true
        case 'Escape':
          onDone()
          return true
        default:
          return false
      }
    })()
    event.stopPropagation()
    if (handled) event.preventDefault()
  }

  const menuPage = menu ? pages.find((page) => page.id === menu.page) ?? null : null
  const menuIndex = menuPage ? pages.indexOf(menuPage) : -1
  const close = () => setMenu(null)

  return (
    <div className="board-pages" data-testid="board-pages">
      <IconButton
        icon={Plus}
        label={t('addPage')}
        disabled={readOnly}
        onClick={() => void run('page.create', { activate: true })}
        data-testid="board-page-add"
      />
      <div ref={strip} className="board-page-strip" role="tablist" aria-label={t('pages')} aria-orientation="horizontal" data-testid="board-page-tabs">
        {pages.map((page) => {
          const active = page.id === current
          return (
            <div
              key={page.id}
              ref={(element) => {
                if (element) tabs.current.set(page.id, element)
                else tabs.current.delete(page.id)
              }}
              role="tab"
              className="board-page-tab"
              aria-selected={active}
              tabIndex={page.id === tabStop ? 0 : -1}
              title={page.name}
              data-page={page.id}
              data-testid={`board-page-tab-${page.id}`}
              onFocus={() => setFocused(page.id)}
              onClick={() => {
                if (renaming === page.id) return
                if (page.id !== current) {
                  show(page.id)
                  onDone()
                }
              }}
              onDoubleClick={() => startRename(page.id)}
              onContextMenu={(event) => {
                event.preventDefault()
                openMenu(page.id, { x: event.clientX, y: event.clientY })
              }}
              onKeyDown={(event) => {
                if (renaming !== page.id) onKey(page.id, event)
              }}
            >
              {renaming === page.id ? (
                <input
                  className="kui-field board-page-rename"
                  autoFocus
                  aria-label={t('pageName')}
                  maxLength={MAX_NAME}
                  value={name}
                  size={Math.max(4, Math.min(30, name.length + 1))}
                  onFocus={(event) => event.currentTarget.select()}
                  onChange={(event) => setName(event.target.value)}
                  onBlur={() => endRename(true)}
                  onClick={(event) => event.stopPropagation()}
                  onDoubleClick={(event) => event.stopPropagation()}
                  onKeyDown={(event) => {
                    // The field's keys are its own, not the canvas's or the tabs'.
                    if (event.metaKey || event.ctrlKey) {
                      if (event.key.toLowerCase() === 'z' || event.key.toLowerCase() === 'y') event.stopPropagation()
                      return
                    }
                    event.stopPropagation()
                    if (event.key === 'Enter') {
                      event.preventDefault()
                      endRename(true)
                    } else if (event.key === 'Escape') {
                      event.preventDefault()
                      endRename(false)
                    }
                  }}
                  data-testid="board-page-rename"
                />
              ) : (
                <span className="board-page-name">{page.name}</span>
              )}
              {active && !readOnly && renaming !== page.id ? (
                <button
                  type="button"
                  className="board-page-arrow"
                  tabIndex={-1}
                  aria-label={t('pageMenu')}
                  title={t('pageMenu')}
                  onClick={(event) => {
                    event.stopPropagation()
                    const box = event.currentTarget.getBoundingClientRect()
                    openMenu(page.id, { x: box.left, y: box.top })
                  }}
                  data-testid="board-page-menu-arrow"
                >
                  <ChevronDown size={12} aria-hidden="true" />
                </button>
              ) : null}
            </div>
          )
        })}
      </div>
      <ContextMenu at={menu?.at ?? null} onClose={close} label={t('pageMenu')} testId="board-page-menu">
        {menuPage ? (
          <>
            <MenuItem label={t('renamePage')} shortcut="F2" onSelect={() => startRename(menuPage.id)} testId="board-page-menu-rename" />
            <MenuItem label={t('duplicatePage')} onSelect={() => void duplicate(menuPage.id)} testId="board-page-menu-duplicate" />
            <MenuItem label={t('movePageLeft')} disabled={menuIndex <= 0} onSelect={() => void run('page.move', { page: menuPage.id, placement: 'previous' })} testId="board-page-menu-left" />
            <MenuItem
              label={t('movePageRight')}
              disabled={menuIndex >= pages.length - 1}
              onSelect={() => void run('page.move', { page: menuPage.id, placement: 'next' })}
              testId="board-page-menu-right"
            />
            <MenuSeparator />
            {/* A board keeps at least one page (the engine's rule): the last page's Delete is off. */}
            <MenuItem label={pages.length > 1 ? t('deletePage') : t('deleteLastPage')} disabled={pages.length <= 1} onSelect={() => void run('page.delete', { page: menuPage.id })} testId="board-page-menu-delete" />
          </>
        ) : null}
      </ContextMenu>
    </div>
  )
}
