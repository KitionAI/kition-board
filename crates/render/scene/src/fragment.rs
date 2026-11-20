//! One element as an SVG fragment (design note 4.2): boxed kinds in their local frame (origin at
//! `position`, unrotated, the editor places them), strokes and connectors in world coordinates.
//!
//! Ported from Kition's `packages/board-editor/src/components/WhiteboardShapeBody.tsx`,
//! `WhiteboardElementRenderer.tsx` and the arrowhead markers of `WhiteboardCanvas.tsx`. Changes:
//! text laid out by the engine with real font metrics (one `tspan` per line with explicit `x` and
//! `y`), arrowheads drawn as paths (a fragment needs no page `<defs>`), line weights in board units
//! (no `non-scaling-stroke`, so the editor, the raster and exports draw the same widths), new kinds
//! (frame, card, group) and image blobs.

use kition_board_kdm::{BoardModel, Element, Kind, Point, Rect};
use kition_board_text::{Family, LABEL_SIZE, Line, REGULAR, RichLine, SEMIBOLD, Style, TEXT_SIZE, layout, layout_rich, width};
use kition_kit::session::BlobSource;
use serde_json::Value;

use crate::route::{self, Path};
use crate::shapes;
use crate::style::{self, Colors, Look, MUTED, PAPER, Role, SURFACE};
use crate::svg::{base64, esc, id_part, num, place, place_rich, rich_text_block, text_block};

/// Level of detail (design note 4.2): `Low` below 50% zoom draws text as bars.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lod {
    Full,
    Low,
}

/// How pictures are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pictures {
    /// `<image data-blob="<hash>">` without bytes: the editor sets `href` from its blob cache.
    Reference,
    /// The bytes inline as a `data:` URL (the raster, exports).
    Inline,
}

/// What a fragment is drawn with.
pub struct Options<'a> {
    pub colors: Colors,
    pub lod: Lod,
    pub pictures: Pictures,
    pub blobs: &'a dyn BlobSource,
}

/// One element's fragment.
#[derive(Clone, Debug, PartialEq)]
pub struct Fragment {
    pub svg: String,
    /// The box the fragment covers: in the local frame (unrotated) when `local`, so moving or
    /// rotating the element leaves the fragment as it is; in world coordinates otherwise.
    pub bounds: Rect,
    /// `true`: the SVG is in the element's local frame; `false`: world coordinates.
    pub local: bool,
    /// Where the editor puts a host preview (table and board cards), in the local frame.
    pub slot: Option<Rect>,
    /// The element's box size for local fragments (a `text` element without a stored size is
    /// measured here, so the editor needs this to select and bind it).
    pub size: Option<(f64, f64)>,
}

fn text_of<'a>(element: &'a Element, field: &str) -> Option<&'a str> {
    element.register(field).and_then(Value::as_str)
}

fn number_of(element: &Element, field: &str) -> Option<f64> {
    element.register(field).and_then(Value::as_f64).filter(|v| v.is_finite())
}

/// The font of an element's text, `size` when it sets none.
pub fn font(element: &Element, size: f64, weight: u16) -> Style {
    Style::new(number_of(element, "fontSize").unwrap_or(size), weight, Family::parse(text_of(element, "fontFamily")))
}

/// The weight of a shape's label: semibold for mind-map and flow nodes, as v1.
pub fn label_weight(element: &Element) -> u16 {
    match text_of(element, "shapeStyle") {
        Some("mind-node" | "flow-node") => SEMIBOLD,
        _ => 500,
    }
}

/// The label padding of a box `w` wide.
pub fn label_pad(w: f64) -> f64 {
    12.0_f64.min(w / 6.0).max(2.0)
}

/// The lines of a shape's label, wrapped inside its padding.
pub fn shape_label(element: &Element, w: f64) -> (Style, Vec<Line>) {
    let style = font(element, LABEL_SIZE, label_weight(element));
    let max = (w - 2.0 * label_pad(w)).max(style.size);
    (style, layout(&element.text, style, Some(max)))
}

/// The lines of a `text` element: wrapped to its stored width, unwrapped when it sizes itself.
pub fn text_lines(element: &Element) -> (Style, Vec<Line>) {
    let style = font(element, TEXT_SIZE, REGULAR);
    let max = element.size.map(|size| size.w);
    (style, layout(&element.text, style, max))
}

