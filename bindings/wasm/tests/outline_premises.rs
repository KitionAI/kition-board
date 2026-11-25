//! A bound end on a shape is where the ray through its anchor leaves the shape's outline (design
//! note 1.4), so a command that reads such an end reads the shape's `shapeType` as it reads its box
//! (`premises.rs` `outline`); a new `shapeType` refreshes the bound ends' stored points as a new
//! position does, a consequence and not a premise (task board-parity).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use serde_json::{Value, json};

/// Page with shapes `A` (rectangle), `B` (diamond) and `C`, and a text `T`; `A`, `B`, `C` 100 × 100.
fn board(a_shape: &str) -> (BoardDocument, [Value; 4]) {
    let mut doc = BoardDocument::create("Outline.kboard", 1200, 800).unwrap();
    let made = doc
        .execute(
            "element.add",
            json!({"elements":[
                {"kind":"shape","shapeType":a_shape,"position":[0,0],"size":[100,100],"text":"A"},
                {"kind":"shape","shapeType":"diamond","position":[300,200],"size":[100,100],"text":"B"},
                {"kind":"shape","position":[700,0],"size":[100,100],"text":"C"},
                {"kind":"text","position":[700,400],"text":"T"}
            ]}),
        )
        .unwrap();
    let ids = made["created"].as_array().unwrap();
    (doc, [ids[0].clone(), ids[1].clone(), ids[2].clone(), ids[3].clone()])
}

fn held(doc: &BoardDocument, command: &str, params: &Value, id: &Value) -> Vec<String> {
    let under = doc.under(command, params).unwrap().expect("an explicit per-call premise");
    let mut fields = under.premises.iter().find(|premise| &premise.id == id).and_then(|premise| premise.fields.clone()).unwrap_or_default();
    fields.sort();
    fields
}

fn holds(fields: &[String], field: &str) -> bool {
    fields.iter().any(|held| held == field)
}

#[test]
fn a_connectors_route_holds_the_outline_of_the_shapes_it_ends_on() {
    let (mut doc, [a, b, c, t]) = board("rectangle");
    let k = doc.execute("connector.add", json!({"from":{"element":a},"to":{"element":b}})).unwrap()["created"][0].clone();
    let text = doc.execute("connector.add", json!({"from":{"element":c},"to":{"element":t}})).unwrap()["created"][0].clone();
    // Aligning the connectors moves them by their bounds, whose ends are on A's, B's and C's
    // outlines and on T's box.
    let params = json!({"ids":[k, text],"edge":"left"});
    for (id, name) in [(&a, "A"), (&b, "B"), (&c, "C")] {
        let fields = held(&doc, "layout.align", &params, id);
        assert!(holds(&fields, "shapeType") && holds(&fields, "~bounds"), "{name}: {fields:?}");
    }
    let fields = held(&doc, "layout.align", &params, &t);
    assert!(!holds(&fields, "shapeType") && holds(&fields, "~bounds"), "T ends on its box: {fields:?}");
    // Aligning the shapes themselves reads their boxes, not their outlines.
    let fields = held(&doc, "layout.align", &json!({"ids":[a, c],"edge":"left"}), &a);
    assert!(!holds(&fields, "shapeType"), "{fields:?}");
}

#[test]
fn an_anchored_end_aims_the_other_end_from_its_outline() {
    // The other end's anchor faces the anchored end's outline point: a diamond's corner anchor
    // is a different point from a rectangle's, so B's anchor follows A's shapeType.
    let mut ends = Vec::new();
    for shape in ["rectangle", "diamond"] {
        let (mut doc, [a, b, _, _]) = board(shape);
        let params = json!({"from":{"element":a,"anchor":[1,0]},"to":{"element":b}});
        assert!(holds(&held(&doc, "connector.add", &params, &a), "shapeType"), "{shape}: the anchored end's outline");
        assert!(!holds(&held(&doc, "connector.add", &params, &b), "shapeType"), "{shape}: B's point is only a cache");
        ends.push(doc.execute("connector.add", params.clone()).unwrap()["end"].clone());
    }
    assert_ne!(ends[0], ends[1], "B's end follows A's outline");
    // Without anchors each end aims at the other's centre; with both anchored nothing is aimed.
    let (doc, [a, b, _, _]) = board("diamond");
    for params in [json!({"from":{"element":a},"to":{"element":b}}), json!({"from":{"element":a,"anchor":[1,0]},"to":{"element":b,"anchor":[0,0.5]}})]
    {
        for id in [&a, &b] {
            assert!(!holds(&held(&doc, "connector.add", &params, id), "shapeType"), "{params}");
        }
    }
}

#[test]
fn a_new_shape_type_is_held_like_a_new_position() {
    let (mut doc, [a, b, _, _]) = board("rectangle");
    let k = doc.execute("connector.add", json!({"from":{"element":a},"to":{"element":b}})).unwrap()["created"][0].clone();
    for set in [json!({"shapeType":"ellipse"}), json!({"position":[10,10]})] {
        let params = json!({"id":a,"set":set});
        let under = doc.under("element.set", &params).unwrap().unwrap();
        // The connector's stored ends are refreshed in the same command (a cache, design note
        // 1.4): no premise on it.
        assert!(under.premises.iter().all(|premise| premise.id != k), "{params}: {:?}", under.premises);
    }
    assert_eq!(held(&doc, "element.set", &json!({"id":a,"set":{"shapeType":"ellipse"}}), &a), ["kind", "locked", "shapeType"]);
    let before = doc.model().element(&kition_kit::kdm::ObjectId::from_string(k.as_str().unwrap())).unwrap().start.clone().unwrap().point;
    doc.execute("element.set", json!({"id":a,"set":{"shapeType":"diamond"}})).unwrap();
    let after = doc.model().element(&kition_kit::kdm::ObjectId::from_string(k.as_str().unwrap())).unwrap().start.clone().unwrap().point;
    assert_ne!(before, after, "the stored end moved onto the diamond's outline");
}
