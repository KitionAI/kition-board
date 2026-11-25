//! `kition-board` opens a board from its replica (`model.open`, which `kit.json` lists) with the
//! pictures it shows: what `model.export` answers for an open board, its changes and the blobs
//! they reference, opens again as the same board, with no file. An agent's headless draft is such
//! a replica (KHP A12); the kit's `model.export` sent no blobs and `model.import` took none, so a
//! draft of a board with a picture lost the picture's bytes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_board_kit::BoardDocument;
use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit::KitDocument;
use kition_kit_server::protocol::Origin;
use kition_kit_server::{KitBackend, OpenRequest};
use serde_json::{Map, Value, json};

/// A 1x1 PNG.
const PICTURE: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// The board as an agent reads it, without what belongs to the editing session.
fn state(kit: &mut Headless<BoardDocument>, id: &str) -> Value {
    let described = kit.describe(id).unwrap();
    json!({ "elements": described.state["elements"], "pages": described.state["pages"].as_array().map(|pages| pages.iter().map(|page| page["elements"].clone()).collect::<Vec<_>>()), "missingBlobs": described.state["missingBlobs"] })
}

#[test]
fn issue_draft_replica_blobs_a_board_opens_from_its_exported_replica_with_its_pictures() {
    let root = std::env::temp_dir().join(format!("kition-board-replica-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let mut board = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    board.execute("image.insert", json!({ "data": PICTURE, "mime": "image/png", "position": [0, 0] })).unwrap();
    std::fs::write(root.join("Plan.kboard"), board.save(None).unwrap()).unwrap();

    let mut kit = Headless::<BoardDocument>::with_replica_port(
        KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_board_kit::EDITOR_STATE).unwrap(),
    );
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let file = kit.open(OpenRequest { file: &folder.file("Plan.kboard").unwrap(), read_only: false, create: None }).unwrap().id;
    kit.execute(&file, "element.create", params(json!({ "elements": [{ "kind": "shape", "position": [300, 0], "text": "Idea" }] })), Origin::Agent)
        .unwrap();
    let before = state(&mut kit, &file);
    assert_eq!(before["missingBlobs"], json!([]), "{before}");

    let exported = kit.model("model.export", json!({ "documentId": file })).unwrap();
    assert!(exported["blobs"].is_string(), "the picture travels with the changes: {exported}");
    let opened = kit
        .model("model.open", json!({ "name": "Plan.kboard", "snapshot": exported["changes"], "blobs": exported["blobs"] }))
        .unwrap_or_else(|error| panic!("model.open: {}", error.message));
    let replica = opened["documentId"].as_str().unwrap().to_owned();
    assert_ne!(replica, file);
    assert_eq!(state(&mut kit, &replica), before, "the replica is the board, with its picture's bytes");

    // A replica that took only the changes takes the blobs through `model.import` too.
    let bare = kit
        .model("model.open", json!({ "name": "Plan.kboard", "snapshot": exported["changes"] }))
        .unwrap_or_else(|error| panic!("model.open without blobs: {}", error.message));
    let bare = bare["documentId"].as_str().unwrap().to_owned();
    assert_ne!(state(&mut kit, &bare)["missingBlobs"], json!([]), "without its blobs the picture is missing");
    let import = kit.model("model.import", json!({ "documentId": bare, "changes": exported["changes"], "blobs": exported["blobs"] }));
    import.unwrap_or_else(|error| panic!("model.import with blobs: {}", error.message));
    assert_eq!(state(&mut kit, &bare)["missingBlobs"], json!([]), "model.import took the picture's bytes");

    // The replica takes edits of its own and leaves the file's board alone.
    kit.execute(&replica, "element.create", params(json!({ "elements": [{ "kind": "shape", "position": [600, 0] }] })), Origin::Agent).unwrap();
    assert_eq!(state(&mut kit, &file), before);
    assert_ne!(state(&mut kit, &replica), before);
    let saved = kit.model("model.save", json!({ "documentId": replica })).unwrap();
    assert!(saved["data"].as_str().is_some_and(|data| !data.is_empty()), "{saved}");
    let _ = std::fs::remove_dir_all(root);
}
