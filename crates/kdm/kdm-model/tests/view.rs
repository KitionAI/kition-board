//! The view (`BoardModel`): reading rules, derived bound ends (design note 1.4), boxes, and the
//! patched view equal to a fresh read.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod common;

use common::*;
use kition_board_kdm::{BoardModel, Kind, Point, Rect, file, schema};
use kition_kit::kdm::{ActorId, Replica};
use serde_json::json;

/// Gap 14 of the board evaluation: a connector bound to a shape follows the shape without any
/// write to the connector, on this replica and on one that merged the move.
#[test]
fn bound_ends_are_derived_from_the_target() {
    let mut a = board();
    let page = first_page(&a);
    let (target, connector) = write(&mut a, |tx| {
        let target = add(tx, &page, shape(100.0, 100.0, 100.0, 50.0));
        Ok((target.clone(), add(tx, &page, connector_from(&target, [1.0, 0.5], [600.0, 400.0]))))
    });
    let model = a.model();
    let element = model.element(&connector).unwrap();
    assert_eq!(model.end_point(&connector, element.start.as_ref().unwrap()), Point::new(200.0, 125.0), "the stored [0, 0] is ignored");
    let mut b = fork(&a);
    write(&mut b, |tx| {
        tx.set(&target, "position", json!({ "x": 300, "y": 0 }))?;
        tx.set(&target, "rotation", json!(90))?;
        Ok(())
    });
    sync(&mut a, &mut b);
    for session in [&a, &b] {
        let model = session.model();
        let element = model.element(&connector).unwrap();
        let derived = model.end_point(&connector, element.start.as_ref().unwrap());
        assert!((derived.x - 350.0).abs() < 1e-9 && (derived.y - 75.0).abs() < 1e-9, "{derived:?}");
        assert_eq!(model.route(element).len(), 2);
    }
    let mut replica = Replica::load(schema(), &a.snapshot().unwrap(), ActorId::random()).unwrap();
    assert!(!file::repair_once(&mut replica).unwrap(), "no repair transaction after concurrent moves");
}

/// An end bound to an element on another page, or a deleted one, uses its stored point.
#[test]
fn ends_bound_to_nothing_readable_use_the_stored_point() {
    let mut a = board();
    let page = first_page(&a);
    let connector = write(&mut a, |tx| {
        Ok(add(
            tx,
            &page,
            json!({ "kind": "connector", "start": { "point": [5, 6], "binding": { "target": "no-such-element", "anchor": [0, 0] } }, "end": { "point": [50, 60] } }),
        ))
    });
    let model = a.model();
    let element = model.element(&connector).unwrap();
    assert_eq!(model.route(element), vec![Point::new(5.0, 6.0), Point::new(50.0, 60.0)]);
}

/// The reading rules of `boardKdmRead.ts`: an element listed twice shows once, at its first entry;
/// entries naming nothing are skipped (before any repair).
#[test]
fn duplicates_and_dangling_entries_are_not_shown() {
    let mut a = board();
    let first = first_page(&a);
    let (two, element, gone) = write(&mut a, |tx| {
        let two = add_page(tx, "Two");
        let element = add(tx, &first, shape(0.0, 0.0, 10.0, 10.0));
        let gone = add(tx, &first, shape(0.0, 0.0, 10.0, 10.0));
        Ok((two, element, gone))
    });
    let snapshot = a.snapshot().unwrap();
    let mut x = Replica::load(schema(), &snapshot, ActorId::random()).unwrap();
    let mut y = Replica::load(schema(), &snapshot, ActorId::random()).unwrap();
    x.transact(&person(), |tx| {
        // `gone` moves to the front while the other replica deletes it; `element` is also
        // listed on page two.
        tx.list_move(&first, "elements", 1, 0)?;
        tx.list_insert(&two, "elements", 0, &element)?;
        Ok(())
    })
    .unwrap();
    y.transact(&person(), |tx| {
        tx.list_remove(&first, "elements", 1)?;
        tx.delete(&gone)?;
        Ok(())
    })
    .unwrap();
    // A raw merge, without the session's repair.
    let from_y = y.export_changes(&x.version()).unwrap();
    x.import_changes(&from_y).unwrap();
    let model = BoardModel::read(&x).unwrap();
    assert_eq!(model.element_count(), 1);
    assert_eq!(model.page_of(&element).unwrap().id, first, "shown at its first entry in board order");
    assert!(model.page(&two).unwrap().elements.is_empty());
}

