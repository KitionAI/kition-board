//! The page as pixels: the same fragments the editor draws, put together into one SVG (literal
//! colours, pictures inline) and rasterised with resvg on the faces registered with `addFont`.
//! This serves the contract's `render` and `render_area` (minimap, agent previews, thumbnails).
//!
//! **The document frame.** Document pixel (0, 0) is world point (`x`, `y`) of [`Area`], and one
//! document pixel is `unit` board units (1 unless the content is wider or taller than
//! [`kition_kit::MAX_RENDER_SIDE`]). A board made with a size (`create(name, width, height)`) is
//! that document from the origin, grown with a margin only on a side the content goes past; a board
//! without one is the page's content with a margin, at least one 1920 × 1080 view (an empty page:
//! that view at the origin).
//!
//! **Exactness.** `render_area` at a whole-number reduction is the exact box mean of the 1:1
//! pixels (a 1:1 tile of the area, averaged by `kition_kit::raster`), as hosts patching a cached
//! frame need; so is `render` while the area it shows at 1:1 stays under [`TILE_LIMIT`] pixels.
//! Larger views (a whole board as a thumbnail) are drawn straight at their scale.

use std::sync::{Arc, Mutex};

use kition_board_kdm::{BoardModel, Element, Kind, PageView, Rect};
use kition_kit::raster::{self as sdk_raster, Frame};
use kition_kit::session::BlobSource;
use resvg::{tiny_skia, usvg};
use serde_json::{Value, json};

use crate::fragment::{Lod, Options, Pictures, fragment};
use crate::style::Colors;
use crate::svg::num;

/// Space around the content, in board units.
pub const MARGIN: f64 = 40.0;
/// The smallest frame: one 1920 × 1080 view.
pub const MIN_WIDTH: f64 = 1920.0;
pub const MIN_HEIGHT: f64 = 1080.0;
/// The most pixels a 1:1 tile may have before a view is drawn straight at its scale.
pub const TILE_LIMIT: u64 = 4096 * 4096;

/// Where the document frame is on the board.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    /// Board units per document pixel.
    pub unit: f64,
    pub width: u32,
    pub height: u32,
}

impl Area {
    pub fn to_json(&self) -> Value {
        json!({ "x": self.x, "y": self.y, "unit": self.unit, "width": self.width, "height": self.height })
    }
}

/// The frame of `page`. A board with a size ([`BoardModel::size`]) is that document from the
/// origin, grown (with the margin) only on a side the content goes past; a board without one is its
/// content with the margin, at least one view (an empty or absent page: that view at the origin).
pub fn area(model: &BoardModel, page: Option<&PageView>) -> Area {
    let side = f64::from(kition_kit::MAX_RENDER_SIDE);
    let content =
        page.and_then(|page| model.content_bounds(page)).filter(|r| r.x0.is_finite() && r.y0.is_finite() && r.x1.is_finite() && r.y1.is_finite());
    let frame = match (model.size(), content) {
        (Some(size), content) => {
            let document = Rect::new(0.0, 0.0, size.w, size.h);
            match content {
                Some(c) => Rect::new(
                    if c.x0 < document.x0 { (c.x0 - MARGIN).floor() } else { document.x0 },
                    if c.y0 < document.y0 { (c.y0 - MARGIN).floor() } else { document.y0 },
                    if c.x1 > document.x1 { (c.x1 + MARGIN).ceil() } else { document.x1.ceil() },
                    if c.y1 > document.y1 { (c.y1 + MARGIN).ceil() } else { document.y1.ceil() },
                ),
                None => Rect::new(0.0, 0.0, document.x1.ceil(), document.y1.ceil()),
            }
        }
        (None, Some(c)) => {
            let (x, y) = ((c.x0 - MARGIN).floor(), (c.y0 - MARGIN).floor());
            Rect::new(x, y, x + ((c.x1 + MARGIN).ceil() - x).max(MIN_WIDTH), y + ((c.y1 + MARGIN).ceil() - y).max(MIN_HEIGHT))
        }
        (None, None) => Rect::new(0.0, 0.0, MIN_WIDTH, MIN_HEIGHT),
    };
    let (w, h) = (frame.width().max(1.0), frame.height().max(1.0));
    let unit = (w.max(h) / side).ceil().max(1.0);
    let pixels = |extent: f64| ((extent / unit).ceil()).clamp(1.0, side) as u32;
    Area { x: frame.x0, y: frame.y0, unit, width: pixels(w), height: pixels(h) }
}

