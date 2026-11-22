//! `kition-board-check`: `board_check` (task B7, design note 5.3), what is wrong with a board, so an
//! agent can check its own work and a host can show it (KHP 2.0 A21, A38).
//!
//! Twelve codes, errors first. The catalogue with a trigger fixture per code is
//! `agent/checks.json`; the fixtures are in `conformance/`.
//!
//! | Code | Severity | Fix |
//! | --- | --- | --- |
//! | `board.card-target-missing` | error | `card.setTarget` |
//! | `board.image-missing-blob` | error | `image.insert` |
//! | `board.dangling-binding` | error | `connector.bind` |
//! | `board.card-target-changed` | warning | `card.setTarget` |
//! | `board.label-overflow` | warning | `element.resize` |
//! | `board.overlapping-label` | warning | `element.move` |
//! | `board.overlap` | warning | `layout.distribute` |
//! | `board.disconnected-node` | warning | `connector.create` |
//! | `board.missing-label` | warning | `element.setText` |
//! | `board.outside-frame` | warning | `frame.fit` |
//! | `board.connector-crosses-node` | info | `connector.setWaypoints` |
//! | `board.empty-frame` | info | — |
//!
//! A card's target is checked only when the host lends files (headless, or a host that answers
//! `kitionHost.size`): a target the host says does not exist is missing; when the reader cannot
//! tell (no lent files, no right to read), nothing is reported (design note 3.3).
//! `board.card-target-changed` compares the `docId` a board card stored with the one its target
//! board's file names now; other targets' ids come only from the host (`links/resolve`), which
//! gives them to no engine yet, so they are not checked. `board.dangling-binding` reports the
//! current state and, through [`check_opened`], the ends a file had dangling that opening detached.
//!
//! The flow-chart rules (`missing-label`, `disconnected-node`, `outside-frame`, `overlapping-label`)
//! port Kition's `packages/board-editor/src/lib/whiteboardLint.ts`.

use std::collections::{HashMap, HashSet};

use kition_board_kdm::{BoardModel, DanglingEnd, Element, Kind, PageView, Point, Rect};
use kition_board_scene::fragment::{label_pad, shape_label, text_lines};
use kition_board_scene::shapes::label_box;
use kition_kit::diagnostics::{Diagnostic, Fix, Severity, Target};
use kition_kit::kdm::ObjectId;
use kition_kit::session::BlobSource;
use serde_json::{Value, json};

/// A cell of the grid that finds overlapping boxes without comparing every pair.
const CELL: f64 = 512.0;
/// More cells than this and a box is compared with everything (a huge frame).
const MAX_CELLS: i64 = 256;

fn diagnostic(code: &str, severity: Severity, message: String, page: usize, id: &str, fix: Option<(&str, Value)>) -> Diagnostic {
    let mut found = Diagnostic::new(code, severity, message).on(Target {
        object_id: Some(id.to_owned()),
        page: u32::try_from(page + 1).ok(),
        ..Target::default()
    });
    if let Some((command, params)) = fix {
        found.fix = Some(Fix { command: command.to_owned(), params: Some(params) });
    }
    found
}

fn text_of<'a>(element: &'a Element, field: &str) -> Option<&'a str> {
    element.register(field).and_then(Value::as_str)
}

fn is_flow_node(element: &Element) -> bool {
    element.kind == Kind::Shape && text_of(element, "shapeStyle") == Some("flow-node")
}

fn label(element: &Element) -> String {
    let text = element.text.trim();
    if text.is_empty() {
        format!("{} {}", element.kind.as_str(), element.id.as_str())
    } else {
        let short: String = text.chars().take(40).collect();
        format!("\"{short}\"")
    }
}

/// What is wrong with the board, errors first.
pub fn check(model: &BoardModel, blobs: &dyn BlobSource) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (index, page) in model.pages().iter().enumerate() {
        check_page(model, page, index, blobs, &mut out);
    }
    out.sort_by_key(|found| found.severity);
    out
}

