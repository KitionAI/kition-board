//! An element's look: colours (a theme token or `#rrggbb`), fill, dashes, line weight, opacity,
//! with each kind's defaults (design note 1.2; v1's `whiteboardStyle.ts` and the semantic styles of
//! `boardElementDefinitions.ts`).
//!
//! Tokens are written as CSS custom properties with the default colour behind them
//! (`var(--kb-stroke-purple, #5645d4)`), so a theme restyles fragments without fetching them again
//! (design note 4.2). The raster writes the default colours themselves.

use kition_board_kdm::{Element, Kind};
use serde_json::Value;

use crate::svg::num;

/// How colours are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Colors {
    /// `var(--kb-…, #default)`: the editor's fragments.
    Css,
    /// `#default`: the raster and exports.
    Literal,
}

/// Whether a colour paints lines and text or fills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Stroke,
    Fill,
}

/// The nine tokens: (token, line and text colour, fill colour).
pub const PALETTE: [(&str, &str, &str); 9] = [
    ("ink", "#1e1e1e", "#e8e8e6"),
    ("gray", "#868e96", "#f1f3f5"),
    ("purple", "#5645d4", "#ece9fb"),
    ("green", "#2f9e44", "#e6f4ea"),
    ("orange", "#e8590c", "#fdebd9"),
    ("red", "#e03131", "#fde4e4"),
    ("yellow", "#f08c00", "#fff6cc"),
    ("blue", "#1971c2", "#e5f0fb"),
    ("white", "#ffffff", "#ffffff"),
];

/// The page behind labels, the muted text and the soft surface of placeholders.
pub const PAPER: (&str, &str) = ("--kb-paper", "#ffffff");
pub const MUTED: (&str, &str) = ("--kb-muted", "#6b6b6b");
pub const SURFACE: (&str, &str) = ("--kb-surface", "#f6f5f4");

fn is_hex(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value.chars().skip(1).all(|ch| ch.is_ascii_hexdigit())
}

/// A named colour of the page (`PAPER`, `MUTED`, `SURFACE`).
pub fn named(color: (&str, &str), colors: Colors) -> String {
    match colors {
        Colors::Css => format!("var({}, {})", color.0, color.1),
        Colors::Literal => color.1.to_owned(),
    }
}

/// `value` (a token or `#rrggbb`) as paint for `role`; anything else is `fallback` (a token).
pub fn paint(value: Option<&str>, role: Role, fallback: &str, colors: Colors) -> String {
    if let Some(hex) = value.filter(|value| is_hex(value)) {
        return hex.to_ascii_lowercase();
    }
    let token = value.filter(|value| PALETTE.iter().any(|(name, _, _)| name == value)).unwrap_or(fallback);
    let (name, stroke, fill) = PALETTE.iter().find(|(name, _, _)| *name == token).copied().unwrap_or(PALETTE[0]);
    let (prefix, default) = match role {
        Role::Stroke => ("stroke", stroke),
        Role::Fill => ("fill", fill),
    };
    match colors {
        Colors::Css => format!("var(--kb-{prefix}-{name}, {default})"),
        Colors::Literal => default.to_owned(),
    }
}

/// A line weight in board units (`s`, `m`, `l`, `xl`; `m` by default).
pub fn stroke_width(size: Option<&str>) -> f64 {
    match size {
        Some("s") => 1.5,
        Some("l") => 4.0,
        Some("xl") => 7.0,
        _ => 2.5,
    }
}

/// The `stroke-dasharray` of a dash style at `width`, `None` for solid lines.
pub fn dash_array(dash: Option<&str>, width: f64) -> Option<String> {
    match dash {
        Some("dashed") => Some(format!("{} {}", num(width * 3.2), num(width * 2.2))),
        Some("dotted") => Some(format!("{} {}", num(width * 0.25), num(width * 2.0))),
        _ => None,
    }
}

/// The resolved look of an element.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    /// Line and text colour.
    pub stroke: String,
    /// `None`: not filled.
    pub fill: Option<String>,
    pub fill_style: String,
    pub width: f64,
    pub dash: Option<String>,
    pub opacity: f64,
}

fn text_of<'a>(element: &'a Element, field: &str) -> Option<&'a str> {
    element.register(field).and_then(Value::as_str)
}

