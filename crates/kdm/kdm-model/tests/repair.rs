//! Validation and repair (design note 1.5): rules V1–V9, each where its broken state can arise.
//!
//! - A file written elsewhere (data model 7.3: a file is content anyone may write) can hold any of
//!   them: one case per rule writes the broken board without the session's repair, saves it as a
//!   `.kboard` and opens it (`file::open_file`, which repairs once).
//! - The session's own entries repair through `KitModel::validate` (`KitSession::settle`): an
//!   agent's draft merged into a document edited meanwhile (data model 6.6, D1–D9) gives V3–V9,
//!   the person's undo of a step an agent's later step builds on gives V2 (and its redo the page
//!   back without dead entries), and a replica following the change log (the host's store, the
//!   editor page's) imports the merge with its repair and repairs nothing of its own. V1 does
//!   not arise from a merge (a draft moving an element the document deleted collides, D6) and an
//!   undo finds a list item where it is now, so V1 comes from files (and the reread of one).
//!
//! Plus determinism (M4): one broken state repairs alike whatever order and way its edits came
//! in, and a healthy board writes nothing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod common;

use std::collections::BTreeMap;

use common::*;
use kition_board_kdm::blobs::SharedBlobs;
use kition_board_kdm::{End, file, root_id, schema};
use kition_kit::kdm::{ActorId, Origin, Replica, Tx, Version};
use kition_kit::session::KitSession;
use serde_json::{Value, json};

fn agent() -> Origin {
    Origin::agent("agent:test")
}

/// `session`'s board as a file written elsewhere: saved as a `.kboard` without the session's
/// repair, then opened (`file::open_file` repairs once). Opening changed the broken state, and a
/// second repair writes nothing.
fn opened_file(session: &Session) -> Replica {
    let saved = file::save_file(session.replica(), BTreeMap::new(), None).unwrap();
    let mut replica = file::open_file(&saved.bytes, ActorId::random()).unwrap().replica;
    assert_ne!(replica.state_json().unwrap(), state(session), "opening the broken board repaired it");
    assert!(!file::repair_once(&mut replica).unwrap(), "the repaired board needs no more repair");
    replica
}

/// `session`'s board opened from a file written elsewhere, as a session.
fn repaired_and_settled(session: &Session) -> Session {
    KitSession::from_replica(opened_file(session), Box::new(SharedBlobs::default())).unwrap()
}

/// The session's state needs no more repair: its own (after a merge, an undo or an import) was
/// complete.
fn settled(session: &Session) {
    let mut replica = Replica::load(schema(), &session.snapshot().unwrap(), ActorId::random()).unwrap();
    assert!(!file::repair_once(&mut replica).unwrap(), "the session's own repair left something to repair");
}

/// An agent's draft of `session` (`KitSession::fork`) and the version it was made at.
fn draft(session: &Session) -> (Session, Version) {
    let draft = session.fork(ActorId::random(), Box::new(SharedBlobs::default())).unwrap();
    let base = draft.version();
    (draft, base)
}

