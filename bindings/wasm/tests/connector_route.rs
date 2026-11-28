//! `connector.getRoute` (plan "Board usable by hand", stage 3; design note 5.1): how connectors
//! would be drawn, without writing anything. The editor's previews ask it while a connector is
//! drawn, while one of its ends is dragged and while the shapes it is bound to move, so each
//! preview here must be what the command it stands for then leaves on the board (cases
//! `board.editor.connector-route/engine`, `board.editor.connector-route/move`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kit::BoardDocument;
use kition_kit::session::KitSessionDocument;
use kition_kit::{KitDocument, schema};
use serde_json::{Value, json};

fn board() -> BoardDocument {
    BoardDocument::create("Route.kboard", 1920, 1080).unwrap()
}

fn run(doc: &mut BoardDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("{command} {params}: {error}"))
}

fn fails(doc: &mut BoardDocument, params: Value, code: &str) -> String {
    let error = doc.execute("connector.getRoute", params.clone()).err().unwrap_or_else(|| panic!("{params} should fail"));
    assert!(error.starts_with(code), "{params}: {error}");
    error
}

fn shape(doc: &mut BoardDocument, x: f64, y: f64, shape_type: &str) -> String {
    let made = run(doc, "element.add", json!({ "elements": [{ "kind": "shape", "shapeType": shape_type, "position": [x, y], "size": [160, 100] }] }));
    made["created"][0].as_str().unwrap().to_owned()
}

fn state(doc: &BoardDocument) -> Value {
    doc.session().replica().state_json().unwrap()
}

/// The board's own fragment of `id` (what the editor draws for it).
fn fragment(doc: &BoardDocument, id: &str) -> Value {
    let answer: Value = serde_json::from_str(&doc.element_svg(&json!({ "ids": [id] }).to_string()).unwrap()).unwrap();
    answer["fragments"][0].clone()
}

fn route(doc: &mut BoardDocument, params: Value) -> Vec<Value> {
    let answer = run(doc, "connector.getRoute", params);
    let output = doc.commands().into_iter().find(|command| command.id == "connector.getRoute").and_then(|command| command.output_schema).unwrap();
    schema::validate_output(&output).unwrap();
    schema::check_params(&output["properties"], answer.as_object().unwrap()).unwrap_or_else(|error| panic!("{error}\n{answer}"));
    answer["connectors"].as_array().unwrap().clone()
}

fn points(value: &Value) -> Vec<(f64, f64)> {
    value.as_array().unwrap().iter().map(|p| (p[0].as_f64().unwrap(), p[1].as_f64().unwrap())).collect()
}

fn close(a: &[(f64, f64)], b: &[(f64, f64)]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(p, q)| (p.0 - q.0).abs() < 1e-6 && (p.1 - q.1).abs() < 1e-6)
}

/// A new connector between two shapes: the preview is what connector.add then draws, and asking
/// changes nothing (not the state, not the revision).
#[test]
fn a_new_connector_is_drawn_as_connector_add_draws_it() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, "rectangle");
    let b = shape(&mut doc, 400.0, 200.0, "diamond");
    for (kind, ends) in [
        ("straight", json!({ "from": { "element": a }, "to": { "element": b } })),
        ("elbow", json!({ "from": { "element": a, "anchor": [1, 0.5] }, "to": { "element": b, "anchor": [0.5, 0] } })),
        ("curved", json!({ "from": { "element": a }, "to": { "point": [700, 40] } })),
    ] {
        let mut entry = ends.clone();
        entry["type"] = json!(kind);
        entry["label"] = json!("yes");
        let (before, revision) = (state(&doc), doc.revision());
        let preview = route(&mut doc, json!({ "connectors": [entry] }))[0].clone();
        assert_eq!(state(&doc), before, "{kind}: a preview writes nothing");
        assert_eq!(doc.revision(), revision, "{kind}: a preview is no change");
        let mut add = ends.clone();
        add["type"] = json!(kind);
        add["label"] = json!("yes");
        let made = run(&mut doc, "connector.add", add);
        let id = made["created"][0].as_str().unwrap().to_owned();
        let drawn = fragment(&doc, &id);
        assert_eq!(preview["svg"], drawn["svg"], "{kind}: the preview is the connector the command draws");
        assert_eq!(preview["box"], drawn["box"], "{kind}");
        assert_eq!(preview["text"], drawn["text"], "{kind}: the label sits where the command puts it");
        assert_eq!(preview["start"]["point"], made["start"], "{kind}: the start resolves as connector.add resolves it");
        assert_eq!(preview["end"]["point"], made["end"], "{kind}");
        let stored: Vec<Value> =
            doc.model().element(&kition_kit::kdm::ObjectId::from_string(&id)).iter().map(|e| e.start.as_ref().unwrap().to_json()).collect();
        assert_eq!(preview["start"]["binding"], stored[0]["binding"], "{kind}: the same anchor");
    }
}

