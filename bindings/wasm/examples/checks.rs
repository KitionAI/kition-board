//! Writes the conformance fixtures: `basic.kboard`, which `board_check` finds clean, and one board
//! per code of `agent/checks.json` that `board_check` reports on it. `cargo run -p kition-board-kit --example checks -- conformance [fixture names]` (with names,
//! only those are written; the others keep their bytes).
//!
//! Two fixtures are written below the commands, since no command makes their state:
//! - `check-dangling-binding`: a connector end bound to an element that does not exist. Opening
//!   the file detaches it (rule V6); `board_check` reports the end the file had dangling while it
//!   stays detached.
//! - `check-card-target-changed`: a board card whose stored `docId` (KitModel M7, as KitLinks
//!   `target.docId` stores it) is not the one the file at its path names. A conformance run puts
//!   only the fixture in its workspace, so the card's path is the fixture itself: the path holds a
//!   board, but another one than the card was made for.

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::{ObjectId, Origin};
use kition_kit::session::KitSessionDocument;
use serde_json::{Value, json};

/// A `docId` no board has: what the card in `check-card-target-changed` was made for.
const OTHER_BOARD: &str = "0192f0a0-0000-7000-8000-00000000c0de";

/// Writes `value` to `field` of `id` without a command (states no command makes).
fn write_raw(doc: &mut BoardDocument, id: &str, field: &str, value: Value) -> Result<(), String> {
    let id = ObjectId::from_string(id);
    doc.session_mut().record(&Origin::system("fixture"), |tx| Ok(tx.set(&id, field, value)?)).map_err(|error| error.to_string())
}

