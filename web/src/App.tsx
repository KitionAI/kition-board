import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  EditorFrame,
  FrameChrome,
  IconButton,
  KitEditorProvider,
  KitElement,
  Panel,
  ToolRail,
  commandMenuTabs,
  createKitStrings,
  toast,
  useKitEditor,
  useKitEditorState,
  useKitSession,
  useUi,
  type HostConnection,
  type KitCanvasHandle,
  type KitCommand,
  type KitEngineInfo,
  type Phrases,
  type RailTool,
} from '@kition/sdk/editor'
import { BoardCanvas, type BoardCanvasApi } from './canvas/BoardCanvas'
import type { DocumentArea } from './canvas/Minimap'
import { useColorScheme } from './colorScheme'
import { BoardSync, type BoardEngine } from './board/sync'
import { pictureMime, pictureParams } from './board/clipboard'
import { engineManifest } from './engine'
import { CATALOGS, summary } from './strings'
import type { Style } from './board/style'
import { graphOf } from './board/mindmap'
import { DownloadIcon as Download, LayoutTemplateIcon as LayoutTemplate } from '@kition/tessera/icons/react'
import { CardSection, LinkDialog, relativePath, type CardState } from './panels/Cards'
import { ExportDialog } from './panels/ExportDialog'
import { PageTabs, type PageTab } from './panels/PageTabs'
import { TemplateGallery } from './panels/TemplateGallery'
import { MindMapSection, type MindNodeState } from './panels/MindMapSection'
import { ShapeLibrary } from './panels/ShapeLibrary'
import { StylePanel, type StyleTarget } from './panels/StylePanel'
import { HIGHLIGHT_STYLE, PICTURE_EXTENSIONS, TOOLS, railId, type Tool } from './tools'

/** What the editor reads from the engine's `inspect()`. */
type BoardState = { elements: number; pages: PageTab[]; page: string | null; area: DocumentArea | null }

function describe(info: KitEngineInfo): BoardState {
  const inspected = info.inspected as { elements?: number; pages?: { id?: unknown; name?: unknown }[]; page?: string | null; area?: DocumentArea }
  const pages = Array.isArray(inspected.pages) ? inspected.pages.flatMap((page) => (typeof page.id === 'string' ? [{ id: page.id, name: typeof page.name === 'string' ? page.name : '' }] : [])) : []
  return { elements: inspected.elements ?? 0, pages, page: inspected.page ?? null, area: inspected.area ?? null }
}

const MENU_ORDER = ['Edit', 'Insert', 'Arrange', 'View']
const VIEW_MENUS = ['View']
/** The tab the editor opens on (in Single line and Classic). */
const DEFAULT_TAB = 'Edit'
/**
 * The rows each menu's tab shows in Single line's row; the rest wait in its group's popover. Insert
 * names none: its rows are the rail's tools (frame, connector, card, picture), which stay on the rail.
 */
const PRIMARY_ROWS: Readonly<Record<string, readonly string[]>> = {
  Edit: ['edit.undo', 'edit.redo', 'element.duplicate', 'element.delete'],
  Insert: [],
  Arrange: ['element.group', 'element.ungroup', 'element.reorder', 'element.lock', 'element.rotate', 'frame.fit'],
  View: ['page.create', 'page.rename', 'page.duplicate', 'page.delete', 'page.move'],
}

const message = (error: unknown) => (error instanceof Error ? error.message : String(error))

/** Whether `command` needs nothing but the selected elements' ids. */
function idsOnly(command: KitCommand | undefined) {
  const schema = command?.schema as Record<string, { required?: boolean }> | null | undefined
  if (!schema || !schema.ids) return false
  return Object.entries(schema).every(([name, field]) => name === 'ids' || !field.required)
}

/** `value`, or the earlier one while it is equal (so memos that depend on it hold). */
function useStable<T>(value: T): T {
  const kept = useRef(value)
  if (JSON.stringify(kept.current) !== JSON.stringify(value)) kept.current = value
  return kept.current
}

/**
 * The board editor: kit-editor's frame (menus from the engine's commands, the tool rail, the status
 * bar) around the board canvas, which draws the engine's SVG fragments over the page replica and
 * sends each gesture as one engine command (R2c).
 */
