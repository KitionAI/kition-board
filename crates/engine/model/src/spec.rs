//! The command table (design note 5.1): id, title, menu, shortcut, whether it changes the board,
//! and the parameters as a DeepSeek Harness `ParameterSchemaSpec` (requirements E7, A5). Commands
//! with a schema are offered to agents; editor-only commands have none.

use kition_kit::KitCommand;
use serde_json::{Value, json};

use crate::fields::{
    ARROWHEADS, COLOR_TOKENS, CONNECTOR_TYPES, DASH_STYLES, FILL_STYLES, FONT_FAMILIES, MIND_MAP_DIRECTIONS, SHAPE_STYLES, SHAPE_TYPES, STROKE_SIZES,
    TEXT_ALIGNS,
};

/// The bounds of `shape.previews`'s `size` (a picture's width in pixels) and its default.
pub const PREVIEW_SIZES: (f64, f64) = (8.0, 128.0);
pub const PREVIEW_SIZE: f64 = 28.0;

/// One command of the table.
pub struct Spec {
    pub id: &'static str,
    pub title: &'static str,
    pub menu: Option<&'static [&'static str]>,
    pub shortcut: Option<&'static str>,
    pub mutates: bool,
    /// Offered to agents (it has a parameter schema).
    pub agent: bool,
    /// Reads workspace files through the host (`kition-board`'s SANDBOXED list).
    pub reads_files: bool,
}

const fn spec(id: &'static str, title: &'static str, menu: Option<&'static [&'static str]>) -> Spec {
    Spec { id, title, menu, shortcut: None, mutates: true, agent: true, reads_files: false }
}

const EDIT: Option<&'static [&'static str]> = Some(&["Edit"]);
const INSERT: Option<&'static [&'static str]> = Some(&["Insert"]);
const ARRANGE: Option<&'static [&'static str]> = Some(&["Arrange"]);
const ALIGN: Option<&'static [&'static str]> = Some(&["Arrange", "Align"]);
const VIEW: Option<&'static [&'static str]> = Some(&["View"]);

/// Every command, in menu order.
pub const COMMANDS: &[Spec] = &[
    Spec { shortcut: Some("Cmd+Z"), ..spec("edit.undo", "Undo", EDIT) },
    Spec { shortcut: Some("Cmd+Shift+Z"), ..spec("edit.redo", "Redo", EDIT) },
    Spec { shortcut: Some("Cmd+D"), ..spec("element.duplicate", "Duplicate", EDIT) },
    Spec { shortcut: Some("Delete"), ..spec("element.delete", "Delete", EDIT) },
    Spec { mutates: false, ..spec("clipboard.copy", "Copy", None) },
    spec("element.paste", "Paste", None),
    spec("element.create", "Create elements", None),
    spec("element.update", "Update an element", None),
    spec("element.move", "Move", None),
    spec("element.resize", "Resize", None),
    spec("element.rotate", "Rotate", ARRANGE),
    spec("element.style", "Style", None),
    Spec { mutates: false, ..spec("element.getStyle", "Read style", None) },
    Spec { mutates: false, ..spec("shape.previews", "Shape library", None) },
    spec("element.setText", "Edit text", None),
    spec("text.mark", "Format text", None),
    spec("frame.create", "Frame", INSERT),
    spec("stroke.create", "Draw a stroke", None),
    spec("connector.create", "Connector", INSERT),
    spec("template.insert", "Template", None),
    Spec { reads_files: true, ..spec("card.create", "Card", INSERT) },
    Spec { reads_files: true, ..spec("image.insert", "Picture", INSERT) },
    spec("connector.bind", "Attach connector end", None),
    spec("connector.setLabel", "Connector label", None),
    spec("connector.setWaypoints", "Connector bends", None),
    Spec { reads_files: true, ..spec("card.setTarget", "Change card target", None) },
    spec("image.setAlt", "Alternative text", None),
    spec("frame.fit", "Fit frame to content", ARRANGE),
    Spec { shortcut: Some("Cmd+G"), ..spec("element.group", "Group", ARRANGE) },
    Spec { shortcut: Some("Cmd+Shift+G"), ..spec("element.ungroup", "Ungroup", ARRANGE) },
    spec("element.reorder", "Order", ARRANGE),
    spec("element.lock", "Lock", ARRANGE),
    spec("layout.align", "Align", ALIGN),
    spec("layout.distribute", "Distribute", ALIGN),
    spec("layout.stack", "Stack", ALIGN),
    spec("layout.mindmap", "Lay out mind map", None),
    spec("mindmap.addChild", "Add branch", None),
    spec("mindmap.addSibling", "Add sibling", None),
    spec("mindmap.collapse", "Collapse branch", None),
    spec("mindmap.setDirection", "Mind map direction", None),
    spec("page.create", "New page", VIEW),
    spec("page.rename", "Rename page", VIEW),
    spec("page.duplicate", "Duplicate page", VIEW),
    spec("page.delete", "Delete page", VIEW),
    spec("page.move", "Move page", VIEW),
    Spec { mutates: false, ..spec("page.activate", "Show page", None) },
    Spec { mutates: false, ..spec("file.export", "Export", None) },
    Spec { mutates: false, ..spec("file.render", "Preview a view", None) },
    spec("links.retarget", "Retarget links", None),
];

