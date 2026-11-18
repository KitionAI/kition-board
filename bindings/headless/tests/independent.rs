//! C8-board-read: what the independent reading of every write command found the declarations
//! got wrong, each with an independent writer that really replaces the file between the agent's
//! observation and its write: a change the write rests on must refuse it, an unrelated one must
//! not (`bindings/wasm/tests/declarations_independent.rs` has the reading).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod support;
use kition_kit::KitDocument;
use serde_json::{Value, json};
use support::*;

/// Whether the write applied; a refusal must be a conflict that wrote nothing.
async fn applies(f: &Fixture, basis: Value, command: &str, params: Value) -> bool {
    let before = f.file();
    match f.write(basis, command, params).await {
        Ok(_) => true,
        Err(error) => {
            assert_eq!(error["error"]["code"], "conflict", "{error}");
            assert_eq!(f.file(), before, "a refusal wrote bytes");
            false
        }
    }
}

/// Held less, a read that is not held: a connector end without an anchor faces the other end
/// through its element's rotation, which a turn from 45 to 135 degrees changes without changing
/// its bounds.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_keeping_bounds_of_an_end_element_refuses_a_new_connector() {
    for turned in [true, false] {
        let f = Fixture::new();
        f.outside(&[("element.update", json!({"id":f.a,"set":{"rotation":45}}))]);
        let basis = f.basis().await;
        if turned {
            f.outside(&[("element.update", json!({"id":f.a,"set":{"rotation":135}}))]);
        } else {
            f.outside(&[("element.setText", json!({"id":f.a,"text":"Colleague"}))]);
        }
        let wrote = applies(&f, basis, "connector.create", json!({"page":f.page,"from":{"element":f.a},"to":{"element":f.b}})).await;
        assert_eq!(wrote, !turned, "turned={turned}");
        f.shutdown().await;
    }
}

/// Held less, a value naming another object: a pasted payload's refs name live elements (without
/// `at` an element of the page among them puts the copies 24 units off), and a pasted connector
/// bound to an element outside the payload stays bound to it.
#[tokio::test(flavor = "multi_thread")]
async fn a_paste_rests_on_the_live_elements_its_payload_names() {
    for (bound, deleted) in [(false, true), (false, false), (true, true), (true, false)] {
        let f = Fixture::new();
        f.outside(&[("connector.create", json!({"page":f.page,"from":{"element":f.a},"to":{"element":f.b}}))]);
        let doc = f.doc();
        let page = doc.model().pages()[0].id.clone();
        let connector = doc.model().elements_of(&doc.model().pages()[0]).find(|e| e.kind.as_str() == "connector").unwrap().id.clone();
        let mut doc = doc;
        let copied = doc.execute("clipboard.copy", json!({"ids":[if bound { json!(connector.as_str()) } else { f.a.clone() }]})).unwrap();
        let basis = f.basis().await;
        if deleted {
            f.outside(&[("element.delete", json!({"ids":[if bound { &f.b } else { &f.a }]}))]);
        } else {
            f.outside(&[("element.style", json!({"ids":[f.a, f.b],"style":{"fillColor":"red"}}))]);
        }
        let params = if bound { json!({"page":page.as_str(),"payload":copied,"at":[0,600]}) } else { json!({"page":page.as_str(),"payload":copied}) };
        let wrote = applies(&f, basis, "element.paste", params).await;
        assert_eq!(wrote, !deleted, "bound={bound} deleted={deleted}");
        f.shutdown().await;
    }
}

/// Held more, ancestry held though not read: a lone element's move does not depend on its
/// container (board coordinates are world coordinates).
#[tokio::test(flavor = "multi_thread")]
async fn a_lone_elements_move_does_not_rest_on_its_container() {
    for moved in [true, false] {
        let f = Fixture::new();
        let basis = f.basis().await;
        if moved {
            f.outside(&[("element.move", json!({"ids":[f.a],"delta":[5,0]}))]);
        } else {
            f.outside(&[("element.update", json!({"id":f.a,"set":{"parent":f.frame}}))]);
        }
        let wrote = applies(&f, basis, "element.move", json!({"ids":[f.a],"delta":[10,0]})).await;
        assert_eq!(wrote, !moved, "moved={moved}");
        f.shutdown().await;
    }
}