/// The box styled lines placed at `placed` cover (italic overhang included).
fn rich_bounds(lines: &[RichLine], placed: &[(f64, f64)]) -> Rect {
    let mut bounds: Option<Rect> = None;
    for (line, (x, baseline)) in lines.iter().zip(placed) {
        let top = baseline - line.baseline;
        let rect = Rect::new(*x, top, x + line.width + 0.25 * line.height, top + line.height);
        bounds = Some(bounds.map_or(rect, |bounds| bounds.union(rect)));
    }
    bounds.unwrap_or_default()
}

/// `text` cut to `max` wide with an ellipsis.
fn ellipsized(text: &str, style: Style, max: f64) -> Line {
    let full = width(text, style);
    if full <= max {
        return Line { text: text.to_owned(), width: full };
    }
    let mut kept = String::new();
    for ch in text.chars() {
        let next = format!("{kept}{ch}…");
        if width(&next, style) > max {
            break;
        }
        kept.push(ch);
    }
    let text = format!("{}…", kept.trim_end());
    let width = width(&text, style);
    Line { text, width }
}

fn paint_attrs(look: &Look, fill: Option<&str>, fill_opacity: f64) -> String {
    let mut out = format!(r#"fill="{}""#, fill.unwrap_or("none"));
    if fill.is_some() && fill_opacity < 1.0 {
        out.push_str(&format!(r#" fill-opacity="{}""#, num(fill_opacity)));
    }
    out.push_str(&stroke_attrs(look));
    out
}

fn stroke_attrs(look: &Look) -> String {
    let mut out = format!(r#" stroke="{}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round""#, look.stroke, num(look.width));
    if let Some(dash) = &look.dash {
        out.push_str(&format!(r#" stroke-dasharray="{dash}""#));
    }
    out
}

/// A hatch pattern for `fillStyle: pattern`, named after the element so fragments do not clash.
fn hatch(out: &mut String, element: &Element, fill: &str, stroke: &str) -> String {
    let id = format!("kb-hatch-{}", id_part(element.id.as_str()));
    out.push_str(&format!(
        r#"<defs><pattern id="{id}" width="8" height="8" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><rect width="8" height="8" fill="{fill}"/><line x1="0" y1="0" x2="0" y2="8" stroke="{stroke}" stroke-width="1.5" stroke-opacity="0.4"/></pattern></defs>"#
    ));
    format!("url(#{id})")
}

fn wrap_opacity(svg: String, opacity: f64) -> String {
    if opacity < 1.0 { format!(r#"<g opacity="{}">{svg}</g>"#, num(opacity)) } else { svg }
}

/// The fragment of `element` on `model`'s board.
pub fn fragment(model: &BoardModel, element: &Element, options: &Options<'_>) -> Fragment {
    match element.kind {
        Kind::Shape | Kind::Text | Kind::Image | Kind::Card | Kind::Frame => boxed(model, element, options),
        Kind::Stroke => stroke(element, options),
        Kind::Connector => connector(model, element, options),
        Kind::Group => Fragment { svg: String::new(), bounds: model.bounds(element).unwrap_or_default(), local: false, slot: None, size: None },
    }
}

fn boxed(model: &BoardModel, element: &Element, options: &Options<'_>) -> Fragment {
    let Some(rect) = model.rect(element) else {
        return Fragment { svg: String::new(), bounds: Rect::default(), local: true, slot: None, size: None };
    };
    let (w, h) = (rect.width(), rect.height());
    let look = style::look(element, options.colors);
    let low = options.lod == Lod::Low;
    let mut out = String::new();
    let mut local = Rect::new(0.0, 0.0, w, h);
    let mut slot = None;
    match element.kind {
        Kind::Shape => {
            let shape = text_of(element, "shapeType").unwrap_or("rectangle");
            let fill = if shapes::supports_fill(shape) {
                match (look.fill.as_deref(), look.fill_style.as_str()) {
                    (Some(fill), "pattern") => Some(hatch(&mut out, element, fill, &look.stroke)),
                    (fill, _) => fill.map(str::to_owned),
                }
            } else {
                None
            };
            let fill_opacity = if look.fill_style == "semi" { 0.48 } else { 1.0 };
            let paint = paint_attrs(&look, fill.as_deref(), fill_opacity);
            let inner = format!(r#"fill="none"{}"#, stroke_attrs(&look));
            let sticky = text_of(element, "shapeStyle") == Some("sticky");
            shapes::outline(&mut out, shape, w, h, &shapes::Paint { body: &paint, inner: &inner, corner: number_of(element, "roundness"), sticky });
            if text_of(element, "shapeStyle") == Some("mind-node") && text_of(element, "mindMapDirection").is_some() {
                root_marker(&mut out, h, &look.stroke);
            }
            if shapes::supports_label(shape)
                && !element.text.is_empty()
                && let Some(paragraphs) = element.styled_text()
            {
                let style = font(element, LABEL_SIZE, label_weight(element));
                let lines = layout_rich(&paragraphs, style, Some((w - 2.0 * label_pad(w)).max(style.size)));
                let top = (h - lines.iter().map(|line| line.height).sum::<f64>()) / 2.0;
                let placed = place_rich(&lines, w, label_pad(w), top, text_of(element, "textAlign").unwrap_or("center"));
                local = local.union(rich_bounds(&lines, &placed));
                rich_text_block(&mut out, &lines, &placed, &look.text, low);
            } else if shapes::supports_label(shape) && !element.text.is_empty() {
                let (style, lines) = shape_label(element, w);
                let top = (h - lines.len() as f64 * style.line_height()) / 2.0;
                let placed = place(&lines, style, w, label_pad(w), top, text_of(element, "textAlign").unwrap_or("center"));
                for placed in &placed {
                    local = local.union(Rect::new(
                        placed.x,
                        placed.baseline - style.size,
                        placed.x + placed.line.width,
                        placed.baseline + 0.3 * style.size,
                    ));
                }
                text_block(&mut out, &placed, style, &look.text, low);
            }
        }
        Kind::Text => {
            // The whole box takes clicks, not only the glyphs.
            out.push_str(&format!(r#"<rect width="{}" height="{}" fill="transparent"/>"#, num(w), num(h)));
            if let Some(paragraphs) = element.styled_text() {
                let lines = layout_rich(&paragraphs, font(element, TEXT_SIZE, REGULAR), element.size.map(|size| size.w));
                let placed = place_rich(&lines, w, 0.0, 0.0, text_of(element, "textAlign").unwrap_or("left"));
                local = local.union(rich_bounds(&lines, &placed));
                rich_text_block(&mut out, &lines, &placed, &look.stroke, low);
            } else {
                let (style, lines) = text_lines(element);
                let placed = place(&lines, style, w, 0.0, 0.0, text_of(element, "textAlign").unwrap_or("left"));
                text_block(&mut out, &placed, style, &look.stroke, low);
            }
        }
        Kind::Image => image(&mut out, element, w, h, &look, options),
        Kind::Card => {
            slot = card(&mut out, element, w, h, &look, options);
            if !element.text.is_empty() {
                let style = font(element, 13.0, REGULAR);
                let lines = layout(&element.text, style, Some(w.max(style.size)));
                let placed = place(&lines, style, w, 0.0, h + 6.0, "center");
                local = local.union(Rect::new(0.0, h, w, h + 6.0 + lines.len() as f64 * style.line_height()));
                text_block(&mut out, &placed, style, &style::named(MUTED, options.colors), low);
            }
        }
        Kind::Frame => {
            let paint = paint_attrs(&look, look.fill.as_deref(), 1.0);
            out.push_str(&format!(r#"<rect width="{}" height="{}" rx="6" {paint}/>"#, num(w), num(h)));
            if !element.text.is_empty() {
                let style = font(element, 14.0, 500);
                let line = ellipsized(element.text.lines().next().unwrap_or_default(), style, w.max(style.size));
                let top = -style.line_height() - 4.0;
                let lines = [line];
                let placed = place(&lines, style, w, 0.0, top, "left");
                local = local.union(Rect::new(0.0, top, w, 0.0));
                text_block(&mut out, &placed, style, &style::named(MUTED, options.colors), low);
            }
        }
        Kind::Stroke | Kind::Connector | Kind::Group => {}
    }
    let pad = look.width / 2.0;
    let local = Rect::new(local.x0 - pad, local.y0 - pad, local.x1 + pad, local.y1 + pad);
    Fragment { svg: wrap_opacity(out, look.opacity), bounds: local, local: true, slot, size: Some((w, h)) }
}

/// The mind-map root marker (v1 `MindMapRootMarker`).
fn root_marker(out: &mut String, h: f64, color: &str) {
    let (x, y) = (17.0, h / 2.0);
    out.push_str(&format!(
        r#"<path d="M {} {} H {} M {} {} V {} M {} {} H {} M {} {} H {}" fill="none" stroke="{color}" stroke-width="1.5" stroke-linecap="round"/>"#,
        num(x - 4.0),
        num(y),
        num(x + 1.0),
        num(x + 1.0),
        num(y - 5.0),
        num(y + 5.0),
        num(x + 1.0),
        num(y - 5.0),
        num(x + 5.0),
        num(x + 1.0),
        num(y + 5.0),
        num(x + 5.0),
    ));
    for (cx, cy) in [(x - 5.0, y), (x + 6.0, y - 5.0), (x + 6.0, y + 5.0)] {
        out.push_str(&format!(r#"<circle cx="{}" cy="{}" r="2" fill="{color}"/>"#, num(cx), num(cy)));
    }
}

/// A picture: its blob, or a placeholder when the bytes have not arrived (they may arrive late).
fn image(out: &mut String, element: &Element, w: f64, h: f64, look: &Look, options: &Options<'_>) {
    let surface = style::named(SURFACE, options.colors);
    let alt = text_of(element, "alt").unwrap_or_default();
    let bytes = element.image.as_ref().and_then(|blob| options.blobs.get(&blob.hash).map(|bytes| (blob, bytes)));
    match bytes {
        Some((blob, bytes)) => {
            out.push_str(&format!(r#"<rect width="{}" height="{}" rx="8" fill="{surface}"/>"#, num(w), num(h)));
            let source = match options.pictures {
                Pictures::Reference => format!(r#"data-blob="{}""#, esc(&blob.hash)),
                Pictures::Inline => format!(r#"href="data:{};base64,{}""#, esc(&blob.mime), base64(&bytes)),
            };
            out.push_str(&format!(
                r#"<image {source} width="{}" height="{}" preserveAspectRatio="xMidYMid meet" role="img" aria-label="{}"/>"#,
                num(w),
                num(h),
                esc(alt)
            ));
        }
        None => {
            let muted = style::named(MUTED, options.colors);
            out.push_str(&format!(
                r#"<rect width="{}" height="{}" rx="8" fill="{surface}" stroke="{muted}" stroke-width="1.5" stroke-dasharray="6 4"/>"#,
                num(w),
                num(h)
            ));
            // A small mountain and sun in the middle.
            let s = (w.min(h) * 0.3).clamp(8.0, 64.0);
            let (x, y) = ((w - s) / 2.0, (h - s) / 2.0);
            out.push_str(&format!(
                r#"<path d="M {} {} L {} {} L {} {} L {} {} L {} {} Z" fill="{muted}" fill-opacity="0.5"/><circle cx="{}" cy="{}" r="{}" fill="{muted}" fill-opacity="0.5"/>"#,
                num(x),
                num(y + s),
                num(x + s * 0.35),
                num(y + s * 0.45),
                num(x + s * 0.55),
                num(y + s * 0.7),
                num(x + s * 0.75),
                num(y + s * 0.5),
                num(x + s),
                num(y + s),
                num(x + s * 0.75),
                num(y + s * 0.2),
                num(s * 0.12),
            ));
        }
    }
    let _ = look;
}

/// A card: its kind's badge, the target's name and path (or the link's title and address), and
/// for table and board cards the slot the editor fills with the host's preview (design note 3).
/// Answers the slot.
fn card(out: &mut String, element: &Element, w: f64, h: f64, look: &Look, options: &Options<'_>) -> Option<Rect> {
    let value = element.card.as_ref();
    let kind = value.and_then(|card| card.get("kind")).and_then(Value::as_str).unwrap_or("file");
    let path = value.and_then(|card| card.get("target")).and_then(|target| target.get("path")).and_then(Value::as_str).unwrap_or_default();
    let url = value.and_then(|card| card.get("url")).and_then(Value::as_str).unwrap_or_default();
    let title = value.and_then(|card| card.get("title")).and_then(Value::as_str).filter(|title| !title.is_empty());
    let (heading, detail) = if kind == "link" {
        let host = url.split("://").nth(1).unwrap_or(url).split(['/', '?', '#']).next().unwrap_or(url);
        (title.unwrap_or(host).to_owned(), url.to_owned())
    } else {
        let name = path.rsplit('/').next().unwrap_or(path);
        (title.unwrap_or(name).to_owned(), path.to_owned())
    };
    let low = options.lod == Lod::Low;
    let paint = paint_attrs(look, look.fill.as_deref(), 1.0);
    out.push_str(&format!(r#"<rect width="{}" height="{}" rx="12" {paint}/>"#, num(w), num(h)));
    let accent = style::paint(
        Some(match kind {
            "note" => "purple",
            "table" => "green",
            "board" => "orange",
            "link" => "blue",
            _ => "gray",
        }),
        Role::Stroke,
        "gray",
        options.colors,
    );
    out.push_str(&format!(r#"<rect x="12" y="12" width="20" height="20" rx="5" fill="{accent}"/>"#));
    let glyph = match kind {
        "note" => "M 17 18 H 27 M 17 22 H 27 M 17 26 H 23",
        "table" => "M 16 17 H 28 V 27 H 16 Z M 16 22 H 28 M 22 17 V 27",
        "board" => "M 16 17 H 21 V 22 H 16 Z M 23 22 H 28 V 27 H 23 Z M 21 19.5 H 25.5 V 22",
        "link" => "M 19 25 L 25 19 M 18 21 L 16.5 22.5 A 2.5 2.5 0 0 0 21.5 27.5 L 23 26 M 21 18 L 22.5 16.5 A 2.5 2.5 0 0 1 27.5 21.5 L 26 23",
        _ => "M 17 16 H 24 L 27 19 V 28 H 17 Z M 24 16 V 19 H 27",
    };
    // The glyph is the paper's colour: white on the light scheme's badge, the dark paper on the dark
    // scheme's lightened one.
    let glyph_color = style::named(PAPER, options.colors);
    out.push_str(&format!(
        r#"<path d="{glyph}" fill="none" stroke="{glyph_color}" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#
    ));
    let text_width = (w - 52.0).max(8.0);
    let title_style = Style::new(15.0, SEMIBOLD, Family::Sans);
    let detail_style = Style::new(12.0, REGULAR, Family::Sans);
    let heading = [ellipsized(&heading, title_style, text_width)];
    text_block(out, &place(&heading, title_style, w, 40.0, 13.0 - 0.1 * title_style.size, "left"), title_style, &look_text(options), low);
    if !detail.is_empty() && h >= 56.0 {
        let detail = [ellipsized(&detail, detail_style, text_width)];
        text_block(out, &place(&detail, detail_style, w, 40.0, 33.0, "left"), detail_style, &style::named(MUTED, options.colors), low);
    }
    if h < 72.0 {
        return None;
    }
    let body = Rect::new(12.0, 56.0, (w - 12.0).max(12.0), (h - 12.0).max(56.0));
    let surface = style::named(SURFACE, options.colors);
    out.push_str(&format!(
        r#"<rect x="{}" y="{}" width="{}" height="{}" rx="8" fill="{surface}"/>"#,
        num(body.x0),
        num(body.y0),
        num(body.width()),
        num(body.height())
    ));
    match kind {
        "table" | "board" => Some(body),
        "note" => {
            // The note's text is drawn by the engine once linked reads land (design note 3.1);
            // until then, lines stand for it.
            let muted = style::named(MUTED, options.colors);
            let mut y = body.y0 + 14.0;
            let mut index = 0;
            while y + 6.0 < body.y1 && index < 8 {
                let fraction = [0.9, 0.75, 0.85, 0.6, 0.8, 0.7, 0.9, 0.5].get(index).copied().unwrap_or(0.7);
                out.push_str(&format!(
                    r#"<rect x="{}" y="{}" width="{}" height="6" rx="3" fill="{muted}" fill-opacity="0.25"/>"#,
                    num(body.x0 + 12.0),
                    num(y),
                    num((body.width() - 24.0).max(0.0) * fraction)
                ));
                y += 16.0;
                index += 1;
            }
            None
        }
        _ => None,
    }
}

fn look_text(options: &Options<'_>) -> String {
    style::paint(Some("ink"), Role::Stroke, "ink", options.colors)
}

fn stroke(element: &Element, options: &Options<'_>) -> Fragment {
    let look = style::look(element, options.colors);
    let Some(path) = route::smooth(&element.points) else {
        return Fragment { svg: String::new(), bounds: Rect::default(), local: false, slot: None, size: None };
    };
    let d = path.d();
    let mut out = format!(
        r#"<path d="{d}" fill="none" stroke="transparent" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round" class="kb-hit"/>"#,
        num(look.width.max(14.0))
    );
    out.push_str(&format!(r#"<path d="{d}" fill="none"{}/>"#, stroke_attrs(&look)));
    let bounds = grow(Rect::of_points(&path.hull()).unwrap_or_default(), look.width.max(14.0) / 2.0);
    Fragment { svg: wrap_opacity(out, look.opacity), bounds, local: false, slot: None, size: None }
}

fn grow(rect: Rect, by: f64) -> Rect {
    Rect::new(rect.x0 - by, rect.y0 - by, rect.x1 + by, rect.y1 + by)
}

/// The length and half width of an arrowhead on a line `width` wide.
fn arrow_size(width: f64) -> (f64, f64) {
    let length = 8.0 + 2.0 * width;
    (length, length * 0.45)
}

fn arrowhead(out: &mut String, kind: &str, tip: Point, direction: Point, look: &Look) {
    match kind {
        "arrow" => {
            let (length, half) = arrow_size(look.width);
            let base = Point::new(tip.x - direction.x * length, tip.y - direction.y * length);
            let normal = Point::new(-direction.y, direction.x);
            out.push_str(&format!(
                r#"<path d="M {} {} L {} {} L {} {} Z" fill="{}" stroke="{}" stroke-width="{}" stroke-linejoin="round"/>"#,
                num(tip.x),
                num(tip.y),
                num(base.x + normal.x * half),
                num(base.y + normal.y * half),
                num(base.x - normal.x * half),
                num(base.y - normal.y * half),
                look.stroke,
                look.stroke,
                num(look.width.min(2.0)),
            ));
        }
        "dot" => out.push_str(&format!(r#"<circle cx="{}" cy="{}" r="{}" fill="{}"/>"#, num(tip.x), num(tip.y), num(2.5 + look.width), look.stroke)),
        _ => {}
    }
}

fn connector(model: &BoardModel, element: &Element, options: &Options<'_>) -> Fragment {
    let look = style::look(element, options.colors);
    let route_points = model.route(element);
    let path =
        route::connector(&route_points, text_of(element, "connectorType"), text_of(element, "connectorRole"), text_of(element, "mindMapBranchAxis"));
    let Some(path) = path else {
        return Fragment { svg: String::new(), bounds: Rect::of_points(&route_points).unwrap_or_default(), local: false, slot: None, size: None };
    };
    let start_head = text_of(element, "startArrowhead").unwrap_or("none");
    let end_head = text_of(element, "endArrowhead").unwrap_or("arrow");
    let inset = |head: &str| if head == "arrow" { arrow_size(look.width).0 * 0.8 } else { 0.0 };
    let drawn: Path = path.trimmed(inset(start_head), inset(end_head));
    let mut out = format!(
        r#"<path d="{}" fill="none" stroke="transparent" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round" class="kb-hit"/>"#,
        path.d(),
        num((look.width + 10.0).max(14.0))
    );
    out.push_str(&format!(r#"<path d="{}" fill="none"{}/>"#, drawn.d(), stroke_attrs(&look)));
    let solid = Look { dash: None, ..look.clone() };
    if let Some(direction) = path.end_direction() {
        arrowhead(&mut out, end_head, path.end(), direction, &solid);
    }
    if let Some(direction) = path.start_direction() {
        arrowhead(&mut out, start_head, path.start, direction, &solid);
    }
    let mut bounds = grow(Rect::of_points(&path.hull()).unwrap_or_default(), arrow_size(look.width).0.max(7.0));
    if !element.text.is_empty() {
        let polyline: Vec<Point> = std::iter::once(path.start).chain(path.segs.iter().map(route::Seg::end)).collect();
        let at = route::along(&polyline, number_of(element, "labelPosition").unwrap_or(0.5)).unwrap_or(path.start);
        let style = font(element, 14.0, 500);
        let lines = layout(&element.text, style, Some(200.0));
        let widest = lines.iter().map(|line| line.width).fold(0.0, f64::max);
        let height = lines.len() as f64 * style.line_height();
        let rect = Rect::new(at.x - widest / 2.0 - 6.0, at.y - height / 2.0 - 3.0, at.x + widest / 2.0 + 6.0, at.y + height / 2.0 + 3.0);
        out.push_str(&format!(
            r#"<rect x="{}" y="{}" width="{}" height="{}" rx="4" fill="{}"/>"#,
            num(rect.x0),
            num(rect.y0),
            num(rect.width()),
            num(rect.height()),
            style::named(PAPER, options.colors)
        ));
        let placed = place(&lines, style, widest, 0.0, at.y - height / 2.0, "center");
        let shifted: Vec<_> = placed
            .into_iter()
            .map(|mut placed| {
                placed.x += at.x - widest / 2.0;
                placed
            })
            .collect();
        text_block(&mut out, &shifted, style, &look.stroke, options.lod == Lod::Low);
        bounds = bounds.union(rect);
    }
    Fragment { svg: wrap_opacity(out, look.opacity), bounds, local: false, slot: None, size: None }
}
