//! Copies of elements: the clipboard payload (`clipboard.copy`, `element.paste`), and the same
//! machinery for `element.duplicate` and `page.duplicate`.
//!
//! The payload (`application/x-kition-board+json`, version 1) lists elements with a `ref` (their
//! id where they were copied) and their fields; `parent` and binding targets name refs. Pasting
//! makes new ids, keeps references inside the payload, keeps a binding to an element outside it
//! when that element is on the destination page, and frees the end otherwise.
//!
//! Ported from Kition's `packages/board-editor/src/lib/boardClipboard.ts` and `boardHierarchy.ts`
//! (`cloneBoardElementTrees`).

use std::collections::{BTreeSet, HashMap, HashSet};

use kition_board_kdm::geometry::clamp_anchor;
use kition_board_kdm::{BoardModel, ELEMENT, Element, End, Kind, PageView, Point};
use kition_kit::kdm::{ObjectId, Tx};
use serde_json::{Map, Value, json};

use crate::Ctx;
use crate::edit::{self, created, roots, subtree};
use crate::fields;
use crate::params::{self, MAX_OBJECTS, Result, invalid};

/// The payload's MIME type, as the editor puts it on the system clipboard.
pub const PAYLOAD_TYPE: &str = "application/x-kition-board+json";
/// The payload version.
pub const PAYLOAD_VERSION: u64 = 1;

/// One element as the payload holds it.
pub fn element_json(element: &Element) -> Value {
    let mut out = Map::new();
    out.insert("ref".into(), json!(element.id.as_str()));
    out.insert("kind".into(), json!(element.kind.as_str()));
    if let Some(position) = element.position {
        out.insert("position".into(), position.to_object());
    }
    if let Some(size) = element.size {
        out.insert("size".into(), size.to_json());
    }
    if element.rotation != 0.0 {
        out.insert("rotation".into(), json!(element.rotation));
    }
    if let Some(parent) = &element.parent {
        out.insert("parent".into(), json!(parent.as_str()));
    }
    if let Some(key) = &element.key {
        out.insert("key".into(), json!(key));
    }
    if !element.text.is_empty() {
        out.insert("text".into(), json!(element.text));
    }
    if !element.runs.is_empty() {
        out.insert("runs".into(), Value::Array(element.runs.iter().map(|run| run.to_json()).collect()));
    }
    if !element.points.is_empty() {
        out.insert("points".into(), Point::list_json(&element.points));
    }
    for (terminal, end) in [("start", &element.start), ("end", &element.end)] {
        if let Some(end) = end {
            out.insert(terminal.into(), end.to_json());
        }
    }
    if !element.waypoints.is_empty() {
        out.insert("waypoints".into(), Point::list_json(&element.waypoints));
    }
    if let Some(image) = &element.image {
        out.insert("image".into(), json!({ "hash": image.hash, "mime": image.mime, "size": image.size }));
    }
    if let Some(card) = &element.card {
        out.insert("card".into(), card.clone());
    }
    if !element.source_ref_ids.is_empty() {
        out.insert("sourceRefIds".into(), json!(element.source_ref_ids));
    }
    for (field, value) in &element.registers {
        out.insert(field.clone(), value.clone());
    }
    Value::Object(out)
}

/// The payload of `ids` and everything inside them, in page order.
pub fn payload(model: &BoardModel, ids: &[ObjectId]) -> Result<Value> {
    let (_, page) = params::elements_on_one_page(model, ids)?;
    let wanted: BTreeSet<ObjectId> = subtree(model, &roots(model, ids)).into_iter().collect();
    let elements: Vec<Value> = model.elements_of(page).filter(|element| wanted.contains(&element.id)).map(element_json).collect();
    Ok(json!({ "type": PAYLOAD_TYPE, "version": PAYLOAD_VERSION, "elements": elements }))
}

