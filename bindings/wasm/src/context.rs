//! Explicit call context (kition-sdk `KitDocument::editor_state`, `set_context`; Kit Process
//! Protocol section 7a; task card C4/C5/C6-board): the editor state a command may read besides its
//! parameters, taken and put back without an edit, set for one call from what the call says it acts
//! in, and kept out of an agent's commands.
//!
//! A board's only editor state is the **page the editor shows**. Commands that make content
//! (`element.add`, `frame.add`, `connector.add`, …) and `file.export` work there when
//! their parameters name no `page`; nothing else reads it. There is no selection.
//!
//! A call's **part** is a page: `{"page": "<id>"}`, the page's UUID as `board_inspect`
//! lists it (the manifest declares the `page` noun), or `{"page": n}`, counted from 1 in the
//! board's order. The UUID names the same page when pages are added, moved or deleted.
//!
//! A call's **`on`** names elements, by id. A board keeps no selection, so `on` reaches a command
//! only through the parameter it marks as its targets (`id`, `ids`, `groupIds`, `node`, `root`);
//! `set_context` checks that every id is an element (on the part's page, when the call has one) and
//! then refuses to make it a selection as not available.

use kition_kit::CallContext;
use kition_kit::kdm::ObjectId;
use serde_json::{Value, json};

use crate::BoardDocument;

/// What [`kition_kit::KitDocument::inspect`] says of the editor rather than of the board, as JSON
/// Pointers: the page the editor shows (`/page`) and that page's document area (`/area`). A
/// headless host leaves them out of what an agent reads (`kition_headless::KitInfo::volatile`);
/// every page's own facts, its area included, are in `/pages`.
pub const EDITOR_STATE: &[&str] = &["/page", "/area"];

const LISTED: &str = "the pages' ids are in board_inspect's `pages`";

/// Commands whose work is moving the editor: they keep their effect for an agent too.
pub(crate) fn moves_the_editor(command: &str, params: &Value) -> bool {
    command == "page.activate" || command == "page.add" && params.get("activate").and_then(Value::as_bool) == Some(true)
}

impl BoardDocument {
    /// The page a call's part names (`{"page": n}` from 1, or `{"page": "<id>"}`).
    fn part_page(&self, part: &Value) -> Result<ObjectId, String> {
        let Some(fields) = part.as_object() else { return Err(format!("the part {part} is not `{{\"page\": n}}`: {LISTED}")) };
        if let Some(other) = fields.keys().find(|key| *key != "page") {
            return Err(format!("the part has `{other}`, which names no part of a board: a part is `{{\"page\": n}}`; {LISTED}"));
        }
        let pages = self.model().pages();
        match fields.get("page") {
            Some(Value::String(id)) => {
                let id = ObjectId::from_string(id);
                if self.model().page(&id).is_some() { Ok(id) } else { Err(format!("the board has no page `{}`: {LISTED}", id.as_str())) }
            }
            Some(Value::Number(number)) => match number.as_u64().filter(|n| *n >= 1) {
                Some(n) => usize::try_from(n - 1)
                    .ok()
                    .and_then(|index| pages.get(index))
                    .map(|page| page.id.clone())
                    .ok_or_else(|| format!("the part names page {n}, and the board has {} pages (counted from 1): {LISTED}", pages.len())),
                None => Err(format!("the part names page {number}: pages are counted from 1 (or give the page's id); {LISTED}")),
            },
            _ => Err(format!("the part {part} names no page: a part is `{{\"page\": n}}` (from 1) or a page's id; {LISTED}")),
        }
    }

    /// The editor state ([`KitDocument::editor_state`]): the page the editor shows (`page`), and
    /// what chooses the page shown in its place once it is gone (`at`, the shown page's index;
    /// `fallback` and `cameFrom` while there are any, [`BoardDocument::settle`]).
    ///
    /// [`KitDocument::editor_state`]: kition_kit::KitDocument::editor_state
    pub(crate) fn editor_snapshot(&self) -> Value {
        let mut state = serde_json::Map::new();
        state.insert("page".into(), json!(self.active_page.as_ref().map(ObjectId::as_str)));
        state.insert("at".into(), json!(self.shown_at));
        if let Some(fallback) = &self.fallback {
            state.insert("fallback".into(), json!(fallback.as_str()));
        }
        if let Some(came_from) = &self.came_from {
            state.insert("cameFrom".into(), json!(came_from.as_str()));
        }
        Value::Object(state)
    }

    /// Puts back what [`Self::editor_snapshot`] answered. When the run deleted the page, the editor
    /// shows the page the person came from to it, else its neighbour (the page before it, or the
    /// one that took its place). No edit.
    pub(crate) fn put_back(&mut self, saved: &Value) -> Result<(), String> {
        let state = saved
            .as_object()
            .filter(|state| state.keys().all(|key| ["page", "at", "fallback", "cameFrom"].contains(&key.as_str())))
            .ok_or_else(|| format!("{saved} is not the editor state this kit answered"))?;
        let id = |key: &str| match state.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(id)) => Ok(Some(ObjectId::from_string(id))),
            Some(other) => Err(format!("{other} is not the editor state's {key}")),
        };
        let page = id("page")?;
        let fallback = id("fallback")?;
        let came_from = id("cameFrom")?;
        let at = match state.get("at") {
            None => 0,
            Some(at) => at.as_u64().and_then(|at| usize::try_from(at).ok()).ok_or_else(|| format!("{at} is not the editor state's at"))?,
        };
        self.active_page = page;
        self.shown_at = at;
        self.fallback = fallback;
        self.came_from = came_from;
        self.settle();
        Ok(())
    }

    /// Sets the page a call acts in (module docs). Everything is checked before anything changes;
    /// nothing of it is an edit.
    pub(crate) fn set_call_context(&mut self, context: &CallContext) -> Result<(), String> {
        if context.time.is_some() {
            return Err("a board has no timeline: setting the current time (`at.time`) is not available in this kit".into());
        }
        let page = context.part.as_ref().map(|part| self.part_page(part)).transpose()?;
        let model = self.model();
        for item in &context.on {
            let id = item.as_str().ok_or_else(|| format!("`on` holds {item}: an item is an element's id (a string)"))?;
            let on = model
                .page_of(&ObjectId::from_string(id))
                .filter(|_| model.element(&ObjectId::from_string(id)).is_some())
                .ok_or_else(|| format!("the board has no element `{id}`: the elements' ids are in board_query's `follow: \"pages.0.elements\"`"))?;
            if let Some(page) = &page
                && *page != on.id
            {
                let named = model.page(page).map_or_else(String::new, |page| page.name.clone());
                return Err(format!("the element `{id}` is on page `{}`, and the call's part is page `{named}`", on.name));
            }
        }
        if !context.on.is_empty() {
            return Err(
                "a board keeps no selection: selecting elements from `on` is not available; `on` fills a command's element parameter (`id`, `ids`, `groupIds`, `node` or `root`, marked as its targets in board_commands)"
                    .into(),
            );
        }
        if let Some(page) = page {
            self.active_page = Some(page);
            self.settle();
        }
        Ok(())
    }
}
