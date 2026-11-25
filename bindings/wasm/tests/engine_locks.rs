//! C8-board-engine, locks: a command refuses when it would write a locked element, and writes
//! nothing then; what it does not write (a resized frame's content) may be locked. Found by the
//! independent reading of the write handlers (C8-board-read).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::ObjectId;
use kition_kit::session::KitSessionDocument;
use serde_json::{Value, json};

fn board() -> BoardDocument {
    BoardDocument::create("Locks.kboard", 1920, 1080).unwrap()
}

fn run(doc: &mut BoardDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("{command} {params}: {error}"))
}

fn state(doc: &BoardDocument) -> Value {
    doc.session().replica().state_json().unwrap()
}

/// Runs `command`, which must refuse as locked and leave the board as it was.
fn refused(doc: &mut BoardDocument, command: &str, params: Value) {
    let before = state(doc);
    let answer = doc.execute(command, params.clone());
    assert!(answer.as_ref().is_err_and(|error| error.starts_with("locked")), "{command} {params}: {answer:?}");
    assert_eq!(state(doc), before, "{command}: a refused command changed the board");
}

fn shape(doc: &mut BoardDocument, x: f64, y: f64) -> String {
    let made = run(doc, "element.add", json!({"elements":[{"kind":"shape","position":[x,y],"size":[100,50]}]}));
    made["created"][0].as_str().unwrap().to_owned()
}

fn frame_with_child(doc: &mut BoardDocument) -> (String, String) {
    let child = shape(doc, 30.0, 30.0);
    let frame = run(doc, "frame.add", json!({"box":[0,0,400,300],"children":[child]}))["frame"].as_str().unwrap().to_owned();
    (frame, child)
}

/// Deleting a frame deletes what is inside it: a locked child refuses the delete as the child
/// alone does.
#[test]
fn deleting_a_container_refuses_its_locked_member() {
    let mut doc = board();
    let (frame, child) = frame_with_child(&mut doc);
    run(&mut doc, "element.lock", json!({"ids":[child],"locked":true}));
    refused(&mut doc, "element.delete", json!({"ids":[frame]}));
    let group_member = shape(&mut doc, 600.0, 0.0);
    let other = shape(&mut doc, 800.0, 0.0);
    let group = run(&mut doc, "arrange.group", json!({"ids":[group_member, other]}))["group"].as_str().unwrap().to_owned();
    run(&mut doc, "element.lock", json!({"ids":[group_member],"locked":true}));
    refused(&mut doc, "element.delete", json!({"ids":[group]}));
}

/// A page's delete deletes every element on it: a locked one refuses it.
#[test]
fn deleting_a_page_refuses_its_locked_element() {
    let mut doc = board();
    let other = run(&mut doc, "page.add", json!({"name":"Other"}))["page"].as_str().unwrap().to_owned();
    let made = run(&mut doc, "element.add", json!({"page":other,"elements":[{"kind":"shape","position":[0,0]}]}));
    let id = made["created"][0].as_str().unwrap().to_owned();
    run(&mut doc, "element.lock", json!({"ids":[id],"locked":true}));
    refused(&mut doc, "page.delete", json!({"page":other}));
    run(&mut doc, "element.lock", json!({"ids":[id],"locked":false}));
    run(&mut doc, "page.delete", json!({"page":other}));
    assert_eq!(doc.model().pages().len(), 1);
}

/// A mind-map command lays out the whole map and rewrites its branches: a locked branch refuses
/// it as a locked node does.
#[test]
fn a_locked_branch_refuses_the_maps_layout() {
    let mut doc = board();
    let made =
        run(&mut doc, "element.add", json!({"elements":[{"kind":"shape","shapeStyle":"mind-node","position":[0,0],"size":[142,56],"text":"Root"}]}));
    let root = made["created"][0].as_str().unwrap().to_owned();
    let added = run(&mut doc, "mindmap.addChild", json!({"node":root,"text":"One"}));
    let branch = added["created"][0].as_str().unwrap().to_owned();
    run(&mut doc, "element.lock", json!({"ids":[branch],"locked":true}));
    refused(&mut doc, "mindmap.setDirection", json!({"node":root,"direction":"left"}));
    refused(&mut doc, "mindmap.addChild", json!({"node":root,"text":"Two"}));
}

/// Resizing one box writes that box only (a frame's content stays where it is): a locked child
/// does not refuse it, the box's own lock does. A group or a stroke is scaled with what it holds,
/// so a locked member refuses it; a move and a turn write the whole subtree.
#[test]
fn resizing_one_box_checks_the_box_it_writes() {
    let mut doc = board();
    let (frame, child) = frame_with_child(&mut doc);
    run(&mut doc, "element.lock", json!({"ids":[child],"locked":true}));
    let before = doc.model().element(&ObjectId::from_string(&child)).cloned().unwrap();
    run(&mut doc, "element.resize", json!({"id":frame,"box":[0,0,500,400]}));
    assert_eq!(doc.model().element(&ObjectId::from_string(&child)).cloned().unwrap(), before, "the locked child stays as it was");
    refused(&mut doc, "element.move", json!({"ids":[frame],"delta":[10,0]}));
    refused(&mut doc, "element.rotate", json!({"ids":[frame],"degrees":90}));
    run(&mut doc, "element.lock", json!({"ids":[frame],"locked":true}));
    refused(&mut doc, "element.resize", json!({"id":frame,"box":[0,0,300,300]}));
    let a = shape(&mut doc, 600.0, 0.0);
    let b = shape(&mut doc, 800.0, 0.0);
    let group = run(&mut doc, "arrange.group", json!({"ids":[a, b]}))["group"].as_str().unwrap().to_owned();
    run(&mut doc, "element.lock", json!({"ids":[a],"locked":true}));
    refused(&mut doc, "element.resize", json!({"id":group,"box":[600,0,1000,100]}));
}
