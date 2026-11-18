//! Arrange commands: `layout.align`, `layout.distribute`, `layout.stack`. Each moves whole
//! selection roots (with what is inside them) and keeps bound connector ends current.
//!
//! Ported from Kition's `packages/board-editor/src/lib/boardLayout.ts`.

use kition_board_kdm::{BoardModel, Rect};
use kition_kit::kdm::{ObjectId, Tx};
use serde_json::{Value, json};

use crate::edit::{self, id_list, roots};
use crate::params::{self, Result, invalid};

/// The selection roots with their boxes (elements without geometry are left out).
fn boxed_roots(model: &BoardModel, params: &Value) -> Result<(Vec<(ObjectId, Rect)>, kition_board_kdm::PageView)> {
    let ids = params::ids(params, "ids")?;
    let (_, page) = params::elements_on_one_page(model, &ids)?;
    // A connector bound at both ends without bends has nothing of its own to move: it follows
    // the elements it is bound to, so it is not arranged (nor answered as moved).
    let follows = |element: &kition_board_kdm::Element| {
        element.kind == kition_board_kdm::Kind::Connector
            && element.waypoints.is_empty()
            && [&element.start, &element.end].iter().all(|end| end.as_ref().is_some_and(|end| end.binding.is_some()))
    };
    let found = roots(model, &ids)
        .into_iter()
        .filter_map(|id| model.element(&id).filter(|element| !follows(element)).and_then(|element| model.bounds(element)).map(|bounds| (id, bounds)))
        .collect();
    Ok((found, page.clone()))
}

fn apply(model: &BoardModel, tx: &mut Tx<'_>, page: &kition_board_kdm::PageView, moves: Vec<(ObjectId, f64, f64)>) -> Result<Value> {
    let mut moved = Vec::new();
    for (id, dx, dy) in moves {
        if dx != 0.0 || dy != 0.0 {
            moved.extend(edit::move_by(model, tx, page, std::slice::from_ref(&id), dx, dy)?);
        }
    }
    Ok(json!({ "moved": id_list(&moved) }))
}

/// `layout.align { ids, edge: left | center | right | top | middle | bottom }`: to the selection's
/// box.
pub fn align(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let edge = params::string(params, "edge")?;
    if !["left", "center", "right", "top", "middle", "bottom"].contains(&edge) {
        return Err(invalid("`edge` must be left, center, right, top, middle or bottom"));
    }
    let (items, page) = boxed_roots(model, params)?;
    let Some(all) = items.iter().map(|(_, bounds)| *bounds).reduce(Rect::union) else { return Ok(json!({ "moved": [] })) };
    let moves = items
        .into_iter()
        .map(|(id, b)| {
            let (dx, dy) = match edge {
                "left" => (all.x0 - b.x0, 0.0),
                "center" => (all.center().x - b.center().x, 0.0),
                "right" => (all.x1 - b.x1, 0.0),
                "top" => (0.0, all.y0 - b.y0),
                "middle" => (0.0, all.center().y - b.center().y),
                _ => (0.0, all.y1 - b.y1),
            };
            (id, dx, dy)
        })
        .collect();
    apply(model, tx, &page, moves)
}

fn axis(params: &Value) -> Result<bool> {
    match params::string(params, "axis")? {
        "horizontal" => Ok(true),
        "vertical" => Ok(false),
        _ => Err(invalid("`axis` must be horizontal or vertical")),
    }
}

/// `layout.distribute { ids, axis }`: equal gaps between the first and the last (three or more).
pub fn distribute(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let horizontal = axis(params)?;
    let (mut items, page) = boxed_roots(model, params)?;
    if items.len() < 3 {
        return Ok(json!({ "moved": [] }));
    }
    let start = |b: &Rect| if horizontal { b.x0 } else { b.y0 };
    let length = |b: &Rect| if horizontal { b.width() } else { b.height() };
    items.sort_by(|a, b| start(&a.1).total_cmp(&start(&b.1)).then_with(|| a.0.cmp(&b.0)));
    let Some(first) = items.first() else { return Ok(json!({ "moved": [] })) };
    // From the first start to the furthest end: the selection's extent stays.
    let end = items.iter().map(|(_, b)| start(b) + length(b)).fold(f64::NEG_INFINITY, f64::max);
    let span = end - start(&first.1);
    let occupied: f64 = items.iter().map(|(_, b)| length(b)).sum();
    let gap = (span - occupied) / (items.len() - 1) as f64;
    let mut cursor = start(&first.1);
    let mut moves = Vec::new();
    for (id, b) in items {
        let delta = cursor - start(&b);
        moves.push(if horizontal { (id, delta, 0.0) } else { (id, 0.0, delta) });
        cursor += length(&b) + gap;
    }
    apply(model, tx, &page, moves)
}

/// `layout.stack { ids, axis, gap? }`: one after another from the first, `gap` apart (24).
pub fn stack(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let horizontal = axis(params)?;
    let gap = params::opt_num(params, "gap")?.unwrap_or(24.0);
    if !(0.0..=10_000.0).contains(&gap) {
        return Err(invalid("`gap` must be between 0 and 10000"));
    }
    let (mut items, page) = boxed_roots(model, params)?;
    let start = |b: &Rect| if horizontal { b.x0 } else { b.y0 };
    let length = |b: &Rect| if horizontal { b.width() } else { b.height() };
    items.sort_by(|a, b| start(&a.1).total_cmp(&start(&b.1)).then_with(|| a.0.cmp(&b.0)));
    let Some(first) = items.first() else { return Ok(json!({ "moved": [] })) };
    let mut cursor = start(&first.1);
    let mut moves = Vec::new();
    for (id, b) in items {
        let delta = cursor - start(&b);
        moves.push(if horizontal { (id, delta, 0.0) } else { (id, 0.0, delta) });
        cursor += length(&b) + gap;
    }
    apply(model, tx, &page, moves)
}
