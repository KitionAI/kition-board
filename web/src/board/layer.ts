/**
 * The elements on screen: one `<g>` per visible element holding the engine's fragment as it is
 * (R2c: the editor draws nothing of its own), placed by the page view's geometry. Culling uses a
 * spatial index of every element's box, so a pan only adds and removes the `<g>`s that enter or
 * leave the view and fetches nothing. A gesture changes only transforms here (`override`) until
 * its command's change arrives; React never renders this layer, so a pan does not re-render the
 * editor (spike S2, 3.2).
 */
import { grow, intersects, offset, rotatedBounds, type Rect } from './geometry'
import type { BoardSync, Fragment, Lod } from './sync'
import { SpatialIndex } from './spatialIndex'

const SVG = 'http://www.w3.org/2000/svg'

export type Viewport = { x: number; y: number; zoom: number; width: number; height: number }

/**
 * A gesture's preview of an element: moved, and for a resize scaled about its top-left corner (a
 * fragment in world coordinates: `x' = x·sx + dx`). A rotation turns a boxed element about its own
 * centre (after the move) and a fragment in world coordinates about `about`.
 */
export type Override = { dx: number; dy: number; sx?: number; sy?: number; rotate?: number; about?: { x: number; y: number } }

export class ElementLayer {
  readonly index = new SpatialIndex()
  private readonly nodes = new Map<string, SVGGElement>()
  private readonly overrides = new Map<string, Override>()
  private readonly hidden = new Set<string>()
  private readonly urls = new Map<string, Promise<string>>()
  private page: string | null = null
  private lod: Lod = 'full'
  private viewport: Viewport | null = null
  /** The element whose text box is open: drawn at full detail with its own text hidden (its background stays). */
  private editing: string | null = null
  /** The view's `layout` the index was ordered by. */
  private layout = -1
  /** Element mounts and removals since the counter was reset (the pan budget check reads it). */
  mounts = 0

  constructor(
    private readonly root: SVGGElement,
    private readonly sync: BoardSync,
  ) {}

  /** The shown page; rebuilds the index. */
  showPage(page: string | null) {
    this.page = page
    for (const node of this.nodes.values()) node.remove()
    this.nodes.clear()
    this.reindex()
    if (this.viewport) this.render(this.viewport)
  }

  get pageId() {
    return this.page
  }

  /** The world box an element's fragment covers (its view bounds before the fragment is known). */
  worldBox(id: string): Rect | null {
    const view = this.sync.view
    const element = view.elements.get(id)
    if (!element) return null
    const fragment = this.sync.fragments.full.get(id)
    if (fragment?.local) {
      const box = view.rect(element)
      if (!box) return null
      return rotatedBounds(offset(fragment.box, box.x0, box.y0), element.rotation)
    }
    return fragment?.box ?? view.bounds(element)
  }

  /** The shown page's view: the one asked for (it may not have arrived yet), else the first. */
  private shownPage() {
    const view = this.sync.view
    return this.page == null ? (view.pages[0] ?? null) : view.page(this.page)
  }

  /** Re-reads every box of the shown page into the index. */
  reindex() {
    const page = this.shownPage()
    const items: Array<[string, Rect]> = []
    for (const id of page?.elements ?? []) {
      const box = this.worldBox(id)
      if (box) items.push([id, box])
    }
    this.index.sync(items)
    this.layout = this.sync.view.layout
  }

  /** Shows the elements of `viewport` (and a quarter view around it). */
  render(viewport: Viewport) {
    this.viewport = viewport
    const lod: Lod = viewport.zoom < 0.5 ? 'low' : 'full'
    if (lod !== this.lod) {
      this.lod = lod
      for (const [id, node] of this.nodes) this.fill(id, node)
      if (lod === 'low') void this.sync.loadLow().then(() => this.refresh(new Set(this.nodes.keys())))
    }
    const w = viewport.width / viewport.zoom
    const h = viewport.height / viewport.zoom
    const area = grow({ x0: viewport.x, y0: viewport.y, x1: viewport.x + w, y1: viewport.y + h }, Math.max(w, h) * 0.25)
    const collapsed = this.sync.view.hidden
    const wanted = this.index.query(area).filter((id) => !this.hidden.has(id) && !collapsed.has(id))
    const keep = new Set(wanted)
    for (const [id, node] of this.nodes) {
      if (keep.has(id)) continue
      node.remove()
      this.nodes.delete(id)
      this.mounts += 1
    }
    // Back to front: move only the nodes out of place.
    let previous: SVGGElement | null = null
    for (const id of wanted) {
      let node = this.nodes.get(id)
      if (!node) {
        const made = this.make(id)
        if (!made) continue
        node = made
        this.nodes.set(id, node)
        this.mounts += 1
      }
      const expected: ChildNode | null = previous ? previous.nextSibling : this.root.firstChild
      if (expected !== node) this.root.insertBefore(node, expected)
      previous = node
    }
  }

