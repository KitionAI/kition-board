//! `file.identity` and `file.fork` (Kit Process Protocol section 5.6, KitModel M7a)
//! through the real `kition-board` process: a `.kboard` embeds its replica as the document's
//! content (`content.json`, the document's state with no history, design note 2.1), so a byte copy of
//! a file is the same document. A host reads the identity a file carries without parsing the
//! board, and forks a file into a new document: a new docId (in the replica and in `meta.json`'s
//! copy), the same pages, elements, ids and text, no history, and everything else in the file as
//! it was.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::{ActorId, Replica};
use kition_kit::session::KitSessionDocument;
use kition_kit_host::{ClientError, HeadlessKit, LaunchOptions};
use serde_json::{Value, json};

const LIMIT: Duration = Duration::from_secs(30);

/// Text of an element deleted before the save: in the session's history, in neither file.
const SECRET: &str = "SECRET-DELETED-BEFORE-THE-FORK";

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kition-board-identity-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

async fn launch(dir: &Path) -> HeadlessKit {
    let mut options = LaunchOptions::new(env!("CARGO_BIN_EXE_kition-board"));
    options.workspace = Some(dir.to_path_buf());
    HeadlessKit::launch(options).await.unwrap()
}

/// A board saved twice: first with two shapes and the secret one, then, opened again, with the
/// secret shape deleted, and an entry this build does not know.
fn board() -> Vec<u8> {
    let mut doc = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    let made = doc
        .execute(
            "element.add",
            json!({ "elements": [
                { "kind": "shape", "position": [0, 0], "text": "Kept" },
                { "kind": "shape", "position": [300, 0], "text": "Also kept" },
                { "kind": "shape", "position": [600, 0], "text": SECRET },
            ] }),
        )
        .unwrap();
    let secret = made["created"][2].clone();
    let first = doc.save(None).unwrap();
    let mut doc = BoardDocument::open("Plan.kboard", &first).unwrap();
    let before = doc.session().version();
    doc.execute("element.delete", json!({ "ids": [secret] })).unwrap();
    let saved = doc.save(None).unwrap();
    assert!(holds(&doc.session().export_changes(&before).unwrap(), SECRET), "the control: the session's change log holds the deleted text");
    let mut parts = kition_board_kboard::read(&saved).unwrap();
    parts.extra.push(("notes/kept.txt".into(), b"an entry another program wrote".to_vec()));
    kition_board_kboard::write(&parts).unwrap()
}

/// Every object of the board with its fields, by id: what must be equal in a fork.
async fn objects(kit: &HeadlessKit, path: &str) -> Value {
    let state = kit.open(path, false, LIMIT).await.unwrap();
    let document = state["documentId"].clone();
    let batch = |request: Value| {
        let mut request = request;
        request["documentId"] = document.clone();
        kit.request("model.batch", request, LIMIT)
    };
    let inventory = batch(json!({ "ops": [], "objects": true })).await.unwrap();
    let ids: Vec<Value> = inventory["objects"].as_array().unwrap().iter().map(|object| object["id"].clone()).collect();
    assert!(ids.len() >= 4, "the board, a page and two shapes: {inventory}");
    let load: Vec<Value> = ids.iter().map(|id| json!({ "target": id })).collect();
    let loaded = batch(json!({ "ops": [], "load": load })).await.unwrap();
    kit.request("file.close", json!({ "documentId": document }), LIMIT).await.unwrap();
    json!({ "objects": inventory["objects"], "loaded": loaded["loaded"] })
}

fn holds(bytes: &[u8], needle: &str) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle.as_bytes())
}

/// What the kit said about a refusal (`error.data.detail`).
fn detail(error: &ClientError) -> String {
    error.data().and_then(|data| data["detail"].as_str()).unwrap_or_default().to_owned()
}

