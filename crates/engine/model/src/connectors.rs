//! Connector commands (design note 5.1): create with bound or free ends, rebind an end, the label
//! and its position, the bends, and the route a connector would take without writing it
//! (`connector.getRoute`, drawn by the binding with the scene's own connector drawing). Bound ends are written with the point derived from the target in
//! the same transaction (design note 1.4, gap 14).
//!
//! Ported from Kition's `packages/board-editor/src/lib/boardBindingEngine.ts`
//! (`getBoardConnectorAnchor`, `getBoardConnectorAnchorAt`) and the `connector.add` and
//! `connector.update-terminal` commands of `boardCommands.ts`.

use kition_board_kdm::geometry::{anchor_side, clamp_anchor, nearest_edge_anchor};
use kition_board_kdm::outline::outline_point;
use kition_board_kdm::{Binding, BoardModel, ELEMENT, Element, End, Kind, PageView, Point, Rect};
use kition_kit::kdm::{ObjectId, Tx};
use kition_kit::session::write_text;
use serde_json::{Map, Value, json};

use crate::Ctx;
use crate::edit;
use crate::fields;
use crate::params::{self, Result, invalid, not_found};

/// A connector end as a command gives it: `{ "element": id, "anchor"?: [ax, ay] }` or
/// `{ "point": [x, y] }`.
enum EndSpec {
    Element { id: ObjectId, anchor: Option<[f64; 2]> },
    Point(Point),
}

fn end_spec(value: Option<&Value>, what: &str) -> Result<EndSpec> {
    let value = value.and_then(Value::as_object).ok_or_else(|| invalid(format!("`{what}` must be {{ element, anchor? }} or {{ point }}")))?;
    match (value.get("element"), value.get("point")) {
        (Some(element), None) => {
            let id = element.as_str().ok_or_else(|| invalid(format!("`{what}.element` must be an element id")))?;
            let anchor = match value.get("anchor") {
                None | Some(Value::Null) => None,
                Some(anchor) => {
                    let point = params::point(anchor, &format!("{what}.anchor"))?;
                    Some(clamp_anchor([point.x, point.y]))
                }
            };
            Ok(EndSpec::Element { id: ObjectId::from_string(id), anchor })
        }
        (None, Some(point)) => Ok(EndSpec::Point(params::point(point, &format!("{what}.point"))?)),
        _ => Err(invalid(format!("`{what}` must have exactly one of `element` and `point`"))),
    }
}

/// The element an end binds to, checked: shown on `page`, of a bindable kind.
fn bind_target<'a>(model: &'a BoardModel, page: &PageView, id: &ObjectId) -> Result<&'a kition_board_kdm::Element> {
    let target = params::element(model, id)?;
    if model.page_of(id).map(|found| &found.id) != Some(&page.id) {
        return Err(invalid(format!("{} is on another page", id.as_str())));
    }
    if !target.kind.bindable() {
        return Err(invalid(format!("a connector cannot bind to a {}", target.kind.as_str())));
    }
    Ok(target)
}

/// A rough point of an end, to aim the other end's anchor at.
fn aim(model: &BoardModel, page: &PageView, spec: &EndSpec) -> Result<Point> {
    match spec {
        EndSpec::Point(point) => Ok(*point),
        EndSpec::Element { id, anchor } => {
            let target = bind_target(model, page, id)?;
            let rect = model.rect(target).ok_or_else(|| invalid(format!("{} has no box", id.as_str())))?;
            Ok(match anchor {
                Some(anchor) => outline_point(target.outline_shape(), rect, target.rotation, *anchor),
                None => rect.center().rotate(rect.center(), target.rotation),
            })
        }
    }
}

