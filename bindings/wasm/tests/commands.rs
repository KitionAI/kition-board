//! The board's commands through the document (design note 5.1; ports of Kition's
//! `boardCommands.spec.ts`, `boardHierarchy.spec.ts`, `boardBindingEngine.spec.ts`,
//! `boardLayout.spec.ts`, `boardClipboard.spec.ts` and `whiteboardAgentPatch.spec.ts` for the
//! board model). Every command has a case; every command survives empty and malformed parameters
//! (E3) and every agent command has a valid parameter schema (E5).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kdm::{End, Kind, Point};
use kition_board_kit::BoardDocument;
use kition_kit::kdm::ObjectId;
use kition_kit::session::KitSessionDocument;
use kition_kit::{KitDocument, schema};
use serde_json::{Value, json};

fn board() -> BoardDocument {
    BoardDocument::create("Plan.kboard", 1920, 1080).unwrap()
}

fn run(doc: &mut BoardDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("{command} {params}: {error}"))
}

fn fails(doc: &mut BoardDocument, command: &str, params: Value, code: &str) -> String {
    let error = doc.execute(command, params.clone()).err().unwrap_or_else(|| panic!("{command} {params} should fail"));
    assert!(error.starts_with(code), "{command} {params}: {error}");
    error
}

fn ids(value: &Value) -> Vec<String> {
    value["created"].as_array().unwrap().iter().map(|id| id.as_str().unwrap().to_owned()).collect()
}

fn shape(doc: &mut BoardDocument, x: f64, y: f64, w: f64, h: f64) -> String {
    ids(&run(doc, "element.create", json!({ "elements": [{ "kind": "shape", "position": [x, y], "size": [w, h] }] }))).remove(0)
}

fn element(doc: &BoardDocument, id: &str) -> kition_board_kdm::Element {
    doc.model().element(&ObjectId::from_string(id)).cloned().unwrap_or_else(|| panic!("no element {id}"))
}

fn stored_end(doc: &BoardDocument, id: &str, terminal: &str) -> End {
    End::parse(&doc.session().replica().get(&ObjectId::from_string(id), terminal).unwrap()).unwrap()
}

fn state(doc: &BoardDocument) -> Value {
    doc.session().replica().state_json().unwrap()
}

fn close(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
}

/// E3: every command, with no parameters and with parameters of the wrong types, answers a typed
/// error or a result, never a panic.
#[test]
fn every_command_survives_empty_params() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, 100.0, 50.0);
    let junk = [
        json!({}),
        json!({ "ids": 5, "id": [], "page": 3, "elements": "x", "set": [], "box": "x", "from": 1, "to": "y", "payload": [] }),
        json!({ "ids": [a], "id": a, "page": "no-page", "elements": [{}], "set": { "nothing": 1 }, "box": [0, 0, "x", 1], "degrees": "x" }),
        json!({ "ids": [], "elements": [], "points": [[0, 0]], "range": [5, 2], "mark": "underline", "data": "!!", "file": "" }),
        json!({ "ids": ["missing"], "id": "missing", "from": { "element": "missing" }, "to": { "point": [1e300, 0] }, "waypoints": [[0]] }),
    ];
    let codes = ["invalid-params", "not-found", "locked", "unknown command"];
    for command in doc.commands() {
        for params in &junk {
            let before = state(&doc);
            match doc.execute(&command.id, params.clone()) {
                Ok(_) => {}
                Err(error) => {
                    assert!(codes.iter().any(|code| error.starts_with(code)), "{} {params}: untyped error {error}", command.id);
                    assert_eq!(state(&doc), before, "{} {params}: a refused command changed the board", command.id);
                }
            }
        }
    }
    assert!(doc.execute("element.create", Value::Null).is_err());
    assert!(doc.execute("element.create", json!([1])).is_err());
}

/// E5, E7: every agent command has a valid `ParameterSchemaSpec`; editor-only ones have none.
#[test]
fn agent_commands_have_parameter_schemas() {
    let doc = board();
    let commands = doc.commands();
    assert!(commands.len() >= 39);
    for command in &commands {
        match &command.schema {
            Some(spec) => schema::validate(spec).unwrap_or_else(|error| panic!("{}: {error}", command.id)),
            None => panic!("{} has no schema: every command is offered to agents (AX5)", command.id),
        }
    }
    for id in ["edit.undo", "edit.redo"] {
        assert!(commands.iter().any(|command| command.id == id));
    }
    assert!(commands.iter().find(|command| command.id == "clipboard.copy").is_some_and(|command| !command.mutates));
    let mut sandboxed = kition_board_kit::SANDBOXED.to_vec();
    let mut reading = kition_board_model::spec::reading_files();
    sandboxed.sort_unstable();
    reading.sort_unstable();
    assert_eq!(sandboxed, reading, "kition-board lends files to exactly the commands that read them");
}

#[test]
fn element_create_makes_shapes_text_frames_and_strokes() {
    let mut doc = board();
    let made = run(
        &mut doc,
        "element.create",
        json!({ "elements": [
            { "kind": "frame", "position": [0, 0], "text": "Q4" },
            { "kind": "shape", "position": { "x": 10, "y": 20 }, "text": "Ship", "fillColor": "#5645D4", "key": "ship" },
            { "kind": "text", "position": [0, 300], "text": "Notes", "fontSize": 24 },
            { "kind": "stroke", "points": [[0, 0], [10, 10], [20, 0]], "strokeColor": "red" },
        ] }),
    );
    let made = ids(&made);
    assert_eq!(made.len(), 4);
    let frame = element(&doc, &made[0]);
    assert_eq!((frame.kind, frame.size.unwrap().w, frame.text.as_str()), (Kind::Frame, 640.0, "Q4"));
    let ship = element(&doc, &made[1]);
    assert_eq!(ship.register("shapeType"), Some(&json!("rectangle")));
    assert_eq!(ship.register("fillColor"), Some(&json!("#5645d4")));
    assert_eq!(ship.key.as_deref(), Some("ship"));
    assert_eq!(element(&doc, &made[3]).points.len(), 3);
    assert_eq!(doc.model().pages()[0].elements.len(), 4, "appended in order");
    fails(&mut doc, "element.create", json!({ "elements": [{ "kind": "connector" }] }), "invalid-params: make a connector with connector.create");
    fails(
        &mut doc,
        "element.create",
        json!({ "elements": [{ "kind": "image", "position": [0, 0] }] }),
        "invalid-params: make a image with image.insert",
    );
    fails(&mut doc, "element.create", json!({ "elements": [{ "kind": "shape" }] }), "invalid-params: a shape needs a `position`");
    fails(&mut doc, "element.create", json!({ "elements": [{ "kind": "stroke", "points": [[0, 0]] }] }), "invalid-params");
    fails(&mut doc, "element.create", json!({ "elements": [{ "kind": "shape", "position": [0, 0], "points": [[0, 0], [1, 1]] }] }), "invalid-params");
    fails(
        &mut doc,
        "element.create",
        json!({ "elements": [{ "kind": "shape", "position": [0, 0], "key": "ship" }] }),
        "invalid-params: key `ship` is already used",
    );
    fails(&mut doc, "element.create", json!({ "elements": [{ "kind": "shape", "position": [0, 0], "fillColor": "violet" }] }), "invalid-params");
    fails(
        &mut doc,
        "element.create",
        json!({ "elements": [{ "kind": "shape", "position": [0, 0], "parent": made[1] }] }),
        "invalid-params: a shape cannot contain",
    );
    // One gesture, one undo step.
    run(&mut doc, "edit.undo", json!({}));
    assert_eq!(doc.model().element_count(), 0);
}

