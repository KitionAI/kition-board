//! Element commands (design note 5.1): create, update, delete, move, resize, rotate, style, text,
//! marks, duplicate, reorder, lock, group and ungroup, frames, strokes.
//!
//! Ported from Kition's `packages/board-editor/src/lib/boardCommands.ts`, `boardHierarchy.ts`,
//! `whiteboardGeometry.ts` (`resizeWhiteboardElements`, `rotateWhiteboardElements`) and the agent
//! operations of `whiteboardAgentPatch.ts`.

use std::collections::{BTreeSet, HashSet};

use kition_board_kdm::{BoardModel, ELEMENT, Element, End, Kind, PageView, Point, Rect, Size};
use kition_kit::kdm::{ObjectId, Tx};
use kition_kit::session::write_text;
use serde_json::{Map, Value, json};

use crate::Ctx;
use crate::edit::{self, check_key_free, check_parent, check_unlocked, created, id_list, roots, subtree};
use crate::fields::{self, STYLE_FIELDS, UPDATABLE};
use crate::params::{self, MAX_OBJECTS, Result, invalid, not_found};

/// The kinds `element.add` makes; the others have their own commands.
const CREATABLE: &[Kind] = &[Kind::Shape, Kind::Text, Kind::Frame, Kind::Stroke];

/// Checks one element description of `element.add` into its stored fields and its text.
fn describe(model: &BoardModel, page: &PageView, spec: &Value, keys: &mut BTreeSet<String>) -> Result<(Map<String, Value>, String)> {
    let spec = spec.as_object().ok_or_else(|| invalid("each element must be an object"))?;
    let kind_name = spec.get("kind").and_then(Value::as_str).ok_or_else(|| invalid("each element needs a `kind`"))?;
    let kind = Kind::parse(kind_name).ok_or_else(|| invalid(format!("unknown kind `{kind_name}`")))?;
    if !CREATABLE.contains(&kind) {
        let hint = match kind {
            Kind::Connector => "connector.add",
            Kind::Image => "image.insert",
            Kind::Card => "card.add",
            _ => "arrange.group",
        };
        return Err(invalid(format!("make a {kind_name} with {hint}")));
    }
    let mut fields = Map::new();
    fields.insert("kind".into(), json!(kind.as_str()));
    let mut text = String::new();
    for (name, value) in spec {
        match name.as_str() {
            "kind" => {}
            "text" => text = value.as_str().ok_or_else(|| invalid("`text` must be text"))?.to_owned(),
            "parent" => {
                let parent = value.as_str().ok_or_else(|| invalid("`parent` must be an element id"))?;
                check_parent(model, page, None, &ObjectId::from_string(parent))?;
                fields.insert(name.clone(), json!(parent));
            }
            "key" => {
                let key = fields::check(kind, "key", value)?;
                if let Some(key) = key.as_str() {
                    check_key_free(model, page, key, None)?;
                    if !keys.insert(key.to_owned()) {
                        return Err(invalid(format!("key `{key}` is given twice")));
                    }
                }
                fields.insert(name.clone(), key);
            }
            "locked" => {
                fields.insert(name.clone(), fields::check(kind, "locked", value)?);
            }
            other if fields::uses(kind, other) && other != "start" && other != "end" && other != "card" && other != "image" => {
                let checked = fields::check(kind, other, value)?;
                if !checked.is_null() {
                    fields.insert(name.clone(), checked);
                }
            }
            other => return Err(invalid(format!("a {kind_name} element has no `{other}`"))),
        }
    }
    if kind.boxed() {
        if !fields.contains_key("position") {
            return Err(invalid(format!("a {kind_name} needs a `position`")));
        }
        if let (false, Some(size)) = (fields.contains_key("size"), fields::default_size(kind)) {
            fields.insert("size".into(), size.to_json());
        }
    }
    if kind == Kind::Shape && !fields.contains_key("shapeType") {
        fields.insert("shapeType".into(), json!("rectangle"));
    }
    if kind == Kind::Stroke && fields.get("points").and_then(Value::as_array).is_none_or(|points| points.len() < 2) {
        return Err(invalid("a stroke needs at least two `points`"));
    }
    Ok((fields, text))
}

/// Creates one element with `fields` and `text` at the front of the page `list` appends to.
pub fn create_one(tx: &mut Tx<'_>, list: &mut edit::Appender, fields: Map<String, Value>, text: &str) -> Result<ObjectId> {
    let id = tx.create(ELEMENT, fields)?;
    if !text.is_empty() {
        tx.text_insert(&id, "text", 0, text)?;
    }
    list.push(tx, &id)?;
    Ok(id)
}

