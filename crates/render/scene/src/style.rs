//! An element's look: colours (a theme token or `#rrggbb`), fill, dashes, line weight, opacity,
//! with each kind's defaults (design note 1.2; v1's `whiteboardStyle.ts` and the semantic styles of
//! `boardElementDefinitions.ts`).
//!
//! Tokens are written as CSS custom properties with the default colour behind them
//! (`var(--kb-stroke-purple, #5645d4)`), so a theme restyles fragments without fetching them again
//! (design note 4.2). The raster writes the default colours themselves.
//!
//! **Schemes.** The tokens have a second set of colours for the host's dark scheme ([`DARK_PALETTE`],
//! [`DARK_PAPER`], ...): the editor's stylesheet (`web/src/app.css`) gives the `--kb-*` properties
//! these values under `[data-color-scheme='dark']`, and the minimap's raster writes them
//! ([`Colors::Dark`]). A token is the board's colour for "this hue, readable on the board", so a
//! scheme restyles it; a `#rrggbb` colour is the file's own and is never restyled. Nothing about the
//! scheme is stored, and every export, preview and thumbnail is drawn light ([`Colors::Literal`]).

use kition_board_kdm::{Element, Kind};
use serde_json::Value;

use crate::svg::num;

/// How colours are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Colors {
    /// `var(--kb-…, #default)`: the editor's fragments.
    Css,
    /// `#default`: the raster and exports (the light scheme).
    Literal,
    /// The dark scheme's colours, literal: the editor's minimap in the host's dark scheme.
    Dark,
}

/// The host's color scheme (Kit UI protocol `KitTheme.colorScheme`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Scheme {
    #[default]
    Light,
    Dark,
}

impl Scheme {
    /// `"light"` or `"dark"`; anything else is not a scheme.
    pub fn parse(name: &str) -> Option<Scheme> {
        match name {
            "light" => Some(Scheme::Light),
            "dark" => Some(Scheme::Dark),
            _ => None,
        }
    }

    /// How a raster in this scheme writes colours.
    pub fn colors(self) -> Colors {
        match self {
            Scheme::Light => Colors::Literal,
            Scheme::Dark => Colors::Dark,
        }
    }

    /// The board's background in this scheme.
    pub fn paper(self) -> &'static str {
        match self {
            Scheme::Light => PAPER.1,
            Scheme::Dark => DARK_PAPER,
        }
    }
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

/// The nine tokens in the dark scheme, in [`PALETTE`]'s order: lines and text lightened so each
/// reads at WCAG AA (4.5:1) on [`DARK_PAPER`], fills darkened to a tint of the hue so the ink's
/// text reads on them. `white` is the paper's colour, as it is in the light scheme.
pub const DARK_PALETTE: [(&str, &str, &str); 9] = [
    ("ink", "#e8e8e6", "#3a3a3c"),
    ("gray", "#a5adb5", "#2c3034"),
    ("purple", "#a99cf5", "#302a55"),
    ("green", "#5fc274", "#1f3a27"),
    ("orange", "#f5925a", "#45291a"),
    ("red", "#f47a7a", "#4a2425"),
    ("yellow", "#f5b544", "#3f3416"),
    ("blue", "#6cb2ee", "#1c3149"),
    ("white", "#1e1e1e", "#1e1e1e"),
];

/// The page behind labels, the muted text and the soft surface of placeholders.
pub const PAPER: (&str, &str) = ("--kb-paper", "#ffffff");
pub const MUTED: (&str, &str) = ("--kb-muted", "#6b6b6b");
pub const SURFACE: (&str, &str) = ("--kb-surface", "#f6f5f4");
/// [`PAPER`], [`MUTED`] and [`SURFACE`] in the dark scheme (Kition's dark paper, as the note kit's).
pub const DARK_PAPER: &str = "#1e1e1e";
pub const DARK_MUTED: &str = "#a0a0a0";
pub const DARK_SURFACE: &str = "#2a2a2c";

/// Each `--kb-*` property and its colour in the dark scheme: what `web/src/app.css` sets under
/// `[data-color-scheme='dark']`.
pub fn dark_properties() -> Vec<(String, &'static str)> {
    let mut out = vec![(PAPER.0.to_owned(), DARK_PAPER), (MUTED.0.to_owned(), DARK_MUTED), (SURFACE.0.to_owned(), DARK_SURFACE)];
    for (name, stroke, fill) in DARK_PALETTE {
        out.push((format!("--kb-stroke-{name}"), stroke));
        out.push((format!("--kb-fill-{name}"), fill));
    }
    out
}