/// Runs `f` as one agent transaction.
fn agent_write<R>(session: &mut Session, f: impl FnOnce(&mut Tx<'_>) -> kition_kit::session::Result<R>) -> R {
    session.execute(&agent(), |_, tx| f(tx)).unwrap().0
}

/// Takes `draft`'s changes since `base` into `session` as the person accepting the agent's draft
/// (`merge_changes`, which repairs the merged state). A replica that follows `session`'s change
/// log takes the merge and its repair as they are and repairs nothing of its own; the result needs
/// no more repair.
fn merge(session: &mut Session, draft: &Session, base: &Version) {
    let mut follower = fork(session);
    session.merge_changes(&draft.export_changes(base).unwrap(), &agent()).unwrap();
    follow(&mut follower, session);
    assert_eq!(state(&follower), state(session));
    settled(session);
}

#[test]
fn a_healthy_board_writes_no_repair() {
    let mut a = board();
    let page = first_page(&a);
    write(&mut a, |tx| {
        let frame = add(tx, &page, frame(0.0, 0.0, 400.0, 300.0));
        let mut child = shape(20.0, 20.0, 100.0, 60.0);
        child["parent"] = json!(frame.as_str());
        let child = add(tx, &page, child);
        add(tx, &page, connector_from(&child, [1.0, 0.5], [600.0, 40.0]));
        Ok(())
    });
    let before = a.version();
    let mut replica = kition_kit::kdm::Replica::load(kition_board_kdm::schema(), &a.snapshot().unwrap(), kition_kit::kdm::ActorId::random()).unwrap();
    assert!(!file::repair_once(&mut replica).unwrap());
    a.undo().unwrap();
    a.redo().unwrap();
    assert_ne!(a.version(), before, "undo and redo wrote");
    assert!(a.can_undo());
}

/// V1 (convergence finding 1): a page deleted and still listed leaves no entry.
#[test]
fn v1_drops_page_entries_naming_nothing() {
    let mut a = board();
    let second = write(&mut a, |tx| Ok(add_page(tx, "Two")));
    write(&mut a, |tx| {
        tx.list_move(&root_id(), "pages", 1, 0)?;
        tx.delete(&second)?;
        Ok(())
    });
    let a = repaired_and_settled(&a);
    assert_eq!(a.replica().list(&root_id(), "pages").unwrap().len(), 1);
    assert_eq!(a.model().pages().len(), 1);
}

/// V1: an element moved in its list and deleted while still listed leaves no entry.
#[test]
fn v1_drops_element_entries_naming_nothing() {
    let mut a = board();
    let page = first_page(&a);
    let (one, _) = write(&mut a, |tx| Ok((add(tx, &page, shape(0.0, 0.0, 10.0, 10.0)), add(tx, &page, shape(20.0, 0.0, 10.0, 10.0)))));
    write(&mut a, |tx| {
        tx.list_move(&page, "elements", 0, 1)?;
        tx.delete(&one)?;
        Ok(())
    });
    let a = repaired_and_settled(&a);
    assert_eq!(listed(&a, &page).len(), 1);
    assert!(listed(&a, &page).iter().all(|id| a.replica().exists(id)));
}

/// V2 (convergence finding 2): an element left on a deleted page is deleted, as the page deletion
/// deleted the others; so is its child.
#[test]
fn v2_deletes_elements_left_on_a_deleted_page_with_their_children() {
    let mut a = board();
    let second = write(&mut a, |tx| Ok(add_page(tx, "Two")));
    let (frame_id, child) = write(&mut a, |tx| {
        let frame_id = add(tx, &second, frame(0.0, 0.0, 300.0, 200.0));
        let mut child = shape(10.0, 10.0, 50.0, 50.0);
        child["parent"] = json!(frame_id.as_str());
        Ok((frame_id, add(tx, &second, child)))
    });
    write(&mut a, |tx| {
        remove(tx, &root_id(), "pages", &second);
        Ok(())
    });
    let a = repaired_and_settled(&a);
    assert!(!a.replica().exists(&frame_id));
    assert!(!a.replica().exists(&child));
    assert!(a.replica().objects_of_type("Element").is_empty());
}

/// V3: an element listed on two pages stays on the page whose id sorts first.
#[test]
fn v3_keeps_an_element_moved_to_two_pages_on_one() {
    let mut a = board();
    let first = first_page(&a);
    let (two, three, element) = write(&mut a, |tx| Ok((add_page(tx, "Two"), add_page(tx, "Three"), add(tx, &first, shape(0.0, 0.0, 10.0, 10.0)))));
    write(&mut a, |tx| {
        tx.list_remove(&first, "elements", 0)?;
        tx.list_insert(&two, "elements", 0, &element)?;
        tx.list_insert(&three, "elements", 0, &element)?;
        Ok(())
    });
    let a = repaired_and_settled(&a);
    let keeper = if two < three { &two } else { &three };
    let other = if two < three { &three } else { &two };
    assert_eq!(listed(&a, keeper), vec![element.clone()]);
    assert!(listed(&a, other).is_empty());
    assert_eq!(a.model().page_of(&element).map(|page| page.id.clone()), Some(keeper.clone()));
}

/// V4: a parent deleted after a child was reparented into it is cleared, and so is a parent that
/// cannot contain; the child keeps its world position.
#[test]
fn v4_clears_parents_that_are_gone_on_another_page_or_cannot_contain() {
    let mut a = board();
    let page = first_page(&a);
    let (frame_id, child, plain) = write(&mut a, |tx| {
        Ok((
            add(tx, &page, frame(0.0, 0.0, 300.0, 200.0)),
            add(tx, &page, shape(10.0, 10.0, 50.0, 50.0)),
            add(tx, &page, shape(100.0, 10.0, 50.0, 50.0)),
        ))
    });
    write(&mut a, |tx| {
        tx.set(&child, "parent", json!(frame_id.as_str()))?;
        // A shape cannot contain: a parent the board does not allow.
        tx.set(&plain, "parent", json!(child.as_str()))?;
        Ok(())
    });
    write(&mut a, |tx| {
        remove(tx, &page, "elements", &frame_id);
        Ok(())
    });
    let a = repaired_and_settled(&a);
    assert_eq!(a.replica().get(&child, "parent").unwrap(), Value::Null);
    assert_eq!(a.replica().get(&plain, "parent").unwrap(), Value::Null);
    assert_eq!(a.model().element(&child).unwrap().position, Some(kition_board_kdm::Point::new(10.0, 10.0)));
}

/// V4: a parent moved to another page after a child joined it is cleared.
#[test]
fn v4_clears_a_parent_on_another_page() {
    let mut a = board();
    let page = first_page(&a);
    let (two, frame_id, child) =
        write(&mut a, |tx| Ok((add_page(tx, "Two"), add(tx, &page, frame(0.0, 0.0, 300.0, 200.0)), add(tx, &page, shape(10.0, 10.0, 50.0, 50.0)))));
    write(&mut a, |tx| {
        tx.set(&child, "parent", json!(frame_id.as_str()))?;
        Ok(())
    });
    write(&mut a, |tx| {
        tx.list_remove(&page, "elements", 0)?;
        tx.list_insert(&two, "elements", 0, &frame_id)?;
        Ok(())
    });
    let a = repaired_and_settled(&a);
    assert_eq!(a.replica().get(&child, "parent").unwrap(), Value::Null);
}

/// V5 (convergence finding 3): two frames nested in each other lose the cycle on the frame whose
/// id sorts first.
#[test]
fn v5_breaks_parent_cycles_on_the_smallest_id() {
    let mut a = board();
    let page = first_page(&a);
    let (one, two) = write(&mut a, |tx| Ok((add(tx, &page, frame(0.0, 0.0, 300.0, 200.0)), add(tx, &page, frame(20.0, 20.0, 100.0, 100.0)))));
    write(&mut a, |tx| {
        tx.set(&one, "parent", json!(two.as_str()))?;
        Ok(())
    });
    write(&mut a, |tx| {
        tx.set(&two, "parent", json!(one.as_str()))?;
        Ok(())
    });
    let a = repaired_and_settled(&a);
    let (first, second) = if one < two { (&one, &two) } else { (&two, &one) };
    assert_eq!(a.replica().get(first, "parent").unwrap(), Value::Null);
    assert_eq!(a.replica().get(second, "parent").unwrap(), json!(first.as_str()));
}

/// V6: a binding to an element deleted since, or to one that cannot be bound, is dropped; the end
/// keeps its point.
#[test]
fn v6_drops_bindings_to_deleted_and_unbindable_elements() {
    let mut a = board();
    let page = first_page(&a);
    let (target, stroke, connector) = write(&mut a, |tx| {
        let target = add(tx, &page, shape(100.0, 100.0, 100.0, 50.0));
        let stroke = add(tx, &page, json!({ "kind": "stroke", "points": [[0, 0], [10, 10]] }));
        let connector = add(tx, &page, json!({ "kind": "connector", "start": { "point": [0, 0] }, "end": { "point": [500, 500] } }));
        Ok((target, stroke, connector))
    });
    write(&mut a, |tx| {
        tx.set(&connector, "start", json!({ "point": [200, 125], "binding": { "target": target.as_str(), "anchor": [1, 0.5] } }))?;
        tx.set(&connector, "end", json!({ "point": [5, 5], "binding": { "target": stroke.as_str(), "anchor": [0.5, 0.5] } }))?;
        Ok(())
    });
    write(&mut a, |tx| {
        remove(tx, &page, "elements", &target);
        Ok(())
    });
    let a = repaired_and_settled(&a);
    let start = End::parse(&a.replica().get(&connector, "start").unwrap()).unwrap();
    let end = End::parse(&a.replica().get(&connector, "end").unwrap()).unwrap();
    assert!(start.binding.is_none() && end.binding.is_none());
    assert_eq!(start.point, kition_board_kdm::Point::new(200.0, 125.0), "a deleted target leaves the stored point");
}

/// V6: a binding to an element moved to another page is dropped at the target's last derived
/// point.
#[test]
fn v6_drops_a_binding_to_another_page_at_the_derived_point() {
    let mut a = board();
    let page = first_page(&a);
    let (two, target, connector) = write(&mut a, |tx| {
        let two = add_page(tx, "Two");
        let target = add(tx, &page, shape(100.0, 100.0, 100.0, 50.0));
        let connector = add(tx, &page, connector_from(&target, [1.0, 0.5], [500.0, 500.0]));
        Ok((two, target, connector))
    });
    write(&mut a, |tx| {
        tx.list_remove(&page, "elements", 0)?;
        tx.list_insert(&two, "elements", 0, &target)?;
        Ok(())
    });
    let replica = opened_file(&a);
    let start = End::parse(&replica.get(&connector, "start").unwrap()).unwrap();
    assert!(start.binding.is_none());
    assert_eq!(start.point, kition_board_kdm::Point::new(200.0, 125.0));
}

/// V7: an anchor outside [0, 1] is clamped.
#[test]
fn v7_clamps_anchors() {
    let mut a = board();
    let page = first_page(&a);
    let connector = write(&mut a, |tx| {
        let target = add(tx, &page, shape(0.0, 0.0, 100.0, 100.0));
        Ok(add(tx, &page, connector_from(&target, [1.5, -0.25], [300.0, 300.0])))
    });
    let replica = opened_file(&a);
    let start = End::parse(&replica.get(&connector, "start").unwrap()).unwrap();
    assert_eq!(start.binding.unwrap().anchor, [1.0, 0.0]);
}

/// V8: one key given to two elements of a page stays on the smaller id.
#[test]
fn v8_keeps_a_key_on_one_element_per_page() {
    let mut a = board();
    let page = first_page(&a);
    let (one, two) = write(&mut a, |tx| Ok((add(tx, &page, shape(0.0, 0.0, 10.0, 10.0)), add(tx, &page, shape(20.0, 0.0, 10.0, 10.0)))));
    write(&mut a, |tx| {
        tx.set(&one, "key", json!("start"))?;
        Ok(())
    });
    write(&mut a, |tx| {
        tx.set(&two, "key", json!("start"))?;
        Ok(())
    });
    let a = repaired_and_settled(&a);
    let (first, second) = if one < two { (&one, &two) } else { (&two, &one) };
    assert_eq!(a.replica().get(first, "key").unwrap(), json!("start"));
    assert_eq!(a.replica().get(second, "key").unwrap(), Value::Null);
}

/// V9: a group left without children (its last child moved out) is deleted, and so is a group
/// that only held it.
#[test]
fn v9_deletes_empty_groups() {
    let mut a = board();
    let page = first_page(&a);
    let (outer, inner, child) = write(&mut a, |tx| {
        let outer = add(tx, &page, json!({ "kind": "group" }));
        let inner = add(tx, &page, json!({ "kind": "group", "parent": outer.as_str() }));
        let mut child = shape(0.0, 0.0, 10.0, 10.0);
        child["parent"] = json!(inner.as_str());
        Ok((outer, inner, add(tx, &page, child)))
    });
    write(&mut a, |tx| {
        tx.set(&child, "parent", Value::Null)?;
        Ok(())
    });
    let a = repaired_and_settled(&a);
    assert!(!a.replica().exists(&inner));
    assert!(!a.replica().exists(&outer));
    assert_eq!(listed(&a, &page), vec![child]);
}

/// M4: the repair's result depends on the state alone, not on the order or the way its edits came
/// in. One broken state (a parent cycle and a binding to a deleted element) is reached three ways:
/// the person's two edits in one order and in the other (each board then saved and opened as a
/// file), and an agent's draft holding the first edit merged into a document that made the second
/// meanwhile (the session's own repair). All three repair to one state: the cycle broken at the
/// frame whose id sorts first, the binding dropped at its stored point.
#[test]
fn repairs_are_deterministic_whatever_order_the_edits_came_in() {
    let mut origin = board();
    let page = first_page(&origin);
    let (one, two, target, connector) = write(&mut origin, |tx| {
        let one = add(tx, &page, frame(0.0, 0.0, 300.0, 200.0));
        let two = add(tx, &page, frame(10.0, 10.0, 100.0, 100.0));
        let target = add(tx, &page, shape(400.0, 0.0, 100.0, 100.0));
        let connector = add(tx, &page, json!({ "kind": "connector", "start": { "point": [0, 0] }, "end": { "point": [800, 50] } }));
        Ok((one, two, target, connector))
    });
    let first = |tx: &mut Tx<'_>| -> kition_kit::session::Result<()> {
        tx.set(&one, "parent", json!(two.as_str()))?;
        remove(tx, &page, "elements", &target);
        Ok(())
    };
    let second = |tx: &mut Tx<'_>| -> kition_kit::session::Result<()> {
        tx.set(&two, "parent", json!(one.as_str()))?;
        tx.set(&connector, "start", json!({ "point": [400, 50], "binding": { "target": target.as_str(), "anchor": [0, 0.5] } }))?;
        Ok(())
    };
    let mut a = fork(&origin);
    write(&mut a, first);
    write(&mut a, second);
    let mut b = fork(&origin);
    write(&mut b, second);
    write(&mut b, first);
    assert_eq!(state(&a), state(&b), "one broken state, its edits written in two orders");
    let mut c = fork(&origin);
    let (mut d, base) = draft(&c);
    agent_write(&mut d, first);
    write(&mut c, second);
    merge(&mut c, &d, &base);
    let (a, b) = (repaired_and_settled(&a), repaired_and_settled(&b));
    assert_eq!(state(&a), state(&b), "both files repaired alike");
    assert_eq!(state(&c), state(&a), "the merge's repair writes what opening the file writes");
    let (low, high) = if one < two { (&one, &two) } else { (&two, &one) };
    assert_eq!(c.replica().get(low, "parent").unwrap(), Value::Null);
    assert_eq!(c.replica().get(high, "parent").unwrap(), json!(low.as_str()));
    assert!(!c.replica().exists(&target));
    let start = End::parse(&c.replica().get(&connector, "start").unwrap()).unwrap();
    assert!(start.binding.is_none());
    assert_eq!(start.point, kition_board_kdm::Point::new(400.0, 50.0));
}

