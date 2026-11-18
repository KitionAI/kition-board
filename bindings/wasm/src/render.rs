//! `file.render`: the agent's preview by explicit view (`agent.render` in kit.json, Kit Process
//! Protocol section 7, task card C4/C5/C6-board). It draws one view of one page:
//!
//! - `target.page`: a page's id as `board_inspect` lists it (`pages[].id`), or its number counted
//!   from 1 as diagnostics give it; without it the page of `frame` or `objectId`, else the
//!   **first page** in the board's order, never the page the editor shows.
//! - at most one of `target.frame` (a frame's id: its box), `target.objectId` (an element's id: its
//!   bounds with a margin of [`OBJECT_MARGIN`] units) and `target.area` (`[x0, y0, x1, y1]` in board
//!   units); without them the page's **document area**, the `area` `board_inspect` lists for the
//!   page (the board's size from the origin, grown where content goes past it).
//! - `scale`: pixels per board unit, more than 0 and at most [`MAX_SCALE`], default 1; brought
//!   down so the longest side is at most `maxSize` (16 to 8192) and the renderer's largest side.
//!
//! It is a function of the board and the call: the shown page and the editor's viewport are
//! neither read nor changed, nothing of the editor is drawn, and nothing comes from the clock, so
//! the same call answers the same bytes.

use base64::Engine as _;
use kition_board_kdm::{Kind, PageView, Rect};
use kition_kit::MAX_RENDER_SIDE;
use kition_kit::kdm::ObjectId;
use serde_json::{Map, Value, json};

use crate::BoardDocument;

/// The largest `scale`.
pub const MAX_SCALE: f64 = 4.0;
/// Space drawn around an element named by `target.objectId`, in board units.
pub const OBJECT_MARGIN: f64 = 16.0;
/// `maxSize`'s bounds.
const MAX_SIZE: std::ops::RangeInclusive<u64> = 16..=8192;
/// The members `target` takes.
pub const VIEW: [&str; 4] = ["page", "frame", "objectId", "area"];

const LISTED: &str = "the pages' ids are in board_inspect's `pages`, the elements' in board_query's `follow: \"pages.0.elements\"`";

fn invalid(message: impl std::fmt::Display) -> String {
    format!("invalid-params: file.render: {message}")
}

fn not_found(message: impl std::fmt::Display) -> String {
    format!("not-found: file.render: {message}")
}

