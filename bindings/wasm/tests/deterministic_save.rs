//! A document built by the same commands saves the same bytes in a reproducible build, as a
//! showcase sample or a gating test builds it: with kition-kit's `reproducible-ids` feature (a
//! dev-dependency only) and `KITION_REPRODUCIBLE_IDS` set, the replica's actor and the document id
//! come from a counter in the process instead of randomness. `SOURCE_DATE_EPOCH` alone leaves ids
//! random (kition-sdk 8d56412). Each build runs in a process of its own: this
//! test starts itself several times. Found rebuilding the showcase: every board sample saved
//! other bytes on each build.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use serde_json::json;

const OUT: &str = "KITION_BOARD_DETERMINISM_OUT";

fn build() -> Vec<u8> {
    let mut doc = BoardDocument::create("plan.kboard", 800, 600).unwrap();
    doc.execute(
        "element.create",
        json!({ "elements": [
            { "kind": "shape", "shapeType": "rectangle", "position": [100, 100], "size": [200, 120], "text": "Idea" },
            { "kind": "shape", "shapeType": "ellipse", "position": [400, 100], "size": [120, 120], "text": "Ship it" },
        ] }),
    )
    .unwrap();
    doc.prepare_save().unwrap();
    doc.save(Some("plan.kboard")).unwrap()
}

/// The child's half: builds the drawing into the file `OUT` names (nothing without it).
#[test]
fn child_builds_a_document() {
    if let Ok(path) = std::env::var(OUT) {
        std::fs::write(path, build()).unwrap();
    }
}

fn build_in_a_process(path: &PathBuf) -> Vec<u8> {
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_builds_a_document", "--test-threads=1", "--quiet"])
        .env("SOURCE_DATE_EPOCH", "1794182400")
        .env("KITION_REPRODUCIBLE_IDS", "1794182400")
        .env(OUT, path)
        .status()
        .unwrap();
    assert!(status.success(), "the child build failed");
    std::fs::read(path).unwrap()
}

#[test]
fn the_same_commands_save_the_same_bytes() {
    let dir = std::env::temp_dir().join(format!("kition-board-determinism-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let first = build_in_a_process(&dir.join("0.kboard"));
    for build in 1..4 {
        // A second apart, so a clock read would show.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let again = build_in_a_process(&dir.join(format!("{build}.kboard")));
        assert!(first == again, "build {build} saved other bytes than build 0");
    }
    std::fs::remove_dir_all(&dir).ok();
}