/// V7b: a bound end written without its cached `point` (an object-model batch, KHP `_batch`) is
/// still bound, and the repair writes the point derived from the target, so the connector counts
/// as connected instead of being silently dropped.
#[test]
fn v7b_a_bound_end_without_a_point_keeps_its_binding_and_gets_the_point() {
    let mut a = board();
    let page = first_page(&a);
    let (target, connector) = write(&mut a, |tx| {
        let target = add(tx, &page, shape(100.0, 100.0, 100.0, 50.0));
        let connector = add(tx, &page, json!({ "kind": "connector", "start": { "point": [0, 0] }, "end": { "point": [500, 500] } }));
        Ok((target, connector))
    });
    let bound = json!({ "binding": { "target": target.as_str(), "anchor": [1, 0.5] } });
    let parsed = End::parse(&bound).expect("a bound end without a point parses");
    assert_eq!(parsed.binding.as_ref().map(|b| b.target.as_str()), Some(target.as_str()));
    write(&mut a, |tx| {
        tx.set(&connector, "start", bound.clone())?;
        Ok(())
    });
    // Opening it writes the missing point.
    let replica = opened_file(&a);
    let start = End::parse(&replica.get(&connector, "start").unwrap()).unwrap();
    assert_eq!(start.binding.map(|b| b.target), Some(target));
    assert_eq!(start.point, kition_board_kdm::Point::new(200.0, 125.0), "the point derived from the target's right edge");
}