/// The stored end for `spec`; an element end without an anchor binds to the edge nearest `toward`
/// (an anchor on the box, which the end resolves onto the target's outline along the same ray from
/// the centre, design note 1.4).
fn resolve(model: &BoardModel, page: &PageView, spec: &EndSpec, toward: Point) -> Result<End> {
    match spec {
        EndSpec::Point(point) => Ok(End::free(*point)),
        EndSpec::Element { id, anchor } => {
            let target = bind_target(model, page, id)?;
            let rect = model.rect(target).ok_or_else(|| invalid(format!("{} has no box", id.as_str())))?;
            let anchor = anchor.unwrap_or_else(|| nearest_edge_anchor(rect, target.rotation, toward).0);
            Ok(End {
                point: outline_point(target.outline_shape(), rect, target.rotation, anchor),
                binding: Some(Binding { target: id.clone(), anchor }),
            })
        }
    }
}

/// `connector.add { page?, from, to, label?, type?, startArrowhead?, endArrowhead?, waypoints?, style? }`.
pub fn create(model: &BoardModel, tx: &mut Tx<'_>, ctx: &Ctx<'_>, params: &Value) -> Result<Value> {
    let page = params::page(model, params, ctx.active_page)?;
    let from = end_spec(params.get("from"), "from")?;
    let to = end_spec(params.get("to"), "to")?;
    same_target_refused(&from, &to)?;
    let (from_aim, to_aim) = (aim(model, page, &from)?, aim(model, page, &to)?);
    let start = resolve(model, page, &from, to_aim)?;
    let end = resolve(model, page, &to, from_aim)?;
    let mut fields = Map::new();
    fields.insert("kind".into(), json!("connector"));
    fields.insert("start".into(), start.to_json());
    fields.insert("end".into(), end.to_json());
    fields.insert("endArrowhead".into(), json!("arrow"));
    for (param, field) in
        [("type", "connectorType"), ("startArrowhead", "startArrowhead"), ("endArrowhead", "endArrowhead"), ("labelPosition", "labelPosition")]
    {
        if let Some(value) = params.get(param).filter(|value| !value.is_null()) {
            fields.insert(field.into(), fields::check(Kind::Connector, field, value)?);
        }
    }
    if let Some(waypoints) = params.get("waypoints").filter(|value| !value.is_null()) {
        fields.insert("waypoints".into(), fields::check(Kind::Connector, "waypoints", waypoints)?);
    }
    if let Some(style) = params::object(params, "style")? {
        for (field, value) in style {
            if !["strokeColor", "strokeSize", "dashStyle", "opacity", "connectorType", "startArrowhead", "endArrowhead"].contains(&field.as_str()) {
                return Err(invalid(format!("a connector has no style `{field}`")));
            }
            fields.insert(field.clone(), fields::check(Kind::Connector, field, value)?);
        }
    }
    if let Some(key) = params::opt_string(params, "key")? {
        let key = fields::check(Kind::Connector, "key", &json!(key))?;
        edit::check_key_free(model, page, key.as_str().unwrap_or_default(), None)?;
        fields.insert("key".into(), key);
    }
    let label = params::opt_string(params, "label")?.unwrap_or_default();
    let id = tx.create(ELEMENT, fields)?;
    if !label.is_empty() {
        tx.text_insert(&id, "text", 0, label)?;
    }
    edit::append(tx, &page.id, &id)?;
    Ok(json!({ "created": [id.as_str()], "start": start.point.to_pair(), "end": end.point.to_pair() }))
}

/// The connector `id`, unlocked.
fn connector<'a>(model: &'a BoardModel, params: &Value) -> Result<(&'a kition_board_kdm::Element, &'a PageView)> {
    let id = params::id(params, "id")?;
    let element = params::element(model, &id)?;
    if element.kind != Kind::Connector {
        return Err(invalid(format!("{} is not a connector", id.as_str())));
    }
    if element.locked {
        return Err(params::locked(&id));
    }
    let page = model.page_of(&id).ok_or_else(|| invalid(format!("no element {}", id.as_str())))?;
    Ok((element, page))
}