#[tokio::test]
async fn a_board_tells_its_identity_and_forks_into_a_new_document_without_history() {
    let dir = workspace("embedded");
    std::fs::write(dir.join("Plan.kboard"), board()).unwrap();
    let kit = launch(&dir).await;
    assert!(kit.serves("file.identity"), "{}", kit.capabilities());

    // The identity the file carries, read without opening it, and the same for a byte copy.
    let source_id = kit.identity("Plan.kboard", LIMIT).await.unwrap().doc_id.expect("a board file has an identity");
    let meta = kition_board_kboard::read(&std::fs::read(dir.join("Plan.kboard")).unwrap()).unwrap().meta;
    assert_eq!(meta.doc_id, source_id, "meta.json's copy names the same document");
    std::fs::copy(dir.join("Plan.kboard"), dir.join("Copy.kboard")).unwrap();
    assert_eq!(kit.identity("Copy.kboard", LIMIT).await.unwrap().doc_id.as_deref(), Some(source_id.as_str()), "a byte copy is the same document");

    // A fork is a new document with the same objects, ids and text.
    let fork = kit.fork("Plan.kboard", "Fork.kboard", LIMIT).await.unwrap();
    let fork_id = fork.doc_id.clone().expect("the fork's identity");
    assert_ne!(fork_id, source_id);
    assert_eq!(fork.name, "Fork.kboard");
    assert!(!fork.version.is_empty());
    assert_eq!(kit.identity("Fork.kboard", LIMIT).await.unwrap().doc_id.as_deref(), Some(fork_id.as_str()), "the forked file carries the new id");
    assert_eq!(kit.identity("Plan.kboard", LIMIT).await.unwrap().doc_id.as_deref(), Some(source_id.as_str()), "the source is unchanged");
    assert_eq!(objects(&kit, "Fork.kboard").await, objects(&kit, "Plan.kboard").await, "the same objects, ids and values");

    // The fork's replica travels as a whole (an agent's draft of it: `model.export`, then
    // `model.open`), though it starts at the fork and has no earlier changes.
    let state = kit.open("Fork.kboard", true, LIMIT).await.unwrap();
    let exported = kit.request("model.export", json!({ "documentId": state["documentId"] }), LIMIT).await.unwrap();
    let draft = kit.request("model.open", json!({ "name": "Fork.kboard", "snapshot": exported["changes"] }), LIMIT).await.unwrap();
    assert_ne!(draft["documentId"], state["documentId"], "{draft}");
    kit.request("file.close", json!({ "documentId": state["documentId"] }), LIMIT).await.unwrap();

    // The fork's file: the new id in meta.json and in its content, everything else kept.
    let (source_bytes, fork_bytes) = (std::fs::read(dir.join("Plan.kboard")).unwrap(), std::fs::read(dir.join("Fork.kboard")).unwrap());
    let (source, forked) = (kition_board_kboard::read(&source_bytes).unwrap(), kition_board_kboard::read(&fork_bytes).unwrap());
    assert_eq!(forked.meta.doc_id, fork_id, "meta.json's copy of the docId is the fork's");
    let content = Replica::load(kition_board_kdm::schema(), &forked.content, ActorId::random()).unwrap();
    assert_eq!(content.doc_id().to_string(), fork_id, "the fork's content is the new document");
    assert_eq!((forked.meta.kit.as_str(), forked.meta.schema_version), (source.meta.kit.as_str(), source.meta.schema_version));
    assert_eq!(forked.blobs, source.blobs);
    assert_eq!(forked.extra, source.extra, "entries the board does not know are kept");

    // The fork edits, saves and opens again as any board, under its own id.
    let mut opened = BoardDocument::open("Fork.kboard", &fork_bytes).unwrap();
    assert_eq!(opened.doc_id(), fork_id);
    opened.execute("element.add", json!({ "elements": [{ "kind": "shape", "position": [0, 300], "text": "New" }] })).unwrap();
    let edited = opened.save(None).unwrap();
    let reopened = BoardDocument::open("Fork.kboard", &edited).unwrap();
    assert_eq!((reopened.doc_id(), reopened.model().element_count()), (fork_id.clone(), opened.model().element_count()));
    assert_eq!(
        reopened.session().replica().state_json().unwrap(),
        opened.session().replica().state_json().unwrap(),
        "reopened: the same objects under the same ids"
    );

    // No history: the deleted shape's text is in neither file.
    assert!(!holds(&source_bytes, SECRET), "the source's file holds no deleted content");
    assert!(!holds(&fork_bytes, SECRET), "deleted content does not travel");

    // A fork of a fork is a third document with the same content.
    let again = kit.fork("Fork.kboard", "Again.kboard", LIMIT).await.unwrap().doc_id.expect("the second fork's identity");
    assert!(again != source_id && again != fork_id, "{again}");
    assert_eq!(objects(&kit, "Again.kboard").await, objects(&kit, "Plan.kboard").await);
    let again_bytes = std::fs::read(dir.join("Again.kboard")).unwrap();
    let mut again_doc = BoardDocument::open("Again.kboard", &again_bytes).unwrap();
    again_doc.execute("element.add", json!({ "elements": [{ "kind": "shape", "position": [300, 300], "text": "Again" }] })).unwrap();
    let again_edited = again_doc.save(None).unwrap();
    let again_reopened = BoardDocument::open("Again.kboard", &again_edited).unwrap();
    assert_eq!(again_reopened.doc_id(), again, "and keeps its id after an edit, a save and a reopen");

    // A fork never replaces a file.
    let taken = kit.fork("Plan.kboard", "Fork.kboard", LIMIT).await.unwrap_err();
    assert_eq!(taken.reason(), Some("file_conflict"), "{taken}");
    assert_eq!(std::fs::read(dir.join("Fork.kboard")).unwrap(), fork_bytes, "the first fork is kept");
    kit.shutdown(Duration::from_secs(5)).await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn an_unsaved_board_has_no_identity_and_bytes_that_are_no_board_are_refused() {
    let dir = workspace("plain");
    let kit = launch(&dir).await;
    // An empty `.kboard` (a file made for the kit before it saved one) holds no replica.
    std::fs::write(dir.join("New.kboard"), b"").unwrap();
    assert_eq!(kit.identity("New.kboard", LIMIT).await.unwrap().doc_id, None);
    let fork = kit.fork("New.kboard", "NewCopy.kboard", LIMIT).await.unwrap();
    assert_eq!(fork.doc_id, None);
    assert_eq!(std::fs::read(dir.join("NewCopy.kboard")).unwrap(), b"", "a file without a replica forks unchanged");

    // Bytes that are not a board are refused, never answered with no identity.
    std::fs::write(dir.join("Text.kboard"), b"words a person wrote elsewhere").unwrap();
    for refused in [kit.identity("Text.kboard", LIMIT).await.unwrap_err(), kit.fork("Text.kboard", "TextCopy.kboard", LIMIT).await.unwrap_err()] {
        assert_eq!(refused.reason(), Some("unsupported_format"), "{refused}: {}", detail(&refused));
    }
    assert!(!dir.join("TextCopy.kboard").exists());
    kit.shutdown(Duration::from_secs(5)).await;
    let _ = std::fs::remove_dir_all(dir);
}