/// `element.add { page?, elements: [{ kind, position?, size?, text?, style fields…, key? }] }`.
pub fn create(model: &BoardModel, tx: &mut Tx<'_>, ctx: &Ctx<'_>, params: &Value) -> Result<Value> {
    let page = params::page(model, params, ctx.active_page)?;
    let specs = params.get("elements").and_then(Value::as_array).ok_or_else(|| invalid("`elements` must be a list"))?;
    if specs.is_empty() || specs.len() > MAX_OBJECTS {
        return Err(invalid(format!("`elements` must hold 1 to {MAX_OBJECTS} elements")));
    }
    let mut keys = BTreeSet::new();
    let described = specs.iter().map(|spec| describe(model, page, spec, &mut keys)).collect::<Result<Vec<_>>>()?;
    let mut ids = Vec::with_capacity(described.len());
    let mut list = edit::Appender::new(tx, &page.id)?;
    for (fields, text) in described {
        ids.push(create_one(tx, &mut list, fields, &text)?);
    }
    Ok(created(&ids))
}

/// `element.set { id, set: { field: value } }`: registers of the element's kind; `null`
/// clears an optional one.
pub fn update(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let id = params::id(params, "id")?;
    let element = params::element(model, &id)?;
    let page = model.page_of(&id).ok_or_else(|| not_found(format!("no element {}", id.as_str())))?;
    let set = params::object(params, "set")?.filter(|set| !set.is_empty()).ok_or_else(|| invalid("`set` must name at least one field"))?;
    if element.locked {
        return Err(params::locked(&id));
    }
    let mut geometry = false;
    for (field, value) in set {
        if !UPDATABLE.contains(&field.as_str()) {
            let hint = match field.as_str() {
                "text" => " (use element.setText)",
                "start" | "end" => " (use connector.bind)",
                "waypoints" => " (use connector.setWaypoints)",
                "card" => " (use card.setTarget)",
                "image" | "alt" => " (use image.insert or image.setAlt)",
                "locked" => " (use element.lock)",
                _ => "",
            };
            return Err(invalid(format!("`{field}` cannot be set by element.set{hint}")));
        }
        let checked = fields::check(element.kind, field, value)?;
        match (field.as_str(), checked.as_str()) {
            ("parent", Some(parent)) => check_parent(model, page, Some(&id), &ObjectId::from_string(parent))?,
            ("key", Some(key)) => check_key_free(model, page, key, Some(&id))?,
            _ => {}
        }
        // A new shapeType is a new outline: the ends bound to it move onto it (design note 1.4).
        geometry |= matches!(field.as_str(), "position" | "size" | "rotation" | "shapeType");
        tx.set(&id, field, checked)?;
    }
    if geometry {
        edit::refresh_bound_ends(model, tx, page, &BTreeSet::from([id.clone()]))?;
    }
    Ok(json!({ "updated": [id.as_str()] }))
}

/// `element.delete { ids }`: with everything inside frames and groups.
pub fn delete(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let ids = params::ids(params, "ids")?;
    let (_, page) = params::elements_on_one_page(model, &ids)?;
    let deleted = edit::delete(model, tx, page, &ids)?;
    Ok(json!({ "deleted": id_list(&deleted) }))
}

/// The smallest frame on `page` whose box holds `point`, not one of `excluding` (v1's
/// `getBoardFrameAtPoint`).
pub fn frame_at<'a>(model: &'a BoardModel, page: &'a PageView, point: Point, excluding: &HashSet<&ObjectId>) -> Option<&'a Element> {
    let frames = model
        .elements_of(page)
        .filter(|element| element.kind == Kind::Frame)
        .filter_map(|element| model.rect(element).map(|rect| (&element.id, rect)));
    smallest_frame(frames, point, excluding).and_then(|id| model.element(id))
}

/// The common destination query for execution and historical premise replay. Equal-area ties
/// keep page order, exactly as Iterator::min_by in the original implementation.
pub fn smallest_frame<'a>(frames: impl Iterator<Item = (&'a ObjectId, Rect)>, point: Point, excluding: &HashSet<&ObjectId>) -> Option<&'a ObjectId> {
    frames
        .filter(|(id, _)| !excluding.contains(id))
        .filter(|(_, rect)| point.x >= rect.x0 && point.x <= rect.x1 && point.y >= rect.y0 && point.y <= rect.y1)
        .min_by(|(_, a), (_, b)| (a.width() * a.height()).total_cmp(&(b.width() * b.height())))
        .map(|(id, _)| id)
}

