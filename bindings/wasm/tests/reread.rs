//! Session continuity on reread (kition-sdk call-context.md, "Session Continuity On Reread"): a
//! clean board whose file another writer saved takes the file's difference into its live replica,
//! keeping both undo histories, the page the editor shows and its blobs; a refused reread changes
//! nothing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::{ObjectId, Origin};
use kition_kit::session::KitSessionDocument;
use serde_json::{Value, json};

const FILE: &str = "Plan.kboard";

fn agent() -> Origin {
    Origin::agent("agent:test")
}

fn element<'a>(doc: &'a BoardDocument, id: &Value) -> &'a kition_board_kdm::Element {
    doc.model().element(&ObjectId::from_string(id.as_str().unwrap())).unwrap()
}

fn x(doc: &BoardDocument, id: &Value) -> f64 {
    element(doc, id).position.unwrap().x
}

/// The board saved and marked saved, as a host stores it; answers the bytes.
fn store(doc: &mut BoardDocument) -> Vec<u8> {
    let bytes = doc.save(Some(FILE)).unwrap();
    doc.mark_saved();
    bytes
}

/// Another writer opens `bytes`, runs `steps` and saves; answers the file read again.
fn outside(bytes: &[u8], steps: &[(&str, Value)]) -> BoardDocument {
    let mut other = BoardDocument::open(FILE, bytes).unwrap();
    for (command, params) in steps {
        other.execute(command, params.clone()).unwrap();
    }
    BoardDocument::open(FILE, &other.save(Some(FILE)).unwrap()).unwrap()
}

#[test]
fn an_outside_save_keeps_the_agents_undo_and_the_colleagues_edit() {
    let mut live = BoardDocument::create(FILE, 1920, 1080).unwrap();
    let made = live
        .execute("element.create", json!({"elements":[{"kind":"shape","position":[0,0],"text":"A"},{"kind":"shape","position":[300,0],"text":"B"}]}))
        .unwrap();
    let (a, b) = (made["created"][0].clone(), made["created"][1].clone());
    live.execute_as("element.move", json!({"ids":[a],"delta":[10,0]}), &agent()).unwrap();
    let bytes = store(&mut live);
    let fresh = outside(&bytes, &[("element.setText", json!({"id":b,"text":"Colleague"}))]);
    let revision = live.revision();
    assert_eq!(x(&live, &a), 10.0);

    assert!(live.reconcile_reread(&fresh).unwrap());
    live.mark_saved();
    assert!(!live.dirty(), "a reconciled session is what the file holds");
    assert!(live.revision() > revision, "the views see the file's difference");
    assert_eq!(element(&live, &b).text, "Colleague");

    assert_eq!(live.execute_as("edit.undo", json!({}), &agent()).unwrap()["done"], true, "the agent's step survives the reread");
    assert_eq!(x(&live, &a), 0.0);
    assert_eq!(element(&live, &b).text, "Colleague", "undo keeps the colleague's edit");

    // The next save appends to the file read: it opens with both writers' work.
    let saved = BoardDocument::open(FILE, &live.save(Some(FILE)).unwrap()).unwrap();
    assert_eq!(x(&saved, &a), 0.0);
    assert_eq!(element(&saved, &b).text, "Colleague");
}

#[test]
fn a_reread_keeps_the_shown_page_and_refuses_when_the_file_deleted_it() {
    let mut live = BoardDocument::create(FILE, 1920, 1080).unwrap();
    let page = live.execute("page.create", json!({"name":"Ideas","activate":true})).unwrap()["page"].clone();
    let bytes = store(&mut live);

    let renamed = outside(&bytes, &[("page.rename", json!({"page":page,"name":"Plans"}))]);
    let state = live.editor_state();
    assert!(live.reconcile_reread(&renamed).unwrap());
    assert_eq!(live.editor_state(), state, "the editor keeps showing the page");
    let bytes = store(&mut live);

    let deleted = outside(&bytes, &[("page.delete", json!({"page":page}))]);
    let before = live.save(Some(FILE)).unwrap();
    assert_eq!(live.reconcile_reread(&deleted).unwrap_err(), "deleted active page");
    assert_eq!(live.save(Some(FILE)).unwrap(), before, "a refused reread changes nothing");
    assert_eq!(live.editor_state(), state);
}

#[test]
fn a_file_of_another_board_is_not_reconciled() {
    let mut live = BoardDocument::create(FILE, 1920, 1080).unwrap();
    live.execute("element.create", json!({"elements":[{"kind":"shape","position":[0,0],"text":"A"}]})).unwrap();
    store(&mut live);
    let mut other = BoardDocument::create(FILE, 1920, 1080).unwrap();
    let other = BoardDocument::open(FILE, &other.save(Some(FILE)).unwrap()).unwrap();
    let before = live.save(Some(FILE)).unwrap();
    assert!(!live.reconcile_reread(&other).unwrap());
    assert_eq!(live.save(Some(FILE)).unwrap(), before);
}
