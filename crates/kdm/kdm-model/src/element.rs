//! One `Element` object as the view holds it (board design note 1.1–1.2): its kind, its coupled
//! geometry registers parsed (`position`, `size`, `start`, `end`, `points`, `waypoints`), its text,
//! and every other register as stored.
//!
//! Ported from Kition's `packages/board-editor/src/kdm/boardKdmRead.ts` (`elementFromFields`,
//! `bindingsOf`), for the board schema.

use std::collections::BTreeMap;

use kition_kit::kdm::{ObjectId, Replica, TextRun};
use kition_kit::session::{BlobRef, Result};
use serde_json::{Map, Value, json};

use crate::geometry::{Point, Rect, Size};

/// An element's `kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Shape,
    Text,
    Stroke,
    Connector,
    Image,
    Card,
    Frame,
    Group,
}

impl Kind {
    pub const ALL: [Kind; 8] = [Kind::Shape, Kind::Text, Kind::Stroke, Kind::Connector, Kind::Image, Kind::Card, Kind::Frame, Kind::Group];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Shape => "shape",
            Kind::Text => "text",
            Kind::Stroke => "stroke",
            Kind::Connector => "connector",
            Kind::Image => "image",
            Kind::Card => "card",
            Kind::Frame => "frame",
            Kind::Group => "group",
        }
    }

    pub fn parse(text: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|kind| kind.as_str() == text)
    }

    /// Only frames and groups contain other elements (rule V4).
    pub fn can_contain(self) -> bool {
        matches!(self, Kind::Frame | Kind::Group)
    }

    /// A connector end can bind to it (rule V6): everything with a box of its own.
    pub fn bindable(self) -> bool {
        matches!(self, Kind::Shape | Kind::Text | Kind::Image | Kind::Card | Kind::Frame)
    }

    /// Its geometry is `position` and `size` (strokes and connectors have points, groups none).
    pub fn boxed(self) -> bool {
        matches!(self, Kind::Shape | Kind::Text | Kind::Image | Kind::Card | Kind::Frame)
    }
}

/// What a connector end is bound to: an element, at a point normalised to its unrotated box.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub target: ObjectId,
    /// Each in [0, 1] once repaired (rule V7).
    pub anchor: [f64; 2],
}

/// A connector end: `{ "point": [x, y], "binding"?: { "target", "anchor": [ax, ay] } }`.
#[derive(Clone, Debug, PartialEq)]
pub struct End {
    /// The stored point: a cache when bound (design note 1.4), the end itself when not.
    pub point: Point,
    pub binding: Option<Binding>,
}

impl End {
    pub fn free(point: Point) -> Self {
        Self { point, binding: None }
    }

    /// A bound end may leave out `point`, the cache the repair (rule V7) writes from its target;
    /// a free end needs it.
    pub fn parse(value: &Value) -> Option<End> {
        let binding = value.get("binding").and_then(|binding| {
            let target = binding.get("target")?.as_str()?;
            let anchor = Point::parse_pair(binding.get("anchor").unwrap_or(&json!([0.5, 0.5])))?;
            Some(Binding { target: ObjectId::from_string(target), anchor: [anchor.x, anchor.y] })
        });
        let point = match value.get("point") {
            Some(point) => Point::parse_pair(point)?,
            None if binding.is_some() => Point::default(),
            None => return None,
        };
        Some(End { point, binding })
    }

    pub fn to_json(&self) -> Value {
        let mut out = Map::new();
        out.insert("point".into(), self.point.to_pair());
        if let Some(binding) = &self.binding {
            out.insert("binding".into(), json!({ "target": binding.target.as_str(), "anchor": [binding.anchor[0], binding.anchor[1]] }));
        }
        Value::Object(out)
    }
}

/// The plain registers (style, mind-map and connector fields) the view keeps as stored.
pub const PLAIN_REGISTERS: &[&str] = &[
    "shapeType",
    "shapeStyle",
    "strokeColor",
    "fillColor",
    "fillStyle",
    "dashStyle",
    "strokeSize",
    "opacity",
    "roundness",
    "textFormat",
    "fontSize",
    "fontFamily",
    "textAlign",
    "labelPosition",
    "connectorType",
    "startArrowhead",
    "endArrowhead",
    "connectorRole",
    "mindMapBranchAxis",
    "mindMapBranchSide",
    "mindMapDirection",
    "mindMapCollapsed",
    "alt",
];

