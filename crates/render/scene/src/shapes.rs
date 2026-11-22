//! The 44 shapes of `shapeType`, drawn in the element's local frame (origin at its top-left
//! corner, `w` × `h`, unrotated). The polygons' outlines come from one table,
//! `kition_board_kdm::outline`.
//!
//! The first 18 are ported from Kition's `packages/board-editor/src/components/WhiteboardShapeBody.tsx`
//! (`WhiteboardShapeGeometry`, `createStarPoints`, `createCloudPath`, `createHeartPath`) and the
//! shape definitions of `boardElementDefinitions.ts`; the flowchart shapes follow the ISO 5807
//! symbols.

use crate::svg::num;

/// Whether a shape is filled: the open shapes (`line`, `check`, the braces and the actor) are not.
pub fn supports_fill(shape: &str) -> bool {
    !matches!(shape, "line" | "check" | "brace-left" | "brace-right" | "actor")
}

/// Whether a shape is labelled (`line` and `check` are not).

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

/// The part of an actor's box its figure takes, from the top; its label goes below.
const ACTOR_HEIGHT: f64 = 0.72;

/// How wide a brace is drawn in a box `w` × `h`: the whole box when it is narrow, else 0.3 of its
/// height.
fn brace_width(w: f64, h: f64) -> f64 {
    w.min(h * 0.3)
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
    if let Some(list) = kition_board_kdm::outline::polygon(shape) {
        let scaled: Vec<(f64, f64)> = list.iter().map(|(fx, fy)| (w * fx, h * fy)).collect();
        polygon(out, &scaled, paint);
        return;
    }
    // "x y" at the fractions (fx, fy) of the box.
    let p = |fx: f64, fy: f64| format!("{} {}", num(w * fx), num(h * fy));
    match shape {
        "ellipse" => out.push_str(&format!(r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}" {paint}/>"#, num(cx), num(cy), num(cx), num(cy))),
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
        "document" => path(
            out,
            &format!("M 0 0 L {} L {} Q {} {} Q {} {} Z", p(1.0, 0.0), p(1.0, 0.85), p(0.75, 0.7), p(0.5, 0.85), p(0.25, 1.0), p(0.0, 0.85)),
            paint,
        ),
        "multi-document" => {
            // Two sheets behind the front one, each 88% × 84% of the box, offset up and right. Only
            // their top and right edges show: their paths stay open, and what their fill covers
            // beyond those edges lies under the sheet in front.
            path(out, &format!("M {} L {} L {} L {} L {}", p(0.12, 0.08), p(0.12, 0.0), p(1.0, 0.0), p(1.0, 0.714), p(0.94, 0.714)), paint);
            path(out, &format!("M {} L {} L {} L {} L {}", p(0.06, 0.16), p(0.06, 0.08), p(0.94, 0.08), p(0.94, 0.794), p(0.88, 0.794)), paint);
            path(
                out,
                &format!(
                    "M {} L {} L {} Q {} {} Q {} {} Z",
                    p(0.0, 0.16),
                    p(0.88, 0.16),
                    p(0.88, 0.874),
                    p(0.66, 0.748),
                    p(0.44, 0.874),
                    p(0.22, 1.0),
                    p(0.0, 0.874)
                ),
                paint,
            );
        }
        "predefined-process" => {
            rect(out, w, h, 0.0, paint);
            path(out, &format!("M {} L {} M {} L {}", p(0.12, 0.0), p(0.12, 1.0), p(0.88, 0.0), p(0.88, 1.0)), inner);
        }
        "internal-storage" => {
            rect(out, w, h, 0.0, paint);
            let i = w.min(h) * 0.18;
            path(out, &format!("M {} 0 L {} {} M 0 {} L {} {}", num(i), num(i), num(h), num(i), num(w), num(i)), inner);
        }
        "cylinder" => {
            // The top is a whole ellipse; the bottom shows its front half.
            let ry = (h * 0.15).min(w * 0.25);
            let (top, low, rx, ry, w) = (num(ry), num(h - ry), num(cx), num(ry), num(w));
            path(out, &format!("M 0 {top} L 0 {low} A {rx} {ry} 0 0 0 {w} {low} L {w} {top} A {rx} {ry} 0 0 0 0 {top} Z"), paint);
            path(out, &format!("M 0 {top} A {rx} {ry} 0 0 0 {w} {top}"), inner);
        }
        "delay" => path(out, &format!("M 0 0 L {} C {}, {}, {} L {} Z", p(0.6, 0.0), p(1.13, 0.0), p(1.13, 1.0), p(0.6, 1.0), p(0.0, 1.0)), paint),
        "display" => path(
            out,
            &format!("M {} L {} Q {} {} L {} L {} Z", p(0.18, 0.0), p(0.85, 0.0), p(1.15, 0.5), p(0.85, 1.0), p(0.18, 1.0), p(0.0, 0.5)),
            paint,
        ),
        "stored-data" => path(
            out,
            &format!(
                "M {} L {} Q {} {} L {} Q {} {} Z",
                p(0.15, 0.0),
                p(1.0, 0.0),
                p(0.7, 0.5),
                p(1.0, 1.0),
                p(0.15, 1.0),
                p(-0.15, 0.5),
                p(0.15, 0.0)
            ),
            paint,
        ),
        "summing-junction" | "or-junction" => {
            out.push_str(&format!(r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}" {paint}/>"#, num(cx), num(cy), num(cx), num(cy)));
            let d = if shape == "or-junction" {
                format!("M {} 0 L {} {} M 0 {} L {} {}", num(cx), num(cx), num(h), num(cy), num(w), num(cy))
            } else {
                let (dx, dy) = (cx * std::f64::consts::FRAC_1_SQRT_2, cy * std::f64::consts::FRAC_1_SQRT_2);
                format!(
                    "M {} {} L {} {} M {} {} L {} {}",
                    num(cx - dx),
                    num(cy - dy),
                    num(cx + dx),
                    num(cy + dy),
                    num(cx + dx),
                    num(cy - dy),
                    num(cx - dx),
                    num(cy + dy)
                )
            };
            path(out, &d, inner);
        }
        "note" => {
            // A sheet with its top right corner folded down.
            let f = w.min(h) * 0.25;
            polygon(out, &[(0.0, 0.0), (w - f, 0.0), (w, f), (w, h), (0.0, h)], paint);
            path(out, &format!("M {} 0 L {} {} L {} {}", num(w - f), num(w - f), num(f), num(w), num(f)), inner);
        }
        "brace-left" | "brace-right" => {
            // The left brace opens to the right, along the box's left side; the right brace is its
            // mirror. A wide box keeps the brace's proportions and leaves the rest to the label.
            let bw = brace_width(w, h);
            let q = |fx: f64, fy: f64| {
                let x = if shape == "brace-left" { bw * fx } else { w - bw * fx };
                format!("{} {}", num(x), num(h * fy))
            };
            let d = format!(
                "M {} Q {} {} L {} Q {} {} Q {} {} L {} Q {} {}",
                q(1.0, 0.0),
                q(0.5, 0.0),
                q(0.5, 0.2),
                q(0.5, 0.4),
                q(0.5, 0.5),
                q(0.0, 0.5),
                q(0.5, 0.5),
                q(0.5, 0.6),
                q(0.5, 0.8),
                q(0.5, 1.0),
                q(1.0, 1.0)
            );
            path(out, &d, inner);
        }
        "actor" => {
            // A stick figure in the top of the box, at most 0.6 times as wide as it is tall; the
            // label goes under it (`label_box`).
            let fh = h * ACTOR_HEIGHT;
            let fw = w.min(fh * 0.6);
            let x = |fx: f64| num(cx + fw * (fx - 0.5));
            let y = |fy: f64| num(fh * fy);
            let r = (fw * 0.25).min(fh * 0.125);
            out.push_str(&format!(r#"<circle cx="{}" cy="{}" r="{}" {inner}/>"#, num(cx), num(r), num(r)));
            let d = format!(
                "M {} {} L {} {} M {} {} L {} {} M {} {} L {} {} M {} {} L {} {}",
                num(cx),
                num(2.0 * r),
                x(0.5),
                y(0.65),
                x(0.1),
                y(0.38),
                x(0.9),
                y(0.38),
                x(0.5),
                y(0.65),
                x(0.1),
                y(1.0),
                x(0.5),
                y(0.65),
                x(0.9),
                y(1.0)
            );
            path(out, &d, inner);
        }
        _ => {
            let default = if sticky { 4.0 } else { 12.0_f64.min(w / 6.0).min(h / 6.0) };
            rect(out, w, h, corner.unwrap_or(default).min(cx).min(cy), paint);
        }
    }
}
