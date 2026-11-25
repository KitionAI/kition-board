//! C8-board-engine, paste and pictures: a paste writes a healthy board (no binding the repair
//! would drop, no anchor it would clamp, no parent it would clear), puts its copies where `at`
//! says, and stores a bound end where the copy's text puts it; a refused picture stores no
//! bytes. Found by the independent reading of the write handlers (C8-board-read).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kdm::{BoardModel, End, Point};
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::ObjectId;
use kition_kit::session::KitSessionDocument;
use serde_json::{Value, json};

/// A 1 × 1 PNG.
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

fn board() -> BoardDocument {
    BoardDocument::create("Paste.kboard", 1920, 1080).unwrap()
}

fn run(doc: &mut BoardDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("{command} {params}: {error}"))
}

fn payload(elements: Value) -> Value {
    json!({"type":"application/x-kition-board+json","version":1,"elements":elements})
}

fn paste(doc: &mut BoardDocument, elements: Value, at: Option<[f64; 2]>) -> Vec<String> {
    let mut params = json!({"payload": payload(elements)});
    if let Some(at) = at {
        params["at"] = json!(at);
    }
    let answer = run(doc, "element.paste", params);
    answer["created"].as_array().unwrap().iter().map(|id| id.as_str().unwrap().to_owned()).collect()
}

fn stored_end(doc: &BoardDocument, id: &str, terminal: &str) -> End {
    End::parse(&doc.session().replica().get(&ObjectId::from_string(id), terminal).unwrap()).unwrap()
}

/// What the board shows after saving and opening the file again: the open-time repair changes
/// nothing on a healthy board.
fn reopened(doc: &mut BoardDocument) -> BoardModel {
    BoardDocument::open("Paste.kboard", &doc.save(None).unwrap()).unwrap().model().clone()
}

fn healthy(doc: &mut BoardDocument, what: &str) {
    assert_eq!(&reopened(doc), doc.model(), "{what}: the repair would change the pasted board");
}

/// A pasted connector keeps a binding only to what can take one (rule V6): a stroke in the
/// payload, or a group on the page, frees the end.
#[test]
fn a_pasted_binding_to_what_takes_none_is_freed() {
    let mut doc = board();
    let (a, b) = {
        let made = run(&mut doc, "element.create", json!({"elements":[{"kind":"shape","position":[0,0]},{"kind":"shape","position":[300,0]}]}));
        (made["created"][0].as_str().unwrap().to_owned(), made["created"][1].as_str().unwrap().to_owned())
    };
    let group = run(&mut doc, "element.group", json!({"ids":[a, b]}))["group"].as_str().unwrap().to_owned();
    let made = paste(
        &mut doc,
        json!([
            {"ref":"s","kind":"stroke","points":[[0,500],[100,600]]},
            {"ref":"k","kind":"connector","start":{"point":[0,500],"binding":{"target":"s","anchor":[0.5,0.5]}},"end":{"point":[400,400],"binding":{"target":group,"anchor":[0.5,0.5]}}}
        ]),
        None,
    );
    let connector = &made[1];
    assert!(stored_end(&doc, connector, "start").binding.is_none(), "bound to a stroke");
    assert!(stored_end(&doc, connector, "end").binding.is_none(), "bound to a group");
    healthy(&mut doc, "bindings");
}

/// A pasted anchor is clamped to [0, 1] (rule V7) as connector.create clamps one.
#[test]
fn a_pasted_anchor_is_clamped() {
    let mut doc = board();
    let made = paste(
        &mut doc,
        json!([
            {"ref":"a","kind":"shape","position":[0,0],"size":[100,100]},
            {"ref":"k","kind":"connector","start":{"point":[0,0],"binding":{"target":"a","anchor":[5,-3]}},"end":{"point":[400,400]}}
        ]),
        None,
    );
    assert_eq!(stored_end(&doc, &made[1], "start").binding.unwrap().anchor, [1.0, 0.0]);
    healthy(&mut doc, "anchor");
}

/// A pasted `parent` is kept only where it names a container of the payload and makes no cycle
/// (rules V4 and V5).
#[test]
fn a_pasted_parent_must_be_a_container() {
    let mut doc = board();
    let made = paste(
        &mut doc,
        json!([
            {"ref":"a","kind":"shape","position":[0,0],"size":[100,100]},
            {"ref":"b","kind":"shape","position":[200,0],"size":[100,100],"parent":"a"},
            {"ref":"f","kind":"frame","position":[0,300],"size":[100,100],"parent":"g"},
            {"ref":"g","kind":"frame","position":[0,500],"size":[100,100],"parent":"f"}
        ]),
        None,
    );
    let parent = |id: &str| doc.model().element(&ObjectId::from_string(id)).unwrap().parent.clone();
    assert_eq!(parent(&made[1]), None, "a shape contains nothing");
    assert!(parent(&made[2]).is_none() || parent(&made[3]).is_none(), "no cycle");
    healthy(&mut doc, "parents");
}

/// `at` puts the copies' top-left there, a connector's points counted.
#[test]
fn pasting_at_a_point_counts_connector_points() {
    let mut doc = board();
    let made = paste(
        &mut doc,
        json!([
            {"ref":"a","kind":"shape","position":[100,100],"size":[100,100]},
            {"ref":"k","kind":"connector","start":{"point":[0,50]},"end":{"point":[100,150]},"waypoints":[[50,20]]}
        ]),
        Some([1000.0, 1000.0]),
    );
    let start = stored_end(&doc, &made[1], "start").point;
    let shape = doc.model().element(&ObjectId::from_string(&made[0])).unwrap().position.unwrap();
    assert_eq!((start, shape), (Point::new(1000.0, 1030.0), Point::new(1100.0, 1080.0)));
}

/// A pasted connector bound to a pasted text without a stored size stores the point the text's
/// measured box gives (what a reader derives), not that of an empty text.
#[test]
fn a_pasted_end_on_a_sizeless_text_is_where_the_text_is() {
    let mut doc = board();
    let made = paste(
        &mut doc,
        json!([
            {"ref":"t","kind":"text","position":[0,0],"text":"A text that is measured"},
            {"ref":"k","kind":"connector","start":{"point":[0,0],"binding":{"target":"t","anchor":[1,0.5]}},"end":{"point":[600,600]}}
        ]),
        None,
    );
    let connector = doc.model().element(&ObjectId::from_string(&made[1])).unwrap().clone();
    let derived = doc.model().end_point(&connector.id, connector.start.as_ref().unwrap());
    let stored = stored_end(&doc, &made[1], "start").point;
    assert!(derived.x > 10.0, "{derived:?}");
    assert_eq!(stored, derived);
}

/// image.insert refused for its `alt` stores no picture bytes.
#[test]
fn a_refused_picture_stores_no_bytes() {
    let hash = run(&mut board(), "image.insert", json!({"data":PNG,"position":[0,0]}))["blob"].as_str().unwrap().to_owned();
    let mut doc = board();
    let refused = doc.execute("image.insert", json!({"data":PNG,"position":[0,0],"alt":7}));
    assert!(refused.as_ref().is_err_and(|error| error.starts_with("invalid-params")), "{refused:?}");
    assert!(!doc.has_blob(&hash), "a refused insert left its bytes");
    run(&mut doc, "image.insert", json!({"data":PNG,"position":[0,0],"alt":"A dot"}));
    assert!(doc.has_blob(&hash));
}
