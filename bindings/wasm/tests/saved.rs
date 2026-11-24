//! C8-land-11-board: native f64/KDM values, materialised geometry and derived facts survive save.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
use kition_board_kit::BoardDocument;
use kition_kit::{KitDocument, session::KitSessionDocument};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn check(doc: &mut BoardDocument, label: &str) -> Value {
    let state = doc.session().replica().state_json().unwrap();
    doc.prepare_save().unwrap();
    let saved = doc.save(None).unwrap();
    let back = BoardDocument::open("Invariant.kboard", &saved).unwrap();
    assert_eq!(state, back.session().replica().state_json().unwrap(), "replica after {label}");
    assert_eq!(doc.model(), back.model(), "engine view after {label}");
    assert_eq!(doc.derived(&[], &[]).unwrap(), back.derived(&[], &[]).unwrap(), "derived values after {label}");
    back.session().replica().state_json().unwrap()
}

fn run(doc: &mut BoardDocument, command: &str, params: Value, count: &mut usize) -> Value {
    let before = doc.session().replica().state_json().unwrap();
    let version = doc.version();
    let result = doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"));
    let saved = check(doc, command);
    let receipt = serde_json::to_value(doc.session().replica().changes_since(&version).unwrap()).unwrap();
    let reopened_diff = serde_json::to_value(kition_kit::kdm::reread::diff_paired(&kition_board_kdm::schema(), &before, &saved)).unwrap();
    for list in ["created", "changed", "moved", "deleted"] {
        assert_eq!(receipt[list], reopened_diff[list], "stored-field receipt {list} after {command}");
    }
    *count += 1;
    result
}

fn fixtures(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            fixtures(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "kboard") {
            out.push(path);
        }
    }
}

#[test]
fn every_fixture_and_representative_command_reopens_as_the_exact_in_memory_state() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut paths = Vec::new();
    fixtures(&root.join("conformance"), &mut paths);
    fixtures(&root.join("cases"), &mut paths);
    paths.sort();
    assert!(!paths.is_empty());
    let mut commands = 0;
    for path in &paths {
        let mut doc = BoardDocument::open("Invariant.kboard", &std::fs::read(path).unwrap()).unwrap();
        check(&mut doc, "open");
        let made = run(
            &mut doc,
            "element.create",
            json!({"elements":[
                {"kind":"shape","position":[0.1,1.0/3.0],"size":[120.123456789,60.987654321],"text":"Alpha"},
                {"kind":"text","position":[400.1,30.2],"text":"Beta"}
            ]}),
            &mut commands,
        );
        let (a, b) = (made["created"][0].clone(), made["created"][1].clone());
        let frame = run(&mut doc, "frame.create", json!({"box":[-10.1,-10.2,300.3,300.4],"children":[a]}), &mut commands)["frame"].clone();
        for (command, params) in [
            ("element.style", json!({"ids":[a,b],"style":{"opacity":0.123456789123,"fontSize":27.123456789}})),
            ("element.move", json!({"ids":[frame],"delta":[1.0/7.0,1.0/3.0]})),
            ("element.resize", json!({"id":a,"box":[0.123,0.456,120.789,60.012]})),
            ("element.rotate", json!({"ids":[a],"degrees":17.123456789})),
            ("element.setText", json!({"id":b,"text":"Changed text"})),
            ("text.mark", json!({"id":b,"range":[0,7],"mark":"bold","value":true})),
            ("frame.fit", json!({"id":frame,"padding":12.3456789})),
            ("layout.align", json!({"ids":[a,b],"edge":"top"})),
            ("layout.stack", json!({"ids":[a,b],"axis":"horizontal","gap":1.0/3.0})),
        ] {
            run(&mut doc, command, params, &mut commands);
        }
        let connector =
            run(&mut doc, "connector.create", json!({"from":{"element":a},"to":{"element":b},"label":"Edge"}), &mut commands)["created"][0].clone();
        for (command, params) in [
            ("connector.setLabel", json!({"id":connector,"text":"Next","position":1.0/3.0})),
            ("connector.setWaypoints", json!({"id":connector,"waypoints":[[222.222,111.111]]})),
            ("connector.bind", json!({"id":connector,"terminal":"end","to":{"point":[500.123456789,300.987654321]}})),
        ] {
            run(&mut doc, command, params, &mut commands);
        }
        let group = run(&mut doc, "element.group", json!({"ids":[a,b]}), &mut commands)["group"].clone();
        run(&mut doc, "element.ungroup", json!({"groupIds":[group]}), &mut commands);
        let duplicate = run(&mut doc, "element.duplicate", json!({"ids":[a],"offset":[0.1,0.2]}), &mut commands)["created"].clone();
        run(&mut doc, "element.delete", json!({"ids":duplicate}), &mut commands);
        run(&mut doc, "stroke.create", json!({"points":[[1.123456789,2.987654321],[100.1,200.2]]}), &mut commands);
        run(&mut doc, "card.create", json!({"url":"https://example.com","position":[123.123456789,321.987654321]}), &mut commands);
        run(&mut doc, "page.create", json!({"name":"Invariant","at":0}), &mut commands);
    }
    println!("in-memory == saved: {} fixture files, {commands} commands, 0 replica/view/derived/receipt mismatches", paths.len());
}
