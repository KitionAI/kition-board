//! Validation and repair (design note 1.5, requirement M4): rules V1–V9, run after open, import
//! of changes, undo and redo, inside the session's `system` transaction labelled `repair`. A
//! healthy board writes nothing.
//!
//! Every rule depends only on the state, never on arrival order or actor, and every choice between
//! equals goes by id order, so two replicas that see the same state write the same repair (M4).
//!
//! | Rule | State | Repair |
//! | --- | --- | --- |
//! | V1 | a list entry (`pages`, `elements`) naming no live object of its type | drop the entry |
//! | V2 | a live element on no live page's list | delete it and its descendants |
//! | V3 | the same page or element in two places | keep the first entry of a list, and an element in the list of the page whose id sorts first |
//! | V4 | `parent` naming a deleted element, one on another page, or a kind that cannot contain | clear `parent` |
//! | V5 | a `parent` cycle | clear `parent` on the cycle's element whose id sorts first |
//! | V6 | a binding naming a deleted element, one on another page, or a kind that cannot be bound | drop the binding; the point becomes the last derived one when the target still has a box |
//! | V7 | an anchor outside [0, 1] | clamp it |
//! | V8 | two live elements of a page with one `key` | clear the key of every one but the element whose id sorts first |
//! | V9 | a `group` without children | delete it |
//!
//! A live `Page` object no list names is a deleted page (its elements go by V2).
//!
//! Ported from the four repairs Kition's convergence suite found in `packages/board-editor`
//! (`repairBoardPlacements` in `boardKdmWrite.ts`, the orphan and parent repairs in
//! `boardStore.ts`, `repairBoardBindingRecords` in `boardBindingEngine.ts`).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use kition_kit::kdm::{ObjectId, Tx};
use kition_kit::session::Result;
use serde_json::Value;

use crate::element::{End, Kind};
use crate::geometry::{Point, Rect, Size, clamp_anchor};
use crate::outline::{outline_point, outline_shape_in};
use crate::view::MAX_DEPTH;
use crate::{ELEMENT, PAGE, root_id};

fn live(tx: &Tx<'_>, id: &ObjectId, ty: &str) -> bool {
    tx.exists(id) && tx.object_type(id).is_ok_and(|found| found == ty)
}

fn kind_of(tx: &Tx<'_>, id: &ObjectId) -> Option<Kind> {
    tx.get(id, "kind").ok()?.as_str().and_then(Kind::parse)
}

fn parent_of(tx: &Tx<'_>, id: &ObjectId) -> Option<ObjectId> {
    tx.get(id, "parent").ok()?.as_str().map(ObjectId::from_string)
}

/// Removes the entries at `indices` (ascending) from a list, last first.
fn remove_entries(tx: &mut Tx<'_>, owner: &ObjectId, field: &str, indices: &[usize]) -> Result<()> {
    for index in indices.iter().rev() {
        tx.list_remove(owner, field, *index)?;
    }
    Ok(())
}

