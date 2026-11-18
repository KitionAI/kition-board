//! C8-land-11-board: command dependencies, audited with the implementations in this crate.
use std::collections::{BTreeMap, BTreeSet};

use kition_board_kdm::{BoardModel, Element, Kind, Point, Rect, root_id};
use kition_kit::kdm::ObjectId;
use kition_kit::{Covered, CoveredAsk, CoveredAt, Covers, DerivedValues, Premise, Under, Within};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Default)]
struct Needs(BTreeMap<String, BTreeSet<String>>);

impl Needs {
    fn add(&mut self, id: &ObjectId, fields: impl IntoIterator<Item = impl Into<String>>) {
        self.0.entry(id.as_str().to_owned()).or_default().extend(fields.into_iter().map(Into::into));
    }
    fn root(&mut self, fact: &str) {
        self.add(&root_id(), [format!("~{fact}")]);
    }
    fn hierarchy(&mut self, model: &BoardModel, id: &ObjectId) {
        if let Some(page) = model.page_of(id) {
            self.add(&page.id, ["~hierarchy"]);
        }
    }
    /// An element's kind and lock, and `fields`. With `ancestry`, the kind and parent of every
    /// container above it: which of several selected elements are roots (`edit::roots`) depends
    /// on them; a lone element's container is not read (board coordinates are world coordinates).
    fn element(&mut self, model: &BoardModel, id: &ObjectId, fields: impl IntoIterator<Item = impl Into<String>>, ancestry: bool) {
        self.add(id, fields);
        self.add(id, ["kind", "locked"]);
        if !ancestry {
            return;
        }
        self.add(id, ["parent"]);
        let mut at = model.element(id).and_then(|element| element.parent.clone());
        let mut seen = BTreeSet::new();
        while let Some(parent) = at.filter(|parent| seen.insert(parent.clone())) {
            self.add(&parent, ["kind", "parent"]);
            at = model.element(&parent).and_then(|element| element.parent.clone());
        }
    }
    /// What an element's box or route reads: its own bounds; a group's members' (which members it
    /// has: the structure); a connector's ends and the elements they are bound to. With `turn`, a
    /// box's rotation too: a connector end faces the other end through it (`nearest_edge_anchor`),
    /// and a turn can keep the bounds (45 to 135 degrees).
    fn geometry(&mut self, model: &BoardModel, id: &ObjectId, turn: bool) {
        let Some(element) = model.element(id) else {
            self.add(id, ["~bounds"]);
            return;
        };
        self.add(id, ["kind"]);
        match element.kind {
            Kind::Group => {
                self.hierarchy(model, id);
                for child in model.children(id) {
                    self.geometry(model, &child.id, turn);
                }
            }
            Kind::Connector => {
                self.add(id, ["start", "end", "waypoints"]);
                for end in [&element.start, &element.end].into_iter().flatten() {
                    if let Some(binding) = &end.binding {
                        self.geometry(model, &binding.target, turn);
                    }
                }
            }
            _ => {
                self.add(id, ["~bounds", "position"]);
                if turn {
                    self.add(id, ["rotation"]);
                }
            }
        }
    }
    /// A command that changes the geometry of `ids` and what they hold: each element's kind and
    /// lock and the fields `edit` reads and writes of its kind (`boxed` of a box, `points` of a
    /// stroke, the free ends and bends of a connector), `text` more of a text element; a member's
    /// `parent` (whether it is in the subtree), a selected element's when several are (roots);
    /// with `bounds`, the selected elements' bounds.
    fn moving(&mut self, model: &BoardModel, ids: &[ObjectId], bounds: bool, boxed: &[&str], text: &[&str]) {
        let several = ids.len() > 1 && ids.iter().any(|id| model.element(id).is_some_and(|element| element.kind.can_contain()));
        let roots = crate::edit::roots(model, ids);
        for id in ids {
            if model.element(id).is_some_and(|element| element.kind.can_contain()) {
                self.hierarchy(model, id);
            }
        }
        for id in crate::edit::subtree(model, ids) {
            let selected = ids.contains(&id);
            let kind = model.element(&id).map(|element| element.kind);
            let mut fields: Vec<&str> = match kind {
                Some(Kind::Stroke) => vec!["points"],
                Some(Kind::Connector) => vec!["start.binding", "end.binding", "waypoints"],
                Some(Kind::Group) => vec![],
                _ => boxed.to_vec(),
            };
            if kind == Some(Kind::Text) {
                fields.extend(text);
            }
            if !selected {
                fields.push("parent");
            }
            self.element(model, &id, fields, selected && several);
            if let Some(element) = model.element(&id).filter(|element| element.kind == Kind::Connector) {
                for (terminal, end) in [("start", &element.start), ("end", &element.end)] {
                    if end.as_ref().is_some_and(|end| end.binding.is_none()) {
                        self.add(&id, [format!("{terminal}.point")]);
                    }
                }
            }
            if bounds && roots.contains(&id) {
                self.geometry(model, &id, false);
            }
        }
    }
    fn finish(self) -> Vec<Premise> {
        self.0
            .into_iter()
            .map(|(id, fields)| {
                let positions = fields.iter().filter(|field| matches!(field.as_str(), "position" | "parent")).cloned().collect::<Vec<_>>();
                Premise::on(id).fields(fields).positions(positions)
            })
            .collect()
    }
}

