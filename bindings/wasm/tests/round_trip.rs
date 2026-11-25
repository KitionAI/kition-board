//! Command-level round trips (plan phase P2): each test edits a new board only with commands,
//! checks the edit took, saves the `.kboard`, reopens it and compares the whole replica state; it
//! then saves the reopened board and reopens that too, so a second save keeps everything as well.
//! Nothing is ignored: the replica state holds no clock, cursor or undo history.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::ObjectId;
use kition_kit::session::KitSessionDocument;
use serde_json::{Value, json};

fn board() -> BoardDocument {
    BoardDocument::create("Plan.kboard", 1920, 1080).unwrap()
}

fn run(doc: &mut BoardDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("{command} {params}: {error}"))
}

fn ids(value: &Value) -> Vec<String> {
    value["created"].as_array().unwrap().iter().map(|id| id.as_str().unwrap().to_owned()).collect()
}

fn shape(doc: &mut BoardDocument, x: f64, y: f64, text: &str) -> String {
    ids(&run(doc, "element.add", json!({ "elements": [{ "kind": "shape", "position": [x, y], "size": [120, 60], "text": text }] }))).remove(0)
}

fn element(doc: &BoardDocument, id: &str) -> kition_board_kdm::Element {
    doc.model().element(&ObjectId::from_string(id)).cloned().unwrap_or_else(|| panic!("no element {id}"))
}

fn state(doc: &BoardDocument) -> Value {
    doc.session().replica().state_json().unwrap()
}

/// Saves, reopens and compares; then saves the reopened board and compares once more.
fn round_trip(doc: &mut BoardDocument) -> BoardDocument {
    let before = state(doc);
    let bytes = doc.save(None).unwrap();
    let back = BoardDocument::open("Plan.kboard", &bytes).unwrap();
    assert_eq!(state(&back), before, "the reopened board differs from the saved one");
    let again = back.save(None).unwrap();
    let twice = BoardDocument::open("Plan.kboard", &again).unwrap();
    assert_eq!(state(&twice), before, "a second save of the reopened board differs");
    back
}

#[test]
fn text_marks_survive_save_and_reopen() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, "Ship the Q4 plan 中文 🚀");
    run(&mut doc, "text.mark", json!({ "id": a, "range": [0, 4], "mark": "bold" }));
    run(&mut doc, "text.mark", json!({ "id": a, "range": [9, 11], "mark": "italic" }));
    run(&mut doc, "text.mark", json!({ "id": a, "range": [12, 16], "mark": "link", "value": { "href": "https://kition.ai" } }));
    let edited = element(&doc, &a);
    let shown = format!("{edited:?}");
    assert!(shown.contains("bold") && shown.contains("https://kition.ai"), "{shown}");
    let back = round_trip(&mut doc);
    assert_eq!(element(&back, &a), edited);
}

#[test]
fn frames_and_strokes_survive_save_and_reopen() {
    let mut doc = board();
    let a = shape(&mut doc, 20.0, 20.0, "Inside");
    let stroke =
        ids(&run(&mut doc, "stroke.add", json!({ "points": [[30, 30], [60, 90], [120, 40]], "style": { "strokeColor": "blue" } }))).remove(0);
    let frame =
        run(&mut doc, "frame.add", json!({ "box": [0, 0, 400, 300], "title": "Q4", "children": [a, stroke] }))["frame"].as_str().unwrap().to_owned();
    run(&mut doc, "frame.fit", json!({ "id": frame, "padding": 16 }));
    assert_eq!(element(&doc, &stroke).points.len(), 3);
    let fitted = element(&doc, &frame);
    assert_eq!(fitted.text, "Q4");
    let back = round_trip(&mut doc);
    assert_eq!(element(&back, &frame).size, fitted.size);
    assert_eq!(element(&back, &stroke).points.len(), 3);
}

#[test]
fn connectors_with_labels_and_waypoints_survive_save_and_reopen() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, "Plan");
    let b = shape(&mut doc, 400.0, 0.0, "Ship");
    let c = shape(&mut doc, 400.0, 300.0, "Review");
    let connector = ids(&run(&mut doc, "connector.add", json!({ "from": { "element": a }, "to": { "element": b } }))).remove(0);
    run(&mut doc, "connector.bind", json!({ "id": connector, "terminal": "end", "to": { "element": c } }));
    run(&mut doc, "connector.setLabel", json!({ "id": connector, "text": "then", "position": 0.25 }));
    run(&mut doc, "connector.setWaypoints", json!({ "id": connector, "waypoints": [[200, 0], [200, 300]] }));
    let edited = element(&doc, &connector);
    assert_eq!(edited.waypoints.len(), 2);
    assert!(format!("{edited:?}").contains("then"), "{edited:?}");
    let back = round_trip(&mut doc);
    assert_eq!(element(&back, &connector), edited);
}

