//! C8-land-11-board: receipt bytes and the stored-child meaning of a container move.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod support;
use serde_json::{Value, json};
use support::*;

fn bytes(label: &str, receipt: &Value) {
    println!("receipt-bytes {label}: changes={} whole={}", receipt["changes"].to_string().len(), receipt.to_string().len());
    for list in ["changed", "moved"] {
        for entry in receipt["changes"][list].as_array().into_iter().flatten() {
            assert!(entry.get("carries").is_none(), "stored child changes are not derived carries: {receipt}");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn property_move_parent_and_uniform_offset_receipt_sizes() {
    let f = Fixture::new();
    let receipt = f.write(f.basis().await, "element.style", json!({"ids":[f.a],"style":{"fillColor":"blue"}})).await.unwrap();
    bytes("one-property", &receipt);
    let receipt = f.write(f.basis().await, "element.move", json!({"ids":[f.a],"delta":[10,20]})).await.unwrap();
    bytes("one-move", &receipt);
    f.shutdown().await;
    for count in [5, 200] {
        let f = Fixture::new();
        let elements = (1..count).map(|n| json!({"kind":"shape","parent":f.frame,"position":[f64::from(n)*5.0,450.0]})).collect::<Vec<_>>();
        f.outside(&[("element.create", json!({"elements":elements}))]);
        let receipt = f.write(f.basis().await, "element.move", json!({"ids":[f.frame],"delta":[10,20]})).await.unwrap();
        assert_eq!(receipt["completed"][0]["result"]["moved"].as_array().unwrap().len(), count as usize + 1);
        bytes(&format!("parent-{count}-children"), &receipt);
        f.shutdown().await;
    }
    let f = Fixture::new();
    let elements = (0..200).map(|n| json!({"kind":"shape","position":[f64::from(n)*5.0,800.0]})).collect::<Vec<_>>();
    f.outside(&[("element.create", json!({"elements":elements}))]);
    let doc = f.doc();
    let ids = doc
        .model()
        .pages()
        .iter()
        .flat_map(|page| doc.model().elements_of(page))
        .filter(|element| element.position.is_some_and(|point| point.y == 800.0))
        .map(|element| json!(element.id.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 200);
    let receipt = f.write(f.basis().await, "element.move", json!({"ids":ids,"delta":[10,20]})).await.unwrap();
    bytes("offset-200-objects", &receipt);
    f.shutdown().await;
}