/// The corners an elbow turns at, the arrowhead at the end, nothing at a start without one.
#[test]
fn corners_and_arrowheads_are_answered() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, "rectangle");
    let b = shape(&mut doc, 400.0, 300.0, "rectangle");
    let answer = route(
        &mut doc,
        json!({ "connectors": [{ "from": { "element": a, "anchor": [1, 0.5] }, "to": { "element": b, "anchor": [0, 0.5] }, "type": "elbow" }] }),
    );
    let corners = points(&answer[0]["points"]);
    assert_eq!(corners, vec![(160.0, 50.0), (280.0, 50.0), (280.0, 350.0), (400.0, 350.0)], "an elbow turns halfway across");
    let heads = answer[0]["heads"].as_array().unwrap();
    assert_eq!(heads.len(), 1, "only the end has a head by default: {heads:?}");
    assert_eq!(heads[0]["end"], "end");
    assert_eq!(heads[0]["kind"], "arrow");
    assert_eq!(points(&json!([heads[0]["tip"]])), vec![(400.0, 350.0)]);
    assert_eq!(points(&json!([heads[0]["direction"]])), vec![(1.0, 0.0)], "it arrives from the left");
    assert!(answer[0]["path"].as_str().unwrap().starts_with("M 160 50"));
    let both = route(
        &mut doc,
        json!({ "connectors": [{ "from": { "point": [0, 500] }, "to": { "point": [300, 500] }, "startArrowhead": "dot", "endArrowhead": "none" }] }),
    );
    let heads = both[0]["heads"].as_array().unwrap();
    assert_eq!(heads.len(), 1, "{heads:?}");
    assert_eq!(heads[0]["end"], "start");
    assert_eq!(heads[0]["kind"], "dot");
    assert!(both[0].get("start").unwrap().get("binding").is_none(), "free ends have no binding");
}

/// Shapes moved by `move`: the bound connectors are drawn where element.move then leaves them, in
/// one call for every connector, and a carried connector's free end and bends shift with it.
#[test]
fn a_move_previews_the_connectors_the_move_leaves() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, "rectangle");
    let b = shape(&mut doc, 400.0, 200.0, "ellipse");
    let c = shape(&mut doc, 0.0, 400.0, "hexagon");
    let ab = run(&mut doc, "connector.add", json!({ "from": { "element": a }, "to": { "element": b }, "type": "elbow", "label": "go" }))["created"]
        [0]
    .clone();
    let ac =
        run(&mut doc, "connector.add", json!({ "from": { "element": a, "anchor": [0.5, 1] }, "to": { "element": c }, "type": "curved" }))["created"]
            [0]
        .clone();
    let free =
        run(&mut doc, "connector.add", json!({ "from": { "element": a }, "to": { "point": [-200, 300] }, "waypoints": [[-100, 200]] }))["created"][0]
            .clone();
    let moving = json!([a, free]);
    let delta = json!([120, -35]);
    let previews = route(&mut doc, json!({ "connectors": [{ "id": ab }, { "id": ac }, { "id": free }], "move": { "ids": moving, "delta": delta } }));
    assert_eq!(previews.iter().map(|p| p["id"].clone()).collect::<Vec<_>>(), vec![ab.clone(), ac.clone(), free.clone()]);
    run(&mut doc, "element.move", json!({ "ids": moving, "delta": delta }));
    for (preview, id) in previews.iter().zip([&ab, &ac, &free]) {
        let id = id.as_str().unwrap();
        let after = route(&mut doc, json!({ "connectors": [{ "id": id }] }))[0].clone();
        assert!(close(&points(&preview["points"]), &points(&after["points"])), "{id}: {} vs {}", preview["points"], after["points"]);
        let drawn = fragment(&doc, id);
        let (x, y) = (drawn["box"][0].as_f64().unwrap(), preview["box"][0].as_f64().unwrap());
        assert!((x - y).abs() < 1e-6, "{id}: the drawing covers the same box");
        assert_eq!(after["svg"], drawn["svg"], "{id}: a stored connector's preview is its fragment");
    }
    let shifted = points(&previews[2]["points"]);
    assert!(close(&shifted[1..], &[(20.0, 165.0), (-80.0, 265.0)]), "the carried connector's bend and free end shift: {shifted:?}");
}

