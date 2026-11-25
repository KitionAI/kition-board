//! C8-land-11-board: rules fixed before running, then real independent-file writes.
//!
//! Necessary refusals by write form (all other colleague changes must apply):
//! - id: fill A; only a changed fill or deleted A invalidates it.
//! - index: insert a named page at index 1; only insertion before that window invalidates it.
//! - point: move A by [50,400] and take the destination frame; A's deletion, position or
//!   parent changing, or the destination frame moving, invalidates it. Title/fill do not.
//! - structural: group A and B; either disappearing or changing parent invalidates membership.
//!
//! These rules are deliberately independent of Under declarations and observed test outcomes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod support;
use kition_kit::kdm::ObjectId;
use kition_kits::tools::{CallOptions, Guard};
use serde_json::{Value, json};
use support::*;

const CHANGES: [&str; 10] = [
    "other-text",
    "same-fill",
    "target-move",
    "target-delete",
    "destination-move",
    "destination-title",
    "insert-before",
    "insert-after",
    "target-parent",
    "other-move",
];
const FORMS: [&str; 4] = ["id", "index", "point", "structural"];

fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

fn necessary(form: usize, change: usize) -> bool {
    match form {
        0 => matches!(change, 1 | 3),
        1 => change == 6,
        2 => matches!(change, 2 | 3 | 4 | 8),
        3 => matches!(change, 3 | 8),
        _ => panic!("unknown form"),
    }
}

fn colleague(f: &Fixture, change: usize, round: usize) {
    let step = match change {
        0 => ("element.setText", json!({"id":f.b,"text":format!("Other {round}")})),
        1 => ("element.style", json!({"ids":[f.a],"style":{"fillColor":"red"}})),
        2 => ("element.move", json!({"ids":[f.a],"delta":[800,0]})),
        3 => ("element.delete", json!({"ids":[f.a]})),
        4 => ("element.move", json!({"ids":[f.frame],"delta":[800,0]})),
        5 => ("element.setText", json!({"id":f.frame,"text":"Renamed destination"})),
        6 => ("page.add", json!({"at":0,"name":"Before"})),
        7 => ("page.add", json!({"at":2,"name":"After"})),
        8 => ("element.set", json!({"id":f.a,"set":{"parent":f.frame}})),
        9 => ("element.move", json!({"ids":[f.b],"delta":[40,0]})),
        _ => panic!("unknown change"),
    };
    f.outside(&[step]);
}

fn write(f: &Fixture, form: usize) -> (&'static str, Value) {
    match form {
        0 => ("element.style", json!({"ids":[f.a],"style":{"fillColor":"blue"}})),
        1 => ("page.add", json!({"name":"Agent","at":1})),
        2 => ("element.move", json!({"ids":[f.a],"delta":[50,400],"reparent":true})),
        3 => ("arrange.group", json!({"ids":[f.a,f.b]})),
        _ => panic!("unknown form"),
    }
}

fn correct(f: &Fixture, form: usize) -> bool {
    let doc = f.doc();
    let model = doc.model();
    let a = model.element(&ObjectId::from_string(f.a.as_str().unwrap()));
    match form {
        0 => a.is_some_and(|a| a.register("fillColor") == Some(&json!("blue"))),
        1 => model.pages()[1].name == "Agent" && model.pages()[2].id.as_str() == f.other_page.as_str().unwrap(),
        2 => {
            a.is_some_and(|a| a.parent.as_ref().map(ObjectId::as_str) == f.frame.as_str() && a.position.is_some_and(|p| p.x == 50.0 && p.y == 400.0))
        }
        3 => a.is_some_and(|a| a.parent.is_some() && a.parent == model.element(&ObjectId::from_string(f.b.as_str().unwrap())).unwrap().parent),
        _ => false,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn fixed_seed_fifty_rounds_mixed_table() {
    // Each of 50 rounds tries every form on its own file. Shuffle ten kinds in each block,
    // so each kind occurs five times per form; seed controls both kind and form order.
    let mut seed = 0x9e3779b97f4a7c15_u64;
    let mut kinds: Vec<usize> = (0..10).collect();
    let mut table = [[[0usize; 5]; 4]; 10];
    for round in 0..50 {
        if round % 10 == 0 {
            for i in (1..10).rev() {
                let j = next(&mut seed) as usize % (i + 1);
                kinds.swap(i, j);
            }
        }
        let change = kinds[round % 10];
        let mut forms = [0, 1, 2, 3];
        for i in (1..4).rev() {
            let j = next(&mut seed) as usize % (i + 1);
            forms.swap(i, j);
        }
        for form in forms {
            let f = Fixture::new();
            let basis = f.basis().await;
            colleague(&f, change, round);
            let before = f.file();
            let (command, params) = write(&f, form);
            let result = f.write(basis, command, params).await;
            let needed = necessary(form, change);
            let classification = match &result {
                Ok(_) if needed || !correct(&f, form) => 3,              // applied wrongly
                Ok(_) => 0,                                              // applied
                Err(error) if error["error"]["code"] != "conflict" => 4, // failed at kit
                Err(_) if needed => 1,                                   // refused necessary
                Err(_) => 2,                                             // refused false
            };
            table[change][form][classification] += 1;
            if result.is_err() {
                assert_eq!(f.file(), before, "a refusal wrote bytes");
            }
            if classification >= 2 {
                println!("mixed problem round={round} {} {} needed={needed}: {result:?}", CHANGES[change], FORMS[form]);
            }
            f.shutdown().await;
        }
    }
    let mut total = [0; 5];
    println!("mixed seed=0x9e3779b97f4a7c15; cells=applied/necessary/false/wrong/kit");
    for (change, forms) in table.iter().enumerate() {
        println!("{}: id={:?} index={:?} point={:?} structural={:?}", CHANGES[change], forms[0], forms[1], forms[2], forms[3]);
        for counts in forms {
            for (i, n) in counts.iter().enumerate() {
                total[i] += n;
            }
        }
    }
    println!("mixed total={total:?}");
    assert_eq!(&total[2..], &[0, 0, 0], "false refusals / wrong applies / kit failures");
}

#[tokio::test(flavor = "multi_thread")]
async fn fifty_unrelated_file_changes_premise_and_epoch() {
    for guard in [Guard::Premise, Guard::Epoch] {
        let f = Fixture::new();
        let mut applied = 0;
        let mut refused = 0;
        for round in 0..50 {
            let basis = f.basis().await;
            f.outside(&[("element.setText", json!({"id":f.b,"text":format!("Colleague {round}")}))]);
            let before = f.file();
            let result = f
                .call_with(
                    "run",
                    json!({"basis":basis,"steps":[{"command":"element.setText","params":{"id":f.a,"text":format!("Agent {round}")}}]}),
                    CallOptions { guard, ..CallOptions::default() },
                )
                .await;
            match result {
                Ok(_) => applied += 1,
                Err(error) => {
                    assert_eq!(error["error"]["code"], "conflict", "{error}");
                    assert_eq!(f.file(), before);
                    refused += 1;
                }
            }
        }
        println!("unrelated {guard:?}: applied={applied} refused={refused}");
        assert_eq!((applied, refused), if guard == Guard::Premise { (50, 0) } else { (0, 50) });
        f.shutdown().await;
    }
}
