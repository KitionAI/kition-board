//! C8-land-12-board: published numeric domains and atomic JSON member validation.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod support;
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use serde_json::json;
use support::*;

#[test]
fn command_schemas_publish_engine_numeric_limits() {
    let doc = BoardDocument::create("Limits.kboard", 800, 600).unwrap();
    let commands = doc.commands();
    for (command, path, min, max) in [
        ("element.rotate", "/degrees", -3600.0, 3600.0),
        ("frame.fit", "/padding", 0.0, 10000.0),
        ("layout.stack", "/gap", 0.0, 10000.0),
        ("connector.setLabel", "/position", 0.0, 1.0),
        ("file.export", "/scale", 0.1, 8.0),
        ("file.render", "/maxSize", 16.0, 8192.0),
        ("element.style", "/style/properties/opacity", 0.0, 1.0),
        ("element.style", "/style/properties/fontSize", 1.0, 1000.0),
        ("element.style", "/style/properties/roundness", 0.0, 100000.0),
        ("element.move", "/delta/oneOf/0/items", -1000000.0, 1000000.0),
        ("image.insert", "/size/oneOf/1/properties/w", 0.0, 100000.0),
    ] {
        let node = commands.iter().find(|c| c.id == command).unwrap().schema.as_ref().unwrap().pointer(path).unwrap();
        assert_eq!((node["minimum"].as_f64(), node["maximum"].as_f64()), (Some(min), Some(max)), "{command}{path}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn json_member_bounds_apply_to_whole_values_and_paths_atomically() {
    let f = Fixture::new();
    for (field, member, low, high, whole) in [
        ("position", "x", -1000000.0, 1000000.0, json!({"x":0,"y":0})),
        ("position", "y", -1000000.0, 1000000.0, json!({"x":0,"y":0})),
        ("size", "w", 0.0, 100000.0, json!({"w":160,"h":100})),
        ("size", "h", 0.0, 100000.0, json!({"w":160,"h":100})),
    ] {
        for path in [false, true] {
            for number in [low - 1.0, high + 1.0] {
                let mut value = whole.clone();
                value[member] = json!(number);
                let before = f.file();
                let error = f.call("batch", json!({"ops":[
                    {"op":"set","target":f.b,"field":"opacity","value":0.5},
                    {"op":"set","target":f.a,"field":if path {format!("{field}.{member}")} else {field.to_owned()},"value":if path {json!(number)} else {value}}
                ]})).await.unwrap_err();
                assert_eq!(error["error"]["code"], "invalid-value", "{error}");
                assert_eq!(f.file(), before);
            }
        }
        for number in [low, high] {
            let mut value = whole.clone();
            value[member] = json!(number);
            f.ok("batch", json!({"ops":[{"op":"set","target":f.a,"field":field,"value":value}]})).await;
        }
    }
    f.shutdown().await;
}

/// `kind` is a creation-only register (kition-sdk call-context.md, "Ordered Places And Creation",
/// item 2): a raw create must give it in `create.fields`, and once made it stays read-only.
#[tokio::test(flavor = "multi_thread")]
async fn immutable_kind_is_given_at_creation_and_read_only_after() {
    let f = Fixture::new();
    let before = f.file();
    let error = f
        .call(
            "batch",
            json!({"ops":[
                {"op":"create","type":"Element","in":format!("{}.elements",f.page.as_str().unwrap()),"ref":"$new","fields":{}},
                {"op":"text.set","target":"$new","field":"text","text":"New"}
            ]}),
        )
        .await
        .unwrap_err();
    println!("SDK sampled creation without immutable kind: {error}");
    assert!(error.to_string().contains("kind"), "{error}");
    assert_eq!(f.file(), before);
    let made = f
        .ok(
            "batch",
            json!({"ops":[
                {"op":"create","type":"Element","in":format!("{}.elements",f.page.as_str().unwrap()),"fields":{"kind":"shape"}}
            ]}),
        )
        .await;
    let created = &made["changes"]["created"][0];
    assert_eq!(created["values"]["kind"], "shape", "{made}");
    let id = created["id"].clone();
    assert!(f.doc().model().element(&kition_kit::kdm::ObjectId::from_string(id.as_str().unwrap())).is_some(), "{made}");
    let before = f.file();
    let error = f.call("batch", json!({"ops":[{"op":"set","target":id,"field":"kind","value":"group"}]})).await.unwrap_err();
    assert_eq!(error["error"]["code"], "read-only", "{error}");
    assert_eq!(f.file(), before);
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn page_creation_and_rich_text_edits_replay_without_derived_sets() {
    let f = Fixture::new();
    f.ok(
        "batch",
        json!({"ops":[
            {"op":"text.set","target":f.a,"field":"text","text":"New"},
            {"op":"text.mark","target":f.a,"field":"text","start":0,"end":3,"key":"bold","value":true}
        ]}),
    )
    .await;
    let doc = f.doc();
    let element = doc.model().element(&kition_kit::kdm::ObjectId::from_string(f.a.as_str().unwrap())).unwrap();
    assert_eq!(element.text, "New");
    assert!(element.runs.iter().any(|run| run.attributes.get("bold") == Some(&json!(true))));
    let page = f.ok("batch", json!({"ops":[{"op":"create","type":"Page","in":"root.pages","fields":{"name":"Replay"}}]})).await;
    assert_eq!(page["changes"]["created"][0]["values"]["name"], "Replay");
    assert_eq!(f.doc().model().pages().len(), 3);
    f.shutdown().await;
}
