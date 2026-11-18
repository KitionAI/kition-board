//! Templates (design note 5.1 `template.insert`): six starting structures (a mind map, a flow
//! chart, a roadmap, a kanban board, a retrospective and a storyboard) placed around a point of a
//! page as one transaction, through the clipboard's paste (references inside the template kept).
//!
//! Ported from Kition's `packages/board-editor/src/lib/whiteboardTemplates.ts` (the six templates,
//! `createTemplateBuilder`) and their English texts (`board.templates.content` of Kition's `en-US`
//! workspace strings): a board's content is never localised. Frames and groups are kinds now, so
//! the template's outer frame is a `frame` element.

use std::collections::HashMap;

use kition_board_kdm::BoardModel;
use kition_kit::kdm::Tx;
use serde_json::{Map, Value, json};

use crate::Ctx;
use crate::clipboard::{self, PAYLOAD_TYPE, PAYLOAD_VERSION};
use crate::params::{self, Result, invalid};

/// The templates and their sizes.
pub const TEMPLATES: &[(&str, f64, f64)] = &[
    ("mindmap", 820.0, 360.0),
    ("flowchart", 920.0, 520.0),
    ("roadmap", 960.0, 560.0),
    ("kanban", 960.0, 600.0),
    ("retrospective", 960.0, 580.0),
    ("storyboard", 1000.0, 650.0),
];

/// Builds a template's elements in its own frame of reference (its top-left at the origin).
struct Builder {
    items: Vec<Value>,
    nodes: HashMap<&'static str, (String, [f64; 4])>,
    root: Option<String>,
    next: usize,
}

impl Builder {
    /// A template inside an outer frame of `w` × `h` (or, `framed: false`, loose on the page).
    fn new(w: f64, h: f64, framed: bool) -> Builder {
        let mut builder = Builder { items: Vec::new(), nodes: HashMap::new(), root: None, next: 0 };
        if framed {
            let root = builder.reference();
            builder.items.push(json!({ "ref": root, "kind": "frame", "position": { "x": 0, "y": 0 }, "size": { "w": w, "h": h } }));
            builder.root = Some(root);
        }
        builder
    }

    fn reference(&mut self) -> String {
        self.next += 1;
        format!("template-{}", self.next)
    }

    fn push(&mut self, mut item: Map<String, Value>, parent: Option<&str>) -> String {
        let id = self.reference();
        item.insert("ref".into(), json!(id));
        if let Some(parent) = parent.map(str::to_owned).or_else(|| self.root.clone()) {
            item.insert("parent".into(), json!(parent));
        }
        self.items.push(Value::Object(item));
        id
    }

    /// An inner frame (a column, a slide).
    fn frame(&mut self, x: f64, y: f64, w: f64, h: f64) -> String {
        let mut item = Map::new();
        item.insert("kind".into(), json!("frame"));
        item.insert("position".into(), json!({ "x": x, "y": y }));
        item.insert("size".into(), json!({ "w": w, "h": h }));
        self.push(item, None)
    }

    /// A shape `key` names for `connect`; `style` holds its shape type, role and colours.
    #[allow(clippy::too_many_arguments)]
    fn node(&mut self, key: &'static str, x: f64, y: f64, w: f64, h: f64, text: &str, style: Value, parent: Option<&str>) -> String {
        let mut item = Map::new();
        item.insert("kind".into(), json!("shape"));
        item.insert("position".into(), json!({ "x": x, "y": y }));
        item.insert("size".into(), json!({ "w": w, "h": h }));
        item.insert("text".into(), json!(text));
        for (field, value) in [
            ("shapeType", json!("rectangle")),
            ("dashStyle", json!("solid")),
            ("fillColor", json!("white")),
            ("fillStyle", json!("solid")),
            ("strokeColor", json!("ink")),
            ("strokeSize", json!("s")),
        ] {
            item.insert(field.into(), value);
        }
        if let Some(style) = style.as_object() {
            for (field, value) in style {
                item.insert(field.clone(), value.clone());
            }
        }
        let id = self.push(item, parent);
        self.nodes.insert(key, (id.clone(), [x, y, w, h]));
        id
    }