/// Gap 14: moving a bound target rewrites the connector's stored point in the same command.
#[test]
fn moving_and_updating_a_target_recomputes_bound_ends_in_the_command() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, 100.0, 50.0);
    let b = shape(&mut doc, 400.0, 0.0, 100.0, 50.0);
    let connector = ids(&run(&mut doc, "connector.create", json!({ "from": { "element": a }, "to": { "element": b } }))).remove(0);
    assert!(close(stored_end(&doc, &connector, "start").point, Point::new(100.0, 25.0)), "the start binds to a's right edge");
    assert!(close(stored_end(&doc, &connector, "end").point, Point::new(400.0, 25.0)), "the end binds to b's left edge");
    assert_eq!(stored_end(&doc, &connector, "start").binding.unwrap().anchor, [1.0, 0.5]);
    run(&mut doc, "element.move", json!({ "ids": [b], "delta": [0, 200] }));
    assert!(close(stored_end(&doc, &connector, "end").point, Point::new(400.0, 225.0)));
    run(&mut doc, "element.update", json!({ "id": a, "set": { "position": [0, 100] } }));
    assert!(close(stored_end(&doc, &connector, "start").point, Point::new(100.0, 125.0)));
    run(&mut doc, "element.resize", json!({ "id": a, "box": [0, 100, 200, 150] }));
    assert!(close(stored_end(&doc, &connector, "start").point, Point::new(200.0, 125.0)));
    run(&mut doc, "element.rotate", json!({ "ids": [a], "degrees": 90 }));
    let model = doc.model();
    let derived = model.end_point(&ObjectId::from_string(&connector), &stored_end(&doc, &connector, "start"));
    assert!(close(stored_end(&doc, &connector, "start").point, derived), "the cache matches the derived end");
}

#[test]
fn element_update_sets_fields_of_the_kind_and_points_elsewhere_for_the_rest() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, 100.0, 50.0);
    run(&mut doc, "element.update", json!({ "id": a, "set": { "shapeType": "diamond", "opacity": 0.5, "roundness": null } }));
    assert_eq!(element(&doc, &a).register("shapeType"), Some(&json!("diamond")));
    fails(
        &mut doc,
        "element.update",
        json!({ "id": a, "set": { "text": "x" } }),
        "invalid-params: `text` cannot be set by element.update (use element.setText)",
    );
    fails(&mut doc, "element.update", json!({ "id": a, "set": { "points": [[0, 0], [1, 1]] } }), "invalid-params: a shape element has no `points`");
    fails(&mut doc, "element.update", json!({ "id": a, "set": {} }), "invalid-params");
    fails(&mut doc, "element.update", json!({ "id": a, "set": { "parent": a } }), "invalid-params");
    fails(&mut doc, "element.update", json!({ "id": "nope", "set": { "opacity": 1 } }), "not-found");
}

#[test]
fn deleting_a_frame_deletes_its_content_and_frees_connectors_bound_to_it() {
    let mut doc = board();
    let child = shape(&mut doc, 20.0, 20.0, 100.0, 50.0);
    let outside = shape(&mut doc, 800.0, 20.0, 100.0, 50.0);
    let frame =
        run(&mut doc, "frame.create", json!({ "box": [0, 0, 400, 300], "title": "Q4", "children": [child] }))["frame"].as_str().unwrap().to_owned();
    assert_eq!(doc.model().pages()[0].elements[0].as_str(), frame, "the frame goes behind its children");
    let connector = ids(&run(&mut doc, "connector.create", json!({ "from": { "element": child }, "to": { "element": outside } }))).remove(0);
    let before = stored_end(&doc, &connector, "start").point;
    let deleted = run(&mut doc, "element.delete", json!({ "ids": [frame] }));
    assert_eq!(deleted["deleted"].as_array().unwrap().len(), 2);
    let start = stored_end(&doc, &connector, "start");
    assert!(start.binding.is_none() && close(start.point, before), "the end stays where it was, unbound");
    assert!(stored_end(&doc, &connector, "end").binding.is_some());
    run(&mut doc, "edit.undo", json!({}));
    assert_eq!(doc.model().element_count(), 4);
}

#[test]
fn moving_a_frame_moves_its_content() {
    let mut doc = board();
    let child = shape(&mut doc, 20.0, 20.0, 100.0, 50.0);
    let stroke = ids(&run(&mut doc, "stroke.create", json!({ "points": [[30, 30], [60, 60]], "style": { "strokeColor": "blue" } }))).remove(0);
    let frame = run(&mut doc, "frame.create", json!({ "box": [0, 0, 400, 300], "children": [child, stroke] }))["frame"].as_str().unwrap().to_owned();
    run(&mut doc, "element.move", json!({ "ids": [frame, child], "delta": [10, 5] }));
    assert_eq!(element(&doc, &child).position, Some(Point::new(30.0, 25.0)), "moved once, as part of the frame");
    assert_eq!(element(&doc, &stroke).points[0], Point::new(40.0, 35.0));
    run(&mut doc, "element.move", json!({ "ids": [frame], "to": [100, 100] }));
    assert_eq!(element(&doc, &frame).position, Some(Point::new(100.0, 100.0)));
    fails(&mut doc, "element.move", json!({ "ids": [frame] }), "invalid-params: give exactly one of `delta` and `to`");
}

#[test]
fn a_drop_into_a_frame_makes_a_child_and_a_drop_outside_frees_it() {
    let mut doc = board();
    let frame = run(&mut doc, "frame.create", json!({ "box": [500, 0, 900, 300] }))["frame"].as_str().unwrap().to_owned();
    let inner = run(&mut doc, "frame.create", json!({ "box": [520, 20, 640, 140] }))["frame"].as_str().unwrap().to_owned();
    let a = shape(&mut doc, 0.0, 0.0, 100.0, 50.0);
    let b = shape(&mut doc, 0.0, 100.0, 100.0, 50.0);
    let order = |doc: &BoardDocument| doc.model().pages()[0].elements.clone();
    // A shape listed behind the frame (so drawn under it) comes in front of it when dropped in.
    run(&mut doc, "element.reorder", json!({ "ids": [b], "placement": "back" }));
    let answer = run(&mut doc, "element.move", json!({ "ids": [a, b], "delta": [700, 100], "reparent": true }));
    assert_eq!(answer["reparented"].as_array().unwrap().len(), 2);
    assert_eq!(element(&doc, &a).parent.as_ref().map(ObjectId::as_str), Some(frame.as_str()));
    let listed = order(&doc);
    let at = |id: &str| listed.iter().position(|item| item.as_str() == id).unwrap();
    assert!(at(&b) > at(&frame), "the child is drawn in front of its frame");
    // Into the smaller frame inside the big one.
    run(&mut doc, "element.move", json!({ "ids": [a], "to": [550, 50], "reparent": true }));
    assert_eq!(element(&doc, &a).parent.as_ref().map(ObjectId::as_str), Some(inner.as_str()), "the smallest frame under the centre");
    // Out of every frame: free again, in one undo step with the move.
    run(&mut doc, "element.move", json!({ "ids": [a], "to": [0, 400], "reparent": true }));
    assert_eq!(element(&doc, &a).parent, None);
    run(&mut doc, "edit.undo", json!({}));
    assert_eq!(element(&doc, &a).parent.as_ref().map(ObjectId::as_str), Some(inner.as_str()));
    assert_eq!(element(&doc, &a).position, Some(Point::new(550.0, 50.0)));
    // A group's member keeps its group when it is moved out of every frame; a move without
    // `reparent` changes no parent.
    let group = run(&mut doc, "element.group", json!({ "ids": [b] }))["group"].as_str().unwrap().to_owned();
    run(&mut doc, "element.move", json!({ "ids": [b], "delta": [0, 1000] }));
    assert_eq!(element(&doc, &b).parent.as_ref().map(ObjectId::as_str), Some(group.as_str()));
    // A frame is never dropped into itself or what moves with it.
    run(&mut doc, "element.move", json!({ "ids": [frame], "delta": [5, 5], "reparent": true }));
    assert_eq!(element(&doc, &frame).parent, None);
}