fn is_hex(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value.chars().skip(1).all(|ch| ch.is_ascii_hexdigit())
}

/// A named colour of the page (`PAPER`, `MUTED`, `SURFACE`).
pub fn named(color: (&str, &str), colors: Colors) -> String {
    match colors {
        Colors::Css => format!("var({}, {})", color.0, color.1),
        Colors::Literal => color.1.to_owned(),
        Colors::Dark => match color.0 {
            "--kb-paper" => DARK_PAPER,
            "--kb-muted" => DARK_MUTED,
            "--kb-surface" => DARK_SURFACE,
            _ => color.1,
        }
        .to_owned(),
    }
}

/// `value` (a token or `#rrggbb`) as paint for `role`; anything else is `fallback` (a token).
pub fn paint(value: Option<&str>, role: Role, fallback: &str, colors: Colors) -> String {
    if let Some(hex) = value.filter(|value| is_hex(value)) {
        return hex.to_ascii_lowercase();
    }
    let token = value.filter(|value| PALETTE.iter().any(|(name, _, _)| name == value)).unwrap_or(fallback);
    let (name, stroke, fill) = PALETTE.iter().find(|(name, _, _)| *name == token).copied().unwrap_or(PALETTE[0]);
    let pick = |(_, stroke, fill): (&str, &'static str, &'static str)| match role {
        Role::Stroke => stroke,
        Role::Fill => fill,
    };
    let default = pick((name, stroke, fill));
    match colors {
        Colors::Css => {
            let prefix = if role == Role::Stroke { "stroke" } else { "fill" };
            format!("var(--kb-{prefix}-{name}, {default})")
        }
        Colors::Literal => default.to_owned(),
        Colors::Dark => DARK_PALETTE.iter().find(|(dark, _, _)| *dark == name).copied().map_or(default, pick).to_owned(),
    }
}

/// WCAG relative luminance of `#rrggbb`; `None` for anything else.
fn luminance(hex: &str) -> Option<f64> {
    if !is_hex(hex) {
        return None;
    }
    let channel = |at: usize| {
        let byte = hex.get(at..at + 2).and_then(|pair| u8::from_str_radix(pair, 16).ok())?;
        let c = f64::from(byte) / 255.0;
        Some(if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) })
    };
    Some(0.2126 * channel(1)? + 0.7152 * channel(3)? + 0.0722 * channel(5)?)
}

/// WCAG contrast ratio of two `#rrggbb` colours.
fn contrast(a: &str, b: &str) -> Option<f64> {
    let (a, b) = (luminance(a)?, luminance(b)?);
    Some((a.max(b) + 0.05) / (a.min(b) + 0.05))
}

