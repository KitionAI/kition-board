//! The engine's share of a commit at 5,000 elements (B10b, spike S2-1): a 20-shape `element.move`,
//! the `inspect()`, `width` and `height` the editor's worker asks for after every command, the
//! change export the page pulls, and the moved shapes' fragments. Before B10b the engine re-read
//! the whole view and walked every element of the page for children and bound connectors on each
//! command (6–15 ms natively, more in the page); now that work follows what changed. The budget is
//! the whole commit's (16 ms), for the engine's part alone, so it holds on a loaded machine and in
//! an unoptimised test build; `cargo test --release -p kition-board-kit --test commit_perf --
//! --nocapture` prints the times.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::time::Instant;

use kition_board_kdm::Kind;
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::session::KitSessionDocument;
use serde_json::{Value, json};

/// The 5,000-element board of spike S2 (`web/tests/flow.mjs`): 70% labelled flow nodes, 20%
/// connectors bound to a node, 10% strokes, 100 columns.
fn payload() -> Value {
    let mut elements = Vec::new();
    let mut last: Option<String> = None;
    for i in 0..5000 {
        let (c, r) = (i % 100, i / 100);
        let (x, y) = (f64::from(c) * 220.0, f64::from(r) * 140.0);
        let k = i % 10;
        if (k == 7 || k == 8) && last.is_some() {
            elements.push(json!({ "ref": format!("e{i}"), "kind": "connector", "start": { "point": [0, 0], "binding": { "target": last, "anchor": [1, 0.5] } }, "end": { "point": [x + 160.0, y + 100.0] } }));
        } else if k >= 7 {
            let points: Vec<Value> = (0..12).map(|j| json!([x + 10.0 + f64::from(j) * 13.0, y + 30.0 + f64::from(j % 2) * 40.0])).collect();
            elements.push(json!({ "ref": format!("e{i}"), "kind": "stroke", "points": points }));
        } else {
            elements.push(json!({ "ref": format!("e{i}"), "kind": "shape", "shapeStyle": "flow-node", "position": { "x": x + 30.0, "y": y + 30.0 }, "size": { "w": 160, "h": 80 }, "text": format!("Node {i}") }));
            last = Some(format!("e{i}"));
        }
    }
    json!({ "type": "application/x-kition-board+json", "version": 2, "elements": elements })
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

#[test]
fn a_20_shape_move_at_5000_elements_stays_in_budget() {
    let mut doc = BoardDocument::create("Perf.kboard", 1920, 1080).unwrap();
    doc.execute("page.create", json!({ "name": "Perf", "activate": true })).unwrap();
    let pasted = doc.execute("element.paste", json!({ "payload": payload() })).unwrap();
    let created: Vec<String> = pasted["created"].as_array().unwrap().iter().map(|id| id.as_str().unwrap().to_owned()).collect();
    assert_eq!(created.len(), 5000);
    for chunk in created.chunks(500) {
        doc.element_svg(&json!({ "ids": chunk, "lod": "full", "known": {} }).to_string()).unwrap();
    }
    let page = doc.active_page().unwrap().clone();
    let ids: Vec<String> = page
        .elements
        .iter()
        .filter(|id| doc.model().element(id).is_some_and(|element| element.kind == Kind::Shape))
        .take(20)
        .map(|id| id.as_str().to_owned())
        .collect();
    let (mut totals, mut executes) = (Vec::new(), Vec::new());
    for k in 0..15 {
        let since = doc.version();
        let started = Instant::now();
        let answer = doc.execute("element.move", json!({ "ids": ids, "delta": [if k % 2 == 1 { -6 } else { 6 }, 0] })).unwrap();
        let executed = ms(started);
        doc.inspect().unwrap();
        let _ = (doc.width(), doc.height());
        let changes = doc.session().export_changes(&since).unwrap();
        doc.element_svg(&json!({ "ids": ids, "lod": "full" }).to_string()).unwrap();
        totals.push(ms(started));
        executes.push(executed);
        assert_eq!(answer["moved"].as_array().unwrap().len(), 20);
        assert!(!changes.is_empty());
    }
    let (total, execute) = (median(totals), median(executes));
    println!("20-shape move at 5,000 elements: engine median {total:.2} ms (execute {execute:.2} ms)");
    assert!(total <= 16.0, "the engine's part of a commit took {total:.2} ms (median), over the 16 ms of S2-1");
}

/// One gesture is one undo step: undoing the move puts every shape back at once.
#[test]
fn one_move_is_one_undo_step_at_5000_elements() {
    let mut doc = BoardDocument::create("Perf.kboard", 1920, 1080).unwrap();
    doc.execute("page.create", json!({ "name": "Perf", "activate": true })).unwrap();
    doc.execute("element.paste", json!({ "payload": payload() })).unwrap();
    let page = doc.active_page().unwrap().clone();
    let ids: Vec<_> =
        page.elements.iter().filter(|id| doc.model().element(id).is_some_and(|element| element.kind == Kind::Shape)).take(20).cloned().collect();
    let before: Vec<_> = ids.iter().map(|id| doc.model().element(id).unwrap().position).collect();
    let names: Vec<&str> = ids.iter().map(|id| id.as_str()).collect();
    doc.execute("element.move", json!({ "ids": names, "delta": [30, 0] })).unwrap();
    doc.execute("edit.undo", json!({})).unwrap();
    let after: Vec<_> = ids.iter().map(|id| doc.model().element(id).unwrap().position).collect();
    assert_eq!(before, after);
}
