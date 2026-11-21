//! An undo group whose changes come to nothing (KitModel M12): a `model.batch` of several commands
//! that cancel out (a shape moved and moved back, text typed and deleted again) is no step of its
//! own. One Undo after [a real edit, that group] takes back the real edit, one Redo puts it back,
//! and the board's own bookkeeping (Undo and Redo enabled, the document dirty) follows the replica.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kdm::Point;
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::ObjectId;
use kition_kit::session::model;
use serde_json::{Value, json};

fn board() -> BoardDocument {
    BoardDocument::create("Plan.kboard", 1920, 1080).unwrap()
}

fn create(doc: &mut BoardDocument, element: Value) -> String {
    doc.execute("element.create", json!({ "elements": [element] })).unwrap()["created"][0].as_str().unwrap().to_owned()
}

fn element(doc: &BoardDocument, id: &str) -> kition_board_kdm::Element {
    doc.model().element(&ObjectId::from_string(id)).cloned().unwrap_or_else(|| panic!("no element {id}"))
}

/// Runs `commands` as one `model.batch`: one undo group of one transaction per command.
fn group(doc: &mut BoardDocument, commands: &[(&str, Value)]) {
    let ops: Vec<Value> = commands.iter().map(|(id, params)| json!({ "op": "command", "id": id, "params": params })).collect();
    model::session_batch(doc, &json!({ "ops": ops }), false).unwrap();
}

fn done(answer: Value) -> bool {
    answer["done"].as_bool().unwrap()
}

fn can_undo(doc: &BoardDocument) -> bool {
    !doc.disabled_commands().contains(&"edit.undo".to_owned())
}

fn can_redo(doc: &BoardDocument) -> bool {
    !doc.disabled_commands().contains(&"edit.redo".to_owned())
}

#[test]
fn a_shape_moved_and_moved_back_in_one_group_is_passed_over() {
    let mut doc = board();
    let shape = create(&mut doc, json!({ "kind": "shape", "position": [10.0, 10.0], "size": [40.0, 30.0] }));
    doc.execute("element.move", json!({ "ids": [shape], "delta": [50.0, 0.0] })).unwrap();
    group(
        &mut doc,
        &[("element.move", json!({ "ids": [shape], "delta": [30.0, 0.0] })), ("element.move", json!({ "ids": [shape], "delta": [-30.0, 0.0] }))],
    );
    assert_eq!(element(&doc, &shape).position, Some(Point::new(60.0, 10.0)), "the group came to nothing");
    doc.mark_saved();

    assert!(done(doc.execute("edit.undo", json!({})).unwrap()));
    assert_eq!(element(&doc, &shape).position, Some(Point::new(10.0, 10.0)), "one Undo takes back the real move");
    assert!(doc.dirty(), "the Undo changed the board");
    assert!(can_undo(&doc), "the shape's creation is still a step");
    assert!(can_redo(&doc));

    assert!(done(doc.execute("edit.redo", json!({})).unwrap()));
    assert_eq!(element(&doc, &shape).position, Some(Point::new(60.0, 10.0)), "one Redo puts the move back");
    assert!(!can_redo(&doc));

    assert!(done(doc.execute("edit.undo", json!({})).unwrap()));
    assert!(done(doc.execute("edit.undo", json!({})).unwrap()));
    assert!(doc.model().element(&ObjectId::from_string(&shape)).is_none(), "the second Undo takes back the creation");
    assert!(!can_undo(&doc));
}

#[test]
fn text_typed_and_deleted_in_a_sticky_in_one_group_is_passed_over() {
    let mut doc = board();
    let sticky = create(&mut doc, json!({ "kind": "shape", "shapeStyle": "sticky", "position": [10.0, 10.0], "size": [160.0, 160.0] }));
    doc.execute("element.setText", json!({ "id": sticky, "text": "Plan" })).unwrap();
    group(&mut doc, &[("element.setText", json!({ "id": sticky, "text": "Plans" })), ("element.setText", json!({ "id": sticky, "text": "Plan" }))]);
    assert_eq!(element(&doc, &sticky).text, "Plan");

    assert!(done(doc.execute("edit.undo", json!({})).unwrap()));
    assert_eq!(element(&doc, &sticky).text, "", "one Undo takes back the typing before the group");
    assert!(can_undo(&doc));

    assert!(done(doc.execute("edit.redo", json!({})).unwrap()));
    assert_eq!(element(&doc, &sticky).text, "Plan");
    assert!(!can_redo(&doc));
}

#[test]
fn a_group_that_only_cancels_out_leaves_nothing_to_undo() {
    let mut doc = board();
    let shape = create(&mut doc, json!({ "kind": "shape", "position": [10.0, 10.0], "size": [40.0, 30.0] }));
    group(
        &mut doc,
        &[("element.move", json!({ "ids": [shape], "delta": [30.0, 0.0] })), ("element.move", json!({ "ids": [shape], "delta": [-30.0, 0.0] }))],
    );
    assert!(done(doc.execute("edit.undo", json!({})).unwrap()), "the Undo takes back the creation under the group");
    assert!(doc.model().element(&ObjectId::from_string(&shape)).is_none());
    assert!(!can_undo(&doc), "no step is left");
    assert!(!done(doc.execute("edit.undo", json!({})).unwrap()));
}
