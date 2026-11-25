//! An object-model batch leaves nothing to repair (regressions/issue-batch-left-a-repair): an
//! agent's `board_batch` writes raw values, so it can write a state the repair (rules V1 to V9)
//! would change; the board repairs it at the end of the batch's transaction, inside the batch's
//! own undo step. One Undo gives back the exact state and view before the batch, and Redo the
//! repaired state. Before, the batch left the broken state until the next undo, import or reopen
//! repaired it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kdm::{End, file, schema};
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::{ActorId, ObjectId, Replica};
use kition_kit::session::{__snapshot, KitSessionDocument, model};
use serde_json::{Value, json};

fn state(doc: &BoardDocument) -> Value {
    doc.session().replica().state_json().unwrap()
}

fn view(doc: &BoardDocument) -> Value {
    let mut view = doc.inspect().unwrap();
    let map = view.as_object_mut().unwrap();
    map.remove("canUndo");
    map.remove("canRedo");
    view
}

fn done(answer: Value) -> bool {
    answer["done"].as_bool().unwrap()
}

#[test]
fn a_batch_that_leaves_something_to_repair_is_repaired_in_its_own_undo_step() {
    let mut doc = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    let made = doc
        .execute(
            "element.add",
            json!({ "elements": [{ "kind": "shape", "position": [0, 0], "size": [100, 100] }, { "kind": "shape", "position": [300, 0], "size": [100, 100] }] }),
        )
        .unwrap();
    let (one, two) = (made["created"][0].as_str().unwrap().to_owned(), made["created"][1].as_str().unwrap().to_owned());
    let connector = doc.execute("connector.add", json!({ "from": { "element": one }, "to": { "element": two } })).unwrap()["created"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let (before_state, before_view) = (state(&doc), view(&doc));

    // One key on two shapes (V8) and an anchor outside [0, 1] (V7), written raw by an agent.
    let request = json!({ "origin": { "kind": "agent", "principal": "agent:test" }, "ops": [
        { "op": "set", "target": one, "field": "key", "value": "start" },
        { "op": "set", "target": two, "field": "key", "value": "start" },
        { "op": "set", "target": connector, "field": "start", "value": { "point": [100, 50], "binding": { "target": one, "anchor": [1.5, -0.25] } } },
    ] });
    model::session_batch(&mut doc, &request, false).unwrap();

    let replica = doc.session().replica();
    let (first, second) = if one < two { (&one, &two) } else { (&two, &one) };
    assert_eq!(replica.get(&ObjectId::from_string(first), "key").unwrap(), json!("start"), "V8: the key stays on the smaller id");
    assert_eq!(replica.get(&ObjectId::from_string(second), "key").unwrap(), Value::Null);
    let start = End::parse(&replica.get(&ObjectId::from_string(&connector), "start").unwrap()).unwrap();
    assert_eq!(start.binding.map(|binding| binding.anchor), Some([1.0, 0.0]), "V7: the anchor is clamped");
    let mut again = Replica::load(schema(), &__snapshot(&doc).unwrap(), ActorId::random()).unwrap();
    assert!(!file::repair_once(&mut again).unwrap(), "the batch left nothing to repair");
    let (repaired_state, repaired_view) = (state(&doc), view(&doc));

    assert!(done(doc.execute("edit.undo", json!({ "scope": "agent" })).unwrap()), "the batch and its repair are one step");
    assert_eq!(state(&doc), before_state, "exactly the state before the batch");
    assert_eq!(view(&doc), before_view);
    assert!(done(doc.execute("edit.redo", json!({ "scope": "agent" })).unwrap()));
    assert_eq!(state(&doc), repaired_state, "Redo gives the repaired state back");
    assert_eq!(view(&doc), repaired_view);
}