    fn text(&mut self, x: f64, y: f64, text: &str, size: f64, color: &str, parent: Option<&str>) -> String {
        let mut item = Map::new();
        item.insert("kind".into(), json!("text"));
        item.insert("position".into(), json!({ "x": x, "y": y }));
        item.insert("text".into(), json!(text));
        item.insert("fontSize".into(), json!(size));
        item.insert("strokeColor".into(), json!(color));
        self.push(item, parent)
    }

    fn title(&mut self, text: &str) -> String {
        self.text(24.0, 46.0, text, 24.0, "ink", None)
    }

    /// A connector from `from`'s `start` side to `to`'s `end` side (north, east, south, west).
    fn connect(&mut self, from: &str, to: &str, start: &str, end: &str, style: Value) {
        let (Some((a, a_box)), Some((b, b_box))) = (self.nodes.get(from).cloned(), self.nodes.get(to).cloned()) else { return };
        let anchor = |side: &str| match side {
            "north" => [0.5, 0.0],
            "south" => [0.5, 1.0],
            "west" => [0.0, 0.5],
            _ => [1.0, 0.5],
        };
        let point = |rect: [f64; 4], at: [f64; 2]| json!([rect[0] + rect[2] * at[0], rect[1] + rect[3] * at[1]]);
        let (sa, ea) = (anchor(start), anchor(end));
        let mut item = Map::new();
        item.insert("kind".into(), json!("connector"));
        item.insert("start".into(), json!({ "point": point(a_box, sa), "binding": { "target": a, "anchor": sa } }));
        item.insert("end".into(), json!({ "point": point(b_box, ea), "binding": { "target": b, "anchor": ea } }));
        for (field, value) in
            [("connectorType", json!("straight")), ("endArrowhead", json!("arrow")), ("strokeColor", json!("gray")), ("strokeSize", json!("s"))]
        {
            item.insert(field.into(), value);
        }
        if let Some(style) = style.as_object() {
            for (field, value) in style {
                item.insert(field.clone(), value.clone());
            }
        }
        self.push(item, None);
    }

    /// The payload: the outer frame first, then connectors behind the rest (v1's `finish`).
    fn finish(self) -> Vec<Value> {
        let is_connector = |item: &Value| item.get("kind").and_then(Value::as_str) == Some("connector");
        let root = self.root.clone();
        let is_root = |item: &Value| root.as_deref().is_some_and(|root| item.get("ref").and_then(Value::as_str) == Some(root));
        let mut out: Vec<Value> = self.items.iter().filter(|item| is_root(item)).cloned().collect();
        out.extend(self.items.iter().filter(|item| !is_root(item) && is_connector(item)).cloned());
        out.extend(self.items.iter().filter(|item| !is_root(item) && !is_connector(item)).cloned());
        out
    }
}

fn mind_map() -> Vec<Value> {
    let mut b = Builder::new(820.0, 360.0, false);
    b.node(
        "topic",
        40.0,
        144.0,
        210.0,
        72.0,
        "Central topic",
        json!({ "fillColor": "gray", "mindMapDirection": "right", "shapeStyle": "mind-node", "strokeColor": "purple" }),
        None,
    );
    b.node("research", 355.0, 70.0, 142.0, 56.0, "Research", json!({ "shapeStyle": "mind-node", "strokeColor": "purple" }), None);
    b.node("ideas", 355.0, 230.0, 142.0, 56.0, "Ideas", json!({ "shapeStyle": "mind-node", "strokeColor": "blue" }), None);
    b.node("questions", 650.0, 72.0, 150.0, 52.0, "Open questions", json!({ "shapeStyle": "mind-node", "strokeColor": "purple" }), None);
    let branch = |color: &str| json!({ "connectorRole": "mind-map-branch", "connectorType": "curved", "endArrowhead": "none", "mindMapBranchAxis": "horizontal", "strokeColor": color });
    b.connect("topic", "research", "east", "west", branch("purple"));
    b.connect("topic", "ideas", "east", "west", branch("blue"));
    b.connect("research", "questions", "east", "west", branch("purple"));
    b.finish()
}