#[test]
fn resize_scales_strokes_and_groups_into_the_box() {
    let mut doc = board();
    let stroke = ids(&run(&mut doc, "stroke.create", json!({ "points": [[0, 0], [100, 50]] }))).remove(0);
    run(&mut doc, "element.resize", json!({ "id": stroke, "box": [0, 0, 200, 200] }));
    assert_eq!(element(&doc, &stroke).points, vec![Point::new(0.0, 0.0), Point::new(200.0, 200.0)]);
    let a = shape(&mut doc, 0.0, 0.0, 100.0, 100.0);
    let b = shape(&mut doc, 100.0, 0.0, 100.0, 100.0);
    let group = run(&mut doc, "element.group", json!({ "ids": [a, b] }))["group"].as_str().unwrap().to_owned();
    run(&mut doc, "element.resize", json!({ "id": group, "box": [0, 0, 400, 100] }));
    assert_eq!(element(&doc, &b).position, Some(Point::new(200.0, 0.0)));
    assert_eq!(element(&doc, &b).size.unwrap().w, 200.0);
}

#[test]
fn resize_scales_a_selection_into_its_new_bounds() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, 100.0, 100.0);
    let b = shape(&mut doc, 200.0, 0.0, 100.0, 100.0);
    run(&mut doc, "element.rotate", json!({ "ids": [b], "degrees": 90 }));
    let stroke = ids(&run(&mut doc, "stroke.create", json!({ "points": [[0, 100], [300, 100]] }))).remove(0);
    let text =
        ids(&run(&mut doc, "element.create", json!({ "elements": [{ "kind": "text", "position": [0, 150], "text": "Note", "fontSize": 20 }] })))
            .remove(0);
    let bounds = |doc: &BoardDocument| {
        [&a, &b, &stroke, &text].iter().filter_map(|id| doc.model().bounds(&element(doc, id))).reduce(kition_board_kdm::Rect::union).unwrap()
    };
    let before = bounds(&doc);
    let undo_before = doc.session().can_undo();
    let target = [before.x0, before.y0, before.x0 + before.width() * 2.0, before.y0 + before.height() * 2.0];
    let answer = run(&mut doc, "element.resize", json!({ "ids": [a, b, stroke, text], "box": target }));
    assert_eq!(answer["updated"].as_array().unwrap().len(), 4);
    assert_eq!(element(&doc, &a).size.unwrap().w, 200.0);
    assert!(close(element(&doc, &a).position.unwrap(), Point::new(0.0, 0.0)), "{:?}", element(&doc, &a).position);
    assert_eq!(element(&doc, &b).rotation, 90.0, "a rotated box keeps its rotation");
    assert!(close(element(&doc, &b).position.unwrap(), Point::new(400.0, 0.0)), "{:?}", element(&doc, &b).position);
    assert_eq!(element(&doc, &stroke).points, vec![Point::new(0.0, 200.0), Point::new(600.0, 200.0)]);
    assert_eq!(element(&doc, &text).register("fontSize").and_then(Value::as_f64), Some(40.0), "letters scale with the area");
    let after = bounds(&doc);
    assert!((after.x0 - target[0]).abs() < 1e-6 && (after.x1 - target[2]).abs() < 1e-6, "{after:?} for {target:?}");
    assert!(undo_before);
    run(&mut doc, "edit.undo", json!({}));
    assert_eq!(element(&doc, &a).size.unwrap().w, 100.0, "one undo step");
    assert_eq!(element(&doc, &stroke).points, vec![Point::new(0.0, 100.0), Point::new(300.0, 100.0)]);
    run(&mut doc, "element.lock", json!({ "ids": [a], "locked": true }));
    fails(&mut doc, "element.resize", json!({ "ids": [a, b], "box": [0, 0, 10, 10] }), "locked");
    fails(&mut doc, "element.resize", json!({ "ids": [b], "id": b, "box": [0, 0, 10, 10] }), "invalid-params");
}

#[test]
fn rotation_about_each_centre_or_the_selection() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, 100.0, 100.0);
    let b = shape(&mut doc, 200.0, 0.0, 100.0, 100.0);
    run(&mut doc, "element.rotate", json!({ "ids": [a, b], "degrees": 180, "about": "selection" }));
    let moved = element(&doc, &a).position.unwrap();
    assert!(close(moved, Point::new(200.0, 0.0)), "{moved:?}");
    assert_eq!(element(&doc, &a).rotation, 180.0);
    run(&mut doc, "element.rotate", json!({ "ids": [a], "degrees": 45 }));
    assert_eq!(element(&doc, &a).rotation, 225.0);
    assert!(close(element(&doc, &a).position.unwrap(), moved), "about its own centre the box stays");
    fails(&mut doc, "element.rotate", json!({ "ids": [a], "degrees": 1, "about": "page" }), "invalid-params");
}

#[test]
fn style_applies_the_fields_each_kind_uses() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, 100.0, 100.0);
    let stroke = ids(&run(&mut doc, "stroke.create", json!({ "points": [[0, 0], [1, 1]] }))).remove(0);
    let updated = run(
        &mut doc,
        "element.style",
        json!({ "ids": [a, stroke], "style": { "fillColor": "yellow", "strokeColor": "#00FF00", "dashStyle": "dashed" } }),
    );
    assert_eq!(updated["updated"].as_array().unwrap().len(), 2);
    assert_eq!(element(&doc, &a).register("fillColor"), Some(&json!("yellow")));
    assert_eq!(element(&doc, &stroke).register("fillColor"), None, "a stroke has no fill");
    assert_eq!(element(&doc, &stroke).register("strokeColor"), Some(&json!("#00ff00")));
    fails(
        &mut doc,
        "element.style",
        json!({ "ids": [stroke], "style": { "fillColor": "red" } }),
        "invalid-params: none of the elements uses `fillColor`",
    );
    fails(&mut doc, "element.style", json!({ "ids": [a], "style": { "shapeType": "star" } }), "invalid-params");
}

#[test]
fn disabled_commands_match_the_command_table() {
    let mut doc = board();
    let derived = |doc: &BoardDocument| doc.commands().into_iter().filter(|command| !command.enabled).map(|command| command.id).collect::<Vec<_>>();
    assert_eq!(doc.disabled_commands(), derived(&doc));
    shape(&mut doc, 0.0, 0.0, 10.0, 10.0);
    assert_eq!(doc.disabled_commands(), derived(&doc));
    run(&mut doc, "edit.undo", json!({}));
    assert_eq!(doc.disabled_commands(), derived(&doc));
}

