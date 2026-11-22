//! The outlines of the shapes, as fractions of the element's unrotated box. The renderer draws the
//! polygons of [`POLYGONS`] (`kition-board-scene` `shapes.rs`), and a bound connector end is
//! derived on the outline ([`outline_point`], design note 1.4), so a shape and the ends that meet
//! it never disagree.
//!
//! `hexagon` and `parallelogram` are polygons too but are not in the table: the hexagon's inset is
//! drawn as `w - w * 0.24`, which `w * 0.76` does not always round to the same number, and the
//! parallelogram's inset is `min(w * 0.24, h * 0.6)`, not a fraction of the box. Their drawing
//! keeps its own arm; for ends, [`outline_of`] describes both ([`HEXAGON`], [`SLANT`]).

use kition_kit::kdm::{ObjectId, Tx};
use serde_json::Value;

use crate::Kind;
use crate::geometry::{Point, Rect, anchor_point, clamp_anchor};

/// A point of an outline: `(x, y)` as fractions of the box's width and height.
pub type Frac = (f64, f64);

/// The shapes whose outline is the ellipse inscribed in their box.
pub const ELLIPSES: &[&str] = &["ellipse", "summing-junction", "or-junction"];

/// The hexagon's outline (inset `w * 0.24` on both sides, as drawn), clockwise from the top. For
/// the point where a ray leaves it, `w * 0.76` and the drawing's `w - w * 0.24` are the same
/// number to the last bit or two.
pub const HEXAGON: &[Frac] = &[(0.24, 0.0), (0.76, 0.0), (1.0, 0.5), (0.76, 1.0), (0.24, 1.0), (0.0, 0.5)];

/// The parallelogram's inset as drawn: `min(w * SLANT.0, h * SLANT.1)`.
pub const SLANT: (f64, f64) = (0.24, 0.6);

/// The most corners an outline has (`cross`).
pub const MAX_CORNERS: usize = 12;

/// What a bound connector end meets on an element: the outline of its shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outline {
    /// The element's box: rectangles, every shape not named below, and every other kind.
    Box,
    /// The ellipse inscribed in the box ([`ELLIPSES`]).
    Ellipse,
    /// A polygon, as fractions of the box ([`POLYGONS`] and [`HEXAGON`]).
    Polygon(&'static [Frac]),
    /// A parallelogram leaning right: corners `(i, 0)`, `(w, 0)`, `(w - i, h)`, `(0, h)` with the
    /// inset `i = min(w * fx, h * fy)` ([`SLANT`]).
    Slant(f64, f64),
}

/// The outline of `shape` (a `shapeType`); `None`, an element that is not a shape, is the box.
pub fn outline_of(shape: Option<&str>) -> Outline {
    match shape {
        None => Outline::Box,
        Some(name) if ELLIPSES.contains(&name) => Outline::Ellipse,
        Some("hexagon") => Outline::Polygon(HEXAGON),
        Some("parallelogram") => Outline::Slant(SLANT.0, SLANT.1),
        Some(name) => polygon(name).map_or(Outline::Box, Outline::Polygon),
    }
}

/// The corners of a polygon outline in a box `w` × `h` (origin at its top-left), without
/// allocating.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Corners {
    points: [Frac; MAX_CORNERS],
    len: usize,
}

impl Corners {
    /// The corners, clockwise from the top.
    pub fn points(&self) -> &[Frac] {
        self.points.get(..self.len).unwrap_or(&[])
    }
}

/// The polygon `outline` is in a box `w` × `h`, in box units: the table's fractions scaled, the
/// parallelogram with its real inset. `None` for the box and the ellipse.
pub fn corners(outline: Outline, w: f64, h: f64) -> Option<Corners> {
    let mut out = Corners { points: [(0.0, 0.0); MAX_CORNERS], len: 0 };
    match outline {
        Outline::Box | Outline::Ellipse => return None,
        Outline::Polygon(list) => {
            if list.len() > MAX_CORNERS {
                return None;
            }
            for (slot, (fx, fy)) in out.points.iter_mut().zip(list) {
                *slot = (w * fx, h * fy);
            }
            out.len = list.len();
        }
        Outline::Slant(fx, fy) => {
            let inset = (w * fx).min(h * fy);
            for (slot, point) in out.points.iter_mut().zip([(inset, 0.0), (w, 0.0), (w - inset, h), (0.0, h)]) {
                *slot = point;
            }
            out.len = 4;
        }
    }
    Some(out)
}