fn ids(params: &Value, key: &str) -> Vec<ObjectId> {
    match params.get(key) {
        Some(Value::String(id)) => vec![ObjectId::from_string(id)],
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(ObjectId::from_string).collect(),
        _ => Vec::new(),
    }
}

/// Values behind exact metadata member paths, without treating numeric coordinates as ids.
pub fn object_values<'a>(params: &'a Value, path: &str) -> Vec<&'a Value> {
    let mut values = vec![params];
    for segment in path.split('.') {
        let (key, many) = segment.strip_suffix("[]").map_or((segment, false), |key| (key, true));
        values = values
            .into_iter()
            .filter_map(|value| value.get(key))
            .flat_map(|value| if many { value.as_array().map(|values| values.iter().collect()).unwrap_or_default() } else { vec![value] })
            .collect();
    }
    values.into_iter().flat_map(|value| value.as_array().map_or_else(|| vec![value], |items| items.iter().collect())).collect()
}

/// Unknown families keep the SDK's conservative default. Root summaries protect dynamic
/// memberships that are not ownership (Element.parent); they deliberately hold more than one
/// container's membership, because the SDK caches derived values per changed object.
pub fn under(model: &BoardModel, active: Option<&ObjectId>, command: &str, params: &Value) -> Option<Under> {
    let mut needs = Needs::default();
    let mut within = Vec::new();
    let targets = crate::acts::targets(command).map(|key| ids(params, key)).unwrap_or_default();
    // A declared reference that names nothing now (a payload ref of another board, or one another
    // writer deleted) is left to the guard, which holds what it named at `basis`.
    for path in crate::acts::objects(command) {
        for value in object_values(params, path) {
            if let Some(id) = value.as_str().map(ObjectId::from_string).filter(|id| model.element(id).is_some() || model.page(id).is_some()) {
                needs.add(&id, std::iter::empty::<String>());
            }
        }
    }
    match command {
        "element.style" | "element.update" => {
            let key = if command == "element.style" { "style" } else { "set" };
            for id in &targets {
                let fields = params
                    .get(key)
                    .and_then(Value::as_object)
                    .into_iter()
                    .flatten()
                    .filter(|(field, _)| {
                        command == "element.update" || model.element(id).is_none_or(|element| crate::fields::uses(element.kind, field))
                    })
                    .map(|(field, _)| field.clone());
                // Property commands never read ancestry or the target's list index.
                needs.add(id, fields);
                needs.add(id, ["kind", "locked"]);
            }
            if params.get("set").and_then(|set| set.get("parent")).is_some() {
                // `check_parent`: the container's kind and page, and that it is not inside the
                // target, which only a container target can hold.
                for id in targets.iter().filter(|id| model.element(id).is_some_and(|element| element.kind.can_contain())) {
                    needs.hierarchy(model, id);
                }
                for value in object_values(params, "set.parent") {
                    if let Some(id) = value.as_str() {
                        needs.add(&ObjectId::from_string(id), ["kind"]);
                    }
                }
            }
            if params.get("set").and_then(|set| set.get("key")).is_some() {
                needs.root("keys");
            }
        }
        "element.setText" | "text.mark" | "connector.setLabel" | "connector.setWaypoints" | "card.setTarget" | "image.setAlt" | "element.lock" => {
            let fields: &[&str] = match command {
                "connector.setWaypoints" => &["kind", "locked", "waypoints"],
                "card.setTarget" => &["kind", "locked", "card"],
                "image.setAlt" => &["kind", "locked", "alt"],
                "element.lock" => &["locked"],
                "connector.setLabel" if params.get("position").is_some_and(|v| !v.is_null()) => &["kind", "locked", "text", "labelPosition"],
                _ => &["kind", "locked", "text"],
            };
            for id in &targets {
                needs.add(id, fields.iter().copied());
            }
        }
        "element.move" => {
            let reparent = params["reparent"] == true;
            needs.moving(model, &targets, params.get("to").is_some_and(|v| !v.is_null()) || reparent, &["position"], &[]);
            if reparent {
                // Each root leaves its frame (not a group) when it lands in none.
                for id in crate::edit::roots(model, &targets) {
                    needs.add(&id, ["parent"]);
                    if let Some(parent) = model.element(&id).and_then(|element| element.parent.clone()) {
                        needs.add(&parent, ["kind"]);
                    }
                }
            }
        }
        // Each root moves by what its own bounds and the others' give.
        "layout.align" | "layout.distribute" | "layout.stack" => needs.moving(model, &targets, true, &["position"], &[]),
        "element.resize" => {
            let many = params.get("ids").is_some_and(|v| !v.is_null());
            let targets = if many { ids(params, "ids") } else { targets };
            let boxed = !many && targets.first().and_then(|id| model.element(id)).is_some_and(|element| element.kind.boxed());
            if boxed {
                // One box takes the new box; what it holds stays where it is and is not read.
                for id in &targets {
                    needs.element(model, id, ["position", "size"], false);
                }
            } else {
                // The old bounds mapped to the new box: each box's place and size, a text's font
                // size when several are scaled (`resize_many`).
                needs.moving(model, &targets, true, &["position", "size"], if many { &["fontSize"] } else { &[] });
            }
        }
        "element.rotate" => {
            if params["about"] == "selection" {
                needs.moving(model, &targets, true, &["position", "size", "rotation"], &[]);
            } else {
                // Each about its own centre: a box's rotation; a stroke's points; a connector's
                // route, whose bound ends its targets give. A container turns with what it holds
                // about its centre: its bounds, and where each member is.
                needs.moving(model, &targets, false, &["rotation"], &[]);
                for id in crate::edit::roots(model, &targets) {
                    if model.element(&id).is_some_and(|element| element.kind.can_contain()) {
                        needs.geometry(model, &id, false);
                        for member in crate::edit::subtree(model, std::slice::from_ref(&id)) {
                            if model.element(&member).is_some_and(|element| element.kind.boxed()) {
                                needs.add(&member, ["position", "size"]);
                            }
                        }
                    }
                }
                for id in crate::edit::subtree(model, &targets) {
                    if model.element(&id).is_some_and(|element| element.kind == Kind::Connector) {
                        needs.geometry(model, &id, false);
                    }
                }
            }
        }
        "connector.create" | "connector.bind" => {
            // A rebind to an explicit anchor reads no box: the binding is the element and the
            // anchor, and the stored point is a cache derived from them (design note 1.4).
            let anchored = command == "connector.bind" && params["to"].get("anchor").is_some_and(|anchor| !anchor.is_null());
            for path in if command == "connector.create" { &["from.element", "to.element"][..] } else { &["to.element"][..] } {
                for value in object_values(params, path) {
                    if let Some(id) = value.as_str() {
                        if anchored {
                            needs.add(&ObjectId::from_string(id), ["kind"]);
                        } else {
                            needs.geometry(model, &ObjectId::from_string(id), true);
                        }
                    }
                }
            }
            for id in &targets {
                let terminal = params["terminal"].as_str().unwrap_or("start");
                let other = if terminal == "start" { "end" } else { "start" };
                needs.add(id, ["kind", "locked"]);
                // The end replaced: an edit of it is lost by the rebind; a refreshed cache is not.
                let bound =
                    model.element(id).and_then(|connector| if terminal == "start" { connector.start.as_ref() } else { connector.end.as_ref() });
                if anchored && params["to"].get("element").is_some() && bound.is_some_and(|end| end.binding.is_some()) {
                    needs.add(id, [format!("{terminal}.binding")]);
                } else {
                    needs.add(id, [terminal]);
                }
                if params["to"].get("element").is_some() {
                    needs.add(id, [format!("{other}.binding")]);
                    if params["to"].get("anchor").is_none()
                        && let Some(connector) = model.element(id)
                    {
                        let end = if other == "start" { &connector.start } else { &connector.end };
                        needs.add(id, [other]);
                        if let Some(binding) = end.as_ref().and_then(|end| end.binding.as_ref()) {
                            needs.geometry(model, &binding.target, true);
                        }
                    }
                }
            }
            if params.get("key").is_some() {
                needs.root("keys");
            }
        }
        "element.group" | "frame.create" => {
            let targets = if command == "frame.create" { ids(params, "children") } else { targets };
            let containers = targets.iter().any(|id| model.element(id).is_some_and(|element| element.kind.can_contain()));
            for id in &targets {
                needs.element(model, id, ["parent"], containers);
            }
        }
        "element.ungroup" => {
            // The group goes; its members take its parent.
            for id in &targets {
                needs.hierarchy(model, id);
                if let Some(element) = model.element(id) {
                    needs.add(id, element_fields(element));
                }
                for member in model.children(id) {
                    needs.add(&member.id, ["kind", "locked", "parent"]);
                }
            }
        }
        "element.delete" | "element.duplicate" | "clipboard.copy" => {
            for id in &targets {
                if model.element(id).is_some_and(|element| element.kind.can_contain()) {
                    needs.root("structure");
                }
                for member in model.with_descendants(id) {
                    // Copy/delete consumes the whole object; a property update does not.
                    let fields = model.element(&member).map(element_fields).unwrap_or_default();
                    needs.add(&member, fields);
                }
            }
        }
        "frame.fit" => {
            for id in &targets {
                needs.hierarchy(model, id);
                needs.element(model, id, ["position", "size"], false);
                for child in model.children(id) {
                    needs.add(&child.id, ["parent"]);
                    needs.geometry(model, &child.id, false);
                }
            }
        }
        "element.reorder" => {
            // `front` and `back` put the selection at an end of the page's list, in its own order;
            // `forward` and `backward` swap each with the next or previous one.
            // A container moves with what it holds (`subtree`): which elements are inside it
            // (their `parent`, the board's structure).
            let selected = targets.clone();
            let containers = selected.iter().any(|id| model.element(id).is_some_and(|element| element.kind.can_contain()));
            let targets = crate::edit::subtree(model, &selected);
            let several = targets.len() > 1;
            let step = matches!(params["placement"].as_str(), Some("forward" | "backward"));
            for id in &targets {
                needs.add(id, ["kind"]);
            }
            let mut finished = needs.finish();
            for premise in &mut finished {
                let member = targets.iter().any(|id| premise.id == json!(id.as_str()));
                let parent = member && !selected.iter().any(|id| premise.id == json!(id.as_str()));
                premise.fields = Some(if parent { vec!["parent".into()] } else { Vec::new() });
                premise.positions = Some(if several || step { vec!["elements".into()] } else { Vec::new() });
            }
            if containers && let Some(page) = selected.first().and_then(|id| model.page_of(id)) {
                finished.push(Premise::on(page.id.as_str()).fields(["~hierarchy"]).positions(std::iter::empty::<String>()));
            }
            if step && let Some(page) = targets.first().and_then(|id| model.page_of(id)) {
                for id in &targets {
                    if let Some(at) = page.elements.iter().position(|item| item == id) {
                        let (from, to) = if params["placement"] == "forward" { (at, at + 2) } else { (at.saturating_sub(1), at + 1) };
                        within.push(Within {
                            parent: json!(page.id.as_str()),
                            field: "elements".into(),
                            from,
                            to: to.min(page.elements.len()),
                            time: None,
                        });
                    }
                }
            }
            return Some(Under { premises: finished, within, ..Under::default() });
        }
        "page.rename" => {
            for id in ids(params, "page") {
                needs.add(&id, ["name"]);
            }
        }
        "page.create" | "page.move" => {
            if command == "page.create" && params.get("name").is_none() {
                needs.root("pageCount");
            }
            let key = if command == "page.create" { "at" } else { "to" };
            let at = params.get(key).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok());
            if let Some(at) = at {
                within.push(Within { parent: json!(root_id().as_str()), field: "pages".into(), from: at, to: at, time: None });
            } else if command == "page.move" {
                // One step: the page and the neighbour it passes.
                let index = ids(params, "page").first().and_then(|id| model.pages().iter().position(|page| &page.id == id));
                if let Some(index) = index {
                    let (from, to) = if params["placement"] == "previous" { (index.saturating_sub(1), index + 1) } else { (index, index + 2) };
                    within.push(Within {
                        parent: json!(root_id().as_str()),
                        field: "pages".into(),
                        from,
                        to: to.min(model.pages().len()),
                        time: None,
                    });
                }
            }
            if command == "page.move" {
                for id in ids(params, "page") {
                    needs.add(&id, std::iter::empty::<String>());
                }
            }
        }
        "page.delete" | "page.duplicate" => {
            // A delete replaces the page and everything on it; a copy reads its elements whole,
            // and its name when the copy's is not given.
            for id in ids(params, "page") {
                needs.add(&id, ["elements"]);
                if command == "page.delete" || params.get("name").is_none_or(Value::is_null) {
                    needs.add(&id, ["name"]);
                }
                if let Some(page) = model.page(&id) {
                    for element in model.elements_of(page) {
                        needs.add(&element.id, element_fields(element));
                    }
                }
            }
        }
        "element.create" | "element.paste" | "stroke.create" | "card.create" | "image.insert" | "template.insert" => {
            let page = params
                .get("page")
                .and_then(Value::as_str)
                .map(ObjectId::from_string)
                .or_else(|| active.cloned())
                .or_else(|| model.pages().first().map(|page| page.id.clone()));
            if let Some(page) = &page {
                needs.add(page, std::iter::empty::<String>());
            }
            if (matches!(command, "card.create" | "image.insert") && params.get("position").is_none()
                || command == "template.insert" && params.get("at").is_none())
                && let Some(page) = &page
            {
                needs.add(page, ["~geometry"]);
            }
            if command == "element.create" && object_values(params, "elements[].key").iter().any(|value| value.is_string()) {
                needs.root("keys");
            }
            for value in object_values(params, "elements[].parent") {
                if let Some(id) = value.as_str() {
                    needs.add(&ObjectId::from_string(id), ["kind"]);
                }
            }
            if command == "element.paste" {
                paste_sources(model, params, &mut needs);
            }
        }
        "layout.mindmap" | "mindmap.addChild" | "mindmap.addSibling" | "mindmap.collapse" | "mindmap.setDirection" => {
            // Addressable members' fields are independent of the page's owning list. Use
            // the same topology reader as Graph::of, then hold its inputs and overwritten ends.
            for id in &targets {
                if let Some(page) = model.page_of(id) {
                    needs.add(&page.id, std::iter::empty::<String>());
                    let topology = kition_board_kdm::mindmap::Topology::read(model, page);
                    let members = topology.below(&topology.root_of(id)).into_iter().collect::<BTreeSet<_>>();
                    for member in &members {
                        needs.element(
                            model,
                            member,
                            [
                                "shapeStyle",
                                "position",
                                "size",
                                "rotation",
                                "mindMapCollapsed",
                                "mindMapBranchSide",
                                "strokeColor",
                                "mindMapDirection",
                            ],
                            false,
                        );
                    }
                    // A new node and its branch take the container of the node they branch from.
                    let from = match command {
                        "mindmap.addChild" => Some(id.clone()),
                        "mindmap.addSibling" => topology.parent.get(id).cloned(),
                        _ => None,
                    };
                    if let Some(from) = from {
                        needs.add(&from, ["parent"]);
                    }
                    for edge in topology.edges.iter().filter(|edge| members.contains(&edge.parent) && members.contains(&edge.child)) {
                        needs.add(
                            &edge.connector,
                            ["kind", "locked", "start", "end", "connectorRole", "connectorType", "mindMapBranchAxis", "endArrowhead"],
                        );
                    }
                }
            }
            // The root fingerprint catches newly connected or reclassified nodes as well.
            needs.root("mapStructure");
        }
        "links.retarget" => {}
        _ => return None,
    }
    let mut under = Under { premises: needs.finish(), within, ..Under::default() };
    if command == "page.move" {
        for premise in &mut under.premises {
            if ids(params, "page").iter().any(|id| premise.id == json!(id.as_str())) {
                premise.positions = Some(vec!["pages".into()]);
            }
        }
    }
    if command == "element.move" && params["reparent"] == true {
        let frames = frame_boxes(model, &ids(params, "ids"));
        under.objects = destination_frames(model, params, &frames);
        under.covers = Some(Covers { fields: ["~frame", "~frameOrder", "parent", "kind"].map(str::to_owned).to_vec() });
        // Where a destination frame is the covered place tells; no field of it is read.
        for id in &under.objects {
            under.premises.push(Premise::on(id.clone()).fields(["kind"]).positions(std::iter::empty::<String>()));
        }
    }
    if command == "links.retarget" {
        under.objects = linked(model, params);
        under.covers = Some(Covers { fields: vec!["~link".into()] });
        for id in &under.objects {
            under.premises.push(Premise::on(id.clone()).fields(["card.target.path"]).positions(std::iter::empty::<String>()));
        }
    }
    Some(under)
}

