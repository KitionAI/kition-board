//! The headless `kition-board` saves on the direct path (feature `board.file.content-only`): the
//! file an agent's batch writes holds the board's content (`content.json`: the state, no version,
//! no origin, nothing overwritten), and the session reads what each later batch changed
//! (`changes_since` on a document opened from such a file).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod support;

use serde_json::json;
use support::*;

/// Written once, overwritten at once: in no file.
const OVERWRITTEN: &str = "Ug5Le1Fo0Dw4";

/// A file's content entry, unzipped, as text.
fn content(file: &[u8]) -> String {
    String::from_utf8(kition_board_kboard::read(file).unwrap().content).unwrap()
}

/// Whether a file's content holds a version, a lineage or an origin (data model 7.4, C10).
fn holds_history(file: &[u8]) -> bool {
    let content = content(file);
    ["\"version\"", "\"lineage\"", "\"origin\"", "\"seq\""].iter().any(|key| content.contains(key))
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agents_batches_save_content_and_tell_what_they_changed() {
    let f = Fixture::new();
    assert!(!holds_history(&f.file()), "the fixture's file: {}", content(&f.file()));
    let first = f.ok("batch", json!({"ops":[{"op":"set","target":f.a,"field":"fillColor","value":OVERWRITTEN}]})).await;
    assert!(first["changes"].is_object(), "the receipt of a batch on a board opened from its file: {first}");
    let second = f.ok("batch", json!({"ops":[{"op":"set","target":f.a,"field":"fillColor","value":"#ffcc00"}]})).await;
    assert_eq!(second["changes"]["changed"][0]["id"], f.a, "{second}");
    let file = f.file();
    assert!(!holds_history(&file), "the file holds no history: {}", content(&file));
    assert!(content(&file).contains("#ffcc00"), "the control: the content holds the value written last");
    assert!(!content(&file).contains(OVERWRITTEN), "the overwritten value is in no file");
    let doc = f.doc();
    let shape = doc.model().element(&kition_kit::kdm::ObjectId::from_string(f.a.as_str().unwrap())).unwrap();
    assert_eq!(shape.registers.get("fillColor"), Some(&json!("#ffcc00")));
    f.shutdown().await;
}
