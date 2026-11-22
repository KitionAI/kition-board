/**
 * The page's replica of the board and the engine's fragments (design note 4.1–4.2, R2c).
 *
 * The page holds a KDM replica of the engine worker's board, loaded from the engine's snapshot and
 * kept in step by pulling the worker's changes after every change (a command, an agent, undo, a
 * collaborator). It never writes the replica. Each pull reads the changed elements into the page
 * view and asks the engine for the fragments whose appearance changed (`elementSvg` with the
 * revisions the page already has); everything is fetched once at open, in batches of 500, so
 * panning and zooming fetch nothing (spike S2, 5.2).
 */
import { parseSchema, randomActor, Replica } from '@kition/sdk/kdm'
import schemaJson from '../../../crates/kdm/kdm-model/model/schema.json'
import type { Rect } from './geometry'
import { BoardView } from './model'
import { readOutlines } from './outline'

export const boardSchema = parseSchema(schemaJson)

/** What the page needs from the engine worker. */
export type BoardEngine = {
  snapshot(): Promise<Uint8Array>
  exportChanges(since: ArrayBuffer): Promise<{ changes: ArrayBuffer; blobs: ArrayBuffer }>
  elementSvg(request: string): Promise<string>
  blob(hash: string): Promise<Uint8Array>
  textMetrics(request: string): Promise<string>
  /** `shape.previews`'s answer: each shape's `outline`, what a bound end meets (design note 1.4). */
  shapePreviews(): Promise<unknown>
}

export type Lod = 'full' | 'low'

/** One element's fragment as the engine drew it. */
export type Fragment = {
  rev: number
  svg: string
  /** In the element's unrotated local frame when `local`, else in world coordinates. */
  box: Rect
  local: boolean
  size: { w: number; h: number } | null
  slot: Rect | null
}

type Answer = {
  fragments: Array<{ id: string; rev: number; svg: string; box: [number, number, number, number]; frame: 'local' | 'world'; size?: [number, number]; slot?: [number, number, number, number] }>
  unchanged: string[]
  missing: string[]
  fontEpoch: number
}

const BATCH = 500
const box = ([x0, y0, x1, y1]: [number, number, number, number]): Rect => ({ x0, y0, x1, y1 })
const own = (bytes: Uint8Array) => bytes.slice().buffer as ArrayBuffer

/** The fragments the page has, by level of detail. */
export class FragmentStore {
  readonly full = new Map<string, Fragment>()
  readonly low = new Map<string, Fragment>()
  /** Fragment requests sent (the pan budget check counts them). */
  requests = 0

  constructor(private readonly engine: BoardEngine) {}

  of(lod: Lod) {
    return lod === 'full' ? this.full : this.low
  }

  /** Brings `ids` up to date at `lod`; answers the ids whose fragment changed or went. */
  async fetch(ids: readonly string[], lod: Lod): Promise<Set<string>> {
    const store = this.of(lod)
    const changed = new Set<string>()
    for (let at = 0; at < ids.length; at += BATCH) {
      const batch = ids.slice(at, at + BATCH)
      const known: Record<string, number> = {}
      for (const id of batch) {
        const fragment = store.get(id)
        if (fragment) known[id] = fragment.rev
      }
      this.requests += 1
      const answer = JSON.parse(await this.engine.elementSvg(JSON.stringify({ ids: batch, lod, known }))) as Answer
      for (const item of answer.fragments) {
        store.set(item.id, {
          rev: item.rev,
          svg: item.svg,
          box: box(item.box),
          local: item.frame === 'local',
          size: item.size ? { w: item.size[0], h: item.size[1] } : null,
          slot: item.slot ? box(item.slot) : null,
        })
        changed.add(item.id)
      }
      for (const id of answer.missing) {
        if (store.delete(id)) changed.add(id)
      }
    }
    return changed
  }
}

/** The page side of an open board. */
export class BoardSync {
  readonly view: BoardView
  readonly fragments: FragmentStore
  private queue: Promise<unknown> = Promise.resolve()
  private closed = false
  private lowLoaded = false
  private readonly listeners = new Set<(changed: ReadonlySet<string>) => void>()

  private constructor(
    readonly replica: Replica,
    private readonly engine: BoardEngine,
  ) {
    this.fragments = new FragmentStore(engine)
    this.view = new BoardView((id) => this.fragments.full.get(id)?.size ?? null)
  }

