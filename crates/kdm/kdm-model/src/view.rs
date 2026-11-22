//! The materialised view of a board (`BoardModel`): the pages the reader shows, in order, each
//! with the elements it shows, in order; bound connector ends derived from their targets (design
//! note 1.4).
//!
//! **Reading rules** (as Kition's `boardKdmRead.ts` `boardLayout`): pages in the root's `pages`
//! order, each live `Page` once; on each page, the live `Element`s of its `elements` list in order,
//! each element only where it is listed first (an element a merge left in two lists shows once).
//! Dangling and duplicate entries are not shown; the repair (`validate`) removes them.
//!
//! **Updates** (B10b): a change that touches only shown elements (a move, a resize, a label) re-reads
//! those elements in place and keeps the pages; anything else (the root, a page, a new, deleted or
//! hidden element) re-reads the lists. Indexes of each element's children and of the connectors
//! bound to it keep hierarchy and binding lookups off the page's full element list.
//!
//! Ported from Kition's `packages/board-editor/src/kdm/boardKdmRead.ts` (`boardLayout`,
//! `readBoardView`, `refreshBoardView`).

use std::collections::{BTreeSet, HashMap, HashSet};

use kition_kit::kdm::{ObjectId, Replica};
use kition_kit::session::Result;
use serde_json::Value;

use crate::element::{Element, End, Kind};
use crate::geometry::{Point, Rect, Size, anchor_side, auto_text_size, styled_text_size};
use crate::outline::outline_point;
use crate::{PAGE, root_id};

/// How deep the view follows `parent` chains and group nesting; deeper is treated as a cycle.
pub const MAX_DEPTH: usize = 64;

/// One page of the view: its elements in list order.
#[derive(Clone, Debug, PartialEq)]
pub struct PageView {
    pub id: ObjectId,
    pub name: String,
    pub elements: Vec<ObjectId>,
}

/// A connector end bound to something it cannot be bound to: an element that is gone, on another
/// page, or of a kind that takes no connectors (what repair rule V6 drops).
#[derive(Clone, Debug, PartialEq)]
pub struct DanglingEnd {
    pub connector: ObjectId,
    /// `start` or `end`.
    pub terminal: &'static str,
    pub target: ObjectId,
}

/// The materialised view of a board.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoardModel {
    title: String,
    /// The document's size from the origin (`Board.size`), when the board has one.
    size: Option<Size>,
    pages: Vec<PageView>,
    elements: HashMap<ObjectId, Element>,
    /// Each shown element's page (an index into `pages`).
    placement: HashMap<ObjectId, usize>,
    /// Each shown element's position in its page's list.
    order: HashMap<ObjectId, usize>,
    /// The shown elements whose `parent` is the key.
    children: HashMap<ObjectId, BTreeSet<ObjectId>>,
    /// The shown connectors with an end bound to the key.
    bound: HashMap<ObjectId, BTreeSet<ObjectId>>,
}

/// Adds `id` under `key` of an index.
fn index_add(index: &mut HashMap<ObjectId, BTreeSet<ObjectId>>, key: &ObjectId, id: &ObjectId) {
    index.entry(key.clone()).or_default().insert(id.clone());
}

/// Takes `id` out from under `key` of an index; an emptied key goes.
fn index_remove(index: &mut HashMap<ObjectId, BTreeSet<ObjectId>>, key: &ObjectId, id: &ObjectId) {
    if let Some(ids) = index.get_mut(key) {
        ids.remove(id);
        if ids.is_empty() {
            index.remove(key);
        }
    }
}

/// The targets an element's ends are bound to.
fn targets(element: &Element) -> impl Iterator<Item = &ObjectId> {
    [&element.start, &element.end].into_iter().flatten().filter_map(|end| end.binding.as_ref().map(|binding| &binding.target))
}

