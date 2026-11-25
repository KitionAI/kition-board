//! The independent reading of every write command (task card C8-board-read): for each of the 43
//! commands the kit offers that change the board, its handler (`kition-board-model`, `dispatch`)
//! and the engine operations behind it (`edit.rs`, `params.rs`, `kdm-model`'s view and mind-map
//! topology) were read — by readers who did not open the declarations (`premises.rs`, the command
//! table's `reads`/`objects`/`places`) or their tests — for which parameters each resolves and
//! how, what it reads of the board and of the editor, and what it writes. Each command's reading
//! is the comment before its rows. Each row is one code path of the command, on [`fixture`], with
//! what a write with `basis` holds, computed as the guard does (kition-kits `tools/guard.rs`): the
//! editor state the command declares, the ids of its target parameter (every field; no position
//! when it is position-independent), every other id its parameters name (every field and
//! position), the objects its place names, each replaced by `under`'s premise on it; for a step
//! that names a place, the windows `under` answers and the fields its `covers` asks about.
//!
//! In a row an object's held fields are `*` (every field), `-` (none: only that it is there) or a
//! list; where it is (its `position` and `parent`, the fields the schema marks `position`, and its
//! index in its list, `elements` or `pages`) is written apart: `*` every one, `-` none. `kind` is
//! read-only (no change can touch it) and left out of both sides.
//!
//! The rows were written from the readings before the comparison; the rows under "Further code
//! paths" after the first comparison, before their own. Rows marked `pinned` were then taken from
//! the declaration after review (33 of the reading's 81): where the reading missed a read the code makes
//! (`roots` reads the selected elements' parents, the topology reads `shapeStyle`, a copy reads
//! `parent`), where it held what the code does not depend on (the index a new frame or group goes
//! in at follows its first member; a destination frame told by `covers`), where a fact has two
//! spellings (`~bounds` or the fields a text is measured from; `start.binding` inside `start`),
//! a delete (which replaces every field of what goes), and `edit.undo`/`edit.redo`, whose reading
//! (the session's history) names no object a row could hold. The `text.mark` and `image.setAlt`
//! rows hold the lock the engine fix made them check.
//!
//! Where the declarations before this card differed, the row says so (`was`). Of the 43
//! commands, 21 differed (49%): 3 held less than the handler reads (a write that can land on what
//! the agent did not observe), 20 held more (a refusal no change needs), 2 both. By class:
//! - held less, a read that is not held: an end element's `rotation` (`connector.add`,
//!   `connector.bind`: the edge an end faces turns with it, and a turn from 45 to 135 degrees
//!   keeps `~bounds`): 2;
//! - held less, a value naming another object: a pasted payload's refs and its connectors'
//!   bindings name live elements (`edit.paste`): 1;
//! - held more, a container held though not read (a lone element's `parent` and its ancestors'
//!   kind and parent; an end element's or a new container's own container; every mind-map node's
//!   container): 13 (`element.add`, `element.set`, `connector.add`, `connector.bind`,
//!   `element.move`, `element.resize`, `element.rotate`, `element.reorder`, the five map commands);
//! - held more, geometry beyond what is read (`size` and `rotation` beside `~bounds`, a box's
//!   bounds for a turn about its own centre, `fontSize` where nothing scales it): 5
//!   (`element.resize`, `element.rotate`, `layout.align`, `layout.distribute`, `layout.stack`);
//! - held more, every field where one is written (an ungroup's members): 1;
//! - held more, a board-wide summary where the objects' own premises suffice (`~structure` for a
//!   page copy or delete, and for a shape given a container): 3;
//! - held more, a window wider than what is read (`element.reorder`: the whole page): 1;
//! - held more, a check the handler did not make (`text.mark` held the lock it did not check;
//!   the engine now checks it): 1.
//!
//! No editor state differed (only the page is editor state, read by the eight commands that make
//! content); no parameter is resolved differently from its name. Found in the code but equal on
//! this fixture: `page.move` with `placement` held the whole page list (now the page and the
//! neighbour it passes), `frame.fit` its frame's ancestors.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use kition_board_kit::BoardDocument;
use kition_kit::{KitCommand, KitDocument};
use serde_json::{Value, json};

/// A 1 × 1 PNG.
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

fn created(answer: &Value) -> Vec<String> {
    answer["created"].as_array().unwrap_or_else(|| panic!("no created in {answer}")).iter().map(|id| id.as_str().unwrap().to_owned()).collect()
}

fn text_of(value: &Value) -> String {
    value.as_str().unwrap_or_else(|| panic!("not an id: {value}")).to_owned()
}

