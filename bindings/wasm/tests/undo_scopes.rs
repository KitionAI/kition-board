//! Undo scopes (KitModel 1.1 M12a, engine rule E14a): the person's Undo takes back only the person's
//! steps and an agent's only the agent's, whichever is on top; `edit.undo` and `edit.redo` work in
//! the scope their `scope` parameter names, else the runner's.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::{ObjectId, Origin};
use kition_kit::session::KitSessionDocument;
use serde_json::{Value, json};

fn board() -> BoardDocument {
    BoardDocument::create("Plan.kboard", 1920, 1080).unwrap()
}

fn agent() -> Origin {
    Origin::agent("agent:test")
}

/// Creates one shape as `origin` and returns its id.
fn shape(doc: &mut BoardDocument, x: f64, origin: Option<&Origin>) -> String {
    let params = json!({ "elements": [{ "kind": "shape", "position": [x, 10.0], "size": [40.0, 30.0] }] });
    let created = match origin {
        Some(origin) => doc.execute_as("element.create", params, origin).unwrap(),
        None => doc.execute("element.create", params).unwrap(),
    };
    created["created"][0].as_str().unwrap().to_owned()
}

fn has(doc: &BoardDocument, id: &str) -> bool {
    doc.model().element(&ObjectId::from_string(id)).is_some()
}

fn done(answer: Value) -> bool {
    answer["done"].as_bool().unwrap()
}

#[test]
fn the_persons_undo_keeps_the_agents_edit() {
    let mut doc = board();
    let mine = shape(&mut doc, 10.0, None);
    let theirs = shape(&mut doc, 100.0, Some(&agent()));
    assert!(done(doc.execute("edit.undo", json!({})).unwrap()));
    assert!(!has(&doc, &mine), "the person's Undo takes back the person's step");
    assert!(has(&doc, &theirs), "and keeps the agent's step on top of it");
    assert!(!done(doc.execute("edit.undo", json!({})).unwrap()), "the person has no other step");
    assert!(has(&doc, &theirs));
}

#[test]
fn an_agent_scope_undo_takes_back_the_agents_edit() {
    let mut doc = board();
    let theirs = shape(&mut doc, 100.0, Some(&agent()));
    assert!(!doc.disabled_commands().contains(&"edit.undo".to_owned()), "Undo is offered while only the agent has a step");
    assert!(done(doc.execute("edit.undo", json!({ "scope": "agent" })).unwrap()));
    assert!(!has(&doc, &theirs));
    assert!(done(doc.execute("edit.redo", json!({ "scope": "agent" })).unwrap()));
    assert!(has(&doc, &theirs));
    // Run as the agent with no scope: the agent's own step.
    assert!(done(doc.execute_as("edit.undo", json!({}), &agent()).unwrap()));
    assert!(!has(&doc, &theirs));
}

#[test]
fn an_agent_undo_under_the_persons_later_step_keeps_the_persons_edit() {
    let mut doc = board();
    let theirs = shape(&mut doc, 100.0, Some(&agent()));
    let mine = shape(&mut doc, 10.0, None);
    assert!(done(doc.execute_as("edit.undo", json!({}), &agent()).unwrap()));
    assert!(!has(&doc, &theirs), "the agent's undo takes back its own step");
    assert!(has(&doc, &mine), "and keeps the person's later one");
    assert!(doc.execute("edit.undo", json!({ "scope": "nobody" })).is_err(), "a scope is the person's or the agent's");
}
