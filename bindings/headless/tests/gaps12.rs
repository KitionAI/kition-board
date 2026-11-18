//! C8-land-12-board: retained SDK regression for deleted page positions in empty windows.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod support;
use serde_json::json;
use support::*;

#[tokio::test(flavor = "multi_thread")]
async fn page_gaps_track_removal_and_movement_before_but_allow_later_edits() {
    let mut failures = Vec::new();
    for moving in [false, true] {
        for change in ["remove-before", "move-before", "remove-after", "move-after"] {
            let f = Fixture::new();
            f.outside(&[
                ("page.create", json!({"name":"Third"})),
                ("page.create", json!({"name":"Fourth"})),
                ("page.create", json!({"name":"Fifth"})),
            ]);
            let doc = f.doc();
            let third = json!(doc.model().pages()[2].id.as_str());
            let fifth = json!(doc.model().pages()[4].id.as_str());
            let basis = f.basis().await;
            let step = match change {
                "remove-before" => ("page.delete", json!({"page":f.page})),
                "move-before" => ("page.move", json!({"page":f.page,"to":2})),
                "remove-after" => ("page.delete", json!({"page":fifth})),
                _ => ("page.move", json!({"page":fifth,"to":3})),
            };
            f.outside(&[step]);
            let bytes = f.file();
            let (command, params) =
                if moving { ("page.move", json!({"page":third,"to":1})) } else { ("page.create", json!({"name":"Agent","at":1})) };
            let result = f.write(basis, command, params).await;
            let valid = if change.ends_with("before") {
                result.as_ref().err().is_some_and(|error| error["error"]["code"] == "conflict") && f.file() == bytes
            } else {
                result.is_ok()
            };
            println!(
                "{command}/{change}: expected {}, got {}",
                if change.ends_with("before") { "conflict" } else { "applied" },
                if result.is_ok() { "applied" } else { "refused" }
            );
            if !valid {
                failures.push(format!("{command}/{change}: {result:?}"));
            }
            f.shutdown().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