impl BoardDocument {
    /// The page `target.page` names, when it names one.
    fn named_page(&self, value: Option<&Value>) -> Result<Option<&PageView>, String> {
        let pages = self.model().pages();
        match value {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(id)) => {
                self.model().page(&ObjectId::from_string(id)).map(Some).ok_or_else(|| not_found(format!("no page `{id}`: {LISTED}")))
            }
            Some(Value::Number(number)) => match number.as_u64() {
                Some(0) | None => Err(invalid(format!("`target.page` is {number}: a page's number is counted from 1 (or give its id; {LISTED})"))),
                Some(n) => usize::try_from(n - 1)
                    .ok()
                    .and_then(|index| pages.get(index))
                    .map(Some)
                    .ok_or_else(|| not_found(format!("page {n}: the board has {} pages", pages.len()))),
            },
            Some(other) => Err(invalid(format!("`target.page` is {other}: a page's id (or its number counted from 1)"))),
        }
    }

    /// The element `target.<member>` names, and its page, which must be `page` when the call names
    /// one.
    fn named_element<'a>(
        &'a self,
        member: &str,
        value: &Value,
        page: Option<&'a PageView>,
    ) -> Result<(&'a kition_board_kdm::Element, &'a PageView), String> {
        let model = self.model();
        let id = value.as_str().ok_or_else(|| invalid(format!("`target.{member}` is {value}: an element's id")))?;
        let element = model.element(&ObjectId::from_string(id)).ok_or_else(|| not_found(format!("no element `{id}`: {LISTED}")))?;
        let on = model.page_of(&element.id).ok_or_else(|| not_found(format!("no element `{id}`: {LISTED}")))?;
        if let Some(page) = page
            && page.id != on.id
        {
            return Err(invalid(format!("`{id}` is on page `{}`, and `target.page` names page `{}`", on.name, page.name)));
        }
        Ok((element, on))
    }

    /// `file.render {target?: {page?, frame?, objectId?, area?}, scale?, maxSize?}` →
    /// `{png, mimeType, width, height, scale, page, pageName, area, frame?, objectId?}` (module
    /// docs).
    pub(crate) fn render_view(&self, params: &Value) -> Result<Value, String> {
        let model = self.model();
        let target: Map<String, Value> = match params.get("target") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(target)) => target.clone(),
            Some(other) => return Err(invalid(format!("`target` is {other}: an object `{{page?, frame?, objectId?, area?}}`"))),
        };
        if let Some(other) = target.keys().find(|key| !VIEW.contains(&key.as_str())) {
            return Err(invalid(format!(
                "`target.{other}` is not a view it draws: `target` takes `page` (a page's id, or its number from 1), `frame` (a frame's id), `objectId` (an element's id) and `area` ([x0, y0, x1, y1] in board units)"
            )));
        }
        let views = ["frame", "objectId", "area"].iter().filter(|key| target.get(**key).is_some_and(|value| !value.is_null())).count();
        if views > 1 {
            return Err(invalid("give one of `frame`, `objectId` and `area`, not two: each is a whole view"));
        }
        let named = self.named_page(target.get("page"))?;
        let mut shows = Map::new();
        let (page, view) = if let Some(value) = target.get("frame").filter(|value| !value.is_null()) {
            let (frame, page) = self.named_element("frame", value, named)?;
            if frame.kind != Kind::Frame {
                return Err(invalid(format!("`{}` is a {}, not a frame (give it as `target.objectId`)", frame.id.as_str(), frame.kind.as_str())));
            }
            let rect = model.rect(frame).ok_or_else(|| invalid(format!("the frame `{}` has no box", frame.id.as_str())))?;
            shows.insert("frame".into(), json!(frame.id.as_str()));
            (page, rect)
        } else if let Some(value) = target.get("objectId").filter(|value| !value.is_null()) {
            let (element, page) = self.named_element("objectId", value, named)?;
            let bounds = model
                .bounds(element)
                .ok_or_else(|| invalid(format!("the {} `{}` has no box to draw", element.kind.as_str(), element.id.as_str())))?;
            shows.insert("objectId".into(), json!(element.id.as_str()));
            (page, Rect::new(bounds.x0 - OBJECT_MARGIN, bounds.y0 - OBJECT_MARGIN, bounds.x1 + OBJECT_MARGIN, bounds.y1 + OBJECT_MARGIN))
        } else {
            let page = match named {
                Some(page) => page,
                None => model.pages().first().ok_or_else(|| not_found("the board has no page"))?,
            };
            let view = match target.get("area").filter(|value| !value.is_null()) {
                Some(value) => area_of(value)?,
                None => {
                    let area = kition_board_scene::area(model, Some(page));
                    Rect::new(area.x, area.y, area.x + f64::from(area.width) * area.unit, area.y + f64::from(area.height) * area.unit)
                }
            };
            (page, view)
        };
        let mut scale = match params.get("scale") {
            None | Some(Value::Null) => 1.0,
            Some(value) => value
                .as_f64()
                .filter(|scale| scale.is_finite() && *scale > 0.0 && *scale <= MAX_SCALE)
                .ok_or_else(|| invalid(format!("`scale` is {value}: pixels per board unit, more than 0 and at most {MAX_SCALE}")))?,
        };
        let longest = view.width().max(view.height()).max(1.0);
        if let Some(value) = params.get("maxSize").filter(|value| !value.is_null()) {
            let max = value
                .as_u64()
                .filter(|max| MAX_SIZE.contains(max))
                .ok_or_else(|| invalid(format!("`maxSize` is {value}: the longest side in pixels, {} to {}", MAX_SIZE.start(), MAX_SIZE.end())))?;
            scale = scale.min(max as f64 / longest);
        }
        scale = scale.min(f64::from(MAX_RENDER_SIDE) / longest);
        let pixels = |extent: f64| (extent * scale).round().clamp(1.0, f64::from(MAX_RENDER_SIDE)) as u32;
        let (width, height) = (pixels(view.width()), pixels(view.height()));
        let png = kition_board_scene::picture(model, page, view, width, height, &self.blobs).map_err(|error| format!("file.render: {error}"))?;
        let mut answer = json!({
            "png": base64::engine::general_purpose::STANDARD.encode(&png),
            "mimeType": "image/png",
            "width": width,
            "height": height,
            "scale": scale,
            "page": page.id.as_str(),
            "pageName": page.name,
            "area": [view.x0, view.y0, view.x1, view.y1],
        });
        if let Some(object) = answer.as_object_mut() {
            object.extend(shows);
        }
        Ok(answer)
    }
}

/// `target.area`: `[x0, y0, x1, y1]` in board units, a box with a positive size within the board's
/// range.
fn area_of(value: &Value) -> Result<Rect, String> {
    let wrong = || {
        invalid(format!(
            "`target.area` is {value}: [x0, y0, x1, y1] in board units, finite, within ±{}, with x1 > x0 and y1 > y0",
            kition_board_model::params::MAX_COORDINATE
        ))
    };
    let numbers: Vec<f64> =
        value.as_array().filter(|items| items.len() == 4).ok_or_else(wrong)?.iter().map(Value::as_f64).collect::<Option<_>>().ok_or_else(wrong)?;
    let [x0, y0, x1, y1] = <[f64; 4]>::try_from(numbers).map_err(|_| wrong())?;
    let bound = kition_board_model::params::MAX_COORDINATE;
    if [x0, y0, x1, y1].iter().any(|n| !n.is_finite() || n.abs() > bound) || x1 <= x0 || y1 <= y0 {
        return Err(wrong());
    }
    Ok(Rect::new(x0, y0, x1, y1))
}
