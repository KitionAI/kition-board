//! `board.connectors.ends`: a bound connector end meets its target's outline, not its box
//! (design note 1.4; case `board.connectors/outline-ends`). The end is derived, never repaired: a
//! board whose stored point is still on the box draws on the outline and saves nothing new.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kboard as kboard;
use kition_board_kdm::{End, Point};
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::{ObjectId, Origin};
use kition_kit::session::KitSessionDocument;
use serde_json::{Value, json};

/// The diamond's box: (400, 100)-(500, 200), centre (450, 150).
const DIAMOND: [f64; 4] = [400.0, 100.0, 100.0, 100.0];

/// Whether board point (x, y) is drawn dark.
fn dark(doc: &BoardDocument, x: f64, y: f64) -> bool {
    let px = doc.render(x, y, 1.0, 1, 1).unwrap();
    px[0] < 128 && px[1] < 128 && px[2] < 128
}

fn fragment(doc: &BoardDocument, id: &str) -> String {
    let answer: Value = serde_json::from_str(&doc.element_svg(&json!({ "ids": [id] }).to_string()).unwrap()).unwrap();
    answer["fragments"][0]["svg"].as_str().unwrap().to_owned()
}

/// Where the connector's route ends: the last point of its fragment's first path (the hit path,
/// which is the whole route; the drawn line stops short of the arrowhead).
fn route_end(svg: &str) -> Point {
    let d = svg.split(r#"d=""#).nth(1).unwrap().split('"').next().unwrap();
    let numbers: Vec<f64> = d.split_whitespace().filter_map(|token| token.parse().ok()).collect();
    Point::new(numbers[numbers.len() - 2], numbers[numbers.len() - 1])
}

/// How far `point` is from the diamond's outline, in units of its half-diagonals (0 on it).
fn off_diamond(point: Point) -> f64 {
    let [x, y, w, h] = DIAMOND;
    ((point.x - x - w / 2.0).abs() / (w / 2.0) + (point.y - y - h / 2.0).abs() / (h / 2.0) - 1.0).abs()
}

/// A rectangle at (100, 300)-(200, 400) and a diamond at `DIAMOND`, both black, and a straight,
/// heavy black connector from the rectangle's right middle to the diamond's box corner `[1, 0]`.
/// Answers (document, diamond, connector).
fn board() -> (BoardDocument, String, String) {
    let mut doc = BoardDocument::create("Ends.kboard", 800, 600).unwrap();
    let [x, y, w, h] = DIAMOND;
    let made = doc
        .execute(
            "element.create",
            json!({ "elements": [
                { "kind": "shape", "position": [100, 300], "size": [100, 100], "strokeColor": "#000000", "fillStyle": "none" },
                { "kind": "shape", "shapeType": "diamond", "position": [x, y], "size": [w, h], "strokeColor": "#000000", "fillStyle": "none" },
            ] }),
        )
        .unwrap();
    let (rectangle, diamond) = (made["created"][0].as_str().unwrap().to_owned(), made["created"][1].as_str().unwrap().to_owned());
    let made = doc
        .execute(
            "connector.create",
            json!({
                "from": { "element": rectangle, "anchor": [1, 0.5] },
                "to": { "element": diamond, "anchor": [1, 0] },
                "type": "straight",
                "style": { "strokeColor": "#000000", "strokeSize": "xl" },
            }),
        )
        .unwrap();
    (doc, diamond, made["created"][0].as_str().unwrap().to_owned())
}

fn stored_end(doc: &BoardDocument, connector: &str) -> End {
    doc.model().element(&ObjectId::from_string(connector)).unwrap().end.clone().unwrap()
}

#[test]
fn an_end_at_a_diamonds_box_corner_meets_the_diamond() {
    let (doc, _, connector) = board();
    let end = route_end(&fragment(&doc, &connector));
    assert!(off_diamond(end) < 1e-3, "the end {end:?} is not on the diamond");
    assert!((end.x - 475.0).abs() < 0.01 && (end.y - 125.0).abs() < 0.01, "{end:?}");
    assert!(!dark(&doc, 500.0, 100.0), "the box corner is paper: the line stops at the diamond");
    assert!(dark(&doc, 337.5, 237.5), "the line is drawn between the two");
    // The cache a command writes is the derived end, so raw readers see it too.
    let stored = stored_end(&doc, &connector).point;
    assert!((stored.x - end.x).abs() < 0.01 && (stored.y - end.y).abs() < 0.01, "{stored:?}");
}

#[test]
fn the_end_is_the_same_after_save_and_reopen() {
    let (doc, _, connector) = board();
    let before = fragment(&doc, &connector);
    let bytes = doc.save(None).unwrap();
    let reopened = BoardDocument::open("Ends.kboard", &bytes).unwrap();
    assert_eq!(fragment(&reopened, &connector), before);
}

#[test]
fn a_stored_point_on_the_box_is_drawn_on_the_outline_and_never_repaired() {
    let (mut doc, diamond, connector) = board();
    // As a board written before ends met outlines has it: the cached point on the box corner.
    let older = json!({ "point": [500, 100], "binding": { "target": diamond, "anchor": [1, 0] } });
    let id = ObjectId::from_string(&connector);
    doc.session_mut().record(&Origin::system("fixture"), |tx| Ok(tx.set(&id, "end", older)?)).unwrap();
    let first = doc.save(None).unwrap();
    let opened = BoardDocument::open("Ends.kboard", &first).unwrap();
    assert_eq!(stored_end(&opened, &connector).point, Point::new(500.0, 100.0), "the file keeps its point");
    let end = route_end(&fragment(&opened, &connector));
    assert!(off_diamond(end) < 1e-3, "the end {end:?} is drawn on the diamond");
    assert!(!dark(&opened, 500.0, 100.0));
    // Derived, not repaired (design note 1.4): a save without edits appends nothing.
    let again = opened.save(None).unwrap();
    assert_eq!(kboard::read(&again).unwrap().changes, kboard::read(&first).unwrap().changes, "the save appended a change");
    assert_eq!(stored_end(&opened, &connector).point, Point::new(500.0, 100.0));
}

#[test]
fn a_new_shape_type_moves_the_cached_end_onto_the_new_outline_and_rectangles_stay_on_the_box() {
    let mut doc = BoardDocument::create("Ends.kboard", 800, 600).unwrap();
    let made = doc
        .execute(
            "element.create",
            json!({ "elements": [
                { "kind": "shape", "position": [100, 300], "size": [100, 100] },
                { "kind": "shape", "position": [400, 100], "size": [100, 100] },
            ] }),
        )
        .unwrap();
    let (a, b) = (made["created"][0].as_str().unwrap().to_owned(), made["created"][1].as_str().unwrap().to_owned());
    let made =
        doc.execute("connector.create", json!({ "from": { "element": a, "anchor": [1, 0.5] }, "to": { "element": b, "anchor": [1, 0] } })).unwrap();
    let connector = made["created"][0].as_str().unwrap().to_owned();
    assert_eq!(stored_end(&doc, &connector).point, Point::new(500.0, 100.0), "a rectangle's end is its box point, exactly");
    doc.execute("element.update", json!({ "id": b, "set": { "shapeType": "diamond" } })).unwrap();
    let cached = stored_end(&doc, &connector).point;
    assert!(off_diamond(cached) < 1e-9, "{cached:?}");
    let derived = doc.model().end_point(&ObjectId::from_string(&connector), &stored_end(&doc, &connector));
    assert_eq!(cached, derived, "the cache is the derived end");
}