/// What a paste reads of the board through its payload: without `at`, whether an element a ref
/// names is shown on the page (the copies then go 24 units off); a connector end bound to an
/// element outside the payload stays bound to it there, its point from that element's box. Refs
/// are held even when missing now, so a source deleted since the observation still refuses a
/// default-offset paste. Explicit placement does not read source existence.
fn paste_sources(model: &BoardModel, params: &Value, needs: &mut Needs) {
    let refs: BTreeSet<&str> = object_values(params, "payload.elements[].ref").into_iter().filter_map(Value::as_str).collect();
    if params.get("at").is_none_or(Value::is_null) {
        for reference in &refs {
            needs.add(&ObjectId::from_string(*reference), std::iter::empty::<String>());
        }
    }
    for path in ["payload.elements[].start.binding.target", "payload.elements[].end.binding.target"] {
        for target in object_values(params, path).into_iter().filter_map(Value::as_str).filter(|target| !refs.contains(target)) {
            let id = ObjectId::from_string(target);
            if model.element(&id).is_some() {
                needs.geometry(model, &id, true);
            }
        }
    }
}

fn element_fields(element: &Element) -> Vec<String> {
    let mut fields = crate::fields::UPDATABLE.iter().map(|field| (*field).to_owned()).collect::<Vec<_>>();
    fields.extend(["kind", "locked", "text", "start", "end", "waypoints", "card", "image", "alt"].map(str::to_owned));
    if element.kind == Kind::Connector {
        fields.push("~bounds".into());
    }
    fields
}

