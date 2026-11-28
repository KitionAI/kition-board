//! Fixed-width and auto-width text (plan "Board usable by hand", stage 1; cases
//! `board.text/fixed-width` and `board.text/text-frame`): a text with a stored size wraps at its
//! width and its box is never shorter than its lines (a bound end follows the box as it grows); a
//! text without one is one line per paragraph. Every fragment that holds text says where the
//! text is laid out (`text`: area, font, line height, alignment, wrap), so the editor's text box
//! sits where the lines are drawn.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kdm::{End, Point};
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::ObjectId;
use kition_kit::session::KitSessionDocument;
use serde_json::{Value, json};

fn board() -> BoardDocument {
    BoardDocument::create("Text.kboard", 1920, 1080).unwrap()
}

fn run(doc: &mut BoardDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("{command} {params}: {error}"))
}

fn created(answer: &Value) -> Vec<String> {
    answer["created"].as_array().unwrap().iter().map(|id| id.as_str().unwrap().to_owned()).collect()
}

fn fragment(doc: &BoardDocument, id: &str) -> Value {
    let answer: Value = serde_json::from_str(&doc.element_svg(&json!({ "ids": [id] }).to_string()).unwrap()).unwrap();
    answer["fragments"][0].clone()
}

fn lines(fragment: &Value) -> usize {
    fragment["svg"].as_str().unwrap().matches("<tspan").count()
}

const WORDS: &str = "a fixed width text wraps at its width and grows downward with its lines";

#[test]
fn a_fixed_width_text_wraps_and_its_box_holds_its_lines() {
    let mut doc = board();
    let ids = created(&run(
        &mut doc,
        "element.add",
        json!({ "elements": [
            { "kind": "text", "position": [0, 0], "size": [160, 25], "text": WORDS },
            { "kind": "text", "position": [0, 400], "text": WORDS },
            { "kind": "text", "position": [400, 0], "size": [300, 200], "text": "short" },
        ] }),
    ));
    let fixed = fragment(&doc, &ids[0]);
    let count = lines(&fixed);
    assert!(count >= 3, "the fixed-width text wraps: {fixed}");
    assert_eq!(fixed["size"][0], 160.0, "its width is the stored one");
    assert_eq!(fixed["size"][1].as_f64().unwrap(), count as f64 * 25.0, "its box is as tall as its lines: {fixed}");
    let text = &fixed["text"];
    assert_eq!(text["wrap"], true);
    assert_eq!(text["area"], json!([0.0, 0.0, 160.0, count as f64 * 25.0]));
    assert_eq!(text["size"], 20.0);
    assert_eq!(text["lineHeight"], 25.0);
    assert_eq!(text["valign"], "top");
    assert_eq!(text["align"], "left");

    let auto = fragment(&doc, &ids[1]);
    assert_eq!(lines(&auto), 1, "a text without a size is one line: {auto}");
    assert_eq!(auto["text"]["wrap"], false);
    assert!(auto["size"][0].as_f64().unwrap() > 160.0);

    // A stored height taller than the lines stays: the box is the taller of the two.
    let tall = fragment(&doc, &ids[2]);
    assert_eq!(tall["size"], json!([300.0, 200.0]));
}

#[test]
fn a_bound_end_follows_a_fixed_width_text_as_it_grows() {
    let mut doc = board();
    let text =
        created(&run(&mut doc, "element.add", json!({ "elements": [{ "kind": "text", "position": [0, 0], "size": [160, 25], "text": "One" }] })))[0]
            .clone();
    let connector =
        created(&run(&mut doc, "connector.add", json!({ "from": { "element": text, "anchor": [0.5, 1] }, "to": { "point": [80, 800] } })))[0].clone();
    let start = |doc: &BoardDocument| {
        let stored = End::parse(&doc.session().replica().get(&ObjectId::from_string(&connector), "start").unwrap()).unwrap().point;
        let element = doc.model().element(&ObjectId::from_string(&connector)).unwrap().clone();
        (stored, doc.model().end_point(&element.id, element.start.as_ref().unwrap()))
    };
    assert_eq!(start(&doc).1, Point::new(80.0, 25.0));
    run(&mut doc, "element.setText", json!({ "id": text, "text": WORDS }));
    let (stored, derived) = start(&doc);
    let height = fragment(&doc, &text)["size"][1].as_f64().unwrap();
    assert!(height > 50.0, "the text grew: {height}");
    assert_eq!(derived, Point::new(80.0, height), "the end is on the grown box");
    assert_eq!(stored, derived, "and its stored point follows");
}

