/**
 * Snapping a moved selection or a resize handle to the edges, centres and equal gaps of the boxes
 * around it.
 *
 * Ported from Kition's `packages/board-editor/src/lib/boardSnapManager.ts`. It works on boxes
 * (`Rect`) instead of v1 elements.
 */
import type { Point, Rect } from './geometry'

export type SnapGuide = { axis: 'x' | 'y'; position: number; start: number; end: number }

type AxisSnap = { delta: number; position: number; target: Rect }

const positions = (box: Rect, axis: 'x' | 'y') => (axis === 'x' ? [box.x0, (box.x0 + box.x1) / 2, box.x1] : [box.y0, (box.y0 + box.y1) / 2, box.y1])
const start = (box: Rect, axis: 'x' | 'y') => (axis === 'x' ? box.x0 : box.y0)
const end = (box: Rect, axis: 'x' | 'y') => (axis === 'x' ? box.x1 : box.y1)

function axisSnap(moving: readonly number[], targets: readonly Rect[], axis: 'x' | 'y', threshold: number): AxisSnap | null {
  let best: AxisSnap | null = null
  for (const target of targets) {
    for (const position of positions(target, axis)) {
      for (const from of moving) {
        const delta = position - from
        if (Math.abs(delta) > threshold) continue
        if (!best || Math.abs(delta) < Math.abs(best.delta)) best = { delta, position, target }
      }
    }
  }
  return best
}

function equalGapSnap(moving: Rect, targets: readonly Rect[], axis: 'x' | 'y', threshold: number): AxisSnap | null {
  const size = end(moving, axis) - start(moving, axis)
  const before = targets.filter((box) => end(box, axis) <= start(moving, axis) + threshold)
  const after = targets.filter((box) => start(box, axis) >= end(moving, axis) - threshold)
  let best: AxisSnap | null = null
  for (const left of before) {
    for (const right of after) {
      const at = (end(left, axis) + start(right, axis) - size) / 2
      const candidate: AxisSnap = {
        delta: at - start(moving, axis),
        position: at + size / 2,
        target: { x0: Math.min(left.x0, right.x0), y0: Math.min(left.y0, right.y0), x1: Math.max(left.x1, right.x1), y1: Math.max(left.y1, right.y1) },
      }
      if (Math.abs(candidate.delta) > threshold) continue
      if (!best || Math.abs(candidate.delta) < Math.abs(best.delta)) best = candidate
    }
  }
  return best
}

/** How far to nudge a selection whose box is `moving` so it lines up with `targets`, and the guides. */
export function translationSnap(moving: Rect, targets: readonly Rect[], threshold: number): { adjustment: Point; guides: SnapGuide[] } {
  if (!targets.length) return { adjustment: { x: 0, y: 0 }, guides: [] }
  const x = axisSnap(positions(moving, 'x'), targets, 'x', threshold) ?? equalGapSnap(moving, targets, 'x', threshold)
  const y = axisSnap(positions(moving, 'y'), targets, 'y', threshold) ?? equalGapSnap(moving, targets, 'y', threshold)
  const moved = { x0: moving.x0 + (x?.delta ?? 0), y0: moving.y0 + (y?.delta ?? 0), x1: moving.x1 + (x?.delta ?? 0), y1: moving.y1 + (y?.delta ?? 0) }
  const guides: SnapGuide[] = []
  if (x) guides.push({ axis: 'x', position: x.position, start: Math.min(moved.y0, x.target.y0), end: Math.max(moved.y1, x.target.y1) })
  if (y) guides.push({ axis: 'y', position: y.position, start: Math.min(moved.x0, y.target.x0), end: Math.max(moved.x1, y.target.x1) })
  return { adjustment: { x: x?.delta ?? 0, y: y?.delta ?? 0 }, guides }
}

export type Handle = 'north' | 'south' | 'east' | 'west' | 'north-east' | 'north-west' | 'south-east' | 'south-west'

/** Where a resize handle dragged to `point` snaps, and the guides. */
export function resizeSnap(handle: Handle, point: Point, selection: Rect, targets: readonly Rect[], threshold: number): { point: Point; guides: SnapGuide[] } {
  const out = { ...point }
  const guides: SnapGuide[] = []
  if (handle.includes('west') || handle.includes('east')) {
    const snap = axisSnap([point.x], targets, 'x', threshold)
    if (snap) {
      out.x += snap.delta
      guides.push({ axis: 'x', position: snap.position, start: Math.min(selection.y0, snap.target.y0), end: Math.max(selection.y1, snap.target.y1) })
    }
  }
  if (handle.includes('north') || handle.includes('south')) {
    const snap = axisSnap([point.y], targets, 'y', threshold)
    if (snap) {
      out.y += snap.delta
      guides.push({ axis: 'y', position: snap.position, start: Math.min(selection.x0, snap.target.x0), end: Math.max(selection.x1, snap.target.x1) })
    }
  }
  return { point: out, guides }
}