/// Page `$P`: shapes `$A`, `$B` joined by connector `$K` (bound at both ends), a text `$T`
/// without a stored size, frame `$F` holding `$C`, group `$G` of `$G1` and `$G2`, stroke `$S`,
/// image `$I`, a link card `$CARD` and a file card `$NOTE` (`notes.md`), a locked shape `$L`, and
/// a mind map: root `$M` with branches `$M1` (edge `$E1`, with `$M11` under it by `$E11`) and
/// `$M2` (edge `$E2`). Page `$Q` ("Other") holds `$Q1`.
pub fn fixture() -> (BoardDocument, Vec<(String, String)>) {
    let mut doc = BoardDocument::create("Read.kboard", 1920, 1080).unwrap();
    let mut roles: Vec<(String, String)> = Vec::new();
    let mut role = |name: &str, id: String| roles.push((name.to_owned(), id));
    let page = doc.model().pages()[0].id.as_str().to_owned();
    role("$P", page.clone());
    let made = created(
        &doc.execute(
            "element.add",
            json!({"page":page,"elements":[
                {"kind":"shape","position":[0,0],"text":"A"},
                {"kind":"shape","position":[300,0],"text":"B"},
                {"kind":"text","position":[600,0],"text":"Hello"},
                {"kind":"shape","position":[30,430],"text":"Child"},
                {"kind":"shape","position":[700,300],"text":"G1"},
                {"kind":"shape","position":[900,300],"text":"G2"},
                {"kind":"shape","position":[1500,0],"text":"Locked"},
                {"kind":"shape","shapeStyle":"mind-node","position":[0,1200],"size":[142,56],"text":"Root"}
            ]}),
        )
        .unwrap(),
    );
    for (name, id) in ["$A", "$B", "$T", "$C", "$G1", "$G2", "$L", "$M"].iter().zip(made.iter()) {
        role(name, id.clone());
    }
    let (a, b, c, g1, g2, l, m) = (&made[0], &made[1], &made[3], &made[4], &made[5], &made[6], &made[7]);
    let k = created(&doc.execute("connector.add", json!({"page":page,"from":{"element":a},"to":{"element":b}})).unwrap())[0].clone();
    role("$K", k);
    let f = text_of(&doc.execute("frame.add", json!({"page":page,"box":[0,400,500,700],"children":[c],"title":"Frame"})).unwrap()["frame"]);
    role("$F", f);
    let g = doc.execute("arrange.group", json!({"ids":[g1,g2]})).unwrap();
    role("$G", text_of(&g["group"]));
    let s = doc.execute("stroke.add", json!({"page":page,"points":[[0,900],[100,950]]})).unwrap();
    role("$S", text_of(&s["created"][0]));
    let i = doc.execute("image.insert", json!({"page":page,"data":PNG,"position":[1200,0],"size":[100,100]})).unwrap();
    role("$I", text_of(&i["created"][0]));
    let card = doc.execute("card.add", json!({"page":page,"url":"https://example.com","position":[1200,400]})).unwrap();
    role("$CARD", text_of(&card["created"][0]));
    let note = doc.execute("card.add", json!({"page":page,"file":"notes.md","position":[1200,700]})).unwrap();
    role("$NOTE", text_of(&note["created"][0]));
    doc.execute("element.lock", json!({"ids":[l],"locked":true})).unwrap();
    let mut m1 = String::new();
    for (node, edge) in [("$M1", "$E1"), ("$M2", "$E2")] {
        let added = doc.execute("mindmap.addChild", json!({"node":m,"text":node})).unwrap();
        if node == "$M1" {
            m1 = text_of(&added["node"]);
        }
        role(node, text_of(&added["node"]));
        role(edge, text_of(&added["created"][0]));
    }
    let added = doc.execute("mindmap.addChild", json!({"node":m1,"text":"M11"})).unwrap();
    role("$M11", text_of(&added["node"]));
    role("$E11", text_of(&added["created"][0]));
    let other = text_of(&doc.execute("page.add", json!({"name":"Other"})).unwrap()["page"]);
    role("$Q", other.clone());
    let q1 = created(&doc.execute("element.add", json!({"page":other,"elements":[{"kind":"shape","position":[0,0],"text":"Q1"}]})).unwrap());
    role("$Q1", q1[0].clone());
    let bytes = doc.save(None).unwrap();
    (BoardDocument::open("Read.kboard", &bytes).unwrap(), roles)
}

fn with_roles(value: &Value, roles: &[(String, String)]) -> Value {
    match value {
        Value::String(text) => roles.iter().find(|(role, _)| role == text).map_or_else(|| value.clone(), |(_, id)| json!(id)),
        Value::Array(items) => Value::Array(items.iter().map(|item| with_roles(item, roles)).collect()),
        Value::Object(fields) => Value::Object(fields.iter().map(|(key, item)| (key.clone(), with_roles(item, roles))).collect()),
        _ => value.clone(),
    }
}

/// The strings at a declared object path (`a.b`, `items[].c`) of `params`, at any depth below it.
fn ids_at(params: &Value, path: &str) -> Vec<String> {
    let mut current = vec![params.clone()];
    for part in path.split('.') {
        let key = part.trim_end_matches("[]");
        let mut next = Vec::new();
        for value in &current {
            match value.get(key) {
                Some(Value::Array(items)) if part.ends_with("[]") => next.extend(items.iter().cloned()),
                Some(item) => next.push(item.clone()),
                None => {}
            }
        }
        current = next;
    }
    fn strings(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::String(text) => out.push(text.clone()),
            Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    current.iter().for_each(|value| strings(value, &mut out));
    out
}

/// The strings a parameter's value holds one level deep (the guard's `mentioned_in`).
fn mentioned(value: &Value) -> Vec<String> {
    let one = |value: &Value| value.as_str().map(str::to_owned);
    match value {
        Value::Array(items) => items
            .iter()
            .flat_map(|item| match item {
                Value::Object(object) => object.values().filter_map(one).collect::<Vec<_>>(),
                other => one(other).into_iter().collect(),
            })
            .collect(),
        Value::Object(object) => object.values().filter_map(one).collect(),
        other => one(other).into_iter().collect(),
    }
}

/// The fields the schema marks `position`.
const POSITIONS: [&str; 2] = ["position", "parent"];

/// An element's stored fields but `kind` and the position fields: a premise listing all of them
/// holds every field (`*`).
const ELEMENT_FIELDS: &[&str] = &[
    "alt",
    "card",
    "connectorRole",
    "connectorType",
    "dashStyle",
    "end",
    "endArrowhead",
    "fillColor",
    "fillStyle",
    "fontFamily",
    "fontSize",
    "image",
    "key",
    "labelPosition",
    "locked",
    "mindMapBranchAxis",
    "mindMapBranchSide",
    "mindMapCollapsed",
    "mindMapDirection",
    "opacity",
    "points",
    "rotation",
    "roundness",
    "shapeStyle",
    "shapeType",
    "size",
    "sourceRefIds",
    "start",
    "startArrowhead",
    "strokeColor",
    "strokeSize",
    "text",
    "textAlign",
    "textFormat",
    "waypoints",
];

fn spelled(set: Option<&BTreeSet<String>>) -> String {
    match set {
        None => "*".to_owned(),
        Some(set) if set.is_empty() => "-".to_owned(),
        Some(set) => set.iter().cloned().collect::<Vec<_>>().join(","),
    }
}

/// One object's need as the guard keeps it, spelled as a row: fields and positions, a position
/// field given among `fields` counted where it is, `kind` left out.
fn need(fields: Option<&Vec<String>>, positions: Option<&Vec<String>>) -> (String, String) {
    let mut fields: Option<BTreeSet<String>> = fields.map(|fields| fields.iter().cloned().collect());
    let mut positions: Option<BTreeSet<String>> = positions.map(|positions| positions.iter().cloned().collect());
    if let Some(set) = fields.as_mut() {
        set.remove("kind");
        let moved: Vec<String> = set.iter().filter(|field| POSITIONS.contains(&field.as_str())).cloned().collect();
        for field in moved {
            set.remove(&field);
            if let Some(positions) = positions.as_mut() {
                positions.insert(field);
            }
        }
        if ELEMENT_FIELDS.iter().all(|field| set.contains(*field)) && set.iter().all(|field| ELEMENT_FIELDS.contains(&field.as_str())) {
            fields = None;
        }
    }
    (spelled(fields.as_ref()), spelled(positions.as_mut().map(|p| &*p)))
}

/// What a write with `basis` of `command` with `params` holds, as the guard computes it: editor
/// state, objects (fields, positions), windows, covered fields.
struct Held {
    editor: String,
    holds: BTreeMap<String, (String, String)>,
    windows: BTreeSet<String>,
    covers: String,
}

fn held(doc: &BoardDocument, command: &KitCommand, params: &Value, name: &dyn Fn(&str) -> String) -> Held {
    let mut editor: Vec<&str> = command.reads.iter().map(|state| state.as_str()).collect();
    editor.sort_unstable();
    let exists = |id: &str| {
        id == "board"
            || doc.model().element(&kition_kit::kdm::ObjectId::from_string(id)).is_some()
            || doc.model().page(&kition_kit::kdm::ObjectId::from_string(id)).is_some()
    };
    let mut holds: BTreeMap<String, (String, String)> = BTreeMap::new();
    if let Some(target) = command.targets {
        let values = match params.get(target) {
            Some(Value::Array(items)) => items.clone(),
            Some(one) => vec![one.clone()],
            None => vec![],
        };
        for id in values.iter().filter_map(Value::as_str) {
            holds.insert(name(id), ("*".to_owned(), if command.position_independent { "-" } else { "*" }.to_owned()));
        }
    }
    let mut others: BTreeSet<String> = BTreeSet::new();
    for (key, value) in params.as_object().into_iter().flatten() {
        if Some(key.as_str()) == command.targets || command.places.contains(&key.as_str()) {
            continue;
        }
        others.extend(mentioned(value));
    }
    for path in command.objects.iter().filter(|path| path.contains('.') || path.contains("[]")) {
        others.extend(ids_at(params, path));
    }
    let under = doc.under(&command.id, params).unwrap_or_else(|e| panic!("{}: {e}", command.id));
    let mut windows = BTreeSet::new();
    let mut covers = "none".to_owned();
    // The guard reads `within` and `covers` for a step that names a place.
    let placed = command.places.iter().any(|place| params.get(*place).is_some_and(|value| !value.is_null()));
    if let Some(under) = under.as_ref().filter(|_| placed) {
        for within in &under.within {
            let value = serde_json::to_value(within).unwrap();
            windows.insert(format!(
                "{}.{}[{}..{}]",
                name(value["parent"].as_str().unwrap_or_default()),
                value["field"].as_str().unwrap_or_default(),
                value["from"],
                value["to"]
            ));
        }
        if let Some(asked) = &under.covers {
            let mut fields = asked.fields.clone();
            fields.sort();
            covers = fields.join(",");
        }
    }
    if let Some(under) = &under {
        for object in under.objects.iter().filter_map(Value::as_str) {
            holds.entry(name(object)).or_insert_with(|| ("*".to_owned(), "*".to_owned()));
        }
        for premise in &under.premises {
            let id = premise.id.as_str().unwrap_or_default();
            holds.insert(name(id), need(premise.fields.as_ref(), premise.positions.as_ref()));
        }
    }
    for id in others.iter().filter(|id| exists(id)) {
        holds.entry(name(id)).or_insert_with(|| ("*".to_owned(), "*".to_owned()));
    }
    Held { editor: editor.join(","), holds, windows, covers }
}

/// One code path of a command: its parameters (objects by role), the editor state it reads, what
/// a write holds of each object (object, fields, positions), the windows, the covered fields
/// (`none`: no `covers`).
struct Row {
    command: &'static str,
    params: &'static str,
    editor: &'static str,
    holds: &'static [(&'static str, &'static str, &'static str)],
    windows: &'static [&'static str],
    covers: &'static str,
}

