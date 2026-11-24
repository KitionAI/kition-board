//! Writes the editor's sample board: `cargo run -p kition-board-kit --example sample -- web/public/sample/Sample.kboard`.

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use serde_json::{Value, json};

fn created(value: &Value) -> Vec<String> {
    value["created"].as_array().map(|ids| ids.iter().filter_map(|id| id.as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

fn main() -> Result<(), String> {
    let path = std::env::args().nth(1).ok_or("usage: sample <out.kboard>")?;
    let mut doc = BoardDocument::create("Sample.kboard", 1920, 1080)?;
    let shapes = doc.execute(
        "element.create",
        json!({ "elements": [
            { "kind": "shape", "position": [140, 180], "size": [200, 110], "text": "Idea", "fillColor": "yellow", "shapeStyle": "sticky" },
            { "kind": "shape", "position": [520, 180], "size": [200, 110], "text": "Ship", "shapeType": "pill", "fillColor": "purple" },
        ] }),
    )?;
    let ids = created(&shapes);
    let (Some(idea), Some(ship)) = (ids.first(), ids.get(1)) else { return Err("no shapes".into()) };
    doc.execute("frame.create", json!({ "box": [80, 80, 840, 400], "title": "Plan", "children": [idea, ship] }))?;
    doc.execute("connector.create", json!({ "from": { "element": idea }, "to": { "element": ship }, "label": "build" }))?;
    doc.prepare_save()?;
    std::fs::write(&path, doc.save(None)?).map_err(|error| format!("{path}: {error}"))
}
