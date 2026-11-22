//! A picture copied on one board and pasted on another (case `regressions/issue-picture-paste`).
//! The clipboard payload names a picture by its blob's hash, not its bytes, and the editor can
//! paste a payload into another document (the system clipboard's `kition-board:` text). A paste
//! whose picture's bytes are not stored in the destination leaves that picture out, says so in
//! the answer (`skipped`, reason `board.image-missing-blob`), and pastes the rest; a payload of
//! nothing but such pictures is refused. Before, the paste made an image element naming a blob
//! the board does not have: a broken picture that `board_check` reported as
//! `board.image-missing-blob`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use serde_json::{Value, json};

/// A 1 × 1 PNG.
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

fn run(doc: &mut BoardDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("{command} {params}: {error}"))
}

fn codes(doc: &BoardDocument) -> Vec<String> {
    doc.check().into_iter().map(|diagnostic| diagnostic.code).collect()
}

fn kinds(doc: &BoardDocument) -> Vec<String> {
    let model = doc.model();
    model.pages().iter().flat_map(|page| model.elements_of(page)).map(|element| element.kind.as_str().to_owned()).collect()
}

/// Board A with a picture and a shape; answers (A, picture id, shape id, the picture's hash).
fn source() -> (BoardDocument, String, String, String) {
    let mut doc = BoardDocument::create("A.kboard", 1920, 1080).unwrap();
    let inserted = run(&mut doc, "image.insert", json!({ "data": PNG, "mime": "image/png", "position": [0, 0], "size": [40, 40] }));
    let picture = inserted["created"][0].as_str().unwrap().to_owned();
    let hash = inserted["blob"].as_str().unwrap().to_owned();
    let made = run(&mut doc, "element.create", json!({ "elements": [{ "kind": "shape", "position": [100, 0], "size": [80, 60], "text": "Note" }] }));
    (doc, picture, made["created"][0].as_str().unwrap().to_owned(), hash)
}

#[test]
fn a_picture_pasted_on_a_board_without_its_bytes_is_left_out_and_named() {
    let (mut a, picture, shape, hash) = source();
    let payload = run(&mut a, "clipboard.copy", json!({ "ids": [picture, shape] }));
    let mut b = BoardDocument::create("B.kboard", 1920, 1080).unwrap();
    assert!(!b.has_blob(&hash));
    let answer = run(&mut b, "element.paste", json!({ "payload": payload, "at": [300, 300] }));
    assert_eq!(answer["created"].as_array().map(Vec::len), Some(1), "only the shape: {answer}");
    assert_eq!(kinds(&b), vec!["shape".to_owned()], "{answer}");
    assert_eq!(answer["skipped"], json!([{ "ref": picture, "reason": "board.image-missing-blob" }]), "{answer}");
    assert!(!codes(&b).iter().any(|code| code == "board.image-missing-blob"), "{:?}", codes(&b));
    // The shape lands where it would have with the picture: the payload's top-left is the picture's.
    let model = b.model();
    let pasted = model.pages().iter().flat_map(|page| model.elements_of(page)).next().unwrap().clone();
    assert_eq!(pasted.position.map(|p| (p.x, p.y)), Some((400.0, 300.0)));
}

#[test]
fn a_payload_of_pictures_without_their_bytes_is_refused() {
    let (mut a, picture, _, _) = source();
    let payload = run(&mut a, "clipboard.copy", json!({ "ids": [picture] }));
    let mut b = BoardDocument::create("B.kboard", 1920, 1080).unwrap();
    let refused = b.execute("element.paste", json!({ "payload": payload }));
    assert!(refused.as_ref().is_err_and(|error| error.starts_with("not-found: board.image-missing-blob")), "{refused:?}");
    assert!(kinds(&b).is_empty());
}

#[test]
fn a_picture_pasted_where_its_bytes_are_stored_is_pasted() {
    let (mut a, picture, _, hash) = source();
    let payload = run(&mut a, "clipboard.copy", json!({ "ids": [picture] }));
    let answer = run(&mut a, "element.paste", json!({ "payload": payload.clone() }));
    assert_eq!(answer["created"].as_array().map(Vec::len), Some(1), "{answer}");
    assert!(answer.get("skipped").is_none(), "{answer}");
    // On another board that has the same bytes (the same picture inserted there), too.
    let mut b = BoardDocument::create("B.kboard", 1920, 1080).unwrap();
    run(&mut b, "image.insert", json!({ "data": PNG, "mime": "image/png", "position": [500, 500] }));
    assert!(b.has_blob(&hash));
    let answer = run(&mut b, "element.paste", json!({ "payload": payload }));
    assert_eq!(answer["created"].as_array().map(Vec::len), Some(1), "{answer}");
    assert_eq!(kinds(&b), vec!["image".to_owned(), "image".to_owned()]);
    assert!(codes(&b).iter().all(|code| code != "board.image-missing-blob"), "{:?}", codes(&b));
}
