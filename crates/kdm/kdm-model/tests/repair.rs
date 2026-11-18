//! Validation and repair (design note 1.5): one case per rule V1–V9, made the way real boards get
//! there (two replicas editing at once), plus determinism (M4): both replicas repair alike, and a
//! healthy board writes nothing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod common;

use common::*;
use kition_board_kdm::{End, file, root_id};
use kition_kit::kdm::ObjectId;
use serde_json::{Value, json};

/// Both replicas end equal, and a repair on either writes nothing more.
fn assert_converged_and_settled(a: &mut Session, b: &mut Session) {
    sync(a, b);
    assert_eq!(state(a), state(b), "the replicas converge");
    let mut replica = kition_kit::kdm::Replica::load(kition_board_kdm::schema(), &a.snapshot().unwrap(), kition_kit::kdm::ActorId::random()).unwrap();
    assert!(!file::repair_once(&mut replica).unwrap(), "the converged board needs no repair");
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

/// V1 (convergence finding 1): a page deleted while another replica moved it leaves no entry.
#[test]
fn v1_drops_page_entries_naming_nothing() {
    let mut a = board();
    let second = write(&mut a, |tx| Ok(add_page(tx, "Two")));
    let mut b = fork(&a);
    write(&mut a, |tx| {
        remove(tx, &root_id(), "pages", &second);
        Ok(())
    });
    write(&mut b, |tx| {
        tx.list_move(&root_id(), "pages", 1, 0)?;
        Ok(())
    });
    assert_converged_and_settled(&mut a, &mut b);
    assert_eq!(a.replica().list(&root_id(), "pages").unwrap().len(), 1);
    assert_eq!(a.model().pages().len(), 1);
}

/// V1: an element deleted while another replica moved it in its list leaves no entry.
#[test]
fn v1_drops_element_entries_naming_nothing() {
    let mut a = board();
    let page = first_page(&a);
    let (one, _) = write(&mut a, |tx| Ok((add(tx, &page, shape(0.0, 0.0, 10.0, 10.0)), add(tx, &page, shape(20.0, 0.0, 10.0, 10.0)))));
    let mut b = fork(&a);
    write(&mut a, |tx| {
        remove(tx, &page, "elements", &one);
        Ok(())
    });
    write(&mut b, |tx| {
        tx.list_move(&page, "elements", 0, 1)?;
        Ok(())
    });
    assert_converged_and_settled(&mut a, &mut b);
    assert_eq!(listed(&a, &page).len(), 1);
    assert!(listed(&a, &page).iter().all(|id| a.replica().exists(id)));
}

/// V2 (convergence finding 2): an element added to a page another replica deleted is deleted, as
/// the page deletion deleted the others; so is its child.
#[test]
fn v2_deletes_elements_left_on_a_deleted_page_with_their_children() {
    let mut a = board();
    let second = write(&mut a, |tx| Ok(add_page(tx, "Two")));
    let mut b = fork(&a);
    write(&mut a, |tx| {
        remove(tx, &root_id(), "pages", &second);
        Ok(())
    });
    let (frame_id, child) = write(&mut b, |tx| {
        let frame_id = add(tx, &second, frame(0.0, 0.0, 300.0, 200.0));
        let mut child = shape(10.0, 10.0, 50.0, 50.0);
        child["parent"] = json!(frame_id.as_str());
        Ok((frame_id, add(tx, &second, child)))
    });
    assert_converged_and_settled(&mut a, &mut b);
    assert!(!a.replica().exists(&frame_id));
    assert!(!a.replica().exists(&child));
    assert!(a.replica().objects_of_type("Element").is_empty());
}

/// V3: an element moved to two pages at once stays on the page whose id sorts first.
#[test]
fn v3_keeps_an_element_moved_to_two_pages_on_one() {
    let mut a = board();
    let first = first_page(&a);
    let (two, three, element) = write(&mut a, |tx| Ok((add_page(tx, "Two"), add_page(tx, "Three"), add(tx, &first, shape(0.0, 0.0, 10.0, 10.0)))));
    let mut b = fork(&a);
    let move_to = |session: &mut Session, to: &ObjectId| {
        let to = to.clone();
        let element = element.clone();
        let first = first.clone();
        write(session, move |tx| {
            tx.list_remove(&first, "elements", 0)?;
            tx.list_insert(&to, "elements", 0, &element)?;
            Ok(())
        });
    };
    move_to(&mut a, &two);
    move_to(&mut b, &three);
    assert_converged_and_settled(&mut a, &mut b);
    let keeper = if two < three { &two } else { &three };
    let other = if two < three { &three } else { &two };
    assert_eq!(listed(&a, keeper), vec![element.clone()]);
    assert!(listed(&a, other).is_empty());
    assert_eq!(a.model().page_of(&element).map(|page| page.id.clone()), Some(keeper.clone()));
}

/// V4: a parent deleted by one replica while another reparented into it is cleared; the child
/// keeps its world position.
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
    let mut b = fork(&a);
    write(&mut a, |tx| {
        remove(tx, &page, "elements", &frame_id);
        Ok(())
    });
    write(&mut b, |tx| {
        tx.set(&child, "parent", json!(frame_id.as_str()))?;
        // A shape cannot contain: a parent the board does not allow.
        tx.set(&plain, "parent", json!(child.as_str()))?;
        Ok(())
    });
    assert_converged_and_settled(&mut a, &mut b);
    assert_eq!(a.replica().get(&child, "parent").unwrap(), Value::Null);
    assert_eq!(a.replica().get(&plain, "parent").unwrap(), Value::Null);
    assert_eq!(a.model().element(&child).unwrap().position, Some(kition_board_kdm::Point::new(10.0, 10.0)));
}

