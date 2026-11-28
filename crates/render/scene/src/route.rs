//! Connector routes (straight, elbow, curved, mind-map branch) and freehand paths, in world
//! coordinates.
//!
//! Ported from Kition's `packages/board-editor/src/lib/whiteboardGeometry.ts`
//! (`getWhiteboardConnectorPath`, `getRoundedOrthogonalPath`, `getWhiteboardMindMapBranchPath`,
//! `whiteboardPointsToPath`). New: routes through `waypoints`, and the tangents arrowheads are
//! drawn along.

use kition_board_kdm::Point;

use crate::svg::num;

/// One piece of a path after its current point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Line(Point),
    Quad(Point, Point),
    Cubic(Point, Point, Point),
}

impl Seg {
    pub fn end(&self) -> Point {
        match *self {
            Seg::Line(p) | Seg::Quad(_, p) | Seg::Cubic(_, _, p) => p,
        }
    }

    fn first_control(&self) -> Point {
        match *self {
            Seg::Line(p) | Seg::Quad(p, _) | Seg::Cubic(p, _, _) => p,
        }
    }

    fn controls(&self) -> Vec<Point> {
        match *self {
            Seg::Line(p) => vec![p],
            Seg::Quad(c, p) => vec![c, p],
            Seg::Cubic(a, b, p) => vec![a, b, p],
        }
    }
}

/// A path: a start and its pieces.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub start: Point,
    pub segs: Vec<Seg>,
}

fn distance(a: Point, b: Point) -> f64 {
    (b.x - a.x).hypot(b.y - a.y)
}

fn toward(from: Point, to: Point, by: f64) -> Point {
    let length = distance(from, to);
    if length == 0.0 {
        return from;
    }
    Point::new(from.x + (to.x - from.x) * by / length, from.y + (to.y - from.y) * by / length)
}

impl Path {
    /// The SVG path data.
    pub fn d(&self) -> String {
        let mut out = format!("M {} {}", num(self.start.x), num(self.start.y));
        for seg in &self.segs {
            match seg {
                Seg::Line(p) => out.push_str(&format!(" L {} {}", num(p.x), num(p.y))),
                Seg::Quad(c, p) => out.push_str(&format!(" Q {} {} {} {}", num(c.x), num(c.y), num(p.x), num(p.y))),
                Seg::Cubic(a, b, p) => out.push_str(&format!(" C {} {} {} {} {} {}", num(a.x), num(a.y), num(b.x), num(b.y), num(p.x), num(p.y))),
            }
        }
        out
    }

    /// Every point and control point.
    pub fn hull(&self) -> Vec<Point> {
        std::iter::once(self.start).chain(self.segs.iter().flat_map(Seg::controls)).collect()
    }

    pub fn end(&self) -> Point {
        self.segs.last().map_or(self.start, Seg::end)
    }

    /// The point before the end the path arrives from (its last control point).
    fn before_end(&self) -> Point {
        let Some(last) = self.segs.last() else { return self.start };
        let previous = || if self.segs.len() >= 2 { self.segs.get(self.segs.len() - 2).map_or(self.start, Seg::end) } else { self.start };
        let candidate = match *last {
            Seg::Line(_) => previous(),
            Seg::Quad(c, _) => c,
            Seg::Cubic(_, b, _) => b,
        };
        if distance(candidate, last.end()) > 1e-9 { candidate } else { previous() }
    }

    /// The point after the start the path leaves toward.
    fn after_start(&self) -> Point {
        let Some(first) = self.segs.first() else { return self.start };
        let candidate = first.first_control();
        if distance(candidate, self.start) > 1e-9 { candidate } else { first.end() }
    }

    /// The unit direction the path arrives at its end with; `None` for a path of no length.
    pub fn end_direction(&self) -> Option<Point> {
        unit(self.before_end(), self.end())
    }

    /// The unit direction the path leaves its start with, pointing back out of the start.
    pub fn start_direction(&self) -> Option<Point> {
        unit(self.after_start(), self.start)
    }

