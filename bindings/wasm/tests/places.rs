//! C8-land-11-board: per-call field and geometry premises, read against engine implementations.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use serde_json::json;

#[test]
fn property_and_delta_premises_hold_only_the_facts_the_command_uses() {
    let mut doc = BoardDocument::create("Premises.kboard", 800, 600).unwrap();
    let id = doc.execute("element.create", json!({"elements":[{"kind":"shape","position":[0,0]}]})).unwrap()["created"][0].clone();
    for (command, params, expected) in [
        ("element.style", json!({"ids":[id],"style":{"fillColor":"blue"}}), vec!["fillColor", "kind", "locked"]),
        ("element.setText", json!({"id":id,"text":"New"}), vec!["kind", "locked", "text"]),
        ("element.update", json!({"id":id,"set":{"rotation":45}}), vec!["kind", "locked", "rotation"]),
        ("element.move", json!({"ids":[id],"delta":[10,0]}), vec!["kind", "locked", "position"]),
    ] {
        let under = doc.under(command, &params).unwrap().expect("an explicit per-call premise");
        let held = under.premises.iter().find(|premise| premise.id == id).unwrap();
        let mut fields = held.fields.clone().unwrap();
        fields.sort();
        assert_eq!(fields, expected, "{command}");
        // C8-board-read: a lone element's move does not read its container.
        let positions = if command == "element.move" { vec!["position".to_owned()] } else { vec![] };
        assert_eq!(held.positions, Some(positions), "{command}: list index is not a dependency");
    }
}

#[test]
fn parent_moves_hold_children_and_connector_binding_holds_the_other_target() {
    let mut doc = BoardDocument::create("Dependencies.kboard", 800, 600).unwrap();
    let made = doc
        .execute(
            "element.create",
            json!({"elements":[
                {"kind":"shape","position":[10,10]}, {"kind":"shape","position":[400,10]}
            ]}),
        )
        .unwrap();
    let (a, b) = (made["created"][0].clone(), made["created"][1].clone());
    let frame = doc.execute("frame.create", json!({"box":[0,0,300,300],"children":[a]})).unwrap()["frame"].clone();
    let connector = doc.execute("connector.create", json!({"from":{"element":a},"to":{"element":b}})).unwrap()["created"][0].clone();
    let under = doc.under("element.move", &json!({"ids":[frame],"delta":[20,0]})).unwrap().unwrap();
    assert!(under.premises.iter().any(|premise| premise.id == a && premise.fields.as_ref().unwrap().iter().any(|field| field == "position")));
    let under = doc.under("connector.bind", &json!({"id":connector,"terminal":"start","to":{"element":a}})).unwrap().unwrap();
    assert!(under.premises.iter().any(|premise| premise.id == b && premise.fields.as_ref().unwrap().iter().any(|field| field == "~bounds")));
}