#[test]
fn get_style_answers_values_defaults_and_mixed_fields() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, 100.0, 100.0);
    let b = shape(&mut doc, 200.0, 0.0, 100.0, 100.0);
    let stroke = ids(&run(&mut doc, "stroke.create", json!({ "points": [[0, 0], [1, 1]] }))).remove(0);
    run(&mut doc, "element.style", json!({ "ids": [a], "style": { "fillColor": "yellow", "strokeSize": "l" } }));
    let revision = doc.revision();
    let both = run(&mut doc, "element.getStyle", json!({ "ids": [a, b] }));
    assert_eq!(both["style"]["fillColor"], Value::Null, "the two shapes differ");
    assert_eq!(both["style"]["strokeColor"], json!("ink"), "both draw the default line colour");
    assert_eq!(both["style"]["fontSize"], json!(16.0));
    assert!(both["fields"].as_array().unwrap().iter().any(|field| field == "fillStyle"));
    assert!(!both["fields"].as_array().unwrap().iter().any(|field| field == "endArrowhead"), "shapes have no arrowheads");
    let one = run(&mut doc, "element.getStyle", json!({ "ids": [a] }));
    assert_eq!(one["style"]["fillColor"], json!("yellow"));
    assert_eq!(one["style"]["strokeSize"], json!("l"));
    let line = run(&mut doc, "element.getStyle", json!({ "ids": [stroke] }));
    assert_eq!(line["fields"], json!(["strokeColor", "dashStyle", "strokeSize", "opacity"]));
    let sticky = run(&mut doc, "element.getStyle", json!({ "kind": "shape", "shapeStyle": "sticky" }));
    assert_eq!(sticky["style"]["fillColor"], json!("yellow"), "a new sticky note's default fill");
    let connector = run(&mut doc, "element.getStyle", json!({ "kind": "connector" }));
    assert_eq!((connector["style"]["startArrowhead"].clone(), connector["style"]["endArrowhead"].clone()), (json!("none"), json!("arrow")));
    assert_eq!(connector["palette"].as_array().unwrap().len(), 9);
    assert_eq!(doc.revision(), revision, "reading a style is no edit");
    fails(&mut doc, "element.getStyle", json!({ "ids": [a], "kind": "shape" }), "invalid-params");
    fails(&mut doc, "element.getStyle", json!({ "ids": ["missing"] }), "not-found");
}

#[test]
fn text_is_written_as_a_difference_and_marked_in_ranges() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, 100.0, 100.0);
    run(&mut doc, "element.setText", json!({ "id": a, "text": "Launch 🚀 plan" }));
    run(&mut doc, "element.setText", json!({ "id": a, "text": "Launch 🚀 the plan" }));
    assert_eq!(element(&doc, &a).text, "Launch 🚀 the plan");
    run(&mut doc, "text.mark", json!({ "id": a, "range": [0, 6], "mark": "bold" }));
    // UTF-16: the rocket is two code units; "the" starts at 10.
    run(&mut doc, "text.mark", json!({ "id": a, "range": [10, 13], "mark": "italic", "units": "utf16" }));
    run(&mut doc, "text.mark", json!({ "id": a, "range": [13, 17], "mark": "link", "value": { "href": "https://kition.ai" } }));
    let runs = doc.session().replica().text_delta(&ObjectId::from_string(&a), "text").unwrap();
    let marked: Vec<(String, Vec<String>)> = runs.iter().map(|run| (run.insert.clone(), run.attributes.keys().cloned().collect())).collect();
    assert_eq!(marked[0], ("Launch".to_owned(), vec!["bold".to_owned()]));
    assert!(marked.iter().any(|(text, marks)| text == "the" && marks == &["italic".to_owned()]));
    assert!(marked.iter().any(|(text, marks)| text == "plan" && marks == &["link".to_owned()]));
    run(&mut doc, "text.mark", json!({ "id": a, "range": [0, 6], "mark": "bold", "value": null }));
    fails(
        &mut doc,
        "text.mark",
        json!({ "id": a, "range": [7, 8], "mark": "bold", "units": "utf16" }),
        "invalid-params: a UTF-16 offset falls inside a character",
    );
    fails(&mut doc, "text.mark", json!({ "id": a, "range": [0, 2], "mark": "link", "value": { "href": "javascript:x" } }), "invalid-params");
    fails(&mut doc, "text.mark", json!({ "id": a, "range": [0, 99], "mark": "bold" }), "invalid-params");
    let stroke = ids(&run(&mut doc, "stroke.create", json!({ "points": [[0, 0], [1, 1]] }))).remove(0);
    fails(&mut doc, "element.setText", json!({ "id": stroke, "text": "x" }), "invalid-params: a stroke has no text");
}

#[test]
fn duplicate_copies_trees_with_their_references() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, 100.0, 50.0);
    let b = shape(&mut doc, 300.0, 0.0, 100.0, 50.0);
    run(&mut doc, "element.update", json!({ "id": a, "set": { "key": "a" } }));
    run(&mut doc, "element.setText", json!({ "id": a, "text": "Bold" }));
    run(&mut doc, "text.mark", json!({ "id": a, "range": [0, 4], "mark": "bold" }));
    let connector = ids(&run(&mut doc, "connector.create", json!({ "from": { "element": a }, "to": { "element": b } }))).remove(0);
    let frame =
        run(&mut doc, "frame.create", json!({ "box": [-20, -20, 500, 100], "children": [a, b, connector] }))["frame"].as_str().unwrap().to_owned();
    let copy = run(&mut doc, "element.duplicate", json!({ "ids": [frame] }));
    assert_eq!(copy["created"].as_array().unwrap().len(), 4);
    let copies = &copy["copies"];
    let (new_a, new_b, new_connector, new_frame) =
        (copies[&a].as_str().unwrap(), copies[&b].as_str().unwrap(), copies[&connector].as_str().unwrap(), copies[&frame].as_str().unwrap());
    assert_eq!(element(&doc, new_a).parent.unwrap().as_str(), new_frame);
    assert_eq!(element(&doc, new_a).position, Some(Point::new(24.0, 24.0)));
    assert_eq!(element(&doc, new_a).key, None, "keys stay unique");
    assert!(!element(&doc, new_a).runs.is_empty(), "marks are copied");
    assert_eq!(stored_end(&doc, new_connector, "start").binding.unwrap().target.as_str(), new_a);
    assert_eq!(stored_end(&doc, new_connector, "end").binding.unwrap().target.as_str(), new_b);
}

#[test]
fn reorder_and_lock() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, 10.0, 10.0);
    let b = shape(&mut doc, 0.0, 0.0, 10.0, 10.0);
    let c = shape(&mut doc, 0.0, 0.0, 10.0, 10.0);
    let order = |doc: &BoardDocument| doc.model().pages()[0].elements.iter().map(|id| id.as_str().to_owned()).collect::<Vec<_>>();
    run(&mut doc, "element.reorder", json!({ "ids": [a], "placement": "front" }));
    assert_eq!(order(&doc), vec![b.clone(), c.clone(), a.clone()]);
    run(&mut doc, "element.reorder", json!({ "ids": [a], "placement": "back" }));
    assert_eq!(order(&doc), vec![a.clone(), b.clone(), c.clone()]);
    run(&mut doc, "element.reorder", json!({ "ids": [a], "placement": "forward" }));
    assert_eq!(order(&doc), vec![b.clone(), a.clone(), c.clone()]);
    run(&mut doc, "element.reorder", json!({ "ids": [c], "placement": "backward" }));
    assert_eq!(order(&doc), vec![b.clone(), c.clone(), a.clone()]);
    run(&mut doc, "element.lock", json!({ "ids": [a], "locked": true }));
    fails(&mut doc, "element.move", json!({ "ids": [a], "delta": [1, 1] }), "locked");
    fails(&mut doc, "element.delete", json!({ "ids": [a] }), "locked");
    run(&mut doc, "element.lock", json!({ "ids": [a], "locked": false }));
    run(&mut doc, "element.move", json!({ "ids": [a], "delta": [1, 1] }));
}

