//! Reading command parameters: every missing or malformed parameter is a typed `invalid-params`
//! error naming it, never a panic (requirement E3).

use kition_board_kdm::{BoardModel, Element, PageView, Point, Size};
use kition_kit::kdm::ObjectId;
use kition_kit::session::KitError;
use serde_json::Value;

/// The largest coordinate a command accepts (board units).
pub const MAX_COORDINATE: f64 = 1_000_000.0;
/// The largest width or height a command accepts.
pub const MAX_DIMENSION: f64 = 100_000.0;
/// The most elements one command creates or changes (design note 1.6, P3).
pub const MAX_OBJECTS: usize = 10_000;

pub fn invalid(message: impl Into<String>) -> KitError {
    KitError::new(format!("invalid-params: {}", message.into()))
}

pub fn not_found(message: impl Into<String>) -> KitError {
    KitError::new(format!("not-found: {}", message.into()))
}

pub fn locked(id: &ObjectId) -> KitError {
    KitError::new(format!("locked: element {} is locked; unlock it with element.lock", id.as_str()))
}

pub type Result<T> = std::result::Result<T, KitError>;

/// A required string parameter.
pub fn string<'a>(params: &'a Value, key: &str) -> Result<&'a str> {
    params.get(key).and_then(Value::as_str).ok_or_else(|| invalid(format!("`{key}` must be a string")))
}

/// An optional string parameter (absent or null is `None`).
pub fn opt_string<'a>(params: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(invalid(format!("`{key}` must be a string"))),
    }
}

/// A required id parameter.
pub fn id(params: &Value, key: &str) -> Result<ObjectId> {
    string(params, key).map(ObjectId::from_string)
}

/// A required non-empty list of ids, without duplicates, at most [`MAX_OBJECTS`].
pub fn ids(params: &Value, key: &str) -> Result<Vec<ObjectId>> {
    let list = params.get(key).and_then(Value::as_array).ok_or_else(|| invalid(format!("`{key}` must be a list of element ids")))?;
    if list.is_empty() {
        return Err(invalid(format!("`{key}` names no element")));
    }
    if list.len() > MAX_OBJECTS {
        return Err(invalid(format!("board.too-many-objects: `{key}` names more than {MAX_OBJECTS} elements")));
    }
    let mut out: Vec<ObjectId> = Vec::with_capacity(list.len());
    for item in list {
        let id = item.as_str().map(ObjectId::from_string).ok_or_else(|| invalid(format!("`{key}` must hold ids (strings)")))?;
        if !out.contains(&id) {
            out.push(id);
        }
    }
    Ok(out)
}

/// A finite number.
pub fn number(value: &Value, what: &str) -> Result<f64> {
    value.as_f64().filter(|v| v.is_finite()).ok_or_else(|| invalid(format!("`{what}` must be a finite number")))
}

/// A required finite number parameter.
pub fn num(params: &Value, key: &str) -> Result<f64> {
    number(params.get(key).unwrap_or(&Value::Null), key)
}

/// An optional finite number parameter.
pub fn opt_num(params: &Value, key: &str) -> Result<Option<f64>> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => number(value, key).map(Some),
    }
}

/// An optional boolean parameter.
pub fn opt_bool(params: &Value, key: &str) -> Result<Option<bool>> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(flag)) => Ok(Some(*flag)),
        Some(_) => Err(invalid(format!("`{key}` must be true or false"))),
    }
}

fn coordinate(value: f64, what: &str) -> Result<f64> {
    if value.abs() <= MAX_COORDINATE { Ok(value) } else { Err(invalid(format!("`{what}` is outside ±{MAX_COORDINATE}"))) }
}

/// A point: `[x, y]` or `{ "x", "y" }`, within the board's range.
pub fn point(value: &Value, what: &str) -> Result<Point> {
    let parsed = Point::parse_any(value).ok_or_else(|| invalid(format!("`{what}` must be [x, y] or {{ x, y }} with finite numbers")))?;
    Ok(Point::new(coordinate(parsed.x, what)?, coordinate(parsed.y, what)?))
}

/// An optional point parameter.
pub fn opt_point(params: &Value, key: &str) -> Result<Option<Point>> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => point(value, key).map(Some),
    }
}