/// The command `id`.
pub fn find(id: &str) -> Option<&'static Spec> {
    COMMANDS.iter().find(|spec| spec.id == id)
}

fn text(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn required(mut node: Value) -> Value {
    if let Some(object) = node.as_object_mut() {
        object.insert("required".into(), json!(true));
    }
    node
}

fn choice(values: &[&str], description: &str) -> Value {
    json!({ "type": "string", "enum": values, "description": description })
}

fn ids(description: &str) -> Value {
    json!({ "type": "array", "items": { "type": "string" }, "description": description })
}

fn number(minimum: f64, maximum: f64, description: &str) -> Value {
    json!({"type":"number", "minimum":minimum, "maximum":maximum, "description":description})
}

/// A point the engine reads with `params::point`: `[x, y]` or `{ "x", "y" }` (a size, `[w, h]` or
/// `{ "w", "h" }`, when the description starts with `[w, h]`).
fn pair(description: &str) -> Value {
    let (a, b) = if description.starts_with("[w, h]") { ("w", "h") } else { ("x", "y") };
    let (min, max) = if a == "w" { (0.0, crate::params::MAX_DIMENSION) } else { (-crate::params::MAX_COORDINATE, crate::params::MAX_COORDINATE) };
    bounded_pair(a, b, min, max, description)
}

fn bounded_pair(a: &str, b: &str, min: f64, max: f64, description: &str) -> Value {
    let item = number(min, max, "One component");
    json!({
        "oneOf": [
            { "type": "array", "items": item },
            { "type": "object", "additionalProperties": false, "properties": { a: required(item.clone()), b: required(item.clone()) } },
        ],
        "description": description,
    })
}

fn points(description: &str) -> Value {
    json!({ "type": "array", "items": { "type": "array", "items": number(-crate::params::MAX_COORDINATE, crate::params::MAX_COORDINATE, "Coordinate") }, "description": description })
}

fn page() -> Value {
    text("The page's id; the shown page when absent")
}

fn color(description: &str) -> Value {
    text(&format!("{description}: one of {} or #rrggbb", COLOR_TOKENS.join(", ")))
}

fn style_properties() -> Value {
    json!({
        "strokeColor": color("Line or text colour"),
        "fillColor": color("Fill colour"),
        "fillStyle": choice(FILL_STYLES, "How the shape is filled"),
        "dashStyle": choice(DASH_STYLES, "Line dashes"),
        "strokeSize": choice(STROKE_SIZES, "Line weight"),
        "opacity": number(0.0, 1.0, "0 to 1"),
        "roundness": number(0.0, crate::params::MAX_DIMENSION, "Corner radius in board units"),
        "fontSize": number(1.0, 1000.0, "Text size in board units"),
        "fontFamily": choice(FONT_FAMILIES, "Typeface"),
        "textAlign": choice(TEXT_ALIGNS, "Text alignment"),
        "connectorType": choice(CONNECTOR_TYPES, "Connector route"),
        "startArrowhead": choice(ARROWHEADS, "Arrowhead at the start"),
        "endArrowhead": choice(ARROWHEADS, "Arrowhead at the end"),
    })
}

/// The style fields a connector takes (`connectors::create` refuses the others).
fn connector_style() -> Value {
    let all = style_properties();
    let properties: serde_json::Map<String, Value> =
        ["strokeColor", "strokeSize", "dashStyle", "opacity", "connectorType", "startArrowhead", "endArrowhead"]
            .into_iter()
            .filter_map(|field| Some((field.to_owned(), all.get(field)?.clone())))
            .collect();
    json!({ "type": "object", "additionalProperties": false, "description": "The connector's style", "properties": properties })
}

fn end(description: &str) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": format!("{description}: {{ element, anchor? }} to attach to an element (anchor [0..1, 0..1] on its box, the nearest edge when absent), or {{ point: [x, y] }}"),
        "properties": {
            "element": text("The element to attach to"),
            "anchor": pair("[x, y] on the element's box, clamped to 0 to 1"),
            "point": pair("A free end at [x, y]"),
        },
    })
}