/// Where a bound connector end is on a target (design note 1.4): the point where the ray from the
/// centre of the unrotated `rect` through `anchor`'s point leaves the outline of `shape` (the
/// target's `shapeType` when it is a shape, else `None`), turned by `rotation` degrees about the
/// centre. The box's own outline, an anchor at the centre, a box with a zero side, or anything not
/// finite answer [`anchor_point`], so rectangles and every other kind resolve as they always did.
pub fn outline_point(shape: Option<&str>, rect: Rect, rotation: f64, anchor: [f64; 2]) -> Point {
    let outline = outline_of(shape);
    if outline == Outline::Box {
        return anchor_point(rect, rotation, anchor);
    }
    let (w, h) = (rect.width(), rect.height());
    let [ax, ay] = clamp_anchor(anchor);
    let center = rect.center();
    let (dx, dy) = (rect.x0 + w * ax - center.x, rect.y0 + h * ay - center.y);
    let usable = w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite() && (dx != 0.0 || dy != 0.0);
    let t = if usable { leave(outline, w, h, dx, dy) } else { None };
    match t.map(|t| Point::new(center.x + dx * t, center.y + dy * t)) {
        Some(local) if local.x.is_finite() && local.y.is_finite() => local.rotate(center, rotation),
        _ => anchor_point(rect, rotation, anchor),
    }
}

/// [`crate::Element::outline_shape`] of `id` as the transaction holds it: its `shapeType` when it is
/// a shape (a command may have just changed it), else `None`.
pub fn outline_shape_in(tx: &Tx<'_>, id: &ObjectId) -> Option<String> {
    if tx.get(id, "kind").ok()?.as_str() != Some(Kind::Shape.as_str()) {
        return None;
    }
    match tx.get(id, "shapeType").ok()? {
        Value::String(shape) => Some(shape),
        _ => None,
    }
}

/// How far along `(dx, dy)` from the centre of a `w` × `h` box the ray leaves `outline` (1 is the
/// anchor's point on the box); `None` when it does not cross it.
fn leave(outline: Outline, w: f64, h: f64, dx: f64, dy: f64) -> Option<f64> {
    if outline == Outline::Ellipse {
        let (ex, ey) = (dx / (w / 2.0), dy / (h / 2.0));
        return Some(1.0 / (ex * ex + ey * ey).sqrt());
    }
    let corners = corners(outline, w, h)?;
    let list = corners.points();
    let (cx, cy) = (w / 2.0, h / 2.0);
    // The outermost crossing: right for concave outlines such as `cross`, and a ray that starts on
    // an edge (the right triangle's centre is on its long side) still leaves through the far one.
    let mut best: Option<f64> = None;
    for (at, &(px, py)) in list.iter().enumerate() {
        let Some(&(qx, qy)) = list.get(at + 1).or_else(|| list.first()) else { continue };
        let (ex, ey) = (qx - px, qy - py);
        let denominator = dx * ey - dy * ex;
        if denominator.abs() <= f64::EPSILON * (dx.abs() + dy.abs()) * (ex.abs() + ey.abs()) {
            continue;
        }
        let (ox, oy) = (px - cx, py - cy);
        let t = (ox * ey - oy * ex) / denominator;
        let s = (ox * dy - oy * dx) / denominator;
        if t >= 0.0 && (-1e-9..=1.0 + 1e-9).contains(&s) && best.is_none_or(|best| t > best) {
            best = Some(t);
        }
    }
    best.filter(|t| t.is_finite())
}

