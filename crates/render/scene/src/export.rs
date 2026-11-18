//! Exports (design note 5.1 `file.export`): a page, or one frame and what is in it, as PNG, SVG or
//! PDF, drawn from the same scene the editor shows (`raster::scene_svg`). SVG is written with the
//! text as outlines, so it shows the same anywhere; PDF is vector (krilla-svg), with the text
//! embedded; PNG is the raster at a scale.
//!
//! Ported from Kition's `packages/board-editor/src/lib/boardExport.ts` and `boardFrameExport.ts`
//! (what an export covers: the page's content with a margin, or a frame and its descendants); the
//! drawing is the engine's scene instead of v1's DOM snapshot.

use std::collections::HashSet;

use kition_board_kdm::{BoardModel, Element, PageView, Rect};
use kition_kit::kdm::ObjectId;
use kition_kit::session::BlobSource;
use resvg::{tiny_skia, usvg};

use crate::raster::{MARGIN, MIN_HEIGHT, MIN_WIDTH, fonts, scene_svg};

/// The formats `file.export` writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Svg,
    Pdf,
}

impl Format {
    pub fn parse(name: &str) -> Option<Format> {
        match name {
            "png" => Some(Format::Png),
            "svg" => Some(Format::Svg),
            "pdf" => Some(Format::Pdf),
            _ => None,
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Format::Png => "image/png",
            Format::Svg => "image/svg+xml",
            Format::Pdf => "application/pdf",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Svg => "svg",
            Format::Pdf => "pdf",
        }
    }
}

/// The most pixels a PNG export may have, and its longest side.
pub const MAX_PIXELS: f64 = 64.0 * 1024.0 * 1024.0;
pub const MAX_SIDE: f64 = 16384.0;
/// PDF points per board unit (one unit is one CSS pixel).
const POINTS_PER_UNIT: f64 = 0.75;

/// An exported file.
#[derive(Clone, Debug)]
pub struct Exported {
    pub bytes: Vec<u8>,
    /// In pixels for PNG, in board units otherwise.
    pub width: f64,
    pub height: f64,
}

/// What to export.
pub struct Request<'a> {
    pub format: Format,
    /// One frame and its descendants instead of the whole page.
    pub frame: Option<&'a Element>,
    /// PNG pixels per board unit.
    pub scale: f64,
    /// A white page behind the elements (else transparent; PDF pages are always white).
    pub background: bool,
    /// A PNG too large at `scale` is drawn at the largest scale that fits (v1's
    /// `getWhiteboardPngRasterSize`) instead of refused.
    pub fit: bool,
}

/// The part of the board an export covers: the frame's box, or the page's content with a margin
/// (a 1920 × 1080 view at the origin for an empty page).
pub fn covered(model: &BoardModel, page: &PageView, frame: Option<&Element>) -> Rect {
    if let Some(rect) = frame.and_then(|frame| model.rect(frame)) {
        return rect;
    }
    match model.content_bounds(page).filter(|r| [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite())) {
        Some(content) => Rect::new(content.x0 - MARGIN, content.y0 - MARGIN, content.x1 + MARGIN, content.y1 + MARGIN),
        None => Rect::new(0.0, 0.0, MIN_WIDTH, MIN_HEIGHT),
    }
}