/// After a move by (`dx`, `dy`), each root's frame (v1's translate end in
/// `useWhiteboardInteractionMachine.ts`): a root whose centre lands in a frame it is not moving
/// with becomes that frame's child, in front of it; one that leaves its frame for none leaves it;
/// a group's member stays in its group. Answers the roots whose parent changed, with the new one.
fn reparent(model: &BoardModel, tx: &mut Tx<'_>, page: &PageView, tops: &[ObjectId], moved: &[ObjectId], dx: f64, dy: f64) -> Result<Vec<Value>> {
    let moving: HashSet<&ObjectId> = moved.iter().collect();
    let mut out = Vec::new();
    for id in tops {
        let Some(element) = model.element(id) else { continue };
        let Some(bounds) = model.bounds(element) else { continue };
        let center = bounds.center();
        let frame = frame_at(model, page, Point::new(center.x + dx, center.y + dy), &moving);
        let current = element.parent.as_ref();
        let container = current.and_then(|parent| model.element(parent)).map(|parent| parent.kind);
        if container == Some(Kind::Group) {
            // A group's member stays in its group; the group itself is a root when it moves.
            continue;
        }
        let in_frame = container == Some(Kind::Frame);
        let next = match frame {
            Some(frame) => Some(&frame.id),
            None if in_frame => None,
            None => current,
        };
        if next == current {
            continue;
        }
        match next {
            Some(parent) => {
                check_parent(model, page, Some(id), parent)?;
                tx.set(id, "parent", json!(parent.as_str()))?;
                // A child is drawn in front of its frame.
                if !element.kind.can_contain() {
                    let items = tx.list_items(&page.id, "elements")?;
                    let (from, at) = (items.iter().position(|item| item == id), items.iter().position(|item| item == parent));
                    if let (Some(from), Some(at)) = (from, at)
                        && from < at
                    {
                        tx.list_move(&page.id, "elements", from, at)?;
                    }
                }
            }
            None => tx.set(id, "parent", Value::Null)?,
        }
        out.push(json!({ "id": id.as_str(), "parent": next.map(ObjectId::as_str) }));
    }
    Ok(out)
}

/// `element.move { ids, delta: [dx, dy] }` or `{ ids, to: [x, y] }` (the selection's top-left);
/// with `reparent: true`, each moved root takes the frame its centre lands in (a drag's drop).
pub fn move_elements(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let ids = params::ids(params, "ids")?;
    let (elements, page) = params::elements_on_one_page(model, &ids)?;
    let (dx, dy) = match (params::opt_point(params, "delta")?, params::opt_point(params, "to")?) {
        (Some(delta), None) => (delta.x, delta.y),
        (None, Some(to)) => {
            let bounds =
                elements.iter().filter_map(|element| model.bounds(element)).reduce(Rect::union).ok_or_else(|| invalid("the elements have no box"))?;
            (to.x - bounds.x0, to.y - bounds.y0)
        }
        _ => return Err(invalid("give exactly one of `delta` and `to`")),
    };
    let moved = edit::move_by(model, tx, page, &ids, dx, dy)?;
    if params::opt_bool(params, "reparent")?.unwrap_or(false) {
        let reparented = reparent(model, tx, page, &roots(model, &ids), &moved, dx, dy)?;
        return Ok(json!({ "moved": id_list(&moved), "reparented": reparented }));
    }
    Ok(json!({ "moved": id_list(&moved) }))
}

/// Writes `element`'s geometry mapped into a new box: `map` takes old world points to new ones,
/// `scale` the size factors.
fn reshape(tx: &mut Tx<'_>, element: &Element, map: &dyn Fn(Point) -> Point, scale: (f64, f64)) -> Result<()> {
    match element.kind {
        Kind::Group => {}
        Kind::Stroke => tx.set(&element.id, "points", Point::list_json(&element.points.iter().map(|p| map(*p)).collect::<Vec<_>>()))?,
        Kind::Connector => {
            for (terminal, end) in [("start", &element.start), ("end", &element.end)] {
                if let Some(end) = end
                    && end.binding.is_none()
                {
                    tx.set(&element.id, terminal, End::free(map(end.point)).to_json())?;
                }
            }
            if !element.waypoints.is_empty() {
                tx.set(&element.id, "waypoints", Point::list_json(&element.waypoints.iter().map(|p| map(*p)).collect::<Vec<_>>()))?;
            }
        }
        _ => {
            if let Some(position) = element.position {
                tx.set(&element.id, "position", map(position).to_object())?;
            }
            if let Some(size) = element.size {
                tx.set(&element.id, "size", Size::new(size.w * scale.0.abs(), size.h * scale.1.abs()).to_json())?;
            }
        }
    }
    Ok(())
}

