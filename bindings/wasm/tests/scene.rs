//! The board scene through the document (task B4): `elementSvg`, `textMetrics`, `blob`, the
//! document frame in `inspect()`, and `render` drawing the same fragments; the 5,000-element
//! board the editor's pan budget uses, timed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::time::Instant;

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use serde_json::{Value, json};

fn created(value: &Value) -> Vec<String> {
    value["created"].as_array().unwrap().iter().map(|id| id.as_str().unwrap().to_owned()).collect()
}

fn svg(doc: &BoardDocument, request: Value) -> Value {
    serde_json::from_str(&doc.element_svg(&request.to_string()).unwrap()).unwrap()
}

#[test]
fn element_svg_answers_fragments_by_revision() {
    let mut doc = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    let ids = created(
        &doc.execute(
            "element.add",
            json!({ "elements": [
        { "kind": "shape", "position": [100, 100], "size": [160, 100], "text": "Idea" },
        { "kind": "text", "position": [400, 100], "text": "Notes" },
    ] }),
        )
        .unwrap(),
    );
    let answer = svg(&doc, json!({ "ids": ids }));
    let fragments = answer["fragments"].as_array().unwrap();
    assert_eq!(fragments.len(), 2);
    assert_eq!(fragments[0]["frame"], "local");
    assert_eq!(fragments[0]["size"], json!([160.0, 100.0]));
    assert!(fragments[0]["svg"].as_str().unwrap().contains(">Idea</tspan>"));
    assert!(fragments[1]["size"][0].as_f64().unwrap() > 0.0, "a text element without a size is measured");
    let known = json!({ ids[0].clone(): fragments[0]["rev"], ids[1].clone(): fragments[1]["rev"] });
    doc.execute("element.move", json!({ "ids": [ids[0]], "delta": [50, 0] })).unwrap();
    let again = svg(&doc, json!({ "ids": ids, "known": known }));
    assert_eq!(again["unchanged"].as_array().unwrap().len(), 2, "moving keeps the fragment: {again}");
    doc.execute("element.setText", json!({ "id": ids[0], "text": "Idea 2" })).unwrap();
    let edited = svg(&doc, json!({ "ids": ids, "known": known }));
    assert_eq!(edited["fragments"][0]["id"], ids[0].as_str());
    assert!(doc.element_svg("not json").unwrap_err().starts_with("invalid-params"));
}

#[test]
fn text_metrics_and_blobs_for_the_editor() {
    let mut doc = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    let metrics: Value = serde_json::from_str(
        &doc.text_metrics(&json!({ "texts": [{ "text": "aaa bbb", "size": 10, "family": "mono", "maxWidth": 40 }] }).to_string()).unwrap(),
    )
    .unwrap();
    let result = &metrics["results"][0];
    assert_eq!(result["lines"].as_array().unwrap().len(), 2);
    assert_eq!(result["height"], 25.0);
    assert!(doc.text_metrics("{}").is_err());

    let png = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEklEQVR4nGP4z8DAwMDAwMAAAA0EAf9n8uLhAAAAAElFTkSuQmCC";
    let inserted = doc.execute("image.insert", json!({ "data": png, "mime": "image/png", "position": [0, 0], "size": [40, 40] })).unwrap();
    let hash = inserted["blob"].as_str().unwrap();
    assert_eq!(&doc.blob(hash)[..4], b"\x89PNG");
    assert!(doc.blob("missing").is_empty());
}

#[test]
fn render_draws_the_scene_in_the_document_frame() {
    let mut doc = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    assert_eq!((doc.width(), doc.height()), (1920, 1080));
    doc.execute("element.add", json!({ "elements": [{ "kind": "shape", "position": [-300, -200], "size": [200, 100], "fillColor": "#ff0000" }] }))
        .unwrap();
    let area = doc.inspect().unwrap()["area"].clone();
    assert_eq!((area["x"].as_f64(), area["y"].as_f64(), area["unit"].as_f64()), (Some(-340.0), Some(-240.0), Some(1.0)));
    // World (-200, -150) is the shape's middle: document pixel (140, 90).
    let rgba = doc.render(140.0, 90.0, 1.0, 1, 1).unwrap();
    assert_eq!(rgba, vec![255, 0, 0, 255]);
    let thumbnail = doc.render(0.0, 0.0, 0.05, 96, 54).unwrap();
    assert_eq!(thumbnail.len(), 96 * 54 * 4);
}

/// The editor's pan budget board (spike S2's mix): 70% labelled flow nodes, 20% connectors bound
/// to a node, 10% strokes, on a 100-column grid, as one clipboard payload.
fn big_payload() -> Value {
    let mut elements = Vec::new();
    let mut last: Option<String> = None;
    for i in 0..5000u32 {
        let (x, y) = (f64::from(i % 100) * 220.0, f64::from(i / 100) * 140.0);
        match (i % 10, &last) {
            (7 | 8, Some(target)) => elements.push(json!({ "ref": format!("e{i}"), "kind": "connector", "start": { "point": [0, 0], "binding": { "target": target, "anchor": [1, 0.5] } }, "end": { "point": [x + 160.0, y + 100.0] } })),
            (7..=9, _) => elements.push(json!({ "ref": format!("e{i}"), "kind": "stroke", "points": (0..12).map(|j| json!([x + 10.0 + f64::from(j) * 13.0, y + 30.0 + f64::from(j % 2) * 40.0])).collect::<Vec<_>>() })),
            _ => {
                let text = if i % 3 == 0 { format!("Review step {i}") } else { format!("Node {i}") };
                elements.push(json!({ "ref": format!("e{i}"), "kind": "shape", "shapeStyle": "flow-node", "position": { "x": x + 30.0, "y": y + 30.0 }, "size": { "w": 160, "h": 80 }, "text": text }));
                last = Some(format!("e{i}"));
            }
        }
    }
    json!({ "type": "application/x-kition-board+json", "version": 1, "elements": elements })
}