#[test]
fn group_and_ungroup_keep_the_hierarchy() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, 10.0, 10.0);
    let b = shape(&mut doc, 50.0, 0.0, 10.0, 10.0);
    let frame = run(&mut doc, "frame.create", json!({ "box": [-10, -10, 100, 100], "children": [a, b] }))["frame"].as_str().unwrap().to_owned();
    let group = run(&mut doc, "element.group", json!({ "ids": [a, b] }))["group"].as_str().unwrap().to_owned();
    assert_eq!(element(&doc, &group).parent.unwrap().as_str(), frame, "the group stays inside the common parent");
    assert_eq!(element(&doc, &a).parent.unwrap().as_str(), group);
    assert_eq!(doc.model().bounds(&element(&doc, &group)).unwrap().x1, 60.0);
    fails(&mut doc, "element.ungroup", json!({ "groupIds": [frame] }), "invalid-params");
    run(&mut doc, "element.ungroup", json!({ "groupIds": [group] }));
    assert!(doc.model().element(&ObjectId::from_string(&group)).is_none());
    assert_eq!(element(&doc, &a).parent.unwrap().as_str(), frame);
}

#[test]
fn frames_fit_their_content() {
    let mut doc = board();
    let a = shape(&mut doc, 100.0, 100.0, 50.0, 50.0);
    let frame = run(&mut doc, "frame.create", json!({ "box": [0, 0, 10, 10], "children": [a] }))["frame"].as_str().unwrap().to_owned();
    run(&mut doc, "frame.fit", json!({ "id": frame, "padding": 10 }));
    assert_eq!(element(&doc, &frame).position, Some(Point::new(90.0, 90.0)));
    assert_eq!(element(&doc, &frame).size.unwrap().w, 70.0);
    let empty = run(&mut doc, "frame.create", json!({ "box": [0, 0, 10, 10] }))["frame"].as_str().unwrap().to_owned();
    fails(&mut doc, "frame.fit", json!({ "id": empty }), "invalid-params: the frame has no children");
    fails(&mut doc, "frame.fit", json!({ "id": a }), "invalid-params");
}

#[test]
fn connectors_bind_label_and_bend() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, 100.0, 50.0);
    let b = shape(&mut doc, 0.0, 300.0, 100.0, 50.0);
    let made = run(
        &mut doc,
        "connector.create",
        json!({ "from": { "element": a, "anchor": [0.5, 1] }, "to": { "point": [500, 500] }, "label": "yes", "type": "elbow" }),
    );
    let connector = ids(&made).remove(0);
    let element_c = element(&doc, &connector);
    assert_eq!((element_c.text.as_str(), element_c.register("endArrowhead")), ("yes", Some(&json!("arrow"))));
    assert!(close(stored_end(&doc, &connector, "start").point, Point::new(50.0, 50.0)));
    run(&mut doc, "connector.bind", json!({ "id": connector, "terminal": "end", "to": { "element": b } }));
    assert_eq!(stored_end(&doc, &connector, "end").binding.unwrap().anchor, [0.5, 0.0], "binds to b's top edge, facing a");
    run(&mut doc, "connector.bind", json!({ "id": connector, "terminal": "start", "to": { "point": [-50, -50] } }));
    assert!(stored_end(&doc, &connector, "start").binding.is_none());
    run(&mut doc, "connector.setLabel", json!({ "id": connector, "text": "no", "position": 0.25 }));
    assert_eq!(element(&doc, &connector).register("labelPosition"), Some(&json!(0.25)));
    run(&mut doc, "connector.setWaypoints", json!({ "id": connector, "waypoints": [[0, 100], [50, 100]] }));
    assert_eq!(element(&doc, &connector).waypoints.len(), 2);
    assert_eq!(doc.model().route(&element(&doc, &connector)).len(), 4);
    run(&mut doc, "connector.setWaypoints", json!({ "id": connector, "waypoints": [] }));
    assert!(element(&doc, &connector).waypoints.is_empty());
    fails(&mut doc, "connector.create", json!({ "from": { "element": a }, "to": { "element": a } }), "invalid-params");
    let stroke = ids(&run(&mut doc, "stroke.create", json!({ "points": [[0, 0], [1, 1]] }))).remove(0);
    fails(
        &mut doc,
        "connector.create",
        json!({ "from": { "element": a }, "to": { "element": stroke } }),
        "invalid-params: a connector cannot bind to a stroke",
    );
    fails(&mut doc, "connector.create", json!({ "from": { "element": a, "point": [0, 0] }, "to": { "point": [1, 1] } }), "invalid-params");
    fails(&mut doc, "connector.setLabel", json!({ "id": a, "text": "x" }), "invalid-params");
}