export function App({ host, phrase }: { host: HostConnection; phrase?: Phrases }) {
  const t = useMemo(() => createKitStrings(CATALOGS, host.init.locale), [host])
  const manifest = useMemo(engineManifest, [])
  const session = useKitSession(host, manifest, describe)
  const [selection, setSelection] = useState<string[]>([])
  const selectionRef = useRef(selection)
  selectionRef.current = selection
  const defaults = useCallback((command: KitCommand) => {
    const schema = (command.schema ?? {}) as Record<string, unknown>
    const out: Record<string, unknown> = {}
    if ('ids' in schema && selectionRef.current.length) out.ids = selectionRef.current
    if ('id' in schema && selectionRef.current.length === 1) out.id = selectionRef.current[0]
    return out
  }, [])
  const editor = useKitEditorState(session, host, t, 'board', phrase, defaults)
  return (
    <KitEditorProvider editor={editor}>
      <BoardEditor session={session} selection={selection} onSelection={setSelection} selectionRef={selectionRef} phrase={phrase} />
    </KitEditorProvider>
  )
}

type Session = ReturnType<typeof useKitSession<BoardState>>

function BoardEditor({ session, selection, onSelection, selectionRef, phrase }: { session: Session; selection: string[]; onSelection: (ids: string[]) => void; selectionRef: { current: string[] }; phrase?: Phrases }) {
  const editor = useKitEditor()
  const ui = useUi()
  const { host, t } = editor
  const { doc, state, change } = session
  const [sync, setSync] = useState<BoardSync | null>(null)
  const [tool, setTool] = useState<Tool>('select')
  const [shapeType, setShapeType] = useState('rectangle')
  const [zoom, setZoom] = useState(1)
  // What the drawing tools give new elements: only the fields the person chose (the rest are the kind's defaults).
  const [drawStyle, setDrawStyle] = useState<Style>({})
  const [highlightStyle, setHighlightStyle] = useState<Style>({ ...HIGHLIGHT_STYLE })
  const canvas = useRef<KitCanvasHandle>(null)
  const canvasApi = useRef<BoardCanvasApi>(null)
  const latest = useRef(session)
  latest.current = session
  const readOnly = host.readOnly === true

  // The page replica, loaded once the engine opened the board.
  useEffect(() => {
    if (!doc) return undefined
    let current = true
    let opened: BoardSync | null = null
    const engine: BoardEngine = {
      snapshot: () => doc.invoke<Uint8Array>('snapshot'),
      exportChanges: (since) => latest.current.replica.exportChanges(since),
      elementSvg: (request) => doc.invoke<string>('elementSvg', request),
      blob: (hash) => doc.invoke<Uint8Array>('blob', hash),
      textMetrics: (request) => doc.invoke<string>('textMetrics', request),
      shapePreviews: async () => JSON.parse(await doc.execute('shape.previews', {})) as unknown,
    }
    void BoardSync.open(engine)
      .then((made) => {
        if (!current) return made.close()
        opened = made
        setSync(made)
      })
      .catch((error: unknown) => toast('error', t('openFailed'), message(error)))
    return () => {
      current = false
      opened?.close()
      setSync(null)
    }
  }, [doc, t])

  // Every change of the engine's board (a command, an agent, undo, a collaborator) reaches the page.
  useEffect(() => {
    if (sync) void sync.pull().catch((error: unknown) => toast('error', t('syncFailed'), message(error)))
  }, [change.revision, sync, t])

  const { commands, byId } = editor
  // Menu rows that work on the selection run on it; New page shows the page it makes (the editor
  // has no other way to reach it); the rest open their dialog.
  const onCommand = useCallback(
    (id: string) => {
      const ids = selectionRef.current
      if (id === 'page.create') void editor.run(id, { activate: true })
      else if (ids.length && idsOnly(byId.get(id))) void editor.run(id, { ids })
      else void editor.run(id)
    },
    [byId, editor, selectionRef],
  )
  const onCommandRef = useRef(onCommand)
  onCommandRef.current = onCommand
  const stableCommand = useCallback((id: string) => onCommandRef.current(id), [])
  const tabs = useMemo(() => commandMenuTabs(commands, { order: MENU_ORDER, phrase, view: VIEW_MENUS, primary: PRIMARY_ROWS, onCommand: stableCommand }), [commands, phrase, stableCommand])

  const run = useCallback((command: string, params: Record<string, unknown>) => latest.current.run(command, params), [])

  /** Where something placed without a gesture goes: its top-left for a `w` × `h` box in the middle of the view. */
  const middle = useCallback((w: number, h: number) => {
    const view = canvas.current?.view()
    return view ? [view.x + view.width / view.zoom / 2 - w / 2, view.y + view.height / view.zoom / 2 - h / 2] : undefined
  }, [])

  /** Runs a command that makes elements, then selects them once they are on the page. */
  const place = useCallback(
    async (command: string, params: Record<string, unknown>, roots = false) => {
      const answer = await editor.run(command, params)
      const created = answer ? ((JSON.parse(answer) as { created?: string[] }).created ?? []) : []
      await sync?.pull(created)
      // A template selects its outermost elements, not everything inside them.
      const made = new Set(created)
      const picked = roots && sync ? created.filter((id) => !made.has(sync.view.elements.get(id)?.parent ?? '')) : created
      if (picked.length) onSelection(picked)
      // The keys (Delete, Undo, the tool letters) go to the canvas again.
      canvasApi.current?.focus()
      return created
    },
    [editor, onSelection, sync],
  )

  // Cards: a workspace file the person picks (stored relative to the board's folder), or a link.
  const [linkDialog, setLinkDialog] = useState<{ id: string | null; url: string; title: string } | null>(null)
  const pickTarget = useCallback(async () => {
    const path = await host.pick(t('pickCard'), [])
    return path ? relativePath(host.init.document.path, path) : null
  }, [host, t])
  const insertCard = useCallback(async () => {
    try {
      const file = await pickTarget()
      if (!file) return
      const position = middle(320, 200)
      await place('card.create', { file, ...(position ? { position } : {}) })
    } catch (error) {
      toast('error', t('cardFailed'), message(error))
    }
  }, [middle, pickTarget, place, t])
  const placeLink = useCallback(
    (url: string, title: string) => {
      const dialog = linkDialog
      setLinkDialog(null)
      if (!dialog) return
      if (dialog.id) void editor.run('card.setTarget', { id: dialog.id, url, ...(title ? { title } : {}) })
      else {
        const position = middle(320, 120)
        void place('card.create', { url, size: [320, 120], ...(title ? { title } : {}), ...(position ? { position } : {}) })
      }
    },
    [editor, linkDialog, middle, place],
  )

  /**
   * Picture: the person picks a workspace picture; its bytes go into the board in the middle of the
   * view, at its natural size (800 at most on its longer side), as a pasted or dropped one does.
   */
  const insertPicture = useCallback(async () => {
    try {
      const path = await host.pick(t('pickPicture'), PICTURE_EXTENSIONS)
      if (!path) return
      const { bytes } = await host.files.read(path)
      const name = path.slice(path.lastIndexOf('/') + 1)
      const mime = pictureMime({ type: '', name }) ?? 'application/octet-stream'
      const view = canvas.current?.view()
      const centre = view ? { x: view.x + view.width / view.zoom / 2, y: view.y + view.height / view.zoom / 2 } : null
      const answer = await latest.current.run('image.insert', await pictureParams(new Uint8Array(bytes), mime, name, centre))
      const created = answer ? ((JSON.parse(answer) as { created?: string[] }).created ?? []) : []
      await sync?.pull(created)
      if (created.length) onSelection(created)
    } catch (error) {
      toast('error', t('pictureFailed'), message(error))
    }
  }, [host, onSelection, sync, t])

  const tools = useMemo<RailTool[]>(() => TOOLS.map((item) => ({ id: item.id, label: t(item.label), icon: item.icon, shortcut: item.shortcut, group: item.group })), [t])
  const activeTool = railId(tool, shapeType)
  /** A tool the host allows: from the rail, a letter, or the canvas after a gesture. */
  const chooseTool = useCallback(
    (next: Tool) => {
      const id = railId(next, shapeType)
      if (ui.state(`tool:${id}`).denied) return
      setTool(next)
    },
    [shapeType, ui],
  )
  const selectRail = useCallback(
    (id: string) => {
      if (id === 'image') {
        void insertPicture()
        return
      }
      if (id === 'card') {
        void insertCard()
        return
      }
      if (id === 'link') {
        setLinkDialog({ id: null, url: 'https://', title: '' })
        return
      }
      if (id.startsWith('shape:')) {
        setShapeType(id.slice('shape:'.length))
        setTool('shape')
      } else setTool(id as Tool)
    },
    [insertCard, insertPicture],
  )
  /** A shape from the library panel: the shape tool draws it next; the keys go to the canvas again. */
  const pickShape = useCallback(
    (shape: string) => {
      if (ui.state(`tool:${railId('shape', shape)}`).denied) return
      setShapeType(shape)
      setTool('shape')
      canvasApi.current?.focus()
    },
    [ui],
  )
  const shown = TOOLS.find((item) => item.id === activeTool)
  // The Focus bar names the shape itself, also one from the library that the rail's slot stands for.
  const focusTool = useMemo(() => (shown ? { label: tool === 'shape' ? t(`shape.${shapeType}`) : t(shown.label), icon: shown.icon } : null), [shapeType, shown, t, tool])

  // The style panel styles the selection, or what the active drawing tool makes next.
  const selectionKey = selection.join(',')
  const styleTarget = useMemo<StyleTarget | null>(() => {
    if (tool === 'select') return selectionKey ? { ids: selectionKey.split(',') } : null
    switch (tool) {
      case 'shape':
        return { kind: 'shape', defaults: drawStyle }
      case 'note':
        return { kind: 'shape', shapeStyle: 'sticky', defaults: drawStyle }
      case 'text':
        return { kind: 'text', defaults: drawStyle }
      case 'connector':
        return { kind: 'connector', defaults: drawStyle }
      case 'pen':
        return { kind: 'stroke', defaults: drawStyle }
      case 'highlight':
        return { kind: 'stroke', defaults: highlightStyle }
      default:
        return null
    }
  }, [drawStyle, highlightStyle, selectionKey, tool])
  const applyStyle = useCallback(
    (style: Style) => {
      if (styleTarget && 'ids' in styleTarget) void editor.run('element.style', { ids: [...styleTarget.ids], style })
      else if (tool === 'highlight') setHighlightStyle((now) => ({ ...now, ...style }))
      else setDrawStyle((now) => ({ ...now, ...style }))
    },
    [editor, styleTarget, tool],
  )

  // A single selected mind-map node gets its map's controls in the panel.
  const mindNode = useStable(useMemo<MindNodeState | null>(() => {
    if (!sync || readOnly || selection.length !== 1) return null
    const id = selection[0]
    const graph = graphOf(sync.view, id)
    const element = sync.view.elements.get(id)
    if (!graph || !element || element.locked) return null
    return { id, root: graph.root === id, direction: graph.direction, collapsed: element.mindMapCollapsed, branches: graph.map.children.get(id)?.length ?? 0 }
    // The view changes in place; `change.revision` says when.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [change.revision, readOnly, selection, sync]))
  // A single selected card gets its target and caption in the panel.
  const cardState = useStable(useMemo<CardState | null>(() => {
    if (!sync || readOnly || selection.length !== 1) return null
    const element = sync.view.elements.get(selection[0])
    if (element?.kind !== 'card' || !element.card || element.locked) return null
    return { id: element.id, ...element.card }
    // The view changes in place; `change.revision` says when.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [change.revision, readOnly, selection, sync]))
  const retarget = useCallback(async () => {
    if (!cardState) return
    if (cardState.kind === 'link') {
      setLinkDialog({ id: cardState.id, url: cardState.url ?? 'https://', title: cardState.title ?? '' })
      return
    }
    try {
      const file = await pickTarget()
      if (file) await editor.run('card.setTarget', { id: cardState.id, file })
    } catch (error) {
      toast('error', t('cardFailed'), message(error))
    }
  }, [cardState, editor, pickTarget, t])

  // Templates, in the middle of the view.
  const [templates, setTemplates] = useState(false)
  const insertTemplate = useCallback(
    (template: string) => {
      setTemplates(false)
      const view = canvas.current?.view()
      const at = view ? [view.x + view.width / view.zoom / 2, view.y + view.height / view.zoom / 2] : undefined
      void place('template.insert', { template, ...(at ? { at } : {}) }, true)
    },
    [place],
  )

  // Export: the page, or the selected frame.
  const [exporting, setExporting] = useState(false)
  const exportFrame = useMemo(() => {
    const element = sync && selection.length === 1 ? sync.view.elements.get(selection[0]) : null
    return element?.kind === 'frame' ? { id: element.id, title: element.text } : null
    // The view changes in place; `change.revision` says when.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [change.revision, selection, sync])

  const grow = useCallback((sibling: boolean) => {
    if (mindNode) canvasApi.current?.grow(mindNode.id, sibling)
  }, [mindNode])

  // The minimap is the engine's raster in the host's color scheme (a view concern: never saved);
  // a new scheme is a new callback, so the minimap draws itself again.
  const scheme = useColorScheme()
  const render = useCallback(
    (x: number, y: number, scale: number, width: number, height: number) => {
      const open = latest.current.doc
      return open ? open.invoke<Uint8Array>('renderScheme', scheme, x, y, scale, width, height) : Promise.resolve(new Uint8Array())
    },
    [scheme],
  )

  // The page tabs, kept while the pages' ids and names stay (the board's other changes do not draw them again).
  const pages = useStable(state?.pages ?? [])
  const focusCanvas = useCallback(() => canvasApi.current?.focus(), [])

  // The frame's rail, actions and panel, kept as the same elements between changes of the
  // board, so a commit does not render them again (the commit budget, S2-1).
  const rail = useMemo(() => (readOnly ? null : <ToolRail tools={tools} active={activeTool} onSelect={selectRail} />), [activeTool, readOnly, selectRail, tools])
  const actions = useMemo(
    () => (
      <>
        {readOnly ? null : (
          <KitElement id="control:templates">
            <IconButton icon={LayoutTemplate} label={t('templates')} onClick={() => setTemplates(true)} data-testid="board-templates-open" />
          </KitElement>
        )}
        <KitElement id="control:export">
          <IconButton icon={Download} label={t('export')} onClick={() => setExporting(true)} data-testid="board-export" />
        </KitElement>
      </>
    ),
    [readOnly, t],
  )
  const panels = useMemo(
    () => (
      <KitElement id="panel:style">
        <Panel side="right" width={232} title={t('style')} testId="board-style">
          {readOnly ? null : <ShapeLibrary active={tool === 'shape' ? shapeType : null} onPick={pickShape} />}
          <StylePanel target={readOnly ? null : styleTarget} onApply={applyStyle} />
          {mindNode ? <MindMapSection node={mindNode} onGrow={grow} /> : null}
          {cardState ? <CardSection card={cardState} onRetarget={() => void retarget()} onCaption={() => canvasApi.current?.edit(cardState.id)} /> : null}
        </Panel>
      </KitElement>
    ),
    [applyStyle, cardState, grow, mindNode, pickShape, readOnly, retarget, shapeType, styleTarget, t, tool],
  )

  return (
    <EditorFrame
      tabs={tabs}
      defaultTab={DEFAULT_TAB}
      canvas={canvas}
      zoom={zoom}
      rail={rail}
      focusTool={focusTool}
      actions={actions}
      panels={panels}
      onKeyDown={(event) => canvasApi.current?.keyDown(event.nativeEvent) ?? false}
      status={
        <span data-testid="board-status">
          {state ? summary(t, host.init.locale, state.elements, Math.max(1, state.pages.length)) : ''}
          {selection.length ? ` · ${t('selected', { count: selection.length })}` : ''}
        </span>
      }
    >
      <div className="board-body" data-testid="board-body">
        {sync ? (
          <BoardCanvas
            sync={sync}
            page={state?.page ?? null}
            run={run}
            query={editor.query}
            tool={tool}
            shapeType={shapeType}
            drawStyle={drawStyle}
            highlightStyle={highlightStyle}
            onTool={chooseTool}
            readOnly={readOnly}
            selection={selection}
            onSelection={onSelection}
            onZoom={setZoom}
            area={state?.area ?? null}
            revision={change.revision}
            render={render}
            t={t}
            handle={canvas}
            api={canvasApi}
          />
        ) : session.failure ? (
          <div className="kui-message" data-testid="board-failed">
            {t('openFailed')}: {session.failure}
          </div>
        ) : (
          <div className="kui-message kui-muted">{t('opening')}</div>
        )}
      </div>
      {state && state.pages.length ? (
        <FrameChrome>
          <PageTabs pages={pages} shown={state.page} onDone={focusCanvas} />
        </FrameChrome>
      ) : null}
      {linkDialog ? <LinkDialog initial={linkDialog} onDone={placeLink} onClose={() => setLinkDialog(null)} /> : null}
      {exporting ? <ExportDialog frame={exportFrame} onClose={() => setExporting(false)} /> : null}
      {templates ? <TemplateGallery onPick={insertTemplate} onClose={() => setTemplates(false)} /> : null}
    </EditorFrame>
  )
}