fn check_page(model: &BoardModel, page: &PageView, index: usize, blobs: &dyn BlobSource, out: &mut Vec<Diagnostic>) {
    let elements: Vec<&Element> = model.elements_of(page).collect();
    let on_page: HashSet<&str> = elements.iter().map(|element| element.id.as_str()).collect();
    let mut connected: HashSet<String> = HashSet::new();
    for element in &elements {
        if element.kind != Kind::Connector {
            continue;
        }
        for end in [&element.start, &element.end] {
            if let Some(binding) = end.as_ref().and_then(|end| end.binding.as_ref())
                && model.element(&binding.target).is_some_and(|target| target.kind.bindable() && on_page.contains(target.id.as_str()))
            {
                connected.insert(binding.target.as_str().to_owned());
            }
        }
    }
    for end in model.dangling_ends(page) {
        out.push(dangling(&end, index, "is attached to", ""));
    }
    let flow_nodes = elements.iter().filter(|element| is_flow_node(element)).count();
    for element in &elements {
        let id = element.id.as_str();
        match element.kind {
            Kind::Image => {
                if let Some(blob) = element.image.as_ref().filter(|blob| blobs.get(&blob.hash).is_none()) {
                    out.push(diagnostic(
                        "board.image-missing-blob",
                        Severity::Error,
                        format!(
                            "The picture's bytes ({}) are not in the board, so it shows as a placeholder.",
                            blob.hash.chars().take(12).collect::<String>()
                        ),
                        index,
                        id,
                        Some(("image.insert", json!({ "page": page.id.as_str() }))),
                    ));
                }
            }
            Kind::Card => card_target(element, index, out),
            Kind::Frame if model.children(&element.id).is_empty() => {
                out.push(diagnostic(
                    "board.empty-frame",
                    Severity::Info,
                    format!("The frame {} holds no elements.", label(element)),
                    index,
                    id,
                    None,
                ));
            }
            _ => {}
        }
        label_overflow(model, element, index, out);
        if element.kind == Kind::Shape && matches!(text_of(element, "shapeStyle"), Some("flow-node" | "mind-node")) && element.text.trim().is_empty()
        {
            out.push(diagnostic(
                "board.missing-label",
                Severity::Warning,
                "A flow-chart or mind-map node has no label.".to_owned(),
                index,
                id,
                Some(("element.setText", json!({ "id": id }))),
            ));
        }
        if is_flow_node(element) && flow_nodes > 1 && !connected.contains(id) {
            out.push(diagnostic(
                "board.disconnected-node",
                Severity::Warning,
                format!("The flow-chart node {} has no connector.", label(element)),
                index,
                id,
                Some(("connector.create", json!({ "from": { "element": id } }))),
            ));
        }
        if let Some(parent) = element.parent.as_ref().and_then(|parent| model.element(parent)).filter(|parent| parent.kind == Kind::Frame)
            && let (Some(frame), Some(bounds)) = (model.rect(parent), model.bounds(element))
            && !frame.contains_rect(&bounds)
        {
            out.push(diagnostic(
                "board.outside-frame",
                Severity::Warning,
                format!("{} reaches outside its frame {}.", label(element), label(parent)),
                index,
                id,
                Some(("frame.fit", json!({ "id": parent.id.as_str() }))),
            ));
        }
    }
    pairs(model, &elements, index, out);
    crossings(model, &elements, index, out);
}

/// `board.dangling-binding` for one connector end.
fn dangling(end: &DanglingEnd, index: usize, verb: &str, after: &str) -> Diagnostic {
    diagnostic(
        "board.dangling-binding",
        Severity::Error,
        format!("The connector's {} {verb} {}, which is not an element on this page{after}.", end.terminal, end.target.as_str()),
        index,
        end.connector.as_str(),
        Some(("connector.bind", json!({ "id": end.connector.as_str(), "terminal": end.terminal }))),
    )
}

/// `board_check` of a document opened from a file, with the connector ends the file had dangling
/// (`file::OpenedFile::detached`): opening repairs them (rule V6 detaches the end), so the state
/// no longer shows them, but the connector lost its attachment and an agent should attach it
/// again. Each is reported while its connector is on the board and that end is still free.
pub fn check_opened(model: &BoardModel, blobs: &dyn BlobSource, detached: &[(ObjectId, DanglingEnd)]) -> Vec<Diagnostic> {
    let mut out = check(model, blobs);
    for (_, end) in detached {
        let Some(connector) = model.element(&end.connector).filter(|element| element.kind == Kind::Connector) else { continue };
        let free = if end.terminal == "start" { &connector.start } else { &connector.end };
        if free.as_ref().is_none_or(|free| free.binding.is_some()) {
            continue;
        }
        let Some(index) = model.page_of(&end.connector).and_then(|page| model.pages().iter().position(|shown| shown.id == page.id)) else { continue };
        out.push(dangling(end, index, "was attached to", " in the file; opening the board detached it"));
    }
    out.sort_by_key(|found| found.severity);
    out
}

