/**
 * Templates: the six starting structures of the engine's `template.insert` (a mind map, a flow
 * chart, a roadmap, a kanban board, a retrospective and a storyboard), placed in the middle of the
 * view, one undo step.
 *
 * Ported from Kition's `packages/board-editor/src/components/WhiteboardTemplateGallery.tsx` (the
 * gallery and its categories).
 */
import { CalendarRangeIcon as CalendarRange, ClipboardListIcon as ClipboardList, Columns3Icon as Columns3, GitBranchIcon as GitBranch, NetworkIcon as Network, PresentationIcon as Presentation } from '@kition/tessera/icons/react'
import { Button, Dialog, ICON_STROKE, useKitEditor } from '@kition/sdk/editor'
import { type IconComponent } from '@kition/tessera/icons/react'

export const TEMPLATES: ReadonlyArray<{ id: string; icon: IconComponent }> = [
  { id: 'mindmap', icon: Network },
  { id: 'flowchart', icon: GitBranch },
  { id: 'roadmap', icon: CalendarRange },
  { id: 'kanban', icon: Columns3 },
  { id: 'retrospective', icon: ClipboardList },
  { id: 'storyboard', icon: Presentation },
]

export function TemplateGallery({ onPick, onClose }: { onPick: (template: string) => void; onClose: () => void }) {
  const { t } = useKitEditor()
  return (
    <Dialog
      title={t('templates')}
      onClose={onClose}
      testId="board-templates"
      actions={
        <Button variant="ghost" onClick={onClose}>
          {t('cancel')}
        </Button>
      }
    >
      <p className="kui-muted board-templates-hint">{t('templatesHint')}</p>
      <div className="board-templates">
        {TEMPLATES.map(({ id, icon: Icon }) => (
          <button key={id} type="button" className="board-template" onClick={() => onPick(id)} data-testid={`board-template-${id}`}>
            <Icon size={20} strokeWidth={ICON_STROKE} aria-hidden="true" />
            <span className="board-template-name">{t(`template.${id}`)}</span>
            <span className="kui-muted">{t(`template.${id}.about`)}</span>
          </button>
        ))}
      </div>
    </Dialog>
  )
}
