//! The 18 shapes of `shapeType`, drawn in the element's local frame (origin at its top-left
//! corner, `w` × `h`, unrotated).
//!
//! Ported from Kition's `packages/board-editor/src/components/WhiteboardShapeBody.tsx`
//! (`WhiteboardShapeGeometry`, `createStarPoints`, `createCloudPath`, `createHeartPath`) and the
//! shape definitions of `boardElementDefinitions.ts`.

use crate::svg::num;

/// Whether a shape is filled and labelled (`line` and `check` are neither).
pub fn supports_fill(shape: &str) -> bool {
    !matches!(shape, "line" | "check")
}

pub fn supports_label(shape: &str) -> bool {
    !matches!(shape, "line" | "check")
}

fn points(list: &[(f64, f64)]) -> String {
    list.iter().map(|(x, y)| format!("{},{}", num(*x), num(*y))).collect::<Vec<_>>().join(" ")
}

fn polygon(out: &mut String, list: &[(f64, f64)], paint: &str) {
    out.push_str(&format!(r#"<polygon points="{}" {paint}/>"#, points(list)));
}

fn path(out: &mut String, d: &str, paint: &str) {
    out.push_str(&format!(r#"<path d="{d}" {paint}/>"#));
}

fn rect(out: &mut String, w: f64, h: f64, rx: f64, paint: &str) {
    out.push_str(&format!(r#"<rect width="{}" height="{}" rx="{}" {paint}/>"#, num(w), num(h), num(rx.max(0.0))));
}

/// How an outline is painted.
pub struct Paint<'a> {
    /// The fill and stroke attributes.
    pub body: &'a str,
    /// The stroke attributes of the inner marks of `x-box` and `check-box`, and of open shapes.
    pub inner: &'a str,
    /// The rectangle's corner radius when the element sets `roundness`.
    pub corner: Option<f64>,
    /// Sticky notes have small corners.
    pub sticky: bool,
}

/// Writes the outline of `shape` (an unknown shape is a rectangle), `w` × `h`.
pub fn outline(out: &mut String, shape: &str, w: f64, h: f64, look: &Paint<'_>) {
    let (paint, inner, corner, sticky) = (look.body, look.inner, look.corner, look.sticky);
    let (cx, cy) = (w / 2.0, h / 2.0);
    match shape {
        "ellipse" => out.push_str(&format!(r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}" {paint}/>"#, num(cx), num(cy), num(cx), num(cy))),
        "triangle" => polygon(out, &[(cx, 0.0), (w, h), (0.0, h)], paint),
        "diamond" => polygon(out, &[(cx, 0.0), (w, cy), (cx, h), (0.0, cy)], paint),
        "hexagon" => {
            let inset = w * 0.24;
            polygon(out, &[(inset, 0.0), (w - inset, 0.0), (w, cy), (w - inset, h), (inset, h), (0.0, cy)], paint);
        }
        "parallelogram" => {
            let inset = (w * 0.24).min(h * 0.6);
            polygon(out, &[(inset, 0.0), (w, 0.0), (w - inset, h), (0.0, h)], paint);
        }
        "star" => {
            let list: Vec<(f64, f64)> = (0..10)
                .map(|index| {
                    let radius = if index % 2 == 0 { 1.0 } else { 0.43 };
                    let angle = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * f64::from(index) / 5.0;
                    (cx + angle.cos() * cx * radius, cy + angle.sin() * cy * radius)
                })
                .collect();
            polygon(out, &list, paint);
        }
        "cloud" => {
            let p = |fx: f64, fy: f64| format!("{} {}", num(w * fx), num(h * fy));
            let d = format!(
                "M {} C {}, {}, {} C {}, {}, {} C {}, {}, {} C {}, {}, {} L {} Z",
                p(0.2, 0.72),
                p(0.04, 0.72),
                p(0.02, 0.47),
                p(0.18, 0.4),
                p(0.17, 0.18),
                p(0.43, 0.1),
                p(0.55, 0.28),
                p(0.72, 0.15),
                p(0.93, 0.31),
                p(0.87, 0.51),
                p(1.03, 0.61),
                p(0.91, 0.83),
                p(0.75, 0.79),
                p(0.2, 0.79),
            );
            path(out, &d, paint);
        }
        "heart" => {
            let p = |fx: f64, fy: f64| format!("{} {}", num(w * fx), num(h * fy));
            let d = format!(
                "M {} C {}, {}, {} C {}, {}, {} C {}, {}, {} C {}, {}, {} Z",
                p(0.5, 0.9),
                p(0.42, 0.79),
                p(0.08, 0.58),
                p(0.08, 0.3),
                p(0.08, 0.02),
                p(0.4, -0.02),
                p(0.5, 0.22),
                p(0.6, -0.02),
                p(0.92, 0.02),
                p(0.92, 0.3),
                p(0.92, 0.58),
                p(0.58, 0.79),
                p(0.5, 0.9),
            );
            path(out, &d, paint);
        }
        "x-box" => {
            rect(out, w, h, 0.0, paint);
            let i = 1.0_f64.min(cx).min(cy);
            path(
                out,
                &format!("M {} {} L {} {} M {} {} L {} {}", num(i), num(i), num(w - i), num(h - i), num(w - i), num(i), num(i), num(h - i)),
                inner,
            );
        }
        "check-box" => {
            rect(out, w, h, 0.0, paint);
            let size = w.min(h) * 0.82;
            let (ox, oy) = ((w - size) / 2.0, (h - size) / 2.0);
            let d = format!(
                "M {} {} L {} {} L {} {}",
                num(ox + size * 0.25),
                num(oy + size * 0.52),
                num(ox + size * 0.45),
                num(oy + size * 0.82),
                num(ox + size * 0.82),
                num(oy + size * 0.22)
            );
            path(out, &d, inner);
        }
        "check" => {
            let d = format!("M {} {} L {} {} L {} {}", num(w * 0.14), num(h * 0.53), num(w * 0.4), num(h * 0.8), num(w * 0.87), num(h * 0.2));
            path(out, &d, inner);
        }
        "arrow-left" => {
            polygon(out, &[(0.0, cy), (w * 0.38, 0.0), (w * 0.38, h * 0.3), (w, h * 0.3), (w, h * 0.7), (w * 0.38, h * 0.7), (w * 0.38, h)], paint)
        }
        "arrow-right" => {
            polygon(out, &[(w, cy), (w * 0.62, 0.0), (w * 0.62, h * 0.3), (0.0, h * 0.3), (0.0, h * 0.7), (w * 0.62, h * 0.7), (w * 0.62, h)], paint)
        }
        "arrow-up" => {
            polygon(out, &[(cx, 0.0), (w, h * 0.38), (w * 0.7, h * 0.38), (w * 0.7, h), (w * 0.3, h), (w * 0.3, h * 0.38), (0.0, h * 0.38)], paint)
        }
        "arrow-down" => {
            polygon(out, &[(cx, h), (w, h * 0.62), (w * 0.7, h * 0.62), (w * 0.7, 0.0), (w * 0.3, 0.0), (w * 0.3, h * 0.62), (0.0, h * 0.62)], paint)
        }
        // Along the box's longer side, through its middle: a tall box draws a vertical line.
        "line" if h > w => out.push_str(&format!(r#"<line x1="{}" y1="0" x2="{}" y2="{}" {inner}/>"#, num(cx), num(cx), num(h))),
        "line" => out.push_str(&format!(r#"<line x1="0" y1="{}" x2="{}" y2="{}" {inner}/>"#, num(cy), num(w), num(cy))),
        "pill" => rect(out, w, h, cy, paint),
        _ => {
            let default = if sticky { 4.0 } else { 12.0_f64.min(w / 6.0).min(h / 6.0) };
            rect(out, w, h, corner.unwrap_or(default).min(cx).min(cy), paint);
        }
    }
}
