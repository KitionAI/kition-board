//! C8-land-11-board: a list premise does not protect fields of its addressable members.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod support;
use serde_json::json;
use support::*;

#[tokio::test(flavor = "multi_thread")]
async fn mindmap_holds_node_fields_it_reads_but_allows_unrelated_labels() {
    for change in 0..4 {
        let f = Fixture::new();
        f.outside(&[
            ("element.set", json!({"id":f.a,"set":{"shapeStyle":"mind-node"}})),
            ("mindmap.addChild", json!({"node":f.a,"text":"Map child"})),
        ]);
        let doc = f.doc();
        let child = json!(
            doc.model()
                .pages()
                .iter()
                .flat_map(|page| doc.model().elements_of(page))
                .find(|element| element.text == "Map child")
                .unwrap()
                .id
                .as_str()
        );
        let basis = f.basis().await;
        match change {
            0 => f.outside(&[("element.setText", json!({"id":f.b,"text":"Unrelated"}))]),
            1 => f.outside(&[("element.move", json!({"ids":[child],"delta":[40,0]}))]),
            2 => f.outside(&[("element.setText", json!({"id":f.a,"text":"Root label"}))]),
            _ => f.outside(&[("element.set", json!({"id":child,"set":{"shapeStyle":"default"}}))]),
        }
        let before = f.file();
        let result = f.write(basis, "layout.mindmap", json!({"root":f.a})).await;
        if change == 1 || change == 3 {
            let error = result.unwrap_err();
            if change == 1 {
                conflict(&error, &child, "relocated");
            } else {
                conflict(&error, &json!("board"), "changed");
            }
            assert_eq!(f.file(), before);
        } else {
            result.unwrap();
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn page_copy_and_delete_hold_the_addressable_elements_they_consume() {
    for command in ["page.duplicate", "page.delete"] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.outside(&[("element.style", json!({"ids":[f.a],"style":{"fillColor":"red"}}))]);
        let before = f.file();
        let error = f.write(basis, command, json!({"page":f.page})).await.unwrap_err();
        conflict(&error, &f.a, "changed");
        assert_eq!(f.file(), before);
        f.shutdown().await;
    }
}