#[test]
fn layout_moves_survive_save_and_reopen() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 10.0, "A");
    let b = shape(&mut doc, 300.0, 80.0, "B");
    let c = shape(&mut doc, 700.0, 40.0, "C");
    run(&mut doc, "layout.align", json!({ "ids": [a, b, c], "edge": "top" }));
    run(&mut doc, "layout.distribute", json!({ "ids": [a, b, c], "axis": "horizontal" }));
    run(&mut doc, "layout.stack", json!({ "ids": [a, b, c], "axis": "vertical", "gap": 10 }));
    let positions = |doc: &BoardDocument| [&a, &b, &c].map(|id| element(doc, id).position);
    let edited = positions(&doc);
    assert_ne!(edited[0], edited[1]);
    let back = round_trip(&mut doc);
    assert_eq!(positions(&back), edited);
}

#[test]
fn mind_maps_survive_save_and_reopen() {
    let mut doc = board();
    let root = ids(&run(
        &mut doc,
        "element.add",
        json!({ "elements": [{ "kind": "shape", "shapeStyle": "mind-node", "position": [0, 0], "size": [160, 60], "text": "Plan", "mindMapDirection": "right" }] }),
    ))
    .remove(0);
    let goals = run(&mut doc, "mindmap.addChild", json!({ "node": root, "text": "Goals" }))["node"].as_str().unwrap().to_owned();
    run(&mut doc, "mindmap.addChild", json!({ "node": goals, "text": "Q4" }));
    run(&mut doc, "mindmap.addSibling", json!({ "node": goals, "text": "Risks" }));
    run(&mut doc, "mindmap.collapse", json!({ "node": goals }));
    run(&mut doc, "mindmap.setDirection", json!({ "node": root, "direction": "left" }));
    run(&mut doc, "layout.mindmap", json!({ "root": root }));
    let count = doc.model().pages()[0].elements.len();
    assert!(count >= 7, "root, three nodes and their branches: {count}");
    let back = round_trip(&mut doc);
    assert_eq!(back.model().pages()[0].elements.len(), count);
}

#[test]
fn pages_survive_save_and_reopen() {
    let mut doc = board();
    shape(&mut doc, 0.0, 0.0, "First page");
    let first = doc.model().pages()[0].id.to_string();
    let second = run(&mut doc, "page.add", json!({ "name": "Ideas", "activate": true }))["page"].as_str().unwrap().to_owned();
    shape(&mut doc, 0.0, 0.0, "On Ideas");
    run(&mut doc, "page.rename", json!({ "page": second, "name": "Later" }));
    run(&mut doc, "page.duplicate", json!({ "page": first }));
    run(&mut doc, "page.move", json!({ "page": second, "to": 0 }));
    run(&mut doc, "page.activate", json!({ "page": first }));
    let names = |doc: &BoardDocument| doc.model().pages().iter().map(|page| page.name.clone()).collect::<Vec<_>>();
    let edited = names(&doc);
    assert_eq!(edited.len(), 3);
    assert_eq!(edited[0], "Later");
    let back = round_trip(&mut doc);
    assert_eq!(names(&back), edited);
}

#[test]
fn pictures_and_link_cards_survive_save_and_reopen() {
    let mut doc = board();
    let picture = run(
        &mut doc,
        "image.insert",
        json!({ "data": kition_board_kboard::base64_encode(b"GIF89a\x10\x00\x20\x00rest"), "position": [0, 0], "alt": "Logo" }),
    );
    let picture_id = ids(&picture).remove(0);
    run(&mut doc, "image.setAlt", json!({ "id": picture_id, "alt": "Kition logo" }));
    let link = ids(&run(&mut doc, "card.add", json!({ "url": "https://kition.ai", "title": "Kition" }))).remove(0);
    let blob = picture["blob"].as_str().unwrap().to_owned();
    let back = round_trip(&mut doc);
    assert!(back.has_blob(&blob), "the picture's bytes travel in the file");
    assert_eq!(element(&back, &picture_id).register("alt"), Some(&json!("Kition logo")));
    assert_eq!(element(&back, &link).card.unwrap()["url"], json!("https://kition.ai"));
}
