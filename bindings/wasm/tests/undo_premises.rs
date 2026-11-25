//! C8-board-engine: what an agent's `edit.undo` and `edit.redo` rest on. The history is the
//! document's, one per scope (KitModel M12a): the agent's undo takes back the agent's last step,
//! whoever wrote since. That step's objects are what the undo writes, so a colleague's later
//! change of one of them should refuse an undo with an older `basis`, and a change of anything
//! else should not.
//!
//! The declaration holds the objects and fields the next undo or redo step of the scope wrote, as
//! the SDK answers them (`Replica::undo_touches_in` / `redo_touches_in`, call-context.md
//! "Isolated Planning And Raw Batches"). In the headless file path an agent's history does not
//! outlive its call (a run saves and the next call reads the file again: `canUndo` is false), so
//! this is the live document's. The test kept the name it had while it waited for that SDK answer.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::Origin;
use kition_kit::session::KitSessionDocument;
use serde_json::{Value, json};

fn held(doc: &BoardDocument, command: &str) -> Vec<Value> {
    doc.under(command, &json!({})).unwrap().map(|under| under.premises.into_iter().map(|premise| premise.id).collect()).unwrap_or_default()
}

#[test]
fn waiting_for_sdk_an_agents_undo_holds_the_objects_of_the_step_it_takes_back() {
    let mut doc = BoardDocument::create("Undo.kboard", 1920, 1080).unwrap();
    let made = doc.execute("element.add", json!({"elements":[{"kind":"shape","position":[0,0]},{"kind":"shape","position":[300,0]}]})).unwrap();
    let (a, b) = (made["created"][0].clone(), made["created"][1].clone());
    let agent = Origin::agent("agent:test");
    doc.execute_as("element.move", json!({"ids":[a],"delta":[10,0]}), &agent).unwrap();
    let undo = held(&doc, "edit.undo");
    assert!(undo.contains(&a) && !undo.contains(&b), "edit.undo holds {undo:?}");
    doc.execute_as("edit.undo", json!({}), &agent).unwrap();
    let redo = held(&doc, "edit.redo");
    assert!(redo.contains(&a) && !redo.contains(&b), "edit.redo holds {redo:?}");
}
