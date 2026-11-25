//! kition-sdk's contract check (`kition_kit::testing::check_contract`) on a real board file.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use serde_json::json;

/// A board titled `Plan` with one shape on its first page, as a `.kboard` file.
fn fixture() -> Vec<u8> {
    let mut doc = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    doc.execute("element.add", json!({ "elements": [{ "kind": "shape", "position": { "x": 40, "y": 40 }, "size": { "w": 160, "h": 100 } }] }))
        .unwrap();
    doc.save(None).unwrap()
}

#[test]
fn keeps_the_kit_sdk_contract() {
    kition_kit::testing::check_contract::<BoardDocument>(&fixture(), "Plan.kboard", |doc| {
        doc.execute("element.add", json!({ "elements": [{ "kind": "shape", "position": { "x": 300, "y": 200 }, "size": { "w": 240, "h": 120 } }] }))?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn opens_only_kboard_files_and_titles_new_boards_after_their_file() {
    assert!(BoardDocument::open("Plan.kiboard", &fixture()).is_err(), "the old suffix is not read");
    assert!(BoardDocument::open("Plan.kboard", b"{\"format\":\"kition-board\",\"version\":2}").is_err());
    let doc = BoardDocument::create("Roadmap.kboard", 1920, 1080).unwrap();
    let inspected = doc.inspect().unwrap();
    assert_eq!(inspected["title"], "Roadmap");
    assert_eq!(inspected["pages"].as_array().unwrap().len(), 1);
    assert_eq!(inspected["elements"], 0);
}