/// V1 cannot come from a draft: a draft that moves an element the document deleted meanwhile
/// collides (data model D6) and the merge changes nothing.
#[test]
fn v1_a_draft_moving_an_element_the_document_deleted_is_refused() {
    let mut a = board();
    let page = first_page(&a);
    let (one, _) = write(&mut a, |tx| Ok((add(tx, &page, shape(0.0, 0.0, 10.0, 10.0)), add(tx, &page, shape(20.0, 0.0, 10.0, 10.0)))));
    let (mut d, base) = draft(&a);
    agent_write(&mut d, |tx| Ok(tx.list_move(&page, "elements", 0, 1)?));
    write(&mut a, |tx| {
        remove(tx, &page, "elements", &one);
        Ok(())
    });
    let (before, version) = (state(&a), a.version());
    assert!(a.merge_changes(&d.export_changes(&base).unwrap(), &agent()).is_err());
    assert_eq!(state(&a), before);
    assert_eq!(a.version(), version);
}

/// V2 through the person's undo: the person adds a page, an agent then adds a frame with a child
/// on it, and the person's Undo takes the page back; the agent's elements, left on no page, go
/// with it in the undo's repair. The person's Redo brings the page back empty, naming none of
/// them (V1 when the redo restores the list it took).
#[test]
fn v2_the_persons_undo_of_a_page_an_agent_drew_on_deletes_the_agents_elements() {
    let mut a = board();
    let second = write(&mut a, |tx| Ok(add_page(tx, "Two")));
    let (frame_id, child) = agent_write(&mut a, |tx| {
        let frame_id = add(tx, &second, frame(0.0, 0.0, 300.0, 200.0));
        let mut child = shape(10.0, 10.0, 50.0, 50.0);
        child["parent"] = json!(frame_id.as_str());
        Ok((frame_id, add(tx, &second, child)))
    });
    assert!(a.undo().unwrap().0);
    assert!(!a.replica().exists(&second));
    assert!(!a.replica().exists(&frame_id));
    assert!(!a.replica().exists(&child));
    assert!(a.replica().objects_of_type("Element").is_empty());
    assert_eq!(a.model().pages().len(), 1);
    settled(&a);
    assert!(a.redo().unwrap().0);
    assert_eq!(a.replica().list(&root_id(), "pages").unwrap().len(), 2);
    assert!(listed(&a, &second).is_empty());
    assert!(a.replica().objects_of_type("Element").is_empty());
    settled(&a);
}

