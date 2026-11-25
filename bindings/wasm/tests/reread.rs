//! Session continuity on reread (kition-sdk call-context.md, "Session Continuity On Reread"): a
//! clean board whose file another writer saved takes the file's difference into its live replica,
//! keeping both undo histories, the page the editor shows and its blobs; a refused reread changes
//! nothing. A file another program wrote with a broken board is taken in as the open repaired it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::collections::BTreeMap;

use kition_board_kboard as kboard;
use kition_board_kdm::{ELEMENT, End, file, schema};
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::{ActorId, ObjectId, Origin, Replica};
use kition_kit::session::{__snapshot, KitSessionDocument};
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
        .execute("element.add", json!({"elements":[{"kind":"shape","position":[0,0],"text":"A"},{"kind":"shape","position":[300,0],"text":"B"}]}))
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
    let page = live.execute("page.add", json!({"name":"Ideas","activate":true})).unwrap()["page"].clone();
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
    live.execute("element.add", json!({"elements":[{"kind":"shape","position":[0,0],"text":"A"}]})).unwrap();
    store(&mut live);
    let other = BoardDocument::create(FILE, 1920, 1080).unwrap();
    let other = BoardDocument::open(FILE, &other.save(Some(FILE)).unwrap()).unwrap();
    let before = live.save(Some(FILE)).unwrap();
    assert!(!live.reconcile_reread(&other).unwrap());
    assert_eq!(live.save(Some(FILE)).unwrap(), before);
}

/// A file is content anyone may write (kition-sdk data model 7.3): another program saved the
/// board with a deleted shape still listed (rule V1), a connector anchored outside its target
/// (V7) and one key on two shapes (V8). The file read again opens repaired, and the reread takes
/// that state in: the live board holds nothing to repair.
#[test]
fn a_broken_file_written_outside_is_taken_in_repaired() {
    let mut live = BoardDocument::create(FILE, 1920, 1080).unwrap();
    let made = live
        .execute(
            "element.add",
            json!({"elements":[{"kind":"shape","position":[0,0],"size":[100,100]},{"kind":"shape","position":[300,0],"size":[100,100]},{"kind":"shape","position":[600,0],"size":[100,100]}]}),
        )
        .unwrap();
    let id = |at: usize| ObjectId::from_string(made["created"][at].as_str().unwrap());
    let (one, two, gone) = (id(0), id(1), id(2));
    let page = live.model().pages()[0].id.clone();
    let bytes = store(&mut live);

    // The other program: the file's content changed without the board's repair.
    let mut replica = Replica::load(schema(), &kboard::read(&bytes).unwrap().content, ActorId::random()).unwrap();
    let connector = replica
        .transact(&Origin::person("person:outside"), |tx| {
            tx.delete(&gone)?;
            let fields =
                json!({"kind":"connector","start":{"point":[0,0],"binding":{"target":one.as_str(),"anchor":[1.5,-0.25]}},"end":{"point":[500,500]}});
            let connector = tx.create(ELEMENT, fields.as_object().cloned().unwrap())?;
            let at = tx.list_items(&page, "elements")?.len();
            tx.list_insert(&page, "elements", at, &connector)?;
            tx.set(&one, "key", json!("start"))?;
            tx.set(&two, "key", json!("start"))?;
            Ok(connector)
        })
        .unwrap();
    assert_eq!(
        replica.list(&page, "elements").unwrap(),
        vec![one.clone(), two.clone(), gone.clone(), connector.clone()],
        "the file lists the deleted shape"
    );
    let broken = file::save_file(&replica, BTreeMap::new(), None).unwrap().bytes;
    let fresh = BoardDocument::open(FILE, &broken).unwrap();

    assert!(live.reconcile_reread(&fresh).unwrap());
    let held = live.session().replica();
    assert_eq!(held.list(&page, "elements").unwrap(), vec![one.clone(), two.clone(), connector.clone()], "V1: no entry names the deleted shape");
    let start = End::parse(&held.get(&connector, "start").unwrap()).unwrap();
    assert_eq!(start.binding.map(|binding| binding.anchor), Some([1.0, 0.0]), "V7: the anchor is clamped");
    let (first, second) = if one < two { (&one, &two) } else { (&two, &one) };
    assert_eq!(held.get(first, "key").unwrap(), json!("start"), "V8: the key stays on the smaller id");
    assert_eq!(held.get(second, "key").unwrap(), Value::Null);
    let mut again = Replica::load(schema(), &__snapshot(&live).unwrap(), ActorId::random()).unwrap();
    assert!(!file::repair_once(&mut again).unwrap(), "the live board holds nothing to repair");
}
