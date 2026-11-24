//! Regression (`regressions/issue-2`): bold and italic marks are drawn (shape labels and text
//! elements, in the editor's SVG and the raster alike), and a text element written in Markdown
//! draws its formatting, not its markers. A test binary of its own: added fonts are the
//! process's.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::PathBuf;

use kition_board_kit::{BoardDocument, add_font};
use kition_kit::KitDocument;
use serde_json::{Value, json};

fn inter() {
    let dir = std::env::var_os("KITION_FONTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../kition-fonts")));
    let path = dir.join("fonts/inter/Inter-Variable.ttf");
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error} (needs the kition-fonts checkout)", path.display()));
    add_font(&bytes);
}

fn created(value: &Value) -> Vec<String> {
    value["created"].as_array().unwrap().iter().map(|id| id.as_str().unwrap().to_owned()).collect()
}

fn fragment(doc: &BoardDocument, id: &str) -> String {
    let answer: Value = serde_json::from_str(&doc.element_svg(&json!({ "ids": [id] }).to_string()).unwrap()).unwrap();
    answer["fragments"][0]["svg"].as_str().unwrap().to_owned()
}

/// Dark pixels in a 400 × 100 board area at `(x, y)`, and the pixels themselves.
fn ink(doc: &BoardDocument, x: f64, y: f64) -> (usize, Vec<u8>) {
    let pixels = doc.render(x, y, 1.0, 400, 100).unwrap();
    (pixels.as_chunks::<4>().0.iter().filter(|px| px[0] < 128 && px[1] < 128 && px[2] < 128).count(), pixels)
}

#[test]
fn marks_and_markdown_are_drawn() {
    inter();
    let mut doc = BoardDocument::create("Styles.kboard", 1600, 1000).unwrap();
    let words = "Quarterly targets met";
    let ids = created(
        &doc.execute(
            "element.create",
            json!({ "elements": [
                { "kind": "text", "position": [0, 0], "size": [400, 100], "text": words, "strokeColor": "#000000" },
                { "kind": "text", "position": [0, 200], "size": [400, 100], "text": words, "strokeColor": "#000000" },
                { "kind": "text", "position": [0, 400], "size": [400, 100], "text": words, "strokeColor": "#000000" },
                { "kind": "shape", "position": [600, 0], "size": [400, 100], "text": words, "strokeColor": "#000000" },
                { "kind": "text", "position": [600, 200], "size": [600, 300], "textFormat": "markdown", "strokeColor": "#000000",
                  "text": "# Launch plan\nShip **on time** and *on budget*.\n- Tests pass\n- Docs `ready`\n> Keep [the list](https://example.com) short" },
            ] }),
        )
        .unwrap(),
    );
    let all = words.chars().count();
    doc.execute("text.mark", json!({ "id": ids[1], "range": [0, all], "mark": "bold" })).unwrap();
    doc.execute("text.mark", json!({ "id": ids[2], "range": [0, all], "mark": "italic" })).unwrap();
    doc.execute("text.mark", json!({ "id": ids[3], "range": [0, 9], "mark": "bold" })).unwrap();

    // Bold draws heavier, in the SVG (the weight axis, as the browser sets it) and the raster.
    let bold = fragment(&doc, &ids[1]);
    assert!(bold.contains("font-weight=\"700\"") && bold.contains("'wght' 700"), "{bold}");
    let (plain_ink, plain_pixels) = ink(&doc, 0.0, 0.0);
    let (bold_ink, _) = ink(&doc, 0.0, 200.0);
    assert!(plain_ink > 100, "the plain text draws ({plain_ink})");
    assert!(bold_ink as f64 > plain_ink as f64 * 1.15, "bold has more ink: {bold_ink} against {plain_ink}");
    // Italic slants: the same letters, other pixels.
    let italic = fragment(&doc, &ids[2]);
    assert!(italic.contains("skewX(") || italic.contains("matrix("), "{italic}");
    let (italic_ink, italic_pixels) = ink(&doc, 0.0, 400.0);
    assert!(italic_ink > 100 && italic_pixels != plain_pixels, "italic draws slanted");
    // A shape label: only the marked word is bold.
    let label = fragment(&doc, &ids[3]);
    assert!(label.contains(">Quarterly<") && label.contains("font-weight=\"700\""), "{label}");

    // Markdown: the formatting without its markers.
    let markdown = fragment(&doc, &ids[4]);
    for marker in ["# Launch", "**", "*on budget*", "`ready`", "- Tests", "> Keep", "](https"] {
        assert!(!markdown.contains(marker), "{marker:?} is not drawn: {markdown}");
    }
    for shown in [">Launch plan<", ">on time<", ">on budget<", "• ", "Tests pass", "ready", "the list"] {
        assert!(markdown.contains(shown), "{shown:?} is drawn: {markdown}");
    }
    assert!(markdown.contains("font-size=\"40\""), "the heading is larger: {markdown}");
    let (markdown_ink, _) = ink(&doc, 600.0, 200.0);
    assert!(markdown_ink > 200, "the Markdown draws ({markdown_ink})");

    // Saved and opened again, it draws the same.
    doc.prepare_save().unwrap();
    let bytes = doc.save(None).unwrap();
    let reopened = BoardDocument::open("Styles.kboard", &bytes).unwrap();
    assert_eq!(fragment(&reopened, &ids[1]), bold);
    assert_eq!(fragment(&reopened, &ids[4]), markdown);
}