/// V3 through a draft merge: the agent's draft moves an element to page two while the person moves
/// it to page three. Both took it off the first page (one change, D6) and put it on a list the
/// other did not change, so it merges onto two pages; the repair keeps it on the page whose id
/// sorts first.
#[test]
fn v3_an_element_moved_to_two_pages_by_a_draft_and_the_person_stays_on_one() {
    let mut a = board();
    let first = first_page(&a);
    let (two, three, element) = write(&mut a, |tx| Ok((add_page(tx, "Two"), add_page(tx, "Three"), add(tx, &first, shape(0.0, 0.0, 10.0, 10.0)))));
    let (mut d, base) = draft(&a);
    agent_write(&mut d, |tx| {
        tx.list_remove(&first, "elements", 0)?;
        tx.list_insert(&two, "elements", 0, &element)?;
        Ok(())
    });
    write(&mut a, |tx| {
        tx.list_remove(&first, "elements", 0)?;
        tx.list_insert(&three, "elements", 0, &element)?;
        Ok(())
    });
    merge(&mut a, &d, &base);
    let keeper = if two < three { &two } else { &three };
    let other = if two < three { &three } else { &two };
    assert_eq!(listed(&a, keeper), vec![element.clone()]);
    assert!(listed(&a, other).is_empty());
    assert!(listed(&a, &first).is_empty());
    assert_eq!(a.model().page_of(&element).map(|page| page.id.clone()), Some(keeper.clone()));
}

