//! Builds an order-handling flow chart through engine commands only and writes the board as a PNG,
//! for a person to look at: every connector end on its shape's outline (diamond and ellipse
//! corners included), heads pointing into their shapes, labels inside the shapes, the branch labels
//! readable. It then prints what `board_check` reports on the board (nothing, for a clean chart).
//!
//! `cargo run -p kition-board-kit --example flowchart_sheet -- /tmp/board-flowchart.png` (with
//! kition-fonts beside the repository or `KITION_FONTS_DIR` set, for the labels' glyphs).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::{Path, PathBuf};

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use serde_json::{Value, json};

const BOARD: (u32, u32) = (1500, 1000);
const FONTS: &[&str] = &["fonts/inter/Inter-Variable.ttf", "fonts/droid-sans-fallback/DroidSansFallbackFull.ttf"];

/// One shape: its type, label, top-left and size.
fn shape(doc: &mut BoardDocument, shape_type: &str, text: &str, at: [f64; 2], size: [f64; 2]) -> Value {
    let element = json!({ "kind": "shape", "shapeType": shape_type, "shapeStyle": "flow-node", "position": at, "size": size, "text": text });
    doc.execute("element.create", json!({ "elements": [element] })).unwrap()["created"][0].clone()
}

/// A connector from `from` at `from_anchor` to `to` at `to_anchor` (`None`: the edge facing the
/// other end), with `extra` (label, heads, type, style).
fn connect(doc: &mut BoardDocument, from: &Value, from_anchor: Option<[f64; 2]>, to: &Value, to_anchor: Option<[f64; 2]>, extra: Value) -> Value {
    let end = |id: &Value, anchor: Option<[f64; 2]>| match anchor {
        Some(anchor) => json!({ "element": id, "anchor": anchor }),
        None => json!({ "element": id }),
    };
    let mut params = json!({ "from": end(from, from_anchor), "to": end(to, to_anchor) });
    params.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    doc.execute("connector.create", params).unwrap()["created"][0].clone()
}

fn main() {
    let Some(target) = std::env::args().nth(1) else {
        eprintln!("usage: flowchart_sheet <out.png>");
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

    let mut doc = BoardDocument::create("Orders.kboard", BOARD.0, BOARD.1).unwrap();
    // The main line runs down the left, across the middle and down to the end on the right.
    let start = shape(&mut doc, "pill", "Start", [80.0, 50.0], [160.0, 64.0]);
    let enter = shape(&mut doc, "manual-input", "Enter order", [60.0, 190.0], [200.0, 90.0]);
    let clerk = shape(&mut doc, "actor", "Clerk", [320.0, 170.0], [90.0, 120.0]);
    let stock = shape(&mut doc, "diamond", "In stock?", [60.0, 380.0], [200.0, 140.0]);
    let reserve = shape(&mut doc, "predefined-process", "Reserve items", [420.0, 405.0], [200.0, 90.0]);
    let orders = shape(&mut doc, "cylinder", "Orders", [740.0, 390.0], [140.0, 120.0]);
    let inventory = shape(&mut doc, "ellipse", "Inventory", [960.0, 170.0], [180.0, 100.0]);
    let invoice = shape(&mut doc, "document", "Print invoice", [1000.0, 405.0], [200.0, 90.0]);
    let ship = shape(&mut doc, "rectangle", "Ship", [1000.0, 620.0], [200.0, 90.0]);
    let end = shape(&mut doc, "pill", "End", [1280.0, 800.0], [160.0, 64.0]);
    let back = shape(&mut doc, "delay", "Back-order", [100.0, 650.0], [200.0, 90.0]);
    let away = shape(&mut doc, "off-page", "A", [150.0, 830.0], [100.0, 110.0]);
    let note = shape(&mut doc, "callout", "Stock is counted every night", [420.0, 190.0], [240.0, 110.0]);

    let flow = |label: Option<&str>| {
        let mut extra = json!({ "type": "elbow", "endArrowhead": "triangle" });
        if let Some(label) = label {
            extra["label"] = json!(label);
        }
        extra
    };
    connect(&mut doc, &start, Some([0.5, 1.0]), &enter, Some([0.5, 0.0]), flow(None));
    connect(&mut doc, &enter, Some([0.5, 1.0]), &stock, Some([0.5, 0.0]), flow(None));
    // The two branches leave the diamond at its right and left corners; "no" bends down to the
    // delay's left side.
    connect(&mut doc, &stock, Some([1.0, 0.5]), &reserve, Some([0.0, 0.5]), flow(Some("yes")));
    connect(&mut doc, &stock, Some([0.0, 0.5]), &back, Some([0.0, 0.5]), flow(Some("no")));
    connect(&mut doc, &reserve, Some([1.0, 0.5]), &orders, Some([0.0, 0.5]), flow(None));
    connect(&mut doc, &orders, Some([1.0, 0.5]), &invoice, Some([0.0, 0.5]), flow(None));
    connect(&mut doc, &invoice, Some([0.5, 1.0]), &ship, Some([0.5, 0.0]), flow(None));
    connect(&mut doc, &ship, Some([1.0, 0.5]), &end, Some([0.5, 0.0]), flow(None));
    connect(&mut doc, &back, Some([0.5, 1.0]), &away, Some([0.5, 0.0]), flow(None));
    // The note points at the diamond's upper right side (its box corner, on the outline).
    connect(&mut doc, &note, Some([0.0, 1.0]), &stock, Some([1.0, 0.0]), json!({ "endArrowhead": "none", "style": { "dashStyle": "dashed" } }));
    // Associations: the clerk enters orders; the orders hold the inventory (to the ellipse's
    // lower left, off its box corner).
    connect(&mut doc, &clerk, None, &enter, None, json!({ "startArrowhead": "circle-open", "endArrowhead": "none" }));
    connect(&mut doc, &orders, Some([0.5, 0.0]), &inventory, Some([0.0, 1.0]), json!({ "startArrowhead": "diamond-open", "endArrowhead": "none" }));

    let rgba = doc.render(0.0, 0.0, 1.0, BOARD.0, BOARD.1).unwrap();
    let picture = image::RgbaImage::from_raw(BOARD.0, BOARD.1, rgba).unwrap();
    picture.save_with_format(&target, image::ImageFormat::Png).unwrap();
    println!("{target}: an order flow, {} x {}", BOARD.0, BOARD.1);

    // Every connector's ends as stored and as a reader derives them.
    let page = doc.model().pages()[0].clone();
    for element in doc.model().elements_of(&page).filter(|element| element.kind.as_str() == "connector") {
        let ends: Vec<String> = [&element.start, &element.end]
            .into_iter()
            .flatten()
            .map(|end| {
                let derived = doc.model().end_point(&element.id, end);
                format!("({:.1}, {:.1})", derived.x, derived.y)
            })
            .collect();
        println!("connector {:?}: {}", element.text, ends.join(" -> "));
    }

    let diagnostics = doc.check();
    println!("board_check: {} diagnostics", diagnostics.len());
    for diagnostic in &diagnostics {
        println!("  {}", serde_json::to_string(diagnostic).unwrap());
    }
}
