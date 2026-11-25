//! A `.kboard` file holds the document's content, not its history (kition-sdk data model 7.3, 7.4,
//! rule M16b; feature `board.file.content-only`): the content entry holds the state and none of the
//! deleted or overwritten content, and no version or origin; an unchanged board saves to the same
//! bytes; a file opened without a store is the same document and edits; undo after a save takes
//! back the last edit; and a file opens on top of a stored replica, as it is when both hold the
//! same state, else by the file's state written by object id, and goes on on the stored line of
//! changes, whose log (the host's history) keeps what the file does not.
//!
//! The needles below are written once each and share no run of four characters with anything else
//! in the board: every test checks the session's change log holds them (the control) before it
//! checks the file does not, in its bytes and in its content entry unzipped.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kboard as kboard;
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::{ObjectId, Origin, Version};
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

/// The file as a host saves it.
fn host_save(doc: &mut BoardDocument) -> Vec<u8> {
    doc.save(Some(FILE)).unwrap()
}

/// Whether a file holds `needle`, in its bytes or in its content entry unzipped.
fn file_holds(bytes: &[u8], needle: &str) -> bool {
    contains(bytes, needle) || contains(&kboard::read(bytes).unwrap().content, needle)
}

/// The session's changes since it was made: its change log, which holds what it deleted and
/// overwrote (the control for the needles).
fn change_log(doc: &BoardDocument, since: &Version) -> Vec<u8> {
    __export_changes(doc, &since.encode()).unwrap()
}

fn created(answer: &Value) -> Vec<String> {
    answer["created"].as_array().unwrap().iter().map(|id| id.as_str().unwrap().to_owned()).collect()
}

/// A board whose change log holds typed-then-deleted text, a deleted element and an overwritten
/// value; answers it, the id of the shape that stays and the version it was made at.
fn board_with_history() -> (BoardDocument, String, Version) {
    let mut doc = BoardDocument::create(FILE, 1920, 1080).unwrap();
    let made_at = doc.version();
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
    (doc, kept, made_at)
}

fn element_ids(doc: &BoardDocument) -> Vec<Vec<ObjectId>> {
    doc.model().pages().iter().map(|page| page.elements.clone()).collect()
}

fn state(doc: &BoardDocument) -> Value {
    doc.session().replica().state_json().unwrap()
}

#[test]
fn an_unchanged_board_saves_to_the_same_bytes_and_its_content_holds_no_history() {
    let (mut doc, kept, made_at) = board_with_history();
    let log = change_log(&doc, &made_at);
    for needle in NEEDLES {
        assert!(contains(&log, needle), "the control: the session's change log holds {needle}");
    }
    let version = doc.version();
    let first = host_save(&mut doc);
    for needle in NEEDLES {
        assert!(!file_holds(&first, needle), "the file holds {needle}");
    }
    // The content is the state, with no version, lineage, actor or origin (data model 7.4, C10).
    let content = String::from_utf8(kboard::read(&first).unwrap().content).unwrap();
    assert!(
        content.starts_with(&format!("{{\"kdm\":1,\"kit\":\"kit.board\",\"docId\":\"{}\",\"schemaVersion\":1,\"objects\":{{\n", doc.doc_id())),
        "{content}"
    );
    assert!(!content.contains("\"version\"") && !content.contains("person:test") && !content.contains("lineage"), "{content}");
    assert_eq!(host_save(&mut doc), first, "two saves with nothing between");
    assert_eq!(doc.version(), version, "a save writes nothing into the replica");
    doc.mark_saved();
    assert_eq!(host_save(&mut doc), first, "and after the host stored the first");
    let mut opened = BoardDocument::open(FILE, &first).unwrap();
    assert_eq!(state(&opened), state(&doc), "the state is all there, under the same ids");
    assert_eq!(opened.model().element(&ObjectId::from_string(&kept)).unwrap().text, "Kept");
    assert_eq!(host_save(&mut opened), first, "and from the file opened again");
}