/// V4 through a draft merge: the draft puts a shape in a frame the person deleted meanwhile, and a
/// shape under a shape (which cannot contain); both parents are cleared and the child keeps its
/// world position.
#[test]
fn v4_a_draft_parenting_into_a_frame_the_person_deleted_is_cleared() {
    let mut a = board();
    let page = first_page(&a);
    let (frame_id, child, plain) = write(&mut a, |tx| {
        Ok((
            add(tx, &page, frame(0.0, 0.0, 300.0, 200.0)),
            add(tx, &page, shape(10.0, 10.0, 50.0, 50.0)),
            add(tx, &page, shape(100.0, 10.0, 50.0, 50.0)),
        ))
    });
    let (mut d, base) = draft(&a);
    agent_write(&mut d, |tx| {
        tx.set(&child, "parent", json!(frame_id.as_str()))?;
        tx.set(&plain, "parent", json!(child.as_str()))?;
        Ok(())
    });
    write(&mut a, |tx| {
        remove(tx, &page, "elements", &frame_id);
        Ok(())
    });
    merge(&mut a, &d, &base);
    assert!(!a.replica().exists(&frame_id));
    assert_eq!(a.replica().get(&child, "parent").unwrap(), Value::Null);
    assert_eq!(a.replica().get(&plain, "parent").unwrap(), Value::Null);
    assert_eq!(a.model().element(&child).unwrap().position, Some(kition_board_kdm::Point::new(10.0, 10.0)));
}

/// V4 through a draft merge: the draft puts a shape in a frame the person moved to another page
/// meanwhile; the parent is cleared.
#[test]
fn v4_a_draft_parenting_into_a_frame_the_person_moved_away_is_cleared() {
    let mut a = board();
    let page = first_page(&a);
    let (two, frame_id, child) =
        write(&mut a, |tx| Ok((add_page(tx, "Two"), add(tx, &page, frame(0.0, 0.0, 300.0, 200.0)), add(tx, &page, shape(10.0, 10.0, 50.0, 50.0)))));
    let (mut d, base) = draft(&a);
    agent_write(&mut d, |tx| Ok(tx.set(&child, "parent", json!(frame_id.as_str()))?));
    write(&mut a, |tx| {
        tx.list_remove(&page, "elements", 0)?;
        tx.list_insert(&two, "elements", 0, &frame_id)?;
        Ok(())
    });
    merge(&mut a, &d, &base);
    assert_eq!(a.replica().get(&child, "parent").unwrap(), Value::Null);
    assert_eq!(listed(&a, &two), vec![frame_id]);
}

/// V5 through a draft merge: the draft nests one frame in the other while the person nests them the
/// other way round (different objects, so it merges); the cycle loses the parent of the frame
/// whose id sorts first.
#[test]
fn v5_frames_nested_both_ways_by_a_draft_and_the_person_lose_the_cycle() {
    let mut a = board();
    let page = first_page(&a);
    let (one, two) = write(&mut a, |tx| Ok((add(tx, &page, frame(0.0, 0.0, 300.0, 200.0)), add(tx, &page, frame(20.0, 20.0, 100.0, 100.0)))));
    let (mut d, base) = draft(&a);
    agent_write(&mut d, |tx| Ok(tx.set(&one, "parent", json!(two.as_str()))?));
    write(&mut a, |tx| Ok(tx.set(&two, "parent", json!(one.as_str()))?));
    merge(&mut a, &d, &base);
    let (first, second) = if one < two { (&one, &two) } else { (&two, &one) };
    assert_eq!(a.replica().get(first, "parent").unwrap(), Value::Null);
    assert_eq!(a.replica().get(second, "parent").unwrap(), json!(first.as_str()));
}

