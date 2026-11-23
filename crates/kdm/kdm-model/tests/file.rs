//! The `.kboard` file as the document's content (design note 2.3, 2.6; kition-sdk rule M16b):
//! round trip (M15), the same bytes for a save of the same version, unknown entries, the repair on
//! open, a file taken into a replica that holds its history (by its changes, or by its state keyed
//! by object id), and refusals.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod common;

use std::collections::BTreeMap;

use common::*;
use kition_board_kboard as kboard;
use kition_board_kdm::file::{FileBase, SavedFile, open_file, save_file};
use kition_kit::kdm::{ActorId, Replica};
use kition_kit::session::take_file;
use serde_json::json;

fn save(session: &Session, base: Option<&FileBase>) -> SavedFile {
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
    let bytes = save(&a, None).bytes;
    assert!(kboard::is_container(&bytes));
    let back = reopen(&bytes);
    assert_eq!(state(&back), state(&a), "open, save, open gives the same state (M15)");
    assert_eq!(back.replica().doc_id(), a.replica().doc_id());
    let parts = kboard::read(&bytes).unwrap();
    assert_eq!(parts.meta.doc_id, a.replica().doc_id().to_string());
    assert_eq!(parts.meta.schema_version, 1);
    assert_eq!(parts.meta.kit, "kit.board");
    let content = Replica::load(kition_board_kdm::schema(), &parts.content, ActorId::random()).unwrap();
    assert!(content.is_sealed(), "the content is sealed");
    assert!(content.is_shallow(), "and holds no history before its seal");
}

#[test]
fn a_save_of_the_version_the_file_holds_writes_the_same_bytes() {
    let mut a = board();
    let page = first_page(&a);
    write(&mut a, |tx| {
        add(tx, &page, shape(0.0, 0.0, 20.0, 20.0));
        Ok(())
    });
    let first = save(&a, None);
    assert!(first.seal.is_some(), "an unsealed replica saves a sealed copy, and answers the seal");
    assert_eq!(save(&a, None).bytes, first.bytes, "saving the same version twice gives the same bytes");
    // Opened again, and saved with nothing new: the same bytes.
    let opened = open_file(&first.bytes, ActorId::random()).unwrap();
    assert_eq!(opened.replica.version(), *opened.base.version(), "the file needed no repair");
    let b = kition_kit::session::KitSession::from_replica(opened.replica, Box::new(kition_board_kdm::blobs::SharedBlobs::default())).unwrap();
    let again = save(&b, Some(&opened.base));
    assert_eq!(again.bytes, first.bytes);
    assert!(again.seal.is_none());
    // The seal taken into the replica that saved: it is at the file's version and sealed, and
    // saves on the direct path, sealing nothing new.
    a.import_changes(first.seal.as_ref().unwrap()).unwrap();
    assert_eq!(a.version(), *first.base.version());
    assert!(a.replica().is_sealed());
    let direct = save(&a, Some(&first.base));
    assert!(direct.seal.is_none());
    assert_eq!(direct.base.version(), first.base.version());
    // A sealed replica saves the same bytes twice.
    assert_eq!(save(&a, Some(&first.base)).bytes, direct.bytes);
}

#[test]
fn unknown_entries_survive_a_save() {
    let a = board();
    let bytes = save(&a, None).bytes;
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
    let saved = save(&session, Some(&opened.base)).bytes;
    assert_eq!(kboard::read(&saved).unwrap().extra, vec![("later/notes.json".to_owned(), b"{\"kept\":true}".to_vec())]);
}

/// Opening repairs once (a merge can leave a file inconsistent) and never writes the file; the
/// repair is in the next save.
#[test]
fn opening_repairs_and_the_next_save_holds_the_repair() {
    let mut a = board();
    let page = first_page(&a);
    let connector = write(&mut a, |tx| {
        let target = add(tx, &page, shape(0.0, 0.0, 100.0, 100.0));
        Ok(add(tx, &page, connector_from(&target, [3.0, 0.5], [300.0, 300.0])))
    });
    let bytes = save(&a, None).bytes;
    let opened = open_file(&bytes, ActorId::random()).unwrap();
    let end = kition_board_kdm::End::parse(&opened.replica.get(&connector, "start").unwrap()).unwrap();
    assert_eq!(end.binding.unwrap().anchor, [1.0, 0.5]);
    assert_ne!(opened.replica.version(), *opened.base.version(), "the repair is not in the file yet");
    let session = kition_kit::session::KitSession::from_replica(opened.replica, Box::new(kition_board_kdm::blobs::SharedBlobs::default())).unwrap();
    let saved = save(&session, Some(&opened.base)).bytes;
    assert_ne!(saved, bytes);
    let end = kition_board_kdm::End::parse(&reopen(&saved).replica().get(&connector, "start").unwrap()).unwrap();
    assert_eq!(end.binding.unwrap().anchor, [1.0, 0.5], "the file holds the repaired anchor");
}

/// A file saved from the stored replica's own history: the store has its version (nothing to
/// take), or lacks only the seal (taken as a change).
#[test]
fn a_file_saved_from_the_stored_history_is_taken_as_its_changes() {
    let mut a = board();
    let page = first_page(&a);
    write(&mut a, |tx| {
        add(tx, &page, shape(0.0, 0.0, 20.0, 20.0));
        Ok(())
    });
    let saved = save(&a, None);
    let file = open_file(&saved.bytes, ActorId::random()).unwrap().replica;
    // The store saved before the seal reached it.
    let mut stored = Replica::load(kition_board_kdm::schema(), &a.snapshot().unwrap(), ActorId::random()).unwrap();
    assert!(take_file(&mut stored, &file).unwrap());
    assert_eq!(stored.version(), file.version(), "the seal came as a change");
    assert!(!stored.is_shallow(), "the store keeps its whole history");
    assert!(!take_file(&mut stored, &file).unwrap(), "a store that has the file's version takes nothing");
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
    let saved = reopen(&save(&a, None).bytes);
    let before = stored.version();
    assert!(take_file(&mut stored, saved.replica()).unwrap());
    assert!(!stored.is_shallow(), "the store keeps its whole history");
    assert!(stored.version().includes(&before));
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
    let a = board();
    let bytes = save(&a, None).bytes;
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
