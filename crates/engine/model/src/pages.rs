//! Page commands: `page.add`, `page.rename`, `page.duplicate`, `page.delete`, `page.move`.
//! (`page.activate` is view state, kept by the document.)
//!
//! Ported from the page commands of Kition's `packages/board-editor/src/lib/boardCommands.ts`
//! (`page.reorder` becomes `page.move`).

use kition_board_kdm::{BoardModel, PAGE, root_id};
use kition_kit::kdm::{ObjectId, Tx};
use serde_json::{Map, Value, json};

use crate::clipboard::{self, Keys};
use crate::params::{self, Result, invalid, not_found};

/// The most characters of a page name.
const MAX_NAME: usize = 200;

fn name(params: &Value, key: &str) -> Result<Option<String>> {
    match params::opt_string(params, key)? {
        Some(name) if name.trim().is_empty() || name.chars().count() > MAX_NAME => {
            Err(invalid(format!("`{key}` must be 1 to {MAX_NAME} characters")))
        }
        other => Ok(other.map(str::to_owned)),
    }
}

/// The index of page `id` in the root's list.
fn index_of(tx: &Tx<'_>, id: &ObjectId) -> Result<usize> {
    tx.list_items(&root_id(), "pages")?.iter().position(|page| page == id).ok_or_else(|| not_found(format!("no page {}", id.as_str())))
}

fn new_page(tx: &mut Tx<'_>, name: &str, at: usize) -> Result<ObjectId> {
    let mut fields = Map::new();
    fields.insert("name".into(), json!(name));
    let page = tx.create(PAGE, fields)?;
    tx.list_insert(&root_id(), "pages", at, &page)?;
    Ok(page)
}

/// `page.add { name?, at? }`: a new empty page (at the end by default).
pub fn create(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let count = tx.list_items(&root_id(), "pages")?.len();
    let name = name(params, "name")?.unwrap_or_else(|| format!("Page {}", model.pages().len() + 1));
    let at = match params::opt_num(params, "at")? {
        Some(at) if at.fract() == 0.0 && at >= 0.0 && at <= count as f64 => at as usize,
        Some(_) => return Err(invalid(format!("`at` must be a whole number from 0 to {count}"))),
        None => count,
    };
    let page = new_page(tx, &name, at)?;
    Ok(json!({ "page": page.as_str(), "name": name }))
}

fn existing<'a>(model: &'a BoardModel, params: &Value) -> Result<&'a kition_board_kdm::PageView> {
    let id = params::id(params, "page")?;
    model.page(&id).ok_or_else(|| not_found(format!("no page {}", id.as_str())))
}

/// `page.rename { page, name }`.
pub fn rename(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let page = existing(model, params)?;
    let name = name(params, "name")?.ok_or_else(|| invalid("`name` must be a string"))?;
    tx.set(&page.id, "name", json!(name))?;
    Ok(json!({ "page": page.id.as_str(), "name": name }))
}

/// `page.duplicate { page, name? }`: a copy right after the page, keys kept.
pub fn duplicate(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let source = existing(model, params)?;
    let name = name(params, "name")?.unwrap_or_else(|| format!("{} copy", source.name));
    let at = index_of(tx, &source.id)? + 1;
    let page = new_page(tx, &name, at)?;
    let items: Vec<Value> = model.elements_of(source).map(clipboard::element_json).collect();
    let view = kition_board_kdm::PageView { id: page.clone(), name: name.clone(), elements: Vec::new() };
    let (made, _) = clipboard::instantiate(model, tx, &view, &items, 0.0, 0.0, Keys::Keep)?;
    Ok(json!({ "page": page.as_str(), "name": name, "elements": made.len() }))
}

/// `page.delete { page }`: the page and its elements; the last page stays.
pub fn delete(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let page = existing(model, params)?;
    if model.pages().len() <= 1 {
        return Err(invalid("a board keeps at least one page"));
    }
    // Everything shown on the page goes with it: a locked element refuses the delete.
    if let Some(element) = model.elements_of(page).find(|element| element.locked) {
        return Err(params::locked(&element.id));
    }
    index_of(tx, &page.id)?;
    for element in model.elements_of(page) {
        if tx.exists(&element.id) {
            tx.delete(&element.id)?;
        }
    }
    let listed = tx.list_items(&root_id(), "pages")?;
    for (index, item) in listed.iter().enumerate().rev() {
        if *item == page.id {
            tx.list_remove(&root_id(), "pages", index)?;
        }
    }
    tx.delete(&page.id)?;
    Ok(json!({ "deleted": [page.id.as_str()] }))
}

/// `page.move { page, to?, placement?: previous | next }`.
pub fn move_page(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let page = existing(model, params)?;
    let from = index_of(tx, &page.id)?;
    let last = tx.list_items(&root_id(), "pages")?.len().saturating_sub(1);
    let to = match (params::opt_num(params, "to")?, params::opt_string(params, "placement")?) {
        (Some(to), None) if to.fract() == 0.0 && to >= 0.0 && to <= last as f64 => to as usize,
        (Some(_), None) => return Err(invalid(format!("`to` must be a whole number from 0 to {last}"))),
        (None, Some("previous")) => from.saturating_sub(1),
        (None, Some("next")) => (from + 1).min(last),
        (None, Some(_)) => return Err(invalid("`placement` must be previous or next")),
        _ => return Err(invalid("give exactly one of `to` and `placement`")),
    };
    if to != from {
        tx.list_move(&root_id(), "pages", from, to)?;
    }
    Ok(json!({ "page": page.id.as_str(), "index": to }))
}