/// Held more, geometry held beyond what is read: a box turns about its own centre wherever it is;
/// alignment reads bounds, which a turn from 45 to 135 degrees keeps.
#[tokio::test(flavor = "multi_thread")]
async fn turning_in_place_and_aligning_do_not_rest_on_what_they_do_not_read() {
    for (command, params, change, wrote) in [
        ("element.rotate", json!({"degrees":90}), ("element.move", json!({"delta":[5,0]})), true),
        ("element.rotate", json!({"degrees":90}), ("element.rotate", json!({"degrees":10})), false),
        ("layout.align", json!({"edge":"left"}), ("element.update", json!({"set":{"rotation":135}})), true),
        ("layout.align", json!({"edge":"left"}), ("element.move", json!({"delta":[5,0]})), false),
    ] {
        let f = Fixture::new();
        f.outside(&[("element.update", json!({"id":f.a,"set":{"rotation":45}}))]);
        let basis = f.basis().await;
        let mut colleague = change.1.clone();
        if change.0 == "element.update" {
            colleague["id"] = f.a.clone();
        } else {
            colleague["ids"] = json!([f.a]);
        }
        f.outside(&[(change.0, colleague)]);
        let mut params = params.clone();
        params["ids"] = if command == "layout.align" { json!([f.a, f.b]) } else { json!([f.a]) };
        assert_eq!(applies(&f, basis, command, params).await, wrote, "{command} after {}", change.0);
        f.shutdown().await;
    }
}

/// Held more, every field held where one is written: an ungroup writes its members' `parent`.
#[tokio::test(flavor = "multi_thread")]
async fn an_ungroup_does_not_rest_on_its_members_styles() {
    for left in [true, false] {
        let f = Fixture::new();
        f.outside(&[("element.group", json!({"ids":[f.a, f.b]}))]);
        let group = f.doc().model().element(&kition_kit::kdm::ObjectId::from_string(f.a.as_str().unwrap())).unwrap().parent.clone().unwrap();
        let basis = f.basis().await;
        if left {
            f.outside(&[("element.update", json!({"id":f.a,"set":{"parent":f.frame}}))]);
        } else {
            f.outside(&[("element.style", json!({"ids":[f.a],"style":{"fillColor":"red"}}))]);
        }
        let wrote = applies(&f, basis, "element.ungroup", json!({"groupIds":[group.as_str()]})).await;
        assert_eq!(wrote, !left, "left={left}");
        f.shutdown().await;
    }
}

/// Held more, a board-wide summary held where the objects' own premises suffice: a page copy
/// reads its own page's elements; giving a shape a container reads that container's kind.
#[tokio::test(flavor = "multi_thread")]
async fn page_copies_and_new_containers_do_not_rest_on_other_structure() {
    for (command, colleague, wrote) in [
        ("page.duplicate", "bind-elsewhere", true),
        ("page.duplicate", "add-to-page", false),
        ("element.update", "bind-elsewhere", true),
        ("element.update", "delete-container", false),
    ] {
        let f = Fixture::new();
        let basis = f.basis().await;
        match colleague {
            "bind-elsewhere" => f.outside(&[("connector.create", json!({"page":f.page,"from":{"element":f.b},"to":{"element":f.child}}))]),
            "add-to-page" => f.outside(&[("element.create", json!({"page":f.other_page,"elements":[{"kind":"shape","position":[0,0]}]}))]),
            _ => f.outside(&[("element.delete", json!({"ids":[f.frame]}))]),
        }
        let params = if command == "page.duplicate" { json!({"page":f.other_page}) } else { json!({"id":f.a,"set":{"parent":f.frame}}) };
        assert_eq!(applies(&f, basis, command, params).await, wrote, "{command} after {colleague}");
        f.shutdown().await;
    }
}

/// Held more, a window wider than what is read: to the front does not read the rest of the page;
/// one step forward reads the neighbour it passes.
#[tokio::test(flavor = "multi_thread")]
async fn reordering_rests_only_on_the_neighbour_it_passes() {
    for (placement, colleague, wrote) in
        [("front", "delete-later", true), ("front", "delete-a", false), ("forward", "delete-later", true), ("forward", "neighbour", false)]
    {
        let f = Fixture::new();
        let basis = f.basis().await;
        match colleague {
            "delete-later" => f.outside(&[("element.delete", json!({"ids":[f.child]}))]),
            "delete-a" => f.outside(&[("element.delete", json!({"ids":[f.a]}))]),
            _ => f.outside(&[("element.reorder", json!({"ids":[f.b],"placement":"front"}))]),
        }
        let wrote_now = applies(&f, basis, "element.reorder", json!({"ids":[f.a],"placement":placement})).await;
        assert_eq!(wrote_now, wrote, "{placement} after {colleague}");
        f.shutdown().await;
    }
}