/// A pasted connector end: `binding.target` names an element (of the payload or of the board).
fn end_binding() -> Value {
    json!({ "type": "object", "additionalProperties": true, "properties": {
        "binding": { "type": "object", "additionalProperties": true, "properties": { "target": text("The element the end is bound to") } },
    } })
}

fn element_spec() -> Value {
    let mut properties = json!({
        "kind": required(choice(&["shape", "text", "frame", "stroke"], "What to make (connectors, pictures, cards and groups have their own commands)")),
        "position": pair("[x, y] of the top-left corner, in board units (shape, text, frame)"),
        "size": pair("[w, h] (shape 160 × 100 and frame 640 × 400 by default; text sizes itself)"),
        "text": text("The label, text or frame title"),
        "points": points("A stroke's points, at least two"),
        "rotation": number(-3600.0, 3600.0, "Degrees about the centre"),
        "parent": text("A frame or group to put it in"),
        "key": text("An authoring id, unique on the page"),
        "shapeType": choice(SHAPE_TYPES, "The shape (rectangle by default); a line runs along its box's longer side (rotation turns it)"),
        "shapeStyle": choice(SHAPE_STYLES, "The shape's role"),
        "textFormat": choice(&["plain", "markdown"], "How the text is written"),
    });
    if let (Some(target), Some(style)) = (properties.as_object_mut(), style_properties().as_object()) {
        for (key, value) in style {
            target.insert(key.clone(), value.clone());
        }
    }
    json!({ "type": "object", "additionalProperties": false, "properties": properties })
}

fn update_properties() -> Value {
    let mut properties = style_properties();
    if let Some(object) = properties.as_object_mut() {
        object.insert("position".into(), pair("[x, y] in board units"));
        object.insert("size".into(), pair("[w, h] in board units"));
        object.insert("rotation".into(), number(-3600.0, 3600.0, "Degrees about the centre"));
        object.insert("labelPosition".into(), number(0.0, 1.0, "Where along the route"));
        object.insert("points".into(), points("Stroke points"));
        object.insert("parent".into(), text("A frame or group UUID"));
        for schema in object.values_mut() {
            *schema = json!({"oneOf":[schema.clone(), {"type":"null"}]});
        }
    }
    properties
}