fn flowchart() -> Vec<Value> {
    let mut b = Builder::new(920.0, 520.0, true);
    b.title("Process flow");
    b.node("start", 40.0, 220.0, 130.0, 64.0, "Start", json!({ "fillColor": "green", "shapeType": "pill", "strokeColor": "green" }), None);
    b.node(
        "process",
        220.0,
        205.0,
        180.0,
        92.0,
        "Collect requirements",
        json!({ "fillColor": "blue", "shapeStyle": "flow-node", "strokeColor": "blue" }),
        None,
    );
    b.node(
        "decision",
        465.0,
        190.0,
        140.0,
        120.0,
        "Ready to proceed?",
        json!({ "fillColor": "yellow", "shapeStyle": "flow-node", "shapeType": "diamond", "strokeColor": "orange" }),
        None,
    );
    b.node("revise", 460.0, 370.0, 150.0, 72.0, "Revise plan", json!({ "fillColor": "red", "shapeStyle": "flow-node", "strokeColor": "red" }), None);
    b.node("finish", 700.0, 220.0, 150.0, 64.0, "Deliver", json!({ "fillColor": "purple", "shapeType": "pill", "strokeColor": "purple" }), None);
    b.text(628.0, 211.0, "Yes", 15.0, "green", None);
    b.text(548.0, 348.0, "No", 15.0, "red", None);
    let elbow = || json!({ "connectorType": "elbow" });
    b.connect("start", "process", "east", "west", elbow());
    b.connect("process", "decision", "east", "west", elbow());
    b.connect("decision", "finish", "east", "west", elbow());
    b.connect("decision", "revise", "south", "north", elbow());
    b.connect("revise", "process", "west", "south", elbow());
    b.finish()
}

fn roadmap() -> Vec<Value> {
    let mut b = Builder::new(960.0, 560.0, true);
    b.title("Project roadmap");
    let phases: [(&'static str, &'static str, f64, &str, &str, &str); 4] = [
        ("discover", "discover-milestone", 45.0, "blue", "Discover", "Problem and audience defined"),
        ("prototype", "prototype-milestone", 275.0, "purple", "Prototype", "Core experience tested"),
        ("validate", "validate-milestone", 505.0, "orange", "Validate", "Beta feedback collected"),
        ("launch", "launch-milestone", 735.0, "green", "Launch", "Release and measure"),
    ];
    for (index, (key, milestone, x, color, name, goal)) in phases.iter().enumerate() {
        b.text(*x, 118.0, &format!("Q{}", index + 1), 16.0, color, None);
        b.node(key, *x, 145.0, 180.0, 150.0, name, json!({ "fillColor": color, "strokeColor": color }), None);
        b.node(milestone, *x, 360.0, 180.0, 90.0, goal, json!({ "fillColor": color, "shapeStyle": "sticky", "strokeColor": color }), None);
        b.connect(key, milestone, "south", "north", json!({ "endArrowhead": "dot" }));
    }
    b.connect("discover", "prototype", "east", "west", json!({}));
    b.connect("prototype", "validate", "east", "west", json!({}));
    b.connect("validate", "launch", "east", "west", json!({}));
    b.finish()
}

