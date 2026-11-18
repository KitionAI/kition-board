/**
 * Thinning a freehand stroke before it becomes `stroke.create`: radial distance, then
 * Douglas–Peucker.
 *
 * Ported from Kition's `packages/board-editor/src/lib/boardFreehand.ts`.
 */
import type { Point } from './geometry'

export function simplifyStroke(points: readonly Point[], tolerance = 1): Point[] {
  if (points.length <= 2) return points.map((point) => ({ ...point }))
  const squared = Math.max(0, tolerance) ** 2
  return douglasPeucker(radial(points, squared), squared)
}

function radial(points: readonly Point[], squared: number): Point[] {
  const out: Point[] = [{ ...points[0] }]
  let previous = points[0]
  for (let index = 1; index < points.length; index += 1) {
    const point = points[index]
    if (distanceSquared(point, previous) <= squared) continue
    out.push({ ...point })
    previous = point
  }
  const last = points[points.length - 1]
  if (previous !== last) out.push({ ...last })
  return out
}

function douglasPeucker(points: readonly Point[], squared: number): Point[] {
  if (points.length <= 2) return points.map((point) => ({ ...point }))
  const keep = new Uint8Array(points.length)
  keep[0] = 1
  keep[points.length - 1] = 1
  const stack: Array<[number, number]> = [[0, points.length - 1]]
  while (stack.length) {
    const [first, last] = stack.pop()!
    let furthest = 0
    let furthestDistance = squared
    for (let index = first + 1; index < last; index += 1) {
      const distance = segmentDistanceSquared(points[index], points[first], points[last])
      if (distance > furthestDistance) {
        furthest = index
        furthestDistance = distance
      }
    }
    if (!furthest) continue
    keep[furthest] = 1
    stack.push([first, furthest], [furthest, last])
  }
  return points.filter((_, index) => keep[index]).map((point) => ({ ...point }))
}

function distanceSquared(a: Point, b: Point) {
  return (a.x - b.x) ** 2 + (a.y - b.y) ** 2
}

function segmentDistanceSquared(p: Point, a: Point, b: Point) {
  let x = a.x
  let y = a.y
  const dx = b.x - x
  const dy = b.y - y
  if (dx !== 0 || dy !== 0) {
    const t = ((p.x - x) * dx + (p.y - y) * dy) / (dx * dx + dy * dy)
    if (t > 1) {
      x = b.x
      y = b.y
    } else if (t > 0) {
      x += dx * t
      y += dy * t
    }
  }
  return (p.x - x) ** 2 + (p.y - y) ** 2
}
