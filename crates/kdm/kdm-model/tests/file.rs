//! The `.kboard` file as a replica (design note 2.3, 2.6): round trip (M15), appending one change
//! entry per save with the snapshot bytes unchanged, compaction, unknown entries, the repair on
//! open, and refusals.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod common;

use std::collections::BTreeMap;

use common::*;
use kition_board_kboard as kboard;
use kition_board_kdm::file::{open_file, save_file};
use kition_kit::kdm::ActorId;
use serde_json::json;

fn save(session: &Session, base: Option<&kition_board_kdm::file::FileBase>) -> (Vec<u8>, kition_board_kdm::file::FileBase) {
    save_file(session.replica(), session.model().title(), BTreeMap::new(), base, "0.0.0").unwrap()
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
    let (bytes, _) = save(&a, None);
    assert!(kboard::is_container(&bytes));
    let back = reopen(&bytes);
    assert_eq!(state(&back), state(&a), "open, save, open gives the same state (M15)");
    assert_eq!(back.replica().doc_id(), a.replica().doc_id());
    let parts = kboard::read(&bytes).unwrap();
    assert_eq!(parts.meta.title, "Plan");
    assert_eq!(parts.meta.doc_id, a.replica().doc_id().to_string());
    assert_eq!(parts.meta.schema_version, 2);
    assert_eq!(parts.meta.app.kit, "kit.board");
    assert!(parts.changes.is_empty());
}

#[test]
fn a_save_after_one_edit_appends_one_entry_and_keeps_the_snapshot_bytes() {
    let mut a = board();
    let page = first_page(&a);
    write(&mut a, |tx| {
        for x in 0..40 {
            add(tx, &page, shape(f64::from(x) * 30.0, 0.0, 20.0, 20.0));
        }
        Ok(())
    });
    let (first, _) = save(&a, None);
    let opened = open_file(&first, ActorId::random()).unwrap();
    let mut b = kition_kit::session::KitSession::from_replica(opened.replica, Box::new(kition_board_kdm::blobs::SharedBlobs::default())).unwrap();
    let base = opened.base;
    let (unchanged, _) = save(&b, Some(&base));
    assert_eq!(kboard::read(&unchanged).unwrap().changes.len(), 0, "a save with nothing new appends nothing");
    write(&mut b, |tx| {
        add(tx, &page, shape(5.0, 300.0, 20.0, 20.0));
        Ok(())
    });
    let (second, base) = save(&b, Some(&base));
    let parts = kboard::read(&second).unwrap();
    assert_eq!(parts.changes.len(), 1);
    assert_eq!(parts.snapshot, kboard::read(&first).unwrap().snapshot, "the snapshot is re-emitted byte for byte");
    assert_eq!(parts.meta.changes, vec!["changes/000001.bin"]);
    write(&mut b, |tx| {
        add(tx, &page, shape(50.0, 300.0, 20.0, 20.0));
        Ok(())
    });
    let (third, _) = save(&b, Some(&base));
    let parts = kboard::read(&third).unwrap();
    assert_eq!(parts.changes.len(), 2);
    assert_eq!(parts.changes[0], kboard::read(&second).unwrap().changes[0], "earlier entries are re-emitted byte for byte");
    assert_eq!(state(&reopen(&third)), state(&b));
}

#[test]
fn a_large_change_log_is_compacted_into_a_fresh_snapshot() {
    let a = board();
    let page = first_page(&a);
    let (first, base) = save(&a, None);
    let mut b = reopen(&first);
    write(&mut b, |tx| {
        for x in 0..50 {
            add(tx, &page, shape(f64::from(x) * 30.0, 0.0, 20.0, 20.0));
        }
        Ok(())
    });
    let (second, base) = save(&b, Some(&base));
    let parts = kboard::read(&second).unwrap();
    assert!(parts.changes.is_empty(), "changes over a quarter of the snapshot compact the file");
    assert_ne!(parts.snapshot, kboard::read(&first).unwrap().snapshot);
    assert_eq!(base.change_entries(), 0);
    assert_eq!(state(&reopen(&second)), state(&b));
}

#[test]
fn unknown_entries_survive_a_save() {
    let a = board();
    let (bytes, _) = save(&a, None);
    let mut parts = kboard::read(&bytes).unwrap();
    parts.extra.push(("later/notes.json".into(), b"{\"kept\":true}".to_vec()));
    let with_extra = kboard::write(&parts).unwrap();
    let opened = open_file(&with_extra, ActorId::random()).unwrap();
    let session = kition_kit::session::KitSession::from_replica(opened.replica, Box::new(kition_board_kdm::blobs::SharedBlobs::default())).unwrap();
    let (saved, _) = save(&session, Some(&opened.base));
    assert_eq!(kboard::read(&saved).unwrap().extra, vec![("later/notes.json".to_owned(), b"{\"kept\":true}".to_vec())]);
}

/// Opening repairs once (a merge can leave a file inconsistent) and never writes the file; the
/// repair is appended at the next save.
#[test]
fn opening_repairs_and_the_next_save_appends_the_repair() {
    let mut a = board();
    let page = first_page(&a);
    let connector = write(&mut a, |tx| {
        let target = add(tx, &page, shape(0.0, 0.0, 100.0, 100.0));
        Ok(add(tx, &page, connector_from(&target, [3.0, 0.5], [300.0, 300.0])))
    });
    let (bytes, _) = save(&a, None);
    let opened = open_file(&bytes, ActorId::random()).unwrap();
    let end = kition_board_kdm::End::parse(&opened.replica.get(&connector, "start").unwrap()).unwrap();
    assert_eq!(end.binding.unwrap().anchor, [1.0, 0.5]);
    assert_ne!(opened.replica.version(), *opened.base.version(), "the repair is not in the file yet");
    let session = kition_kit::session::KitSession::from_replica(opened.replica, Box::new(kition_board_kdm::blobs::SharedBlobs::default())).unwrap();
    let (saved, _) = save(&session, Some(&opened.base));
    assert_eq!(kboard::read(&saved).unwrap().changes.len(), 1);
}

#[test]
fn refuses_files_that_are_not_boards_or_are_damaged() {
    let a = board();
    let (bytes, _) = save(&a, None);
    let error = |bytes: &[u8]| open_file(bytes, ActorId::random()).err().map(|error| error.to_string()).unwrap_or_default();
    assert!(error(b"").starts_with("not-a-board"));
    assert!(error(b"{\"format\":\"kition-board\",\"version\":2,\"snapshot\":\"\"}").starts_with("not-a-board"), "the old JSON envelope is not read");
    let mut parts = kboard::read(&bytes).unwrap();
    parts.meta.doc_id = "another-document".into();
    assert!(error(&kboard::write(&parts).unwrap()).starts_with("damaged"), "meta.json names another document");
    let mut parts = kboard::read(&bytes).unwrap();
    parts.snapshot = b"not a snapshot".to_vec();
    assert!(error(&kboard::write(&parts).unwrap()).starts_with("damaged"));
    let mut parts = kboard::read(&bytes).unwrap();
    parts.changes.push(b"not a change".to_vec());
    assert!(error(&kboard::write(&parts).unwrap()).starts_with("damaged"));
}
