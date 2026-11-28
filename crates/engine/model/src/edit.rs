//! Building blocks the commands share: list placement, selection roots and subtrees, geometry as
//! the transaction holds it, translation, deletion, and keeping bound connector ends current
//! (design note 1.4: commands write the freshly derived point, so raw readers see a near value).
//!
//! Ported from Kition's `packages/board-editor/src/lib/boardHierarchy.ts`
//! (`getBoardSelectionRootIds`, `getBoardElementsWithDescendants`), `boardBindingEngine.ts`
//! (`syncBoardConnectorBindings`, `removeBoardConnectorBindings`) and `whiteboardGeometry.ts`
//! (`translateWhiteboardElement`).

use std::collections::{BTreeSet, HashMap, HashSet};

use kition_board_kdm::geometry::text_size;
use kition_board_kdm::outline::{outline_point, outline_shape_in};
use kition_board_kdm::{BoardModel, Element, End, Kind, PageView, Point, Rect, Size};
use kition_kit::kdm::{ObjectId, Tx};
use serde_json::{Value, json};

use crate::params::{Result, invalid, locked};

/// Appends `id` to `page`'s element list (in front of everything).
pub fn append(tx: &mut Tx<'_>, page: &ObjectId, id: &ObjectId) -> Result<()> {
    Appender::new(tx, page)?.push(tx, id)
}

/// Appends many elements to a page's list, reading the list's length once: reading it for each
/// element made a 5,000-element paste quadratic (minutes in the page).
pub struct Appender {
    page: ObjectId,
    at: usize,
}

impl Appender {
    pub fn new(tx: &Tx<'_>, page: &ObjectId) -> Result<Self> {
        Ok(Self { page: page.clone(), at: tx.list_items(page, "elements")?.len() })
    }

    pub fn push(&mut self, tx: &mut Tx<'_>, id: &ObjectId) -> Result<()> {
        tx.list_insert(&self.page, "elements", self.at, id)?;
        self.at += 1;
        Ok(())
    }
}

/// Removes `id` from `page`'s element list wherever it is listed.
pub fn unlist(tx: &mut Tx<'_>, page: &ObjectId, id: &ObjectId) -> Result<()> {
    let items = tx.list_items(page, "elements")?;
    for (index, item) in items.iter().enumerate().rev() {
        if item == id {
            tx.list_remove(page, "elements", index)?;
        }
    }
    Ok(())
}

/// The ids of `ids` that have no ancestor in `ids`.
pub fn roots(model: &BoardModel, ids: &[ObjectId]) -> Vec<ObjectId> {
    let selected: HashSet<&ObjectId> = ids.iter().collect();
    ids.iter()
        .filter(|id| {
            let mut at = model.element(id).and_then(|element| element.parent.clone());
            let mut depth = 0;
            while let Some(parent) = at {
                if selected.contains(&parent) {
                    return false;
                }
                if depth > kition_board_kdm::view::MAX_DEPTH {
                    return true;
                }
                depth += 1;
                at = model.element(&parent).and_then(|element| element.parent.clone());
            }
            true
        })
        .cloned()
        .collect()
}

/// `ids` and every element below them, each once, in the order found.
pub fn subtree(model: &BoardModel, ids: &[ObjectId]) -> Vec<ObjectId> {
    let mut out: Vec<ObjectId> = Vec::new();
    let mut seen = HashSet::new();
    for id in ids {
        for found in model.with_descendants(id) {
            if seen.insert(found.clone()) {
                out.push(found);
            }
        }
    }
    out
}

/// Refuses when any of `ids` is locked.
pub fn check_unlocked(model: &BoardModel, ids: &[ObjectId]) -> Result<()> {
    match ids.iter().find(|id| model.element(id).is_some_and(|element| element.locked)) {
        Some(id) => Err(locked(id)),
        None => Ok(()),
    }
}

/// Elements whose text the transaction wrote, as it wrote them: a text's box is measured from
/// these, not from the view (which shows the board before the command, and has no
/// element the command created). The transaction cannot read a text back.
pub type Fresh = HashMap<ObjectId, Element>;