fn created(value: &Value) -> String {
    value.get("created").and_then(|ids| ids.get(0)).and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn board(name: &str, build: impl FnOnce(&mut BoardDocument) -> Result<(), String>) -> Result<Vec<u8>, String> {
    let mut doc = BoardDocument::create(name, 1920, 1080)?;
    build(&mut doc)?;
    doc.save(None)
}

fn shape(doc: &mut BoardDocument, x: f64, y: f64, extra: Value) -> Result<String, String> {
    let mut element = json!({ "kind": "shape", "position": [x, y], "size": [160, 80] });
    if let (Some(target), Some(extra)) = (element.as_object_mut(), extra.as_object()) {
        target.extend(extra.clone());
    }
    doc.execute("element.create", json!({ "elements": [element] })).map(|answer| created(&answer))
}

fn main() -> Result<(), String> {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).ok_or("usage: checks <conformance folder> [fixture names]")?);
    let only: Vec<String> = std::env::args().skip(2).collect();
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let fixtures: Vec<(&str, Vec<u8>)> = vec![
        // The board the conformance suite edits: a frame with two labelled, connected flow nodes,
        // on which `board_check` reports nothing.
        (
            "basic.kboard",
            board("Plan.kboard", |doc| {
                let frame = doc.execute("frame.create", json!({ "box": [0, 0, 640, 320], "title": "Sprint" }))?;
                let frame = frame.get("frame").and_then(Value::as_str).unwrap_or_default().to_owned();
                let a = shape(doc, 60.0, 120.0, json!({ "parent": frame, "shapeStyle": "flow-node", "text": "Plan" }))?;
                let b = shape(doc, 420.0, 120.0, json!({ "parent": frame, "shapeStyle": "flow-node", "text": "Build" }))?;
                doc.execute("connector.create", json!({ "from": { "element": a, "anchor": [1, 0.5] }, "to": { "element": b, "anchor": [0, 0.5] } }))
                    .map(|_| ())
            })?,
        ),
        (
            "check-card-target-missing.kboard",
            board("Cards.kboard", |doc| doc.execute("card.create", json!({ "file": "plans/Missing.md", "position": [0, 0] })).map(|_| ()))?,
        ),
        (
            "check-image-missing-blob.kboard",
            board("Pictures.kboard", |doc| {
                let payload = json!({ "type": "application/x-kition-board+json", "version": 2, "elements": [{
                    "ref": "picture", "kind": "image", "position": { "x": 0, "y": 0 }, "size": { "w": 200, "h": 120 },
                    "image": { "hash": "0000000000000000000000000000000000000000000000000000000000000000", "mime": "image/png", "size": 67 },
                }] });
                doc.execute("element.paste", json!({ "payload": payload })).map(|_| ())
            })?,
        ),
        (
            "check-label-overflow.kboard",
            board("Overflow.kboard", |doc| {
                shape(doc, 0.0, 0.0, json!({ "size": [80, 40], "text": "A label far too long for its small box, on many lines" })).map(|_| ())
            })?,
        ),
        (
            "check-overlapping-label.kboard",
            board("Labels.kboard", |doc| {
                doc.execute(
                    "element.create",
                    json!({ "elements": [
                    { "kind": "text", "position": [0, 0], "text": "Notes" },
                    { "kind": "text", "position": [10, 6], "text": "More notes" },
                ] }),
                )
                .map(|_| ())
            })?,
        ),
        (
            "check-overlap.kboard",
            board("Overlap.kboard", |doc| {
                let a = shape(doc, 0.0, 0.0, json!({ "shapeStyle": "flow-node", "text": "Plan" }))?;
                let b = shape(doc, 80.0, 20.0, json!({ "shapeStyle": "flow-node", "text": "Build" }))?;
                doc.execute("connector.create", json!({ "from": { "element": a }, "to": { "element": b } })).map(|_| ())
            })?,
        ),
        (
            "check-disconnected-node.kboard",
            board("Flow.kboard", |doc| {
                shape(doc, 0.0, 0.0, json!({ "shapeStyle": "flow-node", "text": "Plan" }))?;
                shape(doc, 400.0, 0.0, json!({ "shapeStyle": "flow-node", "text": "Build" })).map(|_| ())
            })?,
        ),
        ("check-missing-label.kboard", board("Mind.kboard", |doc| shape(doc, 0.0, 0.0, json!({ "shapeStyle": "mind-node" })).map(|_| ()))?),
        (
            "check-outside-frame.kboard",
            board("Frame.kboard", |doc| {
                let frame = doc.execute("frame.create", json!({ "box": [0, 0, 300, 200], "title": "Sprint" }))?;
                let frame = frame.get("frame").and_then(Value::as_str).unwrap_or_default().to_owned();
                shape(doc, 250.0, 150.0, json!({ "parent": frame, "text": "Late" })).map(|_| ())
            })?,
        ),
        (
            "check-connector-crosses-node.kboard",
            board("Route.kboard", |doc| {
                let a = shape(doc, 0.0, 0.0, json!({ "text": "From" }))?;
                shape(doc, 300.0, 0.0, json!({ "text": "In the way" }))?;
                let b = shape(doc, 600.0, 0.0, json!({ "text": "To" }))?;
                doc.execute("connector.create", json!({ "from": { "element": a, "anchor": [1, 0.5] }, "to": { "element": b, "anchor": [0, 0.5] } }))
                    .map(|_| ())
            })?,
        ),
        (
            "check-dangling-binding.kboard",
            board("Dangling.kboard", |doc| {
                let a = shape(doc, 0.0, 0.0, json!({ "text": "Plan" }))?;
                let b = shape(doc, 400.0, 0.0, json!({ "text": "Build" }))?;
                let made = doc.execute("connector.create", json!({ "from": { "element": a }, "to": { "element": b } }))?;
                write_raw(doc, &created(&made), "end", json!({ "point": [400, 40], "binding": { "target": "gone-element", "anchor": [0, 0.5] } }))
            })?,
        ),
        (
            "check-card-target-changed.kboard",
            board("Changed.kboard", |doc| {
                let made = doc.execute("card.create", json!({ "file": "plans/Roadmap.md", "position": [0, 0] }))?;
                let card = json!({ "kind": "board", "target": { "path": "check-card-target-changed.kboard", "docId": OTHER_BOARD } });
                write_raw(doc, &created(&made), "card", card)
            })?,
        ),
        (
            "check-empty-frame.kboard",
            board("Empty.kboard", |doc| doc.execute("frame.create", json!({ "box": [0, 0, 300, 200], "title": "Later" })).map(|_| ()))?,
        ),
    ];
    for (name, bytes) in fixtures {
        if !only.is_empty() && !only.iter().any(|wanted| wanted == name) {
            continue;
        }
        std::fs::write(dir.join(name), bytes).map_err(|error| format!("{name}: {error}"))?;
    }
    Ok(())
}
