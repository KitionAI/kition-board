//! `file.render`, the agent's preview (`agent.render` in kit.json): a picture of a named page, frame,
//! element or area, a function of the board and the call alone. It never reads or moves the page
//! the editor shows, and the same call answers the same bytes (task card C4/C5/C6-board).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use base64::Engine as _;
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use serde_json::{Value, json};

/// `Plan` (first: two shapes and a frame) and `Ideas` (one ellipse), with `Ideas` shown.
struct Fixture {
    doc: BoardDocument,
    plan: String,
    ideas: String,
    frame: String,
    shape: String,
}

fn fixture() -> Fixture {
    let mut doc = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    let plan = doc.model().pages()[0].id.as_str().to_owned();
    doc.execute("page.rename", json!({ "page": plan, "name": "Plan" })).unwrap();
    let made = doc
        .execute(
            "element.create",
            json!({ "elements": [
                { "kind": "shape", "position": [40, 40], "size": [160, 100], "text": "Idea", "fillColor": "purple" },
                { "kind": "shape", "position": [300, 200], "size": [100, 100], "shapeType": "ellipse" },
            ] }),
        )
        .unwrap();
    let shape = made["created"][0].as_str().unwrap().to_owned();
    let frame = doc.execute("frame.create", json!({ "box": [600, 40, 900, 240], "title": "Later" })).unwrap()["frame"].as_str().unwrap().to_owned();
    let ideas = doc.execute("page.create", json!({ "name": "Ideas", "activate": true })).unwrap()["page"].as_str().unwrap().to_owned();
    doc.execute(
        "element.create",
        json!({ "elements": [{ "kind": "shape", "position": [100, 100], "size": [400, 300], "shapeType": "ellipse", "fillColor": "green" }] }),
    )
    .unwrap();
    Fixture { doc, plan, ideas, frame, shape }
}

fn render(doc: &mut BoardDocument, params: Value) -> Value {
    doc.execute("file.render", params.clone()).unwrap_or_else(|error| panic!("file.render {params}: {error}"))
}

fn png(answer: &Value) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD.decode(answer["png"].as_str().unwrap()).unwrap()
}

fn size(answer: &Value) -> (u64, u64) {
    (answer["width"].as_u64().unwrap(), answer["height"].as_u64().unwrap())
}

#[test]
fn it_answers_a_png_of_the_named_page_with_what_it_drew() {
    let Fixture { mut doc, plan, .. } = fixture();
    let answer = render(&mut doc, json!({ "target": { "page": plan } }));
    let bytes = png(&answer);
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "{answer}");
    let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).unwrap();
    assert_eq!((u64::from(decoded.width()), u64::from(decoded.height())), size(&answer));
    assert_eq!(answer["mimeType"], "image/png");
    assert_eq!((answer["page"].clone(), answer["pageName"].clone()), (json!(plan), json!("Plan")));
    // The page's document area (the 1920 × 1080 board from the origin) at scale 1.
    assert_eq!(answer["area"], json!([0.0, 0.0, 1920.0, 1080.0]), "{answer}");
    assert_eq!(answer["scale"], json!(1.0));
    assert_eq!(size(&answer), (1920, 1080));
}

#[test]
fn it_never_reads_or_moves_the_shown_page_and_answers_the_same_bytes() {
    let Fixture { mut doc, plan, ideas, .. } = fixture();
    // Ideas is shown; the call names Plan.
    let call = json!({ "target": { "page": plan }, "scale": 0.5 });
    let first = render(&mut doc, call.clone());
    let (revision, dirty, state, inspected) = (doc.revision(), doc.dirty(), doc.editor_state(), doc.inspect().unwrap());
    assert_eq!(render(&mut doc, call.clone()), first, "the same call answers the same bytes");
    assert_eq!((doc.revision(), doc.dirty(), doc.editor_state()), (revision, dirty, state), "rendering changed the board or the editor");
    assert_eq!(doc.inspect().unwrap(), inspected, "rendering changed what inspect answers");
    // Another shown page draws the same picture.
    doc.execute("page.activate", json!({ "page": plan })).unwrap();
    assert_eq!(render(&mut doc, call.clone()), first, "the shown page changed the picture");
    doc.execute("page.activate", json!({ "page": ideas })).unwrap();
    // The saved board, reopened, draws the same bytes.
    let mut reopened = BoardDocument::open("Plan.kboard", &doc.save(None).unwrap()).unwrap();
    assert_eq!(render(&mut reopened, call), first, "a reopened board draws the same bytes");
}

