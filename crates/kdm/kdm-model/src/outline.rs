//! The outlines of the polygon shapes, as fractions of the element's unrotated box. The renderer
//! draws them (`kition-board-scene` `shapes.rs`), so a shape and anything that reads its outline
//! from here never disagree.
//!
//! `hexagon` and `parallelogram` are polygons too but are not in the table: the hexagon's inset is
//! drawn as `w - w * 0.24`, which `w * 0.76` does not always round to the same number, and the
//! parallelogram's inset is `min(w * 0.24, h * 0.6)`, not a fraction of the box.

/// A point of an outline: `(x, y)` as fractions of the box's width and height.
pub type Frac = (f64, f64);

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
    }
}