/// A text element with `text` and `runs` (its marks) as [`Fresh`] holds it: what measuring reads
/// (its text, marks and `textFormat`); its geometry and font come from the transaction.
pub fn text_element(id: &ObjectId, text: String, runs: Vec<kition_kit::kdm::TextRun>, text_format: Option<Value>) -> Element {
    let runs = if runs.iter().any(|run| !run.attributes.is_empty()) { runs } else { Vec::new() };
    Element {
        id: id.clone(),
        kind: Kind::Text,
        position: None,
        size: None,
        rotation: 0.0,
        locked: false,
        parent: None,
        key: None,
        text,
        runs,
        points: Vec::new(),
        start: None,
        end: None,
        waypoints: Vec::new(),
        image: None,
        card: None,
        source_ref_ids: Vec::new(),
        registers: text_format.into_iter().map(|format| ("textFormat".to_owned(), format)).collect(),
    }
}

/// Each character's marks in `element`'s text.
pub fn char_marks(element: &Element) -> Vec<std::collections::BTreeMap<String, Value>> {
    if element.runs.is_empty() {
        return vec![std::collections::BTreeMap::new(); element.text.chars().count()];
    }
    element.runs.iter().flat_map(|run| std::iter::repeat_n(run.attributes.clone(), run.insert.chars().count())).collect()
}

/// `text` as runs of characters with equal marks.
pub fn runs_of(text: &str, marks: &[std::collections::BTreeMap<String, Value>]) -> Vec<kition_kit::kdm::TextRun> {
    let mut runs: Vec<kition_kit::kdm::TextRun> = Vec::new();
    let empty = std::collections::BTreeMap::new();
    for (index, ch) in text.chars().enumerate() {
        let attributes = marks.get(index).unwrap_or(&empty);
        match runs.last_mut() {
            Some(run) if &run.attributes == attributes => run.insert.push(ch),
            _ => runs.push(kition_kit::kdm::TextRun { insert: ch.to_string(), attributes: attributes.clone() }),
        }
    }
    runs
}

/// `element` (a text) as [`Fresh`] holds it after its text became `text`: the characters kept at
/// either end keep their marks, the new ones have none (a near value: the stored point is a
/// cache, which readers derive again).
pub fn retexted(element: &Element, text: &str) -> Element {
    let old: Vec<char> = element.text.chars().collect();
    let new: Vec<char> = text.chars().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old.iter().rev().zip(new.iter().rev()).take_while(|(a, b)| a == b).count().min(old.len() - prefix).min(new.len() - prefix);
    let marks = char_marks(element);
    let kept = |range: std::ops::Range<usize>| marks.get(range).map(<[_]>::to_vec).unwrap_or_default();
    let mut fresh = kept(0..prefix);
    fresh.resize(new.len() - suffix, std::collections::BTreeMap::new());
    fresh.extend(kept(old.len() - suffix..old.len()));
    text_element(&element.id, text.to_owned(), runs_of(text, &fresh), element.register("textFormat").cloned())
}

/// The unrotated box and rotation of a boxed element as the transaction holds them.
pub fn geometry(model: &BoardModel, tx: &Tx<'_>, id: &ObjectId) -> Option<(Rect, f64)> {
    geometry_in(model, tx, id, &Fresh::new())
}

/// [`geometry`], a text measured from `fresh` where it holds the element.
pub fn geometry_in(model: &BoardModel, tx: &Tx<'_>, id: &ObjectId, fresh: &Fresh) -> Option<(Rect, f64)> {
    let kind = tx.get(id, "kind").ok()?.as_str().and_then(Kind::parse)?;
    if !kind.boxed() {
        return None;
    }
    let position = Point::parse_object(&tx.get(id, "position").ok()?)?;
    let stored = Size::parse(&tx.get(id, "size").ok()?);
    let size = if kind == Kind::Text {
        let element = fresh.get(id).or_else(|| model.element(id));
        let font = tx.get(id, "fontSize").ok().and_then(|value| value.as_f64());
        let family = tx.get(id, "fontFamily").ok();
        let family = family.as_ref().and_then(|value| value.as_str());
        let styled = element.and_then(Element::styled_text);
        text_size(element.map_or("", |element| element.text.as_str()), styled.as_deref(), stored, font, family)
    } else {
        stored.unwrap_or_default()
    };
    let rotation = tx.get(id, "rotation").ok().and_then(|value| value.as_f64()).filter(|r| r.is_finite()).unwrap_or(0.0);
    Some((Rect::new(position.x, position.y, position.x + size.w, position.y + size.h), rotation))
}

