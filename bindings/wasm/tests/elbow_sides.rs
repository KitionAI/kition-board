//! An elbow connector leaves and enters through the sides its ends are bound to (case
//! `regressions/issue-elbow-sides`). Found drawing an order flow chart
//! (`examples/flowchart_sheet.rs`): every elbow went sideways out of both ends and turned once
//! halfway across, so a line bound to a shape's bottom left it sideways, an arrow bound to a pill's
//! top arrived along the top edge with its head lying on the outline, and a branch from a
//! diamond's left corner to a box below turned back across the diamond.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kdm::Point;
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use serde_json::{Value, json};

fn fragment(doc: &BoardDocument, id: &str) -> String {
    let answer: Value = serde_json::from_str(&doc.element_svg(&json!({ "ids": [id] }).to_string()).unwrap()).unwrap();
    answer["fragments"][0]["svg"].as_str().unwrap().to_owned()
}

/// The connector's whole route (its fragment's first path, the hit path), as the points of its
/// path data; a rounded corner's control point is the corner itself.
fn route(doc: &BoardDocument, id: &str) -> Vec<Point> {
    let svg = fragment(doc, id);
    let d = svg.split(r#"d=""#).nth(1).unwrap().split('"').next().unwrap();
    let numbers: Vec<f64> = d.split_whitespace().filter_map(|token| token.parse().ok()).collect();
    numbers.chunks(2).map(|pair| Point::new(pair[0], pair[1])).collect()
}

fn d(doc: &BoardDocument, id: &str) -> String {
    let svg = fragment(doc, id);
    svg.split(r#"d=""#).nth(1).unwrap().split('"').next().unwrap().to_owned()
}

fn shapes(doc: &mut BoardDocument, list: Value) -> Vec<String> {
    let made = doc.execute("element.create", json!({ "elements": list })).unwrap();
    made["created"].as_array().unwrap().iter().map(|id| id.as_str().unwrap().to_owned()).collect()
}

fn elbow(doc: &mut BoardDocument, from: (&str, [f64; 2]), to: (&str, [f64; 2])) -> String {
    let made = doc
        .execute(
            "connector.create",
            json!({ "from": { "element": from.0, "anchor": from.1 }, "to": { "element": to.0, "anchor": to.1 }, "type": "elbow", "endArrowhead": "triangle" }),
        )
        .unwrap();
    made["created"][0].as_str().unwrap().to_owned()
}

#[test]
fn bottom_to_top_leaves_downward_and_arrives_downward() {
    let mut doc = BoardDocument::create("Elbows.kboard", 800, 600).unwrap();
    let ids = shapes(
        &mut doc,
        json!([{ "kind": "shape", "position": [0, 0], "size": [100, 60] }, { "kind": "shape", "position": [300, 300], "size": [100, 60] }]),
    );
    let k = elbow(&mut doc, (&ids[0], [0.5, 1.0]), (&ids[1], [0.5, 0.0]));
    let points = route(&doc, &k);
    let (first, second) = (points[0], points[1]);
    assert_eq!(first, Point::new(50.0, 60.0), "{points:?}");
    assert!(second.x == 50.0 && second.y > 60.0, "leaves the bottom downward: {points:?}");
    let (before, last) = (points[points.len() - 2], points[points.len() - 1]);
    assert_eq!(last, Point::new(350.0, 300.0), "{points:?}");
    assert!(before.x == 350.0 && before.y < 300.0, "arrives at the top from above: {points:?}");
}

#[test]
fn right_side_to_a_top_below_turns_once() {
    let mut doc = BoardDocument::create("Elbows.kboard", 800, 600).unwrap();
    let ids = shapes(
        &mut doc,
        json!([
            { "kind": "shape", "position": [0, 0], "size": [100, 60] },
            { "kind": "shape", "shapeType": "pill", "position": [300, 300], "size": [100, 60] }
        ]),
    );
    let k = elbow(&mut doc, (&ids[0], [1.0, 0.5]), (&ids[1], [0.5, 0.0]));
    assert_eq!(d(&doc, &k), "M 100 30 L 338 30 Q 350 30 350 42 L 350 300", "right, then down into the pill's top");
}

#[test]
fn a_side_facing_away_steps_out_before_it_turns() {
    // The "no" branch: from a diamond's left corner down to the left side of a box below it.
    let mut doc = BoardDocument::create("Elbows.kboard", 800, 600).unwrap();
    let ids = shapes(
        &mut doc,
        json!([
            { "kind": "shape", "shapeType": "diamond", "position": [100, 100], "size": [200, 140] },
            { "kind": "shape", "position": [140, 370], "size": [200, 90] }
        ]),
    );
    let k = elbow(&mut doc, (&ids[0], [0.0, 0.5]), (&ids[1], [0.0, 0.5]));
    let points = route(&doc, &k);
    assert_eq!(points[0], Point::new(100.0, 170.0), "{points:?}");
    assert!(points[1].y == 170.0 && points[1].x < 100.0, "leaves the left corner leftward: {points:?}");
    let last = points[points.len() - 1];
    assert_eq!(last, Point::new(140.0, 415.0), "{points:?}");
    assert!(points[..points.len() - 1].iter().all(|point| point.x < 140.0), "stays left of both shapes until it enters: {points:?}");
    assert!(points[points.len() - 2].y == 415.0, "arrives at the left side moving right: {points:?}");
}

#[test]
fn free_ends_keep_the_route_they_had() {
    let mut doc = BoardDocument::create("Elbows.kboard", 800, 600).unwrap();
    let made = doc.execute("connector.create", json!({ "from": { "point": [0, 0] }, "to": { "point": [100, 50] }, "type": "elbow" })).unwrap();
    let k = made["created"][0].as_str().unwrap().to_owned();
    assert_eq!(d(&doc, &k), "M 0 0 L 38 0 Q 50 0 50 12 L 50 38 Q 50 50 62 50 L 100 50");
    // Two boxes side by side, bound at the facing sides: across, down and across, as before.
    let ids = shapes(
        &mut doc,
        json!([{ "kind": "shape", "position": [0, 200], "size": [100, 60] }, { "kind": "shape", "position": [300, 300], "size": [100, 60] }]),
    );
    let k = elbow(&mut doc, (&ids[0], [1.0, 0.5]), (&ids[1], [0.0, 0.5]));
    assert_eq!(d(&doc, &k), "M 100 230 L 188 230 Q 200 230 200 242 L 200 318 Q 200 330 212 330 L 300 330");
}