/// V4: a parent moved to another page while a child joined it is cleared.
#[test]
fn v4_clears_a_parent_on_another_page() {
    let mut a = board();
    let page = first_page(&a);
    let (two, frame_id, child) =
        write(&mut a, |tx| Ok((add_page(tx, "Two"), add(tx, &page, frame(0.0, 0.0, 300.0, 200.0)), add(tx, &page, shape(10.0, 10.0, 50.0, 50.0)))));
    let mut b = fork(&a);
    write(&mut a, |tx| {
        tx.list_remove(&page, "elements", 0)?;
        tx.list_insert(&two, "elements", 0, &frame_id)?;
        Ok(())
    });
    write(&mut b, |tx| {
        tx.set(&child, "parent", json!(frame_id.as_str()))?;
        Ok(())
    });
    assert_converged_and_settled(&mut a, &mut b);
    assert_eq!(a.replica().get(&child, "parent").unwrap(), Value::Null);
}

/// V5 (convergence finding 3): two frames nested in each other at once lose the cycle on the
/// frame whose id sorts first.
#[test]
fn v5_breaks_parent_cycles_on_the_smallest_id() {
    let mut a = board();
    let page = first_page(&a);
    let (one, two) = write(&mut a, |tx| Ok((add(tx, &page, frame(0.0, 0.0, 300.0, 200.0)), add(tx, &page, frame(20.0, 20.0, 100.0, 100.0)))));
    let mut b = fork(&a);
    write(&mut a, |tx| {
        tx.set(&one, "parent", json!(two.as_str()))?;
        Ok(())
    });
    write(&mut b, |tx| {
        tx.set(&two, "parent", json!(one.as_str()))?;
        Ok(())
    });
    assert_converged_and_settled(&mut a, &mut b);
    let (first, second) = if one < two { (&one, &two) } else { (&two, &one) };
    assert_eq!(a.replica().get(first, "parent").unwrap(), Value::Null);
    assert_eq!(a.replica().get(second, "parent").unwrap(), json!(first.as_str()));
}

/// V6: a binding to an element another replica deleted is dropped; the end keeps its point.
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
    let mut b = fork(&a);
    write(&mut a, |tx| {
        remove(tx, &page, "elements", &target);
        Ok(())
    });
    write(&mut b, |tx| {
        tx.set(&connector, "start", json!({ "point": [200, 125], "binding": { "target": target.as_str(), "anchor": [1, 0.5] } }))?;
        tx.set(&connector, "end", json!({ "point": [5, 5], "binding": { "target": stroke.as_str(), "anchor": [0.5, 0.5] } }))?;
        Ok(())
    });
    assert_converged_and_settled(&mut a, &mut b);
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
    let mut replica = kition_kit::kdm::Replica::load(kition_board_kdm::schema(), &a.snapshot().unwrap(), kition_kit::kdm::ActorId::random()).unwrap();
    assert!(file::repair_once(&mut replica).unwrap());
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
    let mut replica = kition_kit::kdm::Replica::load(kition_board_kdm::schema(), &a.snapshot().unwrap(), kition_kit::kdm::ActorId::random()).unwrap();
    assert!(file::repair_once(&mut replica).unwrap());
    let start = End::parse(&replica.get(&connector, "start").unwrap()).unwrap();
    assert_eq!(start.binding.unwrap().anchor, [1.0, 0.0]);
}