/// `element.resize { id, box: [x0, y0, x1, y1] }`: a box gets the new box (a rotated one keeps
/// its rotation about the new box's centre); a stroke, a group or a free connector is scaled into
/// it. `{ ids, box }` scales a selection: `box` is the new box of the elements' bounds.
pub fn resize(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    if params.get("ids").is_some_and(|ids| !ids.is_null()) {
        return resize_many(model, tx, params);
    }
    let id = params::id(params, "id")?;
    let element = params::element(model, &id)?;
    let target = params::rect(params.get("box").unwrap_or(&Value::Null), "box")?;
    let page = model.page_of(&id).ok_or_else(|| not_found(format!("no element {}", id.as_str())))?;
    // A box takes the new box and what it holds stays where it is: only the box is written. A
    // stroke, a group or a free connector is scaled with everything inside it.
    let moved = if element.kind.boxed() { vec![id.clone()] } else { subtree(model, std::slice::from_ref(&id)) };
    check_unlocked(model, &moved)?;
    let old = model.bounds(element).ok_or_else(|| invalid(format!("a {} without geometry cannot be resized", element.kind.as_str())))?;
    if element.kind.boxed() {
        tx.set(&id, "position", Point::new(target.x0, target.y0).to_object())?;
        tx.set(&id, "size", Size::new(target.width(), target.height()).to_json())?;
        // A frame's content stays where it is; only the frame changes.
    } else {
        let sx = if old.width() > 0.0 { target.width() / old.width() } else { 1.0 };
        let sy = if old.height() > 0.0 { target.height() / old.height() } else { 1.0 };
        let map = |point: Point| Point::new(target.x0 + (point.x - old.x0) * sx, target.y0 + (point.y - old.y0) * sy);
        for inner in &moved {
            if let Some(inner) = model.element(inner) {
                reshape(tx, inner, &map, (sx, sy))?;
            }
        }
    }
    edit::refresh_bound_ends(model, tx, page, &moved.iter().cloned().collect())?;
    Ok(json!({ "updated": id_list(&moved) }))
}

/// The smallest side a resized box keeps.
const MIN_SIDE: f64 = 1.0;

/// `element.resize { ids, box }`: every element of the selection (and what is inside them) mapped
/// from the selection's bounds into `box`, as v1's `resizeWhiteboardElements`: a box keeps its
/// centre's place in the selection and scales its size (its rotation stays), a text's letters
/// scale with the area, strokes and free connector ends follow their points.
fn resize_many(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let ids = params::ids(params, "ids")?;
    if params.get("id").is_some_and(|id| !id.is_null()) {
        return Err(invalid("give `id` or `ids`, not both"));
    }
    let target = params::rect(params.get("box").unwrap_or(&Value::Null), "box")?;
    let (_, page) = params::elements_on_one_page(model, &ids)?;
    let tops = roots(model, &ids);
    let moved = subtree(model, &tops);
    check_unlocked(model, &moved)?;
    let old = tops
        .iter()
        .filter_map(|id| model.element(id))
        .filter_map(|element| model.bounds(element))
        .reduce(Rect::union)
        .ok_or_else(|| invalid("the elements have no box"))?;
    let sx = if old.width() > 0.0 { target.width() / old.width() } else { 1.0 };
    let sy = if old.height() > 0.0 { target.height() / old.height() } else { 1.0 };
    let map = |point: Point| Point::new(target.x0 + (point.x - old.x0) * sx, target.y0 + (point.y - old.y0) * sy);
    for id in &moved {
        let Some(element) = model.element(id) else { continue };
        match element.kind {
            Kind::Group => {}
            Kind::Stroke | Kind::Connector => reshape(tx, element, &map, (sx, sy))?,
            Kind::Text => {
                if let Some(position) = element.position {
                    tx.set(id, "position", map(position).to_object())?;
                }
                if let Some(size) = element.size {
                    tx.set(id, "size", Size::new((size.w * sx).max(MIN_SIDE), (size.h * sy).max(MIN_SIDE)).to_json())?;
                }
                let letters = element.register("fontSize").and_then(Value::as_f64).unwrap_or(kition_board_kdm::geometry::DEFAULT_TEXT_SIZE);
                let scaled = (letters * (sx * sy).max(0.04).sqrt()).clamp(8.0, 1000.0);
                tx.set(id, "fontSize", json!((scaled * 100.0).round() / 100.0))?;
            }
            _ => {
                let Some(rect) = model.rect(element) else { continue };
                let center = map(rect.center());
                let (w, h) = ((rect.width() * sx).max(MIN_SIDE), (rect.height() * sy).max(MIN_SIDE));
                tx.set(id, "position", Point::new(center.x - w / 2.0, center.y - h / 2.0).to_object())?;
                tx.set(id, "size", Size::new(w, h).to_json())?;
            }
        }
    }
    edit::refresh_bound_ends(model, tx, page, &moved.iter().cloned().collect())?;
    Ok(json!({ "updated": id_list(&moved) }))
}

