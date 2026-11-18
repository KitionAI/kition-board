//! Mind maps as the view reads them: which shapes are nodes, which connectors are their branches,
//! and what a collapsed node hides.
//!
//! A node is a `shape` with `shapeStyle: mind-node`. A connector whose start is bound to one node
//! and whose end is bound to another makes the second the first's child; a node keeps the first
//! such parent in page order. A collapsed node (`mindMapCollapsed`) that is not a root hides its
//! descendants and the branches to them.
//!
//! Ported from Kition's `packages/board-editor/src/lib/whiteboardMindMap.ts`
//! (`buildWhiteboardMindMapTopology`, `getWhiteboardMindMapPresentation`) for schema v2.

use std::collections::{HashMap, HashSet, VecDeque};

use kition_kit::kdm::ObjectId;
use serde_json::Value;

use crate::element::{Element, Kind};
use crate::view::{BoardModel, PageView};

/// Whether `element` is a mind-map node.
pub fn is_node(element: &Element) -> bool {
    element.kind == Kind::Shape && element.register("shapeStyle").and_then(Value::as_str) == Some("mind-node")
}

/// Whether a node is collapsed.
pub fn collapsed(element: &Element) -> bool {
    element.register("mindMapCollapsed").and_then(Value::as_bool) == Some(true)
}

/// A branch: `connector` from `parent` to `child`.
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub connector: ObjectId,
    pub parent: ObjectId,
    pub child: ObjectId,
}

/// The mind maps of one page.
#[derive(Clone, Debug, Default)]
pub struct Topology {
    pub edges: Vec<Edge>,
    pub parent: HashMap<ObjectId, ObjectId>,
    pub children: HashMap<ObjectId, Vec<ObjectId>>,
    pub nodes: HashSet<ObjectId>,
}

impl Topology {
    /// The mind maps on `page`.
    pub fn read(model: &BoardModel, page: &PageView) -> Topology {
        let mut out = Topology::default();
        for element in model.elements_of(page) {
            if is_node(element) {
                out.nodes.insert(element.id.clone());
            }
        }
        if out.nodes.is_empty() {
            return out;
        }
        for connector in model.elements_of(page).filter(|element| element.kind == Kind::Connector) {
            let target = |end: &Option<crate::element::End>| end.as_ref().and_then(|end| end.binding.as_ref()).map(|binding| binding.target.clone());
            let (Some(parent), Some(child)) = (target(&connector.start), target(&connector.end)) else { continue };
            if parent == child || !out.nodes.contains(&parent) || !out.nodes.contains(&child) || out.parent.contains_key(&child) {
                continue;
            }
            out.parent.insert(child.clone(), parent.clone());
            out.children.entry(parent.clone()).or_default().push(child.clone());
            out.edges.push(Edge { connector: connector.id.clone(), parent, child });
        }
        out
    }

    /// The root of `node`'s map (a cycle stops at the node it came back to).
    pub fn root_of(&self, node: &ObjectId) -> ObjectId {
        let mut at = node.clone();
        let mut seen = HashSet::new();
        while let Some(parent) = self.parent.get(&at) {
            if !seen.insert(at.clone()) {
                break;
            }
            at = parent.clone();
        }
        at
    }

    /// `root` and every node below it, breadth first.
    pub fn below(&self, root: &ObjectId) -> Vec<ObjectId> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut queue = VecDeque::from([root.clone()]);
        while let Some(id) = queue.pop_front() {
            if !seen.insert(id.clone()) {
                continue;
            }
            queue.extend(self.children.get(&id).into_iter().flatten().cloned());
            out.push(id);
        }
        out
    }
}

/// The elements of `page` a collapsed node hides: its descendants and the branches to them
/// (v1's `hiddenElementIds`). A root never hides its children.
pub fn hidden(model: &BoardModel, page: &PageView) -> HashSet<ObjectId> {
    let mut out = HashSet::new();
    if !model.elements_of(page).any(|element| is_node(element) && collapsed(element)) {
        return out;
    }
    let topology = Topology::read(model, page);
    let mut branches: HashMap<&ObjectId, Vec<&Edge>> = HashMap::new();
    for edge in &topology.edges {
        branches.entry(&edge.parent).or_default().push(edge);
    }
    let roots: Vec<&ObjectId> = topology.nodes.iter().filter(|id| !topology.parent.contains_key(*id)).collect();
    let mut stack: Vec<(ObjectId, bool)> = roots.into_iter().map(|root| (root.clone(), false)).collect();
    let mut seen = HashSet::new();
    while let Some((id, hidden_above)) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        if hidden_above {
            out.insert(id.clone());
        }
        let is_root = !topology.parent.contains_key(&id);
        let hides = hidden_above || (!is_root && model.element(&id).is_some_and(collapsed));
        for edge in branches.get(&id).into_iter().flatten() {
            if hides {
                out.insert(edge.connector.clone());
            }
            stack.push((edge.child.clone(), hides));
        }
    }
    out
}