    /// The path with its start pulled in by `start_by` and its end by `end_by` (to make room for
    /// arrowheads); ends on pieces too short for it stay.
    #[must_use]
    pub fn trimmed(&self, start_by: f64, end_by: f64) -> Path {
        let mut out = self.clone();
        if start_by > 0.0 {
            let next = self.after_start();
            if distance(self.start, next) > start_by * 1.5 {
                out.start = toward(self.start, next, start_by);
            }
        }
        if end_by > 0.0 {
            let before = self.before_end();
            let end = self.end();
            if distance(before, end) > end_by * 1.5
                && let Some(last) = out.segs.last_mut()
            {
                let moved = toward(end, before, end_by);
                *last = match *last {
                    Seg::Line(_) => Seg::Line(moved),
                    Seg::Quad(c, _) => Seg::Quad(c, moved),
                    Seg::Cubic(a, b, _) => Seg::Cubic(a, b, moved),
                };
            }
        }
        out
    }
}

fn unit(from: Point, to: Point) -> Option<Point> {
    let length = distance(from, to);
    (length > 1e-9).then(|| Point::new((to.x - from.x) / length, (to.y - from.y) / length))
}

/// `points` without repeats and without the middle of three points on one horizontal or vertical
/// line: the corners an orthogonal route turns at.
fn simplified(points: &[Point]) -> Vec<Point> {
    let mut unique: Vec<Point> = Vec::with_capacity(points.len());
    for point in points {
        if unique.last() != Some(point) {
            unique.push(*point);
        }
    }
    unique
        .iter()
        .enumerate()
        .filter(|(index, point)| {
            let (Some(previous), Some(next)) = (index.checked_sub(1).and_then(|i| unique.get(i)), unique.get(index + 1)) else { return true };
            let horizontal = previous.y == point.y && point.y == next.y;
            let vertical = previous.x == point.x && point.x == next.x;
            !horizontal && !vertical
        })
        .map(|(_, point)| *point)
        .collect()
}

/// A polyline with its corners rounded (at most 12 units, at most half of either side).
fn rounded(points: &[Point]) -> Option<Path> {
    let simplified = simplified(points);
    let start = *simplified.first()?;
    let mut segs = Vec::new();
    for (index, corner) in simplified.iter().enumerate().skip(1) {
        let Some(next) = simplified.get(index + 1) else {
            segs.push(Seg::Line(*corner));
            break;
        };
        let previous = simplified.get(index - 1).copied().unwrap_or(start);
        let radius = 12.0_f64.min(distance(previous, *corner) / 2.0).min(distance(*corner, *next) / 2.0);
        segs.push(Seg::Line(toward(*corner, previous, radius)));
        segs.push(Seg::Quad(*corner, toward(*corner, *next, radius)));
    }
    Some(Path { start, segs })
}

/// A smooth path through `points` (quadratic pieces between midpoints), as freehand strokes.
pub fn smooth(points: &[Point]) -> Option<Path> {
    let start = *points.first()?;
    let mut segs = Vec::new();
    match points.len() {
        1 => segs.push(Seg::Line(start)),
        2 => segs.extend(points.get(1).map(|p| Seg::Line(*p))),
        n => {
            for index in 1..n - 1 {
                let (Some(current), Some(next)) = (points.get(index), points.get(index + 1)) else { break };
                segs.push(Seg::Quad(*current, Point::new((current.x + next.x) / 2.0, (current.y + next.y) / 2.0)));
            }
            segs.extend(points.last().map(|p| Seg::Line(*p)));
        }
    }
    Some(Path { start, segs })
}

fn branch_offset(delta: f64) -> f64 {
    let distance = delta.abs();
    if distance == 0.0 {
        return 0.0;
    }
    delta.signum() * (distance / 2.0).min(32.0_f64.max(140.0_f64.min(distance * 0.45)))
}

/// How far an elbow goes straight out of a side that faces away from the other end before it turns.
const STUB: f64 = 24.0;

/// A connector's path through its route (start, waypoints, end). `kind` is its `connectorType`;
/// a mind-map branch (`role`) curves along `axis`.
pub fn connector(route: &[Point], kind: Option<&str>, role: Option<&str>, axis: Option<&str>) -> Option<Path> {
    connector_from(route, kind, role, axis, [None, None])
}