/// Three columns of sticky notes (kanban and retrospective).
fn columns(title: &str, heads: [(&str, &str); 3], notes: [[(&'static str, &str, &str); 2]; 3], height: f64) -> Vec<Value> {
    let mut b = Builder::new(960.0, height, true);
    b.title(title);
    for (column, ((head, color), notes)) in heads.iter().zip(notes.iter()).enumerate() {
        let x = 30.0 + 310.0 * column as f64;
        let frame = b.frame(x, 82.0, 280.0, height - 120.0);
        b.text(x + 20.0, 122.0, head, 18.0, color, Some(&frame));
        for (row, (key, text, note)) in notes.iter().enumerate() {
            b.node(
                key,
                x + 20.0,
                150.0 + 134.0 * row as f64,
                240.0,
                112.0,
                text,
                json!({ "fillColor": note, "shapeStyle": "sticky", "strokeColor": if *note == "yellow" { "orange" } else { *note } }),
                Some(&frame),
            );
        }
    }
    b.finish()
}

fn kanban() -> Vec<Value> {
    columns(
        "Team kanban",
        [("To do", "ink"), ("In progress", "ink"), ("Done", "ink")],
        [
            [("scope", "Define release scope", "yellow"), ("content", "Draft launch content", "blue")],
            [("build", "Build interactive prototype", "purple"), ("testing", "Run user testing", "orange")],
            [("kickoff", "Project kickoff", "green"), ("research", "Customer research", "green")],
        ],
        600.0,
    )
}

fn retrospective() -> Vec<Value> {
    columns(
        "Meeting retrospective",
        [("Went well", "green"), ("Could improve", "orange"), ("Next steps", "purple")],
        [
            [("goal", "The goal stayed clear", "green"), ("feedback", "Feedback arrived quickly", "green")],
            [("decisions", "Decisions came too late", "orange"), ("owners", "Some owners were unclear", "red")],
            [("assign", "Assign one owner per action", "purple"), ("review", "Review progress next week", "blue")],
        ],
        580.0,
    )
}

fn storyboard() -> Vec<Value> {
    let mut b = Builder::new(1000.0, 650.0, true);
    b.title("Slide storyboard");
    let slides: [(&'static str, &str, f64, f64); 6] = [
        ("slide-1", "Opening", 35.0, 85.0),
        ("slide-2", "Problem", 355.0, 85.0),
        ("slide-3", "Key insight", 675.0, 85.0),
        ("slide-4", "Solution", 35.0, 370.0),
        ("slide-5", "Evidence", 355.0, 370.0),
        ("slide-6", "Next step", 675.0, 370.0),
    ];
    for (index, (key, name, x, y)) in slides.iter().enumerate() {
        let frame = b.frame(*x, *y, 290.0, 235.0);
        b.text(x + 18.0, y + 34.0, name, 17.0, "ink", Some(&frame));
        b.text(x + 250.0, y + 34.0, &format!("{:02}", index + 1), 13.0, "gray", Some(&frame));
        let color = if index % 2 == 0 { "purple" } else { "blue" };
        b.node(
            key,
            x + 18.0,
            y + 55.0,
            254.0,
            150.0,
            "Visual placeholder",
            json!({ "fillColor": color, "shapeStyle": "image-placeholder", "strokeColor": color }),
            Some(&frame),
        );
    }
    b.finish()
}

/// A template's elements as a clipboard payload, its top-left at the origin.
pub fn payload(template: &str) -> Option<Value> {
    let items = match template {
        "mindmap" => mind_map(),
        "flowchart" => flowchart(),
        "roadmap" => roadmap(),
        "kanban" => kanban(),
        "retrospective" => retrospective(),
        "storyboard" => storyboard(),
        _ => return None,
    };
    Some(json!({ "type": PAYLOAD_TYPE, "version": PAYLOAD_VERSION, "elements": items }))
}

/// `template.insert { page?, template, at? }`: the template centred on `at` (the page's content's
/// right when absent), one undo step.
pub fn insert(model: &BoardModel, tx: &mut Tx<'_>, ctx: &Ctx<'_>, params: &Value) -> Result<Value> {
    let name = params::string(params, "template")?;
    let (_, w, h) = TEMPLATES
        .iter()
        .find(|(id, _, _)| *id == name)
        .copied()
        .ok_or_else(|| invalid(format!("`template` must be one of {}", TEMPLATES.iter().map(|(id, _, _)| *id).collect::<Vec<_>>().join(", "))))?;
    let page = params::page(model, params, ctx.active_page)?;
    let center = match params::opt_point(params, "at")? {
        Some(at) => at,
        None => model.content_bounds(page).map_or(kition_board_kdm::Point::new(w / 2.0 + 40.0, h / 2.0 + 40.0), |bounds| {
            kition_board_kdm::Point::new(bounds.x1 + 80.0 + w / 2.0, bounds.y0 + h / 2.0)
        }),
    };
    let payload = payload(name).ok_or_else(|| invalid("unknown template"))?;
    // The paste puts the template's top-left element at `at`: a template without an outer frame
    // starts inside its own box.
    let corner = payload
        .get("elements")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("position").and_then(kition_board_kdm::Point::parse_object))
        .fold((f64::INFINITY, f64::INFINITY), |(x, y), point| (x.min(point.x), y.min(point.y)));
    let corner = if corner.0.is_finite() && corner.1.is_finite() { corner } else { (0.0, 0.0) };
    let mut paste = Map::new();
    paste.insert("page".into(), json!(page.id.as_str()));
    paste.insert("payload".into(), payload);
    paste.insert("at".into(), json!([center.x - w / 2.0 + corner.0, center.y - h / 2.0 + corner.1]));
    let mut answer = clipboard::paste(model, tx, ctx, &Value::Object(paste))?;
    if let Some(object) = answer.as_object_mut() {
        object.insert("template".into(), json!(name));
    }
    Ok(answer)
}