/// V8: two replicas giving one key to two elements of a page keep it on the smaller id.
#[test]
fn v8_keeps_a_key_on_one_element_per_page() {
    let mut a = board();
    let page = first_page(&a);
    let (one, two) = write(&mut a, |tx| Ok((add(tx, &page, shape(0.0, 0.0, 10.0, 10.0)), add(tx, &page, shape(20.0, 0.0, 10.0, 10.0)))));
    let mut b = fork(&a);
    write(&mut a, |tx| {
        tx.set(&one, "key", json!("start"))?;
        Ok(())
    });
    write(&mut b, |tx| {
        tx.set(&two, "key", json!("start"))?;
        Ok(())
    });
    assert_converged_and_settled(&mut a, &mut b);
    let (first, second) = if one < two { (&one, &two) } else { (&two, &one) };
    assert_eq!(a.replica().get(first, "key").unwrap(), json!("start"));
    assert_eq!(a.replica().get(second, "key").unwrap(), Value::Null);
}

/// V9: a group left without children (its last child moved out at once elsewhere) is deleted,
/// and so is a group that only held it.
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
    let mut b = fork(&a);
    write(&mut b, |tx| {
        tx.set(&child, "parent", Value::Null)?;
        Ok(())
    });
    assert_converged_and_settled(&mut a, &mut b);
    assert!(!a.replica().exists(&inner));
    assert!(!a.replica().exists(&outer));
    assert_eq!(listed(&a, &page), vec![child]);
}

/// M4: two replicas that reach the same broken state on their own write repairs that merge to
/// one state, whichever arrives first.
#[test]
fn repairs_are_deterministic_across_replicas_and_arrival_orders() {
    let mut origin = board();
    let page = first_page(&origin);
    let (one, two, target, connector) = write(&mut origin, |tx| {
        let one = add(tx, &page, frame(0.0, 0.0, 300.0, 200.0));
        let two = add(tx, &page, frame(10.0, 10.0, 100.0, 100.0));
        let target = add(tx, &page, shape(400.0, 0.0, 100.0, 100.0));
        let connector = add(tx, &page, connector_from(&target, [0.0, 0.5], [800.0, 50.0]));
        Ok((one, two, target, connector))
    });
    let mut a = fork(&origin);
    let mut b = fork(&origin);
    write(&mut a, |tx| {
        tx.set(&one, "parent", json!(two.as_str()))?;
        remove(tx, &page, "elements", &target);
        Ok(())
    });
    write(&mut b, |tx| {
        tx.set(&two, "parent", json!(one.as_str()))?;
        tx.set(&connector, "start", json!({ "point": [400, 50], "binding": { "target": target.as_str(), "anchor": [0, 0.5] } }))?;
        Ok(())
    });
    // Each replica sees the other's edits and repairs on its own, before seeing the other repair.
    let (va, vb) = (a.version(), b.version());
    let from_b = b.export_changes(&va).unwrap();
    let from_a = a.export_changes(&vb).unwrap();
    a.import_changes(&from_b).unwrap();
    b.import_changes(&from_a).unwrap();
    assert_eq!(state(&a), state(&b), "both repaired alike before exchanging the repairs");
    assert_converged_and_settled(&mut a, &mut b);
    // A third replica that gets the changes in the other order agrees.
    let mut c = fork(&origin);
    c.import_changes(&from_b).unwrap();
    c.import_changes(&from_a).unwrap();
    assert_converged_and_settled(&mut a, &mut c);
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
    let mut replica = kition_kit::kdm::Replica::load(kition_board_kdm::schema(), &a.snapshot().unwrap(), kition_kit::kdm::ActorId::random()).unwrap();
    assert!(file::repair_once(&mut replica).unwrap(), "the missing point is written");
    let start = End::parse(&replica.get(&connector, "start").unwrap()).unwrap();
    assert_eq!(start.binding.map(|b| b.target), Some(target));
    assert_eq!(start.point, kition_board_kdm::Point::new(200.0, 125.0), "the point derived from the target's right edge");
}