impl BoardModel {
    /// The whole view of `replica`.
    pub fn read(replica: &Replica) -> Result<Self> {
        let mut model = Self::default();
        model.rebuild(replica, None)?;
        Ok(model)
    }

    /// Brings the view up to date after the objects in `changed` changed: re-reads the root and
    /// the pages' lists, and only the elements that changed or were not shown before.
    pub fn refresh(&mut self, replica: &Replica, changed: &BTreeSet<ObjectId>) -> Result<()> {
        if self.patch(replica, changed)? {
            return Ok(());
        }
        self.rebuild(replica, Some(changed))
    }

    /// Re-reads `changed` in place when every one is a shown element that is still live: the
    /// pages' lists did not change (their owners are not in `changed`), so placement and order
    /// stay. Answers false, having changed nothing, when the lists must be re-read.
    fn patch(&mut self, replica: &Replica, changed: &BTreeSet<ObjectId>) -> Result<bool> {
        let mut fresh = Vec::with_capacity(changed.len());
        for id in changed {
            if !self.elements.contains_key(id) {
                return Ok(false);
            }
            match Element::read(replica, id)? {
                Some(element) => fresh.push(element),
                None => return Ok(false),
            }
        }
        for element in fresh {
            if let Some(old) = self.elements.remove(&element.id) {
                self.unindex(&old);
            }
            self.index(&element);
            self.elements.insert(element.id.clone(), element);
        }
        Ok(true)
    }

    fn index(&mut self, element: &Element) {
        if let Some(parent) = &element.parent {
            index_add(&mut self.children, parent, &element.id);
        }
        if element.kind == Kind::Connector {
            for target in targets(element) {
                index_add(&mut self.bound, target, &element.id);
            }
        }
    }

    fn unindex(&mut self, element: &Element) {
        if let Some(parent) = &element.parent {
            index_remove(&mut self.children, parent, &element.id);
        }
        for target in targets(element) {
            index_remove(&mut self.bound, target, &element.id);
        }
    }

    fn rebuild(&mut self, replica: &Replica, changed: Option<&BTreeSet<ObjectId>>) -> Result<()> {
        let root = root_id();
        let mut old = std::mem::take(&mut self.elements);
        self.placement.clear();
        self.order.clear();
        self.children.clear();
        self.bound.clear();
        self.pages.clear();
        self.title = if replica.exists(&root) { replica.get(&root, "title")?.as_str().unwrap_or_default().to_owned() } else { String::new() };
        self.size = if replica.exists(&root) {
            Size::parse(&replica.get(&root, "size")?).filter(|size| size.w.is_finite() && size.h.is_finite() && size.w > 0.0 && size.h > 0.0)
        } else {
            None
        };
        let page_ids = if replica.exists(&root) { replica.list(&root, "pages")? } else { Vec::new() };
        let mut seen_pages = HashSet::new();
        for page in page_ids {
            if !seen_pages.insert(page.clone()) || !replica.exists(&page) || replica.object_type(&page)? != PAGE {
                continue;
            }
            let name = replica.get(&page, "name")?.as_str().unwrap_or_default().to_owned();
            let index = self.pages.len();
            let mut shown = Vec::new();
            for id in replica.list(&page, "elements")? {
                if self.placement.contains_key(&id) {
                    continue;
                }
                let unchanged = changed.is_some_and(|changed| !changed.contains(&id));
                let element = match old.remove(&id) {
                    Some(element) if unchanged => Some(element),
                    _ => Element::read(replica, &id)?,
                };
                if let Some(element) = element {
                    self.placement.insert(id.clone(), index);
                    self.order.insert(id.clone(), shown.len());
                    self.index(&element);
                    self.elements.insert(id.clone(), element);
                    shown.push(id);
                }
            }
            self.pages.push(PageView { id: page, name, elements: shown });
        }
        Ok(())
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// The document's size in board units from the origin, when the board has one: what renders
    /// and thumbnails show while the content stays inside it.
    pub fn size(&self) -> Option<Size> {
        self.size
    }

    pub fn pages(&self) -> &[PageView] {
        &self.pages
    }

    pub fn page(&self, id: &ObjectId) -> Option<&PageView> {
        self.pages.iter().find(|page| &page.id == id)
    }

    /// A shown element.
    pub fn element(&self, id: &ObjectId) -> Option<&Element> {
        self.elements.get(id)
    }

    /// How many elements the view shows.
    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    /// The page a shown element is on.
    pub fn page_of(&self, id: &ObjectId) -> Option<&PageView> {
        self.placement.get(id).and_then(|index| self.pages.get(*index))
    }

    /// A page's elements in order (back to front).
    pub fn elements_of<'a>(&'a self, page: &'a PageView) -> impl Iterator<Item = &'a Element> + 'a {
        page.elements.iter().filter_map(|id| self.elements.get(id))
    }