/// `element.rotate { ids, degrees, about?: "each" | "selection" }`.
pub fn rotate(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let ids = params::ids(params, "ids")?;
    let degrees = params::num(params, "degrees")?;
    if degrees.abs() > 3600.0 {
        return Err(invalid("`degrees` must be within ±3600"));
    }
    let about = params::opt_string(params, "about")?.unwrap_or("each");
    let (elements, page) = params::elements_on_one_page(model, &ids)?;
    let tops = roots(model, &ids);
    let moved = subtree(model, &tops);
    check_unlocked(model, &moved)?;
    let selection_center = match about {
        "each" => None,
        "selection" => elements.iter().filter_map(|element| model.bounds(element)).reduce(Rect::union).map(|bounds| bounds.center()),
        _ => return Err(invalid("`about` must be each or selection")),
    };
    // Each selected root turns about its own centre with everything inside it (a group as one),
    // or the whole selection about the selection's centre.
    for top in &tops {
        let Some(root) = model.element(top) else { continue };
        let center = selection_center.or_else(|| model.bounds(root).map(|bounds| bounds.center())).unwrap_or_default();
        for id in subtree(model, std::slice::from_ref(top)) {
            let Some(element) = model.element(&id) else { continue };
            match element.kind {
                Kind::Group => {}
                Kind::Stroke | Kind::Connector => reshape(tx, element, &|point: Point| point.rotate(center, degrees), (1.0, 1.0))?,
                _ => {
                    if let Some(rect) = model.rect(element) {
                        let moved_center = rect.center().rotate(center, degrees);
                        let (dx, dy) = (moved_center.x - rect.center().x, moved_center.y - rect.center().y);
                        if dx != 0.0 || dy != 0.0 {
                            tx.set(&id, "position", Point::new(rect.x0 + dx, rect.y0 + dy).to_object())?;
                        }
                    }
                    // A frame or a card has no rotation (`element.set` refuses it): it only moves.
                    if fields::uses(element.kind, "rotation") {
                        let rotation = (element.rotation + degrees).rem_euclid(360.0);
                        tx.set(&id, "rotation", json!(rotation))?;
                    }
                }
            }
        }
    }
    edit::refresh_bound_ends(model, tx, page, &moved.iter().cloned().collect())?;
    Ok(json!({ "updated": id_list(&moved) }))
}

/// `element.style { ids, style: { strokeColor?, fillColor?, … } }`: each element takes the fields
/// its kind uses; a field no element uses is refused.
pub fn style(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let ids = params::ids(params, "ids")?;
    let style = params::object(params, "style")?.filter(|style| !style.is_empty()).ok_or_else(|| invalid("`style` must name at least one field"))?;
    let (elements, page) = params::elements_on_one_page(model, &ids)?;
    for field in style.keys() {
        if !STYLE_FIELDS.contains(&field.as_str()) {
            return Err(invalid(format!("`{field}` is not a style field ({})", STYLE_FIELDS.join(", "))));
        }
        if !elements.iter().any(|element| fields::uses(element.kind, field)) {
            return Err(invalid(format!("none of the elements uses `{field}`")));
        }
    }
    let mut updated = Vec::new();
    for element in elements {
        if element.locked {
            return Err(params::locked(&element.id));
        }
        let mut touched = false;
        for (field, value) in style {
            if fields::uses(element.kind, field) {
                tx.set(&element.id, field, fields::check(element.kind, field, value)?)?;
                touched = true;
            }
        }
        if touched {
            updated.push(element.id.clone());
        }
    }
    // A text's box is measured from its letters (its height too when it has a fixed width): ends
    // bound to it follow.
    let measured: BTreeSet<ObjectId> = updated
        .iter()
        .filter(|id| model.element(id).is_some_and(|element| element.kind == Kind::Text))
        .filter(|_| style.keys().any(|field| field == "fontSize" || field == "fontFamily"))
        .cloned()
        .collect();
    if !measured.is_empty() {
        edit::refresh_bound_ends(model, tx, page, &measured)?;
    }
    Ok(json!({ "updated": id_list(&updated) }))
}

