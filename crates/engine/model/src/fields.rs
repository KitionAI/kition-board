//! Which fields each kind of element uses (design note 1.2), and the values a command accepts for
//! them: colours as one of nine tokens or `#rrggbb`, style enums, the 18 shapes.
//!
//! Ported from Kition's `packages/board-editor/src/lib/whiteboardTypes.ts`, `whiteboardStyle.ts`
//! and `boardElementDefinitions.ts`.

use kition_board_kdm::{Kind, Point, Size};
use serde_json::{Value, json};

use crate::params::{MAX_DIMENSION, Result, invalid, point, size};

/// The colour tokens a theme restyles.
pub const COLOR_TOKENS: &[&str] = &["ink", "gray", "purple", "green", "orange", "red", "yellow", "blue", "white"];
/// The 18 shapes of `shapeType` (v1's 19 without `frame`, which is a kind now).
pub const SHAPE_TYPES: &[&str] = &[
    "rectangle",
    "ellipse",
    "triangle",
    "diamond",
    "hexagon",
    "pill",
    "parallelogram",
    "star",
    "cloud",
    "heart",
    "x-box",
    "check-box",
    "check",
    "arrow-left",
    "arrow-right",
    "arrow-up",
    "arrow-down",
    "line",
];
pub const SHAPE_STYLES: &[&str] = &["default", "sticky", "mind-node", "flow-node", "image-placeholder"];
pub const FILL_STYLES: &[&str] = &["none", "solid", "semi", "pattern"];
pub const DASH_STYLES: &[&str] = &["solid", "dashed", "dotted"];
pub const STROKE_SIZES: &[&str] = &["s", "m", "l", "xl"];
pub const CONNECTOR_TYPES: &[&str] = &["straight", "elbow", "curved"];
pub const ARROWHEADS: &[&str] = &["none", "arrow", "dot"];
pub const FONT_FAMILIES: &[&str] = &["sans", "serif", "mono"];
pub const TEXT_ALIGNS: &[&str] = &["left", "center", "right"];
pub const TEXT_FORMATS: &[&str] = &["plain", "markdown"];
pub const CONNECTOR_ROLES: &[&str] = &["mind-map-branch"];
pub const BRANCH_AXES: &[&str] = &["horizontal", "vertical"];
pub const BRANCH_SIDES: &[&str] = &["left", "right"];
pub const MIND_MAP_DIRECTIONS: &[&str] = &["both", "right", "left", "down"];

/// The style fields `element.style` sets.
pub const STYLE_FIELDS: &[&str] = &[
    "strokeColor",
    "fillColor",
    "fillStyle",
    "dashStyle",
    "strokeSize",
    "opacity",
    "roundness",
    "fontSize",
    "fontFamily",
    "textAlign",
    "connectorType",
    "startArrowhead",
    "endArrowhead",
];

const COMMON: &[&str] = &["locked", "parent", "key", "sourceRefIds"];

/// The fields `kind` uses besides [`COMMON`] (design note 1.2).
fn kind_fields(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Shape => &[
            "position",
            "size",
            "rotation",
            "text",
            "textFormat",
            "shapeType",
            "shapeStyle",
            "strokeColor",
            "fillColor",
            "fillStyle",
            "dashStyle",
            "strokeSize",
            "opacity",
            "roundness",
            "fontSize",
            "fontFamily",
            "textAlign",
            "mindMapBranchSide",
            "mindMapDirection",
            "mindMapCollapsed",
        ],
        Kind::Text => &["position", "size", "rotation", "text", "textFormat", "fontSize", "fontFamily", "textAlign", "strokeColor", "opacity"],
        Kind::Stroke => &["points", "strokeColor", "strokeSize", "dashStyle", "opacity"],
        Kind::Connector => &[
            "start",
            "end",
            "waypoints",
            "text",
            "labelPosition",
            "connectorType",
            "startArrowhead",
            "endArrowhead",
            "connectorRole",
            "mindMapBranchAxis",
            "dashStyle",
            "strokeSize",
            "strokeColor",
            "opacity",
        ],
        Kind::Image => &["position", "size", "rotation", "image", "alt", "opacity"],
        Kind::Card => &["position", "size", "card", "text", "fillColor", "strokeColor"],
        Kind::Frame => &["position", "size", "text", "fillColor", "strokeColor"],
        Kind::Group => &[],
    }
}

