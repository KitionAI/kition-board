//! Mind-map commands (design note 5.1): `mindmap.addChild`, `mindmap.addSibling`,
//! `mindmap.collapse`, `mindmap.setDirection` and `layout.mindmap`. Each one lays the whole map
//! out again in the same transaction (one undo step): nodes placed in columns (or rows, growing
//! down), branches bound to the facing sides of their nodes and drawn as mind-map curves.
//!
//! Ported from Kition's `packages/board-editor/src/lib/whiteboardMindMap.ts`
//! (`getWhiteboardMindMapGraph`, `layoutWhiteboardMindMap`, `getWhiteboardMindMapNodePlacement`,
//! `getWhiteboardMindMapBranchSide`) and `hooks/useWhiteboardMindMapActions.ts` (`addMindMapNode`,
//! `toggleMindMapCollapsed`, `setMindMapDirection`, `updateMindMapConnectorTerminals`) as engine
//! commands.

use std::collections::{BTreeSet, HashMap};

use kition_board_kdm::geometry::anchor_point;
use kition_board_kdm::mindmap::{Edge, Topology, collapsed, is_node};
use kition_board_kdm::{Binding, BoardModel, ELEMENT, End, PageView, Point, Rect, Size};
use kition_kit::kdm::{ObjectId, Tx};
use serde_json::{Map, Value, json};

use crate::edit;
use crate::fields::MIND_MAP_DIRECTIONS;
use crate::params::{self, Result, invalid};

const HORIZONTAL_DEPTH_GAP: f64 = 250.0;
const HORIZONTAL_LEAF_GAP: f64 = 96.0;
const VERTICAL_DEPTH_GAP: f64 = 150.0;
const VERTICAL_LEAF_GAP: f64 = 210.0;
/// A new node's size.
const NODE_SIZE: Size = Size { w: 142.0, h: 56.0 };
/// The colours a root's branches take in turn.
const BRANCH_COLORS: [&str; 5] = ["purple", "blue", "green", "orange", "red"];

/// One node as the layout sees it.
#[derive(Clone, Debug)]
struct Node {
    rect: Rect,
    rotation: f64,
    collapsed: bool,
    side: Option<String>,
    stroke: Option<String>,
    direction: Option<String>,
}

impl Node {
    fn center(&self) -> Point {
        self.rect.center()
    }
}

/// One mind map: its root, nodes (breadth first), branches and current direction.
struct Graph {
    root: ObjectId,
    order: Vec<ObjectId>,
    nodes: HashMap<ObjectId, Node>,
    children: HashMap<ObjectId, Vec<ObjectId>>,
    parent: HashMap<ObjectId, ObjectId>,
    edges: Vec<Edge>,
    direction: String,
}

fn text_of(element: &kition_board_kdm::Element, field: &str) -> Option<String> {
    element.register(field).and_then(Value::as_str).map(str::to_owned)
}

impl Graph {
    /// The map `node` is in (v1's `getWhiteboardMindMapGraph`).
    fn of(model: &BoardModel, page: &PageView, node: &ObjectId) -> Result<Graph> {
        let element = params::element(model, node)?;
        if !is_node(element) {
            return Err(invalid(format!("{} is not a mind-map node (a shape with shapeStyle mind-node)", node.as_str())));
        }
        let topology = Topology::read(model, page);
        let root = topology.root_of(node);
        let order = topology.below(&root);
        let mut nodes = HashMap::new();
        for id in &order {
            let element = params::element(model, id)?;
            if element.locked {
                return Err(params::locked(id));
            }
            let rect = model.rect(element).ok_or_else(|| invalid(format!("{} has no box", id.as_str())))?;
            nodes.insert(
                id.clone(),
                Node {
                    rect,
                    rotation: element.rotation,
                    collapsed: collapsed(element),
                    side: text_of(element, "mindMapBranchSide"),
                    stroke: text_of(element, "strokeColor"),
                    direction: text_of(element, "mindMapDirection"),
                },
            );
        }
        let members: std::collections::HashSet<&ObjectId> = order.iter().collect();
        let edges: Vec<Edge> =
            topology.edges.iter().filter(|edge| members.contains(&edge.parent) && members.contains(&edge.child)).cloned().collect();
        // The layout rewrites every branch of the map (its ends, curve and settings).
        if let Some(edge) = edges.iter().find(|edge| model.element(&edge.connector).is_some_and(|connector| connector.locked)) {
            return Err(params::locked(&edge.connector));
        }
        let children = topology.children.iter().filter(|(id, _)| members.contains(id)).map(|(id, kids)| (id.clone(), kids.clone())).collect();
        let parent = topology.parent.iter().filter(|(id, _)| members.contains(id)).map(|(id, up)| (id.clone(), up.clone())).collect();
        let mut graph = Graph { root, order, nodes, children, parent, edges, direction: String::new() };
        graph.direction = graph.nodes.get(&graph.root).and_then(|root| root.direction.clone()).unwrap_or_else(|| graph.infer_direction());
        Ok(graph)
    }

