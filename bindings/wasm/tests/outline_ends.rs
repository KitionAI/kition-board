//! `board.connectors.ends`: a bound connector end meets its target's outline, not its box
//! (design note 1.4; case `board.connectors/outline-ends`). The end is derived, never repaired: a
//! board whose stored point is still on the box draws on the outline and saves nothing new.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

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
            "element.add",
            json!({ "elements": [
                { "kind": "shape", "position": [100, 300], "size": [100, 100], "strokeColor": "#000000", "fillStyle": "none" },
                { "kind": "shape", "shapeType": "diamond", "position": [x, y], "size": [w, h], "strokeColor": "#000000", "fillStyle": "none" },
            ] }),
        )
        .unwrap();
    let (rectangle, diamond) = (made["created"][0].as_str().unwrap().to_owned(), made["created"][1].as_str().unwrap().to_owned());
    let made = doc
        .execute(
            "connector.add",
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
    // Derived, not repaired (design note 1.4): a save without edits writes the same bytes.
    let again = opened.save(None).unwrap();
    assert_eq!(again, first, "the save wrote a change");
    assert_eq!(stored_end(&opened, &connector).point, Point::new(500.0, 100.0));
}

#[test]
fn a_new_shape_type_moves_the_cached_end_onto_the_new_outline_and_rectangles_stay_on_the_box() {
    let mut doc = BoardDocument::create("Ends.kboard", 800, 600).unwrap();
    let made = doc
        .execute(
            "element.add",
            json!({ "elements": [
                { "kind": "shape", "position": [100, 300], "size": [100, 100] },
                { "kind": "shape", "position": [400, 100], "size": [100, 100] },
            ] }),
        )
        .unwrap();
    let (a, b) = (made["created"][0].as_str().unwrap().to_owned(), made["created"][1].as_str().unwrap().to_owned());
    let made =
        doc.execute("connector.add", json!({ "from": { "element": a, "anchor": [1, 0.5] }, "to": { "element": b, "anchor": [1, 0] } })).unwrap();
    let connector = made["created"][0].as_str().unwrap().to_owned();
    assert_eq!(stored_end(&doc, &connector).point, Point::new(500.0, 100.0), "a rectangle's end is its box point, exactly");
    doc.execute("element.set", json!({ "id": b, "set": { "shapeType": "diamond" } })).unwrap();
    let cached = stored_end(&doc, &connector).point;
    assert!(off_diamond(cached) < 1e-9, "{cached:?}");
    let derived = doc.model().end_point(&ObjectId::from_string(&connector), &stored_end(&doc, &connector));
    assert_eq!(cached, derived, "the cache is the derived end");
}

/// A black, unfilled `shape` over `[x, y, w, h]` and, for each anchor of `ends`, a black
/// rectangle at `from` (its top-left; 40 × 40) with a straight, heavy black connector from that
/// rectangle's `from_anchor` to the shape's anchor. Answers (document, the connectors).
fn joined(shape: &str, [x, y, w, h]: [f64; 4], ends: &[([f64; 2], [f64; 2], [f64; 2])]) -> (BoardDocument, Vec<String>) {
    let mut doc = BoardDocument::create("Ends.kboard", 1000, 700).unwrap();
    let made = doc
        .execute(
            "element.add",
            json!({ "elements": [{ "kind": "shape", "shapeType": shape, "position": [x, y], "size": [w, h], "strokeColor": "#000000", "fillStyle": "none" }] }),
        )
        .unwrap();
    let target = made["created"][0].as_str().unwrap().to_owned();
    let mut connectors = Vec::new();
    for (from, from_anchor, anchor) in ends {
        let made = doc
            .execute(
                "element.add",
                json!({ "elements": [{ "kind": "shape", "position": from, "size": [40, 40], "strokeColor": "#000000", "fillStyle": "none" }] }),
            )
            .unwrap();
        let start = made["created"][0].as_str().unwrap().to_owned();
        let made = doc
            .execute(
                "connector.add",
                json!({
                    "from": { "element": start, "anchor": from_anchor },
                    "to": { "element": target, "anchor": anchor },
                    "type": "straight",
                    "style": { "strokeColor": "#000000", "strokeSize": "xl" },
                }),
            )
            .unwrap();
        connectors.push(made["created"][0].as_str().unwrap().to_owned());
    }
    (doc, connectors)
}

/// The point `back` units from `end` toward `start`.
fn toward(end: Point, start: Point, back: f64) -> Point {
    let length = (start.x - end.x).hypot(start.y - end.y);
    Point::new(end.x + (start.x - end.x) / length * back, end.y + (start.y - end.y) / length * back)
}

