//! A command with a parameter schema refuses a key the schema does not name, and says which
//! (platform rule E7a, kit requirement AX15): an agent that misspells a parameter is told instead
//! of the command running without it.
#![allow(clippy::unwrap_used, clippy::panic)]

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;

fn doc() -> BoardDocument {
    BoardDocument::create("Plan.kboard", 1920, 1080).unwrap()
}

#[test]
fn every_command_refuses_an_unknown_parameter() {
    if let Err(problems) = kition_kit::testing::check_unknown_params(doc, &[]) {
        panic!("{} problems:\n{}", problems.len(), problems.join("\n"));
    }
}

/// With `KITION_COMMANDS_DUMP` set to a file, writes the catalogue there: what callers are checked
/// against when auditing the keys commands take.
#[test]
fn dump_commands() {
    if let Ok(path) = std::env::var("KITION_COMMANDS_DUMP") {
        std::fs::write(path, serde_json::to_string(&doc().commands()).unwrap()).unwrap();
    }
}

#[test]
fn keys_commands_read_are_in_their_schemas() {
    let mut board = doc();
    // page.create {activate} and connector.create {style}: read by the commands, so declared.
    board.execute("page.create", serde_json::json!({ "name": "Ideas", "activate": true })).unwrap();
    let shapes = board
        .execute(
            "element.create",
            serde_json::json!({ "elements": [
                { "kind": "shape", "shapeType": "rectangle", "position": [0, 0], "size": [100, 60] },
                { "kind": "shape", "shapeType": "rectangle", "position": [300, 0], "size": [100, 60] },
            ] }),
        )
        .unwrap();
    let ids = shapes["created"].as_array().unwrap();
    board
        .execute(
            "connector.create",
            serde_json::json!({ "from": { "element": ids[0] }, "to": { "element": ids[1] }, "style": { "strokeColor": "#5645d4" } }),
        )
        .unwrap();
}