/// `view` (board units) of `page` as a PNG of `width` × `height` pixels on white: what the editor
/// draws there, every element included, with nothing of the editor's (no selection, no viewport).
/// A function of the board and the arguments: the same call encodes the same bytes.
pub fn picture(model: &BoardModel, page: &PageView, view: Rect, width: u32, height: u32, blobs: &dyn BlobSource) -> Result<Vec<u8>, String> {
    let svg = scene_svg(model, page, view, (f64::from(width), f64::from(height)), Some("#ffffff"), blobs, &|_| true);
    let options = usvg::Options { fontdb: fonts(), font_family: "sans-serif".to_owned(), ..usvg::Options::default() };
    let tree = usvg::Tree::from_str(&svg, &options).map_err(|error| format!("the board scene did not parse: {error}"))?;
    let mut pixmap = tiny_skia::Pixmap::new(width.max(1), height.max(1)).ok_or("the picture is too large to draw")?;
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|error| format!("the picture was not encoded: {error}"))
}

/// The page as one SVG document over `area`.
pub fn page_svg(model: &BoardModel, page: &PageView, area: &Area, blobs: &dyn BlobSource) -> String {
    let (w, h) = (f64::from(area.width) * area.unit, f64::from(area.height) * area.unit);
    let view = Rect::new(area.x, area.y, area.x + w, area.y + h);
    scene_svg(model, page, view, (f64::from(area.width), f64::from(area.height)), Some("#ffffff"), blobs, &|_| true)
}

/// The elements of `page` that `include` takes, as one SVG document showing `view` (board
/// units) at `size` (its `width` and `height`), on `background` (none: transparent). Colours are
/// literal and pictures inline; what a collapsed mind-map node hides is left out.
pub fn scene_svg(
    model: &BoardModel,
    page: &PageView,
    view: Rect,
    size: (f64, f64),
    background: Option<&str>,
    blobs: &dyn BlobSource,
    include: &dyn Fn(&Element) -> bool,
) -> String {
    let options = Options { colors: Colors::Literal, lod: Lod::Full, pictures: Pictures::Inline, blobs };
    let mut out = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="{} {} {} {}">"#,
        num(size.0),
        num(size.1),
        num(view.x0),
        num(view.y0),
        num(view.width()),
        num(view.height()),
    );
    if let Some(background) = background {
        out.push_str(&format!(
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{background}"/>"#,
            num(view.x0),
            num(view.y0),
            num(view.width()),
            num(view.height())
        ));
    }
    // What a collapsed mind-map node hides is not drawn.
    let hidden = kition_board_kdm::mindmap::hidden(model, page);
    for element in model.elements_of(page).filter(|element| element.kind != Kind::Group && !hidden.contains(&element.id) && include(element)) {
        let piece = fragment(model, element, &options);
        if piece.svg.is_empty() {
            continue;
        }
        match model.rect(element).filter(|_| piece.local) {
            Some(rect) => {
                let rotate = if element.rotation != 0.0 && element.rotation.is_finite() {
                    format!(" rotate({} {} {})", num(element.rotation), num(rect.width() / 2.0), num(rect.height() / 2.0))
                } else {
                    String::new()
                };
                out.push_str(&format!(r#"<g transform="translate({} {}){rotate}">{}</g>"#, num(rect.x0), num(rect.y0), piece.svg));
            }
            None => out.push_str(&piece.svg),
        }
    }
    out.push_str("</svg>");
    out
}

/// The font database of the registered faces, rebuilt when faces are added.
pub(crate) fn fonts() -> Arc<usvg::fontdb::Database> {
    static CACHE: Mutex<Option<(u64, Arc<usvg::fontdb::Database>)>> = Mutex::new(None);
    let epoch = kition_board_text::font_epoch();
    if let Ok(cache) = CACHE.lock()
        && let Some((at, db)) = cache.as_ref()
        && *at == epoch
    {
        return db.clone();
    }
    let mut db = usvg::fontdb::Database::new();
    for data in kition_board_text::font_files() {
        db.load_font_data(data.to_vec());
    }
    let db = Arc::new(db);
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some((epoch, db.clone()));
    }
    db
}

