//! The `.kboard` file as the document's content (design note 2.3, 2.6; kition-sdk rule M16b):
//! round trip (M15), the same bytes for an unchanged document, unknown entries, the repair on
//! open, a file taken into the replica a host stored (nothing when both hold the same state, else
//! the file's state keyed by object id), and refusals.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod common;

use std::collections::BTreeMap;

use common::*;
use kition_board_kboard as kboard;
use kition_board_kdm::file::{FileBase, SavedFile, open_file, save_file};
use kition_kit::kdm::{ActorId, Replica};
use kition_kit::session::take_file;
use serde_json::json;

/// A save as a host makes it.
fn save(session: &mut Session, base: Option<&FileBase>) -> SavedFile {
    save_file(session.replica(), BTreeMap::new(), base).unwrap()
}

fn reopen(bytes: &[u8]) -> Session {
    let opened = open_file(bytes, ActorId::random()).unwrap();
    kition_kit::session::KitSession::from_replica(opened.replica, Box::new(kition_board_kdm::blobs::SharedBlobs::default())).unwrap()
}

#[test]
fn a_board_round_trips_through_its_file() {
    let mut a = board();
    let page = first_page(&a);
    write(&mut a, |tx| {
        let target = add(tx, &page, shape(0.0, 0.0, 100.0, 60.0));
        add(tx, &page, connector_from(&target, [1.0, 0.5], [400.0, 30.0]));
        let text = add(tx, &page, json!({ "kind": "text", "position": { "x": 0, "y": 200 } }));
        tx.text_insert(&text, "text", 0, "Bold start")?;
        tx.text_mark(&text, "text", 0, 4, "bold", json!(true))?;
        Ok(())
    });
    let bytes = save(&mut a, None).bytes;
    assert!(kboard::is_container(&bytes));
    let back = reopen(&bytes);
    assert_eq!(state(&back), state(&a), "open, save, open gives the same state (M15)");
    assert_eq!(back.replica().doc_id(), a.replica().doc_id());
    let parts = kboard::read(&bytes).unwrap();
    assert_eq!(parts.meta.doc_id, a.replica().doc_id().to_string());
    assert_eq!(parts.meta.schema_version, 1);
    assert_eq!(parts.meta.kit, "kit.board");
    assert_eq!(parts.content, a.content_snapshot().unwrap(), "the file holds the replica's content");
    // The content holds the state and no version, lineage, actor or origin (data model 7.4, C2, C10).
    let text = std::str::from_utf8(&parts.content).unwrap();
    let header = format!("{{\"kdm\":1,\"kit\":\"kit.board\",\"docId\":\"{}\",\"schemaVersion\":1,\"objects\":{{\n", a.replica().doc_id());
    assert!(text.starts_with(&header), "{text}");
    assert!(!text.contains("\"version\"") && !text.contains("person:test"), "{text}");
    let content = Replica::load(kition_board_kdm::schema(), &parts.content, ActorId::random()).unwrap();
    assert_eq!(content.state_json().unwrap(), a.replica().state_json().unwrap());
}

#[test]
fn an_unchanged_board_saves_to_the_same_bytes_and_keeps_its_ids() {
    let mut a = board();
    let page = first_page(&a);
    write(&mut a, |tx| {
        add(tx, &page, shape(0.0, 0.0, 20.0, 20.0));
        Ok(())
    });
    let version = a.version();
    let first = save(&mut a, None);
    // A save writes nothing into the replica, and a second save writes the same bytes.
    assert_eq!(a.version(), version, "a save changes nothing");
    assert_eq!(a.version(), *first.base.version());
    let second = save(&mut a, Some(&first.base));
    assert_eq!(second.base.version(), first.base.version(), "nor does a second one");
    assert_eq!(second.bytes, first.bytes, "saving the same version twice gives the same bytes");
    // Opened again, and saved with nothing new: the same bytes.
    let opened = open_file(&first.bytes, ActorId::random()).unwrap();
    assert_eq!(opened.replica.version(), *opened.base.version(), "the file needed no repair");
    let mut b = kition_kit::session::KitSession::from_replica(opened.replica, Box::new(kition_board_kdm::blobs::SharedBlobs::default())).unwrap();
    assert_eq!(state(&b), state(&a), "reopened: the same objects under the same ids");
    let again = save(&mut b, Some(&opened.base));
    assert_eq!(again.bytes, first.bytes);
}

#[test]
fn unknown_entries_survive_a_save() {
    let mut a = board();
    let bytes = save(&mut a, None).bytes;
    let mut parts = kboard::read(&bytes).unwrap();
    parts.extra.push(("later/notes.json".into(), b"{\"kept\":true}".to_vec()));
    let with_extra = kboard::write(&parts).unwrap();
    let opened = open_file(&with_extra, ActorId::random()).unwrap();
    let mut session =
        kition_kit::session::KitSession::from_replica(opened.replica, Box::new(kition_board_kdm::blobs::SharedBlobs::default())).unwrap();
    let page = first_page(&session);
    write(&mut session, |tx| {
        add(tx, &page, shape(0.0, 0.0, 20.0, 20.0));
        Ok(())
    });
    let saved = save(&mut session, Some(&opened.base)).bytes;
    assert_eq!(kboard::read(&saved).unwrap().extra, vec![("later/notes.json".to_owned(), b"{\"kept\":true}".to_vec())]);
}