fn fingerprint(value: &Value) -> Value {
    json!(format!("{:x}", Sha256::digest(value.to_string().as_bytes())))
}

/// Board and Page summaries read what they own. An Element's bounds use only its own fields:
/// groups and bound connectors are deliberately null.
pub fn derived(model: &BoardModel, ids: &[Value], names: &[String]) -> Vec<DerivedValues> {
    let all = if ids.is_empty() {
        std::iter::once(json!(root_id().as_str()))
            .chain(model.pages().iter().map(|page| json!(page.id.as_str())))
            .chain(model.pages().iter().flat_map(|page| model.elements_of(page)).map(|element| json!(element.id.as_str())))
            .collect::<Vec<_>>()
    } else {
        ids.to_vec()
    };
    all.iter()
        .filter_map(|id| {
            let object = id.as_str().map(ObjectId::from_string)?;
            let mut values = serde_json::Map::new();
            let wanted = |name: &str| names.is_empty() || names.iter().any(|n| n == name);
            if object == root_id() {
                if wanted("pageCount") {
                    values.insert("pageCount".into(), json!(model.pages().len()));
                }
                if wanted("frameOrder") {
                    values.insert("frameOrder".into(), json!({"ids":frame_order(model)}));
                }
                if wanted("mapStructure") {
                    let maps = model
                        .pages()
                        .iter()
                        .filter_map(|page| {
                            let topology = kition_board_kdm::mindmap::Topology::read(model, page);
                            if topology.nodes.is_empty() {
                                return None;
                            }
                            let mut nodes = topology.nodes.iter().map(ObjectId::as_str).collect::<Vec<_>>();
                            nodes.sort_unstable();
                            Some(json!([
                                page.id.as_str(),
                                nodes,
                                topology
                                    .edges
                                    .iter()
                                    .map(|edge| { json!([edge.connector.as_str(), edge.parent.as_str(), edge.child.as_str()]) })
                                    .collect::<Vec<_>>()
                            ]))
                        })
                        .collect::<Vec<_>>();
                    values.insert("mapStructure".into(), fingerprint(&json!(maps)));
                }
                for name in ["structure", "keys"] {
                    if !wanted(name) {
                        continue;
                    }
                    let pages = model
                        .pages()
                        .iter()
                        .map(|page| {
                            let mut elements = model
                                .elements_of(page)
                                .map(|element| match name {
                                    "structure"
                                        if element.parent.is_some()
                                            || kition_board_kdm::mindmap::is_node(element)
                                            || [&element.start, &element.end].into_iter().flatten().any(|end| end.binding.is_some()) =>
                                    {
                                        json!([
                                            element.id.as_str(),
                                            element.kind.as_str(),
                                            kition_board_kdm::mindmap::is_node(element),
                                            element.parent.as_ref().map(ObjectId::as_str),
                                            element.start.as_ref().and_then(|end| end.binding.as_ref()).map(|binding| binding.target.as_str()),
                                            element.end.as_ref().and_then(|end| end.binding.as_ref()).map(|binding| binding.target.as_str())
                                        ])
                                    }
                                    "structure" => Value::Null,
                                    "keys" => element.key.as_ref().map_or(Value::Null, |key| json!([element.id.as_str(), key])),
                                    _ => json!([element.id.as_str(), model.bounds(element).map(|rect| rect.to_json())]),
                                })
                                .filter(|value| !value.is_null())
                                .collect::<Vec<_>>();
                            if name == "keys" {
                                elements.sort_by_cached_key(Value::to_string);
                            }
                            if elements.is_empty() { Value::Null } else { json!([page.id.as_str(), elements]) }
                        })
                        .filter(|value| !value.is_null())
                        .collect::<Vec<_>>();
                    values.insert(name.into(), fingerprint(&json!(pages)));
                }
            } else if let Some(page) = model.page(&object) {
                if wanted("hierarchy") {
                    let mut parents = model
                        .elements_of(page)
                        .filter_map(|element| element.parent.as_ref().map(|parent| json!([element.id.as_str(), parent.as_str()])))
                        .collect::<Vec<_>>();
                    parents.sort_by_cached_key(Value::to_string);
                    values.insert("hierarchy".into(), fingerprint(&json!(parents)));
                }
                if wanted("geometry") {
                    values.insert("geometry".into(), model.content_bounds(page).map_or(Value::Null, |bounds| bounds.to_json()));
                }
            } else {
                let element = model.element(&object)?;
                if wanted("bounds") {
                    values.insert(
                        "bounds".into(),
                        if matches!(element.kind, Kind::Group | Kind::Connector) {
                            Value::Null
                        } else {
                            model.bounds(element).map_or(Value::Null, |bounds| bounds.to_json())
                        },
                    );
                }
                if wanted("frame") {
                    values.insert(
                        "frame".into(),
                        if element.kind == Kind::Frame { model.rect(element).map_or(Value::Null, |bounds| bounds.to_json()) } else { Value::Null },
                    );
                }
                if wanted("link") {
                    values.insert(
                        "link".into(),
                        if element.kind == Kind::Card {
                            element.card.as_ref().and_then(|card| card.pointer("/target/path")).cloned().unwrap_or(Value::Null)
                        } else {
                            Value::Null
                        },
                    );
                }
            }
            Some(DerivedValues { id: id.clone(), values })
        })
        .collect()
}

