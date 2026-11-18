//! `kition-board-model`: the Board kit's commands (design note 5.1).
//!
//! Every mutating command is one KDM transaction run by the document's session: it reads the view
//! (`kition_board_kdm::BoardModel`, the board before the command) and writes the transaction.
//! Targets are explicit ids (PT10), parameters have a `ParameterSchemaSpec` ([`spec`]), and empty
//! or malformed parameters answer a typed error (`invalid-params: …`, `not-found: …`,
//! `locked: …`), never a panic (E3). Bound connector ends are recomputed in the command that moves
//! their target (design note 1.4).
//!
//! Not in the engine: a graph layout command, a text authoring format and file import (design
//! note 6).

use kition_board_kdm::BoardModel;
use kition_kit::kdm::{ObjectId, Tx};
use kition_kit::session::BlobSink;
use serde_json::Value;

pub mod acts;
pub mod cards;
pub mod clipboard;
pub mod connectors;
pub mod edit;
pub mod elements;
pub mod fields;
pub mod layout;
pub mod mindmap;
pub mod pages;
pub mod params;
pub mod premises;
pub mod spec;
pub mod templates;

pub use params::Result;

/// What a command may use besides the view and the transaction.
pub struct Ctx<'a> {
    /// Where `image.insert` puts picture bytes (the document's blob store).
    pub blobs: &'a mut dyn BlobSink,
    /// The board's KDM document id (a board card must not show its own board).
    pub doc_id: &'a str,
    /// The page the editor shows: where commands without `page` work.
    pub active_page: Option<&'a ObjectId>,
}

/// Whether `id` is a command of the board.
pub fn is_known(id: &str) -> bool {
    spec::find(id).is_some()
}

/// Whether `id` changes the board.
pub fn mutates(id: &str) -> bool {
    spec::find(id).is_some_and(|spec| spec.mutates)
}

/// Runs a mutating command inside the session's transaction. `edit.undo`, `edit.redo` and
/// `page.activate` belong to the document and are refused here.
///
/// The command's transaction ends healthy. Commands check what rules V1 to V8 repair; a group
/// whose last members a command deletes or moves out (V9) is removed by the repair in the same
/// transaction, one undo step with the command. Every other replica, and this one reopened after a
/// crash, repairs on import and open, so a state left for the repair would make the replica that
/// edited disagree with its own recovered copy (regressions/issue-coedit-local-edit-left-a-repair).
pub fn run(model: &BoardModel, tx: &mut Tx<'_>, ctx: &mut Ctx<'_>, id: &str, params: &Value) -> Result<Value> {
    let answer = dispatch(model, tx, ctx, id, params)?;
    if emptied_a_group(model, tx) {
        kition_board_kdm::validate::repair(tx)?;
    }
    Ok(answer)
}

/// Whether the transaction took every member out of a group that had members before it: each of
/// them is deleted or has another parent now. Reads only the groups' members, not the board (the
/// commit budget); the repair it calls for finds anything a command added meanwhile.
fn emptied_a_group(model: &BoardModel, tx: &Tx<'_>) -> bool {
    model.containers().filter(|(group, _)| group.kind == kition_board_kdm::Kind::Group).any(|(group, members)| {
        tx.exists(&group.id)
            && members
                .iter()
                .all(|member| !tx.exists(member) || tx.get(member, "parent").ok().as_ref().and_then(Value::as_str) != Some(group.id.as_str()))
    })
}

fn dispatch(model: &BoardModel, tx: &mut Tx<'_>, ctx: &mut Ctx<'_>, id: &str, params: &Value) -> Result<Value> {
    if !params.is_object() {
        return Err(params::invalid("params must be an object"));
    }
    match id {
        "element.create" => elements::create(model, tx, ctx, params),
        "element.update" => elements::update(model, tx, params),
        "element.delete" => elements::delete(model, tx, params),
        "element.move" => elements::move_elements(model, tx, params),
        "element.resize" => elements::resize(model, tx, params),
        "element.rotate" => elements::rotate(model, tx, params),
        "element.style" => elements::style(model, tx, params),
        "element.setText" => elements::set_text(model, tx, params),
        "text.mark" => elements::mark(model, tx, params),
        "element.reorder" => elements::reorder(model, tx, params),
        "element.lock" => elements::lock(model, tx, params),
        "element.group" => elements::group(model, tx, params),
        "element.ungroup" => elements::ungroup(model, tx, params),
        "element.duplicate" => clipboard::duplicate(model, tx, params),
        "element.paste" => clipboard::paste(model, tx, ctx, params),
        "frame.create" => elements::frame_create(model, tx, ctx, params),
        "frame.fit" => elements::frame_fit(model, tx, params),
        "stroke.create" => elements::stroke_create(model, tx, ctx, params),
        "connector.create" => connectors::create(model, tx, ctx, params),
        "connector.bind" => connectors::bind(model, tx, params),
        "connector.setLabel" => connectors::set_label(model, tx, params),
        "connector.setWaypoints" => connectors::set_waypoints(model, tx, params),
        "card.create" => cards::create(model, tx, ctx, params),
        "card.setTarget" => cards::set_target(model, tx, ctx, params),
        "image.insert" => cards::image_insert(model, tx, ctx, params),
        "image.setAlt" => cards::image_set_alt(model, tx, params),
        "links.retarget" => cards::retarget(model, tx, params),
        "layout.align" => layout::align(model, tx, params),
        "layout.distribute" => layout::distribute(model, tx, params),
        "layout.stack" => layout::stack(model, tx, params),
        "layout.mindmap" => mindmap::layout(model, tx, params),
        "mindmap.addChild" => mindmap::add_child(model, tx, params),
        "mindmap.addSibling" => mindmap::add_sibling(model, tx, params),
        "mindmap.collapse" => mindmap::collapse(model, tx, params),
        "mindmap.setDirection" => mindmap::set_direction(model, tx, params),
        "template.insert" => templates::insert(model, tx, ctx, params),
        "page.create" => pages::create(model, tx, params),
        "page.rename" => pages::rename(model, tx, params),
        "page.duplicate" => pages::duplicate(model, tx, params),
        "page.delete" => pages::delete(model, tx, params),
        "page.move" => pages::move_page(model, tx, params),
        other => Err(params::invalid(format!("{other} is not a board transaction"))),
    }
}

/// Runs a command that reads the board without changing it.
pub fn read(model: &BoardModel, id: &str, params: &Value) -> Result<Value> {
    match id {
        "clipboard.copy" => clipboard::copy(model, params),
        other => Err(params::invalid(format!("{other} is not a reading command"))),
    }
}
