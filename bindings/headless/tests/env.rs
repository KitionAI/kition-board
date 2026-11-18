//! C8-land-11-board: the object model tells the environment the engine's numeric domains.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod support;
use serde_json::json;
use support::*;

#[tokio::test(flavor = "multi_thread")]
async fn batches_enforce_the_same_scalar_limits_as_commands_atomically() {
    let f = Fixture::new();
    for (field, low, high) in
        [("opacity", 0.0, 1.0), ("labelPosition", 0.0, 1.0), ("rotation", -3600.0, 3600.0), ("roundness", 0.0, 100000.0), ("fontSize", 1.0, 1000.0)]
    {
        for value in [low - 0.25, high + 0.25] {
            let before = f.file();
            let result = f
                .call(
                    "batch",
                    json!({"ops":[
                        {"op":"set","target":f.b,"field":"opacity","value":0.5},
                        {"op":"set","target":f.a,"field":field,"value":value}
                    ]}),
                )
                .await;
            let error = result.unwrap_err();
            assert_eq!(error["error"]["code"], "invalid-value", "{field}: {error}");
            assert_eq!(f.file(), before, "a rejected batch is atomic");
        }
        for value in [low, high] {
            f.ok("batch", json!({"ops":[{"op":"set","target":f.a,"field":field,"value":value}]})).await;
        }
    }
    let before = f.file();
    let error = f.call("batch", json!({"ops":[{"op":"set","target":f.a,"field":"kind","value":"group"}]})).await.unwrap_err();
    assert!(error.to_string().contains("read-only"), "{error}");
    assert_eq!(f.file(), before);
    f.shutdown().await;
}

/// Minimal SDK 1935d52 kit-env reproduction: ops_for emits a stored-field set for
/// every derived receipt edit before replaying its creation. That replay is refused.
#[tokio::test(flavor = "multi_thread")]
async fn environment_create_discard_reproductions() {
    let f = Fixture::new();
    let created = f.ok("batch", json!({"ops":[{"op":"create","type":"Page","in":"root.pages","at":1,"fields":{"name":"New"}}]})).await;
    let root = created["changes"]["changed"].as_array().unwrap().iter().find(|change| change["id"] == "board").unwrap();
    let edit = root["edits"].as_array().unwrap().iter().find(|edit| edit["field"] == "~pageCount").unwrap();
    let before = f.file();
    let error = f.call("batch", json!({"ops":[{"op":"set","target":"root","field":edit["field"],"value":edit["after"]}]})).await.unwrap_err();
    assert!(error.to_string().contains("~pageCount"), "{error}");
    assert_eq!(f.file(), before);
    println!("SDK environment page create replay: {error}");
    // The generic create sampler supplies a rich-text label as a scalar create field.
    let error = f
        .call("batch", json!({"ops":[{"op":"create","type":"Element","in":format!("{}.elements",f.page.as_str().unwrap()),"fields":{"text":"New"}}]}))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("field `text` of `Element` is a container"), "{error}");
    assert_eq!(f.file(), before);
    println!("SDK environment Element create with rich text: {error}");
    f.shutdown().await;
}