/// One end of a stored connector given: connector.bind of that end. The label and the style stay
/// (finding F6.8 on the engine's side).
#[test]
fn one_end_given_is_a_bind_that_keeps_label_and_style() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, "rectangle");
    let b = shape(&mut doc, 400.0, 0.0, "rectangle");
    let c = shape(&mut doc, 400.0, 400.0, "ellipse");
    let id = run(
        &mut doc,
        "connector.add",
        json!({ "from": { "element": a }, "to": { "element": b }, "label": "yes", "style": { "strokeColor": "red", "dashStyle": "dashed" } }),
    )["created"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let preview = route(&mut doc, json!({ "connectors": [{ "id": id, "to": { "element": c } }] }))[0].clone();
    run(&mut doc, "connector.bind", json!({ "id": id, "terminal": "end", "to": { "element": c } }));
    let drawn = fragment(&doc, &id);
    assert_eq!(preview["svg"], drawn["svg"], "the preview of a re-attached end is what connector.bind leaves");
    assert!(drawn["svg"].as_str().unwrap().contains(">yes<"), "the label is still drawn after the end moved to another shape");
    assert!(drawn["svg"].as_str().unwrap().contains("stroke-dasharray"), "and the dashes");
    // Both ends on one element are refused, as connector.bind refuses them.
    fails(&mut doc, json!({ "connectors": [{ "id": id, "from": { "element": c } }] }), "invalid-params");
}

/// Hostile and malformed requests answer typed errors and change nothing.
#[test]
fn bad_requests_are_refused_with_typed_reasons() {
    let mut doc = board();
    let a = shape(&mut doc, 0.0, 0.0, "rectangle");
    let page = run(&mut doc, "page.add", json!({}))["page"].as_str().unwrap().to_owned();
    let elsewhere =
        run(&mut doc, "element.add", json!({ "page": page, "elements": [{ "kind": "shape", "position": [0, 0], "size": [10, 10] }] }))["created"][0]
            .clone();
    let before = state(&doc);
    let point = json!({ "point": [0, 0] });
    for params in [
        json!({}),
        json!({ "connectors": [] }),
        json!({ "connectors": "x" }),
        json!({ "connectors": [5] }),
        json!({ "connectors": [{ "from": point }] }),
        json!({ "connectors": [{ "from": point, "to": point, "bend": 1 }] }),
        json!({ "connectors": [{ "from": { "element": a }, "to": { "element": a } }] }),
        json!({ "connectors": [{ "from": { "element": a }, "to": { "element": elsewhere } }] }),
        json!({ "connectors": [{ "from": point, "to": { "point": [1e300, 0] } }] }),
        json!({ "connectors": [{ "from": point, "to": point, "labelPosition": 2 }] }),
        json!({ "connectors": [{ "from": point, "to": point, "type": "zigzag" }] }),
        json!({ "connectors": [{ "from": point, "to": point, "style": { "fillColor": "red" } }] }),
        json!({ "connectors": [{ "from": point, "to": point, "waypoints": [[0]] }] }),
        json!({ "connectors": [{ "id": a }] }),
        json!({ "connectors": [{ "from": point, "to": point }], "move": { "ids": [a], "delta": [1e300, 0] } }),
        json!({ "connectors": [{ "from": point, "to": point }], "move": { "ids": [], "delta": [1, 0] } }),
    ] {
        fails(&mut doc, params, "invalid-params");
    }
    for params in [
        json!({ "connectors": [{ "id": "missing" }] }),
        json!({ "connectors": [{ "from": { "element": "missing" }, "to": point }] }),
        json!({ "connectors": [{ "from": point, "to": point }], "move": { "ids": ["missing"], "delta": [1, 0] } }),
    ] {
        fails(&mut doc, params, "not-found");
    }
    let many: Vec<Value> = (0..501).map(|_| json!({ "from": point, "to": { "point": [10, 0] } })).collect();
    fails(&mut doc, json!({ "connectors": many }), "invalid-params");
    let most: Vec<Value> = (0..500).map(|i| json!({ "from": point, "to": { "point": [10, i] } })).collect();
    assert_eq!(route(&mut doc, json!({ "connectors": most })).len(), 500, "500 in one call");
    assert_eq!(state(&doc), before, "nothing a refused or answered request did changed the board");
    // A zero-length connector has no path and no head, and no NaN anywhere.
    let zero = route(&mut doc, json!({ "connectors": [{ "from": point, "to": point }] }));
    assert!(!zero[0].to_string().contains("NaN"), "{}", zero[0]);
}