/// The `ParameterSchemaSpec` of command `id` (`None` for editor-only commands).
pub fn schema(id: &str) -> Option<Value> {
    let schema = match id {
        // Whose step (KitModel M12a, engine rule E14a): by default the scope of whoever runs it.
        "edit.undo" => {
            json!({ "scope": choice(&["person", "agent"], "Whose last step to undo: the person's or the agent's. By default the scope of whoever runs it (an agent undoes only its own steps)") })
        }
        "edit.redo" => {
            json!({ "scope": choice(&["person", "agent"], "Whose last undone step to redo: the person's or the agent's. By default the scope of whoever runs it") })
        }
        "element.create" => json!({
            "page": page(),
            "elements": required(json!({ "type": "array", "items": element_spec(), "description": "The elements to make, back to front" })),
        }),
        "element.update" => json!({
            "id": required(text("The element")),
            "set": required(json!({ "type": "object", "additionalProperties": true, "properties": update_properties(), "description": "Fields to set: position [x, y], size [w, h], rotation, parent, key, points, shapeType, shapeStyle, style fields, labelPosition, mind-map fields; null clears an optional field" })),
        }),
        "element.delete" => json!({ "ids": required(ids("The elements; what is inside frames and groups goes with them")) }),
        "element.move" => json!({
            "ids": required(ids("The elements; what is inside them moves along")),
            "delta": pair("[dx, dy] to move by"),
            "to": pair("[x, y] for the selection's top-left corner (instead of delta)"),
            "reparent": { "type": "boolean", "description": "Each moved element takes the frame its centre lands in (and leaves a frame it is moved out of)" },
        }),
        "element.resize" => json!({
            "id": text("The element (or ids)"),
            "ids": ids("Elements to scale together (instead of id)"),
            "box": required(json!({ "type": "array", "items": number(-crate::params::MAX_COORDINATE, crate::params::MAX_COORDINATE, "Coordinate"), "description": "The new box [x0, y0, x1, y1]: the element's unrotated box, or the selection's bounds with ids" })),
        }),
        "element.rotate" => json!({
            "ids": required(ids("The elements")),
            "degrees": required(number(-3600.0, 3600.0, "Degrees, clockwise")),
            "about": choice(&["each", "selection"], "Each about its own centre (default) or all about the selection's centre"),
        }),
        "element.style" => json!({
            "ids": required(ids("The elements; each takes the fields its kind uses")),
            "style": required(json!({ "type": "object", "additionalProperties": false, "properties": style_properties() })),
        }),
        "element.getStyle" => json!({
            "ids": ids("The elements (instead of kind)"),
            "kind": choice(&["shape", "text", "stroke", "connector", "image", "card", "frame"], "A new element's kind, for its defaults (instead of ids)"),
            "shapeStyle": choice(SHAPE_STYLES, "The new shape's role"),
        }),
        "shape.previews" => json!({
            "size": number(PREVIEW_SIZES.0, PREVIEW_SIZES.1, "Each picture's width in pixels, 8 to 128 (28 by default); its height is three quarters of it"),
        }),
        "element.setText" => json!({ "id": required(text("A shape, text, connector, card or frame")), "text": required(text("The whole new text")) }),
        "text.mark" => json!({
            "id": required(text("The element")),
            "range": required(json!({ "type": "array", "items": { "type": "integer", "minimum": 0 }, "description": "[start, end) in characters" })),
            "mark": required(choice(&["bold", "italic", "link"], "The mark")),
            "value": json!({ "type": "json", "description": "true (bold, italic), { href } (link), or null to remove" }),
            "units": choice(&["scalar", "utf16"], "How range counts: characters (default) or UTF-16 code units"),
        }),
        "element.duplicate" => json!({ "ids": required(ids("The elements")), "offset": pair("[dx, dy] of the copies (24, 24 by default)") }),
        "element.paste" => json!({
            "page": page(),
            "payload": required(json!({ "type": "object", "additionalProperties": true, "description": "A clipboard.copy payload (application/x-kition-board+json)", "properties": {
                "elements": { "type": "array", "description": "The copied elements", "items": { "type": "object", "additionalProperties": true, "properties": {
                    "ref": text("The copied element's id"),
                    "start": end_binding(),
                    "end": end_binding(),
                } } },
            } })),
            "at": pair("[x, y] for the copies' top-left corner"),
        }),
        "element.reorder" => json!({
            "ids": required(ids("The elements")),
            "placement": required(choice(&["front", "forward", "backward", "back"], "Where in the stacking order")),
        }),
        "element.lock" => json!({ "ids": required(ids("The elements")), "locked": required(json!({ "type": "boolean" })) }),
        "element.group" => json!({ "ids": required(ids("The elements to group")) }),
        "element.ungroup" => json!({ "groupIds": required(ids("The groups")) }),
        "frame.create" => json!({
            "page": page(),
            "box": required(json!({ "type": "array", "items": number(-crate::params::MAX_COORDINATE, crate::params::MAX_COORDINATE, "Coordinate"), "description": "The frame's box [x0, y0, x1, y1]" })),
            "title": text("The frame's title"),
            "children": ids("Elements to put in the frame"),
        }),
        "frame.fit" => {
            json!({ "id": required(text("The frame")), "padding": number(0.0, 10000.0, "Space around the content (32 by default)") })
        }
        "stroke.create" => json!({
            "page": page(),
            "points": required(points("The stroke's points, at least two")),
            "style": json!({ "type": "object", "additionalProperties": false, "properties": {
                "strokeColor": color("Ink colour"), "strokeSize": choice(STROKE_SIZES, "Line weight"), "dashStyle": choice(DASH_STYLES, "Solid (default), dashed or dotted"),
                "opacity": number(0.0, 1.0, "0 to 1") } }),
        }),
        "connector.create" => json!({
            "page": page(),
            "from": required(end("The start")),
            "to": required(end("The end")),
            "label": text("The label"),
            "type": choice(CONNECTOR_TYPES, "The route"),
            "startArrowhead": choice(ARROWHEADS, "Arrowhead at the start (none by default)"),
            "endArrowhead": choice(ARROWHEADS, "Arrowhead at the end (arrow by default)"),
            "waypoints": points("Bends in path order"),
            "key": text("An authoring id, unique on the page"),
            "style": connector_style(),
        }),
        "connector.bind" => json!({
            "id": required(text("The connector")),
            "terminal": required(choice(&["start", "end"], "Which end")),
            "to": required(end("Where the end goes")),
        }),
        "connector.setLabel" => json!({
            "id": required(text("The connector")),
            "text": required(text("The label; empty removes it")),
            "position": number(0.0, 1.0, "Where along the route, 0 to 1 (0.5 by default)"),
        }),
        "connector.setWaypoints" => {
            json!({ "id": required(text("The connector")), "waypoints": required(points("Bends in path order; empty for an automatic route")) })
        }
        "card.create" | "card.setTarget" => {
            let mut properties = json!({
                "file": text("A workspace file, relative to the board's folder or rooted with /"),
                "url": text("An http, https or mailto address (instead of file)"),
                "kind": choice(&["note", "table", "board", "file", "link"], "The card's kind (from the file's extension when absent)"),
                "view": json!({ "type": "object", "additionalProperties": true, "description": "Where in the target: { heading: [...] }, { blockId }, { page }" }),
                "title": text("A link card's title"),
            });
            if let Some(object) = properties.as_object_mut() {
                if id == "card.create" {
                    object.insert("page".into(), page());
                    object.insert("position".into(), pair("[x, y]; right of the content when absent"));
                    object.insert("size".into(), pair("[w, h]; 320 × 200 by default"));
                    object.insert("caption".into(), text("A caption under the card"));
                } else {
                    object.insert("id".into(), required(text("The card")));
                }
            }
            properties
        }
        "image.insert" => json!({
            "page": page(),
            "file": text("A workspace picture (PNG, JPEG, GIF, WebP, SVG), read once into the board"),
            "data": text("The picture's bytes as base64 (instead of file)"),
            "mime": text("The picture's type, checked against its bytes"),
            "position": pair("[x, y]; right of the content when absent"),
            "size": pair("[w, h]; from the picture, at most 800 wide, when absent"),
            "alt": text("Alternative text"),
        }),
        "image.setAlt" => json!({ "id": required(text("The picture")), "alt": required(text("Alternative text; empty removes it")) }),
        "layout.align" => json!({
            "ids": required(ids("The elements")),
            "edge": required(choice(&["left", "center", "right", "top", "middle", "bottom"], "What to line up")),
        }),
        "layout.distribute" => {
            json!({ "ids": required(ids("Three or more elements")), "axis": required(choice(&["horizontal", "vertical"], "Direction")) })
        }
        "layout.stack" => json!({
            "ids": required(ids("The elements")),
            "axis": required(choice(&["horizontal", "vertical"], "Direction")),
            "gap": number(0.0, 10000.0, "Space between (24 by default)"),
        }),
        "template.insert" => json!({
            "page": page(),
            "template": required(choice(&["mindmap", "flowchart", "roadmap", "kanban", "retrospective", "storyboard"], "The starting structure")),
            "at": pair("[x, y] for the template's centre; right of the content when absent"),
        }),
        "clipboard.copy" => json!({
            "ids": required(ids("The elements to copy; the answer is a board clipboard payload for element.paste")),
        }),
        "page.activate" => json!({
            "page": required(text("The id of the page to show (view state, not an undo step)")),
        }),
        "links.retarget" => json!({
            "from": required(text("The workspace path cards link to now, a file or a folder (KitLinks LK2)")),
            "to": required(text("The workspace path they link to instead; a folder's cards keep the rest of their path")),
        }),
        "file.export" => json!({
            "format": required(choice(&["png", "svg", "pdf"], "The file to write")),
            "page": page(),
            "frame": text("One frame and what is in it, instead of the whole page"),
            "scale": number(0.1, 8.0, "PNG pixels per board unit, 0.1 to 8 (2 by default)"),
            "background": { "type": "boolean", "description": "A white page behind the board (true by default; PDF pages are always white)" },
            "fit": { "type": "boolean", "description": "A PNG too large at its scale is drawn at the largest scale that fits instead of refused" },
        }),
        "file.render" => json!({
            "target": {
                "type": "object",
                "additionalProperties": false,
                "description": "The view to draw; without it the first page's document area. At most one of frame, objectId and area",
                "properties": {
                    "page": { "oneOf": [{ "type": "string" }, { "type": "integer", "minimum": 1 }], "description": "The page's id (or its number counted from 1, as diagnostics give it); the first page when absent" },
                    "frame": text("A frame's id: its box"),
                    "objectId": text("An element's id: its bounds with a margin of 16 units"),
                    "area": { "type": "array", "items": { "type": "number" }, "description": "[x0, y0, x1, y1] in board units on the page" },
                },
            },
            "scale": { "type": "number", "maximum": 4, "description": "Pixels per board unit, more than 0 and at most 4 (1 by default)" },
            "maxSize": { "type": "integer", "minimum": 16, "maximum": 8192, "description": "The longest side in pixels, 16 to 8192; the scale is brought down to fit" },
        }),
        "layout.mindmap" => json!({ "root": required(text("Any node of the mind map")) }),
        "mindmap.addChild" => json!({
            "node": required(text("The node to branch from")),
            "text": text("The new node's text"),
            "side": choice(&["left", "right"], "Which side of the root, for a map that grows both ways"),
        }),
        "mindmap.addSibling" => json!({ "node": required(text("A node other than the root")), "text": text("The new node's text") }),
        "mindmap.collapse" => json!({
            "node": required(text("A node with branches, not the root")),
            "collapsed": { "type": "boolean", "description": "Hide (true) or show (false) what is below; toggles when absent" },
        }),
        "mindmap.setDirection" => json!({
            "node": required(text("Any node of the mind map")),
            "direction": required(choice(MIND_MAP_DIRECTIONS, "Where the map grows")),
        }),
        "page.create" => {
            json!({
                "name": text("The name (Page N by default)"),
                "at": { "type": "integer", "minimum": 0, "description": "Position among the pages (last by default); at most the current page count" },
                "activate": { "type": "boolean", "description": "Make it the page being edited (false by default)" },
            })
        }
        "page.rename" => json!({ "page": required(text("The page")), "name": required(text("The new name")) }),
        "page.duplicate" => json!({ "page": required(text("The page")), "name": text("The copy's name") }),
        "page.delete" => json!({ "page": required(text("The page; the last page stays")) }),
        "page.move" => json!({
            "page": required(text("The page")),
            "to": { "type": "integer", "minimum": 0, "description": "The new position, less than the current page count" },
            "placement": choice(&["previous", "next"], "One step (instead of to)"),
        }),
        _ => return None,
    };
    Some(schema)
}