/// `connector.bind { id, terminal: start | end, to }`: rebinds or frees one end.
pub fn bind(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let (element, page) = connector(model, params)?;
    let terminal = params::string(params, "terminal")?;
    let other = match terminal {
        "start" => &element.end,
        "end" => &element.start,
        _ => return Err(invalid("`terminal` must be start or end")),
    };
    let spec = end_spec(params.get("to"), "to")?;
    let toward = other.as_ref().map_or_else(Point::default, |other| model.end_point(&element.id, other));
    if let (EndSpec::Element { id, .. }, Some(End { binding: Some(binding), .. })) = (&spec, other)
        && &binding.target == id
    {
        return Err(invalid("a connector's ends must be on different elements"));
    }
    let end = resolve(model, page, &spec, toward)?;
    tx.set(&element.id, terminal, end.to_json())?;
    Ok(json!({ "updated": [element.id.as_str()], "point": end.point.to_pair() }))
}

/// `connector.setLabel { id, text, position? }`.
pub fn set_label(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let (element, _) = connector(model, params)?;
    let text = params::string(params, "text")?;
    if let Some(position) = params.get("position").filter(|value| !value.is_null()) {
        tx.set(&element.id, "labelPosition", fields::check(Kind::Connector, "labelPosition", position)?)?;
    }
    write_text(tx, &element.id, "text", &element.text, text)?;
    Ok(json!({ "updated": [element.id.as_str()] }))
}

/// `connector.setWaypoints { id, waypoints }`: the bends in path order; empty for an automatic
/// route.
pub fn set_waypoints(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let (element, _) = connector(model, params)?;
    let waypoints = params::points(params.get("waypoints").unwrap_or(&Value::Null), "waypoints", 1_000)?;
    let value = if waypoints.is_empty() { Value::Null } else { Point::list_json(&waypoints) };
    tx.set(&element.id, "waypoints", value)?;
    Ok(json!({ "updated": [element.id.as_str()] }))
}

/// The most connectors one `connector.getRoute` call draws.
pub const MAX_ROUTES: usize = 500;

/// One connector `connector.getRoute` draws without writing it: the element as the command it
/// previews would leave it (fields, label, ends resolved as `connector.add` and `connector.bind`
/// resolve them), the route it would be drawn along (start, bends, end, with `move` applied) and
/// the sides its bound ends leave and enter through.
#[derive(Clone, Debug, PartialEq)]
pub struct Planned {
    /// The stored connector it starts from, when the entry names one.
    pub id: Option<ObjectId>,
    pub element: Element,
    pub route: Vec<Point>,
    pub sides: [Option<Point>; 2],
    /// The ends as they would be stored, each point where the end is drawn.
    pub start: Option<End>,
    pub end: Option<End>,
}

/// The keys one entry of `connectors` takes.
const ROUTE_KEYS: &[&str] = &["id", "from", "to", "type", "startArrowhead", "endArrowhead", "labelPosition", "waypoints", "label", "style"];

/// `connector.getRoute { connectors: [{ id?, from?, to?, type?, startArrowhead?, endArrowhead?,
/// labelPosition?, waypoints?, label?, style? }], move?: { ids, delta } }`: each connector as it
/// would be drawn, without writing anything. An entry with `id` starts from that connector, and
/// what it gives replaces what is stored (`from` or `to` alone is a `connector.bind` of that end);
/// an entry without `id` is a new connector between `from` and `to` (as `connector.add`). With
/// `move`, the elements `ids` (and what is inside them) are taken as moved by `delta`, as
/// `element.move` would move them: ends bound to them follow, and a moved connector's free ends and
/// bends shift. Ends on elements must be on one page with each other and with the connector.
pub fn plan(model: &BoardModel, params: &Value) -> Result<Vec<Planned>> {
    let entries = params.get("connectors").and_then(Value::as_array).ok_or_else(|| invalid("`connectors` must be a list"))?;
    if entries.is_empty() {
        return Err(invalid("`connectors` names no connector"));
    }
    if entries.len() > MAX_ROUTES {
        return Err(invalid(format!("at most {MAX_ROUTES} connectors per call, not {}", entries.len())));
    }
    let (moved, delta) = match params::object(params, "move")? {
        None => (std::collections::HashSet::new(), Point::default()),
        Some(motion) => {
            let motion = Value::Object(motion.clone());
            let ids = params::ids(&motion, "ids")?;
            for id in &ids {
                params::element(model, id)?;
            }
            let delta = params::point(motion.get("delta").unwrap_or(&Value::Null), "move.delta")?;
            (edit::subtree(model, &edit::roots(model, &ids)).into_iter().collect(), delta)
        }
    };
    entries.iter().map(|entry| plan_one(model, entry, &moved, delta)).collect()
}