    /// v1's `inferMindMapDirection`: where the other nodes are around the root.
    fn infer_direction(&self) -> String {
        let Some(root) = self.nodes.get(&self.root) else { return "right".into() };
        let center = root.center();
        let others: Vec<Point> = self.order.iter().filter(|id| **id != self.root).filter_map(|id| self.nodes.get(id)).map(Node::center).collect();
        if others.is_empty() {
            return "right".into();
        }
        let left = others.iter().filter(|p| p.x < center.x - 20.0).count();
        let right = others.iter().filter(|p| p.x > center.x + 20.0).count();
        let below = others.iter().filter(|p| p.y > center.y + 20.0).count();
        if left > 0 && right > 0 {
            "both".into()
        } else if below > left.max(right) {
            "down".into()
        } else if left > right {
            "left".into()
        } else {
            "right".into()
        }
    }

    fn kids(&self, id: &ObjectId) -> &[ObjectId] {
        self.children.get(id).map_or(&[], Vec::as_slice)
    }

    /// Children the layout places: none under a collapsed node that is not the root.
    fn visible_kids(&self, id: &ObjectId) -> Vec<ObjectId> {
        if *id != self.root && self.nodes.get(id).is_some_and(|node| node.collapsed) {
            return Vec::new();
        }
        self.kids(id).to_vec()
    }

    /// The side of the root a node's branch grows on (v1's `getWhiteboardMindMapBranchSide`).
    fn branch_side(&self, id: &ObjectId) -> Option<String> {
        if *id == self.root {
            return None;
        }
        let mut first = id.clone();
        let mut steps = 0;
        while let Some(parent) = self.parent.get(&first) {
            if *parent == self.root || steps > self.order.len() {
                break;
            }
            first = parent.clone();
            steps += 1;
        }
        let node = self.nodes.get(&first)?;
        if let Some(side) = &node.side {
            return Some(side.clone());
        }
        let root = self.nodes.get(&self.root)?;
        Some(if node.center().x < root.center().x { "left" } else { "right" }.into())
    }

    /// The side a new child of the root goes on when the map grows both ways.
    fn balanced_side(&self) -> String {
        let Some(root) = self.nodes.get(&self.root) else { return "right".into() };
        let (mut left, mut right) = (0, 0);
        for kid in self.kids(&self.root) {
            let side = self.nodes.get(kid).and_then(|node| node.side.clone()).unwrap_or_else(|| {
                if self.nodes.get(kid).is_some_and(|node| node.center().x < root.center().x) { "left".into() } else { "right".into() }
            });
            if side == "left" {
                left += 1;
            } else {
                right += 1;
            }
        }
        if left <= right { "left".into() } else { "right".into() }
    }