/// Each polygon shape's outline, clockwise from the top.
pub const POLYGONS: &[(&str, &[Frac])] = &[
    ("triangle", &[(0.5, 0.0), (1.0, 1.0), (0.0, 1.0)]),
    ("diamond", &[(0.5, 0.0), (1.0, 0.5), (0.5, 1.0), (0.0, 0.5)]),
    ("pentagon", &[(0.5, 0.0), (1.0, 0.38), (0.81, 1.0), (0.19, 1.0), (0.0, 0.38)]),
    ("octagon", &[(0.29, 0.0), (0.71, 0.0), (1.0, 0.29), (1.0, 0.71), (0.71, 1.0), (0.29, 1.0), (0.0, 0.71), (0.0, 0.29)]),
    ("trapezoid", &[(0.2, 0.0), (0.8, 0.0), (1.0, 1.0), (0.0, 1.0)]),
    ("right-triangle", &[(0.0, 0.0), (1.0, 1.0), (0.0, 1.0)]),
    (
        "cross",
        &[
            (0.33, 0.0),
            (0.67, 0.0),
            (0.67, 0.33),
            (1.0, 0.33),
            (1.0, 0.67),
            (0.67, 0.67),
            (0.67, 1.0),
            (0.33, 1.0),
            (0.33, 0.67),
            (0.0, 0.67),
            (0.0, 0.33),
            (0.33, 0.33),
        ],
    ),
    ("chevron", &[(0.0, 0.0), (0.75, 0.0), (1.0, 0.5), (0.75, 1.0), (0.0, 1.0), (0.25, 0.5)]),
    (
        "arrow-left-right",
        &[(0.0, 0.5), (0.25, 0.0), (0.25, 0.3), (0.75, 0.3), (0.75, 0.0), (1.0, 0.5), (0.75, 1.0), (0.75, 0.7), (0.25, 0.7), (0.25, 1.0)],
    ),
    (
        "arrow-up-down",
        &[(0.5, 0.0), (1.0, 0.25), (0.7, 0.25), (0.7, 0.75), (1.0, 0.75), (0.5, 1.0), (0.0, 0.75), (0.3, 0.75), (0.3, 0.25), (0.0, 0.25)],
    ),
    ("callout", &[(0.0, 0.0), (1.0, 0.0), (1.0, 0.75), (0.45, 0.75), (0.25, 1.0), (0.25, 0.75), (0.0, 0.75)]),
    ("manual-input", &[(0.0, 0.25), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]),
    ("manual-operation", &[(0.0, 0.0), (1.0, 0.0), (0.8, 1.0), (0.2, 1.0)]),
    ("off-page", &[(0.0, 0.0), (1.0, 0.0), (1.0, 0.6), (0.5, 1.0), (0.0, 0.6)]),
];

