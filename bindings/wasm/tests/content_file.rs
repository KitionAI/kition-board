//! A `.kboard` file holds the document's content, not its history (kition-sdk data model 7.3, rule
//! M16b; feature `board.file.content-only`): the replica entry, read through the engine, holds one
//! change (the seal) and none of the deleted or overwritten content; an unchanged board saves to
//! the same bytes; a file opened without a store is the same document and edits; a replica from
//! the file and one with the whole history converge; undo after a save takes back the last edit;
//! and a file opens on top of a stored replica, by its changes or by its state. Hosts seal before
//! saving (`prepare_save`); a save nobody sealed for is refused.
//!
//! The needles below are written once each and share no run of four characters with anything else
//! in the board, so the engine's compression keeps each one literal in the bytes that hold it:
//! every test checks the session's full snapshot holds them (the control) before it checks the
//! file does not (the method of kition-sdk's `crates/kition-kdm/tests/content_snapshot.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kboard as kboard;
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::{ObjectId, Origin, OriginKind, Replica};
use kition_kit::session::{__export_changes, __snapshot, KitSessionDocument};
use serde_json::{Value, json};

const FILE: &str = "Plan.kboard";
const TYPED_THEN_DELETED: &str = "Jx4Hb8Ty3Nc6";
const DELETED_ELEMENT: &str = "Qv7Zp9Wm2Rk5";
const OVERWRITTEN: &str = "Ug5Le1Fo0Dw4";
const NEEDLES: [&str; 3] = [TYPED_THEN_DELETED, DELETED_ELEMENT, OVERWRITTEN];

fn contains(bytes: &[u8], needle: &str) -> bool {
    let utf16: Vec<u8> = needle.encode_utf16().flat_map(u16::to_le_bytes).collect();
    bytes.windows(needle.len()).any(|window| window == needle.as_bytes()) || bytes.windows(utf16.len()).any(|window| window == utf16.as_slice())
}

/// The file as a host saves it: sealed first (`prepare_save`), then the bytes.
fn host_save(doc: &mut BoardDocument) -> Vec<u8> {
    doc.prepare_save().unwrap();
    doc.save(Some(FILE)).unwrap()
}

/// The version a file's content holds.
fn file_version(bytes: &[u8]) -> kition_kit::kdm::Version {
    Replica::load(kition_board_kdm::schema(), &kboard::read(bytes).unwrap().content, kition_kit::kdm::ActorId::random()).unwrap().version()
}

fn created(answer: &Value) -> Vec<String> {
    answer["created"].as_array().unwrap().iter().map(|id| id.as_str().unwrap().to_owned()).collect()
}

/// A board whose history holds typed-then-deleted text, a deleted element and an overwritten
/// value; answers it and the id of the shape that stays.
fn board_with_history() -> (BoardDocument, String) {
    let mut doc = BoardDocument::create(FILE, 1920, 1080).unwrap();
    let made = created(
        &doc.execute("element.create", json!({ "elements": [{ "kind": "shape", "position": [0, 0], "text": "Kept" }, { "kind": "shape", "position": [300, 0], "text": DELETED_ELEMENT }] }))
            .unwrap(),
    );
    let (kept, gone) = (made[0].clone(), made[1].clone());
    let id = ObjectId::from_string(&kept);
    doc.session_mut()
        .execute(&Origin::person("person:test"), |_, tx| {
            tx.text_insert(&id, "text", 4, TYPED_THEN_DELETED)?;
            Ok(tx.set(&id, "fillColor", json!(OVERWRITTEN))?)
        })
        .unwrap();
    doc.session_mut()
        .execute(&Origin::person("person:test"), |_, tx| {
            tx.text_delete(&id, "text", 4, TYPED_THEN_DELETED.chars().count())?;
            Ok(tx.set(&id, "fillColor", json!("#ffcc00"))?)
        })
        .unwrap();
    doc.execute("element.delete", json!({ "ids": [gone] })).unwrap();
    (doc, kept)
}

fn element_ids(doc: &BoardDocument) -> Vec<Vec<ObjectId>> {
    doc.model().pages().iter().map(|page| page.elements.clone()).collect()
}

fn state(doc: &BoardDocument) -> Value {
    doc.session().replica().state_json().unwrap()
}

