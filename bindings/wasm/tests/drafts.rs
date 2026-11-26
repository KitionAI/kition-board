//! A draft merged and an earlier state restored through the engine members (`mergeChanges`,
//! `restoreState`; kition-sdk data model 6.6 rules D1–D9, KitModel M12a): an agent's draft is one
//! step of the person's Undo (the host's `draft/merge` passes the agent's origin), never of the
//! agent's; one Undo gives back the exact state and view before, and Redo the merged or restored
//! ones; the person's edit made meanwhile on the same page stays. The engine's views (revision,
//! the cached frame and raster) follow a merge and a restore as they follow any other change
//! (regressions/issue-merge-stale-views).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::{ObjectId, Origin, UndoScope, Version};
use kition_kit::session::{__export_changes, __merge_changes, __restore_state, __snapshot, KitSessionDocument};
use serde_json::{Value, json};

const FILE: &str = "Plan.kboard";
/// The origin the host's `draft/merge` passes (kit-editor `useKitSession`): the agent's.
const AGENT: &str = r#"["agent","agent:test",null]"#;
const PERSON: &str = r#"["person","person:test",null]"#;

fn board() -> BoardDocument {
    BoardDocument::create(FILE, 1920, 1080).unwrap()
}

/// Creates one shape at `[x, y]` as `origin` (the person's for `None`) and answers its id.
fn shape(doc: &mut BoardDocument, x: f64, y: f64, origin: Option<&Origin>) -> String {
    let params = json!({ "elements": [{ "kind": "shape", "position": [x, y], "size": [40.0, 30.0] }] });
    let created = match origin {
        Some(origin) => doc.execute_as("element.add", params, origin).unwrap(),
        None => doc.execute("element.add", params).unwrap(),
    };
    created["created"][0].as_str().unwrap().to_owned()
}

fn has(doc: &BoardDocument, id: &str) -> bool {
    doc.model().element(&ObjectId::from_string(id)).is_some()
}

fn state(doc: &BoardDocument) -> Value {
    doc.session().replica().state_json().unwrap()
}

/// The engine's own view of the board: what `inspect` answers (pages, counts, the frame), its
/// history flags aside.
fn view(doc: &BoardDocument) -> Value {
    let mut view = doc.inspect().unwrap();
    let map = view.as_object_mut().unwrap();
    map.remove("canUndo");
    map.remove("canRedo");
    view
}

/// The view of a board opened afresh from `doc`'s replica: what `doc`'s view must be.
fn fresh_view(doc: &BoardDocument) -> Value {
    view(&BoardDocument::open_with_replica(FILE, &[], &__snapshot(doc).unwrap(), &doc.blobs_bundle()).unwrap())
}

/// An agent's draft of `doc`: a replica of it, and the version it was made at.
fn draft_of(doc: &BoardDocument) -> (BoardDocument, Version) {
    let draft = BoardDocument::open_with_replica(FILE, &[], &__snapshot(doc).unwrap(), &doc.blobs_bundle()).unwrap();
    let base = draft.version();
    assert_eq!(base, doc.version());
    (draft, base)
}

fn done(answer: Value) -> bool {
    answer["done"].as_bool().unwrap()
}

#[test]
fn an_agents_draft_is_one_step_of_the_persons_undo_and_undoes_exactly() {
    let mut doc = board();
    let mine = shape(&mut doc, 10.0, 10.0, None);
    let (mut draft, base) = draft_of(&doc);
    let agent = Origin::agent("agent:test");
    // Far out, so the merge grows the board's frame.
    let theirs = shape(&mut draft, 3000.0, 2500.0, Some(&agent));
    // Meanwhile the person adds another shape to the same page: both insertions merge.
    let meanwhile = shape(&mut doc, 200.0, 10.0, None);
    doc.mark_saved();
    let (before_state, before_view, revision) = (state(&doc), view(&doc), doc.revision());
    assert!(!doc.session().can_undo_in(UndoScope::Agent));

    let changes = __export_changes(&draft, &base.encode()).unwrap();
    let answer: Value = serde_json::from_str(&__merge_changes(&mut doc, &changes, AGENT).unwrap()).unwrap();
    assert_ne!(answer["damage"], json!({ "kind": "none" }));
    assert!(has(&doc, &mine) && has(&doc, &meanwhile) && has(&doc, &theirs), "both sides' shapes are there");
    let page = doc.model().pages()[0].elements.iter().map(|id| id.as_str().to_owned()).collect::<Vec<_>>();
    assert_eq!(page, vec![mine.clone(), meanwhile.clone(), theirs.clone()], "at one position the document's item comes first (D6)");
    // The views follow the merge (regressions/issue-merge-stale-views).
    assert!(doc.revision() > revision, "the merge is a change of the board");
    assert!(doc.dirty(), "a merged draft is unsaved");
    assert_eq!(view(&doc), fresh_view(&doc), "the engine's view is the merged board's");
    let (merged_state, merged_view) = (state(&doc), view(&doc));
    assert_ne!(merged_view, before_view, "the frame grew");

    // M12a: the person's step, not the agent's; the engine's history is the replica's.
    assert!(doc.session().can_undo_in(UndoScope::Person));
    assert!(!doc.session().can_undo_in(UndoScope::Agent), "the agent has no step to take it back");
    assert!(!doc.disabled_commands().contains(&"edit.undo".to_owned()));
    assert!(!done(doc.execute("edit.undo", json!({ "scope": "agent" })).unwrap()));
    assert!(!done(doc.execute_as("edit.undo", json!({}), &agent).unwrap()), "an agent's Undo does not take the merge back");
    assert_eq!(state(&doc), merged_state);

    assert!(done(doc.execute("edit.undo", json!({})).unwrap()), "the person's Undo takes the merge back");
    assert_eq!(state(&doc), before_state, "exactly the state before the merge");
    assert_eq!(view(&doc), before_view, "and the engine's view of it");
    assert!(has(&doc, &meanwhile), "the edit made meanwhile stays");
    assert!(done(doc.execute("edit.redo", json!({})).unwrap()));
    assert_eq!(state(&doc), merged_state, "Redo gives the merged state back exactly");
    assert_eq!(view(&doc), merged_view);
}