/// What each command does, for people and models choosing a command (the SDK's `check_commands`).
pub fn description(id: &str) -> &'static str {
    match id {
        "edit.undo" => {
            "Takes back the last step of one scope: the person's or the agent's (by default the caller's own), leaving the other scope's later steps in place."
        }
        "edit.redo" => {
            "Puts back the last undone step of one scope (by default the caller's own); a new edit in that scope clears what can be redone."
        }
        "element.duplicate" => {
            "Copies the elements (with what is inside frames and groups) on the same page, offset by 24 units unless offset says otherwise; answers the copies' ids."
        }
        "element.delete" => {
            "Removes the elements and everything inside frames and groups among them; connectors attached to them lose that end's binding."
        }
        "clipboard.copy" => {
            "Reads the elements into a board clipboard payload without changing the board; pass the payload to element.paste to place copies on any page."
        }
        "element.paste" => {
            "Places copies of a clipboard.copy payload on a page, at `at` or offset from the originals; answers the new elements' ids."
        }
        "element.create" => {
            "Makes one or more elements (shapes, text, sticky notes, strokes, frames, connectors, cards) on a page in one step; use it to build content, element.update to change existing elements."
        }
        "element.update" => {
            "Sets fields of one existing element (position, size, rotation, parent, shape type and style, text placement, mind-map fields); null clears an optional field."
        }
        "element.move" => {
            "Moves the elements by delta or so the selection's top-left lands at `to`; with reparent each element joins the frame its centre lands in."
        }
        "element.resize" => "Gives one element, or several scaled together, a new box [x0, y0, x1, y1] in board units.",
        "element.rotate" => "Rotates the elements by degrees clockwise, each about its own centre or all about the selection's centre.",
        "element.style" => {
            "Sets style fields (colours, fill, stroke, dash, font, size, alignment, opacity, roundness) on the elements; each element takes only the fields its kind uses."
        }
        "element.getStyle" => {
            "Reads the style fields the elements use, or a new element's defaults for a kind, without changing anything; null marks a field the elements differ on. Also answers the colour palette."
        }
        "shape.previews" => {
            "Answers the shape library without changing anything: the shapeType values in their four groups (basic, flowchart, arrows, marks), each with a small SVG picture the engine draws of it."
        }
        "element.setText" => "Replaces the whole text of a shape, text element, connector label, card caption or frame title.",
        "text.mark" => "Applies or removes bold, italic or a link on a character range of one element's text.",
        "frame.create" => "Makes a frame with a box and title and moves the named elements into it; frames group content and export as one area.",
        "stroke.create" => "Draws a freehand stroke through at least two points, with optional ink colour, weight and opacity.",
        "connector.create" => {
            "Draws a connector between two ends, each attached to an element or at a free point, with route type, arrowheads, label and bends; answers its id."
        }
        "template.insert" => {
            "Inserts a ready structure (mind map, flowchart, roadmap, kanban, retrospective, storyboard) centred at `at` or right of the content."
        }
        "card.create" => {
            "Places a card that links to a workspace file (note, table, board or any file) or a web address; the card shows the target's title and opens it."
        }
        "image.insert" => "Places a picture from a workspace file or base64 bytes (PNG, JPEG, GIF, WebP, SVG); the picture is stored in the board.",
        "connector.bind" => "Moves one end of an existing connector onto an element (at an anchor on its box) or to a free point.",
        "connector.setLabel" => "Sets or removes a connector's label and where along the route it sits (0 to 1).",
        "connector.setWaypoints" => "Replaces a connector's bend points; an empty list returns it to an automatic route.",
        "card.setTarget" => "Points an existing card at another workspace file or web address, keeping its place and size.",
        "image.setAlt" => "Sets or removes a picture's alternative text, read by screen readers and exports.",
        "frame.fit" => "Resizes a frame to wrap its content with padding (32 units by default).",
        "element.group" => "Groups the elements so they move, resize and select together; answers the group's id.",
        "element.ungroup" => "Dissolves the groups, leaving their elements in place and in the same order.",
        "element.reorder" => "Moves the elements to the front, one step forward, one step backward or to the back of the stacking order.",
        "element.lock" => "Locks or unlocks the elements; locked elements cannot be selected, moved or edited by the person until unlocked.",
        "layout.align" => "Lines up the elements' left, centre, right, top, middle or bottom edges.",
        "layout.distribute" => "Spaces three or more elements evenly between the outermost ones, horizontally or vertically.",
        "layout.stack" => "Places the elements one after another in a row or column with a fixed gap (24 by default).",
        "layout.mindmap" => "Re-lays out the whole mind map that a node belongs to, keeping the root in place.",
        "mindmap.addChild" => "Adds a branch node under a mind-map node (on a chosen side for two-way maps) and lays the map out again.",
        "mindmap.addSibling" => "Adds a node next to a non-root mind-map node, under the same parent.",
        "mindmap.collapse" => "Hides or shows the branches below a mind-map node (toggles when collapsed is absent).",
        "mindmap.setDirection" => "Sets which way the mind map grows (right, left, both ways, down) and lays it out again.",
        "page.create" => "Adds a new page with a name (Page N by default) at a position (last by default); answers its id.",
        "page.rename" => "Gives a page a new name; its id and elements stay the same.",
        "page.duplicate" => "Copies a page with all its elements, under a new name; answers the copy's id.",
        "page.delete" => "Deletes a page and its elements; the board's last page cannot be deleted.",
        "page.move" => "Moves a page to a new position among the pages, or one step earlier or later.",
        "page.activate" => "Shows a page in the editor; this is view state, not an undo step, and the document does not change.",
        "file.export" => {
            "Draws the shown page (or `page`, or one frame) as PNG, SVG or PDF and answers the file as base64 without changing the board."
        }
        "file.render" => {
            "Draws one view of the board as a PNG (a page, a frame, an element or an area, by id) for looking at it, without changing the board or the page the editor shows; the same call answers the same picture."
        }
        "links.retarget" => "Re-points every card that links to a moved or renamed workspace file or folder at its new path.",
        _ => "",
    }
}

