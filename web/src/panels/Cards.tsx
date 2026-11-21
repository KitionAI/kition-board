/**
 * Cards (design note 3): a link card's dialog, and the selected card's section in the side panel
 * (what it points at, a new target, its caption). A card's target is a workspace file the host
 * lets the person pick, stored relative to the board's folder, or a web address; the engine draws
 * the card.
 */
import { useState } from 'react'
import { FileSymlinkIcon as FileSymlink, Link2Icon as Link2, TypeIcon as Type } from '@kition/tessera/icons/react'
import { Button, Dialog, PanelSection, RibbonButton, useKitEditor } from '@kition/sdk/editor'

/** `path` (a workspace path) as seen from the folder of `from` (the board's workspace path). */
export function relativePath(from: string, path: string): string {
  const base = from.includes('/') ? from.slice(0, from.lastIndexOf('/')).split('/') : []
  const parts = path.split('/')
  let shared = 0
  while (shared < base.length && shared < parts.length - 1 && base[shared] === parts[shared]) shared += 1
  return [...base.slice(shared).map(() => '..'), ...parts.slice(shared)].join('/')
}

/** Asks for a web address and an optional title (a link card, or a link card's new target). */
export function LinkDialog({ initial, onDone, onClose }: { initial?: { url: string; title: string }; onDone: (url: string, title: string) => void; onClose: () => void }) {
  const { t } = useKitEditor()
  const [url, setUrl] = useState(initial?.url ?? 'https://')
  const [title, setTitle] = useState(initial?.title ?? '')
  const valid = /^(https?:\/\/[^\s/]+|mailto:\S+@\S+)/.test(url.trim())
  const done = () => {
    if (valid) onDone(url.trim(), title.trim())
  }
  return (
    <Dialog
      title={t('linkCard')}
      onClose={onClose}
      testId="board-link-dialog"
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('cancel')}
          </Button>
          <Button variant="primary" disabled={!valid} onClick={done} data-testid="board-link-done">
            {t('linkPlace')}
          </Button>
        </>
      }
    >
      <form
        className="board-form"
        onSubmit={(event) => {
          event.preventDefault()
          done()
        }}
      >
        <label>
          <span>{t('linkUrl')}</span>
          <input className="kui-field" autoFocus value={url} onChange={(event) => setUrl(event.target.value)} data-testid="board-link-url" />
        </label>
        <label>
          <span>{t('linkTitle')}</span>
          <input className="kui-field" value={title} onChange={(event) => setTitle(event.target.value)} data-testid="board-link-title" />
        </label>
      </form>
    </Dialog>
  )
}

export type CardState = { id: string; kind: string; path: string | null; url: string | null; title: string | null }

/** The selected card: its kind and target, a new target, and its caption. */
export function CardSection({ card, onRetarget, onCaption }: { card: CardState; onRetarget: () => void; onCaption: () => void }) {
  const { t } = useKitEditor()
  const link = card.kind === 'link'
  return (
    <PanelSection title={t('card')} testId="board-card-panel">
      <div className="board-card-target" data-testid="board-card-target">
        <span className="kui-muted">{t(`cardKind.${card.kind}`)}</span>
        <span className="board-card-path" title={link ? (card.url ?? '') : (card.path ?? '')}>
          {link ? card.url : card.path}
        </span>
      </div>
      <div className="board-mind-actions">
        <RibbonButton icon={link ? Link2 : FileSymlink} label={link ? t('cardEditLink') : t('cardRetarget')} onClick={onRetarget} data-testid="board-card-retarget" />
        <RibbonButton icon={Type} label={t('cardCaption')} onClick={onCaption} data-testid="board-card-caption" />
      </div>
    </PanelSection>
  )
}