#[test]
fn a_restored_state_is_one_step_of_the_persons_undo_and_undoes_exactly() {
    let mut doc = board();
    let kept = shape(&mut doc, 10.0, 10.0, None);
    let checkpoint = __snapshot(&doc).unwrap();
    let checkpoint_state = state(&doc);
    // Meanwhile: another shape on the same page, far out, and a move of the first.
    let later = shape(&mut doc, 3000.0, 2500.0, None);
    doc.execute("element.move", json!({ "ids": [kept], "delta": [25, 0] })).unwrap();
    doc.mark_saved();
    let (before_state, before_view, revision) = (state(&doc), view(&doc), doc.revision());

    let damage: Value = serde_json::from_str(&__restore_state(&mut doc, &checkpoint, PERSON).unwrap()).unwrap();
    assert_ne!(damage, json!({ "kind": "none" }));
    assert_eq!(state(&doc), checkpoint_state, "the checkpoint's state");
    assert!(!has(&doc, &later));
    assert!(doc.revision() > revision, "the restore is a change of the board");
    assert!(doc.dirty());
    assert_eq!(view(&doc), fresh_view(&doc), "the engine's view is the restored board's");
    let (restored_state, restored_view) = (state(&doc), view(&doc));
    assert_ne!(restored_view, before_view, "the frame shrank");

    assert!(done(doc.execute("edit.undo", json!({})).unwrap()), "one Undo takes the restore back");
    assert_eq!(state(&doc), before_state, "exactly the state before the restore");
    assert_eq!(view(&doc), before_view);
    assert!(done(doc.execute("edit.redo", json!({})).unwrap()));
    assert_eq!(state(&doc), restored_state);
    assert_eq!(view(&doc), restored_view);
}

/// A draft the board's repair takes back whole records nothing (kition-sdk 6a57d68, data model
/// 6.6 D9): the agent's draft only puts a shape into a frame the person deleted meanwhile, and the
/// repair (V4) clears the dangling parent, so the merge leaves the board as it was: the state, the
/// version, the person's undo depth, the revision and the view stay, the merge answers no scope
/// and no damage, and the person's next Undo takes back their own deletion exactly. Before, the
/// merge recorded a step that changed nothing, which the person's Undo passed over (case
/// regressions/issue-merge-cancelled-by-repair).
#[test]
fn a_merge_the_repair_takes_back_whole_records_nothing() {
    let mut doc = board();
    let mine = shape(&mut doc, 10.0, 10.0, None);
    let frame = doc.execute("frame.add", json!({ "box": [500, 0, 900, 300] })).unwrap()["frame"].as_str().unwrap().to_owned();
    let (mut draft, base) = draft_of(&doc);
    let agent = Origin::agent("agent:test");
    draft.execute_as("element.set", json!({ "id": mine, "set": { "parent": frame } }), &agent).unwrap();
    let changes = __export_changes(&draft, &base.encode()).unwrap();

    let with_frame = state(&doc);
    doc.execute("element.delete", json!({ "ids": [frame] })).unwrap();
    assert!(!has(&doc, &frame));
    let (deleted, deleted_view, version, revision) = (state(&doc), view(&doc), doc.version(), doc.revision());
    let depth = doc.session().undo_depth(UndoScope::Person);

    let answer: Value = serde_json::from_str(&__merge_changes(&mut doc, &changes, AGENT).unwrap()).unwrap();
    assert_eq!(answer["scope"], Value::Null, "no step was recorded: {answer}");
    assert_eq!(answer["damage"], json!({ "kind": "none" }), "{answer}");
    assert_eq!(state(&doc), deleted, "the repair took the draft back whole: the merge changes nothing");
    assert_eq!(doc.version(), version, "no change is logged");
    assert_eq!(doc.session().undo_depth(UndoScope::Person), depth, "the merge records no step");
    assert_eq!(doc.revision(), revision, "the views did not change");
    assert_eq!(view(&doc), deleted_view);

    assert!(done(doc.execute("edit.undo", json!({})).unwrap()), "the person's Undo takes back their deletion");
    assert_eq!(state(&doc), with_frame, "exactly the state before the deletion");
    assert!(done(doc.execute("edit.redo", json!({})).unwrap()));
    assert_eq!(state(&doc), deleted, "Redo deletes the frame again");
}