/// What a reading command answers (KHP A1 `output.schema`), as a ValueSchemaSpec.
pub fn output(id: &str) -> Option<Value> {
    let schema = match id {
        "clipboard.copy" => json!({ "type": "object", "additionalProperties": false, "properties": {
            "type": { "type": "string", "required": true, "description": "application/x-kition-board+json" },
            "version": { "type": "integer", "required": true },
            "elements": { "type": "array", "required": true, "items": { "type": "json" }, "description": "The copied elements, for element.paste" },
        } }),
        "element.getStyle" => json!({ "type": "object", "additionalProperties": false, "properties": {
            "fields": { "type": "array", "required": true, "items": { "type": "string" }, "description": "The style fields the elements use, in element.style order" },
            "style": { "type": "object", "required": true, "additionalProperties": true, "description": "Each field's value; null where the elements differ" },
            "palette": { "type": "array", "required": true, "items": { "type": "object", "additionalProperties": false, "properties": {
                "token": { "type": "string", "required": true }, "stroke": { "type": "string", "required": true }, "fill": { "type": "string", "required": true } } } },
        } }),
        "shape.previews" => json!({ "type": "object", "additionalProperties": false, "properties": {
            "groups": { "type": "array", "required": true, "description": "The shape library's groups, in the order the editor shows them", "items": { "type": "object", "additionalProperties": false, "properties": {
                "id": { "type": "string", "required": true, "description": "basic, flowchart, arrows or marks" },
                "shapes": { "type": "array", "required": true, "items": { "type": "object", "additionalProperties": false, "properties": {
                    "shape": { "type": "string", "required": true, "description": "A shapeType value" },
                    "svg": { "type": "string", "required": true, "description": "The shape's outline as an SVG document, stroked in currentColor and not filled" } } } },
            } } },
        } }),
        "page.activate" => json!({ "type": "object", "additionalProperties": false, "properties": {
            "page": { "type": "string", "required": true, "description": "The page now shown" },
        } }),
        "file.render" => json!({ "type": "object", "additionalProperties": false, "properties": {
            "png": { "type": "string", "required": true, "description": "The picture, base64" },
            "mimeType": { "type": "string", "required": true },
            "width": { "type": "integer", "required": true },
            "height": { "type": "integer", "required": true },
            "scale": { "type": "number", "required": true, "description": "Pixels per board unit, after maxSize" },
            "page": { "type": "string", "required": true, "description": "The id of the page drawn" },
            "pageName": { "type": "string", "required": true },
            "area": { "type": "array", "required": true, "items": { "type": "number" }, "description": "[x0, y0, x1, y1]: what was drawn, in board units" },
            "frame": { "type": "string", "description": "The frame drawn, when the view named one" },
            "objectId": { "type": "string", "description": "The element drawn, when the view named one" },
        } }),
        "file.export" => json!({ "type": "object", "additionalProperties": false, "properties": {
            "format": { "type": "string", "required": true, "enum": ["png", "svg", "pdf"] },
            "mime": { "type": "string", "required": true },
            "extension": { "type": "string", "required": true },
            "base64": { "type": "string", "required": true, "description": "The file's bytes" },
            "width": { "type": "number", "required": true },
            "height": { "type": "number", "required": true },
        } }),
        _ => return None,
    };
    Some(schema)
}