/// Where `end` is now in the transaction: derived from its bindable target on `page` (on the
/// target's outline, design note 1.4), else its stored point.
pub fn end_point(model: &BoardModel, tx: &Tx<'_>, page: &ObjectId, end: &End) -> Point {
    end_point_in(model, tx, page, end, &Fresh::new())
}

/// [`end_point`], a text target measured from `fresh` where it holds it.
pub fn end_point_in(model: &BoardModel, tx: &Tx<'_>, page: &ObjectId, end: &End, fresh: &Fresh) -> Point {
    if let Some(binding) = &end.binding
        && on_page(model, tx, page, &binding.target)
        && let Some((rect, rotation)) = geometry_in(model, tx, &binding.target, fresh)
    {
        return outline_point(outline_shape_in(tx, &binding.target).as_deref(), rect, rotation, binding.anchor);
    }
    end.point
}

/// Whether `id` is a live element on `page` in the transaction (shown there, or created in it).
pub fn on_page(model: &BoardModel, tx: &Tx<'_>, page: &ObjectId, id: &ObjectId) -> bool {
    if !tx.exists(id) {
        return false;
    }
    match model.page_of(id) {
        Some(found) => &found.id == page,
        None => tx.list_items(page, "elements").is_ok_and(|items| items.contains(id)),
    }
}

/// Rewrites the stored points of `page`'s connector ends bound to any of `changed`, so they match
/// the targets' new geometry.
pub fn refresh_bound_ends(model: &BoardModel, tx: &mut Tx<'_>, page: &PageView, changed: &BTreeSet<ObjectId>) -> Result<()> {
    refresh_bound_ends_in(model, tx, page, changed, &Fresh::new())
}

/// [`refresh_bound_ends`], text targets measured from `fresh` where it holds them.
pub fn refresh_bound_ends_in(model: &BoardModel, tx: &mut Tx<'_>, page: &PageView, changed: &BTreeSet<ObjectId>, fresh: &Fresh) -> Result<()> {
    // The view's binding index names the connectors to look at (not the page's whole list).
    let mut seen = HashSet::new();
    let connectors: Vec<&Element> = changed
        .iter()
        .flat_map(|target| model.connectors_bound_to(target))
        .filter(|connector| model.page_of(&connector.id).is_some_and(|on| on.id == page.id) && seen.insert(connector.id.clone()))
        .collect();
    for connector in connectors {
        if !tx.exists(&connector.id) {
            continue;
        }
        for terminal in ["start", "end"] {
            let Some(end) = End::parse(&tx.get(&connector.id, terminal)?) else { continue };
            let Some(binding) = &end.binding else { continue };
            if !changed.contains(&binding.target) {
                continue;
            }
            let point = end_point_in(model, tx, &page.id, &end, fresh);
            if point != end.point {
                tx.set(&connector.id, terminal, End { point, binding: end.binding.clone() }.to_json())?;
            }
        }
    }
    Ok(())
}

/// Moves one element by (`dx`, `dy`): its box, its points, or a connector's free ends and bends.
/// A group has no geometry of its own (its children move themselves).
pub fn translate(tx: &mut Tx<'_>, element: &Element, dx: f64, dy: f64) -> Result<()> {
    match element.kind {
        Kind::Group => {}
        Kind::Stroke => {
            let points: Vec<Point> = element.points.iter().map(|point| point.offset(dx, dy)).collect();
            tx.set(&element.id, "points", Point::list_json(&points))?;
        }
        Kind::Connector => {
            for (terminal, end) in [("start", &element.start), ("end", &element.end)] {
                if let Some(end) = end
                    && end.binding.is_none()
                {
                    tx.set(&element.id, terminal, End::free(end.point.offset(dx, dy)).to_json())?;
                }
            }
            if !element.waypoints.is_empty() {
                let waypoints: Vec<Point> = element.waypoints.iter().map(|point| point.offset(dx, dy)).collect();
                tx.set(&element.id, "waypoints", Point::list_json(&waypoints))?;
            }
        }
        _ => {
            if let Some(position) = element.position {
                tx.set(&element.id, "position", position.offset(dx, dy).to_object())?;
            }
        }
    }
    Ok(())
}