const fn r(command: &'static str, params: &'static str, editor: &'static str, holds: &'static [(&'static str, &'static str, &'static str)]) -> Row {
    Row { command, params, editor, holds, windows: &[], covers: "none" }
}

const fn rw(
    command: &'static str,
    params: &'static str,
    editor: &'static str,
    holds: &'static [(&'static str, &'static str, &'static str)],
    windows: &'static [&'static str],
    covers: &'static str,
) -> Row {
    Row { command, params, editor, holds, windows, covers }
}

/// Every write command's rows hold what its reading says, and every write command has a row.
#[test]
fn every_write_rests_on_what_its_handler_reads() {
    let (doc, roles) = fixture();
    let names: BTreeMap<String, String> = roles.iter().map(|(role, id)| (id.clone(), role.clone())).collect();
    let name = |id: &str| names.get(id).cloned().unwrap_or_else(|| id.to_owned());
    let commands = doc.commands();
    let mut wrong = Vec::new();
    let mut differing = BTreeSet::new();
    for row in ROWS {
        let command = commands.iter().find(|c| c.id == row.command).unwrap_or_else(|| panic!("no command {}", row.command));
        let params = with_roles(&serde_json::from_str(row.params).unwrap_or_else(|e| panic!("{}: {e}", row.params)), &roles);
        let got = held(&doc, command, &params, &name);
        let expected: BTreeMap<String, (String, String)> =
            row.holds.iter().map(|(object, fields, at)| ((*object).to_owned(), ((*fields).to_owned(), (*at).to_owned()))).collect();
        let windows: BTreeSet<String> = row.windows.iter().map(|w| (*w).to_owned()).collect();
        if got.editor != row.editor || got.holds != expected || got.windows != windows || got.covers != row.covers {
            differing.insert(row.command);
            wrong.push(format!(
                "{} {}:\n  declared {} {:?} {:?} covers {}\n  reading  {} {expected:?} {windows:?} covers {}",
                row.command, row.params, got.editor, got.holds, got.windows, got.covers, row.editor, row.covers
            ));
        }
    }
    let rows: BTreeSet<&str> = ROWS.iter().map(|row| row.command).collect();
    let writes: BTreeSet<&str> = commands.iter().filter(|c| c.mutates).map(|c| c.id.as_str()).collect();
    for missing in writes.difference(&rows) {
        wrong.push(format!("{missing}: a write command with no reading"));
    }
    for gone in rows.difference(&writes) {
        wrong.push(format!("{gone}: a reading of no write command"));
    }
    assert_eq!(writes.len(), 43, "the write commands read");
    assert!(wrong.is_empty(), "{} rows of {} commands differ from the readings:\n{}", wrong.len(), differing.len(), wrong.join("\n"));
}

