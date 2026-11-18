//! C8-board-engine, derived data and answers: a bound end's stored point follows a text without
//! a stored size when its text or letters change; a layout answers only what it moved and keeps
//! the selection's extent; an agent's command does not land on another page when the page it
//! works in is gone. Found by the independent reading of the write handlers (C8-board-read).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kdm::{End, Point, Rect};
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::{ObjectId, Origin};
use kition_kit::session::KitSessionDocument;
use serde_json::{Value, json};

fn board() -> BoardDocument {
    BoardDocument::create("Derived.kboard", 1920, 1080).unwrap()
}

fn run(doc: &mut BoardDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("{command} {params}: {error}"))
}

fn created(answer: &Value) -> Vec<String> {
    answer["created"].as_array().unwrap().iter().map(|id| id.as_str().unwrap().to_owned()).collect()
}

fn stored_end(doc: &BoardDocument, id: &str, terminal: &str) -> End {
    End::parse(&doc.session().replica().get(&ObjectId::from_string(id), terminal).unwrap()).unwrap()
}

/// The stored point of `connector`'s start and the one a reader derives from its target.
fn start_points(doc: &BoardDocument, connector: &str) -> (Point, Point) {
    let element = doc.model().element(&ObjectId::from_string(connector)).unwrap().clone();
    (stored_end(doc, connector, "start").point, doc.model().end_point(&element.id, element.start.as_ref().unwrap()))
}

/// A connector bound to the right edge of a text without a stored size keeps its stored point
/// where the text's measured box puts it after a new font size, a new text and a mark.
#[test]
fn an_end_on_a_sizeless_text_follows_its_text() {
    let mut doc = board();
    let text = created(&run(&mut doc, "element.create", json!({"elements":[{"kind":"text","position":[0,0],"text":"Short"}]})))[0].clone();
    let connector =
        created(&run(&mut doc, "connector.create", json!({"from":{"element":text,"anchor":[1,0.5]},"to":{"point":[800,800]}})))[0].clone();
    let mut last = start_points(&doc, &connector).1;
    for (command, params) in [
        ("element.style", json!({"ids":[text],"style":{"fontSize":48}})),
        ("element.setText", json!({"id":text,"text":"A much longer text than before"})),
        ("text.mark", json!({"id":text,"range":[0,30],"mark":"bold"})),
    ] {
        run(&mut doc, command, params);
        let (stored, derived) = start_points(&doc, &connector);
        assert_eq!(stored, derived, "{command}: the stored point is where a reader derives it");
        if command != "text.mark" {
            assert_ne!(derived, last, "{command} changes where the end is");
        }
        last = derived;
    }
}

/// Aligning a selection with a connector bound at both ends answers only what moved: the
/// connector follows its ends, it has nothing of its own to move.
#[test]
fn alignment_answers_only_what_it_moves() {
    let mut doc = board();
    let made = created(&run(
        &mut doc,
        "element.create",
        json!({"elements":[{"kind":"shape","position":[0,0],"size":[100,100]},{"kind":"shape","position":[300,200],"size":[100,100]}]}),
    ));
    let (a, b) = (made[0].clone(), made[1].clone());
    let connector = created(&run(&mut doc, "connector.create", json!({"from":{"element":a},"to":{"element":b}})))[0].clone();
    let before = doc.session().replica().get(&ObjectId::from_string(&connector), "waypoints").unwrap();
    let answer = run(&mut doc, "layout.align", json!({"ids":[a, b, connector],"edge":"top"}));
    let moved: Vec<&str> = answer["moved"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
    assert_eq!(moved, vec![b.as_str()], "{answer}");
    assert_eq!(doc.session().replica().get(&ObjectId::from_string(&connector), "waypoints").unwrap(), before);
}

/// Distributing keeps the selection's extent: the far end is where the furthest element ends,
/// not where the last one starts.
#[test]
fn distribution_keeps_the_selections_extent() {
    let mut doc = board();
    let made = created(&run(
        &mut doc,
        "element.create",
        json!({"elements":[
            {"kind":"shape","position":[0,0],"size":[100,50]},
            {"kind":"shape","position":[50,100],"size":[950,50]},
            {"kind":"shape","position":[200,200],"size":[100,50]}
        ]}),
    ));
    run(&mut doc, "layout.distribute", json!({"ids":made,"axis":"horizontal"}));
    let bounds =
        made.iter().map(|id| doc.model().bounds(doc.model().element(&ObjectId::from_string(id)).unwrap()).unwrap()).reduce(Rect::union).unwrap();
    assert_eq!((bounds.x0, bounds.x1), (0.0, 1000.0));
}

/// An agent's command without `page` works in the page the editor shows; when another writer
/// deleted that page, it is refused instead of landing on the first page. A person's command
/// works in the page the editor falls back to showing.
#[test]
fn an_agents_command_does_not_land_on_another_page() {
    let mut doc = board();
    let first = doc.model().pages()[0].id.as_str().to_owned();
    let other = run(&mut doc, "page.create", json!({"name":"Other"}))["page"].as_str().unwrap().to_owned();
    run(&mut doc, "page.activate", json!({"page":other}));
    run(&mut doc, "page.delete", json!({"page":other}));
    let agent = Origin::agent("agent:test");
    let shape = json!({"elements":[{"kind":"shape","position":[0,0]}]});
    let before = doc.session().replica().state_json().unwrap();
    let refused = doc.execute_as("element.create", shape.clone(), &agent);
    assert!(refused.as_ref().is_err_and(|error| error.starts_with("not-found")), "{refused:?}");
    assert_eq!(doc.session().replica().state_json().unwrap(), before);
    let made = created(&run(&mut doc, "element.create", shape));
    assert_eq!(doc.model().page_of(&ObjectId::from_string(&made[0])).unwrap().id.as_str(), first);
}
