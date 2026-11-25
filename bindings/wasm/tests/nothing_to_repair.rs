//! Nothing is left to repair (design note 1.5, rules V1 to V9; case board.history/nothing-to-repair):
//! seeded random steps on one board's line of changes — the person's commands (shapes, styles,
//! moves, deletes, duplicates, stacking, groups, connectors bound and rebound, frames, drops into
//! and out of frames, parents, labels, cards), Undo and Redo, now and then an agent's draft merged
//! in after the person edited meanwhile (or refused as a collision, changing nothing), and now and
//! then the file changed outside and reread — and after every step a repair pass over the board's
//! state writes nothing: a command never leaves a state the repair would change, and the session
//! repairs what a merge, an undo or a redo brings.
//!
//! The command steps are the deleted convergence suite's random edits (`convergence.rs`, kition-sdk
//! 2d79991), run on one replica.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::collections::BTreeSet;

use kition_board_kdm::{Kind, file, schema};
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::{ActorId, ObjectId, Origin, Replica};
use kition_kit::session::{__export_changes, __merge_changes, __snapshot, KitSessionDocument};
use serde_json::{Value, json};

const FILE: &str = "Plan.kboard";
const AGENT: &str = r#"["agent","agent:test",null]"#;
const SEEDS: [u64; 4] = [0x9E37_79B9_7F4A_7C15, 7, 2026, 0xB0A2_D5EE_D000_0001];
const STEPS: usize = 160;

/// xorshift64*: the same steps for the same seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    fn pick<T: Clone>(&mut self, items: &[T]) -> Option<T> {
        if items.is_empty() { None } else { Some(items[self.below(items.len())].clone()) }
    }
}

fn person() -> Origin {
    Origin::person("person:test")
}

fn agent() -> Origin {
    Origin::agent("agent:test")
}

fn state(doc: &BoardDocument) -> Value {
    doc.session().replica().state_json().unwrap()
}

/// The first page's unlocked elements of `kind` (any kind for `None`), back to front.
fn elements(doc: &BoardDocument, kind: Option<Kind>) -> Vec<String> {
    let model = doc.model();
    let Some(page) = model.pages().first() else { return Vec::new() };
    model
        .elements_of(page)
        .filter(|element| !element.locked && kind.is_none_or(|kind| element.kind == kind))
        .map(|element| element.id.as_str().to_owned())
        .collect()
}

/// The first page's elements a connector end can bind to.
fn bindable(doc: &BoardDocument) -> Vec<String> {
    let model = doc.model();
    let Some(page) = model.pages().first() else { return Vec::new() };
    model.elements_of(page).filter(|element| element.kind.bindable()).map(|element| element.id.as_str().to_owned()).collect()
}

/// The middle of `frame`'s box less a 120 by 80 shape's half, where a drop lands it in the frame.
fn frame_middle(doc: &BoardDocument, frame: &str) -> Option<[f64; 2]> {
    let model = doc.model();
    let rect = model.rect(model.element(&ObjectId::from_string(frame))?)?;
    Some([(rect.x0 + rect.x1) / 2.0 - 60.0, (rect.y0 + rect.y1) / 2.0 - 40.0])
}

fn color(value: u64) -> String {
    format!("#{:06x}", value.wrapping_mul(0x9E37_79B9) & 0xff_ffff)
}

/// One random command as `origin`; answers its label when it ran (a refusal is no edit).
fn edit(doc: &mut BoardDocument, rng: &mut Rng, origin: &Origin) -> Option<&'static str> {
    let all = elements(doc, None);
    let target = rng.pick(&all);
    let value = rng.next() % 10_000;
    let (label, command, params) = match (rng.below(18), target) {
        (0..=2, _) => (
            "shape",
            "element.add",
            json!({ "elements": [{ "kind": "shape", "position": [value % 1200, value / 7 % 800], "size": [120, 80], "fillColor": color(value) }] }),
        ),
        (3, Some(id)) => ("fill", "element.style", json!({ "ids": [id], "style": { "fillColor": color(value) } })),
        (4, Some(id)) if all.len() > 2 => ("delete", "element.delete", json!({ "ids": [id] })),
        (5, Some(id)) => ("move", "element.move", json!({ "ids": [id], "delta": [(value % 41) as i64 - 20, (value % 23) as i64 - 11] })),
        (6, Some(id)) => ("duplicate", "element.duplicate", json!({ "ids": [id] })),
        (7, Some(id)) => {
            let placement = ["front", "forward", "backward", "back"][rng.below(4)];
            ("reorder", "element.reorder", json!({ "ids": [id], "placement": placement }))
        }
        (8, Some(id)) => {
            let other = rng.pick(&all)?;
            if other == id {
                return None;
            }
            ("group", "arrange.group", json!({ "ids": [id, other] }))
        }
        (9, _) => {
            let group = rng.pick(&elements(doc, Some(Kind::Group)))?;
            ("ungroup", "arrange.ungroup", json!({ "groupIds": [group] }))
        }
        (10, _) => {
            let ends = bindable(doc);
            let (from, to) = (rng.pick(&ends)?, rng.pick(&ends)?);
            let anchor = [(value % 11) as f64 / 10.0, (value % 7) as f64 / 6.0];
            ("connect", "connector.add", json!({ "from": { "element": from }, "to": { "element": to, "anchor": anchor } }))
        }
        (11, _) => {
            let (connector, to) = (rng.pick(&elements(doc, Some(Kind::Connector)))?, rng.pick(&bindable(doc))?);
            let terminal = if rng.chance(50) { "start" } else { "end" };
            let to = if rng.chance(25) { json!({ "point": [value % 900, value % 700] }) } else { json!({ "element": to }) };
            ("rebind", "connector.bind", json!({ "id": connector, "terminal": terminal, "to": to }))
        }
        (12, _) => {
            let (x, y) = ((value % 1000) as f64, (value % 600) as f64);
            ("frame", "frame.add", json!({ "box": [x, y, x + 300.0, y + 200.0] }))
        }
        (13, Some(id)) => {
            // Into a frame or out of one, as a drag's drop does.
            let to = match rng.pick(&elements(doc, Some(Kind::Frame))) {
                Some(frame) if rng.chance(75) => frame_middle(doc, &frame).unwrap_or([(value % 900) as f64, 900.0]),
                _ => [(value % 900) as f64, 900.0],
            };
            ("drop", "element.move", json!({ "ids": [id], "to": to, "reparent": true }))
        }
        (14, Some(id)) => {
            let parent = rng.pick(&elements(doc, Some(Kind::Frame))).map_or(Value::Null, Value::String);
            ("parent", "element.set", json!({ "id": id, "set": { "parent": parent } }))
        }
        (15, Some(id)) => {
            let element = doc.model().element(&ObjectId::from_string(&id))?.clone();
            if element.kind != Kind::Shape {
                return None;
            }
            ("label", "element.setText", json!({ "id": id, "text": format!("{}{}", element.text, value % 100) }))
        }
        (16, _) => {
            let file = ["notes/plan.md", "data/table.ktable", "docs/brief.pdf", "boards/other.kboard"][rng.below(4)];
            ("card", "card.add", json!({ "file": file, "position": [value % 1200, value % 800] }))
        }
        (17, Some(id)) => ("rotate", "element.rotate", json!({ "ids": [id], "degrees": (value % 90) as f64 })),
        _ => return None,
    };
    doc.execute_as(command, params, origin).is_ok().then_some(label)
}

