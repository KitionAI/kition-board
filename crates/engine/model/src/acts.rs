//! What each command acts on besides its parameters, and the parameter a step's `on` fills
//! (kition-sdk `KitCommand::reads` and `KitCommand::targets`; Kit Process Protocol sections 6 and 7a;
//! task card C4/C5/C6-board).
//!
//! A board's only editor state is the page the editor shows ([`EditorState::Part`]): the commands
//! that make content or export work there when they name no `page`. Nothing reads a selection (a
//! board keeps none), a time, a mode or an active object. `kition-board-kit`'s `tests/acts.rs`
//! follows each command's code to the shown page and fails when [`reads`] says otherwise, and
//! checks [`targets`] against the element ids each command's code reads.

use kition_kit::EditorState;

const PART: &[EditorState] = &[EditorState::Part];

/// The editor state command `id` reads besides its parameters.
pub fn reads(id: &str) -> &'static [EditorState] {
    match id {
        "element.add" | "edit.paste" | "frame.add" | "stroke.add" | "connector.add" | "card.add" | "image.insert" | "template.insert"
        | "file.export" => PART,
        _ => &[],
    }
}

/// The parameter naming the elements command `id` acts on, which a step's `on` fills: one id
/// (`id`, `node`, `root`) or a list (`ids`, `groupIds`), as the command's schema types it.
pub fn targets(id: &str) -> Option<&'static str> {
    Some(match id {
        "element.delete" | "element.move" | "element.rotate" | "element.style" | "element.getStyle" | "element.duplicate" | "element.reorder"
        | "element.lock" | "arrange.group" | "edit.copy" | "layout.align" | "layout.distribute" | "layout.stack" => "ids",
        "arrange.ungroup" => "groupIds",
        "element.set"
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
        _ => return None,
    })
}

/// Live UUID references besides the target, audited against the command resolvers. A clipboard
/// payload's connector bindings can name live elements. Source refs are conditional inputs:
/// `premises::paste_sources` holds them only when placement tests whether they are still here.
const PASTE: &[&str] = &["page", "payload.elements[].start.binding.target", "payload.elements[].end.binding.target"];
const PASTE_TYPES: &[(&str, &[&str])] =
    &[("page", &["Page"]), ("payload.elements[].start.binding.target", &["Element"]), ("payload.elements[].end.binding.target", &["Element"])];

pub fn objects(id: &str) -> &'static [&'static str] {
    match id {
        "edit.paste" => PASTE,
        "element.add" => &["page", "elements[].parent"],
        "element.set" => &["set.parent"],
        "element.resize" => &["ids"],
        "frame.add" => &["page", "children"],
        "connector.add" => &["page", "from.element", "to.element"],
        "connector.bind" => &["to.element"],
        "stroke.add" | "template.insert" | "card.add" | "image.insert" | "page.rename" | "page.duplicate" | "page.delete" | "page.move"
        | "page.activate" => &["page"],
        "file.export" => &["page", "frame"],
        "file.render" => &["target.frame", "target.objectId"],
        _ => &[],
    }
}

/// Types follow what the resolver accepts. Frames, groups and mind-map nodes are all Element
/// records. A render's integer page is an ordinal; its UUID string is typed but not marked as
/// an integer object reference.
pub fn object_types(id: &str) -> &'static [(&'static str, &'static [&'static str])] {
    match id {
        "element.add" => &[("page", &["Page"]), ("elements[].parent", &["Element"])],
        "element.set" => &[("id", &["Element"]), ("set.parent", &["Element"])],
        "element.resize" => &[("id", &["Element"]), ("ids", &["Element"])],
        "frame.add" => &[("page", &["Page"]), ("children", &["Element"])],
        "connector.add" => &[("page", &["Page"]), ("from.element", &["Element"]), ("to.element", &["Element"])],
        "connector.bind" => &[("id", &["Element"]), ("to.element", &["Element"])],
        "file.export" => &[("page", &["Page"]), ("frame", &["Element"])],
        "file.render" => &[("target.page", &["Page"]), ("target.frame", &["Element"]), ("target.objectId", &["Element"])],
        "edit.paste" => PASTE_TYPES,
        "stroke.add" | "template.insert" | "card.add" | "image.insert" | "page.rename" | "page.duplicate" | "page.delete" | "page.move"
        | "page.activate" => &[("page", &["Page"])],
        _ => match targets(id) {
            Some("ids") => &[("ids", &["Element"])],
            Some("id") => &[("id", &["Element"])],
            Some("node") => &[("node", &["Element"])],
            Some("root") => &[("root", &["Element"])],
            Some("groupIds") => &[("groupIds", &["Element"])],
            _ => &[],
        },
    }
}

/// Element or page ids a command's code reads that are not what it acts on, with why: `on` never
/// fills them.
pub const NOT_TARGETS: &[(&str, &str, &str)] = &[
    ("frame.add", "children", "the elements a new frame takes in are the new frame's data"),
    ("page.rename", "page", "a page is a part: the run's `target` or a step's `at.part`, not `on`"),
    ("page.duplicate", "page", "a page is a part: the run's `target` or a step's `at.part`, not `on`"),
    ("page.delete", "page", "a page is a part: the run's `target` or a step's `at.part`, not `on`"),
    ("page.move", "page", "a page is a part: the run's `target` or a step's `at.part`, not `on`"),
    ("page.activate", "page", "a page is a part, and showing one is the editor's navigation"),
    ("element.resize", "ids", "`ids` scales several elements as one; `on` names the one element `id` resizes"),
];