/// The outline of `shape`, or `None` when it is not a polygon of the table.
pub fn polygon(shape: &str) -> Option<&'static [Frac]> {
    POLYGONS.iter().find(|(name, _)| *name == shape).map(|(_, list)| *list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_outline_is_inside_its_box_and_named_once() {
        for (at, (name, list)) in POLYGONS.iter().enumerate() {
            assert!(list.len() >= 3, "{name} is not a polygon");
            assert!(list.iter().all(|(x, y)| (0.0..=1.0).contains(x) && (0.0..=1.0).contains(y)), "{name} leaves its box");
            assert!(POLYGONS.iter().skip(at + 1).all(|(other, _)| other != name), "{name} is in the table twice");
            assert_eq!(polygon(name), Some(*list));
        }
        assert_eq!(polygon("rectangle"), None);
        assert!(POLYGONS.iter().all(|(_, list)| list.len() <= MAX_CORNERS) && HEXAGON.len() <= MAX_CORNERS);
    }

    fn close(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
    }

    const SQUARE: Rect = Rect { x0: 0.0, y0: 0.0, x1: 100.0, y1: 100.0 };

    #[test]
    fn a_diamond_end_at_the_box_corner_meets_the_diamond() {
        assert!(close(outline_point(Some("diamond"), SQUARE, 0.0, [1.0, 0.0]), Point::new(75.0, 25.0)));
        assert!(close(outline_point(Some("diamond"), SQUARE, 0.0, [1.0, 0.5]), Point::new(100.0, 50.0)), "a side's middle is a corner");
        // Turned a quarter: (75, 25) about (50, 50) is (75, 75).
        assert!(close(outline_point(Some("diamond"), SQUARE, 90.0, [1.0, 0.0]), Point::new(75.0, 75.0)));
    }

    #[test]
    fn an_ellipse_end_is_on_the_ellipse() {
        let rect = Rect::new(0.0, 0.0, 200.0, 100.0);
        assert!(close(outline_point(Some("ellipse"), rect, 0.0, [1.0, 0.5]), Point::new(200.0, 50.0)));
        let corner = outline_point(Some("summing-junction"), rect, 0.0, [1.0, 1.0]);
        let on = ((corner.x - 100.0) / 100.0).powi(2) + ((corner.y - 50.0) / 50.0).powi(2);
        assert!((on - 1.0).abs() < 1e-9, "{corner:?}");
    }

    #[test]
    fn the_box_and_every_other_kind_resolve_as_before() {
        for (shape, rotation, anchor) in [(None, 0.0, [0.3, 0.0]), (Some("rectangle"), 30.0, [0.7, 1.0]), (Some("cloud"), 0.0, [1.0, 0.2])] {
            assert_eq!(outline_point(shape, SQUARE, rotation, anchor), anchor_point(SQUARE, rotation, anchor));
        }
        assert!(close(outline_point(None, SQUARE, 0.0, [0.3, 0.0]), Point::new(30.0, 0.0)));
    }

    #[test]
    fn a_concave_outline_answers_its_outermost_crossing() {
        assert!(close(outline_point(Some("cross"), SQUARE, 0.0, [1.0, 0.5]), Point::new(100.0, 50.0)));
        // Toward the box corner the ray leaves the cross at its inner corner (67, 33).
        assert!(close(outline_point(Some("cross"), SQUARE, 0.0, [1.0, 0.0]), Point::new(67.0, 33.0)));
    }

    #[test]
    fn the_hexagon_and_the_parallelogram_use_their_real_insets() {
        let rect = Rect::new(0.0, 0.0, 200.0, 100.0);
        // The hexagon's top edge runs from (48, 0) to (152, 0).
        assert!(close(outline_point(Some("hexagon"), rect, 0.0, [0.5, 0.0]), Point::new(100.0, 0.0)));
        assert!(close(outline_point(Some("hexagon"), rect, 0.0, [1.0, 0.5]), Point::new(200.0, 50.0)));
        // A flat parallelogram: the inset is h * 0.6 = 12, not w * 0.24 = 48.
        let flat = Rect::new(0.0, 0.0, 200.0, 20.0);
        let corners = corners(outline_of(Some("parallelogram")), 200.0, 20.0).unwrap();
        assert_eq!(corners.points(), &[(12.0, 0.0), (200.0, 0.0), (188.0, 20.0), (0.0, 20.0)]);
        assert!(close(outline_point(Some("parallelogram"), flat, 0.0, [0.0, 0.5]), Point::new(6.0, 10.0)));
    }

    #[test]
    fn the_centre_a_zero_side_and_non_finite_anchors_fall_back_to_the_box() {
        assert_eq!(outline_point(Some("diamond"), SQUARE, 0.0, [0.5, 0.5]), Point::new(50.0, 50.0));
        let flat = Rect::new(10.0, 10.0, 10.0, 60.0);
        assert_eq!(outline_point(Some("diamond"), flat, 0.0, [1.0, 0.0]), anchor_point(flat, 0.0, [1.0, 0.0]));
        for anchor in [[f64::NAN, 0.0], [f64::INFINITY, f64::NEG_INFINITY], [f64::NAN, f64::NAN]] {
            let point = outline_point(Some("star-of-nothing"), SQUARE, f64::NAN, anchor);
            assert!(point.x.is_finite() && point.y.is_finite());
            let point = outline_point(Some("diamond"), SQUARE, 0.0, anchor);
            assert!(point.x.is_finite() && point.y.is_finite(), "{anchor:?}: {point:?}");
        }
        let huge = Rect::new(0.0, 0.0, f64::MAX, f64::MAX);
        let point = outline_point(Some("ellipse"), huge, 0.0, [1.0, 0.0]);
        assert!(point.x.is_finite() && point.y.is_finite(), "{point:?}");
    }

    #[test]
    fn every_outline_answers_a_point_on_its_edge_for_every_anchor() {
        let rect = Rect::new(-40.0, 10.0, 160.0, 130.0);
        let names = POLYGONS.iter().map(|(name, _)| *name).chain(["hexagon", "parallelogram", "ellipse"]);
        for name in names {
            for step in 0..=16 {
                let f = f64::from(step) / 16.0;
                for anchor in [[f, 0.0], [1.0, f], [f, 1.0], [0.0, f]] {
                    let point = outline_point(Some(name), rect, 0.0, anchor);
                    assert!(point.x.is_finite() && point.y.is_finite(), "{name} {anchor:?}");
                    assert!(
                        point.x >= rect.x0 - 1e-9 && point.x <= rect.x1 + 1e-9 && point.y >= rect.y0 - 1e-9 && point.y <= rect.y1 + 1e-9,
                        "{name} {anchor:?}: {point:?}"
                    );
                }
            }
        }
    }
}
