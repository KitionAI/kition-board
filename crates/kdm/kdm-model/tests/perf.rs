//! Open and save at 5,000 elements (design note 2.6): a fresh save, an open, and the save after one
//! edit, each writing the replica's content (`content.json`; the file holds no history, rule M16b).
//! Kition's TypeScript board re-encoded the whole snapshot as base64 on every save: 85.5 ms at
//! 5,000 elements (board evaluation, Node bench). The budgets here are guards for unoptimised test builds on a loaded
//! machine; the measured times are printed (`cargo test --release -p kition-board-kdm --test perf
//! -- --nocapture`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod common;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use common::*;
use kition_board_kdm::file::{open_file, save_file};
use kition_board_kdm::{BOARD, ELEMENT, PAGE, root_id, schema};
use kition_kit::kdm::{ActorId, DocId, Origin, Replica};
use serde_json::json;

const ELEMENTS: usize = 5_000;

fn big_board() -> Replica {
    let (replica, ()) = Replica::create_with(schema(), DocId::new(), ActorId::random(), &Origin::system("import"), |tx| {
        tx.create_with_id(BOARD, &root_id(), fields(json!({ "title": "Big" })))?;
        let page = tx.create(PAGE, fields(json!({ "name": "Page 1" })))?;
        tx.list_insert(&root_id(), "pages", 0, &page)?;
        for index in 0..ELEMENTS {
            let (x, y) = ((index % 100) as f64 * 40.0, (index / 100) as f64 * 40.0);
            let id = tx.create(
                ELEMENT,
                fields(json!({ "kind": "shape", "position": { "x": x, "y": y }, "size": { "w": 30, "h": 30 }, "fillColor": "purple" })),
            )?;
            tx.list_insert(&page, "elements", index, &id)?;
        }
        Ok(())
    })
    .unwrap();
    replica
}

fn timed<R>(label: &str, f: impl FnOnce() -> R) -> (R, Duration) {
    let start = Instant::now();
    let value = f();
    let took = start.elapsed();
    println!("{label}: {:.1} ms", took.as_secs_f64() * 1000.0);
    (value, took)
}

#[test]
fn open_and_save_at_5000_elements() {
    let replica = big_board();
    let (saved, fresh) = timed("fresh save", || save_file(&replica, BTreeMap::new(), None).unwrap());
    let bytes = saved.bytes;
    println!("file: {} bytes", bytes.len());
    let (opened, open) = timed("open", || open_file(&bytes, ActorId::random()).unwrap());
    let base = opened.base;
    let mut session: Session =
        kition_kit::session::KitSession::from_replica(opened.replica, Box::new(kition_board_kdm::blobs::SharedBlobs::default())).unwrap();
    assert_eq!(session.model().element_count(), ELEMENTS);
    let page = first_page(&session);
    write(&mut session, |tx| {
        add(tx, &page, shape(0.0, 4000.0, 30.0, 30.0));
        Ok(())
    });
    let (edited, edit) = timed("save after one edit", || save_file(session.replica(), BTreeMap::new(), Some(&base)).unwrap());
    assert_ne!(edited.bytes, bytes);
    let (again, unchanged) = timed("save with no edit", || save_file(&replica, BTreeMap::new(), Some(&saved.base)).unwrap());
    assert_eq!(again.bytes, bytes, "the unchanged board saves to the same bytes");
    for (label, took) in [("fresh save", fresh), ("open", open), ("save after an edit", edit), ("save with no edit", unchanged)] {
        assert!(took < Duration::from_secs(10), "{label} took {took:?}");
    }
}
