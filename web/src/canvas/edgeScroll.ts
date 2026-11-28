/**
 * Edge scrolling (plan "Board usable by hand", rule 7; tldraw's numbers in gap-tldraw.md 3): while
 * a move, a marquee or a resize drags the pointer within 8 screen pixels of the canvas's edge (or
 * past it), after 200 ms the view starts to scroll that way, easing in over 200 ms, at up to 25
 * screen pixels per 60 Hz frame times how deep the pointer is in the zone, times 0.612 on an axis
 * where the canvas is narrower than 1000 pixels. The canvas moves the view and gives the gesture the
 * board point now under the pointer (`tick`).
 */
import type { Point } from '../board/geometry'

/** The zone along each edge (screen pixels). */
export const EDGE_ZONE = 8
/** How long the pointer stays in the zone before the view scrolls (ms). */
export const EDGE_DELAY = 200
const EDGE_EASE = 200
const EDGE_SPEED = 25
const FRAME_MS = 1000 / 60

/** How deep `at` is in the zone before `low` or after `high`, -1 to 1 (0: not in it). */
function depth(at: number, low: number, high: number): number {
  if (high - low <= EDGE_ZONE * 2) return 0
  if (at < low + EDGE_ZONE) return -Math.min(1, (low + EDGE_ZONE - at) / EDGE_ZONE)
  if (at > high - EDGE_ZONE) return Math.min(1, (at - (high - EDGE_ZONE)) / EDGE_ZONE)
  return 0
}

export class EdgeScroller {
  private since: number | null = null
  private last = 0
  private frame: number | null = null
  private client: Point | null = null
  private area: DOMRect | null = null

  /** `tick` scrolls the view by (dx, dy) screen pixels and lets the gesture follow. */
  constructor(private readonly tick: (dx: number, dy: number) => void) {}

  /** The pointer of a gesture that edge-scrolls, at `client` over the canvas whose box is `area`. */
  update(client: Point, area: DOMRect) {
    this.client = client
    this.area = area
    const inZone = depth(client.x, area.left, area.right) !== 0 || depth(client.y, area.top, area.bottom) !== 0
    if (!inZone) {
      this.stop()
      return
    }
    if (this.since == null) {
      this.since = performance.now()
      this.last = this.since
      this.frame = requestAnimationFrame(this.step)
    }
  }

  /** Whether the view is scrolling (or about to). */
  get active() {
    return this.since != null
  }

  stop() {
    this.since = null
    if (this.frame != null) cancelAnimationFrame(this.frame)
    this.frame = null
  }

  private readonly step = (now: number) => {
    const { since, client, area } = this
    if (since == null || !client || !area) return
    const elapsed = now - since
    const frames = Math.max(0, now - this.last) / FRAME_MS
    this.last = now
    if (elapsed >= EDGE_DELAY) {
      const ease = Math.min(1, (elapsed - EDGE_DELAY) / EDGE_EASE)
      const narrowX = area.width < 1000 ? 0.612 : 1
      const narrowY = area.height < 1000 ? 0.612 : 1
      const dx = depth(client.x, area.left, area.right) * EDGE_SPEED * ease * narrowX * frames
      const dy = depth(client.y, area.top, area.bottom) * EDGE_SPEED * ease * narrowY * frames
      if (dx || dy) this.tick(dx, dy)
    }
    this.frame = requestAnimationFrame(this.step)
  }
}
