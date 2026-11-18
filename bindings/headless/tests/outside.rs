//! C8-land-11-board: independent writers replace the file between observation and writing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod support;
use kition_kit::kdm::ObjectId;
use kition_kits::tools::{CallOptions, Guard};
use serde_json::{Value, json};
use support::*;

fn fill(id: &Value, color: &str) -> Value {
    json!({"ids":[id],"style":{"fillColor":color}})
}

#[tokio::test(flavor = "multi_thread")]
async fn unrelated_fields_apply_and_others_names_only_the_changed_object() {
    for same in [false, true] {
        let f = Fixture::new();
        let basis = f.basis().await;
        let other = if same { &f.a } else { &f.b };
        f.outside(&[("element.setText", json!({"id":other,"text":"Colleague"}))]);
        let ran = f.write(basis, "element.style", fill(&f.a, "blue")).await.unwrap();
        assert_eq!(
            ran["others"]["changed"].as_array().unwrap().iter().map(|item| item["id"].clone()).collect::<Vec<_>>(),
            vec![other.clone()],
            "{ran}"
        );
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_field_or_deleted_target_is_refused_without_writing() {
    for deleted in [false, true] {
        let f = Fixture::new();
        let basis = f.basis().await;
        if deleted {
            f.outside(&[("element.delete", json!({"ids":[f.a]}))]);
        } else {
            f.outside(&[("element.style", fill(&f.a, "red"))]);
        }
        let before = f.file();
        let refused = f.write(basis, "element.style", fill(&f.a, "blue")).await.unwrap_err();
        conflict(&refused, &f.a, if deleted { "deleted" } else { "changed" });
        assert_eq!(f.file(), before);
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn destination_frame_moved_away_refuses_the_point_addressed_reparent() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.outside(&[("element.move", json!({"ids":[f.frame],"delta":[800,0]}))]);
    let before = f.file();
    let refused = f.write(basis, "element.move", json!({"ids":[f.a],"delta":[50,400],"reparent":true})).await.unwrap_err();
    conflict(&refused, &f.frame, "relocated");
    assert_eq!(f.file(), before);
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn equal_area_frame_order_changes_are_not_replayed_as_if_unchanged() {
    let f = Fixture::new();
    f.outside(&[("frame.create", json!({"page":f.page,"box":[0,400,500,700],"title":"Second"}))]);
    let basis = f.basis().await;
    f.outside(&[("element.reorder", json!({"ids":[f.frame],"placement":"front"}))]);
    let before = f.file();
    let refused = f.write(basis, "element.move", json!({"ids":[f.a],"delta":[50,400],"reparent":true})).await.unwrap_err();
    assert_eq!(refused["error"]["code"], "conflict", "{refused}");
    assert_eq!(f.file(), before);
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn insertion_before_an_index_refuses_but_insertion_after_applies() {
    for before in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.outside(&[("page.create", json!({"at":if before {0} else {2},"name":"Colleague"}))]);
        let bytes = f.file();
        let answer = f.write(basis, "page.create", json!({"at":1,"name":"Agent"})).await;
        if before {
            assert_eq!(answer.unwrap_err()["error"]["code"], "conflict");
            assert_eq!(f.file(), bytes);
        } else {
            answer.unwrap();
            let doc = f.doc();
            assert_eq!(doc.model().pages().iter().map(|page| page.name.as_str()).collect::<Vec<_>>(), ["Page 1", "Agent", "Other", "Colleague"]);
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_move_allows_a_field_write_by_id_and_refuses_a_point_dependent_move() {
    for point in [false, true] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.outside(&[("element.move", json!({"ids":[f.a],"delta":[800,0]}))]);
        let bytes = f.file();
        if point {
            let error = f.write(basis, "element.move", json!({"ids":[f.a],"delta":[50,400],"reparent":true})).await.unwrap_err();
            conflict(&error, &f.a, "relocated");
            assert_eq!(f.file(), bytes);
        } else {
            f.write(basis, "element.style", fill(&f.a, "blue")).await.unwrap();
            assert_eq!(f.doc().model().element(&ObjectId::from_string(f.a.as_str().unwrap())).unwrap().register("fillColor"), Some(&json!("blue")));
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_parents_move_refuses_a_child_move_but_allows_its_property_write() {
    for moving in [false, true] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.outside(&[("element.move", json!({"ids":[f.frame],"delta":[20,0]}))]);
        let bytes = f.file();
        if moving {
            let error = f.write(basis, "element.move", json!({"ids":[f.child],"delta":[10,0]})).await.unwrap_err();
            conflict(&error, &f.child, "relocated");
            assert_eq!(f.file(), bytes);
        } else {
            f.write(basis, "element.style", fill(&f.child, "blue")).await.unwrap();
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn name_swaps_do_not_change_uuid_addresses_and_names_are_not_addresses() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.outside(&[("page.rename", json!({"page":f.page,"name":"Other"})), ("page.rename", json!({"page":f.other_page,"name":"Page 1"}))]);
    f.write(basis, "element.style", fill(&f.a, "blue")).await.unwrap();
    let bytes = f.file();
    let basis = f.basis().await;
    let error = f.write(basis, "page.rename", json!({"page":"Other","name":"Agent"})).await.unwrap_err();
    assert_ne!(error["error"]["code"], "conflict", "a name is an invalid UUID address: {error}");
    assert_eq!(f.file(), bytes);
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn rename_away_keeps_the_page_uuid_and_rejects_the_old_name() {
    let f = Fixture::new();
    let basis = f.basis().await;
    f.outside(&[("page.rename", json!({"page":f.page,"name":"Away"}))]);
    let bytes = f.file();
    let error = f.write(basis.clone(), "page.rename", json!({"page":"Page 1","name":"Agent"})).await.unwrap_err();
    assert_ne!(error["error"]["code"], "conflict", "names are not resolved: {error}");
    assert_eq!(f.file(), bytes);
    let error = f.write(basis, "page.rename", json!({"page":f.page,"name":"Agent"})).await.unwrap_err();
    conflict(&error, &f.page, "changed");
    assert_eq!(f.file(), bytes);
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn epoch_premise_and_off_have_their_declared_meaning() {
    for (guard, same, refused) in
        [(Guard::Premise, false, false), (Guard::Premise, true, true), (Guard::Epoch, false, true), (Guard::Off, true, false)]
    {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.outside(&[("element.style", fill(if same { &f.a } else { &f.b }, "red"))]);
        let bytes = f.file();
        let result = f
            .call_with(
                "run",
                json!({"basis":basis,"steps":[{"command":"element.style","params":fill(&f.a,"blue")}]}),
                CallOptions { guard, ..CallOptions::default() },
            )
            .await;
        assert_eq!(result.is_err(), refused, "{guard:?}: {result:?}");
        if refused {
            assert_eq!(result.unwrap_err()["error"]["code"], "conflict");
            assert_eq!(f.file(), bytes);
        }
        f.shutdown().await;
    }
}