fn plan_one(model: &BoardModel, entry: &Value, moved: &std::collections::HashSet<ObjectId>, delta: Point) -> Result<Planned> {
    let object = entry.as_object().ok_or_else(|| invalid("every entry of `connectors` must be an object"))?;
    if let Some(key) = object.keys().find(|key| !ROUTE_KEYS.contains(&key.as_str())) {
        return Err(invalid(format!("a `connectors` entry has no parameter `{key}`")));
    }
    let given = |key: &str| entry.get(key).filter(|value| !value.is_null());
    let stored = match params::opt_string(entry, "id")? {
        Some(id) => {
            let id = ObjectId::from_string(id);
            let element = params::element(model, &id)?;
            if element.kind != Kind::Connector {
                return Err(invalid(format!("{} is not a connector", id.as_str())));
            }
            Some(element)
        }
        None => None,
    };
    let from = given("from").map(|value| end_spec(Some(value), "from")).transpose()?;
    let to = given("to").map(|value| end_spec(Some(value), "to")).transpose()?;
    if stored.is_none() && (from.is_none() || to.is_none()) {
        return Err(invalid("a new connector needs `from` and `to` (or name a connector with `id`)"));
    }
    let page = route_page(model, stored, [from.as_ref(), to.as_ref()])?;
    let mut element = stored.cloned().unwrap_or_else(|| {
        let mut registers = std::collections::BTreeMap::new();
        registers.insert("endArrowhead".to_owned(), json!("arrow"));
        Element {
            id: ObjectId::from_string("preview"),
            kind: Kind::Connector,
            position: None,
            size: None,
            rotation: 0.0,
            locked: false,
            parent: None,
            key: None,
            text: String::new(),
            runs: Vec::new(),
            points: Vec::new(),
            start: None,
            end: None,
            waypoints: Vec::new(),
            image: None,
            card: None,
            source_ref_ids: Vec::new(),
            registers,
        }
    });
    // The ends as connector.add (both given) or connector.bind (one given) resolves them.
    match (&from, &to) {
        (Some(from), Some(to)) => {
            same_target_refused(from, to)?;
            let (from_aim, to_aim) = (aim_on(model, page, from)?, aim_on(model, page, to)?);
            element.start = Some(resolve_on(model, page, from, to_aim)?);
            element.end = Some(resolve_on(model, page, to, from_aim)?);
        }
        (Some(spec), None) | (None, Some(spec)) => {
            let starting = from.is_some();
            let other = if starting { &element.end } else { &element.start };
            if let (EndSpec::Element { id, .. }, Some(End { binding: Some(binding), .. })) = (spec, other)
                && &binding.target == id
            {
                return Err(invalid("a connector's ends must be on different elements"));
            }
            let toward = other.as_ref().map_or_else(Point::default, |other| model.end_point(&element.id, other));
            let end = Some(resolve_on(model, page, spec, toward)?);
            if starting {
                element.start = end;
            } else {
                element.end = end;
            }
        }
        (None, None) => {}
    }
    for (param, field) in
        [("type", "connectorType"), ("startArrowhead", "startArrowhead"), ("endArrowhead", "endArrowhead"), ("labelPosition", "labelPosition")]
    {
        if let Some(value) = given(param) {
            element.registers.insert(field.to_owned(), fields::check(Kind::Connector, field, value)?);
        }
    }
    if let Some(style) = params::object(entry, "style")? {
        for (field, value) in style {
            if !["strokeColor", "strokeSize", "dashStyle", "opacity", "connectorType", "startArrowhead", "endArrowhead"].contains(&field.as_str()) {
                return Err(invalid(format!("a connector has no style `{field}`")));
            }
            element.registers.insert(field.clone(), fields::check(Kind::Connector, field, value)?);
        }
    }
    if let Some(waypoints) = given("waypoints") {
        element.waypoints = params::points(waypoints, "waypoints", 1_000)?;
    }
    if let Some(label) = params::opt_string(entry, "label")? {
        label.clone_into(&mut element.text);
        element.runs.clear();
    }
    // Where it is drawn: bound ends derived from their targets (moved ones by `delta`), a moved
    // connector's free ends and bends shifted, as element.move leaves them.
    let page_id = page.map(|page| page.id.clone());
    let carried = stored.is_some_and(|stored| moved.contains(&stored.id));
    let place = |end: &End| -> (End, Option<Point>) {
        if let Some(binding) = &end.binding
            && let Some(target) = model.element(&binding.target)
            && target.kind.bindable()
            && model.page_of(&binding.target).map(|found| &found.id) == page_id.as_ref()
            && let Some(rect) = model.rect(target)
        {
            let rect =
                if moved.contains(&target.id) { Rect::new(rect.x0 + delta.x, rect.y0 + delta.y, rect.x1 + delta.x, rect.y1 + delta.y) } else { rect };
            let point = outline_point(target.outline_shape(), rect, target.rotation, binding.anchor);
            return (End { point, binding: Some(binding.clone()) }, anchor_side(binding.anchor, target.rotation));
        }
        let point = if carried && end.binding.is_none() { end.point.offset(delta.x, delta.y) } else { end.point };
        (End { point, binding: end.binding.clone() }, None)
    };
    let start = element.start.as_ref().map(&place);
    let end = element.end.as_ref().map(&place);
    let waypoints: Vec<Point> =
        if carried { element.waypoints.iter().map(|p| p.offset(delta.x, delta.y)).collect() } else { element.waypoints.clone() };
    let mut route = Vec::with_capacity(waypoints.len() + 2);
    route.extend(start.as_ref().map(|(end, _)| end.point));
    route.extend(waypoints);
    route.extend(end.as_ref().map(|(end, _)| end.point));
    let sides = [start.as_ref().and_then(|(_, side)| *side), end.as_ref().and_then(|(_, side)| *side)];
    Ok(Planned { id: stored.map(|stored| stored.id.clone()), element, route, sides, start: start.map(|(end, _)| end), end: end.map(|(end, _)| end) })
}