#[test]
fn a_file_opened_without_a_store_is_the_same_document_and_edits() {
    let (mut doc, kept, _) = board_with_history();
    let bytes = host_save(&mut doc);
    doc.mark_saved();
    let mut back = BoardDocument::open(FILE, &bytes).unwrap();
    assert_eq!(back.doc_id(), doc.doc_id());
    assert_eq!(element_ids(&back), element_ids(&doc), "the same object ids");
    assert_eq!(state(&back), state(&doc), "the same state");
    back.execute("element.setText", json!({ "id": kept, "text": "Edited" })).unwrap();
    let made = created(&back.execute("element.create", json!({ "elements": [{ "kind": "shape", "position": [0, 300], "text": "New" }] })).unwrap());
    let saved = host_save(&mut back);
    let again = BoardDocument::open(FILE, &saved).unwrap();
    assert_eq!(again.doc_id(), doc.doc_id());
    assert_eq!(again.model().element(&ObjectId::from_string(&kept)).unwrap().text, "Edited");
    assert!(again.model().element(&ObjectId::from_string(&made[0])).is_some());
}

#[test]
fn undo_after_a_save_takes_back_the_last_edit() {
    let mut doc = BoardDocument::create(FILE, 1920, 1080).unwrap();
    let made = created(&doc.execute("element.create", json!({ "elements": [{ "kind": "shape", "position": [0, 0], "text": "A" }] })).unwrap());
    doc.execute("element.move", json!({ "ids": [made[0]], "delta": [25, 0] })).unwrap();
    host_save(&mut doc);
    doc.mark_saved();
    assert!(doc.inspect().unwrap()["canUndo"].as_bool().unwrap(), "a save is no undo step and clears none");
    assert_eq!(doc.execute("edit.undo", json!({})).unwrap()["done"], true);
    let id = ObjectId::from_string(&made[0]);
    assert_eq!(doc.model().element(&id).unwrap().position.unwrap().x, 0.0, "the undo took back the move");
    assert_eq!(doc.execute("edit.undo", json!({})).unwrap()["done"], true);
    assert!(doc.model().element(&id).is_none(), "then the creation");
    assert_eq!(doc.execute("edit.redo", json!({})).unwrap()["done"], true);
    assert!(doc.model().element(&id).is_some());
}

#[test]
fn a_file_opens_on_the_stored_replica_that_holds_its_state() {
    let (mut doc, kept, made_at) = board_with_history();
    let bytes = host_save(&mut doc);
    doc.mark_saved();
    let stored = __snapshot(&doc).unwrap();
    let mut opened = BoardDocument::open_with_replica(FILE, &bytes, &stored, &doc.blobs_bundle()).unwrap();
    assert_eq!(opened.version(), doc.version(), "the stored replica, as it was");
    assert_eq!(state(&opened), state(&doc));
    assert_eq!(host_save(&mut opened), bytes, "and saves the document it opened to the same bytes");
    // The history stays the host's and goes on (what "the history keeps" checked when the file
    // held one): the opened document continues the stored line of changes.
    opened.execute("element.move", json!({ "ids": [kept], "delta": [5, 0] })).unwrap();
    continues_the_stored_history(&mut doc, &opened, &made_at);
}

/// `opened` (a file opened on the replica `store` holds) continues `store`'s line of changes: its
/// changes since `store`'s version are changes on that line, not a snapshot of a new one, and
/// `store` (the host's history) takes them as its next ones, reaching `opened`'s version and
/// state while its change log still holds what the board deleted and overwrote. The file holds
/// none of it.
fn continues_the_stored_history(store: &mut BoardDocument, opened: &BoardDocument, made_at: &Version) {
    let next = __export_changes(opened, &store.version().encode()).unwrap();
    let parsed: Value = serde_json::from_slice(&next).unwrap();
    assert!(parsed["changes"].is_array(), "changes on the stored line, not a snapshot: {parsed}");
    KitSessionDocument::import_changes(store, &next).unwrap();
    assert_eq!(store.version(), opened.version(), "the store follows the opened document");
    assert_eq!(state(store), state(opened));
    let log = change_log(store, made_at);
    for needle in NEEDLES {
        assert!(contains(&log, needle), "the history keeps {needle}");
        assert!(!file_holds(&opened.save(Some(FILE)).unwrap(), needle), "the file holds {needle}");
    }
}

#[test]
fn a_file_saved_elsewhere_opens_on_the_stored_replica_by_its_state() {
    let (mut doc, kept, made_at) = board_with_history();
    let stored = __snapshot(&doc).unwrap();
    let stored_version = doc.version();
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
    assert!(opened.version().includes(&stored_version), "on top of the stored replica, on its line of changes");
    assert_ne!(opened.version(), stored_version, "with the file's state as a change");
    // The stored history (here `doc`, at the stored version) takes that change as its next one.
    continues_the_stored_history(&mut doc, &opened, &made_at);
}
