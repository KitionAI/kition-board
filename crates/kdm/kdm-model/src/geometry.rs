//! Board geometry in board units (1 unit = 1 CSS pixel at 100%): points, sizes, boxes, rotation
//! about a box centre, and the derived point of a bound connector end (design note 1.4).
//!
//! Ported from Kition's `packages/board-editor/src/lib/boardBindingEngine.ts`
//! (`getBoardConnectorAnchor`, `resolveBoardConnectorAnchor`) and `whiteboardGeometry.ts`
//! (`rotateWhiteboardPoint`).

use serde_json::{Value, json};

/// A point in board units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

fn finite(value: &Value) -> Option<f64> {
    value.as_f64().filter(|v| v.is_finite())
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// `{ "x": n, "y": n }` (the `position` register).
    pub fn parse_object(value: &Value) -> Option<Point> {
        Some(Point { x: finite(value.get("x")?)?, y: finite(value.get("y")?)? })
    }

    /// `[x, y]` (points, waypoints, anchors).
    pub fn parse_pair(value: &Value) -> Option<Point> {
        let pair = value.as_array()?;
        if pair.len() != 2 {
            return None;
        }
        Some(Point { x: finite(pair.first()?)?, y: finite(pair.get(1)?)? })
    }

    /// Either form.
    pub fn parse_any(value: &Value) -> Option<Point> {
        Self::parse_pair(value).or_else(|| Self::parse_object(value))
    }

    /// `[[x, y], ...]`; entries that are not points are skipped.
    pub fn parse_list(value: &Value) -> Vec<Point> {
        value.as_array().map(|items| items.iter().filter_map(Point::parse_pair).collect()).unwrap_or_default()
    }

    pub fn to_object(self) -> Value {
        json!({ "x": self.x, "y": self.y })
    }

    pub fn to_pair(self) -> Value {
        json!([self.x, self.y])
    }

    pub fn list_json(points: &[Point]) -> Value {
        Value::Array(points.iter().map(|point| point.to_pair()).collect())
    }

    #[must_use]
    pub fn offset(self, dx: f64, dy: f64) -> Point {
        Point { x: self.x + dx, y: self.y + dy }
    }

    /// The point rotated by `degrees` (clockwise on screen, y down) about `origin`.
    #[must_use]
    pub fn rotate(self, origin: Point, degrees: f64) -> Point {
        if degrees == 0.0 || !degrees.is_finite() {
            return self;
        }
        let radians = degrees.to_radians();
        let (sine, cosine) = radians.sin_cos();
        let (x, y) = (self.x - origin.x, self.y - origin.y);
        Point { x: origin.x + x * cosine - y * sine, y: origin.y + x * sine + y * cosine }
    }
}

/// A size in board units, both ≥ 0.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub w: f64,
    pub h: f64,
}

impl Size {
    pub const fn new(w: f64, h: f64) -> Self {
        Self { w, h }
    }

    /// `{ "w": n, "h": n }` (the `size` register); negative sizes read as 0.
    pub fn parse(value: &Value) -> Option<Size> {
        Some(Size { w: finite(value.get("w")?)?.max(0.0), h: finite(value.get("h")?)?.max(0.0) })
    }

    pub fn to_json(self) -> Value {
        json!({ "w": self.w, "h": self.h })
    }
}

