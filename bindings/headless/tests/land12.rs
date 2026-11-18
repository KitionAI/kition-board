//! C8-land-12-board: refuse changed dependencies and allow unrelated saved-file edits.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod support;
use kition_kit::kdm::ObjectId;
use serde_json::json;
use support::*;

#[tokio::test(flavor = "multi_thread")]
async fn container_membership_ignores_isolated_creation_but_holds_new_children() {
    for child in [false, true] {
        let f = Fixture::new();
        let basis = f.basis().await;
        let mut element = json!({"kind":"shape","position":[900,0],"text":"Colleague"});
        if child {
            element["parent"] = f.frame.clone();
        }
        f.outside(&[("element.create", json!({"page":f.page,"elements":[element]}))]);
        let before = f.file();
        let result = f.write(basis, "element.move", json!({"ids":[f.frame],"delta":[20,0]})).await;
        if child {
            assert_eq!(result.unwrap_err()["error"]["code"], "conflict");
            assert_eq!(f.file(), before);
        } else {
            result.unwrap();
            assert_eq!(f.doc().model().element(&ObjectId::from_string(f.child.as_str().unwrap())).unwrap().position.unwrap().x, 50.0);
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn keys_are_held_only_when_creation_checks_uniqueness() {
    for keyed in [false, true] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.outside(&[("element.update", json!({"id":f.b,"set":{"key":"agent"}}))]);
        let mut element = json!({"kind":"shape","position":[900,0]});
        if keyed {
            element["key"] = json!("agent");
        }
        let before = f.file();
        let result = f.write(basis, "element.create", json!({"page":f.page,"elements":[element]})).await;
        if keyed {
            assert_eq!(result.unwrap_err()["error"]["code"], "conflict");
            assert_eq!(f.file(), before);
        } else {
            result.unwrap();
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn link_summaries_ignore_view_changes_but_hold_target_paths() {
    for target in [false, true] {
        let f = Fixture::new();
        f.outside(&[("card.create", json!({"page":f.page,"file":"notes/A.md","position":[900,0]}))]);
        let card = f.doc().model().elements_of(&f.doc().model().pages()[0]).find(|e| e.card.is_some()).unwrap().id.clone();
        let basis = f.basis().await;
        f.outside(&[("card.setTarget", json!({"id":card.as_str(),"file":if target {"notes/B.md"} else {"notes/A.md"},"view":{"heading":["New"]}}))]);
        let before = f.file();
        let result = f.write(basis, "links.retarget", json!({"from":"notes/A.md","to":"archive/A.md"})).await;
        if target {
            assert_eq!(result.unwrap_err()["error"]["code"], "conflict");
            assert_eq!(f.file(), before);
        } else {
            result.unwrap();
            let doc = f.doc();
            let value = doc.model().element(&card).unwrap().card.as_ref().unwrap();
            assert_eq!(value.pointer("/target/path"), Some(&json!("archive/A.md")));
            assert_eq!(value["view"], json!({"heading":["New"]}));
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn equal_area_reorder_and_deletion_hold_the_winner_only() {
    for delete in [false, true] {
        for winner in [false, true] {
            let f = Fixture::new();
            f.outside(&[
                ("frame.create", json!({"page":f.page,"box":[0,400,500,700],"title":"Second"})),
                ("frame.create", json!({"page":f.page,"box":[900,400,1400,700],"title":"Third"})),
            ]);
            let doc = f.doc();
            let second = doc.model().elements_of(&doc.model().pages()[0]).find(|e| e.text == "Second").unwrap().id.clone();
            let basis = f.basis().await;
            let changed = if winner { f.frame.as_str().unwrap() } else { second.as_str() };
            f.outside(&[if delete {
                ("element.delete", json!({"ids":[changed]}))
            } else {
                ("element.reorder", json!({"ids":[changed],"placement":"front"}))
            }]);
            let before = f.file();
            let result = f.write(basis, "element.move", json!({"ids":[f.a],"delta":[50,400],"reparent":true})).await;
            let changes_pick = winner;
            if changes_pick {
                assert_eq!(result.unwrap_err()["error"]["code"], "conflict");
                assert_eq!(f.file(), before);
            } else {
                result.unwrap();
                assert_eq!(
                    f.doc().model().element(&ObjectId::from_string(f.a.as_str().unwrap())).unwrap().parent.as_ref().map(ObjectId::as_str),
                    f.frame.as_str()
                );
            }
            f.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn page_move_destinations_hold_the_gap_before_removing_the_source() {
    for forward in [false, true] {
        for before in [false, true] {
            let f = Fixture::new();
            f.outside(&[("page.create", json!({"name":"Third"})), ("page.create", json!({"name":"Fourth"}))]);
            let doc = f.doc();
            let source = if forward { f.page.clone() } else { json!(doc.model().pages()[3].id.as_str()) };
            let basis = f.basis().await;
            f.outside(&[("page.create", json!({"name":"Colleague","at":if before {0} else {4}}))]);
            let bytes = f.file();
            let result = f.write(basis, "page.move", json!({"page":source,"to":1})).await;
            if before {
                assert_eq!(result.unwrap_err()["error"]["code"], "conflict");
                assert_eq!(f.file(), bytes);
            } else {
                result.unwrap();
                assert_eq!(f.doc().model().pages()[1].id.as_str(), source.as_str().unwrap());
            }
            f.shutdown().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn link_discovery_ignores_other_paths_and_holds_new_matching_cards() {
    for matching in [false, true] {
        let f = Fixture::new();
        let basis = f.basis().await;
        f.outside(&[("card.create", json!({"page":f.page,"file":if matching {"notes/A.md"} else {"other/A.md"},"position":[900,0]}))]);
        let before = f.file();
        let result = f.write(basis, "links.retarget", json!({"from":"notes","to":"archive"})).await;
        if matching {
            assert_eq!(result.unwrap_err()["error"]["code"], "conflict");
            assert_eq!(f.file(), before);
        } else {
            result.unwrap();
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn duplicate_drops_keys_but_still_holds_membership() {
    for child in [false, true] {
        let f = Fixture::new();
        let basis = f.basis().await;
        if child {
            f.outside(&[("element.create", json!({"page":f.page,"elements":[{"kind":"shape","parent":f.frame,"position":[50,450]}]}))]);
        } else {
            f.outside(&[("element.update", json!({"id":f.b,"set":{"key":"colleague"}}))]);
        }
        let before = f.file();
        let result = f.write(basis, "element.duplicate", json!({"ids":[f.frame]})).await;
        if child {
            assert_eq!(result.unwrap_err()["error"]["code"], "conflict");
            assert_eq!(f.file(), before);
        } else {
            result.unwrap();
        }
        f.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn keyed_creation_ignores_stacking_order_of_existing_keys() {
    let f = Fixture::new();
    f.outside(&[("element.update", json!({"id":f.a,"set":{"key":"a"}})), ("element.update", json!({"id":f.b,"set":{"key":"b"}}))]);
    let basis = f.basis().await;
    f.outside(&[("element.reorder", json!({"ids":[f.a],"placement":"front"}))]);
    f.write(basis, "element.create", json!({"page":f.page,"elements":[{"kind":"shape","position":[900,0],"key":"new"}]})).await.unwrap();
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn link_deletion_ignores_other_paths_but_holds_matching_cards() {
    for matching in [false, true] {
        let f = Fixture::new();
        f.outside(&[("card.create", json!({"page":f.page,"file":if matching {"notes/A.md"} else {"other/A.md"},"position":[900,0]}))]);
        let doc = f.doc();
        let card = doc.model().elements_of(&doc.model().pages()[0]).find(|element| element.card.is_some()).unwrap().id.clone();
        let basis = f.basis().await;
        f.outside(&[("element.delete", json!({"ids":[card.as_str()]}))]);
        let before = f.file();
        let result = f.write(basis, "links.retarget", json!({"from":"notes","to":"archive"})).await;
        if matching {
            assert_eq!(result.unwrap_err()["error"]["code"], "conflict");
            assert_eq!(f.file(), before);
        } else {
            result.unwrap();
        }
        f.shutdown().await;
    }
}
