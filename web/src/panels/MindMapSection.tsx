/**
 * The selected mind-map node's controls in the side panel: where the map grows, a branch or a
 * sibling, and collapsing what is below. Each is one engine command.
 *
 * Ported from the mind-map part of Kition's `packages/board-editor/src/components/WhiteboardSelectionToolbar.tsx`
 * (the direction menu) and the actions of `hooks/useWhiteboardMindMapActions.ts`.
 */
import { ArrowDownIcon as ArrowDown, ArrowLeftIcon as ArrowLeft, ArrowLeftRightIcon as ArrowLeftRight, ArrowRightIcon as ArrowRight, ChevronsDownUpIcon as ChevronsDownUp, ChevronsUpDownIcon as ChevronsUpDown, GitBranchPlusIcon as GitBranchPlus, ListPlusIcon as ListPlus } from '@kition/tessera/icons/react'
import { PanelSection, RibbonButton, Segmented, useKitEditor } from '@kition/sdk/editor'
import type { Direction } from '../board/mindmap'

export type MindNodeState = { id: string; root: boolean; direction: Direction; collapsed: boolean; branches: number }

const DIRECTIONS: readonly Direction[] = ['both', 'right', 'left', 'down']
const ICONS = { both: ArrowLeftRight, right: ArrowRight, left: ArrowLeft, down: ArrowDown } as const

export function MindMapSection({ node, onGrow }: { node: MindNodeState; onGrow: (sibling: boolean) => void }) {
  const editor = useKitEditor()
  const { t } = editor
  return (
    <PanelSection title={t('mindMap')} testId="board-mind-panel">
      <Segmented
        label={t('mindDirection')}
        value={node.direction}
        segments={DIRECTIONS.map((direction) => ({ value: direction, icon: ICONS[direction], label: t(`mindGrow.${direction}`) }))}
        onChange={(direction) => void editor.run('mindmap.setDirection', { node: node.id, direction })}
        testId="board-mind-direction"
      />
      <div className="board-mind-actions">
        <RibbonButton icon={GitBranchPlus} label={t('mindAddChild')} onClick={() => onGrow(false)} data-testid="board-mind-panel-child" />
        {node.root ? null : <RibbonButton icon={ListPlus} label={t('mindAddSibling')} onClick={() => onGrow(true)} data-testid="board-mind-panel-sibling" />}
        {node.root || !node.branches ? null : (
          <RibbonButton
            icon={node.collapsed ? ChevronsUpDown : ChevronsDownUp}
            label={node.collapsed ? t('mindExpand', { count: node.branches }) : t('mindCollapse')}
            onClick={() => void editor.run('mindmap.collapse', { node: node.id })}
            data-testid="board-mind-panel-collapse"
          />
        )}
      </div>
    </PanelSection>
  )
}