#[test]
fn another_page_frame_element_or_area_is_another_picture() {
    let Fixture { mut doc, plan, ideas, frame, shape } = fixture();
    let page = render(&mut doc, json!({ "target": { "page": plan } }));
    let other = render(&mut doc, json!({ "target": { "page": ideas } }));
    assert_ne!(page["png"], other["png"], "two pages, two pictures");
    // Without a target: the first page by the board's order, never the shown one (Ideas).
    let default = render(&mut doc, json!({}));
    assert_eq!((default["page"].clone(), default["png"].clone()), (json!(plan), page["png"].clone()), "Ideas is shown, Plan is first");
    // A frame: its box, on its page.
    let framed = render(&mut doc, json!({ "target": { "frame": frame } }));
    assert_eq!(
        (framed["page"].clone(), framed["frame"].clone(), framed["area"].clone()),
        (json!(plan), json!(frame), json!([600.0, 40.0, 900.0, 240.0]))
    );
    assert_eq!(size(&framed), (300, 200));
    // An element: its bounds with a margin of 16 units.
    let element = render(&mut doc, json!({ "target": { "objectId": shape } }));
    assert_eq!((element["objectId"].clone(), element["area"].clone()), (json!(shape), json!([24.0, 24.0, 216.0, 156.0])), "{element}");
    // An area of a page, in board units.
    let area = render(&mut doc, json!({ "target": { "page": ideas, "area": [0, 0, 600, 500] } }));
    assert_eq!((area["page"].clone(), size(&area)), (json!(ideas), (600, 500)));
    assert_ne!(area["png"], render(&mut doc, json!({ "target": { "page": plan, "area": [0, 0, 600, 500] } }))["png"]);
    // A page by its number counted from 1 (as diagnostics give it) is that page.
    assert_eq!(render(&mut doc, json!({ "target": { "page": 2 } }))["png"], other["png"]);
    // A diagnostic's target names the page and the element: they must agree.
    assert_eq!(render(&mut doc, json!({ "target": { "page": 1, "objectId": shape } }))["png"], element["png"]);
}

#[test]
fn scale_and_max_size_set_the_pictures_size() {
    let Fixture { mut doc, frame, .. } = fixture();
    let one = render(&mut doc, json!({ "target": { "frame": frame } }));
    let two = render(&mut doc, json!({ "target": { "frame": frame }, "scale": 2 }));
    assert_eq!((size(&one), size(&two)), ((300, 200), (600, 400)));
    let small = render(&mut doc, json!({ "target": { "frame": frame }, "scale": 2, "maxSize": 64 }));
    let (w, h) = size(&small);
    assert!(w.max(h) <= 64, "{small}");
    assert!(small["scale"].as_f64().unwrap() < 1.0, "{small}");
    let half = render(&mut doc, json!({ "scale": 0.25 }));
    assert_eq!(size(&half), (480, 270));
}

#[test]
fn what_it_cannot_draw_is_refused_saying_what_it_takes() {
    let Fixture { mut doc, plan, ideas, frame, shape } = fixture();
    let before = (doc.revision(), doc.editor_state());
    for (params, code, says) in [
        (json!({ "target": { "page": "nowhere" } }), "not-found", "no page `nowhere`"),
        (json!({ "target": { "page": 0 } }), "invalid-params", "counted from 1"),
        (json!({ "target": { "page": 3 } }), "not-found", "the board has 2 pages"),
        (json!({ "target": { "frame": shape } }), "invalid-params", "is a shape, not a frame"),
        (json!({ "target": { "frame": frame, "page": ideas } }), "invalid-params", "is on page `Plan`"),
        (json!({ "target": { "objectId": "nothing" } }), "not-found", "no element `nothing`"),
        (json!({ "target": { "frame": frame, "objectId": shape } }), "invalid-params", "one of `frame`, `objectId` and `area`"),
        (json!({ "target": { "page": plan, "area": [0, 0, 0, 10] } }), "invalid-params", "`target.area`"),
        (json!({ "target": { "page": plan, "area": [0, 0] } }), "invalid-params", "`target.area`"),
        (json!({ "target": { "slide": 1 } }), "invalid-params", "`target.slide`"),
        (json!({ "target": "Plan" }), "invalid-params", "`target`"),
        (json!({ "scale": 0 }), "invalid-params", "`scale`"),
        (json!({ "scale": 5 }), "invalid-params", "`scale`"),
        (json!({ "maxSize": 8 }), "invalid-params", "`maxSize`"),
    ] {
        let error = doc.execute("file.render", params.clone()).unwrap_err();
        assert!(error.starts_with(code) && error.contains(says), "{params}: {error}");
    }
    assert_eq!((doc.revision(), doc.editor_state()), before, "a refused render changed nothing");
}