#[test]
fn boxes_cover_rotation_groups_strokes_and_measured_text() {
    let mut a = board();
    let page = first_page(&a);
    let (group, text, stroke) = write(&mut a, |tx| {
        let group = add(tx, &page, json!({ "kind": "group" }));
        let mut one = shape(0.0, 0.0, 100.0, 50.0);
        one["parent"] = json!(group.as_str());
        one["rotation"] = json!(90);
        add(tx, &page, one);
        let mut two = shape(200.0, 200.0, 10.0, 10.0);
        two["parent"] = json!(group.as_str());
        add(tx, &page, two);
        let text = add(tx, &page, json!({ "kind": "text", "position": { "x": 0, "y": 300 }, "fontSize": 20 }));
        tx.text_insert(&text, "text", 0, "Hello\nworld!")?;
        let stroke = add(tx, &page, json!({ "kind": "stroke", "points": [[5, 5], [-5, 40], [30, 10]] }));
        Ok((group, text, stroke))
    });
    let model = a.model();
    let bounds = model.bounds(model.element(&group).unwrap()).unwrap();
    assert!((bounds.x0 - 25.0).abs() < 1e-9 && (bounds.y0 + 25.0).abs() < 1e-9 && bounds.x1 == 210.0 && bounds.y1 == 210.0, "{bounds:?}");
    let text_box = model.bounds(model.element(&text).unwrap()).unwrap();
    assert_eq!(text_box, Rect::new(0.0, 300.0, 6.0 * 20.0 * 0.6, 300.0 + 2.0 * 20.0 * 1.25));
    assert_eq!(model.bounds(model.element(&stroke).unwrap()).unwrap(), Rect::new(-5.0, 5.0, 30.0, 40.0));
    assert_eq!(model.children(&group).len(), 2);
    assert_eq!(model.with_descendants(&group).len(), 3);
    assert!(model.content_bounds(&model.pages()[0]).is_some());
    assert_eq!(model.element(&group).unwrap().kind, Kind::Group);
}

/// The view patched after each change equals a fresh read of the replica (E11).
#[test]
fn the_patched_view_equals_a_fresh_read() {
    let mut a = board();
    let page = first_page(&a);
    let mut b = fork(&a);
    let mut ids = Vec::new();
    for step in 0..30 {
        let session = if step % 3 == 0 { &mut b } else { &mut a };
        let x = f64::from(step) * 10.0;
        let id = write(session, |tx| Ok(add(tx, &page, shape(x, 0.0, 20.0, 20.0))));
        ids.push(id.clone());
        if step % 4 == 1 {
            write(session, |tx| {
                tx.set(&id, "position", json!({ "x": x, "y": 50 }))?;
                tx.list_move(&page, "elements", 0, 1).ok();
                Ok(())
            });
        }
        if step % 5 == 2 {
            session.undo().unwrap();
        }
        if step % 7 == 6 {
            sync(&mut a, &mut b);
        }
        for session in [&a, &b] {
            assert_eq!(session.model(), &BoardModel::read(session.replica()).unwrap(), "step {step}");
        }
    }
    sync(&mut a, &mut b);
    assert_eq!(a.model(), b.model());
}

/// Changes that touch only shown elements patch them in place (B10b), and the patched view, its
/// child index and its binding index still equal a fresh read: moves, a new parent, a rebound
/// connector, and an element deleted while still listed.
#[test]
fn in_place_patches_equal_a_fresh_read() {
    let mut a = board();
    let page = first_page(&a);
    let (frame_id, one, two, connector) = write(&mut a, |tx| {
        let frame_id = add(tx, &page, frame(0.0, 0.0, 400.0, 400.0));
        let one = add(tx, &page, shape(10.0, 10.0, 50.0, 50.0));
        let two = add(tx, &page, shape(200.0, 10.0, 50.0, 50.0));
        let connector = add(tx, &page, connector_from(&one, [1.0, 0.5], [300.0, 300.0]));
        Ok((frame_id, one, two, connector))
    });
    let check = |session: &Session, step: &str| {
        assert_eq!(session.model(), &BoardModel::read(session.replica()).unwrap(), "{step}");
    };
    write(&mut a, |tx| {
        tx.set(&one, "position", json!({ "x": 20, "y": 20 }))?;
        Ok(())
    });
    check(&a, "move");
    assert_eq!(a.model().connectors_bound_to(&one).len(), 1);
    write(&mut a, |tx| {
        tx.set(&one, "parent", json!(frame_id.as_str()))?;
        Ok(())
    });
    check(&a, "parent");
    assert_eq!(a.model().children(&frame_id).iter().map(|element| element.id.clone()).collect::<Vec<_>>(), vec![one.clone()]);
    write(&mut a, |tx| {
        tx.set(&connector, "start", json!({ "point": [0, 0], "binding": { "target": two.as_str(), "anchor": [0, 0.5] } }))?;
        Ok(())
    });
    check(&a, "rebind");
    assert!(a.model().connectors_bound_to(&one).is_empty());
    assert_eq!(a.model().connectors_bound_to(&two).len(), 1);
    write(&mut a, |tx| {
        tx.delete(&two)?;
        Ok(())
    });
    check(&a, "deleted while listed");
    assert!(a.model().element(&two).is_none());
    a.undo().unwrap();
    check(&a, "undo");
    assert_eq!(a.model().connectors_bound_to(&two).len(), 1);
}