/// The second reading uses the same command rows, with the addressed containers genuinely
/// nested: F in FG, G in GF; sizeless T and A in F; B and locked L in G. K spans F and G.
fn nested_fixture() -> (BoardDocument, Vec<(String, String)>) {
    let (mut doc, mut roles) = fixture();
    let ids: BTreeMap<_, _> = roles.iter().cloned().collect();
    let fg = doc.execute("arrange.group", json!({"ids":[ids["$F"]]})).unwrap()["group"].clone();
    let gf = doc.execute("frame.add", json!({"page":ids["$P"],"box":[650,250,1150,650],"children":[ids["$G"]]})).unwrap()["frame"].clone();
    for (child, parent) in [("$T", "$F"), ("$A", "$F"), ("$B", "$G")] {
        doc.execute("element.set", json!({"id":ids[child],"set":{"parent":ids[parent]}})).unwrap();
    }
    doc.execute("element.lock", json!({"ids":[ids["$L"]],"locked":false})).unwrap();
    doc.execute("element.set", json!({"id":ids["$L"],"set":{"parent":ids["$G"]}})).unwrap();
    doc.execute("element.lock", json!({"ids":[ids["$L"]],"locked":true})).unwrap();
    roles.extend([("$FG".into(), text_of(&fg)), ("$GF".into(), text_of(&gf))]);
    (BoardDocument::open("Nested.kboard", &doc.save(None).unwrap()).unwrap(), roles)
}

fn nested_reading(row: &Row) -> BTreeMap<String, (String, String)> {
    let mut expected: BTreeMap<String, (String, String)> =
        row.holds.iter().map(|(id, fields, at)| ((*id).into(), ((*fields).into(), (*at).into()))).collect();
    let mut add = |id: &str, fields: &str, at: &str| {
        expected.insert(id.into(), (fields.into(), at.into()));
    };
    if row.params.contains("$F") {
        match row.command {
            "element.duplicate" | "element.delete" => {
                for id in ["$A", "$T"] {
                    add(id, "*", "parent,position");
                }
            }
            "element.move" => {
                for id in ["$A", "$T"] {
                    add(id, "locked", "parent,position");
                }
            }
            "element.reorder" => {
                for id in ["$A", "$T"] {
                    add(id, "-", "elements,parent");
                }
            }
            "frame.fit" => {
                for id in ["$A", "$T"] {
                    add(id, "~bounds", "parent,position");
                }
            }
            "layout.align" => {
                add("$T", "locked", "parent,position");
                // A is carried by selected F. Its size does not set the alignment delta.
                add("$A", "locked", "parent,position");
                add("$FG", "-", "parent");
            }
            _ => {}
        }
    }
    if row.params.contains("$G") {
        match row.command {
            "element.move" => {
                for id in ["$B", "$L"] {
                    add(id, "locked", "parent,position");
                }
            }
            "element.resize" => {
                for id in ["$B", "$L"] {
                    add(id, "locked,size,~bounds", "parent,position");
                }
            }
            "element.rotate" => {
                for id in ["$B", "$L"] {
                    add(id, "locked,rotation,size,~bounds", "parent,position");
                }
            }
            "element.delete" => {
                for id in ["$B", "$L"] {
                    add(id, "*", "parent,position");
                }
            }
            "arrange.ungroup" => {
                for id in ["$B", "$L"] {
                    add(id, "locked", "parent");
                }
            }
            _ => {}
        }
    }
    expected
}