/// A repair pass over `doc`'s state writes nothing.
fn nothing_to_repair(doc: &BoardDocument, seed: u64, step: usize, what: &str) {
    let mut replica = Replica::load(schema(), &__snapshot(doc).unwrap(), ActorId::random()).unwrap();
    let before = replica.state_json().unwrap();
    if file::repair_once(&mut replica).unwrap() {
        let after = replica.state_json().unwrap();
        let changed: Vec<&String> = before["objects"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(id, object)| after["objects"].get(id.as_str()) != Some(*object))
            .map(|(id, _)| id)
            .collect();
        panic!("seed {seed:#x}, step {step} ({what}) left something to repair, in {changed:?}");
    }
}

/// An agent's draft of `doc` with a few of its commands, merged after the person edited `doc`
/// meanwhile; a merge refused as a collision changes nothing.
fn merge_draft(doc: &mut BoardDocument, rng: &mut Rng, seed: u64, step: usize) -> Option<&'static str> {
    let mut draft = BoardDocument::open_with_replica(FILE, &[], &__snapshot(doc).unwrap(), &doc.blobs_bundle()).unwrap();
    let base = draft.version();
    for _ in 0..1 + rng.below(3) {
        edit(&mut draft, rng, &agent());
    }
    for _ in 0..rng.below(3) {
        if let Some(label) = edit(doc, rng, &person()) {
            nothing_to_repair(doc, seed, step, label);
        }
    }
    let changes = __export_changes(&draft, &base.encode()).unwrap();
    let before = state(doc);
    match __merge_changes(doc, &changes, AGENT) {
        Ok(_) => Some("merge"),
        Err(error) => {
            assert!(error.contains("collides"), "seed {seed:#x}, step {step}: a merge refused for another reason: {error}");
            assert_eq!(state(doc), before, "a refused merge changes nothing");
            Some("refused merge")
        }
    }
}

/// The file `doc` saved, changed outside by a few commands, read again and taken in.
fn reread(doc: &mut BoardDocument, rng: &mut Rng) -> Option<&'static str> {
    let mut outside = BoardDocument::open(FILE, &doc.save(Some(FILE)).unwrap()).unwrap();
    for _ in 0..1 + rng.below(3) {
        edit(&mut outside, rng, &person());
    }
    let fresh = BoardDocument::open(FILE, &outside.save(Some(FILE)).unwrap()).unwrap();
    assert!(doc.reconcile_reread(&fresh).unwrap());
    assert_eq!(state(doc), state(&fresh), "the file's state, by object id");
    Some("reread")
}

#[test]
fn no_step_leaves_anything_to_repair() {
    let mut ran = BTreeSet::new();
    for seed in SEEDS {
        let mut rng = Rng(seed);
        let mut doc = BoardDocument::create(FILE, 1920, 1080).unwrap();
        for step in 0..STEPS {
            let label = match rng.below(20) {
                0 => KitSessionDocument::undo(&mut doc).unwrap().0.then_some("undo"),
                1 => KitSessionDocument::redo(&mut doc).unwrap().0.then_some("redo"),
                2 => merge_draft(&mut doc, &mut rng, seed, step),
                3 => reread(&mut doc, &mut rng),
                _ => edit(&mut doc, &mut rng, &person()),
            };
            if let Some(label) = label {
                ran.insert(label);
                nothing_to_repair(&doc, seed, step, label);
            }
        }
    }
    // The steps that matter ran (a command whose parameters went stale would only be refused).
    for label in [
        "shape",
        "fill",
        "delete",
        "move",
        "duplicate",
        "reorder",
        "group",
        "connect",
        "rebind",
        "frame",
        "drop",
        "parent",
        "undo",
        "redo",
        "merge",
        "reread",
    ] {
        assert!(ran.contains(label), "no {label} step ran: {ran:?}");
    }
}