/// [`connector`], with the sides its start and end are bound to (each a unit axis vector pointing
/// out of its element, `BoardModel::end_side`; `None` for a free end): an elbow without bends
/// leaves and enters through them.
pub fn connector_from(route: &[Point], kind: Option<&str>, role: Option<&str>, axis: Option<&str>, sides: [Option<Point>; 2]) -> Option<Path> {
    let (start, end) = (*route.first()?, *route.last()?);
    if route.len() < 2 {
        return None;
    }
    let bends = route.len() > 2;
    if role == Some("mind-map-branch") && !bends {
        let seg = if axis == Some("vertical") {
            let o = branch_offset(end.y - start.y);
            Seg::Cubic(Point::new(start.x, start.y + o), Point::new(end.x, end.y - o), end)
        } else {
            let o = branch_offset(end.x - start.x);
            Seg::Cubic(Point::new(start.x + o, start.y), Point::new(end.x - o, end.y), end)
        };
        return Some(Path { start, segs: vec![seg] });
    }
    match kind {
        Some("elbow") if !bends => rounded(&elbow(start, end, sides)),
        Some("elbow") => rounded(route),
        Some("curved") if !bends => {
            let middle = (start.x + end.x) / 2.0;
            Some(Path { start, segs: vec![Seg::Cubic(Point::new(middle, start.y), Point::new(middle, end.y), end)] })
        }
        Some("curved") => smooth(route),
        _ => Some(Path { start, segs: route.iter().skip(1).map(|p| Seg::Line(*p)).collect() }),
    }
}

/// The corners a connector's path goes through, start and end included: an elbow's turns (its
/// automatic ones without bends, else its route without repeats or straight-through points), the
/// route itself for the other kinds (their bends; a curve's points it passes through). This is
/// the line the editor puts a selected connector's segment handles on (`connector.getRoute`).
pub fn corners(route: &[Point], kind: Option<&str>, role: Option<&str>, sides: [Option<Point>; 2]) -> Vec<Point> {
    let (Some(start), Some(end)) = (route.first(), route.last()) else { return Vec::new() };
    let bends = route.len() > 2;
    if role == Some("mind-map-branch") && !bends {
        return vec![*start, *end];
    }
    match kind {
        Some("elbow") if !bends => simplified(&elbow(*start, *end, sides)),
        Some("elbow") => simplified(route),
        _ => route.to_vec(),
    }
}

/// The corners of an elbow from `start` to `end` that leaves and enters through `sides` (pointing
/// out of each end's element). A free end goes sideways toward the other end, so two free ends,
/// or two facing sides, turn halfway across as they always did. A side that faces away first
/// goes [`STUB`] straight out. No route avoids other elements.
fn elbow(start: Point, end: Point, sides: [Option<Point>; 2]) -> Vec<Point> {
    let sideways = |from: f64, to: f64| Point::new(if to >= from { 1.0 } else { -1.0 }, 0.0);
    let [from, to] = sides;
    let out = from.unwrap_or_else(|| sideways(start.x, end.x));
    let back = to.unwrap_or_else(|| sideways(end.x, start.x));
    let (s, e) = (Point::new(start.x + out.x * STUB, start.y + out.y * STUB), Point::new(end.x + back.x * STUB, end.y + back.y * STUB));
    match (out.y == 0.0, back.y == 0.0) {
        (true, true) => {
            let middle = (start.x + end.x) / 2.0;
            if (middle - start.x) * out.x >= 0.0 && (middle - end.x) * back.x >= 0.0 {
                vec![start, Point::new(middle, start.y), Point::new(middle, end.y), end]
            } else if out.x == back.x {
                // Both sides face the same way: around the outer one.
                let x = if out.x > 0.0 { s.x.max(e.x) } else { s.x.min(e.x) };
                vec![start, Point::new(x, start.y), Point::new(x, end.y), end]
            } else {
                let y = (start.y + end.y) / 2.0;
                vec![start, s, Point::new(s.x, y), Point::new(e.x, y), e, end]
            }
        }
        (false, false) => {
            let middle = (start.y + end.y) / 2.0;
            if (middle - start.y) * out.y >= 0.0 && (middle - end.y) * back.y >= 0.0 {
                vec![start, Point::new(start.x, middle), Point::new(end.x, middle), end]
            } else if out.y == back.y {
                let y = if out.y > 0.0 { s.y.max(e.y) } else { s.y.min(e.y) };
                vec![start, Point::new(start.x, y), Point::new(end.x, y), end]
            } else {
                let x = (start.x + end.x) / 2.0;
                vec![start, s, Point::new(x, s.y), Point::new(x, e.y), e, end]
            }
        }
        // Out sideways, in from above or below: one turn where both lines meet, when it is
        // outside both sides.
        (true, false) => {
            let corner = Point::new(end.x, start.y);
            if (corner.x - start.x) * out.x > 0.0 && (corner.y - end.y) * back.y > 0.0 {
                vec![start, corner, end]
            } else {
                vec![start, s, Point::new(s.x, e.y), e, end]
            }
        }
        (false, true) => {
            let corner = Point::new(start.x, end.y);
            if (corner.y - start.y) * out.y > 0.0 && (corner.x - end.x) * back.x > 0.0 {
                vec![start, corner, end]
            } else {
                vec![start, s, Point::new(e.x, s.y), e, end]
            }
        }
    }
}