/// `element.setText { id, text }`: the label, title or caption, written as the difference to
/// the current text so concurrent typing merges.
pub fn set_text(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let id = params::id(params, "id")?;
    let text = params::string(params, "text")?;
    let element = params::element(model, &id)?;
    if !fields::uses(element.kind, "text") {
        return Err(invalid(format!("a {} has no text", element.kind.as_str())));
    }
    if element.locked {
        return Err(params::locked(&id));
    }
    if text.chars().count() > 100_000 {
        return Err(invalid("`text` is longer than 100000 characters"));
    }
    write_text(tx, &id, "text", &element.text, text)?;
    refresh_measured(model, tx, element, || edit::retexted(element, text))?;
    Ok(json!({ "updated": [id.as_str()] }))
}

/// After a change of a text's characters or marks: the ends bound to a text follow its measured
/// box (its width without a stored size, its height with a fixed width), which `fresh` gives (the
/// transaction cannot read a text back).
fn refresh_measured(model: &BoardModel, tx: &mut Tx<'_>, element: &Element, fresh: impl FnOnce() -> Element) -> Result<()> {
    if element.kind != Kind::Text || model.connectors_bound_to(&element.id).is_empty() {
        return Ok(());
    }
    let Some(page) = model.page_of(&element.id) else { return Ok(()) };
    let fresh = edit::Fresh::from([(element.id.clone(), fresh())]);
    edit::refresh_bound_ends_in(model, tx, page, &BTreeSet::from([element.id.clone()]), &fresh)
}

/// UTF-16 offset `at` in `text` as a Unicode scalar offset.
fn scalar_offset(text: &str, at: usize) -> Result<usize> {
    let mut units = 0;
    for (index, ch) in text.chars().enumerate() {
        if units == at {
            return Ok(index);
        }
        units += ch.len_utf16();
        if units > at {
            return Err(invalid("a UTF-16 offset falls inside a character"));
        }
    }
    if units == at { Ok(text.chars().count()) } else { Err(invalid("the range is outside the text")) }
}

/// `text.mark { id, range: [start, end], mark: bold | italic | link, value?, units? }`: `value`
/// `null` removes the mark; `link` takes `{ "href" }` (http, https or mailto).
pub fn mark(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let id = params::id(params, "id")?;
    let element = params::element(model, &id)?;
    if !fields::uses(element.kind, "text") {
        return Err(invalid(format!("a {} has no text", element.kind.as_str())));
    }
    if element.locked {
        return Err(params::locked(&id));
    }
    let range =
        params.get("range").and_then(Value::as_array).filter(|range| range.len() == 2).ok_or_else(|| invalid("`range` must be [start, end]"))?;
    let bound = |value: Option<&Value>| {
        value.and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok()).ok_or_else(|| invalid("`range` must hold whole numbers"))
    };
    let (mut start, mut end) = (bound(range.first())?, bound(range.get(1))?);
    match params::opt_string(params, "units")?.unwrap_or("scalar") {
        "scalar" => {}
        "utf16" => (start, end) = (scalar_offset(&element.text, start)?, scalar_offset(&element.text, end)?),
        _ => return Err(invalid("`units` must be scalar or utf16")),
    }
    if start >= end || end > element.text.chars().count() {
        return Err(invalid("`range` must be a non-empty range inside the text"));
    }
    let mark = params::string(params, "mark")?;
    let value = params.get("value").cloned();
    let value = match (mark, value) {
        ("bold" | "italic", None) => json!(true),
        ("bold" | "italic", Some(Value::Bool(true))) => json!(true),
        ("bold" | "italic" | "link", Some(Value::Null)) => Value::Null,
        ("link", Some(link)) => {
            let href = link.get("href").and_then(Value::as_str).ok_or_else(|| invalid("a link `value` is { href }"))?;
            if !["http://", "https://", "mailto:"].iter().any(|scheme| href.starts_with(scheme)) {
                return Err(invalid("a link must be http, https or mailto"));
            }
            json!({ "href": href })
        }
        ("bold" | "italic" | "link", _) => return Err(invalid("`value` must be true, a link { href }, or null to remove")),
        _ => return Err(invalid("`mark` must be bold, italic or link")),
    };
    if value.is_null() {
        tx.text_unmark(&id, "text", start, end, mark)?;
    } else {
        tx.text_mark(&id, "text", start, end, mark, value.clone())?;
    }
    refresh_measured(model, tx, element, || {
        let mut marks = edit::char_marks(element);
        for marks in marks.iter_mut().take(end).skip(start) {
            if value.is_null() {
                marks.remove(mark);
            } else {
                marks.insert(mark.to_owned(), value.clone());
            }
        }
        edit::text_element(&id, element.text.clone(), edit::runs_of(&element.text, &marks), element.register("textFormat").cloned())
    })?;
    Ok(json!({ "updated": [id.as_str()] }))
}