/// An axis-aligned box `[x0, y0, x1, y1]` with `x0 ≤ x1`, `y0 ≤ y1`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    /// The box spanned by two corners in any order.
    pub fn new(ax: f64, ay: f64, bx: f64, by: f64) -> Self {
        Self { x0: ax.min(bx), y0: ay.min(by), x1: ax.max(bx), y1: ay.max(by) }
    }

    pub fn of_points(points: &[Point]) -> Option<Rect> {
        let first = points.first()?;
        Some(points.iter().fold(Rect::new(first.x, first.y, first.x, first.y), |rect, point| rect.include(*point)))
    }

    #[must_use]
    pub fn include(self, point: Point) -> Rect {
        Rect { x0: self.x0.min(point.x), y0: self.y0.min(point.y), x1: self.x1.max(point.x), y1: self.y1.max(point.y) }
    }

    #[must_use]
    pub fn union(self, other: Rect) -> Rect {
        Rect { x0: self.x0.min(other.x0), y0: self.y0.min(other.y0), x1: self.x1.max(other.x1), y1: self.y1.max(other.y1) }
    }

    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    pub fn center(&self) -> Point {
        Point { x: (self.x0 + self.x1) / 2.0, y: (self.y0 + self.y1) / 2.0 }
    }

    pub fn corners(&self) -> [Point; 4] {
        [Point::new(self.x0, self.y0), Point::new(self.x1, self.y0), Point::new(self.x1, self.y1), Point::new(self.x0, self.y1)]
    }

    /// The axis-aligned box around this box rotated by `degrees` about its centre.
    #[must_use]
    pub fn rotated_bounds(self, degrees: f64) -> Rect {
        if degrees == 0.0 || !degrees.is_finite() {
            return self;
        }
        let center = self.center();
        let corners = self.corners().map(|corner| corner.rotate(center, degrees));
        Rect::of_points(&corners).unwrap_or(self)
    }

    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.x0 && point.x <= self.x1 && point.y >= self.y0 && point.y <= self.y1
    }

    /// Whether `other` lies inside this box (edges included).
    pub fn contains_rect(&self, other: &Rect) -> bool {
        other.x0 >= self.x0 && other.y0 >= self.y0 && other.x1 <= self.x1 && other.y1 <= self.y1
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x0 < other.x1 && other.x0 < self.x1 && self.y0 < other.y1 && other.y0 < self.y1
    }

    pub fn to_json(self) -> Value {
        json!([self.x0, self.y0, self.x1, self.y1])
    }
}

/// An anchor clamped to [0, 1] on both axes (rule V7); non-finite values become 0.5.
pub fn clamp_anchor(anchor: [f64; 2]) -> [f64; 2] {
    anchor.map(|v| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.5 })
}

/// The world point of `anchor` (normalised to the unrotated `rect`) on a box rotated by
/// `rotation` degrees about its centre.
pub fn anchor_point(rect: Rect, rotation: f64, anchor: [f64; 2]) -> Point {
    let [ax, ay] = clamp_anchor(anchor);
    let local = Point { x: rect.x0 + rect.width() * ax, y: rect.y0 + rect.height() * ay };
    local.rotate(rect.center(), rotation)
}

/// The anchor on the edge of `rect` (rotated by `rotation`) that faces the world point `point`,
/// and that edge point in world coordinates: where a connector toward `point` binds. Outside the
/// box it is where the line from the centre to `point` leaves the box; inside, the nearest edge
/// (as Kition's TypeScript board did for drops).
pub fn nearest_edge_anchor(rect: Rect, rotation: f64, point: Point) -> ([f64; 2], Point) {
    let center = rect.center();
    let local = point.rotate(center, -rotation);
    let (dx, dy) = (local.x - center.x, local.y - center.y);
    let (half_w, half_h) = (rect.width() / 2.0, rect.height() / 2.0);
    if !rect.contains(local) && (dx != 0.0 || dy != 0.0) {
        let tx = if dx != 0.0 { half_w / dx.abs() } else { f64::INFINITY };
        let ty = if dy != 0.0 { half_h / dy.abs() } else { f64::INFINITY };
        let t = tx.min(ty);
        let edge = Point::new(center.x + dx * t, center.y + dy * t);
        let anchor = [
            if rect.width() > 0.0 { (edge.x - rect.x0) / rect.width() } else { 0.5 },
            if rect.height() > 0.0 { (edge.y - rect.y0) / rect.height() } else { 0.5 },
        ];
        return (clamp_anchor(anchor), edge.rotate(center, rotation));
    }
    let clamped = Point { x: local.x.clamp(rect.x0, rect.x1), y: local.y.clamp(rect.y0, rect.y1) };
    let candidates = [
        ((local.x - rect.x1).abs(), Point::new(rect.x1, clamped.y)),
        ((local.x - rect.x0).abs(), Point::new(rect.x0, clamped.y)),
        ((local.y - rect.y1).abs(), Point::new(clamped.x, rect.y1)),
        ((local.y - rect.y0).abs(), Point::new(clamped.x, rect.y0)),
    ];
    let edge = candidates.iter().fold(None::<(f64, Point)>, |best, (distance, point)| match best {
        Some((best_distance, _)) if best_distance <= *distance => best,
        _ => Some((*distance, *point)),
    });
    let edge = edge.map_or(center, |(_, point)| point);
    let anchor = [
        if rect.width() > 0.0 { (edge.x - rect.x0) / rect.width() } else { 0.5 },
        if rect.height() > 0.0 { (edge.y - rect.y0) / rect.height() } else { 0.5 },
    ];
    (clamp_anchor(anchor), edge.rotate(center, rotation))
}