fn frame_order(model: &BoardModel) -> Vec<String> {
    model
        .pages()
        .iter()
        .flat_map(|page| model.elements_of(page))
        .filter(|element| element.kind == Kind::Frame)
        .map(|element| element.id.as_str().to_owned())
        .collect()
}

fn matches_link(params: &Value, path: &str) -> bool {
    params.get("from").and_then(Value::as_str).is_some_and(|from| path == from.trim() || path.starts_with(&format!("{}/", from.trim())))
}

fn linked(model: &BoardModel, params: &Value) -> Vec<Value> {
    model
        .pages()
        .iter()
        .flat_map(|page| model.elements_of(page))
        .filter(|element| element.kind == Kind::Card)
        .filter(|element| {
            element.card.as_ref().and_then(|card| card.pointer("/target/path")).and_then(Value::as_str).is_some_and(|path| matches_link(params, path))
        })
        .map(|element| json!(element.id.as_str()))
        .collect()
}

fn covered_links(model: &BoardModel, params: &Value, objects: &[CoveredAsk]) -> CoveredAt {
    let mut under = linked(model, params);
    let covered = objects
        .iter()
        .map(|ask| {
            under.retain(|id| id != &ask.id);
            let was = if ask.created {
                Some(false)
            } else if ask.unknown.iter().any(|field| matches!(field.as_str(), "~link" | "$was")) {
                None
            } else if let Some(path) = ask.before.get("~link") {
                Some(path.as_str().is_some_and(|path| matches_link(params, path)))
            } else if ask.deleted {
                None
            } else {
                Some(
                    ask.id
                        .as_str()
                        .and_then(|id| model.element(&ObjectId::from_string(id)))
                        .and_then(|element| element.card.as_ref())
                        .and_then(|card| card.pointer("/target/path"))
                        .and_then(Value::as_str)
                        .is_some_and(|path| matches_link(params, path)),
                )
            };
            if was == Some(true) {
                under.push(ask.id.clone());
            }
            Covered { id: ask.id.clone(), covered: was, ..Default::default() }
        })
        .collect();
    CoveredAt { under, covered }
}

