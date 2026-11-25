//! C8-board-engine, structure: commands keep the hierarchy their parameters say (an ungroup of
//! nested groups, a dropped group member, a container's stacking, a turned group). Found by the
//! independent reading of the write handlers (C8-board-read).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kdm::{Element, Point};
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::ObjectId;
use serde_json::{Value, json};

fn board() -> BoardDocument {
    BoardDocument::create("Structure.kboard", 1920, 1080).unwrap()
}

fn run(doc: &mut BoardDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("{command} {params}: {error}"))
}

fn shape(doc: &mut BoardDocument, x: f64, y: f64) -> String {
    let made = run(doc, "element.add", json!({"elements":[{"kind":"shape","position":[x,y],"size":[100,50]}]}));
    made["created"][0].as_str().unwrap().to_owned()
}

fn element(doc: &BoardDocument, id: &str) -> Element {
    doc.model().element(&ObjectId::from_string(id)).cloned().unwrap_or_else(|| panic!("no element {id}"))
}

fn parent(doc: &BoardDocument, id: &str) -> Option<String> {
    element(doc, id).parent.map(|parent| parent.as_str().to_owned())
}

fn order(doc: &BoardDocument) -> Vec<String> {
    doc.model().pages()[0].elements.iter().map(|id| id.as_str().to_owned()).collect()
}

fn close(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
}

/// Ungrouping a group and the group inside it, outer first, leaves every member at the outer
/// group's parent (none here), not at the deleted outer group; the saved file reopens as it was.
#[test]
fn ungrouping_nested_groups_together_frees_every_member() {
    for outer_first in [true, false] {
        let mut doc = board();
        let (a, b, c) = (shape(&mut doc, 0.0, 0.0), shape(&mut doc, 200.0, 0.0), shape(&mut doc, 400.0, 0.0));
        let inner = run(&mut doc, "arrange.group", json!({"ids":[a, b]}))["group"].as_str().unwrap().to_owned();
        let outer = run(&mut doc, "arrange.group", json!({"ids":[inner, c]}))["group"].as_str().unwrap().to_owned();
        let groups = if outer_first { json!([outer, inner]) } else { json!([inner, outer]) };
        run(&mut doc, "arrange.ungroup", json!({"groupIds":groups}));
        for id in [&a, &b, &c] {
            assert_eq!(parent(&doc, id), None, "outer_first={outer_first}: {id} keeps no deleted parent");
        }
        let saved = doc.model().clone();
        let reopened = BoardDocument::open("Structure.kboard", &doc.save(None).unwrap()).unwrap();
        assert_eq!(reopened.model(), &saved, "outer_first={outer_first}: no repair on reopen");
    }
}

/// A drop (`reparent`) keeps a group's member in its group, as the command says, even when its
/// centre lands in a frame.
#[test]
fn a_dropped_group_member_stays_in_its_group() {
    let mut doc = board();
    let frame = run(&mut doc, "frame.add", json!({"box":[0,400,600,800]}))["frame"].as_str().unwrap().to_owned();
    let (a, b) = (shape(&mut doc, 0.0, 0.0), shape(&mut doc, 200.0, 0.0));
    let group = run(&mut doc, "arrange.group", json!({"ids":[a, b]}))["group"].as_str().unwrap().to_owned();
    let answer = run(&mut doc, "element.move", json!({"ids":[a],"delta":[100,500],"reparent":true}));
    assert_eq!(parent(&doc, &a).as_deref(), Some(group.as_str()), "{answer}");
    assert_eq!(answer["reparented"], json!([]));
    let _ = frame;
}

/// A container moves in the stacking order with what it holds: brought to the front, a frame
/// stays behind its children (and a group before its members), as v1's `reorderBoardElements`
/// (with descendants).
#[test]
fn a_container_is_reordered_with_its_content() {
    let mut doc = board();
    let child = shape(&mut doc, 30.0, 30.0);
    let frame = run(&mut doc, "frame.add", json!({"box":[0,0,400,300],"children":[child]}))["frame"].as_str().unwrap().to_owned();
    let other = shape(&mut doc, 600.0, 0.0);
    assert_eq!(order(&doc), vec![frame.clone(), child.clone(), other.clone()]);
    run(&mut doc, "element.reorder", json!({"ids":[frame],"placement":"front"}));
    assert_eq!(order(&doc), vec![other.clone(), frame.clone(), child.clone()], "front");
    run(&mut doc, "element.reorder", json!({"ids":[frame],"placement":"back"}));
    assert_eq!(order(&doc), vec![frame.clone(), child.clone(), other.clone()], "back");
    run(&mut doc, "element.reorder", json!({"ids":[frame],"placement":"forward"}));
    assert_eq!(order(&doc), vec![other.clone(), frame.clone(), child.clone()], "forward");
    run(&mut doc, "element.reorder", json!({"ids":[frame],"placement":"backward"}));
    assert_eq!(order(&doc), vec![frame, child, other], "backward");
}

/// `element.rotate` writes `rotation` only where the kind has one (`element.set` refuses it
/// on a frame or a card); about each element's centre, a group turns as one about its own centre.
#[test]
fn rotation_follows_the_kinds_and_turns_a_group_as_one() {
    let mut doc = board();
    let frame = run(&mut doc, "frame.add", json!({"box":[0,400,400,700]}))["frame"].as_str().unwrap().to_owned();
    let card = run(&mut doc, "card.add", json!({"url":"https://example.com","position":[600,400]}))["created"][0].as_str().unwrap().to_owned();
    run(&mut doc, "element.rotate", json!({"ids":[frame],"degrees":30}));
    run(&mut doc, "element.rotate", json!({"ids":[card, frame],"degrees":30,"about":"selection"}));
    for id in [&frame, &card] {
        assert_eq!(element(&doc, id).rotation, 0.0, "{id}: a frame or card has no rotation");
        assert!(doc.execute("element.set", json!({"id":id,"set":{"rotation":30}})).is_err());
    }
    let (a, b) = (shape(&mut doc, 0.0, 0.0), shape(&mut doc, 200.0, 0.0));
    let group = run(&mut doc, "arrange.group", json!({"ids":[a, b]}))["group"].as_str().unwrap().to_owned();
    let centre = doc.model().bounds(&element(&doc, &group)).unwrap().center();
    let before = |id: &str| doc.model().rect(&element(&doc, id)).unwrap().center();
    let (ca, cb) = (before(&a), before(&b));
    run(&mut doc, "element.rotate", json!({"ids":[group],"degrees":90}));
    let after = |id: &str| doc.model().rect(&element(&doc, id)).unwrap().center();
    assert!(close(after(&a), ca.rotate(centre, 90.0)), "{:?} {:?}", after(&a), ca.rotate(centre, 90.0));
    assert!(close(after(&b), cb.rotate(centre, 90.0)));
    assert_eq!((element(&doc, &a).rotation, element(&doc, &b).rotation), (90.0, 90.0));
}