/// [`aim`] where an end on an element has its page (a free end needs none).
fn aim_on(model: &BoardModel, page: Option<&PageView>, spec: &EndSpec) -> Result<Point> {
    match (spec, page) {
        (EndSpec::Point(point), _) => Ok(*point),
        (_, Some(page)) => aim(model, page, spec),
        (_, None) => Err(invalid("an end on an element needs its page")),
    }
}

/// [`resolve`] where an end on an element has its page (a free end needs none).
fn resolve_on(model: &BoardModel, page: Option<&PageView>, spec: &EndSpec, toward: Point) -> Result<End> {
    match (spec, page) {
        (EndSpec::Point(point), _) => Ok(End::free(*point)),
        (_, Some(page)) => resolve(model, page, spec, toward),
        (_, None) => Err(invalid("an end on an element needs its page")),
    }
}

/// Refuses two ends on one element (as `connector.add` does).
fn same_target_refused(from: &EndSpec, to: &EndSpec) -> Result<()> {
    if let (EndSpec::Element { id: a, .. }, EndSpec::Element { id: b, .. }) = (from, to)
        && a == b
    {
        return Err(invalid("a connector's ends must be on different elements"));
    }
    Ok(())
}

/// The page a previewed connector is on: the stored connector's, else the page of the first end
/// on an element (the other end is checked against it when resolved); `None` for two free ends.
fn route_page<'a>(model: &'a BoardModel, stored: Option<&Element>, specs: [Option<&EndSpec>; 2]) -> Result<Option<&'a PageView>> {
    if let Some(stored) = stored {
        return model.page_of(&stored.id).map(Some).ok_or_else(|| not_found(format!("no element {}", stored.id.as_str())));
    }
    for spec in specs.into_iter().flatten() {
        if let EndSpec::Element { id, .. } = spec {
            params::element(model, id)?;
            return model.page_of(id).map(Some).ok_or_else(|| not_found(format!("no element {}", id.as_str())));
        }
    }
    Ok(None)
}