/// The size a `text` element's letters have when it sets none.
pub const DEFAULT_TEXT_SIZE: f64 = kition_board_text::TEXT_SIZE;

/// The size of a `text` element without a stored size: its lines measured with the registered
/// fonts (`kition-board-text`), unwrapped, at `font_size` (the text default when absent).
pub fn auto_text_size(text: &str, font_size: Option<f64>, family: Option<&str>) -> Size {
    use kition_board_text::{Family, REGULAR, Style, TEXT_SIZE, block};
    let style = Style::new(font_size.unwrap_or(TEXT_SIZE), REGULAR, Family::parse(family));
    let (w, h) = block(text, style, None);
    Size { w, h }
}

/// [`auto_text_size`] for styled text (marks, Markdown): its runs laid out unwrapped.
pub fn styled_text_size(paragraphs: &[kition_board_text::Paragraph], font_size: Option<f64>, family: Option<&str>) -> Size {
    use kition_board_text::{Family, REGULAR, Style, TEXT_SIZE, rich_block};
    let style = Style::new(font_size.unwrap_or(TEXT_SIZE), REGULAR, Family::parse(family));
    let (w, h) = rich_block(paragraphs, style, None);
    Size { w, h }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
    }

    #[test]
    fn anchors_resolve_on_the_unrotated_and_the_rotated_box() {
        let rect = Rect::new(0.0, 0.0, 100.0, 50.0);
        assert!(close(anchor_point(rect, 0.0, [1.0, 0.5]), Point::new(100.0, 25.0)));
        assert!(close(anchor_point(rect, 90.0, [1.0, 0.5]), Point::new(50.0, 75.0)), "a quarter turn moves the right middle below the centre");
        assert!(close(anchor_point(rect, 0.0, [2.0, -1.0]), Point::new(100.0, 0.0)), "anchors are clamped");
    }

    #[test]
    fn the_nearest_edge_anchor_round_trips_through_anchor_point() {
        let rect = Rect::new(10.0, 10.0, 110.0, 60.0);
        for rotation in [0.0, 30.0, 90.0, 200.0] {
            let (anchor, point) = nearest_edge_anchor(rect, rotation, Point::new(500.0, 40.0));
            assert!(close(anchor_point(rect, rotation, anchor), point));
            assert!(anchor.iter().all(|v| (0.0..=1.0).contains(v)));
        }
        let (anchor, _) = nearest_edge_anchor(rect, 0.0, Point::new(120.0, 35.0));
        assert_eq!(anchor, [1.0, 0.5], "a point right of the box binds to the right edge");
        let (anchor, _) = nearest_edge_anchor(rect, 0.0, Point::new(60.0, 500.0));
        assert_eq!(anchor, [0.5, 1.0], "a point far below binds to the bottom edge");
        let (anchor, _) = nearest_edge_anchor(rect, 0.0, Point::new(105.0, 20.0));
        assert_eq!(anchor, [1.0, 0.2], "inside, the nearest edge");
    }

    #[test]
    fn rotated_bounds_cover_the_corners() {
        let rect = Rect::new(0.0, 0.0, 100.0, 50.0).rotated_bounds(90.0);
        assert!((rect.width() - 50.0).abs() < 1e-9 && (rect.height() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn parsing_refuses_non_finite_and_malformed_values() {
        assert_eq!(Point::parse_pair(&json!([1, 2])), Some(Point::new(1.0, 2.0)));
        assert_eq!(Point::parse_pair(&json!([1])), None);
        assert_eq!(Point::parse_object(&json!({ "x": 1 })), None);
        assert_eq!(Point::parse_list(&json!([[1, 2], "x", [3, 4, 5], [5, 6]])), vec![Point::new(1.0, 2.0), Point::new(5.0, 6.0)]);
        assert_eq!(Size::parse(&json!({ "w": -4, "h": 3 })), Some(Size::new(0.0, 3.0)));
    }
}