  /** Loads the engine's board into a page replica, its view, and every element's fragment. */
  static async open(engine: BoardEngine): Promise<BoardSync> {
    const replica = Replica.load(boardSchema, await engine.snapshot(), randomActor())
    const sync = new BoardSync(replica, engine)
    sync.view.refresh(replica, null)
    await Promise.all([sync.fragments.fetch([...sync.view.elements.keys()], 'full'), sync.loadOutlines()])
    // The first import of a page replica is slow (spike S2: 18–26 ms); warm it up now, not on the
    // first commit.
    const { changes } = await engine.exportChanges(own(replica.version().encode()))
    replica.importChanges(new Uint8Array(changes))
    return sync
  }

  /**
   * The shapes' outlines from the engine, so the page derives bound ends as the engine does. A
   * failed or empty answer leaves every end on the box.
   */
  private async loadOutlines() {
    try {
      const outlines = readOutlines(await this.engine.shapePreviews())
      if (outlines.size) this.view.outlines = outlines
    } catch {
      // The box until a later open: the engine's fragments still draw the ends where they are.
    }
  }

  /** Called with the ids that changed after each pull that changed something. */
  subscribe(listener: (changed: ReadonlySet<string>) => void) {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  /** The low-detail fragments, fetched for every element the first time the zoom drops below 50%. */
  async loadLow() {
    if (this.lowLoaded) return
    this.lowLoaded = true
    await this.enqueue(async () => {
      await this.fragments.fetch([...this.view.elements.keys()], 'low')
    })
  }

  /**
   * Takes the engine's changes the page does not have yet. Answers once they are shown.
   *
   * `hint` names elements the caller knows the change touched (a command's answer): their
   * fragments (and their bound connectors') are asked for together with the changes, so a commit
   * waits for one round trip to the worker instead of two (B10b). The `known` revisions make a
   * wrong hint cost nothing but the request; whatever else the change touched is asked for after.
   */
  pull(hint: readonly string[] = []): Promise<void> {
    return this.enqueue(async () => {
      const early = new Set(hint.filter((id) => this.view.elements.has(id)))
      for (const id of this.view.connectorsOf(early)) early.add(id)
      const [{ changes }, fetchedEarly] = await Promise.all([
        this.engine.exportChanges(own(this.replica.version().encode())),
        early.size ? this.fetchLods([...early]) : Promise.resolve(new Set<string>()),
      ])
      if (this.closed) return
      const changedObjects = this.replica.importChanges(new Uint8Array(changes))
      if (!changedObjects.size && !fetchedEarly.size) return
      const touched = changedObjects.size ? this.view.refresh(this.replica, changedObjects) : new Set<string>()
      // A connector's look follows its targets: ask for it again when they changed.
      const ask = new Set(touched)
      for (const id of this.view.connectorsOf(touched)) ask.add(id)
      const present = [...ask].filter((id) => this.view.elements.has(id) && !early.has(id))
      const gone = [...ask].filter((id) => !this.view.elements.has(id))
      for (const id of gone) {
        this.fragments.full.delete(id)
        this.fragments.low.delete(id)
      }
      const changed = present.length ? await this.fetchLods(present) : new Set<string>()
      for (const id of fetchedEarly) changed.add(id)
      for (const id of [...touched, ...gone]) changed.add(id)
      if (this.closed || !changed.size) return
      for (const listener of this.listeners) listener(changed)
    })
  }

  /** Brings `ids` up to date at every level of detail the page holds, in one go. */
  private async fetchLods(ids: string[]): Promise<Set<string>> {
    const [full, low] = await Promise.all([this.fragments.fetch(ids, 'full'), this.lowLoaded ? this.fragments.fetch(ids, 'low') : Promise.resolve(new Set<string>())])
    for (const id of low) full.add(id)
    return full
  }

  /** The engine's line layout (`textMetrics`), for checking it against the browser. */
  textMetrics(request: string) {
    return this.engine.textMetrics(request)
  }

  /** A picture's bytes. */
  blob(hash: string) {
    return this.engine.blob(hash)
  }

  close() {
    this.closed = true
    this.listeners.clear()
  }

  private enqueue(step: () => Promise<void>): Promise<void> {
    const next = this.queue.then(step, step)
    this.queue = next.catch(() => undefined)
    return next
  }
}