/// A connector bound to the bottom middle and the bottom quarter of a document ends on its drawn
/// wave, not on the box's bottom edge 15 units further down: the line reaches the wave (ink just
/// below the end, where the box's edge left paper before) and stops there (paper inside).
#[test]
fn an_end_on_a_documents_bottom_meets_the_wave() {
    // The document's box: (400, 100)-(600, 200); its wave is `Q 0.75 0.7 0.5 0.85 Q 0.25 1 0 0.85`.
    let (doc, connectors) =
        joined("document", [400.0, 100.0, 200.0, 100.0], &[([480.0, 400.0], [0.5, 0.0], [0.5, 1.0]), ([430.0, 400.0], [0.5, 0.0], [0.25, 1.0])]);
    // On the wave's left half x runs straight from 0.5 to 0 while y goes 0.85, 1 (control), 0.85.
    let wave = |x: f64| {
        let t = (500.0 - x) / 100.0;
        100.0 + 100.0 * (0.85 * (1.0 - t) * (1.0 - t) + 2.0 * (1.0 - t) * t + 0.85 * t * t)
    };
    let middle = route_end(&fragment(&doc, &connectors[0]));
    assert!((middle.x - 500.0).abs() < 0.01 && (middle.y - 185.0).abs() < 0.5, "bottom middle: {middle:?}");
    assert!(dark(&doc, 500.0, 193.0), "the line reaches the wave: the gap above the box's bottom edge is closed");
    assert!(!dark(&doc, 500.0, 172.0), "the line stops at the wave");
    let quarter = route_end(&fragment(&doc, &connectors[1]));
    assert!(quarter.x > 450.0 && quarter.x < 500.0, "bottom quarter: {quarter:?}");
    assert!((quarter.y - wave(quarter.x)).abs() < 0.5, "bottom quarter {quarter:?} is not on the wave ({})", wave(quarter.x));
    let below = toward(quarter, Point::new(450.0, 400.0), 6.0);
    assert!(dark(&doc, below.x, below.y), "ink just below the end at {below:?}");
    let inside = toward(quarter, Point::new(450.0, 400.0), -12.0);
    assert!(!dark(&doc, inside.x, inside.y), "paper inside the document at {inside:?}");
    // The cache a command writes is the derived end.
    let stored = stored_end(&doc, &connectors[1]).point;
    assert!((stored.x - quarter.x).abs() < 0.01 && (stored.y - quarter.y).abs() < 0.01, "{stored:?}");
}

/// A connector bound to the right side of a delay, off its middle, ends on the drawn round side.
#[test]
fn an_end_on_a_delays_right_side_meets_its_curve() {
    // The delay's box: (400, 100)-(600, 200); its right side is `C 1.13 0, 1.13 1, 0.6 1` from (0.6, 0).
    let (doc, connectors) = joined("delay", [400.0, 100.0, 200.0, 100.0], &[([700.0, 50.0], [0.0, 0.5], [1.0, 0.15])]);
    let end = route_end(&fragment(&doc, &connectors[0]));
    let curve = |t: f64| {
        let u = 1.0 - t;
        let fx = u * u * u * 0.6 + 3.0 * u * u * t * 1.13 + 3.0 * u * t * t * 1.13 + t * t * t * 0.6;
        let fy = 3.0 * u * t * t + t * t * t;
        Point::new(400.0 + 200.0 * fx, 100.0 + 100.0 * fy)
    };
    let off = (0..=2000).map(|step| curve(f64::from(step) / 2000.0)).map(|p| (p.x - end.x).hypot(p.y - end.y)).fold(f64::INFINITY, f64::min);
    assert!(off < 0.6, "the end {end:?} is {off} from the drawn curve");
    assert!(end.x < 595.0, "the end {end:?} is not on the box's right edge");
    let start = Point::new(700.0, 70.0);
    let beside = toward(end, start, 6.0);
    assert!(dark(&doc, beside.x, beside.y), "ink just outside the curve at {beside:?}");
    let inside = toward(end, start, -12.0);
    assert!(!dark(&doc, inside.x, inside.y), "paper inside the delay at {inside:?}");
}

/// `shape.previews` carries the curved shapes' sampled outlines as fraction lists, so the page
/// (`web/src/board/outline.ts`, which takes any list of `[fx, fy]`) derives their ends as the
/// engine does; the drawing in the same answer is still the curve.
#[test]
fn previews_carry_the_curved_outlines() {
    use kition_board_kdm::outline::CURVES;
    let mut doc = BoardDocument::create("Ends.kboard", 800, 600).unwrap();
    let answer = doc.execute("shape.previews", json!({})).unwrap();
    let shapes: Vec<Value> = answer["groups"].as_array().unwrap().iter().flat_map(|g| g["shapes"].as_array().unwrap().clone()).collect();
    for (name, list) in CURVES {
        let shape = shapes.iter().find(|s| s["shape"] == *name).unwrap();
        let corners: Vec<[f64; 2]> = list.iter().map(|(x, y)| [*x, *y]).collect();
        assert_eq!(shape.get("outline"), Some(&json!(corners)), "{name}");
        let svg = shape["svg"].as_str().unwrap();
        assert!(svg.contains("<path") && !svg.contains("<polygon"), "{name} is drawn as curves: {svg}");
    }
}
