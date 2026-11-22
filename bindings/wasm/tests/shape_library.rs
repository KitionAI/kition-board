//! `board.shapes`: every shape of `shapeType` is drawn as itself, inside its box, and survives a
//! save and a reopen (case `board.shapes/library`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kit::BoardDocument;
use kition_board_model::fields::{SHAPE_GROUPS, SHAPE_TYPES};
use kition_kit::KitDocument;
use serde_json::{Value, json};

/// Whether board point (x, y) is drawn dark.
fn dark(doc: &BoardDocument, x: f64, y: f64) -> bool {
    let px = doc.render(x, y, 1.0, 1, 1).unwrap();
    px[0] < 128 && px[1] < 128 && px[2] < 128
}

fn fragment(doc: &BoardDocument, id: &str) -> String {
    let answer: Value = serde_json::from_str(&doc.element_svg(&json!({ "ids": [id] }).to_string()).unwrap()).unwrap();
    answer["fragments"][0]["svg"].as_str().unwrap().to_owned()
}

/// A black, unfilled `shape` in the box (100, 100)-(300, 220).
fn shape(doc: &mut BoardDocument, shape: &str) -> String {
    let made = doc
        .execute(
            "element.create",
            json!({ "elements": [{ "kind": "shape", "shapeType": shape, "position": [100, 100], "size": [200, 120], "strokeColor": "#000000", "fillStyle": "none" }] }),
        )
        .unwrap();
    made["created"][0].as_str().unwrap().to_owned()
}

#[test]
fn there_are_44_shapes_and_every_one_is_in_exactly_one_group() {
    assert_eq!(SHAPE_TYPES.len(), 44);
    let grouped: Vec<&str> = SHAPE_GROUPS.iter().flat_map(|(_, shapes)| shapes.iter().copied()).collect();
    assert_eq!(grouped.len(), 44, "no shape is in two groups");
    for shape in SHAPE_TYPES {
        assert!(grouped.contains(shape), "{shape} is in no group");
    }
}

#[test]
fn every_shape_is_drawn_as_itself() {
    let mut doc = BoardDocument::create("Shapes.kboard", 800, 600).unwrap();
    let rectangle = shape(&mut doc, "rectangle");
    let plain = fragment(&doc, &rectangle);
    let mut seen = vec![("rectangle".to_owned(), plain)];
    for name in SHAPE_TYPES.iter().filter(|name| **name != "rectangle") {
        let id = shape(&mut doc, name);
        let svg = fragment(&doc, &id);
        assert!(!svg.contains("NaN") && !svg.contains("inf"), "{name}: {svg}");
        for (other, drawn) in &seen {
            assert_ne!(&svg, drawn, "{name} is drawn exactly as {other}: it has no drawing of its own");
        }
        seen.push(((*name).to_owned(), svg));
    }
}

#[test]
fn open_shapes_are_not_filled() {
    for name in ["brace-left", "brace-right", "actor"] {
        let mut doc = BoardDocument::create("Shapes.kboard", 800, 600).unwrap();
        let made = doc
            .execute(
                "element.create",
                json!({ "elements": [{ "kind": "shape", "shapeType": name, "position": [100, 100], "size": [200, 120], "fillColor": "#000000", "fillStyle": "solid" }] }),
            )
            .unwrap();
        let svg = fragment(&doc, made["created"][0].as_str().unwrap());
        assert!(!svg.contains("fill=\"#000000\""), "{name} is filled: {svg}");
    }
}

#[test]
fn no_shape_draws_outside_its_box() {
    for name in SHAPE_TYPES {
        let mut doc = BoardDocument::create("Shapes.kboard", 800, 600).unwrap();
        shape(&mut doc, name);
        // The box is (100, 100)-(300, 220); the line is 2.5 wide at most, so 6 units out is paper.
        for (x, y) in [(92.0, 160.0), (308.0, 160.0), (200.0, 92.0), (200.0, 228.0), (92.0, 92.0), (308.0, 228.0)] {
            assert!(!dark(&doc, x, y), "{name} draws at ({x}, {y}), outside its box");
        }
        let inside = (0..=40).any(|i| (0..=24).any(|j| dark(&doc, 100.0 + f64::from(i) * 5.0, 100.0 + f64::from(j) * 5.0)));
        assert!(inside, "{name} draws nothing");
    }
}