/// The commands as the engine lists them; `can_undo`/`can_redo` enable Undo and Redo.
pub fn list(can_undo: bool, can_redo: bool) -> Vec<KitCommand> {
    COMMANDS
        .iter()
        .map(|spec| KitCommand {
            id: spec.id.into(),
            title: spec.title.into(),
            menu: spec.menu.map(|menu| menu.iter().map(|part| (*part).to_owned()).collect()),
            shortcut: spec.shortcut.map(str::to_owned),
            params: None,
            schema: if spec.agent { schema(spec.id) } else { None },
            output_schema: if spec.agent { output(spec.id) } else { None },
            description: description(spec.id).into(),
            keywords: Vec::new(),
            mutates: spec.mutates,
            enabled: match spec.id {
                "edit.undo" => can_undo,
                "edit.redo" => can_redo,
                _ => true,
            },
            reads: crate::acts::reads(spec.id),
            targets: if spec.agent { crate::acts::targets(spec.id) } else { None },
            objects: crate::acts::objects(spec.id),
            object_types: crate::acts::object_types(spec.id),
            places: match spec.id {
                "page.create" => &["at"],
                "page.move" => &["to", "placement"],
                "element.reorder" => &["placement"],
                "element.move" => &["reparent"],
                "links.retarget" => &["from"],
                _ => &[],
            },
            ..KitCommand::default()
        })
        .collect()
}

/// The commands that read workspace files through the host.
pub fn reading_files() -> Vec<&'static str> {
    COMMANDS.iter().filter(|spec| spec.reads_files).map(|spec| spec.id).collect()
}