/// One test touches the host's lent files (a process-wide setting): cards and pictures read
/// through it.
#[test]
fn cards_and_pictures_read_workspace_files_through_the_host() {
    let root = std::env::temp_dir().join(format!("kition-board-cards-{}", std::process::id()));
    std::fs::create_dir_all(root.join("plans")).unwrap();
    let mut doc = board();
    let own = doc.save(None).unwrap();
    std::fs::write(root.join("plans/Self.kboard"), &own).unwrap();
    let other = board().save(None).unwrap();
    std::fs::write(root.join("plans/Map.kboard"), &other).unwrap();
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend_from_slice(&1600u32.to_be_bytes());
    png.extend_from_slice(&400u32.to_be_bytes());
    png.extend_from_slice(b"rest of the picture");
    std::fs::write(root.join("plans/chart.png"), &png).unwrap();
    kition_kit::host::set_root(Some(root.clone()));
    kition_kit::host::set_folder("");

    let note = run(&mut doc, "card.create", json!({ "file": "plans/Roadmap.md", "view": { "heading": ["Plan"] }, "caption": "The plan" }));
    assert_eq!(note["card"], json!({ "kind": "note", "target": { "path": "plans/Roadmap.md" }, "view": { "heading": ["Plan"] } }));
    let note_id = ids(&note).remove(0);
    assert_eq!(element(&doc, &note_id).text, "The plan");
    let map = run(&mut doc, "card.create", json!({ "file": "plans/Map.kboard", "position": [0, 400] }));
    assert_eq!(map["card"]["target"]["docId"], json!(BoardDocument::open("Map.kboard", &other).unwrap().doc_id()));
    fails(&mut doc, "card.create", json!({ "file": "plans/Self.kboard" }), "invalid-params: a board card cannot show the board it is on");
    let link = run(&mut doc, "card.create", json!({ "url": "https://kition.ai", "title": "Kition" }));
    assert_eq!(link["card"], json!({ "kind": "link", "url": "https://kition.ai", "title": "Kition" }));
    fails(&mut doc, "card.create", json!({ "url": "ftp://x.example" }), "invalid-params");
    fails(&mut doc, "card.create", json!({ "file": "a.md", "url": "https://kition.ai" }), "invalid-params");
    fails(&mut doc, "card.create", json!({ "file": "a.md", "kind": "table" }), "invalid-params");
    run(&mut doc, "card.setTarget", json!({ "id": note_id, "file": "plans/Later.md" }));
    assert_eq!(element(&doc, &note_id).text, "The plan", "the caption stays");
    let moved = run(&mut doc, "links.retarget", json!({ "from": "plans", "to": "archive/plans" }));
    assert_eq!(moved["retargeted"].as_array().unwrap().len(), 2);
    assert_eq!(element(&doc, &note_id).card.unwrap()["target"]["path"], "archive/plans/Later.md");
    let again = run(&mut doc, "links.retarget", json!({ "from": "plans", "to": "archive/plans" }));
    assert!(again["retargeted"].as_array().unwrap().is_empty(), "idempotent");

    let picture = run(&mut doc, "image.insert", json!({ "file": "plans/chart.png", "alt": "Sales" }));
    let picture_id = ids(&picture).remove(0);
    let image = element(&doc, &picture_id);
    assert_eq!(image.size.unwrap().w, 800.0, "scaled down to 800 wide");
    assert_eq!(image.size.unwrap().h, 200.0);
    assert_eq!(image.image.as_ref().unwrap().mime, "image/png");
    assert!(doc.has_blob(picture["blob"].as_str().unwrap()));
    run(&mut doc, "image.setAlt", json!({ "id": picture_id, "alt": "Sales by month" }));
    assert_eq!(element(&doc, &picture_id).register("alt"), Some(&json!("Sales by month")));
    let pasted =
        run(&mut doc, "image.insert", json!({ "data": kition_board_kboard::base64_encode(b"GIF89a\x10\x00\x20\x00rest"), "position": [0, 0] }));
    assert_eq!(element(&doc, &ids(&pasted)[0]).size.unwrap().h, 32.0);
    fails(&mut doc, "image.insert", json!({ "data": kition_board_kboard::base64_encode(b"plain text") }), "invalid-params: the file is not a PNG");
    fails(&mut doc, "image.insert", json!({ "file": "plans/chart.png", "mime": "image/jpeg" }), "invalid-params");
    fails(&mut doc, "image.insert", json!({ "file": "plans/missing.png" }), "invalid-params: cannot read");
    fails(&mut doc, "image.insert", json!({ "file": "../outside.png" }), "invalid-params: cannot read");
    let saved = doc.save(None).unwrap();
    assert_eq!(kition_board_kboard::read(&saved).unwrap().blobs.len(), 2);

    kition_kit::host::set_root(None);
    // A host that lends nothing still gets its card, without a docId.
    let unlent = run(&mut doc, "card.create", json!({ "file": "plans/Map.kboard" }));
    assert!(unlent["card"]["target"].get("docId").is_none());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn layout_aligns_distributes_and_stacks() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, 100.0, 50.0);
    let b = shape(&mut doc, 150.0, 40.0, 100.0, 50.0);
    let c = shape(&mut doc, 500.0, 80.0, 100.0, 50.0);
    run(&mut doc, "layout.align", json!({ "ids": [a, b, c], "edge": "top" }));
    assert!([&a, &b, &c].iter().all(|id| element(&doc, id).position.unwrap().y == 0.0));
    run(&mut doc, "layout.distribute", json!({ "ids": [a, b, c], "axis": "horizontal" }));
    assert_eq!(element(&doc, &b).position.unwrap().x, 250.0, "equal gaps between a and c");
    run(&mut doc, "layout.stack", json!({ "ids": [a, b, c], "axis": "vertical", "gap": 10 }));
    assert_eq!(element(&doc, &b).position.unwrap().y, 60.0);
    assert_eq!(element(&doc, &c).position.unwrap().y, 120.0);
    run(&mut doc, "layout.align", json!({ "ids": [a, b, c], "edge": "center" }));
    assert_eq!(element(&doc, &a).position.unwrap().x, 250.0);
    fails(&mut doc, "layout.align", json!({ "ids": [a], "edge": "diagonal" }), "invalid-params");
    fails(&mut doc, "layout.stack", json!({ "ids": [a], "axis": "z" }), "invalid-params");
}

#[test]
fn pages_are_created_renamed_duplicated_moved_activated_and_deleted() {
    let mut doc = board();
    let first = doc.model().pages()[0].id.as_str().to_owned();
    let a = shape(&mut doc, 0.0, 0.0, 100.0, 50.0);
    let b = shape(&mut doc, 300.0, 0.0, 100.0, 50.0);
    run(&mut doc, "element.update", json!({ "id": a, "set": { "key": "a" } }));
    run(&mut doc, "connector.create", json!({ "from": { "element": a }, "to": { "element": b } }));
    let second = run(&mut doc, "page.create", json!({ "name": "Ideas", "activate": true }))["page"].as_str().unwrap().to_owned();
    assert_eq!(doc.inspect().unwrap()["page"], json!(second));
    let on_second = shape(&mut doc, 0.0, 0.0, 10.0, 10.0);
    assert_eq!(doc.model().page_of(&ObjectId::from_string(&on_second)).unwrap().id.as_str(), second, "commands without page use the shown page");
    run(&mut doc, "page.activate", json!({ "page": first }));
    assert_eq!(doc.inspect().unwrap()["page"], json!(first));
    assert!(!doc.session().can_redo() || doc.session().can_undo(), "activating is not an undo step");
    run(&mut doc, "page.rename", json!({ "page": second, "name": "Later" }));
    let copy = run(&mut doc, "page.duplicate", json!({ "page": first }));
    assert_eq!(copy["name"], "Page 1 copy");
    assert_eq!(copy["elements"], 3);
    let copy_id = ObjectId::from_string(copy["page"].as_str().unwrap());
    let model = doc.model();
    assert_eq!(model.pages()[1].id, copy_id, "right after the source");
    let copied = model.page(&copy_id).unwrap();
    let connector = model.elements_of(copied).find(|element| element.kind == Kind::Connector).unwrap();
    let target = connector.start.as_ref().unwrap().binding.as_ref().unwrap().target.clone();
    assert_eq!(model.page_of(&target).unwrap().id, copy_id, "bindings point at the copies");
    assert!(model.by_key(copied, "a").is_some(), "keys stay on a new page");
    run(&mut doc, "page.move", json!({ "page": second, "to": 0 }));
    assert_eq!(doc.model().pages()[0].id.as_str(), second);
    run(&mut doc, "page.move", json!({ "page": second, "placement": "next" }));
    assert_eq!(doc.model().pages()[1].id.as_str(), second);
    run(&mut doc, "page.delete", json!({ "page": copy_id.as_str() }));
    run(&mut doc, "page.delete", json!({ "page": second }));
    assert!(doc.model().element(&ObjectId::from_string(&on_second)).is_none());
    fails(&mut doc, "page.delete", json!({ "page": first }), "invalid-params: a board keeps at least one page");
    fails(&mut doc, "page.rename", json!({ "page": first, "name": " " }), "invalid-params");
    fails(&mut doc, "page.activate", json!({ "page": "nope" }), "not-found");
    fails(&mut doc, "page.move", json!({ "page": first, "to": 5 }), "invalid-params");
}

#[test]
fn copy_and_paste_within_and_across_pages() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, 100.0, 50.0);
    let b = shape(&mut doc, 300.0, 0.0, 100.0, 50.0);
    let connector = ids(&run(&mut doc, "connector.create", json!({ "from": { "element": a }, "to": { "element": b } }))).remove(0);
    let payload = run(&mut doc, "clipboard.copy", json!({ "ids": [a, connector] }));
    assert_eq!(payload["type"], "application/x-kition-board+json");
    assert_eq!(payload["elements"].as_array().unwrap().len(), 2);
    let pasted = ids(&run(&mut doc, "element.paste", json!({ "payload": payload })));
    assert_eq!(element(&doc, &pasted[0]).position, Some(Point::new(24.0, 24.0)), "offset on the same page");
    let pasted_connector = &pasted[1];
    assert_eq!(stored_end(&doc, pasted_connector, "start").binding.unwrap().target.as_str(), pasted[0]);
    assert_eq!(
        stored_end(&doc, pasted_connector, "end").binding.unwrap().target.as_str(),
        b,
        "a target outside the payload on this page stays bound"
    );
    let other = run(&mut doc, "page.create", json!({}))["page"].as_str().unwrap().to_owned();
    let across = ids(&run(&mut doc, "element.paste", json!({ "page": other, "payload": payload, "at": [1000, 1000] })));
    assert_eq!(element(&doc, &across[0]).position, Some(Point::new(1000.0, 1000.0)));
    assert!(stored_end(&doc, &across[1], "end").binding.is_none(), "a target on another page is not followed");
    fails(&mut doc, "element.paste", json!({ "payload": { "type": "text/plain", "elements": [] } }), "invalid-params");
    fails(
        &mut doc,
        "element.paste",
        json!({ "payload": { "type": "application/x-kition-board+json", "version": 2, "elements": [{ "ref": "x", "kind": "shape", "position": "nowhere" }] } }),
        "invalid-params",
    );
}

