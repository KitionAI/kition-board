//! C8-land-11-board: declaration audit against the UUID resolvers, including nested members.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_board_kit::BoardDocument;
use kition_kit::{KitCommand, KitDocument};
use serde_json::{Value, json};

/// These strings describe data being authored, not an address resolved in this board.
const NOT_NAMES: &[(&str, &str, &str)] = &[
    ("page.create", "name", "the new page's label"),
    ("page.rename", "name", "the replacement page label"),
    ("page.duplicate", "name", "the copy's label"),
    ("element.create", "elements[].key", "a new page-local authoring key, checked only for uniqueness"),
    ("element.update", "set.key", "a replacement authoring key, checked only for uniqueness"),
    ("connector.create", "key", "a new authoring key, checked only for uniqueness"),
];

/// Audited against params::{id,ids,page}, fields::check(parent), connectors::end_spec and render.
fn references(command: &str) -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    let target = match command {
        "element.delete" | "element.move" | "element.rotate" | "element.style" | "element.getStyle" | "element.duplicate" | "element.reorder"
        | "element.lock" | "element.group" | "clipboard.copy" | "layout.align" | "layout.distribute" | "layout.stack" => "ids",
        "element.ungroup" => "groupIds",
        "element.update"
        | "element.resize"
        | "element.setText"
        | "text.mark"
        | "frame.fit"
        | "connector.bind"
        | "connector.setLabel"
        | "connector.setWaypoints"
        | "card.setTarget"
        | "image.setAlt" => "id",
        "mindmap.addChild" | "mindmap.addSibling" | "mindmap.collapse" | "mindmap.setDirection" => "node",
        "layout.mindmap" => "root",
        _ => "",
    };
    if !target.is_empty() {
        out.push((target, "Element"));
    }
    match command {
        "element.create" => out.extend([("page", "Page"), ("elements[].parent", "Element")]),
        "element.update" => out.push(("set.parent", "Element")),
        "element.resize" => out.push(("ids", "Element")),
        "element.paste" | "stroke.create" | "template.insert" | "card.create" | "image.insert" => out.push(("page", "Page")),
        "frame.create" => out.extend([("page", "Page"), ("children", "Element")]),
        "connector.create" => out.extend([("page", "Page"), ("from.element", "Element"), ("to.element", "Element")]),
        "connector.bind" => out.push(("to.element", "Element")),
        "page.rename" | "page.duplicate" | "page.delete" | "page.move" | "page.activate" => out.push(("page", "Page")),
        "file.export" => out.extend([("page", "Page"), ("frame", "Element")]),
        "file.render" => out.extend([("target.frame", "Element"), ("target.objectId", "Element")]),
        _ => {}
    }
    out
}

fn gaps(commands: &[KitCommand]) -> Vec<String> {
    let mut gaps = Vec::new();
    for command in commands {
        for (path, ty) in references(&command.id) {
            if command.targets != Some(path) && !command.objects.contains(&path)
                || !command.object_types.iter().any(|(p, types)| *p == path && *types == [ty])
            {
                gaps.push(format!("{} {path}: {ty}", command.id));
            }
        }
    }
    gaps
}

#[test]
fn every_uuid_resolver_is_marked_and_typed() {
    let doc = BoardDocument::create("Audit.kboard", 100, 100).unwrap();
    let commands = doc.commands();
    kition_kit::testing::check_id_params(&commands, &[]).unwrap();
    assert!(gaps(&commands).is_empty(), "undeclared UUID references:\n{}", gaps(&commands).join("\n"));
    let count: usize = commands.iter().map(|command| references(&command.id).len()).sum();
    println!("UUID references marked/typed: {count}; integer scanner allows: 0");
    for (id, path) in [("element.create", "elements[].parent"), ("element.update", "set.parent"), ("connector.create", "from.element")] {
        let mut broken = commands.clone();
        broken.iter_mut().find(|command| command.id == id).unwrap().objects = &[];
        assert!(gaps(&broken).iter().any(|gap| gap.starts_with(&format!("{id} {path}:"))));
    }
    let render = commands.iter().find(|command| command.id == "file.render").unwrap();
    // Integer pages are ordinals, not object ids; the string form is a UUID. Never mark both.
    assert!(!render.objects.contains(&"target.page"));
    assert!(render.object_types.contains(&("target.page", &["Page"][..])));
}

fn name_gaps(commands: &[KitCommand]) -> Vec<String> {
    fn walk(command: &KitCommand, node: &Value, path: &str, out: &mut Vec<String>) {
        let description = node["description"].as_str().unwrap_or_default();
        if description.contains("id or name") || description.contains("id or its name") {
            let named = command.targets == Some(path) && command.name_field.is_some()
                || command.name_params.iter().any(|(p, _)| *p == path)
                || node.get("x-khp-name").is_some();
            if !named {
                out.push(format!("{} {path}", command.id));
            }
        }
        if let Some(properties) = node.get("properties").and_then(Value::as_object) {
            for (key, child) in properties {
                walk(command, child, &format!("{path}.{key}"), out);
            }
        }
        if let Some(items) = node.get("items") {
            walk(command, items, &format!("{path}[]"), out);
        }
    }
    let mut out = Vec::new();
    for command in commands {
        if let Some(properties) = command.schema.as_ref().and_then(Value::as_object) {
            for (key, node) in properties {
                walk(command, node, key, &mut out);
            }
        }
    }
    out
}

#[test]
fn names_are_authored_values_never_uuid_aliases() {
    let mut doc = BoardDocument::create("Names.kboard", 100, 100).unwrap();
    let commands = doc.commands();
    assert!(name_gaps(&commands).is_empty());
    assert!(commands.iter().all(|command| command.name_field.is_none() && command.name_params.is_empty()));
    for (id, path, reason) in NOT_NAMES {
        assert!(!reason.is_empty());
        let command = commands.iter().find(|command| command.id == *id).unwrap();
        assert!(!command.objects.contains(path), "{id} {path}: {reason}");
    }
    let mut broken = commands.clone();
    broken.iter_mut().find(|command| command.id == "element.update").unwrap().schema.as_mut().unwrap()["id"]["description"] =
        json!("An Element id or name");
    assert_eq!(name_gaps(&broken), ["element.update id"]);
    assert!(doc.execute("page.rename", json!({"page": "Page 1", "name": "Wrong"})).unwrap_err().starts_with("not-found:"));
    println!("name resolvers: 0; authored-name exceptions: {}", NOT_NAMES.len());
}