/// The pan budget board's fragments and a thumbnail, timed. Prints what they cost; fails only when
/// far off.
#[test]
fn five_thousand_elements_draw_within_budget() {
    let mut doc = BoardDocument::create("Big.kboard", 1920, 1080).unwrap();
    doc.execute("edit.paste", json!({ "payload": big_payload() })).unwrap();
    assert_eq!(doc.inspect().unwrap()["elements"], 5000);
    let all: Vec<String> = doc.model().pages()[0].elements.iter().map(|id| id.as_str().to_owned()).collect();
    let started = Instant::now();
    let mut bytes = 0;
    for batch in all.chunks(500) {
        bytes += doc.element_svg(&json!({ "ids": batch }).to_string()).unwrap().len();
    }
    let fragments = started.elapsed();
    let started = Instant::now();
    let w = doc.width();
    let thumbnail = doc.render(0.0, 0.0, 256.0 / f64::from(w), 256, 82).unwrap();
    let raster = started.elapsed();
    assert_eq!(thumbnail.len(), 256 * 82 * 4);
    eprintln!("5,000 elements: all fragments in {fragments:?} ({} KB); thumbnail raster in {raster:?}", bytes / 1024);
    assert!(fragments.as_secs_f64() < 20.0, "fragments took {fragments:?}");
}

/// A paste of 5,000 elements (the pan budget board) is one quick command: appending each element
/// used to read the whole page list again, which made it quadratic (over two minutes in the page).
#[test]
fn a_five_thousand_element_paste_is_linear() {
    let mut doc = BoardDocument::create("Big.kboard", 1920, 1080).unwrap();
    let started = Instant::now();
    doc.execute("edit.paste", json!({ "payload": big_payload() })).unwrap();
    let pasted = started.elapsed();
    assert_eq!(doc.inspect().unwrap()["elements"], 5000);
    let started = Instant::now();
    let first = doc.model().pages()[0].elements.first().unwrap().as_str().to_owned();
    doc.execute("element.move", json!({ "ids": [first], "delta": [5, 0] })).unwrap();
    let moved = started.elapsed();
    eprintln!("5,000-element paste in {pasted:?}; one move at 5,000 in {moved:?}");
    assert!(pasted.as_secs_f64() < 30.0, "the paste took {pasted:?}");
}

/// Issue #1: `create` gives the board its document size. Board (0, 0) is document pixel (0, 0)
/// and the document is width × height while the content stays inside it (edge to edge included);
/// content past an edge grows the frame on that side only, with the margin; the size survives a
/// save and reopen.
#[test]
fn issue_1_create_sets_the_document_size_at_the_origin() {
    let mut doc = BoardDocument::create("Poster.kboard", 1200, 800).unwrap();
    assert_eq!((doc.width(), doc.height()), (1200, 800), "an empty board is its document size");
    doc.execute(
        "element.add",
        json!({ "elements": [
            { "kind": "shape", "position": [0, 0], "size": [100, 100], "fillColor": "#ff0000" },
            { "kind": "shape", "position": [1100, 700], "size": [100, 100], "fillColor": "#0000ff" },
        ] }),
    )
    .unwrap();
    assert_eq!((doc.width(), doc.height()), (1200, 800), "content edge to edge keeps the size");
    let area = doc.inspect().unwrap()["area"].clone();
    assert_eq!((area["x"].as_f64(), area["y"].as_f64(), area["unit"].as_f64()), (Some(0.0), Some(0.0), Some(1.0)));
    assert_eq!(doc.render(50.0, 50.0, 1.0, 1, 1).unwrap(), vec![255, 0, 0, 255], "board (50, 50) is document pixel (50, 50)");
    assert_eq!(doc.render(1150.0, 750.0, 1.0, 1, 1).unwrap(), vec![0, 0, 255, 255], "the far corner is inside the document");
    let thumbnail = doc.render(0.0, 0.0, 0.5, 600, 400).unwrap();
    assert_eq!(&thumbnail[(375 * 600 + 575) * 4..][..4], &[0, 0, 255, 255], "the whole document fits a half-size thumbnail");

    let bytes = doc.save(None).unwrap();
    let reopened = BoardDocument::open("Poster.kboard", &bytes).unwrap();
    assert_eq!((reopened.width(), reopened.height()), (1200, 800), "the size is saved");
    assert_eq!(reopened.inspect().unwrap()["area"], area);

    // Past the right edge: the frame grows to the right (with the margin), the origin stays.
    doc.execute("element.add", json!({ "elements": [{ "kind": "shape", "position": [1300, 100], "size": [100, 100] }] })).unwrap();
    let area = doc.inspect().unwrap()["area"].clone();
    assert_eq!((area["x"].as_f64(), area["y"].as_f64()), (Some(0.0), Some(0.0)));
    assert_eq!((doc.width(), doc.height()), (1440, 800));
}