/// A kind's colours and fill before any style is set: (line colour, fill colour, fill style,
/// dashes), by `kind` and, for shapes, `shapeStyle`.
pub fn kind_defaults(kind: Kind, shape_style: Option<&str>) -> (&'static str, &'static str, &'static str, &'static str) {
    match kind {
        Kind::Frame => ("gray", "white", "none", "solid"),
        Kind::Card => ("gray", "white", "solid", "solid"),
        Kind::Shape => match shape_style {
            Some("sticky") => ("ink", "yellow", "solid", "solid"),
            Some("mind-node") => ("ink", "purple", "solid", "solid"),
            Some("image-placeholder") => ("ink", "blue", "solid", "solid"),
            _ => ("ink", "white", "solid", "solid"),
        },
        _ => ("ink", "white", "solid", "solid"),
    }
}

/// The value a style field has on an element of `kind` that does not set it: what the scene
/// draws (the style panel shows it; `element.getStyle`).
pub fn field_default(kind: Kind, shape_style: Option<&str>, field: &str) -> Option<Value> {
    let (stroke, fill, fill_style, dash) = kind_defaults(kind, shape_style);
    let value = match field {
        "strokeColor" => Value::from(stroke),
        "fillColor" => Value::from(fill),
        "fillStyle" => Value::from(fill_style),
        "dashStyle" => Value::from(dash),
        "strokeSize" => Value::from("m"),
        "opacity" => Value::from(1.0),
        "fontSize" => Value::from(if kind == Kind::Text { kition_board_text::TEXT_SIZE } else { kition_board_text::LABEL_SIZE }),
        "fontFamily" => Value::from("sans"),
        "textAlign" => Value::from(if kind == Kind::Text { "left" } else { "center" }),
        "connectorType" => Value::from("straight"),
        "startArrowhead" => Value::from("none"),
        "endArrowhead" => Value::from("arrow"),
        _ => return None,
    };
    Some(value)
}

/// The look of `element`, its kind's defaults filled in.
pub fn look(element: &Element, colors: Colors) -> Look {
    let (stroke_default, fill_default, fill_style_default, dash_default) = kind_defaults(element.kind, text_of(element, "shapeStyle"));
    // A frame fills only when it was given a fill colour.
    let fill_style = match element.kind {
        Kind::Frame if text_of(element, "fillColor").is_some() => "solid".to_owned(),
        _ => text_of(element, "fillStyle").unwrap_or(fill_style_default).to_owned(),
    };
    let width = match element.kind {
        Kind::Frame | Kind::Card => 1.5,
        _ => stroke_width(text_of(element, "strokeSize")),
    };
    let fill = (fill_style != "none").then(|| paint(text_of(element, "fillColor"), Role::Fill, fill_default, colors));
    Look {
        stroke: paint(text_of(element, "strokeColor"), Role::Stroke, stroke_default, colors),
        fill,
        fill_style,
        width,
        dash: dash_array(text_of(element, "dashStyle").or(Some(dash_default)), width),
        opacity: element.register("opacity").and_then(Value::as_f64).filter(|v| v.is_finite()).map_or(1.0, |v| v.clamp(0.05, 1.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_tokens_or_hex() {
        assert_eq!(paint(Some("purple"), Role::Stroke, "ink", Colors::Css), "var(--kb-stroke-purple, #5645d4)");
        assert_eq!(paint(Some("purple"), Role::Fill, "ink", Colors::Literal), "#ece9fb");
        assert_eq!(paint(Some("#AABBCC"), Role::Fill, "ink", Colors::Css), "#aabbcc");
        assert_eq!(paint(Some("chartreuse"), Role::Stroke, "gray", Colors::Literal), "#868e96", "unknown values fall back");
        assert_eq!(paint(None, Role::Stroke, "nonsense", Colors::Literal), "#1e1e1e");
        assert_eq!(named(PAPER, Colors::Css), "var(--kb-paper, #ffffff)");
    }

    #[test]
    fn weights_and_dashes() {
        assert_eq!(stroke_width(None), 2.5);
        assert_eq!(stroke_width(Some("xl")), 7.0);
        assert_eq!(dash_array(Some("dashed"), 2.5).as_deref(), Some("8 5.5"));
        assert_eq!(dash_array(Some("solid"), 2.5), None);
    }
}
