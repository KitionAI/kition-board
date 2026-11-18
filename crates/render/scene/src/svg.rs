//! Writing SVG text: numbers, escaping, base64, and the text block every label uses.

use kition_board_text::{BASELINE, Line, REGULAR, RichLine, Style, css_family};

/// A coordinate or length: at most two decimals, no `-0`, `0` for non-finite values.
pub fn num(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_owned();
    }
    let rounded = (value * 100.0).round() / 100.0;
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    format!("{rounded}")
}

/// Text escaped for an attribute value or character data.
pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            // XML 1.0 has no other control characters.
            ch if ch.is_control() && !matches!(ch, '\t' | '\n' | '\r') => {}
            ch => out.push(ch),
        }
    }
    out
}

/// An id usable inside an SVG `id` (letters, digits, `-`, `_`).
pub fn id_part(id: &str) -> String {
    id.chars().map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' { ch } else { '_' }).collect()
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk.first().copied().unwrap_or(0), chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET.get(((n >> shift) & 63) as usize).copied().unwrap_or(b'A')));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The CSS that makes a browser draw the advances the engine measured (spike S2): no kerning, no
/// ligatures, default optical size, spaces kept.
pub const TEXT_CSS: &str = "font-kerning:none;font-variant-ligatures:none;font-optical-sizing:none;white-space:pre";

/// Where each line of a block goes: its left edge and baseline.
pub struct Placed<'a> {
    pub line: &'a Line,
    pub x: f64,
    pub baseline: f64,
}

/// Lines placed in a box `width` wide from `top`, aligned `align` (`left`, `center`, `right`)
/// inside `pad` on both sides.
pub fn place<'a>(lines: &'a [Line], style: Style, width: f64, pad: f64, top: f64, align: &str) -> Vec<Placed<'a>> {
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let x = match align {
                "left" => pad,
                "right" => width - pad - line.width,
                _ => (width - line.width) / 2.0,
            };
            Placed { line, x, baseline: top + index as f64 * style.line_height() + BASELINE * style.size }
        })
        .collect()
}

/// A `<text>` of placed lines, one `<tspan>` with an explicit `x` and `y` per line. Low detail
/// draws each line as a bar instead (no glyphs to lay out at small zoom).
pub fn text_block(out: &mut String, placed: &[Placed<'_>], style: Style, fill: &str, low: bool) {
    if placed.iter().all(|placed| placed.line.text.trim().is_empty()) {
        return;
    }
    if low {
        for placed in placed.iter().filter(|placed| placed.line.width > 0.0) {
            out.push_str(&format!(
                r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="{fill}" fill-opacity="0.35"/>"#,
                num(placed.x),
                num(placed.baseline - 0.7 * style.size),
                num(placed.line.width),
                num(0.6 * style.size),
                num(0.15 * style.size),
            ));
        }
        return;
    }
    out.push_str(&format!(
        r#"<text font-family="{}" font-size="{}" font-weight="{}" fill="{fill}" xml:space="preserve" style="{TEXT_CSS}">"#,
        esc(&css_family(style.family)),
        num(style.size),
        style.weight,
    ));
    for placed in placed {
        if placed.line.text.is_empty() {
            continue;
        }
        out.push_str(&format!(r#"<tspan x="{}" y="{}">{}</tspan>"#, num(placed.x), num(placed.baseline), esc(&placed.line.text)));
    }
    out.push_str("</text>");
}

/// How far italic text slants: tan 12°, the oblique a browser synthesizes.
const SLANT: f64 = 0.2126;

/// Styled lines placed in a box `width` wide from `top`, aligned `align` inside `pad`: each line's
/// left edge and baseline.
pub fn place_rich(lines: &[RichLine], width: f64, pad: f64, top: f64, align: &str) -> Vec<(f64, f64)> {
    let mut y = top;
    lines
        .iter()
        .map(|line| {
            let x = match align {
                "left" => pad,
                "right" => width - pad - line.width,
                _ => (width - line.width) / 2.0,
            };
            let at = (x, y + line.baseline);
            y += line.height;
            at
        })
        .collect()
}

/// A `tspan` of one styled piece at `(x, baseline)`: its own family, size and weight. A weight
/// other than regular also sets the weight axis, which the raster (usvg) does not take from
/// `font-weight`, so variable faces draw it there as the browser does.
fn piece_span(out: &mut String, text: &str, style: Style, x: f64, baseline: f64) {
    out.push_str(&format!(
        r#"<tspan x="{}" y="{}" font-family="{}" font-size="{}" font-weight="{}""#,
        num(x),
        num(baseline),
        esc(&css_family(style.family)),
        num(style.size),
        style.weight,
    ));
    if style.weight != REGULAR {
        out.push_str(&format!(r#" style="font-variation-settings:'wght' {}""#, style.weight));
    }
    out.push_str(&format!(">{}</tspan>", esc(text)));
}

/// Styled lines at `placed` (from [`place_rich`]): upright pieces in one `<text>`, each italic
/// piece in its own, slanted about its baseline (the browser and the raster draw the same oblique,
/// whatever faces they have). Low detail draws bars, as [`text_block`] does.
pub fn rich_text_block(out: &mut String, lines: &[RichLine], placed: &[(f64, f64)], fill: &str, low: bool) {
    if lines.iter().all(|line| line.pieces.iter().all(|piece| piece.text.trim().is_empty())) {
        return;
    }
    if low {
        for (line, (x, baseline)) in lines.iter().zip(placed) {
            for piece in line.pieces.iter().filter(|piece| piece.width > 0.0) {
                let size = piece.style.size;
                out.push_str(&format!(
                    r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="{fill}" fill-opacity="0.35"/>"#,
                    num(x + piece.x),
                    num(baseline - 0.7 * size),
                    num(piece.width),
                    num(0.6 * size),
                    num(0.15 * size),
                ));
            }
        }
        return;
    }
    let open = format!(r#"<text fill="{fill}" xml:space="preserve" style="{TEXT_CSS}""#);
    out.push_str(&open);
    out.push('>');
    for (line, (x, baseline)) in lines.iter().zip(placed) {
        for piece in line.pieces.iter().filter(|piece| !piece.italic) {
            piece_span(out, &piece.text, piece.style, x + piece.x, *baseline);
        }
    }
    out.push_str("</text>");
    for (line, (x, baseline)) in lines.iter().zip(placed) {
        for piece in line.pieces.iter().filter(|piece| piece.italic) {
            out.push_str(&format!(r#"{open} transform="matrix(1 0 {} 1 {} 0)">"#, num(-SLANT), num(SLANT * baseline)));
            piece_span(out, &piece.text, piece.style, x + piece.x, *baseline);
            out.push_str("</text>");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_short_and_finite() {
        assert_eq!(num(12.0), "12");
        assert_eq!(num(0.1 + 0.2), "0.3");
        assert_eq!(num(-0.001), "0");
        assert_eq!(num(f64::NAN), "0");
        assert_eq!(num(-3.456), "-3.46");
    }

    #[test]
    fn escaping_and_base64() {
        assert_eq!(esc(r#"a<b>&"c'"#), "a&lt;b&gt;&amp;&quot;c&#39;");
        assert_eq!(esc("x\u{1}y"), "xy");
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(id_part("a:b-c_1"), "a_b-c_1");
    }
}