  /**
   * After a pull: re-index and redraw `changed`. Only a change that re-read the page lists
   * (`view.layout` moved) can reorder elements; a patch in place re-indexes just `changed`.
   */
  refresh(changed: ReadonlySet<string>) {
    const page = this.shownPage()
    const view = this.sync.view
    if (!page) this.reindex()
    else if (view.layout === this.layout) {
      for (const id of changed) {
        const box = view.placement.has(id) ? this.worldBox(id) : null
        if (box) this.index.upsert(id, box, this.index.order(id) ?? page.elements.indexOf(id))
        else this.index.remove(id)
      }
    } else {
      const order = new Map(page.elements.map((id, at) => [id, at]))
      for (const id of changed) {
        const at = order.get(id)
        const box = at == null ? null : this.worldBox(id)
        if (box && at != null) this.index.upsert(id, box, at)
        else this.index.remove(id)
      }
      // Reordering (bring to front) changes positions without changing boxes.
      for (const [id, at] of order) {
        const box = this.index.box(id)
        if (box) this.index.upsert(id, box, at)
      }
    }
    this.layout = view.layout
    for (const id of changed) {
      const node = this.nodes.get(id)
      if (!node) continue
      if (!this.sync.view.elements.has(id)) {
        node.remove()
        this.nodes.delete(id)
        continue
      }
      this.fill(id, node)
    }
    if (this.viewport) this.render(this.viewport)
  }

  /** The DOM node of a shown element. */
  node(id: string) {
    return this.nodes.get(id) ?? null
  }

  /** Previews a gesture on `ids` (moves their nodes only). */
  setOverrides(overrides: ReadonlyMap<string, Override>) {
    const before = [...this.overrides.keys()]
    this.overrides.clear()
    for (const [id, value] of overrides) this.overrides.set(id, value)
    for (const id of new Set([...before, ...overrides.keys()])) {
      const node = this.nodes.get(id)
      if (node) this.place(id, node)
    }
  }

  /** Hides `ids` (connectors redrawn live during a drag). */
  setHidden(ids: Iterable<string>) {
    const next = new Set(ids)
    const changed = [...this.hidden].some((id) => !next.has(id)) || [...next].some((id) => !this.hidden.has(id))
    if (!changed) return
    this.hidden.clear()
    for (const id of next) this.hidden.add(id)
    for (const id of next) {
      this.nodes.get(id)?.remove()
      this.nodes.delete(id)
    }
    if (this.viewport) this.render(this.viewport)
  }

  /**
   * The element whose text is being edited (null: none). Its text is hidden while the box shows
   * it (`.board-editing` in app.css), its outline and fill stay; at low detail it is drawn at full
   * detail, whose text is glyphs the rule can hide, not bars.
   */
  setEditing(id: string | null) {
    const before = this.editing
    this.editing = id
    for (const which of new Set([before, id])) {
      const node = which ? this.nodes.get(which) : undefined
      if (which && node) this.fill(which, node)
    }
  }

  /** The ids whose boxes meet `area`. */
  query(area: Rect) {
    return this.index.query(area)
  }

  /** Whether `id`'s box meets `area`. */
  meets(id: string, area: Rect) {
    const box = this.index.box(id)
    return box ? intersects(box, area) : false
  }

  private lodOf(id: string): Lod {
    return id === this.editing ? 'full' : this.lod
  }

  private fragment(id: string): Fragment | null {
    return (this.lodOf(id) === 'low' ? this.sync.fragments.low.get(id) : null) ?? this.sync.fragments.full.get(id) ?? null
  }

  private make(id: string) {
    const element = this.sync.view.elements.get(id)
    if (!element) return null
    const node = document.createElementNS(SVG, 'g') as SVGGElement
    node.setAttribute('data-id', id)
    node.setAttribute('data-kind', element.kind)
    this.fill(id, node)
    return node
  }

  private fill(id: string, node: SVGGElement) {
    const fragment = this.fragment(id)
    const svg = fragment?.svg ?? ''
    const drawn = String(fragment?.rev ?? 0) + this.lodOf(id)
    if (node.dataset.svg !== drawn) {
      node.innerHTML = svg
      node.dataset.svg = drawn
      for (const image of node.querySelectorAll('image[data-blob]')) this.picture(image as SVGImageElement)
    }
    node.classList.toggle('board-editing', id === this.editing)
    this.place(id, node)
  }

  /** Points a picture at an object URL of its blob's bytes. */
  private picture(image: SVGImageElement) {
    const hash = image.getAttribute('data-blob')
    if (!hash) return
    let url = this.urls.get(hash)
    if (!url) {
      url = this.sync.blob(hash).then((bytes) => (bytes.length ? URL.createObjectURL(new Blob([bytes.slice().buffer as ArrayBuffer])) : ''))
      this.urls.set(hash, url)
    }
    void url.then((href) => {
      if (href) image.setAttribute('href', href)
    })
  }

  private place(id: string, node: SVGGElement) {
    const view = this.sync.view
    const element = view.elements.get(id)
    const fragment = this.sync.fragments.full.get(id)
    const override = this.overrides.get(id)
    if (!element) return
    let transform = ''
    if (fragment?.local) {
      const box = view.rect(element)
      if (box) {
        const x = box.x0 + (override?.dx ?? 0)
        const y = box.y0 + (override?.dy ?? 0)
        const w = (box.x1 - box.x0) * (override?.sx ?? 1)
        const h = (box.y1 - box.y0) * (override?.sy ?? 1)
        const rotation = element.rotation + (override?.rotate ?? 0)
        transform = `translate(${x} ${y})`
        if (rotation) transform += ` rotate(${rotation} ${w / 2} ${h / 2})`
        if (override?.sx != null || override?.sy != null) transform += ` scale(${override.sx ?? 1} ${override.sy ?? 1})`
      }
    } else if (override?.rotate && override.about) {
      transform = `rotate(${override.rotate} ${override.about.x} ${override.about.y})`
    } else if (override) {
      transform = override.sx != null || override.sy != null ? `matrix(${override.sx ?? 1} 0 0 ${override.sy ?? 1} ${override.dx} ${override.dy})` : `translate(${override.dx} ${override.dy})`
    }
    if (transform) node.setAttribute('transform', transform)
    else node.removeAttribute('transform')
  }
}