/// How instantiated copies treat authoring keys.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Keys {
    /// Dropped (copies on the same page would clash).
    Drop,
    /// Kept (a new page).
    Keep,
}

/// Creates copies of payload `items` at the front of `page`, moved by (`dx`, `dy`). Answers the new
/// ids in payload order and the map from refs to them.
pub fn instantiate(
    model: &BoardModel,
    tx: &mut Tx<'_>,
    page: &PageView,
    items: &[Value],
    dx: f64,
    dy: f64,
    keys: Keys,
) -> Result<(Vec<ObjectId>, HashMap<String, ObjectId>)> {
    if items.len() > MAX_OBJECTS {
        return Err(invalid(format!("board.too-many-objects: the payload has more than {MAX_OBJECTS} elements")));
    }
    let mut map: HashMap<String, ObjectId> = HashMap::new();
    let mut made: Vec<(ObjectId, &Map<String, Value>, Kind)> = Vec::with_capacity(items.len());
    let mut list = edit::Appender::new(tx, &page.id)?;
    for item in items {
        let item = item.as_object().ok_or_else(|| invalid("payload elements must be objects"))?;
        let kind = item.get("kind").and_then(Value::as_str).and_then(Kind::parse).ok_or_else(|| invalid("a payload element has no valid `kind`"))?;
        let reference = item.get("ref").and_then(Value::as_str).ok_or_else(|| invalid("a payload element has no `ref`"))?;
        if map.contains_key(reference) {
            return Err(invalid(format!("the payload lists `{reference}` twice")));
        }
        let mut fields = Map::new();
        fields.insert("kind".into(), json!(kind.as_str()));
        for (field, value) in item {
            match field.as_str() {
                "ref" | "kind" | "text" | "runs" | "parent" | "start" | "end" | "locked" => {}
                "key" if keys == Keys::Drop => {}
                "position" => {
                    let point = params::point(value, "position")?;
                    fields.insert(field.clone(), point.offset(dx, dy).to_object());
                }
                "points" | "waypoints" => {
                    let points = params::points(value, field, 100_000)?;
                    fields.insert(field.clone(), Point::list_json(&points.iter().map(|p| p.offset(dx, dy)).collect::<Vec<_>>()));
                }
                "image" | "card" if fields::uses(kind, field) => {
                    fields.insert(field.clone(), value.clone());
                }
                other => {
                    let checked = fields::check(kind, other, value)?;
                    if !checked.is_null() {
                        fields.insert(field.clone(), checked);
                    }
                }
            }
        }
        let id = tx.create(ELEMENT, fields)?;
        if let Some(text) = item.get("text").and_then(Value::as_str).filter(|text| !text.is_empty()) {
            tx.text_insert(&id, "text", 0, text)?;
            write_runs(tx, &id, item.get("runs"))?;
        }
        list.push(tx, &id)?;
        map.insert(reference.to_owned(), id.clone());
        made.push((id, item, kind));
    }
    let kinds: HashMap<&ObjectId, Kind> = made.iter().map(|(id, _, kind)| (id, *kind)).collect();
    // References: parents, kept where they name a container of the payload and make no cycle
    // (rules V4 and V5), in payload order.
    let mut parents: HashMap<ObjectId, ObjectId> = HashMap::new();
    for (id, item, _) in &made {
        let Some(parent) = item.get("parent").and_then(Value::as_str).and_then(|parent| map.get(parent)) else { continue };
        if !kinds.get(parent).is_some_and(|kind| kind.can_contain()) {
            continue;
        }
        let mut at = Some(parent);
        let mut seen = HashSet::new();
        while let Some(above) = at.filter(|above| seen.insert(*above)) {
            if above == id {
                break;
            }
            at = parents.get(above);
        }
        if at.is_some() {
            continue;
        }
        parents.insert(id.clone(), parent.clone());
        tx.set(id, "parent", json!(parent.as_str()))?;
    }
    // A text without a stored size is measured from the copy's text (the transaction cannot read
    // it back).
    let fresh: edit::Fresh = made
        .iter()
        .filter(|(_, item, kind)| *kind == Kind::Text && item.get("size").is_none_or(Value::is_null))
        .map(|(id, item, _)| {
            let text = item.get("text").and_then(Value::as_str).unwrap_or_default().to_owned();
            let runs = item.get("runs").cloned().and_then(|runs| serde_json::from_value(runs).ok()).unwrap_or_default();
            (id.clone(), edit::text_element(id, text, runs, item.get("textFormat").cloned()))
        })
        .collect();
    // Bound ends: to a copy, or to an element of the destination page, that can take a binding
    // (rule V6), its anchor in [0, 1] (rule V7); otherwise the end is free.
    for (id, item, kind) in &made {
        if *kind != Kind::Connector {
            continue;
        }
        for terminal in ["start", "end"] {
            let Some(mut end) = item.get(terminal).and_then(End::parse) else { continue };
            end.point = end.point.offset(dx, dy);
            if let Some(binding) = end.binding.as_mut() {
                let bindable = match map.get(binding.target.as_str()) {
                    Some(copy) => {
                        binding.target = copy.clone();
                        kinds.get(copy).is_some_and(|kind| kind.bindable())
                    }
                    None => {
                        model.page_of(&binding.target).map(|found| &found.id) == Some(&page.id)
                            && model.element(&binding.target).is_some_and(|target| target.kind.bindable())
                    }
                };
                binding.anchor = clamp_anchor(binding.anchor);
                if !bindable {
                    end.binding = None;
                }
            }
            let point = edit::end_point_in(model, tx, &page.id, &end, &fresh);
            tx.set(id, terminal, End { point, binding: end.binding }.to_json())?;
        }
    }
    Ok((made.into_iter().map(|(id, _, _)| id).collect(), map))
}