#[test]
fn degenerate_boxes_do_not_break_a_shape() {
    for name in SHAPE_TYPES {
        for size in [[0, 0], [1, 300], [300, 1]] {
            let mut doc = BoardDocument::create("Shapes.kboard", 800, 600).unwrap();
            let made = doc
                .execute(
                    "element.create",
                    json!({ "elements": [{ "kind": "shape", "shapeType": name, "position": [10, 10], "size": size, "text": "Label" }] }),
                )
                .unwrap();
            let svg = fragment(&doc, made["created"][0].as_str().unwrap());
            assert!(!svg.contains("NaN") && !svg.contains("inf"), "{name} at {size:?}: {svg}");
        }
    }
}

#[test]
fn an_unknown_shape_is_refused_and_a_known_one_survives_reopening() {
    let mut doc = BoardDocument::create("Shapes.kboard", 800, 600).unwrap();
    let refused = doc.execute("element.create", json!({ "elements": [{ "kind": "shape", "shapeType": "blob" }] }));
    assert!(format!("{refused:?}").contains("invalid-params"), "{refused:?}");
    let id = shape(&mut doc, "cylinder");
    let bytes = doc.save(None).unwrap();
    let reopened = BoardDocument::open("Shapes.kboard", &bytes).unwrap();
    assert!(fragment(&reopened, &id).contains("A "), "the reopened cylinder still has its arcs");
}

/// The baseline of the first line of a fragment's label.
fn first_baseline(svg: &str) -> f64 {
    svg.split("<tspan").nth(1).and_then(|t| t.split("y=\"").nth(1)).and_then(|t| t.split('"').next()).unwrap().parse().unwrap()
}

#[test]
fn a_callouts_label_stays_above_its_tail() {
    let mut doc = BoardDocument::create("Shapes.kboard", 800, 600).unwrap();
    let made = doc
        .execute(
            "element.create",
            json!({ "elements": [{ "kind": "shape", "shapeType": "callout", "position": [0, 0], "size": [200, 200], "text": "Hello" }] }),
        )
        .unwrap();
    let svg = fragment(&doc, made["created"][0].as_str().unwrap());
    // One line, centred in the upper three quarters (0 to 150): its baseline is near 80, where the
    // whole box would put it near 105.
    let y = first_baseline(&svg);
    assert!(y < 95.0 && y > 40.0, "the label's baseline is at {y}: {svg}");
}

#[test]
fn an_actors_label_is_under_its_figure() {
    let mut doc = BoardDocument::create("Shapes.kboard", 800, 600).unwrap();
    let made = doc
        .execute(
            "element.create",
            json!({ "elements": [{ "kind": "shape", "shapeType": "actor", "position": [0, 0], "size": [120, 200], "text": "Buyer" }] }),
        )
        .unwrap();
    let svg = fragment(&doc, made["created"][0].as_str().unwrap());
    // The figure takes the top 72% (to 144); the label's line is below it and inside the box.
    let y = first_baseline(&svg);
    assert!(y > 144.0 && y < 200.0, "the label's baseline is at {y}: {svg}");
}

/// `shape.previews` (case `board.shapes/library-panel`): the library panel's groups and pictures
/// come from the engine, one SVG per shape drawn by the renderer, in the order of `SHAPE_GROUPS`.
#[test]
fn the_engine_answers_one_preview_per_shape_in_its_group() {
    let mut doc = BoardDocument::create("Shapes.kboard", 800, 600).unwrap();
    let answer = doc.execute("shape.previews", json!({})).unwrap();
    let groups = answer["groups"].as_array().unwrap();
    assert_eq!(groups.iter().map(|g| g["id"].as_str().unwrap()).collect::<Vec<_>>(), ["basic", "flowchart", "arrows", "marks"]);
    assert_eq!(groups.iter().map(|g| g["shapes"].as_array().unwrap().len()).sum::<usize>(), 44);
    assert!(groups.iter().all(|g| g["shapes"].as_array().unwrap().iter().all(|s| s["svg"].as_str().unwrap().starts_with("<svg"))));
    assert!(format!("{:?}", doc.execute("shape.previews", json!({ "size": 1e9 }))).contains("invalid-params"));
}

