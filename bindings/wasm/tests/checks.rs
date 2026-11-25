//! `board_check` against its catalogue (`agent/checks.json`, KHP 2.0 A38): every fixture in
//! `conformance/` makes the check report its code, every code the fixtures make is catalogued, and
//! the catalogue holds the twelve codes of the design note (5.3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::Path;

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use serde_json::Value;

/// The lent root is process-wide: tests that lend files take turns.
static LENDING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn repo() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

#[test]
fn every_fixture_reports_its_code() {
    let catalogue: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(repo().join("agent/checks.json")).unwrap()).unwrap();
    assert_eq!(catalogue.len(), 12);
    let _lending = LENDING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let codes: Vec<&str> = catalogue.iter().map(|entry| entry["code"].as_str().unwrap()).collect();
    // A card's target is checked against the lent files: lend a folder holding only the fixtures.
    let lent = std::env::temp_dir().join(format!("kition-board-checks-{}", std::process::id()));
    std::fs::create_dir_all(&lent).unwrap();
    kition_kit::host::set_root(Some(lent.clone()));
    kition_kit::host::set_folder("");
    let mut failures = Vec::new();
    for entry in &catalogue {
        let code = entry["code"].as_str().unwrap();
        assert!(code.starts_with("board.") && ["error", "warning", "info"].contains(&entry["severity"].as_str().unwrap()));
        let Some(fixture) = entry["fixture"].as_str() else { continue };
        let bytes = std::fs::read(repo().join("conformance").join(fixture)).unwrap_or_else(|error| panic!("{fixture}: {error}"));
        // As a conformance run does, the fixture is in the workspace (a card may point at it).
        std::fs::write(lent.join(fixture), &bytes).unwrap();
        let doc = BoardDocument::open(fixture, &bytes).unwrap();
        let found = doc.check();
        if !found.iter().any(|diagnostic| diagnostic.code == code && serde_json::to_value(diagnostic.severity).unwrap() == entry["severity"]) {
            failures.push(format!("{fixture} does not report {code}: {found:?}"));
        }
        for diagnostic in &found {
            if !codes.contains(&diagnostic.code.as_str()) {
                failures.push(format!("{fixture} reports {} outside the catalogue", diagnostic.code));
            }
        }
    }
    kition_kit::host::set_root(None);
    let _ = std::fs::remove_dir_all(&lent);
    assert!(failures.is_empty(), "{failures:#?}");
}

/// `basic.kboard`, the board the conformance suite edits, has nothing to report.
#[test]
fn the_conformance_basic_board_is_clean() {
    let bytes = std::fs::read(repo().join("conformance/basic.kboard")).unwrap();
    let doc = BoardDocument::open("basic.kboard", &bytes).unwrap();
    let found: Vec<String> = doc.check().into_iter().map(|diagnostic| diagnostic.code).collect();
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn the_sample_board_is_clean() {
    let bytes = std::fs::read(repo().join("web/public/sample/Sample.kboard")).unwrap();
    let doc = BoardDocument::open("Sample.kboard", &bytes).unwrap();
    let found: Vec<String> = doc.check().into_iter().map(|diagnostic| diagnostic.code).collect();
    assert_eq!(found, Vec::<String>::new());
}

/// The fix of `board.dangling-binding` clears it: attaching the detached end again (or leaving
/// the end free on purpose with `to: { point }`, which keeps it free) is what the check asks for.
#[test]
fn reattaching_a_detached_end_clears_the_dangling_binding() {
    let bytes = std::fs::read(repo().join("conformance/check-dangling-binding.kboard")).unwrap();
    let mut doc = BoardDocument::open("check-dangling-binding.kboard", &bytes).unwrap();
    let found = doc.check();
    let dangling = found.iter().find(|diagnostic| diagnostic.code == "board.dangling-binding").expect("reported after the repair");
    let fix = dangling.fix.clone().unwrap();
    let connector = fix.params.as_ref().unwrap()["id"].as_str().unwrap().to_owned();
    let model = doc.model();
    let start = model.element(&kition_kit::kdm::ObjectId::from_string(&connector)).unwrap().start.clone().unwrap().binding.unwrap().target;
    let target = doc.active_page().unwrap().elements.iter().find(|id| id.as_str() != connector && **id != start).unwrap().as_str().to_owned();
    doc.execute(&fix.command, serde_json::json!({ "id": connector, "terminal": "end", "to": { "element": target } })).unwrap();
    assert!(!doc.check().iter().any(|diagnostic| diagnostic.code == "board.dangling-binding"), "{:?}", doc.check());
}

/// A board card whose stored `docId` is the target's own is not reported as changed.
#[test]
fn a_card_on_the_board_it_was_made_for_is_not_changed() {
    let _lending = LENDING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let lent = std::env::temp_dir().join(format!("kition-board-changed-{}", std::process::id()));
    std::fs::create_dir_all(&lent).unwrap();
    let target = BoardDocument::create("Target.kboard", 1920, 1080).unwrap();
    std::fs::write(lent.join("Target.kboard"), target.save(None).unwrap()).unwrap();
    kition_kit::host::set_root(Some(lent.clone()));
    kition_kit::host::set_folder("");
    let mut doc = BoardDocument::create("Here.kboard", 1920, 1080).unwrap();
    let made = doc.execute("card.create", serde_json::json!({ "file": "Target.kboard", "position": [0, 0] })).unwrap();
    assert_eq!(made["card"]["target"]["docId"], serde_json::json!(target.doc_id()));
    let codes: Vec<String> = doc.check().into_iter().map(|diagnostic| diagnostic.code).collect();
    kition_kit::host::set_root(None);
    let _ = std::fs::remove_dir_all(&lent);
    assert!(!codes.iter().any(|code| code.starts_with("board.card-target")), "{codes:?}");
}