/// A page ready to rasterise.
pub struct Raster {
    pub area: Area,
    tree: usvg::Tree,
}

impl Raster {
    /// Parses the page's SVG.
    pub fn build(model: &BoardModel, page: Option<&PageView>, blobs: &dyn BlobSource) -> Result<Raster, String> {
        let area = area(model, page);
        let svg = match page {
            Some(page) => page_svg(model, page, &area, blobs),
            None => format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}"><rect width="100%" height="100%" fill="#ffffff"/></svg>"##,
                area.width, area.height
            ),
        };
        let options = usvg::Options { fontdb: fonts(), font_family: "sans-serif".to_owned(), ..usvg::Options::default() };
        let tree = usvg::Tree::from_str(&svg, &options).map_err(|error| format!("the board scene did not parse: {error}"))?;
        Ok(Raster { area, tree })
    }

    /// Straight RGBA of the document pixels from (`x`, `y`) at `scale`, `width` × `height`.
    fn draw(&self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let mut pixmap = tiny_skia::Pixmap::new(width.max(1), height.max(1)).ok_or("the render area is too large")?;
        let (x, y, scale) = (finite(x), finite(y), if scale.is_finite() && scale > 0.0 { scale } else { 1.0 });
        let transform = tiny_skia::Transform::from_row(scale as f32, 0.0, 0.0, scale as f32, (-x * scale) as f32, (-y * scale) as f32);
        resvg::render(&self.tree, transform, &mut pixmap.as_mut());
        let mut out = Vec::with_capacity(width as usize * height as usize * 4);
        for pixel in pixmap.pixels() {
            let color = pixel.demultiply();
            out.extend_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
        }
        if width == 0 || height == 0 {
            out.clear();
        }
        Ok(out)
    }

    /// [`kition_kit::KitDocument::render`].
    pub fn render(&self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        let (x, y) = (finite(x), finite(y));
        let (tx, ty) = (x.floor(), y.floor());
        let tw = ((x + f64::from(width) / scale).ceil() - tx).max(1.0);
        let th = ((y + f64::from(height) / scale).ceil() - ty).max(1.0);
        if tw * th > TILE_LIMIT as f64 || tw > f64::from(u32::MAX) || th > f64::from(u32::MAX) {
            return self.draw(x, y, scale, width, height);
        }
        let (tw, th) = (tw as u32, th as u32);
        let tile = self.draw(tx, ty, 1.0, tw, th)?;
        Ok(sdk_raster::render(Frame { rgba: &tile, width: tw, height: th }, x - tx, y - ty, scale, width, height))
    }

    /// [`kition_kit::KitDocument::render_area`].
    pub fn render_area(&self, x0: i32, y0: i32, x1: i32, y1: i32, width: u32, height: u32) -> Result<Vec<u8>, String> {
        let (left, top) = (x0.min(x1), y0.min(y1));
        let (span_x, span_y) = (u64::from(x0.abs_diff(x1)).max(1), u64::from(y0.abs_diff(y1)).max(1));
        if span_x * span_y > TILE_LIMIT || span_x > u64::from(u32::MAX) || span_y > u64::from(u32::MAX) {
            let scale = width as f64 / span_x as f64;
            return self.draw(f64::from(left), f64::from(top), scale, width, height);
        }
        let (tw, th) = (span_x as u32, span_y as u32);
        let tile = self.draw(f64::from(left), f64::from(top), 1.0, tw, th)?;
        Ok(sdk_raster::render_area(Frame { rgba: &tile, width: tw, height: th }, 0, 0, tw as i32, th as i32, width, height))
    }
}

fn finite(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}
