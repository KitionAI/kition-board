//! C8-land-11-board: stable page context through the real agent tools.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod support;
use serde_json::json;
use support::*;

#[tokio::test(flavor = "multi_thread")]
async fn uuid_page_context_reaches_the_page_the_agent_named() {
    let f = Fixture::new();
    let ran = f
        .ok(
            "run",
            json!({"target":{"page":f.other_page},"steps":[
                {"command":"element.add","params":{"elements":[{"kind":"shape","position":[50,50],"text":"Explicit page"}]}}
            ]}),
        )
        .await;
    assert_eq!(ran["saved"], true);
    let doc = f.doc();
    assert_eq!(doc.model().pages()[0].elements.len(), 4);
    assert_eq!(doc.model().pages()[1].elements.len(), 1);
    f.shutdown().await;
}