/// The point `t` (0..1) of the way along a polyline.
pub fn along(points: &[Point], t: f64) -> Option<Point> {
    let first = *points.first()?;
    let total: f64 = points
        .windows(2)
        .map(|pair| match pair {
            [a, b] => distance(*a, *b),
            _ => 0.0,
        })
        .sum();
    if total <= 0.0 {
        return Some(first);
    }
    let mut left = total * if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.5 };
    for pair in points.windows(2) {
        if let [a, b] = pair {
            let length = distance(*a, *b);
            if left <= length {
                return Some(toward(*a, *b, left));
            }
            left -= length;
        }
    }
    points.last().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn straight_elbow_and_curved_routes() {
        let route = [p(0.0, 0.0), p(100.0, 50.0)];
        assert_eq!(connector(&route, None, None, None).unwrap().d(), "M 0 0 L 100 50");
        let elbow = connector(&route, Some("elbow"), None, None).unwrap();
        assert_eq!(elbow.d(), "M 0 0 L 38 0 Q 50 0 50 12 L 50 38 Q 50 50 62 50 L 100 50");
        assert_eq!(connector(&route, Some("curved"), None, None).unwrap().d(), "M 0 0 C 50 0 50 50 100 50");
        let through = connector(&[p(0.0, 0.0), p(0.0, 100.0), p(100.0, 100.0)], Some("straight"), None, None).unwrap();
        assert_eq!(through.d(), "M 0 0 L 0 100 L 100 100", "straight routes pass through waypoints");
        assert!(connector(&[p(1.0, 1.0)], None, None, None).is_none());
    }

    #[test]
    fn tangents_and_trimming_follow_the_arrival() {
        let path = connector(&[p(0.0, 0.0), p(100.0, 0.0)], None, None, None).unwrap();
        assert_eq!(path.end_direction(), Some(p(1.0, 0.0)));
        assert_eq!(path.start_direction(), Some(p(-1.0, 0.0)));
        assert_eq!(path.trimmed(10.0, 20.0).d(), "M 10 0 L 80 0");
        let curved = connector(&[p(0.0, 0.0), p(100.0, 100.0)], Some("curved"), None, None).unwrap();
        assert_eq!(curved.end_direction(), Some(p(1.0, 0.0)), "a curve arrives horizontally");
        let short = connector(&[p(0.0, 0.0), p(5.0, 0.0)], None, None, None).unwrap();
        assert_eq!(short.trimmed(10.0, 10.0), short, "too short to trim");
    }

    #[test]
    fn corners_are_the_turns_the_path_takes() {
        let route = [p(0.0, 0.0), p(100.0, 50.0)];
        assert_eq!(corners(&route, Some("elbow"), None, [None, None]), vec![p(0.0, 0.0), p(50.0, 0.0), p(50.0, 50.0), p(100.0, 50.0)]);
        assert_eq!(corners(&route, None, None, [None, None]), route.to_vec());
        let bent = [p(0.0, 0.0), p(0.0, 0.0), p(50.0, 0.0), p(80.0, 0.0), p(80.0, 50.0)];
        assert_eq!(corners(&bent, Some("elbow"), None, [None, None]), vec![p(0.0, 0.0), p(80.0, 0.0), p(80.0, 50.0)]);
        assert_eq!(corners(&route, Some("elbow"), Some("mind-map-branch"), [None, None]), route.to_vec());
        assert!(corners(&[], None, None, [None, None]).is_empty());
    }

    #[test]
    fn labels_sit_along_the_route() {
        let route = [p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0)];
        assert_eq!(along(&route, 0.5), Some(p(100.0, 0.0)));
        assert_eq!(along(&route, 0.25), Some(p(50.0, 0.0)));
        assert_eq!(along(&route, f64::NAN), Some(p(100.0, 0.0)));
        assert_eq!(along(&[], 0.5), None);
    }

    #[test]
    fn strokes_are_smoothed_through_midpoints() {
        assert_eq!(smooth(&[p(0.0, 0.0), p(10.0, 0.0), p(20.0, 10.0)]).unwrap().d(), "M 0 0 Q 10 0 15 5 L 20 10");
        assert_eq!(smooth(&[p(1.0, 2.0)]).unwrap().d(), "M 1 2 L 1 2");
    }
}