/// A size: `[w, h]` or `{ "w", "h" }`, each in [0, MAX_DIMENSION].
pub fn size(value: &Value, what: &str) -> Result<Size> {
    let (w, h) = match value {
        Value::Array(pair) if pair.len() == 2 => (pair.first(), pair.get(1)),
        Value::Object(map) => (map.get("w"), map.get("h")),
        _ => (None, None),
    };
    let w = number(w.unwrap_or(&Value::Null), what)?;
    let h = number(h.unwrap_or(&Value::Null), what)?;
    if !(0.0..=MAX_DIMENSION).contains(&w) || !(0.0..=MAX_DIMENSION).contains(&h) {
        return Err(invalid(format!("`{what}` must be between 0 and {MAX_DIMENSION} on each side")));
    }
    Ok(Size::new(w, h))
}

/// A list of points (`[[x, y], ...]`), at most `max`.
pub fn points(value: &Value, what: &str, max: usize) -> Result<Vec<Point>> {
    let list = value.as_array().ok_or_else(|| invalid(format!("`{what}` must be a list of [x, y] points")))?;
    if list.len() > max {
        return Err(invalid(format!("`{what}` has more than {max} points")));
    }
    list.iter().map(|item| point(item, what)).collect()
}

/// A box `[x0, y0, x1, y1]` with a positive size.
pub fn rect(value: &Value, what: &str) -> Result<kition_board_kdm::Rect> {
    let list = value.as_array().filter(|list| list.len() == 4).ok_or_else(|| invalid(format!("`{what}` must be [x0, y0, x1, y1]")))?;
    let mut numbers = [0.0; 4];
    for (slot, item) in numbers.iter_mut().zip(list) {
        *slot = coordinate(number(item, what)?, what)?;
    }
    let [x0, y0, x1, y1] = numbers;
    let rect = kition_board_kdm::Rect::new(x0, y0, x1, y1);
    if rect.width() > MAX_DIMENSION || rect.height() > MAX_DIMENSION {
        return Err(invalid(format!("`{what}` is larger than {MAX_DIMENSION} on a side")));
    }
    Ok(rect)
}

/// The page a command works on: `page` when given, else the active page.
pub fn page<'a>(model: &'a BoardModel, params: &Value, active: Option<&ObjectId>) -> Result<&'a PageView> {
    match opt_string(params, "page")? {
        Some(page) => model.page(&ObjectId::from_string(page)).ok_or_else(|| not_found(format!("no page {page}"))),
        // The page the editor shows; one another writer deleted is not replaced by another page.
        None => match active {
            Some(id) => {
                model.page(id).ok_or_else(|| not_found(format!("the page {} the editor showed is gone; name the `page` to work in", id.as_str())))
            }
            None => model.pages().first().ok_or_else(|| invalid("the board has no page")),
        },
    }
}

/// A shown element.
pub fn element<'a>(model: &'a BoardModel, id: &ObjectId) -> Result<&'a Element> {
    model.element(id).ok_or_else(|| not_found(format!("no element {}", id.as_str())))
}

/// Shown elements, all on one page; answers the page too.
pub fn elements_on_one_page<'a>(model: &'a BoardModel, ids: &[ObjectId]) -> Result<(Vec<&'a Element>, &'a PageView)> {
    let mut out = Vec::with_capacity(ids.len());
    let mut page: Option<&PageView> = None;
    for id in ids {
        out.push(element(model, id)?);
        let here = model.page_of(id).ok_or_else(|| not_found(format!("no element {}", id.as_str())))?;
        match page {
            Some(page) if page.id != here.id => return Err(invalid("the elements must be on one page")),
            _ => page = Some(here),
        }
    }
    let page = page.ok_or_else(|| invalid("no elements given"))?;
    Ok((out, page))
}

/// A parameter object: `params[key]` must be an object when present.
pub fn object<'a>(params: &'a Value, key: &str) -> Result<Option<&'a serde_json::Map<String, Value>>> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(map)) => Ok(Some(map)),
        Some(_) => Err(invalid(format!("`{key}` must be an object"))),
    }
}
