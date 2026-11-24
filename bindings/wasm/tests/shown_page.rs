//! The page the editor shows (task board-page-switcher): showing a page is view state, and when
//! the shown page goes (the person deletes it, an undo takes back its creation, an agent or a
//! co-editor deletes it) the editor shows a page that is there: the page the person came from to a
//! page they made, else its neighbour. A redo or an undo that brings the page back shows it again.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::Origin;
use kition_kit::session::KitSessionDocument;
use serde_json::{Value, json};

const FILE: &str = "Plan.kboard";

fn board() -> BoardDocument {
    BoardDocument::create(FILE, 1920, 1080).unwrap()
}

fn run(doc: &mut BoardDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("{command} {params}: {error}"))
}

fn shown(doc: &BoardDocument) -> String {
    doc.inspect().unwrap()["page"].as_str().unwrap().to_owned()
}

fn first(doc: &BoardDocument) -> String {
    doc.model().pages()[0].id.as_str().to_owned()
}

/// Pages named A (the board's own), B, C and D, in that order; answers their ids.
fn four_pages(doc: &mut BoardDocument) -> [String; 4] {
    let a = first(doc);
    let [b, c, d] = ["B", "C", "D"].map(|name| run(doc, "page.create", json!({ "name": name }))["page"].as_str().unwrap().to_owned());
    [a, b, c, d]
}

#[test]
fn showing_a_page_is_not_an_edit() {
    let mut doc = board();
    let [_, b, ..] = four_pages(&mut doc);
    doc.prepare_save().unwrap();
    let saved = doc.save(Some(FILE)).unwrap();
    assert!(!saved.is_empty());
    doc.mark_saved();
    let (revision, state, undo) = (doc.revision(), doc.session().replica().state_json().unwrap(), doc.inspect().unwrap()["canUndo"].clone());
    run(&mut doc, "page.activate", json!({ "page": b }));
    assert_eq!(shown(&doc), b);
    assert_eq!(doc.revision(), revision, "the revision stays");
    assert!(!doc.dirty(), "the board stays saved");
    assert_eq!(doc.session().replica().state_json().unwrap(), state);
    assert_eq!(doc.inspect().unwrap()["canUndo"], undo, "no undo step");
    run(&mut doc, "edit.undo", json!({}));
    assert_eq!(shown(&doc), b, "an undo takes back the last edit, not the switch");
}

#[test]
fn deleting_the_shown_page_shows_its_neighbour() {
    let mut doc = board();
    let [a, b, c, d] = four_pages(&mut doc);
    run(&mut doc, "page.activate", json!({ "page": c }));
    run(&mut doc, "page.delete", json!({ "page": c }));
    assert_eq!(shown(&doc), b, "the page before it");
    run(&mut doc, "page.activate", json!({ "page": a }));
    run(&mut doc, "page.delete", json!({ "page": a }));
    assert_eq!(shown(&doc), b, "the first page's place is taken by the next");
    run(&mut doc, "edit.undo", json!({}));
    assert_eq!(shown(&doc), a, "an undo of the delete shows the page again");
    run(&mut doc, "edit.redo", json!({}));
    assert_eq!(shown(&doc), b);
    // A move of the pages afterwards does not change what is shown.
    run(&mut doc, "page.move", json!({ "page": d, "to": 0 }));
    assert_eq!(shown(&doc), b);
}

#[test]
fn an_undo_of_the_persons_new_page_takes_them_back_and_a_redo_shows_it_again() {
    let mut doc = board();
    let [a, ..] = four_pages(&mut doc);
    run(&mut doc, "page.activate", json!({ "page": a }));
    let made = run(&mut doc, "page.create", json!({ "activate": true }))["page"].as_str().unwrap().to_owned();
    assert_eq!(shown(&doc), made);
    run(&mut doc, "edit.undo", json!({}));
    assert_eq!(doc.model().pages().len(), 4);
    assert_eq!(shown(&doc), a, "back on the page they made it from, not the last page's neighbour");
    run(&mut doc, "edit.redo", json!({}));
    assert_eq!(shown(&doc), made, "the redo shows the page again");
}

#[test]
fn an_agent_deleting_the_persons_page_leaves_them_on_its_neighbour() {
    let mut doc = board();
    let [_, b, c, _] = four_pages(&mut doc);
    run(&mut doc, "page.activate", json!({ "page": c }));
    // Kit UI protocol 2.9: the person's state is put aside, the agent's step runs in its own, and
    // the person's is put back.
    let person = doc.editor_state();
    doc.execute_as("page.delete", json!({ "page": c }), &Origin::agent("agent:test")).unwrap();
    doc.restore_editor_state(&person).unwrap();
    assert_eq!(shown(&doc), b);
    // And an agent's step that deletes another page leaves the person where they are.
    let person = doc.editor_state();
    let a = first(&doc);
    doc.execute_as("page.delete", json!({ "page": a }), &Origin::agent("agent:test")).unwrap();
    doc.restore_editor_state(&person).unwrap();
    assert_eq!(shown(&doc), b);
}

#[test]
fn a_co_editor_deleting_the_shown_page_leaves_the_editor_on_its_neighbour() {
    let mut live = board();
    let [a, b, c, _] = four_pages(&mut live);
    live.prepare_save().unwrap();
    let bytes = live.save(Some(FILE)).unwrap();
    live.mark_saved();
    let mut peer = BoardDocument::open(FILE, &bytes).unwrap();
    run(&mut live, "page.activate", json!({ "page": b }));
    let since = live.version();
    run(&mut peer, "page.delete", json!({ "page": b }));
    let changes = peer.session().export_changes(&since).unwrap();
    KitSessionDocument::import_changes(&mut live, &changes).unwrap();
    assert_eq!(shown(&live), a, "the page before it");
    // The peer moves the pages afterwards: the editor stays on the page it shows.
    let since = live.version();
    run(&mut peer, "page.move", json!({ "page": c, "to": 0 }));
    let changes = peer.session().export_changes(&since).unwrap();
    KitSessionDocument::import_changes(&mut live, &changes).unwrap();
    assert_eq!(shown(&live), a);
}