/// Every mutating command is one undo step: one undo restores the board as it was.
#[test]
fn each_command_is_one_undo_step() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, 100.0, 50.0);
    let b = shape(&mut doc, 300.0, 0.0, 100.0, 50.0);
    let steps = [
        ("connector.create", json!({ "from": { "element": a }, "to": { "element": b }, "label": "x" })),
        ("element.move", json!({ "ids": [a, b], "delta": [10, 10] })),
        ("element.style", json!({ "ids": [a], "style": { "fillColor": "red" } })),
        ("element.group", json!({ "ids": [a, b] })),
        ("element.duplicate", json!({ "ids": [a] })),
        ("frame.create", json!({ "box": [0, 0, 500, 500], "children": [b] })),
        ("page.create", json!({ "name": "Two" })),
        ("layout.stack", json!({ "ids": [a, b], "axis": "horizontal" })),
    ];
    for (command, params) in steps {
        let before = state(&doc);
        run(&mut doc, command, params);
        assert_ne!(state(&doc), before, "{command} changed the board");
        run(&mut doc, "edit.undo", json!({}));
        assert_eq!(state(&doc), before, "{command} undoes in one step");
        run(&mut doc, "edit.redo", json!({}));
        run(&mut doc, "edit.undo", json!({}));
    }
}

/// Commands from two replicas merge, and the merge needs no repair.
#[test]
fn commands_on_two_replicas_converge() {
    let mut a_doc = board();
    let a = shape(&mut a_doc, 0.0, 0.0, 100.0, 50.0);
    let b = shape(&mut a_doc, 300.0, 0.0, 100.0, 50.0);
    // As a host saves: sealed first (`prepare_save`), so the file's seal is the session's own.
    a_doc.prepare_save().unwrap();
    let file = a_doc.save(None).unwrap();
    let mut other = BoardDocument::open("Plan.kboard", &file).unwrap();
    let since_a = a_doc.version().encode();
    let since_other = other.version().encode();
    run(&mut a_doc, "connector.create", json!({ "from": { "element": a }, "to": { "element": b } }));
    run(&mut other, "element.move", json!({ "ids": [b], "delta": [0, 300] }));
    let from_a = kition_kit::session::__export_changes(&a_doc, &since_a).unwrap();
    let from_other = kition_kit::session::__export_changes(&other, &since_other).unwrap();
    KitSessionDocument::import_changes(&mut a_doc, &from_other).unwrap();
    KitSessionDocument::import_changes(&mut other, &from_a).unwrap();
    assert_eq!(state(&a_doc), state(&other));
    let connector = a_doc.model().pages()[0].elements.iter().find(|id| a_doc.model().element(id).unwrap().kind == Kind::Connector).cloned().unwrap();
    let end = a_doc.model().element(&connector).unwrap().end.clone().unwrap();
    assert!(close(a_doc.model().end_point(&connector, &end), Point::new(300.0, 325.0)), "the end follows the moved target without a repair");
}

#[test]
fn mind_maps_grow_collapse_and_turn_in_one_command_each() {
    let mut doc = board();
    let root = ids(&run(
        &mut doc,
        "element.create",
        json!({ "elements": [{ "kind": "shape", "shapeStyle": "mind-node", "position": [0, 0], "size": [160, 60], "text": "Plan", "mindMapDirection": "right" }] }),
    ))
    .remove(0);
    let center = |doc: &BoardDocument, id: &str| doc.model().rect(&element(doc, id)).unwrap().center();
    let first = run(&mut doc, "mindmap.addChild", json!({ "node": root, "text": "Goals" }));
    let a = first["node"].as_str().unwrap().to_owned();
    assert_eq!(first["created"].as_array().unwrap().len(), 2, "a node and its branch");
    let b = run(&mut doc, "mindmap.addChild", json!({ "node": root, "text": "Risks" }))["node"].as_str().unwrap().to_owned();
    let rc = center(&doc, &root);
    assert!(close(center(&doc, &a), Point::new(rc.x + 250.0, rc.y - 48.0)), "{:?}", center(&doc, &a));
    assert!(close(center(&doc, &b), Point::new(rc.x + 250.0, rc.y + 48.0)), "{:?}", center(&doc, &b));
    assert_eq!(element(&doc, &a).text, "Goals");
    assert_eq!(element(&doc, &a).register("strokeColor"), Some(&json!("purple")), "the root's first branch colour");
    assert_eq!(element(&doc, &b).register("strokeColor"), Some(&json!("blue")));
    // The branch is bound from the root's right side to the child's left side.
    let page = doc.model().pages()[0].clone();
    let topology = kition_board_kdm::mindmap::Topology::read(doc.model(), &page);
    let edge = topology.edges.iter().find(|edge| edge.child.as_str() == a).unwrap().clone();
    let branch = element(&doc, edge.connector.as_str());
    assert_eq!(branch.register("connectorRole"), Some(&json!("mind-map-branch")));
    assert_eq!(branch.start.as_ref().unwrap().binding.as_ref().unwrap().anchor, [1.0, 0.5]);
    assert_eq!(branch.end.as_ref().unwrap().binding.as_ref().unwrap().anchor, [0.0, 0.5]);
    let deeper = run(&mut doc, "mindmap.addChild", json!({ "node": a, "text": "Q4" }))["node"].as_str().unwrap().to_owned();
    assert!((center(&doc, &deeper).x - (rc.x + 500.0)).abs() < 1e-6);
    assert_eq!(element(&doc, &deeper).register("strokeColor"), Some(&json!("purple")), "a branch keeps its colour");
    let sibling = run(&mut doc, "mindmap.addSibling", json!({ "node": b, "text": "Team" }))["node"].as_str().unwrap().to_owned();
    assert_eq!(topology_parent(&doc, &sibling).as_deref(), Some(root.as_str()));
    // One undo step takes a node and its branch away.
    let count = doc.model().element_count();
    run(&mut doc, "edit.undo", json!({}));
    assert_eq!(doc.model().element_count(), count - 2);
    run(&mut doc, "edit.redo", json!({}));

    // Collapsing hides what is below, from the page and from the raster.
    let collapsed = run(&mut doc, "mindmap.collapse", json!({ "node": a }));
    assert_eq!(collapsed["collapsed"], json!(true));
    let page = doc.model().pages()[0].clone();
    let hidden = kition_board_kdm::mindmap::hidden(doc.model(), &page);
    assert!(hidden.contains(&ObjectId::from_string(&deeper)) && hidden.len() == 2, "the node below and its branch: {hidden:?}");
    let svg = kition_board_scene::raster::page_svg(
        doc.model(),
        &page,
        &kition_board_scene::area(doc.model(), Some(&page)),
        &kition_board_kdm::blobs::SharedBlobs::default(),
    );
    assert!(!svg.contains("Q4") && svg.contains("Goals"), "a hidden node is not drawn");
    run(&mut doc, "mindmap.collapse", json!({ "node": a }));
    assert!(kition_board_kdm::mindmap::hidden(doc.model(), &doc.model().pages()[0].clone()).is_empty(), "toggled open again");

    // Both ways: the root's branches split between its sides.
    run(&mut doc, "mindmap.setDirection", json!({ "node": deeper, "direction": "both" }));
    assert_eq!(element(&doc, &root).register("mindMapDirection"), Some(&json!("both")));
    let rc = center(&doc, &root);
    let sides: Vec<bool> = [&a, &b, &sibling].iter().map(|id| center(&doc, id).x < rc.x).collect();
    assert!(sides.contains(&true) && sides.contains(&false), "{sides:?}");
    run(&mut doc, "mindmap.setDirection", json!({ "node": root, "direction": "down" }));
    assert!(center(&doc, &a).y > rc.y + 100.0, "growing down");
    run(&mut doc, "layout.mindmap", json!({ "root": b }));

    fails(&mut doc, "mindmap.collapse", json!({ "node": root }), "invalid-params");
    fails(&mut doc, "mindmap.addSibling", json!({ "node": root }), "invalid-params");
    fails(&mut doc, "mindmap.setDirection", json!({ "node": root, "direction": "up" }), "invalid-params");
    let plain = shape(&mut doc, 900.0, 900.0, 10.0, 10.0);
    fails(&mut doc, "mindmap.addChild", json!({ "node": plain }), "invalid-params");
    run(&mut doc, "element.lock", json!({ "ids": [b], "locked": true }));
    fails(&mut doc, "mindmap.addChild", json!({ "node": root }), "locked");
}

