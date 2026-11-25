//! Connector commands (design note 5.1): create with bound or free ends, rebind an end, the label
//! and its position, the bends. Bound ends are written with the point derived from the target in
//! the same transaction (design note 1.4, gap 14).
//!
//! Ported from Kition's `packages/board-editor/src/lib/boardBindingEngine.ts`
//! (`getBoardConnectorAnchor`, `getBoardConnectorAnchorAt`) and the `connector.add` and
//! `connector.update-terminal` commands of `boardCommands.ts`.

use kition_board_kdm::geometry::{clamp_anchor, nearest_edge_anchor};
use kition_board_kdm::outline::outline_point;
use kition_board_kdm::{Binding, BoardModel, ELEMENT, End, Kind, PageView, Point};
use kition_kit::kdm::{ObjectId, Tx};
use kition_kit::session::write_text;
use serde_json::{Map, Value, json};

use crate::Ctx;
use crate::edit;
use crate::fields;
use crate::params::{self, Result, invalid};

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
    if let (EndSpec::Element { id: a, .. }, EndSpec::Element { id: b, .. }) = (&from, &to)
        && a == b
    {
        return Err(invalid("a connector's ends must be on different elements"));
    }
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