/// `element.reorder { ids, placement: front | forward | backward | back }`.
pub fn reorder(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let ids = params::ids(params, "ids")?;
    let placement = params::string(params, "placement")?;
    let (_, page) = params::elements_on_one_page(model, &ids)?;
    // A container moves with what it holds (v1's `reorderBoardElements`): a frame stays behind its
    // children, a group before its members.
    let moving = subtree(model, &ids);
    let selected: BTreeSet<&ObjectId> = moving.iter().collect();
    let mut order: Vec<ObjectId> = tx.list_items(&page.id, "elements")?;
    match placement {
        "front" => {
            let (picked, rest): (Vec<_>, Vec<_>) = order.into_iter().partition(|id| selected.contains(id));
            order = rest.into_iter().chain(picked).collect();
        }
        "back" => {
            let (picked, rest): (Vec<_>, Vec<_>) = order.into_iter().partition(|id| selected.contains(id));
            order = picked.into_iter().chain(rest).collect();
        }
        "forward" => {
            for index in (0..order.len().saturating_sub(1)).rev() {
                if order.get(index).is_some_and(|id| selected.contains(id)) && order.get(index + 1).is_some_and(|id| !selected.contains(id)) {
                    order.swap(index, index + 1);
                }
            }
        }
        "backward" => {
            for index in 1..order.len() {
                if order.get(index).is_some_and(|id| selected.contains(id)) && order.get(index - 1).is_some_and(|id| !selected.contains(id)) {
                    order.swap(index, index - 1);
                }
            }
        }
        _ => return Err(invalid("`placement` must be front, forward, backward or back")),
    }
    // Moves into the new order one item at a time, so concurrent edits of the list merge.
    for (target, id) in order.iter().enumerate() {
        let current = tx.list_items(&page.id, "elements")?;
        if let Some(from) = current.iter().position(|item| item == id)
            && from != target
        {
            tx.list_move(&page.id, "elements", from, target)?;
        }
    }
    Ok(json!({ "updated": id_list(&ids) }))
}

/// `element.lock { ids, locked }`.
pub fn lock(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let ids = params::ids(params, "ids")?;
    let flag = params::opt_bool(params, "locked")?.ok_or_else(|| invalid("`locked` must be true or false"))?;
    params::elements_on_one_page(model, &ids)?;
    for id in &ids {
        tx.set(id, "locked", if flag { json!(true) } else { Value::Null })?;
    }
    Ok(json!({ "updated": id_list(&ids) }))
}

/// `arrange.group { ids }`: a new group holding the selection's roots, inside their common parent.
pub fn group(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let ids = params::ids(params, "ids")?;
    let (_, page) = params::elements_on_one_page(model, &ids)?;
    let members = roots(model, &ids);
    check_unlocked(model, &members)?;
    let parents: BTreeSet<Option<ObjectId>> = members.iter().map(|id| model.element(id).and_then(|element| element.parent.clone())).collect();
    let mut fields = Map::new();
    fields.insert("kind".into(), json!("group"));
    if let (1, Some(Some(parent))) = (parents.len(), parents.iter().next()) {
        fields.insert("parent".into(), json!(parent.as_str()));
    }
    let group = tx.create(ELEMENT, fields)?;
    let first = tx.list_items(&page.id, "elements")?.iter().position(|id| members.contains(id)).unwrap_or(0);
    tx.list_insert(&page.id, "elements", first, &group)?;
    for id in &members {
        tx.set(id, "parent", json!(group.as_str()))?;
    }
    Ok(json!({ "group": group.as_str(), "members": id_list(&members) }))
}

/// `arrange.ungroup { groupIds }`: the members move to the group's parent; the groups go.
pub fn ungroup(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let ids = params::ids(params, "groupIds")?;
    let (groups, page) = params::elements_on_one_page(model, &ids)?;
    let going: HashSet<&ObjectId> = ids.iter().collect();
    let mut freed = Vec::new();
    for group in groups {
        if group.kind != Kind::Group {
            return Err(invalid(format!("{} is a {}, not a group", group.id.as_str(), group.kind.as_str())));
        }
        if group.locked {
            return Err(params::locked(&group.id));
        }
        // The members go to the nearest container above that stays: a group inside another one
        // ungrouped with it goes too.
        let mut above = group.parent.clone();
        let mut depth = 0;
        while let Some(parent) = above.as_ref().filter(|parent| going.contains(parent)) {
            if depth > kition_board_kdm::view::MAX_DEPTH {
                above = None;
                break;
            }
            depth += 1;
            above = model.element(parent).and_then(|element| element.parent.clone());
        }
        let parent = above.as_ref().map_or(Value::Null, |parent| json!(parent.as_str()));
        for child in model.children(&group.id) {
            if going.contains(&child.id) {
                continue;
            }
            if child.locked {
                return Err(params::locked(&child.id));
            }
            tx.set(&child.id, "parent", parent.clone())?;
            freed.push(child.id.clone());
        }
        edit::unlist(tx, &page.id, &group.id)?;
        tx.delete(&group.id)?;
    }
    Ok(json!({ "ungrouped": id_list(&ids), "members": id_list(&freed) }))
}