fn topology_parent(doc: &BoardDocument, id: &str) -> Option<String> {
    let page = doc.model().pages()[0].clone();
    kition_board_kdm::mindmap::Topology::read(doc.model(), &page).parent.get(&ObjectId::from_string(id)).map(|parent| parent.as_str().to_owned())
}

#[test]
fn templates_are_placed_whole_in_one_command() {
    let mut doc = board();
    let expected = [("mindmap", 7), ("flowchart", 14), ("roadmap", 21), ("kanban", 14), ("retrospective", 14), ("storyboard", 26)];
    for (template, count) in expected {
        let before = doc.model().element_count();
        let answer = run(&mut doc, "template.insert", json!({ "template": template, "at": [0, 0] }));
        assert_eq!(answer["template"], json!(template));
        assert_eq!(doc.model().element_count() - before, count, "{template}");
        run(&mut doc, "edit.undo", json!({}));
        assert_eq!(doc.model().element_count(), before, "{template}: one undo step");
        run(&mut doc, "edit.redo", json!({}));
    }
    let page = doc.model().pages()[0].clone();
    // The mind map is a map: three branches from its central topic.
    let map = kition_board_kdm::mindmap::Topology::read(doc.model(), &page);
    assert_eq!(map.edges.len(), 3);
    // Kanban notes sit in their columns, the columns in the template's frame; connectors are bound.
    let notes = doc.model().elements_of(&page).filter(|element| element.text == "Define release scope").count();
    assert_eq!(notes, 1);
    let scope = doc.model().elements_of(&page).find(|element| element.text == "Define release scope").unwrap().clone();
    let column = element(&doc, scope.parent.as_ref().unwrap().as_str());
    assert_eq!(column.kind, Kind::Frame);
    assert_eq!(element(&doc, column.parent.as_ref().unwrap().as_str()).kind, Kind::Frame);
    let flow = doc
        .model()
        .elements_of(&page)
        .filter(|element| element.kind == Kind::Connector)
        .filter(|element| element.start.as_ref().is_some_and(|end| end.binding.is_some()))
        .count();
    assert!(flow >= 3 + 5 + 7, "every template connector is bound at both ends ({flow})");
    // Centred on `at`: the storyboard's frame is 1000 × 650.
    let frames: Vec<_> =
        doc.model().elements_of(&page).filter(|element| element.kind == Kind::Frame && element.size.is_some_and(|size| size.w == 1000.0)).collect();
    assert_eq!(frames.len(), 1);
    assert!(close(frames[0].position.unwrap(), Point::new(-500.0, -325.0)), "{:?}", frames[0].position);
    fails(&mut doc, "template.insert", json!({ "template": "gantt" }), "invalid-params");
}

/// Every command says what it does, and every reading command an agent is offered declares what
/// it answers (kition-sdk `check_commands`); the answers match those output schemas.
#[test]
fn every_command_is_described_and_reading_commands_answer_their_output_schema() {
    let mut doc = board();
    kition_kit::testing::check_commands(&doc.commands(), &[]).unwrap();
    let made = run(&mut doc, "element.create", json!({ "elements": [{ "kind": "shape", "position": [0, 0], "size": [100, 60], "text": "A" }] }));
    let id = ids(&made)[0].clone();
    let page = doc.inspect().unwrap()["activePage"].as_str().map(str::to_owned);
    let mut answers = vec![
        ("clipboard.copy", run(&mut doc, "clipboard.copy", json!({ "ids": [id] }))),
        ("element.getStyle", run(&mut doc, "element.getStyle", json!({ "kind": "shape" }))),
        ("file.export", run(&mut doc, "file.export", json!({ "format": "svg" }))),
    ];
    if let Some(page) = page {
        answers.push(("page.activate", run(&mut doc, "page.activate", json!({ "page": page }))));
    }
    let commands = doc.commands();
    for (command, answer) in answers {
        let output = commands.iter().find(|c| c.id == command).and_then(|c| c.output_schema.clone()).unwrap();
        schema::validate_output(&output).unwrap();
        schema::check_params(&output["properties"], answer.as_object().unwrap()).unwrap_or_else(|error| panic!("{command}: {error}\n{answer}"));
    }
}

/// regressions/issue-coedit-local-edit-left-a-repair: a command never leaves a state the repair
/// (rules V1 to V9) would change. Moving a group's last members into a frame left the group empty
/// (V9); every other replica, and this one reopened after a crash, deleted it, so the replica
/// that made the edit disagreed with its own recovered copy (found by the convergence fuzz).
#[test]
fn a_local_edit_that_empties_a_group_leaves_nothing_to_repair() {
    let mut doc = board();
    let b = shape(&mut doc, 240.0, 40.0, 120.0, 80.0);
    let c = shape(&mut doc, 440.0, 40.0, 120.0, 80.0);
    let group = run(&mut doc, "element.group", json!({ "ids": [b, c] }))["group"].as_str().unwrap().to_owned();
    let frame = run(&mut doc, "frame.create", json!({ "box": [600, 500, 1000, 800] }))["frame"].as_str().unwrap().to_owned();
    run(&mut doc, "element.update", json!({ "id": b, "set": { "parent": frame } }));
    run(&mut doc, "element.update", json!({ "id": c, "set": { "parent": frame } }));
    assert!(doc.model().element(&ObjectId::from_string(&group)).is_none(), "the emptied group stayed");
    // Reopened, the replica repairs nothing.
    let snapshot = kition_kit::session::__snapshot(&doc).unwrap();
    let reopened = BoardDocument::open_with_replica("Plan.kboard", &[], &snapshot, &[]).unwrap();
    assert_eq!(state(&reopened), state(&doc));
    // One undo step brings the group and its members back.
    KitSessionDocument::undo(&mut doc).unwrap();
    assert_eq!(element(&doc, &c).parent.as_ref().map(ObjectId::as_str), Some(group.as_str()));
}
