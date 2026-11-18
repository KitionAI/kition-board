//! Regression (`regressions/issue-3`): a `line` shape runs along its box's longer side (a
//! tall box draws a vertical line, as a wide one draws a horizontal line), and a stroke takes a
//! dash style when it is drawn (`stroke.create`) and later (`element.update`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use serde_json::{Value, json};

fn created(value: &Value) -> Vec<String> {
    value["created"].as_array().unwrap().iter().map(|id| id.as_str().unwrap().to_owned()).collect()
}

/// Whether board point (x, y) is drawn dark.
fn dark(doc: &BoardDocument, x: f64, y: f64) -> bool {
    let px = doc.render(x, y, 1.0, 1, 1).unwrap();
    px[0] < 128 && px[1] < 128 && px[2] < 128
}

fn fragment(doc: &BoardDocument, id: &str) -> String {
    let answer: Value = serde_json::from_str(&doc.element_svg(&json!({ "ids": [id] }).to_string()).unwrap()).unwrap();
    answer["fragments"][0]["svg"].as_str().unwrap().to_owned()
}

#[test]
fn a_tall_line_is_vertical() {
    let mut doc = BoardDocument::create("Lines.kboard", 800, 600).unwrap();
    doc.execute(
        "element.create",
        json!({ "elements": [
            { "kind": "shape", "shapeType": "line", "position": [100, 100], "size": [20, 300], "strokeColor": "#000000", "strokeSize": "l" },
            { "kind": "shape", "shapeType": "line", "position": [300, 100], "size": [300, 20], "strokeColor": "#000000", "strokeSize": "l" },
        ] }),
    )
    .unwrap();
    // The tall box: down its middle, top to bottom.
    for y in [110.0, 250.0, 390.0] {
        assert!(dark(&doc, 110.0, y), "the tall line passes (110, {y})");
    }
    assert!(!dark(&doc, 102.0, 250.0) && !dark(&doc, 118.0, 250.0), "the tall line is vertical, not across its box");
    // The wide box: across its middle, as before.
    for x in [310.0, 450.0, 590.0] {
        assert!(dark(&doc, x, 110.0), "the wide line passes ({x}, 110)");
    }
}

#[test]
fn strokes_take_a_dash_style() {
    let mut doc = BoardDocument::create("Strokes.kboard", 800, 600).unwrap();
    let points: Vec<[f64; 2]> = (0..=40).map(|i| [100.0 + f64::from(i) * 10.0, 300.0]).collect();
    let id = created(
        &doc.execute("stroke.create", json!({ "points": points, "style": { "strokeColor": "#000000", "strokeSize": "l", "dashStyle": "dashed" } }))
            .unwrap(),
    )
    .remove(0);
    assert!(fragment(&doc, &id).contains("stroke-dasharray"), "{}", fragment(&doc, &id));
    // Dashed: along the line, ink and gaps.
    let along: Vec<bool> = (0..300).map(|i| dark(&doc, 150.0 + f64::from(i), 300.0)).collect();
    assert!(along.iter().any(|d| *d) && along.iter().any(|d| !*d), "a dashed stroke has gaps");
    doc.execute("element.update", json!({ "id": id, "set": { "dashStyle": "solid" } })).unwrap();
    assert!(!fragment(&doc, &id).contains("stroke-dasharray"));
    assert!((0..300).all(|i| dark(&doc, 150.0 + f64::from(i), 300.0)), "a solid stroke has none");
}