/// `frame.add { page?, box, title?, children? }`: a frame behind its children.
pub fn frame_create(model: &BoardModel, tx: &mut Tx<'_>, ctx: &Ctx<'_>, params: &Value) -> Result<Value> {
    let page = params::page(model, params, ctx.active_page)?;
    let rect = params::rect(params.get("box").unwrap_or(&Value::Null), "box")?;
    let title = params::opt_string(params, "title")?.unwrap_or_default();
    let children = match params.get("children") {
        None | Some(Value::Null) => Vec::new(),
        Some(_) => params::ids(params, "children")?,
    };
    for child in &children {
        params::element(model, child)?;
        if model.page_of(child).map(|found| &found.id) != Some(&page.id) {
            return Err(invalid("the children must be on the frame's page"));
        }
    }
    let members = roots(model, &children);
    check_unlocked(model, &members)?;
    let mut fields = Map::new();
    fields.insert("kind".into(), json!("frame"));
    fields.insert("position".into(), Point::new(rect.x0, rect.y0).to_object());
    fields.insert("size".into(), Size::new(rect.width(), rect.height()).to_json());
    let frame = tx.create(ELEMENT, fields)?;
    if !title.is_empty() {
        tx.text_insert(&frame, "text", 0, title)?;
    }
    let at = tx.list_items(&page.id, "elements")?.iter().position(|id| members.contains(id));
    match at {
        Some(at) => tx.list_insert(&page.id, "elements", at, &frame)?,
        None => edit::append(tx, &page.id, &frame)?,
    }
    for id in &members {
        tx.set(id, "parent", json!(frame.as_str()))?;
    }
    Ok(json!({ "frame": frame.as_str(), "children": id_list(&members) }))
}

/// `frame.fit { id, padding? }`: the frame's box around its children.
pub fn frame_fit(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let id = params::id(params, "id")?;
    let frame = params::element(model, &id)?;
    if frame.kind != Kind::Frame {
        return Err(invalid(format!("{} is not a frame", id.as_str())));
    }
    if frame.locked {
        return Err(params::locked(&id));
    }
    let padding = params::opt_num(params, "padding")?.unwrap_or(32.0);
    if !(0.0..=10_000.0).contains(&padding) {
        return Err(invalid("`padding` must be between 0 and 10000"));
    }
    let bounds = model
        .children(&id)
        .into_iter()
        .filter_map(|child| model.bounds(child))
        .reduce(Rect::union)
        .ok_or_else(|| invalid("the frame has no children to fit"))?;
    tx.set(&id, "position", Point::new(bounds.x0 - padding, bounds.y0 - padding).to_object())?;
    tx.set(&id, "size", Size::new(bounds.width() + 2.0 * padding, bounds.height() + 2.0 * padding).to_json())?;
    if let Some(page) = model.page_of(&id) {
        edit::refresh_bound_ends(model, tx, page, &BTreeSet::from([id.clone()]))?;
    }
    Ok(json!({ "updated": [id.as_str()] }))
}

/// `stroke.add { page?, points, style? }`.
pub fn stroke_create(model: &BoardModel, tx: &mut Tx<'_>, ctx: &Ctx<'_>, params: &Value) -> Result<Value> {
    let page = params::page(model, params, ctx.active_page)?;
    let points = params::points(params.get("points").unwrap_or(&Value::Null), "points", 100_000)?;
    if points.len() < 2 {
        return Err(invalid("a stroke needs at least two `points`"));
    }
    let mut fields = Map::new();
    fields.insert("kind".into(), json!("stroke"));
    fields.insert("points".into(), Point::list_json(&points));
    if let Some(style) = params::object(params, "style")? {
        for (field, value) in style {
            if !["strokeColor", "strokeSize", "dashStyle", "opacity"].contains(&field.as_str()) {
                return Err(invalid(format!("a stroke has no `{field}`")));
            }
            fields.insert(field.clone(), fields::check(Kind::Stroke, field, value)?);
        }
    }
    let mut list = edit::Appender::new(tx, &page.id)?;
    let id = create_one(tx, &mut list, fields, "")?;
    Ok(created(&[id]))
}
