//! Draws every shape of `shapeType` on one board, labelled with its name and named again under it,
//! and writes the board as a PNG, for a person to look at: a closed outline, nothing clipped at the
//! box, the label inside the shape. The tests check only that shapes are distinct and in their box.
//!
//! `cargo run -p kition-board-kit --example shape_sheet -- /tmp/board-shapes.png` (with
//! kition-fonts beside the repository or `KITION_FONTS_DIR` set, for the labels' glyphs).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::{Path, PathBuf};

use kition_board_kit::BoardDocument;
use kition_board_model::fields::SHAPE_TYPES;
use kition_kit::KitDocument;
use serde_json::json;

const COLUMNS: usize = 11;
const CELL: (f64, f64) = (160.0, 200.0);
const SHAPE: (f64, f64) = (120.0, 80.0);
const BOARD: (u32, u32) = (1840, 900);
const FONTS: &[&str] = &["fonts/inter/Inter-Variable.ttf", "fonts/droid-sans-fallback/DroidSansFallbackFull.ttf"];

fn main() {
    let Some(target) = std::env::args().nth(1) else {
        eprintln!("usage: shape_sheet <out.png>");
        std::process::exit(2);
    };
    let fonts = std::env::var_os("KITION_FONTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../kition-fonts"));
    let mut faces = 0;
    for font in FONTS {
        match std::fs::read(fonts.join(font)) {
            Ok(bytes) => faces += kition_board_kit::add_font(&bytes),
            Err(error) => eprintln!("{}: {error}", fonts.join(font).display()),
        }
    }
    if faces == 0 {
        eprintln!("no fonts (KITION_FONTS_DIR, or kition-fonts beside the repository): the labels have no glyphs");
    }

    let mut doc = BoardDocument::create("Shapes.kboard", BOARD.0, BOARD.1).unwrap();
    let mut elements = Vec::new();
    for (index, name) in SHAPE_TYPES.iter().enumerate() {
        let (column, row) = (index % COLUMNS, index / COLUMNS);
        let (x, y) = (40.0 + column as f64 * CELL.0, 40.0 + row as f64 * CELL.1);
        let (sx, sy) = (x + (CELL.0 - SHAPE.0) / 2.0, y + 20.0);
        elements.push(json!({ "kind": "shape", "shapeType": name, "position": [sx, sy], "size": [SHAPE.0, SHAPE.1], "text": name }));
        elements.push(json!({ "kind": "text", "position": [x, sy + SHAPE.1 + 16.0], "size": [CELL.0, 24.0], "text": name, "textAlign": "center", "fontSize": 13 }));
    }
    doc.execute("element.add", json!({ "elements": elements })).unwrap();

    let rgba = doc.render(0.0, 0.0, 1.0, BOARD.0, BOARD.1).unwrap();
    let picture = image::RgbaImage::from_raw(BOARD.0, BOARD.1, rgba).unwrap();
    picture.save_with_format(&target, image::ImageFormat::Png).unwrap();
    println!("{target}: {} shapes, {} x {}", SHAPE_TYPES.len(), BOARD.0, BOARD.1);
}