    /// New centres for `direction` (v1's `layoutWhiteboardMindMap`), and the first-level sides
    /// when the map grows both ways. Hidden nodes keep where they are.
    fn layout(&self, direction: &str) -> (HashMap<ObjectId, Point>, HashMap<ObjectId, String>) {
        let mut centers = HashMap::new();
        let mut sides = HashMap::new();
        let Some(root) = self.nodes.get(&self.root) else { return (centers, sides) };
        let origin = root.center();
        match direction {
            "both" => {
                centers.insert(self.root.clone(), origin);
                let (mut left, mut right) = (Vec::new(), Vec::new());
                for kid in self.visible_kids(&self.root) {
                    let node = self.nodes.get(&kid);
                    let existing = node.and_then(|node| node.side.clone()).or_else(|| {
                        (self.direction == "both")
                            .then(|| node.map_or("right", |node| if node.center().x < origin.x { "left" } else { "right" }).to_owned())
                    });
                    let side = existing.unwrap_or_else(|| if left.len() <= right.len() { "left".into() } else { "right".into() });
                    sides.insert(kid.clone(), side.clone());
                    if side == "left" { left.push(kid) } else { right.push(kid) }
                }
                for (kids, sign) in [(left, -1.0), (right, 1.0)] {
                    if kids.is_empty() {
                        continue;
                    }
                    let mut side_centers = HashMap::new();
                    let mut leaf = 0.0;
                    for kid in &kids {
                        self.place(
                            kid,
                            1.0,
                            &mut leaf,
                            &mut side_centers,
                            &|depth, along| Point::new(origin.x + sign * depth * HORIZONTAL_DEPTH_GAP, along),
                            HORIZONTAL_LEAF_GAP,
                        );
                    }
                    let ys: Vec<f64> = side_centers.values().map(|p: &Point| p.y).collect();
                    let middle = (ys.iter().copied().fold(f64::INFINITY, f64::min) + ys.iter().copied().fold(f64::NEG_INFINITY, f64::max)) / 2.0;
                    for (id, p) in side_centers {
                        centers.insert(id, Point::new(p.x, p.y + origin.y - middle));
                    }
                }
            }
            "down" => {
                let mut leaf = 0.0;
                let along = self.place(
                    &self.root,
                    0.0,
                    &mut leaf,
                    &mut centers,
                    &|depth, along| Point::new(along, origin.y + depth * VERTICAL_DEPTH_GAP),
                    VERTICAL_LEAF_GAP,
                );
                for p in centers.values_mut() {
                    p.x += origin.x - along;
                }
            }
            _ => {
                let sign = if direction == "left" { -1.0 } else { 1.0 };
                let mut leaf = 0.0;
                let along = self.place(
                    &self.root,
                    0.0,
                    &mut leaf,
                    &mut centers,
                    &|depth, along| Point::new(origin.x + sign * depth * HORIZONTAL_DEPTH_GAP, along),
                    HORIZONTAL_LEAF_GAP,
                );
                for p in centers.values_mut() {
                    p.y += origin.y - along;
                }
            }
        }
        (centers, sides)
    }

    /// Places `id` at `depth`: leaves one gap apart along the cross axis, a parent at its
    /// children's mean. Answers its cross-axis coordinate.
    fn place(
        &self,
        id: &ObjectId,
        depth: f64,
        leaf: &mut f64,
        centers: &mut HashMap<ObjectId, Point>,
        at: &dyn Fn(f64, f64) -> Point,
        gap: f64,
    ) -> f64 {
        if centers.len() > self.order.len() || depth > self.order.len() as f64 {
            return 0.0;
        }
        let kids = self.visible_kids(id);
        let along = if kids.is_empty() {
            let value = *leaf * gap;
            *leaf += 1.0;
            value
        } else {
            let sum: f64 = kids.iter().map(|kid| self.place(kid, depth + 1.0, leaf, centers, at, gap)).sum();
            sum / kids.len() as f64
        };
        centers.insert(id.clone(), at(depth, along));
        along
    }
}

/// The anchors a branch takes on its parent and child (v1's `getMindMapConnectorAnchors`).
fn branch_anchors(direction: &str, opens_left: bool) -> ([f64; 2], [f64; 2]) {
    if direction == "down" {
        ([0.5, 1.0], [0.5, 0.0])
    } else if direction == "left" || (direction == "both" && opens_left) {
        ([0.0, 0.5], [1.0, 0.5])
    } else {
        ([1.0, 0.5], [0.0, 0.5])
    }
}

