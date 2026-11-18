#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kdm::blobs::SharedBlobs;
use kition_board_kdm::{BoardModel, create_replica};
use kition_kit::kdm::{ActorId, ObjectId, Origin};
use kition_kit::session::KitSession;
use serde_json::{Value, json};

use crate::fragment::{Lod, Options, Pictures, fragment};
use crate::raster::{Raster, area};
use crate::style::Colors;
use crate::{Revisions, raster};

/// A board driven through the engine's commands.
struct Board {
    session: KitSession<BoardModel>,
    blobs: SharedBlobs,
}

impl Board {
    fn new() -> Self {
        let replica = create_replica("Plan", None, ActorId::random(), &Origin::system("create")).unwrap();
        let blobs = SharedBlobs::default();
        let session = KitSession::from_replica(replica, Box::new(blobs.clone())).unwrap();
        Self { session, blobs }
    }

    fn run(&mut self, command: &str, params: Value) -> Value {
        let mut blobs = self.blobs.clone();
        let page = self.session.model().pages()[0].id.clone();
        self.session
            .execute(&Origin::person("person:test"), |model, tx| {
                let mut ctx = kition_board_model::Ctx { blobs: &mut blobs, doc_id: "doc", active_page: Some(&page) };
                kition_board_model::run(model, tx, &mut ctx, command, &params)
            })
            .unwrap()
            .0
    }

    /// Creates one element and answers its id.
    fn create(&mut self, element: Value) -> String {
        let answer = self.run("element.create", json!({ "elements": [element] }));
        answer["created"][0].as_str().unwrap().to_owned()
    }

    fn model(&self) -> &BoardModel {
        self.session.model()
    }

    fn fragment(&self, id: &str, lod: Lod, pictures: Pictures) -> crate::Fragment {
        let element = self.model().element(&ObjectId::from_string(id)).unwrap();
        fragment(self.model(), element, &Options { colors: Colors::Css, lod, pictures, blobs: &self.blobs })
    }

    fn svg(&self, id: &str) -> String {
        self.fragment(id, Lod::Full, Pictures::Reference).svg
    }
}

/// A 2 × 2 PNG, all red.
const RED_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEklEQVR4nGP4z8DAwMDAwMAAAA0EAf9n8uLhAAAAAElFTkSuQmCC";

#[test]
fn every_shape_type_draws_its_outline_and_label() {
    let mut board = Board::new();
    for shape in kition_board_model::fields::SHAPE_TYPES {
        let id = board.create(json!({ "kind": "shape", "shapeType": shape, "position": [0, 0], "size": [160, 100], "text": "Plan" }));
        let piece = board.fragment(&id, Lod::Full, Pictures::Reference);
        assert!(piece.local, "{shape} is drawn in its local frame");
        assert!(piece.svg.contains("stroke=\"var(--kb-stroke-ink, #1e1e1e)\""), "{shape}: {}", piece.svg);
        let labelled = !matches!(*shape, "line" | "check");
        assert_eq!(piece.svg.contains("<tspan"), labelled, "{shape} label");
        assert!(piece.bounds.x0 <= 0.0 && piece.bounds.x1 >= 160.0, "{shape} bounds {:?}", piece.bounds);
    }
}

#[test]
fn labels_wrap_inside_the_shape_and_centre_each_line() {
    let mut board = Board::new();
    let id = board.create(
        json!({ "kind": "shape", "position": [10, 20], "size": [120, 200], "text": "one two three four five six seven", "fontFamily": "mono" }),
    );
    let svg = board.svg(&id);
    let lines = svg.matches("<tspan").count();
    assert!(lines >= 3, "wrapped to the shape's width: {svg}");
    assert!(svg.contains("font-kerning:none;font-variant-ligatures:none"), "the browser draws the measured advances");
    assert!(svg.contains("font-family=\"monospace\""));
    // mono is 0.6 em: "one two" is 7 × 9.6 = 67.2 wide, centred in 120.
    assert!(svg.contains("x=\"26.4\""), "{svg}");
}

#[test]
fn styles_fill_dash_and_opacity() {
    let mut board = Board::new();
    let id = board.create(json!({ "kind": "shape", "position": [0, 0], "size": [100, 100], "fillColor": "purple", "fillStyle": "pattern", "dashStyle": "dashed", "opacity": 0.5, "strokeColor": "#336699" }));
    let svg = board.svg(&id);
    assert!(svg.contains("<pattern id=\"kb-hatch-"), "{svg}");
    assert!(svg.contains("stroke-dasharray=\"8 5.5\""));
    assert!(svg.starts_with("<g opacity=\"0.5\">"));
    assert!(svg.contains("stroke=\"#336699\""));
    let semi = board.create(json!({ "kind": "shape", "position": [0, 0], "size": [100, 100], "fillStyle": "semi" }));
    assert!(board.svg(&semi).contains("fill-opacity=\"0.48\""));
    let none = board.create(json!({ "kind": "shape", "position": [0, 0], "size": [100, 100], "fillStyle": "none" }));
    assert!(board.svg(&none).contains("fill=\"none\""));
    let sticky = board.create(json!({ "kind": "shape", "shapeStyle": "sticky", "position": [0, 0], "size": [180, 140] }));
    assert!(board.svg(&sticky).contains("var(--kb-fill-yellow"));
}