#[test]
fn previews_follow_the_groups_are_unfilled_and_answer_their_output_schema() {
    let mut doc = BoardDocument::create("Shapes.kboard", 800, 600).unwrap();
    let answer = doc.execute("shape.previews", json!({ "size": 40 })).unwrap();
    let listed: Vec<(String, Vec<String>)> = answer["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            (g["id"].as_str().unwrap().to_owned(), g["shapes"].as_array().unwrap().iter().map(|s| s["shape"].as_str().unwrap().to_owned()).collect())
        })
        .collect();
    let expected: Vec<(String, Vec<String>)> =
        SHAPE_GROUPS.iter().map(|(id, shapes)| ((*id).to_owned(), shapes.iter().map(|s| (*s).to_owned()).collect())).collect();
    assert_eq!(listed, expected);
    let mut seen: Vec<String> = Vec::new();
    for group in answer["groups"].as_array().unwrap() {
        for shape in group["shapes"].as_array().unwrap() {
            let svg = shape["svg"].as_str().unwrap();
            let name = shape["shape"].as_str().unwrap();
            assert!(svg.contains(r#"width="40" height="30""#) && svg.ends_with("</svg>"), "{name}: {svg}");
            assert!(svg.contains("currentColor") && !svg.contains("fill=\"var(") && !svg.contains("NaN"), "{name}: {svg}");
            assert!(!seen.iter().any(|other| other == svg), "{name} looks like another shape");
            seen.push(svg.to_owned());
        }
    }
    // A reading command: nothing changed, nothing to undo.
    let inspected = doc.inspect().unwrap();
    assert_eq!((inspected["elements"].as_u64(), inspected["canUndo"].as_bool()), (Some(0), Some(false)));
    let output = doc.commands().into_iter().find(|c| c.id == "shape.previews").and_then(|c| c.output_schema).unwrap();
    kition_kit::schema::validate_output(&output).unwrap();
    kition_kit::schema::check_params(&output["properties"], answer.as_object().unwrap()).unwrap();
    for refused in [json!({ "size": 4 }), json!({ "size": "big" }), json!({ "size": f64::MAX }), json!({ "shape": "rectangle" })] {
        assert!(format!("{:?}", doc.execute("shape.previews", refused.clone())).contains("invalid-params"), "{refused}");
    }
}

/// `shape.previews` carries what a bound connector end meets (case `board.connectors/outline-ends`),
/// so the page derives ends as the engine does: the polygon table's corners, the ellipses, the
/// parallelogram's inset rule, and nothing for the box.
#[test]
fn previews_carry_the_outline_a_bound_end_meets() {
    use kition_board_kdm::outline::{HEXAGON, POLYGONS, SLANT};
    let mut doc = BoardDocument::create("Shapes.kboard", 800, 600).unwrap();
    let answer = doc.execute("shape.previews", json!({})).unwrap();
    let shapes: Vec<Value> = answer["groups"].as_array().unwrap().iter().flat_map(|g| g["shapes"].as_array().unwrap().clone()).collect();
    let outline = |name: &str| shapes.iter().find(|s| s["shape"] == name).unwrap().get("outline").cloned();
    assert_eq!(outline("diamond").unwrap().as_array().unwrap().len(), 4);
    assert_eq!(outline("ellipse"), Some(json!("ellipse")));
    assert_eq!(outline("or-junction"), Some(json!("ellipse")));
    assert_eq!(outline("rectangle"), None);
    assert_eq!(outline("star"), None, "the box");
    assert_eq!(outline("parallelogram"), Some(json!({ "slant": [SLANT.0, SLANT.1] })));
    for (name, list) in POLYGONS.iter().chain([&("hexagon", HEXAGON)]) {
        let corners: Vec<[f64; 2]> = list.iter().map(|(x, y)| [*x, *y]).collect();
        assert_eq!(outline(name), Some(json!(corners)), "{name}");
    }
}