/// Opening repairs once (a file written elsewhere can be inconsistent) and never writes the file; the
/// repair is in the next save.
#[test]
fn opening_repairs_and_the_next_save_holds_the_repair() {
    let mut a = board();
    let page = first_page(&a);
    let connector = write(&mut a, |tx| {
        let target = add(tx, &page, shape(0.0, 0.0, 100.0, 100.0));
        Ok(add(tx, &page, connector_from(&target, [3.0, 0.5], [300.0, 300.0])))
    });
    let bytes = save(&mut a, None).bytes;
    let opened = open_file(&bytes, ActorId::random()).unwrap();
    let end = kition_board_kdm::End::parse(&opened.replica.get(&connector, "start").unwrap()).unwrap();
    assert_eq!(end.binding.unwrap().anchor, [1.0, 0.5]);
    assert_ne!(opened.replica.version(), *opened.base.version(), "the repair is not in the file yet");
    let mut session =
        kition_kit::session::KitSession::from_replica(opened.replica, Box::new(kition_board_kdm::blobs::SharedBlobs::default())).unwrap();
    let saved = save(&mut session, Some(&opened.base)).bytes;
    assert_ne!(saved, bytes);
    let end = kition_board_kdm::End::parse(&reopen(&saved).replica().get(&connector, "start").unwrap()).unwrap();
    assert_eq!(end.binding.unwrap().anchor, [1.0, 0.5], "the file holds the repaired anchor");
}

/// A file of the state the stored replica holds: nothing to take, the store stays as it was.
#[test]
fn a_file_of_the_stored_state_is_taken_as_nothing() {
    let mut a = board();
    let page = first_page(&a);
    write(&mut a, |tx| {
        add(tx, &page, shape(0.0, 0.0, 20.0, 20.0));
        Ok(())
    });
    let mut stored = Replica::load(kition_board_kdm::schema(), &a.snapshot().unwrap(), ActorId::random()).unwrap();
    let before = stored.version();
    let saved = save(&mut a, None);
    let file = open_file(&saved.bytes, ActorId::random()).unwrap().replica;
    assert!(!take_file(&mut stored, &file).unwrap(), "a store that holds the file's state takes nothing");
    assert_eq!(stored.version(), before);
}

/// A file saved elsewhere, from changes the store never saw: its state is written over the
/// store's as one `system` transaction keyed by object id, so every object keeps its id.
#[test]
fn a_file_saved_elsewhere_is_taken_as_its_state_by_object_id() {
    let mut a = board();
    let page = first_page(&a);
    let (kept, gone, text) = write(&mut a, |tx| {
        let kept = add(tx, &page, shape(0.0, 0.0, 20.0, 20.0));
        let gone = add(tx, &page, shape(50.0, 0.0, 20.0, 20.0));
        let text = add(tx, &page, json!({ "kind": "text", "position": { "x": 0, "y": 200 } }));
        tx.text_insert(&text, "text", 0, "Plan")?;
        Ok((kept, gone, text))
    });
    let mut stored = Replica::load(kition_board_kdm::schema(), &a.snapshot().unwrap(), ActorId::random()).unwrap();
    // Elsewhere: a moves, deletes, retypes, adds, and saves.
    let added = write(&mut a, |tx| {
        tx.set(&kept, "position", json!({ "x": 5, "y": 5 }))?;
        tx.delete(&gone)?;
        let at = tx.list_items(&page, "elements")?.iter().position(|id| *id == gone).unwrap();
        tx.list_remove(&page, "elements", at)?;
        tx.text_insert(&text, "text", 4, " B")?;
        tx.text_mark(&text, "text", 0, 4, "bold", json!(true))?;
        Ok(add(tx, &page, shape(90.0, 0.0, 20.0, 20.0)))
    });
    let saved = reopen(&save(&mut a, None).bytes);
    let before = stored.version();
    assert!(take_file(&mut stored, saved.replica()).unwrap());
    assert!(stored.version().includes(&before), "the file's state is a change on the store's line");
    assert!(!stored.can_undo(), "the import is no undo step");
    let back = kition_kit::session::KitSession::<kition_board_kdm::BoardModel>::from_replica(
        stored,
        Box::new(kition_board_kdm::blobs::SharedBlobs::default()),
    )
    .unwrap();
    assert_eq!(back.replica().doc_id(), a.replica().doc_id());
    assert_eq!(state(&back), state(&a), "the file's state, by object id");
    assert!(back.replica().exists(&added), "a new object keeps its id");
    assert!(!back.replica().exists(&gone));
    assert_eq!(back.replica().text(&text, "text").unwrap(), "Plan B");
}

#[test]
fn refuses_files_that_are_not_boards_or_are_damaged() {
    let mut a = board();
    let bytes = save(&mut a, None).bytes;
    let error = |bytes: &[u8]| open_file(bytes, ActorId::random()).err().map(|error| error.to_string()).unwrap_or_default();
    assert!(error(b"").starts_with("not-a-board"));
    assert!(error(b"{\"format\":\"kition-board\",\"version\":2,\"snapshot\":\"\"}").starts_with("not-a-board"), "the old JSON envelope is not read");
    let mut parts = kboard::read(&bytes).unwrap();
    parts.meta.doc_id = "another-document".into();
    assert!(error(&kboard::write(&parts).unwrap()).starts_with("damaged"), "meta.json names another document");
    let mut parts = kboard::read(&bytes).unwrap();
    parts.content = b"not a snapshot".to_vec();
    assert!(error(&kboard::write(&parts).unwrap()).starts_with("damaged"));
}