fn axis(direction: &str) -> &'static str {
    if direction == "down" { "vertical" } else { "horizontal" }
}

/// Sets `field` to `value` when it differs from what the element has (`None`: removes it).
fn put(tx: &mut Tx<'_>, model: &BoardModel, id: &ObjectId, field: &str, value: Option<Value>) -> Result<bool> {
    let current = model.element(id).and_then(|element| element.register(field)).filter(|value| !value.is_null());
    if current == value.as_ref() {
        return Ok(false);
    }
    tx.set(id, field, value.unwrap_or(Value::Null))?;
    Ok(true)
}

/// Lays `graph` out for `direction` and writes it: node positions, the root's direction, the
/// first-level sides, and every branch's look and bound ends. Answers the ids written.
fn apply(model: &BoardModel, tx: &mut Tx<'_>, page: &PageView, graph: &Graph, direction: &str) -> Result<Vec<ObjectId>> {
    let (centers, sides) = graph.layout(direction);
    let mut touched = BTreeSet::new();
    let mut rects = HashMap::new();
    for id in &graph.order {
        let Some(node) = graph.nodes.get(id) else { continue };
        let center = centers.get(id).copied().unwrap_or_else(|| node.center());
        let (w, h) = (node.rect.width(), node.rect.height());
        let rect = Rect::new(center.x - w / 2.0, center.y - h / 2.0, center.x + w / 2.0, center.y + h / 2.0);
        if (rect.x0 - node.rect.x0).abs() > 1e-9 || (rect.y0 - node.rect.y0).abs() > 1e-9 {
            tx.set(id, "position", Point::new(rect.x0, rect.y0).to_object())?;
            touched.insert(id.clone());
        }
        rects.insert(id.clone(), (rect, node.rotation));
        let first_level = graph.parent.get(id) == Some(&graph.root);
        if direction == "both"
            && first_level
            && let Some(side) = sides.get(id)
        {
            touched.extend(put(tx, model, id, "mindMapBranchSide", Some(json!(side)))?.then(|| id.clone()));
        }
        if *id == graph.root {
            touched.extend(put(tx, model, id, "mindMapDirection", Some(json!(direction)))?.then(|| id.clone()));
            touched.extend(put(tx, model, id, "mindMapCollapsed", None)?.then(|| id.clone()));
        } else {
            touched.extend(put(tx, model, id, "mindMapDirection", None)?.then(|| id.clone()));
        }
    }
    for edge in &graph.edges {
        let (Some(&(parent, parent_turn)), Some(&(child, child_turn))) = (rects.get(&edge.parent), rects.get(&edge.child)) else { continue };
        let connector = &edge.connector;
        let mut changed = false;
        for (field, value) in
            [("connectorRole", "mind-map-branch"), ("connectorType", "curved"), ("mindMapBranchAxis", axis(direction)), ("endArrowhead", "none")]
        {
            changed |= put(tx, model, connector, field, Some(json!(value)))?;
        }
        let (start, end) = branch_anchors(direction, child.center().x < parent.center().x);
        for (terminal, target, rect, turn, anchor) in
            [("start", &edge.parent, parent, parent_turn, start), ("end", &edge.child, child, child_turn, end)]
        {
            let value = End { point: anchor_point(rect, turn, anchor), binding: Some(Binding { target: target.clone(), anchor }) }.to_json();
            changed |= put(tx, model, connector, terminal, Some(value))?;
        }
        if changed {
            touched.insert(connector.clone());
        }
    }
    // Other connectors bound to moved nodes follow them.
    edit::refresh_bound_ends(model, tx, page, &touched)?;
    Ok(touched.into_iter().collect())
}

fn node_param(params: &Value) -> Result<ObjectId> {
    params::id(params, "node")
}

fn page_of<'a>(model: &'a BoardModel, id: &ObjectId) -> Result<&'a PageView> {
    model.page_of(id).ok_or_else(|| params::not_found(format!("no element {}", id.as_str())))
}

