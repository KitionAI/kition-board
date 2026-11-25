//! Validation and repair (design note 1.5): one case per rule V1–V9, each a broken board written
//! without the session's repair (as a file written elsewhere holds it), plus determinism (M4): the
//! same broken state repairs alike whatever order its edits came in, and a healthy board writes
//! nothing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod common;

use common::*;
use kition_board_kdm::{End, file, root_id};
use serde_json::{Value, json};

/// `session`'s board repaired once, as opening it repairs; a second repair writes nothing more.
fn repaired_and_settled(session: &Session) -> Session {
    let mut replica =
        kition_kit::kdm::Replica::load(kition_board_kdm::schema(), &session.snapshot().unwrap(), kition_kit::kdm::ActorId::random()).unwrap();
    assert!(file::repair_once(&mut replica).unwrap(), "the broken board is repaired");
    assert!(!file::repair_once(&mut replica).unwrap(), "the repaired board needs no more repair");
    kition_kit::session::KitSession::from_replica(replica, Box::new(kition_board_kdm::blobs::SharedBlobs::default())).unwrap()
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

/// M4: the same broken state repairs to one state, whichever order its edits were written in.
#[test]
fn repairs_are_deterministic_whatever_order_the_edits_came_in() {
    let mut origin = board();
    let page = first_page(&origin);
    let (one, two, target, connector) = write(&mut origin, |tx| {
        let one = add(tx, &page, frame(0.0, 0.0, 300.0, 200.0));
        let two = add(tx, &page, frame(10.0, 10.0, 100.0, 100.0));
        let target = add(tx, &page, shape(400.0, 0.0, 100.0, 100.0));
        let connector = add(tx, &page, connector_from(&target, [0.0, 0.5], [800.0, 50.0]));
        Ok((one, two, target, connector))
    });
    let first = |session: &mut Session| {
        write(session, |tx| {
            tx.set(&one, "parent", json!(two.as_str()))?;
            remove(tx, &page, "elements", &target);
            Ok(())
        });
    };
    let second = |session: &mut Session| {
        write(session, |tx| {
            tx.set(&two, "parent", json!(one.as_str()))?;
            tx.set(&connector, "start", json!({ "point": [400, 50], "binding": { "target": target.as_str(), "anchor": [0, 0.5] } }))?;
            Ok(())
        });
    };
    let mut a = fork(&origin);
    first(&mut a);
    second(&mut a);
    let mut b = fork(&origin);
    second(&mut b);
    first(&mut b);
    let (a, b) = (repaired_and_settled(&a), repaired_and_settled(&b));
    assert_eq!(state(&a), state(&b), "both repaired alike");
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