/// A card whose target the host says does not exist, or (a board card) whose path now holds
/// another board than the one the card was made for.
fn card_target(element: &Element, index: usize, out: &mut Vec<Diagnostic>) {
    let Some(card) = element.card.as_ref() else { return };
    if card.get("kind").and_then(Value::as_str) == Some("link") {
        return;
    }
    let Some(path) = card.get("target").and_then(|target| target.get("path")).and_then(Value::as_str) else { return };
    let size = kition_kit::host::size(path);
    if let Ok(size) = size {
        target_changed(element, card, path, size, index, out);
        return;
    }
    let Err(error) = size else { return };
    let lower = error.to_ascii_lowercase();
    // Only "not found" means missing; no lent files or no right to read means unknown (3.3).
    if lower.contains("not found") || lower.contains("notfound") || lower.contains("no such file") {
        out.push(diagnostic(
            "board.card-target-missing",
            Severity::Error,
            format!("The card points at {path}, which does not exist."),
            index,
            element.id.as_str(),
            Some(("card.setTarget", json!({ "id": element.id.as_str() }))),
        ));
    }
}

/// The most bytes of a target board the check reads to learn its `docId`.
const MAX_TARGET_BYTES: u64 = 512 * 1024 * 1024;

/// `board.card-target-changed`: the card stored its target's `docId` (KitModel M7) and the file
/// at the path names another. Only a board names its `docId` in its own file (`meta.json`); for
/// other targets the id would come from the host (KitLinks LR4 `links/resolve`), which does not
/// give it to the engine, so they are not checked.
fn target_changed(element: &Element, card: &Value, path: &str, size: u64, index: usize, out: &mut Vec<Diagnostic>) {
    let Some(stored) = card.get("target").and_then(|target| target.get("docId")).and_then(Value::as_str) else { return };
    if card.get("kind").and_then(Value::as_str) != Some("board") || size > MAX_TARGET_BYTES {
        return;
    }
    let Some(now) = kition_kit::host::read(path).ok().and_then(|bytes| kition_board_kdm::file::doc_id_of(&bytes)) else { return };
    if now != stored {
        out.push(diagnostic(
            "board.card-target-changed",
            Severity::Warning,
            format!("The card was made for the board {stored}, but {path} now holds the board {now}."),
            index,
            element.id.as_str(),
            Some(("card.setTarget", json!({ "id": element.id.as_str() }))),
        ));
    }
}

/// A label taller or wider than the part of its shape a label may use, or a sized text box its
/// lines overflow.
fn label_overflow(model: &BoardModel, element: &Element, index: usize, out: &mut Vec<Diagnostic>) {
    let Some(rect) = model.rect(element) else { return };
    let (w, h) = (rect.width(), rect.height());
    let overflow = match element.kind {
        Kind::Shape if !element.text.trim().is_empty() => {
            let shape = element.register("shapeType").and_then(Value::as_str).unwrap_or("rectangle");
            let area = label_box(shape, w, h);
            let (lw, lh) = (area.width(), area.height());
            let (style, lines) = shape_label(element, lw);
            let height = lines.len() as f64 * style.line_height();
            let widest = lines.iter().map(|line| line.width).fold(0.0, f64::max);
            (height - lh).max(widest - (lw - 2.0 * label_pad(lw)).max(style.size))
        }
        Kind::Text if element.size.is_some() && !element.text.is_empty() => {
            let (style, lines) = text_lines(element);
            lines.len() as f64 * style.line_height() - h
        }
        _ => return,
    };
    if overflow > 0.5 {
        out.push(diagnostic(
            "board.label-overflow",
            Severity::Warning,
            format!("The text of {} does not fit its box by {:.0} units.", label(element), overflow),
            index,
            element.id.as_str(),
            Some(("element.resize", json!({ "id": element.id.as_str() }))),
        ));
    }
}

/// The grid cells a box covers, or `None` when it covers too many.
fn cells(rect: &Rect) -> Option<Vec<(i64, i64)>> {
    let span = |a: f64, b: f64| ((a / CELL).floor() as i64, (b / CELL).floor() as i64);
    let (x0, x1) = span(rect.x0, rect.x1);
    let (y0, y1) = span(rect.y0, rect.y1);
    if (x1 - x0 + 1).saturating_mul(y1 - y0 + 1) > MAX_CELLS {
        return None;
    }
    Some((x0..=x1).flat_map(|x| (y0..=y1).map(move |y| (x, y))).collect())
}

/// The pairs of boxes in `items` that overlap, each once, in the items' order.
fn overlapping<'a>(items: &[(&'a Element, Rect)]) -> Vec<(&'a Element, &'a Element)> {
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    let mut wide = Vec::new();
    for (at, (_, rect)) in items.iter().enumerate() {
        match cells(rect) {
            Some(cells) => cells.into_iter().for_each(|cell| grid.entry(cell).or_default().push(at)),
            None => wide.push(at),
        }
    }
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    let mut consider = |a: usize, b: usize| {
        let (a, b) = (a.min(b), a.max(b));
        if a == b || !seen.insert((a, b)) {
            return;
        }
        if let (Some((left, ra)), Some((right, rb))) = (items.get(a), items.get(b))
            && ra.intersects(rb)
        {
            found.push((*left, *right));
        }
    };
    for members in grid.values() {
        for (i, a) in members.iter().enumerate() {
            for b in members.iter().skip(i + 1) {
                consider(*a, *b);
            }
        }
    }
    for a in &wide {
        for b in 0..items.len() {
            consider(*a, b);
        }
    }
    found.sort_by_key(|(a, b)| (a.id.as_str().to_owned(), b.id.as_str().to_owned()));
    found
}

