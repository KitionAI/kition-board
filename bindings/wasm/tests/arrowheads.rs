//! `board.connectors.ends`: every arrowhead is drawn, at the end it is set on, with its tip on
//! the route's end (case `board.connectors/arrowheads`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kit::BoardDocument;
use kition_board_model::fields::ARROWHEADS;
use kition_kit::KitDocument;
use serde_json::{Value, json};

fn fragment(doc: &BoardDocument, id: &str) -> String {
    let answer: Value = serde_json::from_str(&doc.element_svg(&json!({ "ids": [id] }).to_string()).unwrap()).unwrap();
    answer["fragments"][0]["svg"].as_str().unwrap().to_owned()
}

fn line(doc: &mut BoardDocument, start: &str, end: &str) -> String {
    let made = doc
        .execute(
            "connector.create",
            json!({ "from": { "point": [100, 100] }, "to": { "point": [400, 100] }, "startArrowhead": start, "endArrowhead": end }),
        )
        .unwrap();
    fragment(doc, made["created"][0].as_str().unwrap())
}

/// Whether board point (x, y) is drawn dark.
fn dark(doc: &BoardDocument, x: f64, y: f64) -> bool {
    let px = doc.render(x, y, 1.0, 1, 1).unwrap();
    px[0] < 128 && px[1] < 128 && px[2] < 128
}

#[test]
fn there_are_nine_arrowheads_and_each_is_drawn_differently() {
    assert_eq!(ARROWHEADS.len(), 9);
    let mut doc = BoardDocument::create("Heads.kboard", 800, 600).unwrap();
    let mut seen: Vec<(&str, String)> = Vec::new();
    for head in ARROWHEADS {
        let svg = line(&mut doc, "none", head);
        assert!(!svg.contains("NaN"), "{head}: {svg}");
        for (other, drawn) in &seen {
            assert_ne!(&svg, drawn, "{head} is drawn as {other}");
        }
        seen.push((head, svg));
    }
}

#[test]
fn a_head_is_at_the_end_it_is_set_on() {
    let mut doc = BoardDocument::create("Heads.kboard", 800, 600).unwrap();
    assert_ne!(line(&mut doc, "diamond", "none"), line(&mut doc, "none", "diamond"));
    let refused = doc.execute("connector.create", json!({ "from": { "point": [0, 0] }, "to": { "point": [10, 0] }, "endArrowhead": "claw" }));
    assert!(format!("{refused:?}").contains("invalid-params"), "{refused:?}");
}

#[test]
fn a_zero_length_connector_with_any_head_is_drawn_without_nan() {
    let mut doc = BoardDocument::create("Heads.kboard", 800, 600).unwrap();
    for head in ARROWHEADS {
        let made = doc
            .execute(
                "connector.create",
                json!({ "from": { "point": [50, 50] }, "to": { "point": [50, 50] }, "startArrowhead": head, "endArrowhead": head }),
            )
            .unwrap();
        assert!(!fragment(&doc, made["created"][0].as_str().unwrap()).contains("NaN"), "{head}");
    }
}

#[test]
fn open_heads_are_hollow_and_closed_heads_are_filled() {
    // A black line of weight m (2.5) from (100, 300) to (400, 300): a head is 13 long; the open
    // circle's radius is 5. Each probe is inside the head, off the trimmed line.
    for (head, probe_x, filled) in [
        ("triangle", 391.0, true),
        ("triangle-open", 391.0, false),
        ("diamond", 387.0, true),
        ("diamond-open", 387.0, false),
        ("circle-open", 395.0, false),
    ] {
        let mut doc = BoardDocument::create("Heads.kboard", 800, 600).unwrap();
        doc.execute(
            "connector.create",
            json!({ "from": { "point": [100, 300] }, "to": { "point": [400, 300] }, "endArrowhead": head, "style": { "strokeColor": "#000000" } }),
        )
        .unwrap();
        assert_eq!(dark(&doc, probe_x, 300.0), filled, "{head} at x {probe_x}");
        assert!(dark(&doc, 250.0, 300.0), "{head}: the line is drawn");
        assert!(dark(&doc, 399.0, 300.0), "{head}: the head reaches the end");
    }
}

#[test]
fn an_open_head_is_the_paper_of_either_scheme() {
    // In the editor an open head is filled with the page's paper property, which the dark scheme
    // restyles; the dark raster fills it with the dark paper, under the light default ink.
    let mut doc = BoardDocument::create("Heads.kboard", 800, 600).unwrap();
    let made = doc
        .execute("connector.create", json!({ "from": { "point": [100, 300] }, "to": { "point": [400, 300] }, "endArrowhead": "triangle-open" }))
        .unwrap();
    assert!(fragment(&doc, made["created"][0].as_str().unwrap()).contains(r#"fill="var(--kb-paper, #ffffff)""#));
    let inside = doc.render_scheme("dark", 391.0, 300.0, 1.0, 1, 1).unwrap();
    let line = doc.render_scheme("dark", 250.0, 300.0, 1.0, 1, 1).unwrap();
    assert!(inside[0] < 64 && inside[1] < 64 && inside[2] < 64, "the open head is the dark paper: {inside:?}");
    assert!(line[0] > 192 && line[1] > 192 && line[2] > 192, "the line is the light ink: {line:?}");
}