#[test]
fn the_file_holds_the_content_and_of_the_history_only_the_seal() {
    let (mut doc, kept) = board_with_history();
    let full = __snapshot(&doc).unwrap();
    for needle in NEEDLES {
        assert!(contains(&full, needle), "the control: the session's history holds {needle}");
    }
    let bytes = host_save(&mut doc);
    for needle in NEEDLES {
        assert!(!contains(&bytes, needle), "the file holds {needle}");
    }
    let parts = kboard::read(&bytes).unwrap();
    let held = Replica::snapshot_origins(&parts.content).unwrap();
    assert_eq!(held.len(), 1, "of the history, the file holds one change: {held:?}");
    let origin = held[0].clone().unwrap();
    assert_eq!(origin, Origin::system("content-seal"));
    assert_eq!(origin.kind, OriginKind::System);
    assert!(Replica::snapshot_origins(&full).unwrap().len() > 1, "the control: the history holds every change");
    // The direct path: the seal is the session's own, before the host marks the file saved.
    assert_eq!(doc.version(), file_version(&bytes));
    // The state is all there.
    let back = BoardDocument::open(FILE, &bytes).unwrap();
    assert_eq!(state(&back), state(&doc));
    assert_eq!(back.model().element(&ObjectId::from_string(&kept)).unwrap().text, "Kept");
}

#[test]
fn an_unchanged_board_saves_to_the_same_bytes() {
    let (mut doc, _) = board_with_history();
    let first = host_save(&mut doc);
    let version = doc.version();
    assert_eq!(host_save(&mut doc), first, "two saves with nothing between");
    assert_eq!(doc.version(), version, "a sealed board seals nothing more");
    doc.mark_saved();
    assert_eq!(host_save(&mut doc), first, "and after the host stored the first");
    let mut opened = BoardDocument::open(FILE, &first).unwrap();
    assert_eq!(host_save(&mut opened), first, "and from the file opened again");
}

#[test]
fn a_file_opened_without_a_store_is_the_same_document_and_edits() {
    let (mut doc, kept) = board_with_history();
    let bytes = host_save(&mut doc);
    doc.mark_saved();
    let mut back = BoardDocument::open(FILE, &bytes).unwrap();
    assert_eq!(back.doc_id(), doc.doc_id());
    assert_eq!(element_ids(&back), element_ids(&doc), "the same object ids");
    assert_eq!(back.version(), doc.version(), "the same version: the seal is the session's");
    back.execute("element.setText", json!({ "id": kept, "text": "Edited" })).unwrap();
    let made = created(&back.execute("element.create", json!({ "elements": [{ "kind": "shape", "position": [0, 300], "text": "New" }] })).unwrap());
    let saved = host_save(&mut back);
    let again = BoardDocument::open(FILE, &saved).unwrap();
    assert_eq!(again.doc_id(), doc.doc_id());
    assert_eq!(again.model().element(&ObjectId::from_string(&kept)).unwrap().text, "Edited");
    assert!(again.model().element(&ObjectId::from_string(&made[0])).is_some());
}

#[test]
fn a_replica_from_the_file_and_one_with_the_whole_history_converge() {
    let (mut full, kept) = board_with_history();
    let bytes = host_save(&mut full);
    full.mark_saved();
    let mut file = BoardDocument::open(FILE, &bytes).unwrap();
    // Edits on both after the save.
    full.execute("element.move", json!({ "ids": [kept], "delta": [40, 0] })).unwrap();
    full.execute("element.create", json!({ "elements": [{ "kind": "shape", "position": [600, 600], "text": "Full" }] })).unwrap();
    file.execute("element.setText", json!({ "id": kept, "text": "From the file" })).unwrap();
    file.execute("element.create", json!({ "elements": [{ "kind": "shape", "position": [0, 600], "text": "File" }] })).unwrap();
    for _ in 0..4 {
        let (to_file, to_full) =
            (__export_changes(&full, &file.version().encode()).unwrap(), __export_changes(&file, &full.version().encode()).unwrap());
        KitSessionDocument::import_changes(&mut full, &to_full).unwrap();
        KitSessionDocument::import_changes(&mut file, &to_file).unwrap();
        if full.version() == file.version() {
            break;
        }
    }
    assert_eq!(full.version(), file.version());
    assert_eq!(state(&full), state(&file), "both replicas hold the same document");
    let shape = full.model().element(&ObjectId::from_string(&kept)).unwrap();
    assert_eq!((shape.text.as_str(), shape.position.unwrap().x), ("From the file", 40.0), "both edits survive");
    assert_eq!(full.model().element_count(), 3);
}