/// Overlapping text labels and overlapping flow-chart nodes.
fn pairs(model: &BoardModel, elements: &[&Element], index: usize, out: &mut Vec<Diagnostic>) {
    let boxes = |keep: &dyn Fn(&Element) -> bool| -> Vec<(&Element, Rect)> {
        elements.iter().filter(|element| keep(element)).filter_map(|element| model.bounds(element).map(|rect| (*element, rect))).collect()
    };
    let labels = boxes(&|element| element.kind == Kind::Text && !element.text.trim().is_empty());
    for (a, b) in overlapping(&labels) {
        out.push(diagnostic(
            "board.overlapping-label",
            Severity::Warning,
            format!("The texts {} and {} overlap.", label(a), label(b)),
            index,
            a.id.as_str(),
            Some(("element.move", json!({ "ids": [a.id.as_str()] }))),
        ));
    }
    let nodes = boxes(&|element| is_flow_node(element));
    for (a, b) in overlapping(&nodes) {
        out.push(diagnostic(
            "board.overlap",
            Severity::Warning,
            format!("The flow-chart nodes {} and {} overlap.", label(a), label(b)),
            index,
            a.id.as_str(),
            Some(("layout.distribute", json!({ "ids": [a.id.as_str(), b.id.as_str()], "axis": "horizontal" }))),
        ));
    }
}

/// Whether the segment `a`–`b` passes through the inside of `rect`.
fn segment_crosses(a: Point, b: Point, rect: &Rect) -> bool {
    // Liang–Barsky clipping against the box.
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;
    for (p, q) in [(-dx, a.x - rect.x0), (dx, rect.x1 - a.x), (-dy, a.y - rect.y0), (dy, rect.y1 - a.y)] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            t0 = t0.max(t);
        } else {
            t1 = t1.min(t);
        }
        if t0 > t1 {
            return false;
        }
    }
    t1 - t0 > 1e-9
}

/// Connectors whose route runs through a shape that is neither of their ends.
fn crossings(model: &BoardModel, elements: &[&Element], index: usize, out: &mut Vec<Diagnostic>) {
    let shapes: Vec<(&Element, Rect)> = elements
        .iter()
        .filter(|element| element.kind == Kind::Shape)
        .filter_map(|element| model.bounds(element).map(|rect| (*element, Rect::new(rect.x0 + 2.0, rect.y0 + 2.0, rect.x1 - 2.0, rect.y1 - 2.0))))
        .filter(|(_, rect)| rect.width() > 0.0 && rect.height() > 0.0)
        .collect();
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (at, (_, rect)) in shapes.iter().enumerate() {
        for cell in cells(rect).unwrap_or_default() {
            grid.entry(cell).or_default().push(at);
        }
    }
    for connector in elements.iter().filter(|element| element.kind == Kind::Connector) {
        let ends: HashSet<&str> = [&connector.start, &connector.end]
            .into_iter()
            .filter_map(|end| end.as_ref().and_then(|end| end.binding.as_ref()).map(|binding| binding.target.as_str()))
            .collect();
        let route = model.route(connector);
        let mut hit: Option<&Element> = None;
        'segments: for pair in route.windows(2) {
            let [a, b] = pair else { continue };
            let Some(cells) = cells(&Rect::new(a.x, a.y, b.x, b.y)) else { continue };
            for cell in cells {
                for at in grid.get(&cell).into_iter().flatten() {
                    let Some((shape, rect)) = shapes.get(*at) else { continue };
                    if ends.contains(shape.id.as_str()) || connector.parent.as_ref() == Some(&shape.id) {
                        continue;
                    }
                    if segment_crosses(*a, *b, rect) {
                        hit = Some(shape);
                        break 'segments;
                    }
                }
            }
        }
        if let Some(shape) = hit {
            out.push(diagnostic(
                "board.connector-crosses-node",
                Severity::Info,
                format!("A connector runs through {}, which it does not connect.", label(shape)),
                index,
                connector.id.as_str(),
                Some(("connector.setWaypoints", json!({ "id": connector.id.as_str() }))),
            ));
        }
    }
}

#[cfg(test)]
mod tests;