/// V6 through a draft merge: the draft binds a connector to a shape the person deleted meanwhile,
/// and its other end to a stroke (which cannot be bound); both bindings go, the end at the deleted
/// shape keeps its stored point.
#[test]
fn v6_a_draft_binding_to_a_shape_the_person_deleted_is_dropped() {
    let mut a = board();
    let page = first_page(&a);
    let (target, stroke, connector) = write(&mut a, |tx| {
        let target = add(tx, &page, shape(100.0, 100.0, 100.0, 50.0));
        let stroke = add(tx, &page, json!({ "kind": "stroke", "points": [[0, 0], [10, 10]] }));
        let connector = add(tx, &page, json!({ "kind": "connector", "start": { "point": [0, 0] }, "end": { "point": [500, 500] } }));
        Ok((target, stroke, connector))
    });
    let (mut d, base) = draft(&a);
    agent_write(&mut d, |tx| {
        tx.set(&connector, "start", json!({ "point": [200, 125], "binding": { "target": target.as_str(), "anchor": [1, 0.5] } }))?;
        tx.set(&connector, "end", json!({ "point": [5, 5], "binding": { "target": stroke.as_str(), "anchor": [0.5, 0.5] } }))?;
        Ok(())
    });
    write(&mut a, |tx| {
        remove(tx, &page, "elements", &target);
        Ok(())
    });
    merge(&mut a, &d, &base);
    let start = End::parse(&a.replica().get(&connector, "start").unwrap()).unwrap();
    let end = End::parse(&a.replica().get(&connector, "end").unwrap()).unwrap();
    assert!(start.binding.is_none() && end.binding.is_none());
    assert_eq!(start.point, kition_board_kdm::Point::new(200.0, 125.0), "a deleted target leaves the stored point");
}

/// V6 through a draft merge: the draft draws a connector from a shape the person moved to another
/// page meanwhile; the binding is dropped at the shape's derived point.
#[test]
fn v6_a_draft_connector_from_a_shape_the_person_moved_away_is_unbound_at_the_derived_point() {
    let mut a = board();
    let page = first_page(&a);
    let (two, target) = write(&mut a, |tx| Ok((add_page(tx, "Two"), add(tx, &page, shape(100.0, 100.0, 100.0, 50.0)))));
    let (mut d, base) = draft(&a);
    let connector = agent_write(&mut d, |tx| Ok(add(tx, &page, connector_from(&target, [1.0, 0.5], [500.0, 500.0]))));
    write(&mut a, |tx| {
        tx.list_remove(&page, "elements", 0)?;
        tx.list_insert(&two, "elements", 0, &target)?;
        Ok(())
    });
    merge(&mut a, &d, &base);
    assert_eq!(listed(&a, &page), vec![connector.clone()]);
    let start = End::parse(&a.replica().get(&connector, "start").unwrap()).unwrap();
    assert!(start.binding.is_none());
    assert_eq!(start.point, kition_board_kdm::Point::new(200.0, 125.0));
}

/// V7 and V7b through a draft merge: a draft whose agent wrote raw values (an object-model batch
/// runs no repair) brings an anchor outside [0, 1] and a bound end without its point; the merge
/// clamps the one and writes the other's derived point.
#[test]
fn v7_a_drafts_anchor_out_of_range_and_bound_end_without_a_point_are_repaired_on_merge() {
    let mut a = board();
    let page = first_page(&a);
    let target = write(&mut a, |tx| Ok(add(tx, &page, shape(100.0, 100.0, 100.0, 50.0))));
    let (mut d, base) = draft(&a);
    let (clamped, pointless) = agent_write(&mut d, |tx| {
        let clamped = add(tx, &page, connector_from(&target, [1.5, -0.25], [300.0, 300.0]));
        let pointless = add(
            tx,
            &page,
            json!({ "kind": "connector", "start": { "binding": { "target": target.as_str(), "anchor": [1, 0.5] } }, "end": { "point": [500, 500] } }),
        );
        Ok((clamped, pointless))
    });
    merge(&mut a, &d, &base);
    let start = End::parse(&a.replica().get(&clamped, "start").unwrap()).unwrap();
    assert_eq!(start.binding.unwrap().anchor, [1.0, 0.0]);
    let start = End::parse(&a.replica().get(&pointless, "start").unwrap()).unwrap();
    assert_eq!(start.binding.map(|b| b.target), Some(target));
    assert_eq!(start.point, kition_board_kdm::Point::new(200.0, 125.0));
}