/// Adds a child of `parent` on `side` (or the side the map gives it), lays the map out, and
/// answers the new node and branch.
fn add(model: &BoardModel, tx: &mut Tx<'_>, parent: &ObjectId, text: &str, side: Option<&str>) -> Result<Value> {
    let page = page_of(model, parent)?;
    let mut graph = Graph::of(model, page, parent)?;
    let Some(parent_node) = graph.nodes.get(parent).cloned() else { return Err(invalid("the parent is not in its map")) };
    // v1's `getWhiteboardMindMapNodePlacement`: the side, then the layout places the node.
    let side = match graph.direction.as_str() {
        "left" => "left".to_owned(),
        "right" => "right".to_owned(),
        "down" => side.map_or_else(|| graph.branch_side(parent).unwrap_or_else(|| "right".into()), str::to_owned),
        _ if *parent == graph.root && side.is_none() => graph.balanced_side(),
        _ => side.map_or_else(|| graph.branch_side(parent).unwrap_or_else(|| "right".into()), str::to_owned),
    };
    let color = if *parent == graph.root {
        BRANCH_COLORS.get(graph.kids(&graph.root).len() % BRANCH_COLORS.len()).copied().unwrap_or("purple").to_owned()
    } else {
        parent_node.stroke.clone().unwrap_or_else(|| "purple".into())
    };
    if parent_node.collapsed && *parent != graph.root {
        tx.set(parent, "mindMapCollapsed", Value::Null)?;
        if let Some(node) = graph.nodes.get_mut(parent) {
            node.collapsed = false;
        }
    }
    let center = parent_node.center();
    let offset = if side == "left" { -HORIZONTAL_DEPTH_GAP } else { HORIZONTAL_DEPTH_GAP };
    let start = Point::new(center.x + offset - NODE_SIZE.w / 2.0, center.y - NODE_SIZE.h / 2.0);
    let frame = model.element(parent).and_then(|element| element.parent.clone());
    let mut fields = Map::new();
    fields.insert("kind".into(), json!("shape"));
    fields.insert("shapeStyle".into(), json!("mind-node"));
    fields.insert("shapeType".into(), json!("rectangle"));
    fields.insert("position".into(), start.to_object());
    fields.insert("size".into(), NODE_SIZE.to_json());
    for (field, value) in
        [("dashStyle", "solid"), ("fillColor", "white"), ("fillStyle", "solid"), ("strokeColor", color.as_str()), ("strokeSize", "s")]
    {
        fields.insert(field.into(), json!(value));
    }
    let both_root = *parent == graph.root && graph.direction == "both";
    if both_root {
        fields.insert("mindMapBranchSide".into(), json!(side));
    }
    if let Some(frame) = &frame {
        fields.insert("parent".into(), json!(frame.as_str()));
    }
    let node = tx.create(ELEMENT, fields)?;
    if !text.is_empty() {
        tx.text_insert(&node, "text", 0, text)?;
    }
    let mut branch = Map::new();
    branch.insert("kind".into(), json!("connector"));
    let free = End::free(center).to_json();
    branch.insert("start".into(), free.clone());
    branch.insert("end".into(), free);
    for (field, value) in [
        ("connectorRole", "mind-map-branch"),
        ("connectorType", "curved"),
        ("mindMapBranchAxis", axis(&graph.direction)),
        ("endArrowhead", "none"),
        ("strokeColor", color.as_str()),
        ("strokeSize", "s"),
    ] {
        branch.insert(field.into(), json!(value));
    }
    if let Some(frame) = &frame {
        branch.insert("parent".into(), json!(frame.as_str()));
    }
    let connector = tx.create(ELEMENT, branch)?;
    let mut list = edit::Appender::new(tx, &page.id)?;
    list.push(tx, &connector)?;
    list.push(tx, &node)?;
    // The new node joins the map in memory; the layout writes its place and its branch.
    graph.nodes.insert(
        node.clone(),
        Node {
            rect: Rect::new(start.x, start.y, start.x + NODE_SIZE.w, start.y + NODE_SIZE.h),
            rotation: 0.0,
            collapsed: false,
            side: both_root.then(|| side.clone()),
            stroke: Some(color),
            direction: None,
        },
    );
    graph.children.entry(parent.clone()).or_default().push(node.clone());
    graph.parent.insert(node.clone(), parent.clone());
    graph.edges.push(Edge { connector: connector.clone(), parent: parent.clone(), child: node.clone() });
    graph.order.push(node.clone());
    let direction = graph.direction.clone();
    let updated = apply(model, tx, page, &graph, &direction)?;
    let updated: Vec<ObjectId> = updated.into_iter().filter(|id| *id != node && *id != connector).collect();
    Ok(json!({ "created": [connector.as_str(), node.as_str()], "node": node.as_str(), "updated": edit::id_list(&updated) }))
}

