//! `file.export` (design note 5.1, task B5's PNG, SVG and PDF part): the page or one frame, drawn
//! from the scene the editor shows; a reading command that leaves the board as it was.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use base64::Engine as _;
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::session::KitSessionDocument;
use serde_json::{Value, json};

fn board() -> (BoardDocument, String) {
    let mut doc = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    doc.execute(
        "element.create",
        json!({ "elements": [
            { "kind": "shape", "position": [0, 0], "size": [160, 100], "text": "Idea", "fillColor": "purple" },
            { "kind": "shape", "position": [300, 200], "size": [100, 100], "shapeType": "ellipse" },
        ] }),
    )
    .unwrap();
    let frame = doc.execute("frame.create", json!({ "box": [600, 0, 900, 200], "title": "Later" })).unwrap()["frame"].as_str().unwrap().to_owned();
    (doc, frame)
}

fn export(doc: &mut BoardDocument, params: Value) -> (Value, Vec<u8>) {
    let answer = doc.execute("file.export", params.clone()).unwrap_or_else(|error| panic!("{params}: {error}"));
    let bytes = base64::engine::general_purpose::STANDARD.decode(answer["base64"].as_str().unwrap()).unwrap();
    (answer, bytes)
}

#[test]
fn a_page_exports_as_png_svg_and_pdf() {
    let (mut doc, _) = board();
    let revision = doc.revision();
    let undo = doc.session().can_undo();
    // The content (0, 0)–(900, 300) with a 40-unit margin, at twice the size.
    let (png, bytes) = export(&mut doc, json!({ "format": "png" }));
    assert_eq!(png["mime"], json!("image/png"));
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!((png["width"].as_f64(), png["height"].as_f64()), (Some(1960.0), Some(760.0)));
    let (_, one) = export(&mut doc, json!({ "format": "png", "scale": 1 }));
    assert!(one.len() < bytes.len(), "a smaller scale is a smaller picture");
    let (svg, bytes) = export(&mut doc, json!({ "format": "svg" }));
    let text = String::from_utf8(bytes).unwrap();
    assert_eq!(svg["mime"], json!("image/svg+xml"));
    assert!(text.starts_with("<svg") && text.contains(r#"width="980""#), "{}", &text[..text.len().min(200)]);
    assert!(text.to_ascii_lowercase().contains("#ece9fb"), "the purple fill is written as its colour");
    let (pdf, bytes) = export(&mut doc, json!({ "format": "pdf" }));
    assert_eq!(pdf["mime"], json!("application/pdf"));
    assert!(bytes.starts_with(b"%PDF-"));
    assert_eq!(doc.revision(), revision, "exporting is no edit");
    assert_eq!(doc.session().can_undo(), undo);
}

#[test]
fn a_frame_exports_alone_and_bad_requests_are_refused() {
    let (mut doc, frame) = board();
    let (answer, bytes) = export(&mut doc, json!({ "format": "png", "frame": frame, "scale": 1, "background": false }));
    assert_eq!((answer["width"].as_f64(), answer["height"].as_f64()), (Some(300.0), Some(200.0)), "the frame's box");
    assert!(bytes.starts_with(b"\x89PNG"));
    let (_, svg) = export(&mut doc, json!({ "format": "svg", "frame": frame }));
    assert!(!String::from_utf8(svg).unwrap().to_ascii_lowercase().contains("#ece9fb"), "what is outside the frame is left out");
    // Too large at its scale, a PNG is refused, or drawn smaller with `fit`.
    let big = doc.execute("element.create", json!({ "elements": [{ "kind": "shape", "position": [0, 0], "size": [40000, 100] }] })).unwrap();
    assert!(big["created"].is_array());
    assert!(doc.execute("file.export", json!({ "format": "png", "scale": 1 })).unwrap_err().starts_with("invalid-params"));
    let (fitted, _) = export(&mut doc, json!({ "format": "png", "scale": 1, "fit": true }));
    assert!(fitted["width"].as_f64().unwrap() <= 16384.0, "{fitted}");
    let shape = doc.model().pages()[0].elements[0].as_str().to_owned();
    for (params, code) in [
        (json!({}), "invalid-params"),
        (json!({ "format": "gif" }), "invalid-params"),
        (json!({ "format": "png", "scale": 100 }), "invalid-params"),
        (json!({ "format": "png", "frame": shape }), "invalid-params"),
        (json!({ "format": "png", "frame": "missing" }), "not-found"),
        (json!({ "format": "png", "page": "missing" }), "not-found"),
    ] {
        let error = doc.execute("file.export", params.clone()).unwrap_err();
        assert!(error.starts_with(code), "{params}: {error}");
    }
}

#[test]
fn a_damaged_picture_is_drawn_as_missing_in_a_pdf() {
    let (mut doc, _) = board();
    // A PNG whose data chunk fails its CRC: the raster draws what it can, the PDF writer refuses it.
    let damaged = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAFklEQVR4nGP8z8DwnwEIGBkZGBgAGEoCAWFhc/YAAAAASUVORK5CYII=";
    doc.execute("image.insert", json!({ "data": damaged, "mime": "image/png", "position": [0, 400] })).unwrap();
    let (_, bytes) = export(&mut doc, json!({ "format": "pdf" }));
    assert!(bytes.starts_with(b"%PDF-"));
    let (_, png) = export(&mut doc, json!({ "format": "png", "scale": 1 }));
    assert!(png.starts_with(b"\x89PNG"));
}