/// Moves `ids` (and everything below them) by (`dx`, `dy`) and keeps bound ends current. Answers
/// the ids moved.
pub fn move_by(model: &BoardModel, tx: &mut Tx<'_>, page: &PageView, ids: &[ObjectId], dx: f64, dy: f64) -> Result<Vec<ObjectId>> {
    let moved = subtree(model, &roots(model, ids));
    check_unlocked(model, &moved)?;
    if dx == 0.0 && dy == 0.0 {
        return Ok(moved);
    }
    for id in &moved {
        if let Some(element) = model.element(id) {
            translate(tx, element, dx, dy)?;
        }
    }
    refresh_bound_ends(model, tx, page, &moved.iter().cloned().collect())?;
    Ok(moved)
}

/// Deletes `ids` and everything below them; connectors bound to a deleted element keep their last
/// derived point (rule V6 at the source). Answers the ids deleted.
pub fn delete(model: &BoardModel, tx: &mut Tx<'_>, page: &PageView, ids: &[ObjectId]) -> Result<Vec<ObjectId>> {
    let doomed = subtree(model, ids);
    // What is inside a deleted container goes with it: a locked member refuses it too.
    check_unlocked(model, &doomed)?;
    let gone: BTreeSet<ObjectId> = doomed.iter().cloned().collect();
    let mut seen = HashSet::new();
    let bound: Vec<&Element> = doomed
        .iter()
        .flat_map(|target| model.connectors_bound_to(target))
        .filter(|connector| !gone.contains(&connector.id) && seen.insert(connector.id.clone()))
        .collect();
    for connector in bound {
        for (terminal, end) in [("start", &connector.start), ("end", &connector.end)] {
            if let Some(end) = end
                && end.binding.as_ref().is_some_and(|binding| gone.contains(&binding.target))
            {
                let point = model.end_point(&connector.id, end);
                tx.set(&connector.id, terminal, End::free(point).to_json())?;
            }
        }
    }
    let items = tx.list_items(&page.id, "elements")?;
    for (index, item) in items.iter().enumerate().rev() {
        if gone.contains(item) {
            tx.list_remove(&page.id, "elements", index)?;
        }
    }
    for id in &doomed {
        if tx.exists(id) {
            tx.delete(id)?;
        }
    }
    Ok(doomed)
}

/// Whether `parent` may contain `child` on `page`: a frame or group there, not `child` itself or
/// one of its descendants (no cycle).
pub fn check_parent(model: &BoardModel, page: &PageView, child: Option<&ObjectId>, parent: &ObjectId) -> Result<()> {
    let element = model.element(parent).ok_or_else(|| invalid(format!("no parent element {}", parent.as_str())))?;
    if !element.kind.can_contain() {
        return Err(invalid(format!("a {} cannot contain elements; only frames and groups can", element.kind.as_str())));
    }
    if model.page_of(parent).map(|found| &found.id) != Some(&page.id) {
        return Err(invalid("the parent is on another page"));
    }
    if let Some(child) = child
        && model.with_descendants(child).contains(parent)
    {
        return Err(invalid("an element cannot be inside itself"));
    }
    Ok(())
}

/// Refuses a key another live element of `page` already has (`except` may keep its own).
pub fn check_key_free(model: &BoardModel, page: &PageView, key: &str, except: Option<&ObjectId>) -> Result<()> {
    match model.by_key(page, key) {
        Some(holder) if Some(&holder.id) != except => Err(invalid(format!("key `{key}` is already used on this page"))),
        _ => Ok(()),
    }
}

/// The JSON a command answers for created ids.
pub fn created(ids: &[ObjectId]) -> Value {
    json!({ "created": ids.iter().map(ObjectId::as_str).collect::<Vec<_>>() })
}

/// Ids as a JSON list.
pub fn id_list(ids: &[ObjectId]) -> Value {
    Value::Array(ids.iter().map(|id| json!(id.as_str())).collect())
}