/// Whether `kind` uses `field`.
pub fn uses(kind: Kind, field: &str) -> bool {
    field == "kind" || COMMON.contains(&field) || kind_fields(kind).contains(&field)
}

/// The fields `element.update` may set (the rest have their own commands).
pub const UPDATABLE: &[&str] = &[
    "position",
    "size",
    "rotation",
    "parent",
    "key",
    "points",
    "textFormat",
    "shapeType",
    "shapeStyle",
    "strokeColor",
    "fillColor",
    "fillStyle",
    "dashStyle",
    "strokeSize",
    "opacity",
    "roundness",
    "fontSize",
    "fontFamily",
    "textAlign",
    "labelPosition",
    "connectorType",
    "startArrowhead",
    "endArrowhead",
    "connectorRole",
    "mindMapBranchAxis",
    "mindMapBranchSide",
    "mindMapDirection",
    "mindMapCollapsed",
    "sourceRefIds",
];

fn one_of(value: &Value, field: &str, choices: &[&str]) -> Result<Value> {
    match value.as_str() {
        Some(text) if choices.contains(&text) => Ok(json!(text)),
        _ => Err(invalid(format!("`{field}` must be one of {}", choices.join(", ")))),
    }
}

/// A colour: a token, or `#rrggbb` (stored in lower case).
pub fn color(value: &Value, field: &str) -> Result<Value> {
    let text = value.as_str().ok_or_else(|| invalid(format!("`{field}` must be a colour token or #rrggbb")))?;
    if COLOR_TOKENS.contains(&text) {
        return Ok(json!(text));
    }
    let hex = text.strip_prefix('#').filter(|hex| hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()));
    match hex {
        Some(hex) => Ok(json!(format!("#{}", hex.to_ascii_lowercase()))),
        None => Err(invalid(format!("`{field}` must be one of {} or #rrggbb", COLOR_TOKENS.join(", ")))),
    }
}

/// Whether `key` is an authoring key: a Mermaid node id, or a connector's `<from>><to>#<n>`.
pub fn valid_key(key: &str) -> bool {
    fn ident(text: &str) -> bool {
        let mut chars = text.chars();
        chars.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    }
    match key.split_once('>') {
        None => ident(key),
        Some((from, rest)) => match rest.split_once('#') {
            Some((to, n)) => ident(from) && ident(to) && !n.is_empty() && n.bytes().all(|byte| byte.is_ascii_digit()),
            None => false,
        },
    }
}

fn unit_interval(value: &Value, field: &str) -> Result<Value> {
    let number =
        value.as_f64().filter(|v| v.is_finite() && (0.0..=1.0).contains(v)).ok_or_else(|| invalid(format!("`{field}` must be between 0 and 1")))?;
    Ok(json!(number))
}

fn bounded(value: &Value, field: &str, min: f64, max: f64) -> Result<Value> {
    let number = value
        .as_f64()
        .filter(|v| v.is_finite() && (min..=max).contains(v))
        .ok_or_else(|| invalid(format!("`{field}` must be between {min} and {max}")))?;
    Ok(json!(number))
}

