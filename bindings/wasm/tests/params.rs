//! `element.add`'s schema states what the engine reads (KHP A5): a point or size is `[x, y]` or
//! `{ "x", "y" }` (`{ "w", "h" }`), as `params::point` and `params::size` take them, so the SDK's
//! parameter check lets both through and refuses anything else.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::schema::check_params;
use serde_json::{Value, json};

fn create_schema() -> Value {
    let doc = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    doc.commands().into_iter().find(|command| command.id == "element.add").and_then(|command| command.schema).unwrap()
}

#[test]
fn points_and_sizes_are_lists_or_objects() {
    let schema = create_schema();
    let check = |params: Value| check_params(&schema, params.as_object().unwrap());
    for element in [
        json!({ "kind": "shape", "position": [300, 200], "size": [240, 120] }),
        json!({ "kind": "shape", "position": { "x": 300, "y": 200 }, "size": { "w": 240, "h": 120 } }),
    ] {
        check(json!({ "elements": [element.clone()] })).unwrap_or_else(|error| panic!("{element}: {error}"));
        let mut doc = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
        doc.execute("element.add", json!({ "elements": [element] })).unwrap();
    }
    assert!(check(json!({ "elements": [{ "kind": "shape", "position": "300,200" }] })).is_err());
    assert!(check(json!({ "elements": [{ "kind": "shape", "position": { "x": 300 } }] })).is_err());
}
