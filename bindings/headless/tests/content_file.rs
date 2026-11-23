//! The headless `kition-board` saves on the direct path (feature `board.file.content-only`): the
//! host seals the session (`prepare_save`) before every save, so the file an agent's batch writes
//! holds the board's content and, of its history, only that seal, and the session reads what each
//! later batch changed (`changes_since` on a document opened from such a file).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod support;

use kition_kit::kdm::{Origin, Replica};
use serde_json::json;
use support::*;

/// Written once, overwritten at once: in no file.
const OVERWRITTEN: &str = "Ug5Le1Fo0Dw4";

fn origins(file: &[u8]) -> Vec<Option<Origin>> {
    Replica::snapshot_origins(&kition_board_kboard::read(file).unwrap().content).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agents_batches_save_sealed_content_and_tell_what_they_changed() {
    let f = Fixture::new();
    assert_eq!(origins(&f.file()), vec![Some(Origin::system("content-seal"))], "the fixture's file");
    let first = f.ok("batch", json!({"ops":[{"op":"set","target":f.a,"field":"fillColor","value":OVERWRITTEN}]})).await;
    assert!(first["changes"].is_object(), "the receipt of a batch on a board opened from its file: {first}");
    let second = f.ok("batch", json!({"ops":[{"op":"set","target":f.a,"field":"fillColor","value":"#ffcc00"}]})).await;
    assert_eq!(second["changes"]["changed"][0]["id"], f.a, "{second}");
    let file = f.file();
    assert_eq!(origins(&file), vec![Some(Origin::system("content-seal"))], "of the history, the file holds the seal");
    assert!(!file.windows(OVERWRITTEN.len()).any(|window| window == OVERWRITTEN.as_bytes()), "the overwritten value is in no file");
    let doc = f.doc();
    let shape = doc.model().element(&kition_kit::kdm::ObjectId::from_string(f.a.as_str().unwrap())).unwrap();
    assert_eq!(shape.registers.get("fillColor"), Some(&json!("#ffcc00")));
    f.shutdown().await;
}