#[test]
fn text_frames_cards_and_pictures() {
    let mut board = Board::new();
    let text = board.create(json!({ "kind": "text", "position": [0, 0], "text": "Hello\nworld" }));
    let svg = board.svg(&text);
    assert_eq!(svg.matches("<tspan").count(), 2);
    assert!(svg.contains("fill=\"transparent\""), "the whole text box takes clicks");

    let frame = board.create(json!({ "kind": "frame", "position": [0, 0], "size": [400, 300], "text": "Sprint" }));
    let piece = board.fragment(&frame, Lod::Full, Pictures::Reference);
    assert!(piece.svg.contains(">Sprint</tspan>"));
    assert!(piece.bounds.y0 < -10.0, "the title sits above the frame: {:?}", piece.bounds);

    let card = board.run("card.create", json!({ "file": "notes/Plan.ktable", "position": [0, 0], "size": [320, 200], "caption": "Budget" }));
    let card = card["created"][0].as_str().unwrap().to_owned();
    let piece = board.fragment(&card, Lod::Full, Pictures::Reference);
    assert!(piece.svg.contains(">Plan.ktable</tspan>") && piece.svg.contains(">notes/Plan.ktable</tspan>"), "{}", piece.svg);
    assert!(piece.svg.contains(">Budget</tspan>"), "the caption is drawn under the card");
    assert_eq!(piece.slot.map(|slot| (slot.x0, slot.y0)), Some((12.0, 56.0)), "a table card has a preview slot");
    let link = board.run("card.create", json!({ "url": "https://kition.ai/docs", "position": [0, 0] }));
    let link = board.svg(link["created"][0].as_str().unwrap());
    assert!(link.contains(">kition.ai</tspan>"), "{link}");

    let picture = board.run("image.insert", json!({ "data": RED_PNG, "mime": "image/png", "position": [0, 0], "size": [40, 40], "alt": "red" }));
    let picture = picture["created"][0].as_str().unwrap().to_owned();
    let reference = board.svg(&picture);
    assert!(reference.contains("data-blob=\"") && !reference.contains("base64"), "{reference}");
    let inline = board.fragment(&picture, Lod::Full, Pictures::Inline).svg;
    assert!(inline.contains("href=\"data:image/png;base64,iVBOR"));
}

#[test]
fn strokes_and_connectors_are_in_world_coordinates() {
    let mut board = Board::new();
    let stroke = board.run("stroke.create", json!({ "points": [[0, 0], [10, 0], [20, 10]] }));
    let piece = board.fragment(stroke["created"][0].as_str().unwrap(), Lod::Full, Pictures::Reference);
    assert!(!piece.local && piece.svg.contains("d=\"M 0 0 Q 10 0 15 5 L 20 10\""), "{}", piece.svg);

    let a = board.create(json!({ "kind": "shape", "position": [0, 0], "size": [100, 100] }));
    let b = board.create(json!({ "kind": "shape", "position": [300, 0], "size": [100, 100] }));
    let link = board.run(
        "connector.create",
        json!({ "from": { "element": a, "anchor": [1, 0.5] }, "to": { "element": b, "anchor": [0, 0.5] }, "label": "next", "startArrowhead": "dot" }),
    );
    let link = link["created"][0].as_str().unwrap().to_owned();
    let piece = board.fragment(&link, Lod::Full, Pictures::Reference);
    assert!(!piece.local);
    assert!(piece.svg.contains("class=\"kb-hit\""));
    assert!(piece.svg.contains("<path d=\"M 300 50 L"), "the arrowhead's tip is on the target: {}", piece.svg);
    assert!(piece.svg.contains("<circle cx=\"100\" cy=\"50\""), "a dot at the start");
    assert!(piece.svg.contains(">next</tspan>"));
    assert!(piece.bounds.x0 <= 100.0 && piece.bounds.x1 >= 300.0);
}

#[test]
fn low_detail_draws_bars_instead_of_glyphs() {
    let mut board = Board::new();
    let id = board.create(json!({ "kind": "shape", "position": [0, 0], "size": [160, 100], "text": "Plan" }));
    let low = board.fragment(&id, Lod::Low, Pictures::Reference).svg;
    assert!(!low.contains("<text") && low.contains("fill-opacity=\"0.35\""), "{low}");
}