/// Applies a payload's marked runs to the text just written.
fn write_runs(tx: &mut Tx<'_>, id: &ObjectId, runs: Option<&Value>) -> Result<()> {
    let Some(runs) = runs.and_then(Value::as_array) else { return Ok(()) };
    let mut at = 0usize;
    for run in runs {
        let length = run.get("insert").and_then(Value::as_str).map_or(0, |text| text.chars().count());
        if let Some(attributes) = run.get("attributes").and_then(Value::as_object) {
            for (mark, value) in attributes {
                if ["bold", "italic", "link"].contains(&mark.as_str()) && length > 0 {
                    tx.text_mark(id, "text", at, at + length, mark, value.clone())?;
                }
            }
        }
        at += length;
    }
    Ok(())
}

/// `clipboard.copy { ids }`: the payload of the elements and everything inside them.
pub fn copy(model: &BoardModel, params: &Value) -> Result<Value> {
    payload(model, &params::ids(params, "ids")?)
}

/// `element.paste { page?, payload, at? }`: copies of a payload; `at` puts the copies' top-left
/// there, otherwise copies of elements still on the page are offset by 24. A picture whose bytes
/// the board does not store is left out and named in `skipped`; a payload of only such pictures is
/// refused (`not-found`, `board.image-missing-blob`).
pub fn paste(model: &BoardModel, tx: &mut Tx<'_>, ctx: &Ctx<'_>, params: &Value) -> Result<Value> {
    let page = params::page(model, params, ctx.active_page)?;
    let payload = params.get("payload").and_then(Value::as_object).ok_or_else(|| invalid("`payload` must be a board clipboard payload"))?;
    if payload.get("type").and_then(Value::as_str) != Some(PAYLOAD_TYPE) || payload.get("version").and_then(Value::as_u64) != Some(PAYLOAD_VERSION) {
        return Err(invalid(format!("`payload` must be an {PAYLOAD_TYPE} payload, version {PAYLOAD_VERSION}")));
    }
    let items =
        payload.get("elements").and_then(Value::as_array).filter(|items| !items.is_empty()).ok_or_else(|| invalid("the payload has no elements"))?;
    let (dx, dy) = match params::opt_point(params, "at")? {
        Some(at) => {
            let mut corner: Option<Point> = None;
            for item in items {
                // Every point a copy is drawn from: a box's place, a stroke's points, a
                // connector's ends and bends.
                let ends = ["start", "end"].into_iter().filter_map(|terminal| item.get(terminal).and_then(End::parse)).map(|end| end.point);
                let points = item
                    .get("position")
                    .and_then(Point::parse_object)
                    .into_iter()
                    .chain(item.get("points").map(Point::parse_list).unwrap_or_default())
                    .chain(item.get("waypoints").map(Point::parse_list).unwrap_or_default())
                    .chain(ends);
                for point in points {
                    corner = Some(corner.map_or(point, |corner| Point::new(corner.x.min(point.x), corner.y.min(point.y))));
                }
            }
            let corner = corner.unwrap_or(at);
            (at.x - corner.x, at.y - corner.y)
        }
        None => {
            let still_here = items
                .iter()
                .filter_map(|item| item.get("ref").and_then(Value::as_str))
                .any(|reference| model.page_of(&ObjectId::from_string(reference)).map(|found| &found.id) == Some(&page.id));
            if still_here { (24.0, 24.0) } else { (0.0, 0.0) }
        }
    };
    // A picture names its bytes by hash; a payload copied on another board can name bytes this
    // board does not store. Such a picture is left out (a copy would be a broken picture,
    // `board.image-missing-blob`) and named in the answer; the rest lands where it would have.
    let (kept, skipped): (Vec<&Value>, Vec<&Value>) = items.iter().partition(|item| !missing_picture(ctx, item));
    if kept.is_empty() {
        return Err(params::not_found(
            "board.image-missing-blob: the payload's pictures are not stored in this board; insert the picture files instead",
        ));
    }
    let owned: Vec<Value>;
    let kept: &[Value] = if skipped.is_empty() {
        items
    } else {
        owned = kept.into_iter().cloned().collect();
        &owned
    };
    let (ids, _) = instantiate(model, tx, page, kept, dx, dy, Keys::Drop)?;
    let mut answer = created(&ids);
    if !skipped.is_empty() {
        let named: Vec<Value> = skipped
            .iter()
            .map(|item| json!({ "ref": item.get("ref").cloned().unwrap_or(Value::Null), "reason": "board.image-missing-blob" }))
            .collect();
        if let Some(fields) = answer.as_object_mut() {
            fields.insert("skipped".into(), Value::Array(named));
        }
    }
    Ok(answer)
}

/// Whether payload `item` is a picture whose bytes the board does not store.
fn missing_picture(ctx: &Ctx<'_>, item: &Value) -> bool {
    if item.get("kind").and_then(Value::as_str) != Some(Kind::Image.as_str()) {
        return false;
    }
    item.get("image").and_then(|image| image.get("hash")).and_then(Value::as_str).is_none_or(|hash| ctx.blobs.get(hash).is_none())
}

/// `element.duplicate { ids, offset? }`: copies next to the originals (offset `[24, 24]`).
pub fn duplicate(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let ids = params::ids(params, "ids")?;
    let (_, page) = params::elements_on_one_page(model, &ids)?;
    let offset = params::opt_point(params, "offset")?.unwrap_or(Point::new(24.0, 24.0));
    let payload = payload(model, &ids)?;
    let items = payload.get("elements").and_then(Value::as_array).cloned().unwrap_or_default();
    let (made, map) = instantiate(model, tx, page, &items, offset.x, offset.y, Keys::Drop)?;
    let pairs: Map<String, Value> = map.into_iter().map(|(from, to)| (from, json!(to.as_str()))).collect();
    Ok(json!({ "created": edit::id_list(&made), "copies": Value::Object(pairs) }))
}