/// The text colour on a solid `fill` that is a file's own `#rrggbb` (written literally, so no
/// scheme restyles it), when the text colour `value` is a token (`fallback` when unset or not a
/// colour): the token's light- or dark-scheme line colour, whichever contrasts more with the fill,
/// written literally too. `None` when the fill is a token (the scheme restyles fill and text
/// together) or the text colour is the file's own.
pub fn text_on(value: Option<&str>, fallback: &str, fill: &str) -> Option<String> {
    if !is_hex(fill) || value.is_some_and(is_hex) {
        return None;
    }
    let token = value.filter(|value| PALETTE.iter().any(|(name, _, _)| name == value)).unwrap_or(fallback);
    let light = PALETTE.iter().find(|(name, _, _)| *name == token).map_or(PALETTE[0].1, |(_, stroke, _)| stroke);
    let dark = DARK_PALETTE.iter().find(|(name, _, _)| *name == token).map_or(DARK_PALETTE[0].1, |(_, stroke, _)| stroke);
    let best = if contrast(dark, fill)? > contrast(light, fill)? { dark } else { light };
    Some((*best).to_owned())
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
    /// Line colour.
    pub stroke: String,
    /// The colour of the text drawn on the element (its label): the line colour, except on a
    /// solid fill in a file's own `#rrggbb`, which no scheme restyles: there a token text colour is
    /// written as that token's light- or dark-scheme colour, whichever reads better on the fill, so
    /// a pastel sticky's text stays dark in the dark scheme ([`text_on`]).
    pub text: String,
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
    let stroke = paint(text_of(element, "strokeColor"), Role::Stroke, stroke_default, colors);
    let text = match fill.as_deref() {
        Some(fill) if fill_style == "solid" => text_on(text_of(element, "strokeColor"), stroke_default, fill).unwrap_or_else(|| stroke.clone()),
        _ => stroke.clone(),
    };
    Look {
        stroke,
        text,
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

    /// WCAG relative luminance of `#rrggbb`.
    fn luminance(hex: &str) -> f64 {
        let channel = |at: usize| {
            let c = f64::from(u8::from_str_radix(&hex[at..at + 2], 16).unwrap()) / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
    }

    /// WCAG contrast ratio of two `#rrggbb` colours.
    fn contrast(a: &str, b: &str) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn dark_lines_and_text_meet_wcag_aa_on_the_dark_paper() {
        for (name, stroke, fill) in DARK_PALETTE {
            assert!(is_hex(stroke) && is_hex(fill), "{name}");
            if name == "white" {
                // The paper's own colour, in both schemes.
                assert_eq!((stroke, fill), (DARK_PAPER, DARK_PAPER));
                continue;
            }
            let ratio = contrast(stroke, DARK_PAPER);
            assert!(ratio >= 4.5, "{name} lines and text on the dark paper: {ratio:.2}");
            // A label in the default ink on a shape filled with this token.
            let ink = contrast(DARK_PALETTE[0].1, fill);
            assert!(ink >= 4.5, "ink text on the dark {name} fill: {ink:.2}");
        }
        assert!(contrast(DARK_MUTED, DARK_PAPER) >= 4.5 && contrast(DARK_MUTED, DARK_SURFACE) >= 4.5, "muted text");
        assert!(luminance(DARK_PAPER) <= 0.1, "the dark paper is dark");
        // Same tokens, same order as the light palette.
        assert_eq!(PALETTE.map(|(name, _, _)| name), DARK_PALETTE.map(|(name, _, _)| name));
    }

    #[test]
    fn the_dark_scheme_restyles_tokens_and_keeps_the_files_own_colours() {
        assert_eq!(paint(Some("ink"), Role::Stroke, "ink", Colors::Dark), "#e8e8e6");
        assert_eq!(paint(None, Role::Fill, "white", Colors::Dark), DARK_PAPER);
        assert_eq!(paint(Some("#336699"), Role::Stroke, "ink", Colors::Dark), "#336699", "a file's own colour is never restyled");
        assert_eq!(named(PAPER, Colors::Dark), DARK_PAPER);
        assert_eq!(named(MUTED, Colors::Dark), DARK_MUTED);
        assert_eq!(named(SURFACE, Colors::Dark), DARK_SURFACE);
        // The light colours stay what they were.
        assert_eq!(paint(Some("ink"), Role::Stroke, "ink", Colors::Literal), "#1e1e1e");
        assert_eq!(Scheme::Light.colors(), Colors::Literal);
        assert_eq!((Scheme::parse("dark"), Scheme::parse("light"), Scheme::parse("sepia")), (Some(Scheme::Dark), Some(Scheme::Light), None));
    }

    /// The editor's stylesheet gives the `--kb-*` properties the dark palette under
    /// `[data-color-scheme='dark']`, value for value.
    #[test]
    fn the_editor_stylesheet_mirrors_the_dark_palette() {
        let css = include_str!("../../../../web/src/app.css");
        let start = css.find("[data-color-scheme='dark'] .board-canvas").expect("a dark block in app.css");
        let block = &css[start..start + css[start..].find('}').unwrap()];
        for (property, value) in dark_properties() {
            let line = format!("{property}: {value};");
            assert!(block.contains(&line), "app.css's dark block lacks `{line}`");
        }
        assert_eq!(block.matches("--kb-").count(), dark_properties().len(), "app.css's dark block sets only the palette's properties");
    }

    #[test]
    fn weights_and_dashes() {
        assert_eq!(stroke_width(None), 2.5);
        assert_eq!(stroke_width(Some("xl")), 7.0);
        assert_eq!(dash_array(Some("dashed"), 2.5).as_deref(), Some("8 5.5"));
        assert_eq!(dash_array(Some("solid"), 2.5), None);
    }
}