/// V8 through a draft merge: the draft gives one element a key while the person gives the same key
/// to another (different objects, so it merges); the key stays on the smaller id.
#[test]
fn v8_one_key_given_by_a_draft_and_the_person_stays_on_one_element() {
    let mut a = board();
    let page = first_page(&a);
    let (one, two) = write(&mut a, |tx| Ok((add(tx, &page, shape(0.0, 0.0, 10.0, 10.0)), add(tx, &page, shape(20.0, 0.0, 10.0, 10.0)))));
    let (mut d, base) = draft(&a);
    agent_write(&mut d, |tx| Ok(tx.set(&one, "key", json!("start"))?));
    write(&mut a, |tx| Ok(tx.set(&two, "key", json!("start"))?));
    merge(&mut a, &d, &base);
    let (first, second) = if one < two { (&one, &two) } else { (&two, &one) };
    assert_eq!(a.replica().get(first, "key").unwrap(), json!("start"));
    assert_eq!(a.replica().get(second, "key").unwrap(), Value::Null);
}

/// V9 through a draft merge: a group holds two shapes; the draft takes one out, the person the
/// other meanwhile. The merged group is empty and goes, and so does the group that only held it.
#[test]
fn v9_a_group_emptied_by_a_draft_and_the_person_is_deleted() {
    let mut a = board();
    let page = first_page(&a);
    let (outer, inner, left, right) = write(&mut a, |tx| {
        let outer = add(tx, &page, json!({ "kind": "group" }));
        let inner = add(tx, &page, json!({ "kind": "group", "parent": outer.as_str() }));
        let mut left = shape(0.0, 0.0, 10.0, 10.0);
        left["parent"] = json!(inner.as_str());
        let mut right = shape(20.0, 0.0, 10.0, 10.0);
        right["parent"] = json!(inner.as_str());
        Ok((outer, inner, add(tx, &page, left), add(tx, &page, right)))
    });
    let (mut d, base) = draft(&a);
    agent_write(&mut d, |tx| Ok(tx.set(&left, "parent", Value::Null)?));
    write(&mut a, |tx| Ok(tx.set(&right, "parent", Value::Null)?));
    merge(&mut a, &d, &base);
    assert!(!a.replica().exists(&inner));
    assert!(!a.replica().exists(&outer));
    assert_eq!(listed(&a, &page), vec![left, right]);
}

/// A draft's preview runs the board's repair (kition-sdk `KitSession::compare_changes`,
/// `merged_state`; the engine's `compareChanges`), so it is what accepting the draft gives: the V9
/// case above, a group the draft and the person each take one shape out of, previews with the
/// emptied groups deleted. The preview changes nothing; the merge then gives exactly the
/// previewed state and the compared changes, one Undo the exact state before and one Redo the
/// repaired state (case regressions/issue-draft-preview-repaired).
#[test]
fn a_drafts_preview_is_the_repaired_merge() {
    let mut a = board();
    let page = first_page(&a);
    let (outer, inner, left, right) = write(&mut a, |tx| {
        let outer = add(tx, &page, json!({ "kind": "group" }));
        let inner = add(tx, &page, json!({ "kind": "group", "parent": outer.as_str() }));
        let mut left = shape(0.0, 0.0, 10.0, 10.0);
        left["parent"] = json!(inner.as_str());
        let mut right = shape(20.0, 0.0, 10.0, 10.0);
        right["parent"] = json!(inner.as_str());
        Ok((outer, inner, add(tx, &page, left), add(tx, &page, right)))
    });
    let (mut d, base) = draft(&a);
    agent_write(&mut d, |tx| Ok(tx.set(&left, "parent", Value::Null)?));
    write(&mut a, |tx| Ok(tx.set(&right, "parent", Value::Null)?));
    let changes = d.export_changes(&base).unwrap();
    let (before, version) = (a.state_hash().unwrap(), a.version());

    let compared = a.compare_changes(&changes).unwrap().to_json();
    let preview = a.merged_state(&changes).unwrap();
    assert_eq!((a.state_hash().unwrap(), a.version()), (before.clone(), version.clone()), "a preview changes nothing");
    let deleted: Vec<&Value> = compared["deleted"].as_array().unwrap().iter().map(|deleted| &deleted["id"]).collect();
    assert!(deleted.contains(&&json!(inner.to_string())) && deleted.contains(&&json!(outer.to_string())), "the preview has the repair: {compared}");

    a.merge_changes(&changes, &agent()).unwrap();
    assert!(!a.replica().exists(&inner) && !a.replica().exists(&outer));
    assert_eq!(state(&a), preview, "the preview is the merge");
    assert_eq!(a.replica().diff_since(&version).unwrap().to_json(), compared, "the comparison is what the merge changed");
    let repaired = a.state_hash().unwrap();
    assert!(a.undo().unwrap().0);
    assert_eq!(a.state_hash().unwrap(), before, "one undo: the exact state before the merge");
    assert!(a.redo().unwrap().0);
    assert_eq!(a.state_hash().unwrap(), repaired, "one redo: the exact repaired state");
}