#[test]
fn revisions_move_with_the_look_not_the_place() {
    let mut board = Board::new();
    let a = board.create(json!({ "kind": "shape", "position": [0, 0], "size": [100, 100] }));
    let b = board.create(json!({ "kind": "shape", "position": [300, 0], "size": [100, 100] }));
    let link = board.run("connector.create", json!({ "from": { "element": a }, "to": { "element": b } }))["created"][0].as_str().unwrap().to_owned();
    let mut revisions = Revisions::default();
    let ask = |board: &Board, revisions: &mut Revisions, known: Value| {
        revisions.answer(board.model(), &json!({ "ids": [a, b, link, "nope"], "known": known }), &board.blobs).unwrap()
    };
    let first = ask(&board, &mut revisions, json!({}));
    assert_eq!(first["fragments"].as_array().unwrap().len(), 3);
    assert_eq!(first["missing"], json!(["nope"]));
    let rev = |answer: &Value, id: &str| answer["fragments"].as_array().unwrap().iter().find(|f| f["id"] == id).map(|f| f["rev"].as_u64().unwrap());
    let known = json!({ a.clone(): rev(&first, &a), b.clone(): rev(&first, &b), link.clone(): rev(&first, &link) });
    let again = ask(&board, &mut revisions, known.clone());
    assert_eq!(again["fragments"], json!([]), "nothing changed");
    assert_eq!(again["unchanged"].as_array().unwrap().len(), 3);

    board.run("element.move", json!({ "ids": [a], "delta": [0, 40] }));
    let moved = ask(&board, &mut revisions, known.clone());
    let changed: Vec<&str> = moved["fragments"].as_array().unwrap().iter().map(|f| f["id"].as_str().unwrap()).collect();
    assert_eq!(changed, [link.as_str()], "moving a shape keeps its fragment; its connector's route changes");

    board.run("element.style", json!({ "ids": [b], "style": { "fillColor": "green" } }));
    let styled = ask(&board, &mut revisions, known);
    assert!(styled["fragments"].as_array().unwrap().iter().any(|f| f["id"] == b.as_str()));
    assert!(revisions.answer(board.model(), &json!({ "ids": vec!["x"; 501] }), &board.blobs).is_err());
    assert!(revisions.answer(board.model(), &json!({ "ids": [a], "lod": "tiny" }), &board.blobs).is_err());
    assert!(revisions.answer(board.model(), &json!({}), &board.blobs).is_err());
}

fn pixel(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    [rgba[at], rgba[at + 1], rgba[at + 2], rgba[at + 3]]
}

#[test]
fn the_raster_draws_the_same_scene() {
    let mut board = Board::new();
    board.create(json!({ "kind": "shape", "position": [100, 100], "size": [200, 100], "fillColor": "purple" }));
    board.create(json!({ "kind": "shape", "position": [400, 100], "size": [100, 100], "fillColor": "#ff0000", "rotation": 45 }));
    let page = board.model().pages()[0].clone();
    let frame = area(board.model(), Some(&page));
    assert_eq!((frame.x, frame.y, frame.unit, frame.width, frame.height), (60.0, 39.0, 1.0, 1920, 1080));
    let raster = Raster::build(board.model(), Some(&page), &board.blobs).unwrap();
    let full = raster.render(0.0, 0.0, 1.0, frame.width, frame.height).unwrap();
    assert_eq!(full.len(), 1920 * 1080 * 4);
    assert_eq!(pixel(&full, 1920, 140, 90), [0xec, 0xe9, 0xfb, 255], "the purple fill inside the first shape");
    assert_eq!(pixel(&full, 1920, 5, 5), [255, 255, 255, 255], "paper outside");
    assert_eq!(pixel(&full, 1920, 390, 90), [255, 0, 0, 255], "the rotated square's centre");
    assert_eq!(pixel(&full, 1920, 343, 43), [255, 255, 255, 255], "its unrotated corner is empty");
    // render_area at 1/2 is the box mean of the 1:1 pixels.
    let half = raster.render_area(0, 0, 400, 200, 200, 100).unwrap();
    for (x, y) in [(70, 45), (40, 20), (0, 0), (199, 99)] {
        for channel in 0..4 {
            let mut sum = 0u32;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                sum += u32::from(pixel(&full, 1920, x * 2 + dx, y * 2 + dy)[channel]);
            }
            let mean = ((f64::from(sum) / 4.0).round()) as u8;
            let got = pixel(&half, 200, x, y)[channel];
            assert!(got.abs_diff(mean) <= 1, "({x}, {y}) channel {channel}: {got} vs {mean}");
        }
    }
    let small = raster.render(0.0, 0.0, 0.1, 192, 108).unwrap();
    assert_eq!(small.len(), 192 * 108 * 4);
}

#[test]
fn the_frame_follows_the_content_and_scales_huge_boards() {
    let mut board = Board::new();
    let page = board.model().pages()[0].clone();
    assert_eq!(area(board.model(), Some(&page)).width, 1920, "an empty page is one view");
    board.create(json!({ "kind": "shape", "position": [-5000, 0], "size": [100, 100] }));
    board.create(json!({ "kind": "shape", "position": [20000, 0], "size": [100, 100] }));
    let page = board.model().pages()[0].clone();
    let frame = area(board.model(), Some(&page));
    assert_eq!(frame.x, -5040.0);
    assert_eq!(frame.unit, 4.0, "25,180 units over 8,192 pixels");
    assert!(frame.width <= kition_kit::MAX_RENDER_SIDE);
    let svg = raster::page_svg(board.model(), &page, &frame, &board.blobs);
    assert!(svg.contains("viewBox=\"-5040 -40"), "{}", &svg[..200]);
}