    /// The elements that contain shown elements (frames and groups with children), with those
    /// children's ids, in no order: as many as there are containers, not elements.
    pub fn containers(&self) -> impl Iterator<Item = (&Element, &BTreeSet<ObjectId>)> + '_ {
        self.children.iter().filter_map(|(id, children)| self.elements.get(id).map(|element| (element, children)))
    }

    /// The elements on `id`'s page whose `parent` is `id`, in page order.
    pub fn children(&self, id: &ObjectId) -> Vec<&Element> {
        let Some(page) = self.placement.get(id) else { return Vec::new() };
        self.in_page_order(self.children.get(id), *page)
    }

    /// The connectors on `id`'s page with an end bound to `id`, in page order.
    pub fn connectors_bound_to(&self, id: &ObjectId) -> Vec<&Element> {
        let Some(page) = self.placement.get(id) else { return Vec::new() };
        self.in_page_order(self.bound.get(id), *page)
    }

    /// The shown elements of `ids` on page `page`, in page order.
    fn in_page_order(&self, ids: Option<&BTreeSet<ObjectId>>, page: usize) -> Vec<&Element> {
        let mut out: Vec<(usize, &Element)> = ids
            .into_iter()
            .flatten()
            .filter(|id| self.placement.get(*id) == Some(&page))
            .filter_map(|id| Some((*self.order.get(id)?, self.elements.get(id)?)))
            .collect();
        out.sort_by_key(|(at, _)| *at);
        out.into_iter().map(|(_, element)| element).collect()
    }

    /// `id` and every element below it through `parent`, depth first, in page order.
    pub fn with_descendants(&self, id: &ObjectId) -> Vec<ObjectId> {
        let mut out = vec![id.clone()];
        let mut at = 0;
        while let Some(current) = out.get(at).cloned() {
            at += 1;
            if out.len() > self.elements.len() {
                break;
            }
            for child in self.children(&current) {
                if !out.contains(&child.id) {
                    out.push(child.id.clone());
                }
            }
        }
        out
    }

    /// The unrotated box of a boxed element; a `text` element without a stored size is measured
    /// with the registered fonts.
    pub fn rect(&self, element: &Element) -> Option<Rect> {
        if element.kind == Kind::Text && element.size.is_none() {
            let position = element.position?;
            let font_size = element.register("fontSize").and_then(Value::as_f64);
            let family = element.register("fontFamily").and_then(Value::as_str);
            let size = match element.styled_text() {
                Some(paragraphs) => styled_text_size(&paragraphs, font_size, family),
                None => auto_text_size(&element.text, font_size, family),
            };
            return Some(Rect::new(position.x, position.y, position.x + size.w, position.y + size.h));
        }
        element.rect()
    }

    /// Where a connector end is: derived from its binding's target when the target is a bindable
    /// element on the same page (design note 1.4: on the target's outline), the stored point
    /// otherwise.
    pub fn end_point(&self, connector: &ObjectId, end: &End) -> Point {
        if let Some(binding) = &end.binding
            && let Some(target) = self.elements.get(&binding.target)
            && target.kind.bindable()
            && self.placement.get(&binding.target) == self.placement.get(connector)
            && let Some(rect) = self.rect(target)
        {
            return outline_point(target.outline_shape(), rect, target.rotation, binding.anchor);
        }
        end.point
    }

    /// The side of its target a bound end leaves or enters through, pointing out of the target
    /// (`geometry::anchor_side`); `None` for an end [`end_point`](Self::end_point) does not derive
    /// from a target.
    pub fn end_side(&self, connector: &ObjectId, end: &End) -> Option<Point> {
        let binding = end.binding.as_ref()?;
        let target = self.elements.get(&binding.target)?;
        if !target.kind.bindable() || self.placement.get(&binding.target) != self.placement.get(connector) || self.rect(target).is_none() {
            return None;
        }
        anchor_side(binding.anchor, target.rotation)
    }

    /// A connector's route: its start, waypoints and end, ends derived.
    pub fn route(&self, connector: &Element) -> Vec<Point> {
        let mut out = Vec::with_capacity(connector.waypoints.len() + 2);
        if let Some(start) = &connector.start {
            out.push(self.end_point(&connector.id, start));
        }
        out.extend(connector.waypoints.iter().copied());
        if let Some(end) = &connector.end {
            out.push(self.end_point(&connector.id, end));
        }
        out
    }

    /// The axis-aligned world box an element covers (rotation included); a group's is its
    /// children's.
    pub fn bounds(&self, element: &Element) -> Option<Rect> {
        self.bounds_at(element, 0)
    }

    fn bounds_at(&self, element: &Element, depth: usize) -> Option<Rect> {
        match element.kind {
            Kind::Stroke => Rect::of_points(&element.points),
            Kind::Connector => Rect::of_points(&self.route(element)),
            Kind::Group => {
                if depth >= MAX_DEPTH {
                    return None;
                }
                self.children(&element.id).into_iter().filter_map(|child| self.bounds_at(child, depth + 1)).reduce(Rect::union)
            }
            _ => self.rect(element).map(|rect| rect.rotated_bounds(element.rotation)),
        }
    }

    /// The box around everything on a page.
    pub fn content_bounds(&self, page: &PageView) -> Option<Rect> {
        self.elements_of(page).filter(|element| element.kind != Kind::Group).filter_map(|element| self.bounds(element)).reduce(Rect::union)
    }

    /// The live element with `key` on `page`.
    pub fn by_key<'a>(&'a self, page: &'a PageView, key: &str) -> Option<&'a Element> {
        self.elements_of(page).find(|element| element.key.as_deref() == Some(key))
    }

    /// The connector ends of `page` bound to something they cannot be bound to (rule V6's state),
    /// in page order.
    pub fn dangling_ends(&self, page: &PageView) -> Vec<DanglingEnd> {
        let Some(index) = self.pages.iter().position(|shown| shown.id == page.id) else { return Vec::new() };
        let mut out = Vec::new();
        for connector in self.elements_of(page).filter(|element| element.kind == Kind::Connector) {
            for (terminal, end) in [("start", &connector.start), ("end", &connector.end)] {
                let Some(binding) = end.as_ref().and_then(|end| end.binding.as_ref()) else { continue };
                let ok = binding.target != connector.id
                    && self.placement.get(&binding.target) == Some(&index)
                    && self.elements.get(&binding.target).is_some_and(|target| target.kind.bindable());
                if !ok {
                    out.push(DanglingEnd { connector: connector.id.clone(), terminal, target: binding.target.clone() });
                }
            }
        }
        out
    }

    /// The blob hashes the shown images reference.
    pub fn image_hashes(&self) -> BTreeSet<String> {
        self.elements.values().filter_map(|element| element.image.as_ref().map(|blob| blob.hash.clone())).collect()
    }
}
