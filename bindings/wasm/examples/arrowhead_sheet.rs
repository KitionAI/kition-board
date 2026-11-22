//! Draws every arrowhead at both ends of a horizontal and a diagonal connector, at the line weights
//! `m` (left) and `xl` (right), each labelled with its head's name, and writes the board as a PNG,
//! for a person to look at: the tip on the end point, pointing out of the line, open heads hollow,
//! sizes in step with the line. The tests check only that heads are distinct, finite and filled
//! or hollow.
//!
//! `cargo run -p kition-board-kit --example arrowhead_sheet -- /tmp/board-arrowheads.png` (with
//! kition-fonts beside the repository or `KITION_FONTS_DIR` set, for the labels' glyphs).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::{Path, PathBuf};

use kition_board_kit::BoardDocument;
use kition_board_model::fields::ARROWHEADS;
use kition_kit::KitDocument;
use serde_json::json;

const ROW: f64 = 120.0;
const BLOCK: f64 = 780.0;
const BOARD: (u32, u32) = (1600, 1150);
const FONTS: &[&str] = &["fonts/inter/Inter-Variable.ttf", "fonts/droid-sans-fallback/DroidSansFallbackFull.ttf"];

fn main() {
    let Some(target) = std::env::args().nth(1) else {
        eprintln!("usage: arrowhead_sheet <out.png>");
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

    let mut doc = BoardDocument::create("Arrowheads.kboard", BOARD.0, BOARD.1).unwrap();
    for (block, size) in ["m", "xl"].iter().enumerate() {
        let x0 = 40.0 + block as f64 * BLOCK;
        for (index, head) in ARROWHEADS.iter().enumerate() {
            let y = 60.0 + index as f64 * ROW;
            for (from, to) in [([x0 + 30.0, y + 20.0], [x0 + 360.0, y + 20.0]), ([x0 + 470.0, y - 25.0], [x0 + 680.0, y + 70.0])] {
                doc.execute(
                    "connector.create",
                    json!({ "from": { "point": from }, "to": { "point": to }, "label": head, "startArrowhead": head, "endArrowhead": head, "style": { "strokeSize": size } }),
                )
                .unwrap();
            }
        }
    }

    let rgba = doc.render(0.0, 0.0, 1.0, BOARD.0, BOARD.1).unwrap();
    let picture = image::RgbaImage::from_raw(BOARD.0, BOARD.1, rgba).unwrap();
    picture.save_with_format(&target, image::ImageFormat::Png).unwrap();
    println!("{target}: {} arrowheads at m and xl, {} x {}", ARROWHEADS.len(), BOARD.0, BOARD.1);
}
