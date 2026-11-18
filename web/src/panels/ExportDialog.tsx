/**
 * Export: the shown page, or the selected frame, as PNG (at 1, 2 or 3 times its size, on white or
 * transparent), SVG or PDF, written next to the board as a new file through the host. The engine
 * draws the file from the scene the editor shows (`file.export`).
 *
 * Ported from Kition's `packages/board-editor/src/hooks/useWhiteboardExport.ts`
 * (`getWhiteboardExportFilename`, PNG and SVG) and `lib/boardFrameExport.ts` (one frame), with PDF
 * new and the file written through the host instead of a browser download.
 */
import { useState } from 'react'
import { Button, Checkbox, Choice, Dialog, toast, useKitEditor } from '@kition/kit-editor'

export type ExportFormat = 'png' | 'svg' | 'pdf'

/** A file name from the board's name (v1's `getWhiteboardExportFilename`). */
export function exportName(board: string, part: string | null, extension: string) {
  const stem = board.replace(/\.[^.]+$/, '')
  const name = (part ? `${stem} - ${part}` : stem).replace(/[\\/:*?"<>|]/g, '_').trim() || 'board'
  return `${name}.${extension}`
}

export function ExportDialog({ frame, onClose }: { frame: { id: string; title: string } | null; onClose: () => void }) {
  const { t, query, host } = useKitEditor()
  const [format, setFormat] = useState<ExportFormat>('png')
  const [scope, setScope] = useState<'page' | 'frame'>(frame ? 'frame' : 'page')
  const [scale, setScale] = useState('2')
  const [background, setBackground] = useState(true)
  const [busy, setBusy] = useState(false)
  const run = async () => {
    setBusy(true)
    try {
      const params: Record<string, unknown> = { format }
      const part = scope === 'frame' && frame ? frame : null
      if (part) params.frame = part.id
      if (format === 'png') {
        params.scale = Number(scale)
        params.fit = true
      }
      if (format !== 'pdf') params.background = background
      const answer = await query<{ base64: string; extension: string }>('file.export', params)
      if (!answer) throw new Error(t('exportFailed'))
      const bytes = Uint8Array.from(atob(answer.base64), (char) => char.charCodeAt(0))
      const path = host.init.document.path
      const folder = path.includes('/') ? path.slice(0, path.lastIndexOf('/') + 1) : ''
      const written = await host.exportFile(`${folder}${exportName(host.init.document.name || 'Board', part ? part.title || t('frame') : null, answer.extension)}`, bytes)
      toast('success', t('exported'), written)
      onClose()
    } catch (error) {
      toast('error', t('exportFailed'), error instanceof Error ? error.message : String(error))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Dialog
      title={t('export')}
      onClose={onClose}
      testId="board-export-dialog"
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('cancel')}
          </Button>
          <Button variant="primary" disabled={busy} onClick={() => void run()} data-testid="board-export-run">
            {t('exportRun')}
          </Button>
        </>
      }
    >
      <div className="board-form">
        <Choice label={t('exportFormat')} value={format} choices={[['png', 'PNG'], ['svg', 'SVG'], ['pdf', 'PDF']] as const} onChange={setFormat} testId="board-export-format" />
        {frame ? <Choice label={t('exportScope')} value={scope} choices={[['page', t('exportPage')], ['frame', frame.title || t('frame')]] as const} onChange={setScope} testId="board-export-scope" /> : null}
        {format === 'png' ? <Choice label={t('exportScale')} value={scale} choices={[['1', '1×'], ['2', '2×'], ['3', '3×']] as const} onChange={setScale} testId="board-export-scale" /> : null}
        {format !== 'pdf' ? <Checkbox label={t('exportBackground')} checked={background} onChange={setBackground} testId="board-export-background" /> : null}
      </div>
    </Dialog>
  )
}