/// `page` (or a frame on it) as a file.
pub fn export(model: &BoardModel, page: &PageView, request: &Request<'_>, blobs: &dyn BlobSource) -> Result<Exported, String> {
    let view = covered(model, page, request.frame);
    let (w, h) = (view.width().max(1.0), view.height().max(1.0));
    let inside: Option<HashSet<ObjectId>> = request.frame.map(|frame| model.with_descendants(&frame.id).into_iter().collect());
    let include = |element: &Element| inside.as_ref().is_none_or(|ids| ids.contains(&element.id));
    let background = (request.background || request.format == Format::Pdf).then_some("#ffffff");
    let tree_of = |blobs: &dyn BlobSource| {
        let svg = scene_svg(model, page, view, (w, h), background, blobs, &include);
        let options = usvg::Options { fontdb: fonts(), font_family: "sans-serif".to_owned(), ..usvg::Options::default() };
        usvg::Tree::from_str(&svg, &options).map_err(|error| format!("the board scene did not parse: {error}"))
    };
    match request.format {
        Format::Svg => Ok(Exported { bytes: tree_of(blobs)?.to_string(&usvg::WriteOptions::default()).into_bytes(), width: w, height: h }),
        Format::Png => {
            let mut scale = if request.scale.is_finite() && request.scale > 0.0 { request.scale } else { 1.0 };
            if request.fit {
                scale = scale.min(MAX_SIDE / w).min(MAX_SIDE / h).min((MAX_PIXELS / (w * h)).sqrt());
            }
            let (pw, ph) = ((w * scale).ceil(), (h * scale).ceil());
            if pw > MAX_SIDE || ph > MAX_SIDE || pw * ph > MAX_PIXELS {
                return Err(format!(
                    "invalid-params: a {pw} × {ph} picture is larger than an export may be ({MAX_SIDE} on a side, {} megapixels); give a smaller scale or a frame",
                    MAX_PIXELS / 1_048_576.0
                ));
            }
            let mut pixmap = tiny_skia::Pixmap::new(pw as u32, ph as u32).ok_or("the picture is too large to draw")?;
            resvg::render(&tree_of(blobs)?, tiny_skia::Transform::from_scale(scale as f32, scale as f32), &mut pixmap.as_mut());
            let bytes = pixmap.encode_png().map_err(|error| format!("the picture was not encoded: {error}"))?;
            Ok(Exported { bytes, width: pw, height: ph })
        }
        Format::Pdf => {
            let size =
                krilla::geom::Size::from_wh((w * POINTS_PER_UNIT) as f32, (h * POINTS_PER_UNIT) as f32).ok_or("the page is too small for a PDF")?;
            // The PDF writer decodes pictures itself and refuses a damaged one: those are drawn as
            // missing pictures (as the editor draws a picture whose bytes it lacks), and if the
            // writer still refuses, every picture is.
            let checked = Without { inner: blobs, broken: damaged_pictures(model, page, blobs) };
            let bytes = match pdf(&tree_of(&checked)?, size) {
                Ok(bytes) => bytes,
                Err(_) => pdf(&tree_of(&Without { inner: blobs, broken: all_pictures(model, page) })?, size)?,
            };
            Ok(Exported { bytes, width: w, height: h })
        }
    }
}

/// One page of vector PDF holding `tree` at `size` (points).
fn pdf(tree: &usvg::Tree, size: krilla::geom::Size) -> Result<Vec<u8>, String> {
    let mut document = krilla::Document::new();
    {
        let mut pdf_page = document.start_page_with(krilla::page::PageSettings::new(size));
        let mut surface = pdf_page.surface();
        krilla_svg::SurfaceExt::draw_svg(&mut surface, tree, size, krilla_svg::SvgSettings::default())
            .ok_or("the board did not draw into the PDF")?;
        surface.finish();
        pdf_page.finish();
    }
    document.finish().map_err(|error| format!("the PDF was not written: {error:?}"))
}

/// The pictures on `page` whose PNG bytes do not decode.
fn damaged_pictures(model: &BoardModel, page: &PageView, blobs: &dyn BlobSource) -> HashSet<String> {
    model
        .elements_of(page)
        .filter_map(|element| element.image.as_ref())
        .filter(|image| image.mime == "image/png")
        .filter(|image| blobs.get(&image.hash).is_some_and(|bytes| tiny_skia::Pixmap::decode_png(&bytes).is_err()))
        .map(|image| image.hash.clone())
        .collect()
}

fn all_pictures(model: &BoardModel, page: &PageView) -> HashSet<String> {
    model.elements_of(page).filter_map(|element| element.image.as_ref()).map(|image| image.hash.clone()).collect()
}

/// A blob store without the `broken` blobs.
struct Without<'a> {
    inner: &'a dyn BlobSource,
    broken: HashSet<String>,
}

impl BlobSource for Without<'_> {
    fn get(&self, hash: &str) -> Option<std::sync::Arc<[u8]>> {
        if self.broken.contains(hash) { None } else { self.inner.get(hash) }
    }
}