/// One element of the view.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub id: ObjectId,
    pub kind: Kind,
    pub position: Option<Point>,
    pub size: Option<Size>,
    /// Degrees about the box centre.
    pub rotation: f64,
    pub locked: bool,
    pub parent: Option<ObjectId>,
    pub key: Option<String>,
    /// The text's characters.
    pub text: String,
    /// The text as runs when any run has marks (bold, italic, link); empty otherwise.
    pub runs: Vec<TextRun>,
    pub points: Vec<Point>,
    pub start: Option<End>,
    pub end: Option<End>,
    pub waypoints: Vec<Point>,
    pub image: Option<BlobRef>,
    pub card: Option<Value>,
    pub source_ref_ids: Vec<String>,
    /// [`PLAIN_REGISTERS`] that hold a value.
    pub registers: BTreeMap<String, Value>,
}

impl Element {
    /// Reads the element `id` from the replica; `None` when it is not a live `Element`.
    pub fn read(replica: &Replica, id: &ObjectId) -> Result<Option<Element>> {
        if !replica.exists(id) || replica.object_type(id)? != crate::ELEMENT {
            return Ok(None);
        }
        let get = |field: &str| replica.get(id, field);
        let Some(kind) = get("kind")?.as_str().and_then(Kind::parse) else { return Ok(None) };
        let mut registers = BTreeMap::new();
        for field in PLAIN_REGISTERS {
            let value = get(field)?;
            if !value.is_null() {
                registers.insert((*field).to_owned(), value);
            }
        }
        Ok(Some(Element {
            id: id.clone(),
            kind,
            position: Point::parse_object(&get("position")?),
            size: Size::parse(&get("size")?),
            rotation: get("rotation")?.as_f64().filter(|r| r.is_finite()).unwrap_or(0.0),
            locked: get("locked")?.as_bool().unwrap_or(false),
            parent: get("parent")?.as_str().map(ObjectId::from_string),
            key: get("key")?.as_str().map(str::to_owned),
            text: replica.text(id, "text")?,
            runs: Some(replica.text_delta(id, "text")?).filter(|runs| runs.iter().any(|run| !run.attributes.is_empty())).unwrap_or_default(),
            points: Point::parse_list(&get("points")?),
            start: End::parse(&get("start")?),
            end: End::parse(&get("end")?),
            waypoints: Point::parse_list(&get("waypoints")?),
            image: serde_json::from_value(get("image")?).ok(),
            card: Some(get("card")?).filter(|card| !card.is_null()),
            source_ref_ids: get("sourceRefIds")?
                .as_array()
                .map(|ids| ids.iter().filter_map(|id| id.as_str().map(str::to_owned)).collect())
                .unwrap_or_default(),
            registers,
        }))
    }

    /// A plain register's value, `None` when absent.
    pub fn register(&self, field: &str) -> Option<&Value> {
        self.registers.get(field)
    }

    /// The `shapeType` whose outline a bound connector end meets (design note 1.4,
    /// [`crate::outline::outline_point`]): a shape's, `None` for every other kind (their box).
    pub fn outline_shape(&self) -> Option<&str> {
        if self.kind == Kind::Shape { self.register("shapeType").and_then(Value::as_str) } else { None }
    }

    /// The element's text as styled paragraphs when it is styled: its Markdown (`textFormat:
    /// markdown`, whose marks are not used), else its bold and italic marks. `None` for plain text.
    pub fn styled_text(&self) -> Option<Vec<kition_board_text::Paragraph>> {
        use kition_board_text::{Run, markdown};
        if self.register("textFormat").and_then(Value::as_str) == Some("markdown") {
            return Some(markdown::parse(&self.text));
        }
        if self.runs.is_empty() {
            return None;
        }
        let mut paragraphs = vec![Vec::new()];
        for run in &self.runs {
            let marked = |mark: &str| run.attributes.get(mark).is_some_and(|value| value == &Value::Bool(true));
            for (i, part) in run.insert.split('\n').enumerate() {
                if i > 0 {
                    paragraphs.push(Vec::new());
                }
                if !part.is_empty()
                    && let Some(paragraph) = paragraphs.last_mut()
                {
                    paragraph.push(Run { bold: marked("bold"), italic: marked("italic"), ..Run::plain(part) });
                }
            }
        }
        Some(paragraphs)
    }

    /// The element's own unrotated box, for boxed kinds with a position (a missing size is 0 × 0).
    pub fn rect(&self) -> Option<Rect> {
        if !self.kind.boxed() {
            return None;
        }
        let position = self.position?;
        let size = self.size.unwrap_or_default();
        Some(Rect::new(position.x, position.y, position.x + size.w, position.y + size.h))
    }
}