#[test]
fn nested_write_rows_match_the_second_independent_reading() {
    let (doc, roles) = nested_fixture();
    let names: BTreeMap<String, String> = roles.iter().map(|(role, id)| (id.clone(), role.clone())).collect();
    let name = |id: &str| names.get(id).cloned().unwrap_or_else(|| id.into());
    let commands = doc.commands();
    let mut wrong = Vec::new();
    let mut changed = 0;
    for row in ROWS {
        let command = commands.iter().find(|c| c.id == row.command).unwrap();
        let params = with_roles(&serde_json::from_str(row.params).unwrap(), &roles);
        let got = held(&doc, command, &params, &name);
        let expected = nested_reading(row);
        let base: BTreeMap<String, (String, String)> =
            row.holds.iter().map(|(id, fields, at)| ((*id).into(), ((*fields).into(), (*at).into()))).collect();
        if base != expected {
            changed += 1;
            println!("nested differs {} {}", row.command, row.params);
        }
        let windows = row.windows.iter().map(|w| (*w).to_owned()).collect();
        if got.editor != row.editor || got.holds != expected || got.windows != windows || got.covers != row.covers {
            wrong.push(format!("{} {}: declared {:?} / reading {expected:?}; windows {:?}", row.command, row.params, got.holds, got.windows));
        }
    }
    println!("nested rows={} differing-from-flat={changed} wrong={}", ROWS.len(), wrong.len());
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Every node of the fixture's mind map, as a map command reads it.
const NODE: &str = "locked,mindMapBranchSide,mindMapCollapsed,mindMapDirection,rotation,shapeStyle,size,strokeColor";
/// Every branch of it, as the layout rewrites it (and its lock, which the layout checks since
/// C8-board-engine).
const EDGE: &str = "connectorRole,connectorType,end,endArrowhead,locked,mindMapBranchAxis,start";

const ROWS: &[Row] = &[
    // ---- Making content. element.add (elements.rs create): `page` by id, else the editor's
    // page (`ctx.active_page`, else the first). New elements go at the end of the page's list (the
    // front): no sibling is read. A spec's `parent` (an id in a member of `elements`) is checked
    // for kind (frame or group) and page; a spec's `key` against every key on the page.
    r("element.add", r#"{"page":"$P","elements":[{"kind":"shape","position":[50,50]}]}"#, "part", &[("$P", "-", "-")]),
    // pinned from the declaration after review: the editor's page held as a premise of existence
    // is the same fact as the part it reads.
    r("element.add", r#"{"elements":[{"kind":"shape","position":[50,50]}]}"#, "part", &[("$P", "-", "-")]),
    // was: $F ("-", "parent"): the container's own container, which `check_parent` does not read.
    r(
        "element.add",
        r#"{"page":"$P","elements":[{"kind":"shape","position":[50,450],"parent":"$F"}]}"#,
        "part",
        &[("$P", "-", "-"), ("$F", "-", "-")],
    ),
    r(
        "element.add",
        r#"{"page":"$P","elements":[{"kind":"shape","position":[50,50],"key":"k1"}]}"#,
        "part",
        &[("$P", "-", "-"), ("board", "~keys", "-")],
    ),
    // frame.add: as create; with `children`, each one's lock, its place (the frame goes in at
    // the first member's index) and its `parent`, replaced.
    r("frame.add", r#"{"page":"$P","box":[0,0,100,100],"title":"New"}"#, "part", &[("$P", "-", "-")]),
    // pinned from the declaration after review: the frame goes in at its first member's index
    // wherever that is, so the member's index is no premise (the reading held `elements`).
    r("frame.add", r#"{"page":"$P","box":[0,0,400,400],"children":["$A"]}"#, "part", &[("$P", "-", "-"), ("$A", "locked", "parent")]),
    // stroke.add: the page, the points given.
    r("stroke.add", r#"{"page":"$P","points":[[0,0],[10,10]]}"#, "part", &[("$P", "-", "-")]),
    // connector.add: an `element` end by id, on the page, bindable; its box (position, size,
    // rotation) gives the end point and, without an anchor, the nearest edge toward the other end.
    // pinned from the declaration after review: the box as `~bounds` (it holds a text's measured
    // size too) beside `rotation`.
    // was: ("~bounds", "parent,position"): no rotation (a turn from 45 to 135 degrees keeps the
    // bounds and changes the edge an end faces), the end element's container (not read).
    r(
        "connector.add",
        r#"{"page":"$P","from":{"element":"$A"},"to":{"element":"$B"}}"#,
        "part",
        &[("$P", "-", "-"), ("$A", "rotation,~bounds", "position"), ("$B", "rotation,~bounds", "position")],
    ),
    r("connector.add", r#"{"page":"$P","from":{"point":[0,0]},"to":{"point":[100,0]}}"#, "part", &[("$P", "-", "-")]),
    // A text without a stored size is measured from its text and font: its `~bounds`.
    // pinned from the declaration after review (the reading listed the fields it is measured from).
    // was: ("~bounds", "parent,position").
    r(
        "connector.add",
        r#"{"page":"$P","from":{"element":"$T"},"to":{"point":[0,0]}}"#,
        "part",
        &[("$P", "-", "-"), ("$T", "rotation,~bounds", "position")],
    ),
    r(
        "connector.add",
        r#"{"page":"$P","from":{"point":[0,0]},"to":{"point":[100,0]},"key":"k2"}"#,
        "part",
        &[("$P", "-", "-"), ("board", "~keys", "-")],
    ),
    // card.add / image.insert: without `position`, right of the page's content
    // (content_bounds: every element's bounds).
    r("card.add", r#"{"page":"$P","url":"https://example.org","position":[0,0]}"#, "part", &[("$P", "-", "-")]),
    r("card.add", r#"{"page":"$P","url":"https://example.org"}"#, "part", &[("$P", "~geometry", "-")]),
    r(
        "image.insert",
        r#"{"page":"$P","data":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==","position":[0,0],"size":[10,10]}"#,
        "part",
        &[("$P", "-", "-")],
    ),
    r(
        "image.insert",
        r#"{"page":"$P","data":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="}"#,
        "part",
        &[("$P", "~geometry", "-")],
    ),
    // template.insert: `at` is the centre; without it right of the content.
    r("template.insert", r#"{"page":"$P","template":"kanban","at":[0,0]}"#, "part", &[("$P", "-", "-")]),
    r("template.insert", r#"{"page":"$P","template":"kanban"}"#, "part", &[("$P", "~geometry", "-")]),
    // edit.paste (clipboard.rs paste): the payload is a parameter. With `at`, only the
    // payload's own positions; without it, whether any payload ref is an element shown on the
    // page (24, 24 then). A binding to an element outside the payload on the page is kept, its
    // point from that element's box.
    r(
        "edit.paste",
        r#"{"page":"$P","payload":{"type":"application/x-kition-board+json","version":1,"elements":[{"ref":"x","kind":"shape","position":{"x":0,"y":0}}]},"at":[10,10]}"#,
        "part",
        &[("$P", "-", "-")],
    ),
    // was: $A not held (the payload's refs were read as source identities only).
    r(
        "edit.paste",
        r#"{"page":"$P","payload":{"type":"application/x-kition-board+json","version":1,"elements":[{"ref":"$A","kind":"shape","position":{"x":0,"y":0},"size":{"w":10,"h":10}}]}}"#,
        "part",
        &[("$P", "-", "-"), ("$A", "-", "-")],
    ),
    // was: $B not held.
    r(
        "edit.paste",
        r#"{"page":"$P","payload":{"type":"application/x-kition-board+json","version":1,"elements":[{"ref":"x","kind":"connector","start":{"point":[0,0]},"end":{"point":[5,5],"binding":{"target":"$B","anchor":[0,0.5]}}}]},"at":[10,10]}"#,
        "part",
        &[("$P", "-", "-"), ("$B", "rotation,~bounds", "position")],
    ),
    // element.duplicate: every field of each copied element and of what it holds (`element_json`
    // copies `parent` too, remapped when the parent is copied); where it is (offset from it).
    // pinned from the declaration after review: `parent` is read (the reading had position only).
    r("element.duplicate", r#"{"ids":["$A"]}"#, "", &[("$A", "*", "parent,position")]),
    r("element.duplicate", r#"{"ids":["$F"]}"#, "", &[("$F", "*", "parent,position"), ("$C", "*", "parent,position"), ("board", "~structure", "-")]),
    // ---- Fields of one element. element.set (elements.rs update): the target's kind and lock,
    // the fields set; `position`/`size`/`rotation` refresh bound connector ends (a consequence);
    // `parent` checks the new container's kind and page; `key` every key on the page.
    r("element.set", r#"{"id":"$A","set":{"fillColor":"red"}}"#, "", &[("$A", "fillColor,locked", "-")]),
    r("element.set", r#"{"id":"$A","set":{"position":[10,10]}}"#, "", &[("$A", "locked", "position")]),
    // was: $F ("-", "parent") and the board's ~structure: a shape holds nothing the new container
    // could be inside.
    r("element.set", r#"{"id":"$A","set":{"parent":"$F"}}"#, "", &[("$A", "locked", "parent"), ("$F", "-", "-")]),
    r("element.set", r#"{"id":"$A","set":{"key":"k3"}}"#, "", &[("$A", "key,locked", "-"), ("board", "~keys", "-")]),
    r("element.set", r#"{"id":"$T","set":{"fontSize":30}}"#, "", &[("$T", "fontSize,locked", "-")]),
    // element.style: per element its kind (which fields it takes) and lock; the fields written.
    r("element.style", r#"{"ids":["$A"],"style":{"fillColor":"red"}}"#, "", &[("$A", "fillColor,locked", "-")]),
    // element.setText: kind, lock, the text (diffed).
    r("element.setText", r#"{"id":"$A","text":"New"}"#, "", &[("$A", "locked,text", "-")]),
    // text.mark: kind and the text (its length, UTF-16 widths). The reading found no lock check
    // (an engine defect, fixed: `locked_elements_refuse_marks_and_alternative_text`); the lock is
    // read now, as the declaration held it.
    r("text.mark", r#"{"id":"$T","range":[0,2],"mark":"bold","value":true}"#, "", &[("$T", "locked,text", "-")]),
    r("text.mark", r#"{"id":"$T","range":[0,2],"mark":"italic","value":true,"units":"utf16"}"#, "", &[("$T", "locked,text", "-")]),
    // element.lock: writes `locked` on the ids, reads nothing of them.
    r("element.lock", r#"{"ids":["$A"],"locked":true}"#, "", &[("$A", "locked", "-")]),
    // connector.setLabel / setWaypoints: kind and lock of the connector, the field written.
    r("connector.setLabel", r#"{"id":"$K","text":"L"}"#, "", &[("$K", "locked,text", "-")]),
    r("connector.setLabel", r#"{"id":"$K","text":"L","position":0.3}"#, "", &[("$K", "labelPosition,locked,text", "-")]),
    r("connector.setWaypoints", r#"{"id":"$K","waypoints":[[10,10]]}"#, "", &[("$K", "locked,waypoints", "-")]),
    // connector.bind: the connector's lock, the end replaced and the other end (its point, derived
    // from its target's box when bound: no two ends on one target, the edge faced); an `element`
    // end's box on the connector's page.
    // pinned from the declaration after review: `start.binding` is a path inside `start`, and the
    // boxes as for connector.add.
    // was: $A and $T ("~bounds", "parent,position").
    r(
        "connector.bind",
        r#"{"id":"$K","terminal":"end","to":{"element":"$T"}}"#,
        "",
        &[("$K", "end,locked,start,start.binding", "-"), ("$A", "rotation,~bounds", "position"), ("$T", "rotation,~bounds", "position")],
    ),
    // pinned from the declaration after review, as the row above. was: $B and $F ("~bounds", "parent,position").
    r(
        "connector.bind",
        r#"{"id":"$K","terminal":"start","to":{"element":"$F"}}"#,
        "",
        &[("$K", "end,end.binding,locked,start", "-"), ("$B", "rotation,~bounds", "position"), ("$F", "rotation,~bounds", "position")],
    ),
    r("connector.bind", r#"{"id":"$K","terminal":"end","to":{"point":[50,50]}}"#, "", &[("$K", "end,locked", "-")]),
    // card.setTarget: the card's kind and lock; `card` replaced whole.
    r("card.setTarget", r#"{"id":"$CARD","url":"https://example.org/2"}"#, "", &[("$CARD", "card,locked", "-")]),
    // image.setAlt: the image's kind; `alt` written. The reading found no lock check (an engine
    // defect, fixed); was: ("alt", "-").
    r("image.setAlt", r#"{"id":"$I","alt":"A picture"}"#, "", &[("$I", "alt,locked", "-")]),
    // link.retarget: every shown card's `card.target.path` on every page; the matching cards'
    // `card` rewritten.
    // pinned from the declaration after review: the cards a path names are a place (`from`), told
    // by asking (`covers ~link`), each holding the path it is rewritten from.
    rw("link.retarget", r#"{"from":"notes.md","to":"new.md"}"#, "", &[("$NOTE", "card.target.path", "-")], &[], "~link"),
    // ---- Geometry. element.move (elements.rs move_elements): the subtree of the roots (a
    // frame's or group's members), each one's kind and lock and the fields shifted.
    // was: ("locked", "parent,position"): a lone element's container is not read.
    r("element.move", r#"{"ids":["$A"],"delta":[10,0]}"#, "", &[("$A", "locked", "position")]),
    // was: $F ("locked", "parent,position").
    r(
        "element.move",
        r#"{"ids":["$F"],"delta":[10,0]}"#,
        "",
        &[("$F", "locked", "position"), ("$C", "locked", "parent,position"), ("$P", "~hierarchy", "-")],
    ),
    // was: $G ("locked", "parent").
    r(
        "element.move",
        r#"{"ids":["$G"],"delta":[10,0]}"#,
        "",
        &[("$G", "locked", "-"), ("$G1", "locked", "parent,position"), ("$G2", "locked", "parent,position"), ("$P", "~hierarchy", "-")],
    ),
    // `to`: the delta from the top-left of the named elements' bounds.
    // was: ("locked,~bounds", "parent,position").
    r("element.move", r#"{"ids":["$A"],"to":[100,100]}"#, "", &[("$A", "locked,~bounds", "position")]),
    // `reparent`: the smallest frame (page order on ties) whose box holds the bounds' centre after
    // the move; the root's parent; in front of the frame when behind it (page order).
    // pinned from the declaration after review: the frame the centre lands in is a place told by
    // asking (`covers`), with the frames' order (`~frameOrder`) and kinds; the root goes in front
    // of the frame wherever it was (no `elements`).
    // was: $F ("-", "parent"): where the destination frame is, `covers` tells; its container is
    // not read.
    rw(
        "element.move",
        r#"{"ids":["$A"],"delta":[50,400],"reparent":true}"#,
        "",
        &[("$A", "locked,~bounds", "parent,position"), ("$F", "-", "-")],
        &[],
        "kind,parent,~frame,~frameOrder",
    ),
    // element.resize: one boxed element: its box written, its lock and its members' locks.
    // C8-board-engine: only the box is written, so only its lock is checked (a frame's content
    // stays where it is and is not read).
    // was: ("fontSize,locked,rotation,size,~bounds", "parent,position").
    r("element.resize", r#"{"id":"$A","box":[0,0,200,100]}"#, "", &[("$A", "locked,size", "position")]),
    // C8-board-engine: was $C ("locked", "parent,position") beside the frame.
    r("element.resize", r#"{"id":"$F","box":[0,400,600,800]}"#, "", &[("$F", "locked,size", "position")]),
    // A group: its bounds (its members'), each member mapped and scaled.
    // pinned from the declaration after review: the old box is the members' `~bounds` (the
    // reading had their rotation and size); a lone group's container is not read.
    // was: $G ("fontSize,locked,rotation,size", "parent"), members ("fontSize,locked,rotation,size,~bounds", "parent,position").
    r(
        "element.resize",
        r#"{"id":"$G","box":[700,300,1200,500]}"#,
        "",
        &[
            ("$G", "locked", "-"),
            ("$G1", "locked,size,~bounds", "parent,position"),
            ("$G2", "locked,size,~bounds", "parent,position"),
            ("$P", "~hierarchy", "-"),
        ],
    ),
    // Several: the union of their bounds; each box's centre mapped, its size scaled (a text's font
    // size too); which are roots (their parents).
    // pinned from the declaration after review: `~bounds` and the roots' parents.
    // was: ("fontSize,locked,rotation,size,~bounds", "parent,position").
    r(
        "element.resize",
        r#"{"ids":["$A","$B"],"box":[0,0,800,200]}"#,
        "",
        &[("$A", "locked,size,~bounds", "position"), ("$B", "locked,size,~bounds", "position")],
    ),
    // element.rotate: each about its own centre: a box's rotation; a stroke's points.
    // was: ("locked,rotation,size,~bounds", "parent,position").
    r("element.rotate", r#"{"ids":["$A"],"degrees":90}"#, "", &[("$A", "locked,rotation", "-")]),
    // was: ("locked,points,rotation,size,~bounds", "parent,position").
    r("element.rotate", r#"{"ids":["$S"],"degrees":45}"#, "", &[("$S", "locked,points", "-")]),
    // was: $G ("locked,rotation,size", "parent"), members ("locked,rotation,size,~bounds", "parent,position").
    // C8-board-engine: a group turns as one about its centre (its members' bounds), each member's
    // box moved about it; was members ("locked,rotation", "parent").
    r(
        "element.rotate",
        r#"{"ids":["$G"],"degrees":90}"#,
        "",
        &[
            ("$G", "locked", "-"),
            ("$G1", "locked,rotation,size,~bounds", "parent,position"),
            ("$G2", "locked,rotation,size,~bounds", "parent,position"),
            ("$P", "~hierarchy", "-"),
        ],
    ),
    // About the selection: the union's centre; each box's centre moved about it.
    // pinned from the declaration after review: `~bounds` and the roots' parents.
    r(
        "element.rotate",
        r#"{"ids":["$A","$B"],"degrees":90,"about":"selection"}"#,
        "",
        &[("$A", "locked,rotation,size,~bounds", "position"), ("$B", "locked,rotation,size,~bounds", "position")],
    ),
    // frame.fit: the frame's lock, its box written; its direct members' bounds.
    // pinned from the declaration after review: a member's `position` beside its `~bounds`.
    r("frame.fit", r#"{"id":"$F"}"#, "", &[("$F", "locked,size", "position"), ("$C", "~bounds", "parent,position"), ("$P", "~hierarchy", "-")]),
    // layout.align / distribute / stack: each root's bounds (the reference is all of them), its
    // kind and lock, its position shifted; which are roots (their parents).
    // pinned from the declaration after review: the roots' parents (the reading missed `roots`).
    // was: ("locked,rotation,size,~bounds", "parent,position"): rotation and size beside the bounds.
    r("layout.align", r#"{"ids":["$A","$B"],"edge":"left"}"#, "", &[("$A", "locked,~bounds", "position"), ("$B", "locked,~bounds", "position")]),
    r(
        "layout.distribute",
        r#"{"ids":["$A","$B","$T"],"axis":"horizontal"}"#,
        "",
        &[("$A", "locked,~bounds", "position"), ("$B", "locked,~bounds", "position"), ("$T", "locked,~bounds", "position")],
    ),
    r("layout.stack", r#"{"ids":["$A","$B"],"axis":"vertical"}"#, "", &[("$A", "locked,~bounds", "position"), ("$B", "locked,~bounds", "position")]),
    // pinned from the declaration after review, as the rows above.
    // was: $F and $A ("locked,rotation,size,~bounds", "parent,position"), $C ("locked,rotation,size", "parent,position").
    r(
        "layout.align",
        r#"{"ids":["$F","$A"],"edge":"left"}"#,
        "",
        &[
            ("$F", "locked,~bounds", "parent,position"),
            ("$C", "locked", "parent,position"),
            ("$A", "locked,~bounds", "parent,position"),
            ("$P", "~hierarchy", "-"),
        ],
    ),
    // ---- Structure. element.delete: the subtree; the roots' locks; connectors bound to what goes
    // are freed (a consequence). C8-board-engine: every lock of the subtree is checked.
    // pinned from the declaration after review: a delete replaces every field of what goes (the
    // reading held only what it checks).
    r("element.delete", r#"{"ids":["$A"]}"#, "", &[("$A", "*", "parent,position")]),
    r("element.delete", r#"{"ids":["$F"]}"#, "", &[("$F", "*", "parent,position"), ("$C", "*", "parent,position"), ("board", "~structure", "-")]),
    r(
        "element.delete",
        r#"{"ids":["$G"]}"#,
        "",
        &[("$G", "*", "parent,position"), ("$G1", "*", "parent,position"), ("$G2", "*", "parent,position"), ("board", "~structure", "-")],
    ),
    // element.reorder: the page's raw list. `front`/`back` put it at an end; `forward`/`backward`
    // past its neighbour.
    // was: ("locked", "parent") and the whole page's list as the window.
    r("element.reorder", r#"{"ids":["$A"],"placement":"front"}"#, "", &[("$A", "-", "-")]),
    r("element.reorder", r#"{"ids":["$A"],"placement":"back"}"#, "", &[("$A", "-", "-")]),
    rw("element.reorder", r#"{"ids":["$A"],"placement":"forward"}"#, "", &[("$A", "-", "elements")], &["$P.elements[0..2]"], "none"),
    // C8-board-engine: a container moves with what it holds (v1's `reorderBoardElements`): which
    // elements are inside it, and where each one is.
    r(
        "element.reorder",
        r#"{"ids":["$F"],"placement":"front"}"#,
        "",
        &[("$F", "-", "elements"), ("$C", "-", "elements,parent"), ("$P", "~hierarchy", "-")],
    ),
    // arrange.group: the roots' locks and parents (a common parent is the group's), the first
    // member's index (where the group goes).
    // pinned from the declaration after review: the group goes in at the first member's index
    // wherever that is (no `elements`).
    r("arrange.group", r#"{"ids":["$A","$B"]}"#, "", &[("$A", "locked", "parent"), ("$B", "locked", "parent")]),
    // arrange.ungroup: the group's kind, lock and parent (its members' new parent), its members.
    // pinned from the declaration after review: the group goes, every field of it.
    // was: every field of $G1 and $G2, of which only `parent` is written.
    r(
        "arrange.ungroup",
        r#"{"groupIds":["$G"]}"#,
        "",
        &[("$G", "*", "parent,position"), ("$G1", "locked", "parent"), ("$G2", "locked", "parent"), ("$P", "~hierarchy", "-")],
    ),
    // ---- Mind maps (mindmap.rs): the map from the topology (mind-node shapes, connectors from one
    // to another, page order), the root by walking up; every node's lock, box, rotation,
    // collapsed, branch side, stroke colour, direction; each edge rewritten.
    // pinned from the declaration after review: each node's `shapeStyle` (what makes it a node,
    // the reading missed it) and the page.
    // was: every node ("…", "parent,position") and its containers' kind and parent: no map
    // command but addChild/addSibling reads a node's container.
    r(
        "layout.mindmap",
        r#"{"root":"$M1"}"#,
        "",
        &[
            ("$M", NODE, "position"),
            ("$M1", NODE, "position"),
            ("$M2", NODE, "position"),
            ("$M11", NODE, "position"),
            ("$E1", EDGE, "-"),
            ("$E2", EDGE, "-"),
            ("$E11", EDGE, "-"),
            ("$P", "-", "-"),
            ("board", "~mapStructure", "-"),
        ],
    ),
    r(
        "mindmap.setDirection",
        r#"{"node":"$M1","direction":"right"}"#,
        "",
        &[
            ("$M", NODE, "position"),
            ("$M1", NODE, "position"),
            ("$M2", NODE, "position"),
            ("$M11", NODE, "position"),
            ("$E1", EDGE, "-"),
            ("$E2", EDGE, "-"),
            ("$E11", EDGE, "-"),
            ("$P", "-", "-"),
            ("board", "~mapStructure", "-"),
        ],
    ),
    r(
        "mindmap.collapse",
        r#"{"node":"$M1","collapsed":true}"#,
        "",
        &[
            ("$M", NODE, "position"),
            ("$M1", NODE, "position"),
            ("$M2", NODE, "position"),
            ("$M11", NODE, "position"),
            ("$E1", EDGE, "-"),
            ("$E2", EDGE, "-"),
            ("$E11", EDGE, "-"),
            ("$P", "-", "-"),
            ("board", "~mapStructure", "-"),
        ],
    ),
    // addChild / addSibling: as the layout, and the container (`parent`) of the node branched
    // from, which the new node and branch take; from the root, its branches' count (structure).
    r(
        "mindmap.addChild",
        r#"{"node":"$M1","text":"New"}"#,
        "",
        &[
            ("$M", NODE, "position"),
            ("$M1", NODE, "parent,position"),
            ("$M2", NODE, "position"),
            ("$M11", NODE, "position"),
            ("$E1", EDGE, "-"),
            ("$E2", EDGE, "-"),
            ("$E11", EDGE, "-"),
            ("$P", "-", "-"),
            ("board", "~mapStructure", "-"),
        ],
    ),
    r(
        "mindmap.addChild",
        r#"{"node":"$M","text":"New"}"#,
        "",
        &[
            ("$M", NODE, "parent,position"),
            ("$M1", NODE, "position"),
            ("$M2", NODE, "position"),
            ("$M11", NODE, "position"),
            ("$E1", EDGE, "-"),
            ("$E2", EDGE, "-"),
            ("$E11", EDGE, "-"),
            ("$P", "-", "-"),
            ("board", "~mapStructure", "-"),
        ],
    ),
    r(
        "mindmap.addSibling",
        r#"{"node":"$M1","text":"New"}"#,
        "",
        &[
            ("$M", NODE, "parent,position"),
            ("$M1", NODE, "position"),
            ("$M2", NODE, "position"),
            ("$M11", NODE, "position"),
            ("$E1", EDGE, "-"),
            ("$E2", EDGE, "-"),
            ("$E11", EDGE, "-"),
            ("$P", "-", "-"),
            ("board", "~mapStructure", "-"),
        ],
    ),
    // ---- Pages (pages.rs). page.add: `at` a place in the raw list; without a name "Page N"
    // from the count.
    rw("page.add", r#"{"name":"N","at":1}"#, "", &[], &["board.pages[1..1]"], "none"),
    r("page.add", r#"{}"#, "", &[("board", "~pageCount", "-")]),
    // page.rename: the name written.
    r("page.rename", r#"{"page":"$Q","name":"Renamed"}"#, "", &[("$Q", "name", "-")]),
    // page.duplicate: the source's name (the copy's default) and index (the copy goes after it),
    // every shown element on it, whole.
    // pinned from the declaration after review: the copy goes after the source wherever it is
    // (no `pages`); a copied element's `parent` is read (remapped).
    // was: also the board's ~structure (every page's parent links and bindings).
    r("page.duplicate", r#"{"page":"$Q"}"#, "", &[("$Q", "elements,name", "-"), ("$Q1", "*", "parent,position")]),
    // page.delete: the page and every element in its list.
    // pinned from the declaration after review: a delete replaces every field of what goes.
    // was: also the board's ~structure.
    r("page.delete", r#"{"page":"$Q"}"#, "", &[("$Q", "elements,name", "-"), ("$Q1", "*", "parent,position")]),
    // page.move: the page's index; `to` the destination gap; `placement` past its neighbour.
    rw("page.move", r#"{"page":"$Q","to":0}"#, "", &[("$Q", "-", "pages")], &["board.pages[0..0]"], "none"),
    // was: the whole page list as the window (equal here, with two pages).
    rw("page.move", r#"{"page":"$Q","placement":"previous"}"#, "", &[("$Q", "-", "pages")], &["board.pages[0..2]"], "none"),
    // ---- History (bindings/wasm execute_as): the session's history in the caller's scope.
    // pinned from the declaration after review: no row can name what an undone step wrote.
    r("edit.undo", r#"{}"#, "", &[]),
    r("edit.redo", r#"{}"#, "", &[]),
];

/// Engine defects the reading found (checks made on the wrong object, or not made): a mark and
/// alternative text are edits of a locked element like a new text (`element.setText`) and a new
/// target (`card.setTarget`), which refuse it.
#[test]
fn locked_elements_refuse_marks_and_alternative_text() {
    let (mut doc, roles) = fixture();
    let id = |role: &str| json!(roles.iter().find(|(name, _)| name == role).unwrap().1);
    doc.execute("element.lock", json!({"ids":[id("$T"), id("$I")],"locked":true})).unwrap();
    let element =
        |doc: &BoardDocument, role: &str| doc.model().element(&kition_kit::kdm::ObjectId::from_string(id(role).as_str().unwrap())).cloned().unwrap();
    let (text, image) = (element(&doc, "$T"), element(&doc, "$I"));
    let marked = doc.execute("text.mark", json!({"id":id("$T"),"range":[0,2],"mark":"bold"}));
    assert!(marked.as_ref().is_err_and(|error| error.contains("locked")), "{marked:?}");
    let alt = doc.execute("image.setAlt", json!({"id":id("$I"),"alt":"A picture"}));
    assert!(alt.as_ref().is_err_and(|error| error.contains("locked")), "{alt:?}");
    assert_eq!((element(&doc, "$T"), element(&doc, "$I")), (text, image), "nothing changed");
}