fn frame_boxes(model: &BoardModel, ids: &[ObjectId]) -> Vec<(ObjectId, Rect)> {
    ids.first()
        .and_then(|id| model.page_of(id))
        .into_iter()
        .flat_map(|page| model.elements_of(page))
        .filter(|element| element.kind == Kind::Frame)
        .filter_map(|element| Some((element.id.clone(), model.rect(element)?)))
        .collect()
}

fn destination_points(model: &BoardModel, params: &Value) -> Vec<Point> {
    let ids = ids(params, "ids");
    let delta = crate::params::opt_point(params, "delta")
        .ok()
        .flatten()
        .or_else(|| {
            let to = crate::params::opt_point(params, "to").ok().flatten()?;
            let bounds = ids.iter().filter_map(|id| model.element(id)).filter_map(|element| model.bounds(element)).reduce(Rect::union)?;
            Some(Point::new(to.x - bounds.x0, to.y - bounds.y0))
        })
        .unwrap_or_default();
    crate::edit::roots(model, &ids).iter().filter_map(|id| Some(model.bounds(model.element(id)?)?.center().offset(delta.x, delta.y))).collect()
}

fn destination_frames(model: &BoardModel, params: &Value, frames: &[(ObjectId, Rect)]) -> Vec<Value> {
    let ids = ids(params, "ids");
    let moved = crate::edit::subtree(model, &crate::edit::roots(model, &ids));
    let excluding = moved.iter().collect();
    destination_points(model, params)
        .into_iter()
        .filter_map(|center| {
            crate::elements::smallest_frame(frames.iter().map(|(id, rect)| (id, *rect)), center, &excluding).map(|id| id.as_str().to_owned())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(Value::String)
        .collect()
}

/// Restore every asked frame rectangle together. Ordinary target movement is separately held
/// by the target's positional premise. Deleted frames have no surviving page identity in the
/// SDK's replay request, so consider their old rectangle conservatively on this page.
pub fn covered(model: &BoardModel, command: &str, params: &Value, objects: &[CoveredAsk]) -> Option<CoveredAt> {
    if command == "links.retarget" {
        return Some(covered_links(model, params, objects));
    }
    if command != "element.move" || params["reparent"] != true {
        return None;
    }
    let ids = ids(params, "ids");
    let mut frames = frame_boxes(model, &ids);
    let page = ids.first().and_then(|id| model.page_of(id));
    let mut unknown = BTreeSet::new();
    for ask in objects {
        let Some(text) = ask.id.as_str() else { continue };
        let id = ObjectId::from_string(text);
        let live = model.element(&id);
        if ask.created {
            frames.retain(|(frame, _)| frame != &id);
            continue;
        }
        let on_page = model.page_of(&id).zip(page).is_some_and(|(a, b)| a.id == b.id);
        let is_frame = live.is_some_and(|element| element.kind == Kind::Frame) || ask.before.get("~frame").is_some_and(|value| !value.is_null());
        if !ask.deleted && !on_page {
            continue;
        }
        // `kind` never changes after creation: a deleted object that was no frame at basis was no
        // destination, whether or not the journal kept its (null) `~frame`.
        let frame_at_basis = ask.before.get("kind").and_then(Value::as_str).is_none_or(|kind| kind == "frame");
        if ask.unknown.iter().any(|field| matches!(field.as_str(), "~frame" | "$was" | "kind"))
            || ask.deleted && frame_at_basis && !ask.before.contains_key("~frame")
        {
            unknown.insert(text.to_owned());
            continue;
        }
        if !is_frame {
            continue;
        }
        if let Some(before) = ask.before.get("~frame") {
            if before.is_null() {
                frames.retain(|(frame, _)| frame != &id);
            } else if let Ok(rect) = crate::params::rect(before, "~frame") {
                if let Some((_, current)) = frames.iter_mut().find(|(frame, _)| frame == &id) {
                    *current = rect;
                } else {
                    frames.push((id, rect));
                }
            } else {
                unknown.insert(text.to_owned());
            }
        }
    }
    // The root owns the lists: retain their frame order as a derived value, since a
    // CoveredAsk has no old list index. Large omitted values keep the conservative tie refusal.
    let root = objects.iter().find(|ask| ask.id.as_str() == Some(root_id().as_str()));
    let order = root
        .and_then(|ask| ask.before.get("~frameOrder"))
        .and_then(|value| value.get("ids"))
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(str::to_owned).collect::<Vec<_>>())
        .unwrap_or_else(|| frame_order(model));
    let order_unknown = root.is_some_and(|ask| ask.unknown.iter().any(|field| field == "~frameOrder"));
    frames.sort_by_key(|(id, _)| order.iter().position(|item| item == id.as_str()).unwrap_or(usize::MAX));
    for ask in objects.iter().filter(|ask| order_unknown && !ask.created) {
        if let Some((_, rect)) = frames.iter().find(|(id, _)| ask.id.as_str() == Some(id.as_str())) {
            for point in destination_points(model, params) {
                if rect.contains(point)
                    && frames
                        .iter()
                        .filter(|(_, other)| other.contains(point) && other.width() * other.height() == rect.width() * rect.height())
                        .count()
                        > 1
                    && let Some(id) = ask.id.as_str()
                {
                    unknown.insert(id.to_owned());
                }
            }
        }
    }
    let under = destination_frames(model, params, &frames);
    let covered = objects
        .iter()
        .map(|ask| Covered {
            id: ask.id.clone(),
            covered: if ask.id.as_str().is_some_and(|id| unknown.contains(id)) { None } else { Some(under.contains(&ask.id)) },
            ..Default::default()
        })
        .collect();
    Some(CoveredAt { under, covered })
}