/// `mindmap.addChild { node, text?, side? }`.
pub fn add_child(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let node = node_param(params)?;
    let side = params::opt_string(params, "side")?;
    if side.is_some_and(|side| side != "left" && side != "right") {
        return Err(invalid("`side` must be left or right"));
    }
    add(model, tx, &node, params::opt_string(params, "text")?.unwrap_or_default(), side)
}

/// `mindmap.addSibling { node, text? }`: a child of the node's parent, on the node's side.
pub fn add_sibling(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let node = node_param(params)?;
    let page = page_of(model, &node)?;
    let graph = Graph::of(model, page, &node)?;
    let parent = graph.parent.get(&node).cloned().ok_or_else(|| invalid("the root of a mind map has no siblings"))?;
    let side = graph.branch_side(&node);
    add(model, tx, &parent, params::opt_string(params, "text")?.unwrap_or_default(), side.as_deref())
}

/// `mindmap.collapse { node, collapsed? }`: hides (or shows) what is below a node; without
/// `collapsed` it toggles.
pub fn collapse(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let node = node_param(params)?;
    let page = page_of(model, &node)?;
    let mut graph = Graph::of(model, page, &node)?;
    if node == graph.root {
        return Err(invalid("the root of a mind map does not collapse"));
    }
    if graph.kids(&node).is_empty() {
        return Err(invalid("the node has no branches to collapse"));
    }
    let now = graph.nodes.get(&node).is_some_and(|found| found.collapsed);
    let next = params::opt_bool(params, "collapsed")?.unwrap_or(!now);
    tx.set(&node, "mindMapCollapsed", if next { json!(true) } else { Value::Null })?;
    if let Some(found) = graph.nodes.get_mut(&node) {
        found.collapsed = next;
    }
    let direction = graph.direction.clone();
    let mut updated = apply(model, tx, page, &graph, &direction)?;
    if !updated.contains(&node) {
        updated.push(node.clone());
    }
    Ok(json!({ "updated": edit::id_list(&updated), "collapsed": next }))
}

/// `mindmap.setDirection { node, direction }`: the map grows right, left, both ways or down.
pub fn set_direction(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let node = node_param(params)?;
    let direction = params::string(params, "direction")?;
    if !MIND_MAP_DIRECTIONS.contains(&direction) {
        return Err(invalid(format!("`direction` must be one of {}", MIND_MAP_DIRECTIONS.join(", "))));
    }
    let page = page_of(model, &node)?;
    let graph = Graph::of(model, page, &node)?;
    let updated = apply(model, tx, page, &graph, direction)?;
    Ok(json!({ "updated": edit::id_list(&updated), "root": graph.root.as_str() }))
}

/// `layout.mindmap { root }`: lays a map out again in its direction (`root` may be any node).
pub fn layout(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let node = params::id(params, "root")?;
    let page = page_of(model, &node)?;
    let graph = Graph::of(model, page, &node)?;
    let direction = graph.direction.clone();
    let updated = apply(model, tx, page, &graph, &direction)?;
    Ok(json!({ "updated": edit::id_list(&updated), "root": graph.root.as_str() }))
}