#[test]
fn fragments_say_where_their_text_is_laid_out() {
    let mut doc = board();
    let ids = created(&run(
        &mut doc,
        "element.add",
        json!({ "elements": [
            { "kind": "shape", "position": [0, 0], "size": [200, 100], "text": "Idea", "fontSize": 24 },
            { "kind": "shape", "position": [300, 0], "size": [200, 100], "shapeStyle": "sticky" },
            { "kind": "text", "position": [0, 300], "text": "Notes", "textAlign": "center" },
        ] }),
    ));
    let frame = run(&mut doc, "frame.add", json!({ "box": [600, 0, 900, 200], "title": "Plan" }));
    let shape = fragment(&doc, &ids[0])["text"].clone();
    // Inside the label box's padding (12 on each side), centred top to bottom.
    assert_eq!(shape["area"], json!([12.0, 0.0, 188.0, 100.0]), "{shape}");
    assert_eq!(shape["valign"], "middle");
    assert_eq!(shape["align"], "center");
    assert_eq!(shape["wrap"], true);
    assert_eq!(shape["size"], 24.0);
    assert_eq!(shape["lineHeight"], 30.0);
    assert!(shape["family"].as_str().unwrap().ends_with("sans-serif"));
    assert!(shape["color"].as_str().is_some_and(|color| !color.is_empty()));
    // An empty sticky note has a frame too: the editor opens on it before anything is written.
    let note = fragment(&doc, &ids[1])["text"].clone();
    assert_eq!(note["valign"], "middle", "{note}");
    assert_eq!(fragment(&doc, &ids[2])["text"]["align"], "center");
    let frame_id = frame["frame"].as_str().unwrap();
    let title = fragment(&doc, frame_id)["text"].clone();
    assert!(title["area"][3].as_f64().unwrap() <= 0.0, "a frame's title is above it: {title}");
    // A connector's label: centred on the middle of its route, wrapping at 200, in world coordinates.
    let connector =
        created(&run(&mut doc, "connector.add", json!({ "from": { "point": [0, 600] }, "to": { "point": [400, 600] }, "label": "Yes" })))[0].clone();
    let label = fragment(&doc, &connector);
    assert_eq!(label["frame"], "world");
    assert_eq!(label["text"]["area"], json!([100.0, 591.25, 300.0, 608.75]), "{label}");
    assert_eq!(label["text"]["valign"], "middle");
    // A new font size is a new frame, even with nothing drawn: the revision moves.
    let before = fragment(&doc, &ids[1])["rev"].clone();
    run(&mut doc, "element.style", json!({ "ids": [ids[1]], "style": { "fontSize": 40 } }));
    let after = fragment(&doc, &ids[1]);
    assert_ne!(after["rev"], before);
    assert_eq!(after["text"]["size"], 40.0);
}

#[test]
fn text_metrics_name_the_family_and_line_height() {
    let doc = board();
    let metrics: Value = serde_json::from_str(&doc.text_metrics(&json!({ "texts": [{ "text": "", "size": 20 }] }).to_string()).unwrap()).unwrap();
    let result = &metrics["results"][0];
    assert_eq!(result["lineHeight"], 25.0);
    assert!(result["css"].as_str().unwrap().ends_with("sans-serif"));
}