/// The stored value of `field` for a command's `value`: checked, normalised, `null` to clear an
/// optional field. Fields with their own structure (`start`, `end`, `card`, `image`, `text`) are not
/// handled here.
pub fn check(kind: Kind, field: &str, value: &Value) -> Result<Value> {
    if !uses(kind, field) {
        return Err(invalid(format!("a {} element has no `{field}`", kind.as_str())));
    }
    if value.is_null() {
        return Ok(Value::Null);
    }
    match field {
        "position" => Ok(point(value, field)?.to_object()),
        "size" => Ok(size(value, field)?.to_json()),
        "rotation" => bounded(value, field, -3600.0, 3600.0),
        "locked" | "mindMapCollapsed" => value.as_bool().map(Value::Bool).ok_or_else(|| invalid(format!("`{field}` must be true or false"))),
        "parent" => value.as_str().map(|id| json!(id)).ok_or_else(|| invalid("`parent` must be an element id")),
        "key" => match value.as_str() {
            Some(key) if valid_key(key) => Ok(json!(key)),
            _ => Err(invalid("`key` must be an authoring id like `start` or `a>b#1`")),
        },
        "strokeColor" | "fillColor" => color(value, field),
        "fillStyle" => one_of(value, field, FILL_STYLES),
        "dashStyle" => one_of(value, field, DASH_STYLES),
        "strokeSize" => one_of(value, field, STROKE_SIZES),
        "opacity" | "labelPosition" => unit_interval(value, field),
        "roundness" => bounded(value, field, 0.0, MAX_DIMENSION),
        "fontSize" => bounded(value, field, 1.0, 1000.0),
        "fontFamily" => one_of(value, field, FONT_FAMILIES),
        "textAlign" => one_of(value, field, TEXT_ALIGNS),
        "textFormat" => one_of(value, field, TEXT_FORMATS),
        "shapeType" => one_of(value, field, SHAPE_TYPES),
        "shapeStyle" => one_of(value, field, SHAPE_STYLES),
        "connectorType" => one_of(value, field, CONNECTOR_TYPES),
        "startArrowhead" | "endArrowhead" => one_of(value, field, ARROWHEADS),
        "connectorRole" => one_of(value, field, CONNECTOR_ROLES),
        "mindMapBranchAxis" => one_of(value, field, BRANCH_AXES),
        "mindMapBranchSide" => one_of(value, field, BRANCH_SIDES),
        "mindMapDirection" => one_of(value, field, MIND_MAP_DIRECTIONS),
        "alt" => value.as_str().map(|text| json!(text)).ok_or_else(|| invalid("`alt` must be text")),
        "points" | "waypoints" => Ok(Point::list_json(&crate::params::points(value, field, 100_000)?)),
        "sourceRefIds" => {
            let ids = value
                .as_array()
                .filter(|ids| ids.len() <= 16 && ids.iter().all(Value::is_string))
                .ok_or_else(|| invalid("`sourceRefIds` must be at most 16 strings"))?;
            Ok(Value::Array(ids.clone()))
        }
        other => Err(invalid(format!("`{other}` is not set this way"))),
    }
}

/// A new element's default size.
pub fn default_size(kind: Kind) -> Option<Size> {
    match kind {
        Kind::Shape => Some(Size::new(160.0, 100.0)),
        Kind::Frame => Some(Size::new(640.0, 400.0)),
        Kind::Card => Some(Size::new(320.0, 200.0)),
        Kind::Image => Some(Size::new(320.0, 240.0)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_tokens_or_lower_case_hex() {
        assert_eq!(color(&json!("purple"), "c").unwrap(), json!("purple"));
        assert_eq!(color(&json!("#5645D4"), "c").unwrap(), json!("#5645d4"));
        assert!(color(&json!("#5645d"), "c").is_err());
        assert!(color(&json!("violet"), "c").is_err());
        assert!(color(&json!(3), "c").is_err());
    }

    #[test]
    fn fields_follow_the_kind_table() {
        assert!(check(Kind::Shape, "fillColor", &json!("red")).is_ok());
        assert!(check(Kind::Stroke, "fillColor", &json!("red")).is_err(), "a stroke has no fill");
        assert!(check(Kind::Group, "position", &json!([0, 0])).is_err(), "a group's box is derived");
        assert_eq!(check(Kind::Shape, "position", &json!([1, 2])).unwrap(), json!({ "x": 1.0, "y": 2.0 }));
        assert_eq!(check(Kind::Shape, "opacity", &Value::Null).unwrap(), Value::Null);
        assert!(check(Kind::Shape, "shapeType", &json!("frame")).is_err(), "frame is a kind now");
    }

    #[test]
    fn keys_are_authoring_ids() {
        for good in ["start", "_a1", "a>b#1", "node_2>x#10"] {
            assert!(valid_key(good), "{good}");
        }
        for bad in ["", "1a", "a b", "a>b", "a>b#", "a>#1", "a-b"] {
            assert!(!valid_key(bad), "{bad}");
        }
    }
}
