//! `kition-board-scene`: how a board looks (task B4).
//!
//! Every element is one SVG fragment ([`fragment`]): shapes, text, pictures, cards and frames in
//! their local frame, strokes and connectors in world coordinates. The in-page editor draws these
//! fragments as they are (R2c; it has no drawing code of its own) and caches them by appearance
//! revision ([`cache`], the `elementSvg` extra). The raster ([`raster`]) puts the same fragments
//! together into one SVG and rasterises it, so previews, thumbnails and the minimap show exactly
//! what the editor shows.
//!
//! Text is laid out here with the registered fonts (`kition-board-text`): one `tspan` per line,
//! each with an explicit `x` and `y`, kerning and ligatures off, so a browser draws the advances
//! the engine measured.

pub mod cache;
pub mod export;
pub mod fragment;
pub mod raster;
pub mod route;
pub mod shapes;
pub mod style;
pub mod svg;

pub use cache::Revisions;
pub use fragment::{Fragment, Lod, Options, Pictures, TextFrame, fragment};
pub use raster::{Area, Raster, area, picture};
pub use style::Colors;

#[cfg(test)]
mod tests;