#[test]
fn undo_after_a_save_takes_back_the_last_edit() {
    let mut doc = BoardDocument::create(FILE, 1920, 1080).unwrap();
    let made = created(&doc.execute("element.create", json!({ "elements": [{ "kind": "shape", "position": [0, 0], "text": "A" }] })).unwrap());
    doc.execute("element.move", json!({ "ids": [made[0]], "delta": [25, 0] })).unwrap();
    host_save(&mut doc);
    doc.mark_saved();
    assert!(doc.inspect().unwrap()["canUndo"].as_bool().unwrap(), "the seal is no undo step and clears none");
    assert_eq!(doc.execute("edit.undo", json!({})).unwrap()["done"], true);
    let id = ObjectId::from_string(&made[0]);
    assert_eq!(doc.model().element(&id).unwrap().position.unwrap().x, 0.0, "the undo took back the move");
    assert_eq!(doc.execute("edit.undo", json!({})).unwrap()["done"], true);
    assert!(doc.model().element(&id).is_none(), "then the creation");
    assert_eq!(doc.execute("edit.redo", json!({})).unwrap()["done"], true);
    assert!(doc.model().element(&id).is_some());
}

#[test]
fn a_file_opens_on_the_stored_replica_that_holds_its_version() {
    let (mut doc, _) = board_with_history();
    let bytes = host_save(&mut doc);
    doc.mark_saved();
    let stored = __snapshot(&doc).unwrap();
    let mut opened = BoardDocument::open_with_replica(FILE, &bytes, &stored, &doc.blobs_bundle()).unwrap();
    assert_eq!(opened.version(), doc.version(), "the stored replica, as it was");
    assert!(!opened.session().replica().is_shallow(), "with its whole history");
    for needle in NEEDLES {
        assert!(contains(&__snapshot(&opened).unwrap(), needle), "the history keeps {needle}");
    }
    assert_eq!(file_version(&host_save(&mut opened)), file_version(&bytes), "and saves the document it opened, sealing nothing new");
}

#[test]
fn a_file_saved_elsewhere_opens_on_the_stored_replica_by_its_state() {
    let (mut doc, kept) = board_with_history();
    let stored = __snapshot(&doc).unwrap();
    let bytes = host_save(&mut doc);
    doc.mark_saved();
    // Elsewhere, from the file alone: an edit, a new shape, a save.
    let mut elsewhere = BoardDocument::open(FILE, &bytes).unwrap();
    elsewhere.execute("element.setText", json!({ "id": kept, "text": "Elsewhere" })).unwrap();
    elsewhere.execute("element.create", json!({ "elements": [{ "kind": "shape", "position": [0, 600], "text": "New" }] })).unwrap();
    let edited = host_save(&mut elsewhere);
    let opened = BoardDocument::open_with_replica(FILE, &edited, &stored, &[]).unwrap();
    assert_eq!(opened.doc_id(), doc.doc_id());
    assert_eq!(state(&opened), state(&elsewhere), "the file's state, by object id");
    assert_eq!(element_ids(&opened), element_ids(&elsewhere));
    assert!(!opened.session().replica().is_shallow(), "on top of the stored history");
    assert!(contains(&__snapshot(&opened).unwrap(), DELETED_ELEMENT), "which it keeps");
}

/// A save writes the content of a sealed session only: one nobody sealed for (`prepare_save`) is
/// refused and changes nothing, and the same session saves once it is sealed.
#[test]
fn a_save_nobody_sealed_for_is_refused() {
    let (mut doc, _) = board_with_history();
    let version = doc.version();
    let error = doc.save(Some(FILE)).unwrap_err();
    assert!(error.contains("not-sealed"), "{error}");
    assert_eq!(doc.version(), version, "a refused save changes nothing");
    let bytes = host_save(&mut doc);
    for needle in NEEDLES {
        assert!(!contains(&bytes, needle), "the file holds {needle}");
    }
}