/// Runs V1–V9 on the transaction's state; answers whether it wrote anything.
pub fn repair(tx: &mut Tx<'_>) -> Result<bool> {
    let root = root_id();
    if !live(tx, &root, crate::BOARD) {
        return Ok(false);
    }
    let mut wrote = false;

    // V1, V3: the root's page list.
    let mut pages: Vec<ObjectId> = Vec::new();
    let mut drop = Vec::new();
    for (index, page) in tx.list_items(&root, "pages")?.into_iter().enumerate() {
        if !live(tx, &page, PAGE) || pages.contains(&page) {
            drop.push(index);
        } else {
            pages.push(page);
        }
    }
    wrote |= !drop.is_empty();
    remove_entries(tx, &root, "pages", &drop)?;
    // A live page no list names is a deleted page.
    let listed: BTreeSet<ObjectId> = pages.iter().cloned().collect();
    let mut unlisted: Vec<ObjectId> = tx.objects_of_type(PAGE).into_iter().filter(|page| !listed.contains(page)).collect();
    unlisted.sort();
    for page in unlisted {
        tx.delete(&page)?;
        wrote = true;
    }

    // V1, V3: the pages' element lists, pages in id order so an element listed twice stays on the
    // page whose id sorts first.
    let mut by_id = pages.clone();
    by_id.sort();
    let mut placed: HashMap<ObjectId, ObjectId> = HashMap::new();
    for page in &by_id {
        let mut drop = Vec::new();
        for (index, id) in tx.list_items(page, "elements")?.into_iter().enumerate() {
            if !live(tx, &id, ELEMENT) || placed.contains_key(&id) {
                drop.push(index);
            } else {
                placed.insert(id, page.clone());
            }
        }
        wrote |= !drop.is_empty();
        remove_entries(tx, page, "elements", &drop)?;
    }

    // V2: elements on no live page, and everything below them.
    let mut elements: Vec<ObjectId> = tx.objects_of_type(ELEMENT);
    elements.sort();
    let parents: BTreeMap<ObjectId, ObjectId> = elements.iter().filter_map(|id| parent_of(tx, id).map(|parent| (id.clone(), parent))).collect();
    let mut doomed: BTreeSet<ObjectId> = elements.iter().filter(|id| !placed.contains_key(*id)).cloned().collect();
    if !doomed.is_empty() {
        // Descendants: an element whose parent chain reaches a doomed element.
        for id in &elements {
            let mut at = parents.get(id);
            for _ in 0..MAX_DEPTH {
                let Some(parent) = at else { break };
                if doomed.contains(parent) {
                    doomed.insert(id.clone());
                    break;
                }
                at = parents.get(parent);
            }
        }
        for page in &pages {
            let drop: Vec<usize> =
                tx.list_items(page, "elements")?.iter().enumerate().filter(|(_, id)| doomed.contains(*id)).map(|(index, _)| index).collect();
            remove_entries(tx, page, "elements", &drop)?;
        }
        for id in &doomed {
            tx.delete(id)?;
            placed.remove(id);
        }
        wrote = true;
    }
    let mut live_elements: Vec<ObjectId> = placed.keys().cloned().collect();
    live_elements.sort();
    let kinds: HashMap<ObjectId, Kind> = live_elements.iter().filter_map(|id| kind_of(tx, id).map(|kind| (id.clone(), kind))).collect();

    // V4: parents that do not exist, are on another page, or cannot contain.
    for id in &live_elements {
        let Some(parent) = parent_of(tx, id) else { continue };
        let valid = placed.get(&parent).is_some_and(|page| Some(page) == placed.get(id)) && kinds.get(&parent).is_some_and(|kind| kind.can_contain());
        if !valid {
            tx.set(id, "parent", Value::Null)?;
            wrote = true;
        }
    }

    // V5: parent cycles; each cycle loses the parent of its element whose id sorts first.
    let mut parents: BTreeMap<ObjectId, ObjectId> =
        live_elements.iter().filter_map(|id| parent_of(tx, id).map(|parent| (id.clone(), parent))).collect();
    for start in &live_elements {
        let mut path: Vec<ObjectId> = Vec::new();
        let mut at = Some(start.clone());
        while let Some(current) = at {
            if let Some(position) = path.iter().position(|seen| *seen == current) {
                if let Some(first) = path.get(position..).and_then(|cycle| cycle.iter().min()).cloned() {
                    tx.set(&first, "parent", Value::Null)?;
                    parents.remove(&first);
                    wrote = true;
                }
                break;
            }
            if path.len() > live_elements.len() {
                break;
            }
            path.push(current.clone());
            at = parents.get(&current).cloned();
        }
    }

    // V6, V7: connector bindings.
    for id in &live_elements {
        if kinds.get(id) != Some(&Kind::Connector) {
            continue;
        }
        for terminal in ["start", "end"] {
            let stored = tx.get(id, terminal)?;
            let Some(end) = End::parse(&stored) else { continue };
            let Some(binding) = &end.binding else { continue };
            let target = &binding.target;
            let bindable = target != id
                && placed.get(target).is_some_and(|page| Some(page) == placed.get(id))
                && kinds.get(target).is_some_and(|kind| kind.bindable());
            let repaired = if !bindable {
                let point = if placed.contains_key(target) { target_point(tx, target, binding.anchor).unwrap_or(end.point) } else { end.point };
                Some(End::free(point))
            } else if stored.get("point").is_none() {
                // V7: a bound end written without its cached point gets the one its target gives.
                let mut cached = end.clone();
                cached.point = target_point(tx, target, binding.anchor).unwrap_or(end.point);
                if let Some(binding) = cached.binding.as_mut() {
                    binding.anchor = clamp_anchor(binding.anchor);
                }
                Some(cached)
            } else if clamp_anchor(binding.anchor) != binding.anchor {
                let mut clamped = end.clone();
                if let Some(binding) = clamped.binding.as_mut() {
                    binding.anchor = clamp_anchor(binding.anchor);
                }
                Some(clamped)
            } else {
                None
            };
            if let Some(repaired) = repaired {
                tx.set(id, terminal, repaired.to_json())?;
                wrote = true;
            }
        }
    }

    // V8: one key per page.
    let mut keys: BTreeMap<(ObjectId, String), Vec<ObjectId>> = BTreeMap::new();
    for id in &live_elements {
        if let (Some(key), Some(page)) = (tx.get(id, "key")?.as_str(), placed.get(id)) {
            keys.entry((page.clone(), key.to_owned())).or_default().push(id.clone());
        }
    }
    for holders in keys.values() {
        for later in holders.iter().skip(1) {
            tx.set(later, "key", Value::Null)?;
            wrote = true;
        }
    }

    // V9: empty groups (deleting one can empty its own group).
    loop {
        let mut alive: Vec<ObjectId> = placed.keys().cloned().collect();
        alive.sort();
        let empty: Vec<ObjectId> = alive
            .iter()
            .filter(|id| kinds.get(*id) == Some(&Kind::Group))
            .filter(|group| !alive.iter().any(|other| parent_of(tx, other).as_ref() == Some(*group)))
            .cloned()
            .collect();
        if empty.is_empty() {
            break;
        }
        for group in &empty {
            if let Some(page) = placed.remove(group) {
                let drop: Vec<usize> =
                    tx.list_items(&page, "elements")?.iter().enumerate().filter(|(_, id)| *id == group).map(|(index, _)| index).collect();
                remove_entries(tx, &page, "elements", &drop)?;
            }
            tx.delete(group)?;
        }
        wrote = true;
    }
    Ok(wrote)
}

/// Where `anchor` is on `target` as stored now, when the target has a box: on its outline (design
/// note 1.4).
fn target_point(tx: &Tx<'_>, target: &ObjectId, anchor: [f64; 2]) -> Option<Point> {
    let position = Point::parse_object(&tx.get(target, "position").ok()?)?;
    let size = Size::parse(&tx.get(target, "size").ok()?)?;
    let rotation = tx.get(target, "rotation").ok().and_then(|value| value.as_f64()).unwrap_or(0.0);
    let rect = Rect::new(position.x, position.y, position.x + size.w, position.y + size.h);
    Some(outline_point(outline_shape_in(tx, target).as_deref(), rect, rotation, anchor))
}
