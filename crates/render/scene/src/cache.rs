//! The engine extra `elementSvg` (design note 4.2): fragments by id with an appearance revision
//! per element, so the editor caches them by (id, rev) and fetches only what changed.
//!
//! The revision moves whenever a fragment's text or box changes (an element's own position and
//! rotation are not in a local fragment, so moving it keeps its revision; a connector's route is,
//! so moving its target moves it). Revisions come from one counter per document and never repeat.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use kition_board_kdm::BoardModel;
use kition_kit::kdm::ObjectId;
use kition_kit::session::BlobSource;
use serde_json::{Map, Value, json};

use crate::fragment::{Fragment, Lod, Options, Pictures, fragment};
use crate::style::Colors;

/// The most ids one request may name.
pub const MAX_IDS: usize = 500;

/// The revisions handed out so far.
#[derive(Debug, Default)]
pub struct Revisions {
    seen: HashMap<(String, Lod), (u64, u64)>,
    next: u64,
}

fn digest(fragment: &Fragment) -> u64 {
    let mut hasher = DefaultHasher::new();
    fragment.svg.hash(&mut hasher);
    fragment.local.hash(&mut hasher);
    if let Some((w, h)) = fragment.size {
        w.to_bits().hash(&mut hasher);
        h.to_bits().hash(&mut hasher);
    }
    for rect in std::iter::once(&fragment.bounds).chain(fragment.slot.as_ref()) {
        for value in [rect.x0, rect.y0, rect.x1, rect.y1] {
            value.to_bits().hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn rect_json(rect: &kition_board_kdm::Rect) -> Value {
    json!([rect.x0, rect.y0, rect.x1, rect.y1])
}

impl Revisions {
    /// The revision of `id`'s fragment now.
    fn revision(&mut self, id: &str, lod: Lod, fragment: &Fragment) -> u64 {
        let digest = digest(fragment);
        if let Some((known, rev)) = self.seen.get(&(id.to_owned(), lod))
            && *known == digest
        {
            return *rev;
        }
        self.next += 1;
        self.seen.insert((id.to_owned(), lod), (digest, self.next));
        self.next
    }

    /// Answers `elementSvg(request)`:
    /// `{ ids: [...], lod?: "full" | "low", known?: { id: rev } }` →
    /// `{ fragments: [{ id, rev, svg, box, frame, size?, slot? }], unchanged: [...], missing: [...], fontEpoch }`.
    /// `frame` is `local` (place it at the element's position and rotation; `box` and `slot` are in
    /// that unrotated local frame) or `world` (`box` in world coordinates).
    pub fn answer(&mut self, model: &BoardModel, request: &Value, blobs: &dyn BlobSource) -> Result<Value, String> {
        let ids = request.get("ids").and_then(Value::as_array).ok_or("invalid-params: `ids` must be a list of element ids")?;
        if ids.len() > MAX_IDS {
            return Err(format!("invalid-params: at most {MAX_IDS} ids per request, not {}", ids.len()));
        }
        let lod = match request.get("lod").and_then(Value::as_str) {
            None | Some("full") => Lod::Full,
            Some("low") => Lod::Low,
            Some(other) => return Err(format!("invalid-params: `lod` is full or low, not {other:?}")),
        };
        let empty = Map::new();
        let known = request.get("known").and_then(Value::as_object).unwrap_or(&empty);
        let options = Options { colors: Colors::Css, lod, pictures: Pictures::Reference, blobs };
        let (mut fragments, mut unchanged, mut missing) = (Vec::new(), Vec::new(), Vec::new());
        for id in ids {
            let Some(id) = id.as_str() else { return Err("invalid-params: every id must be text".into()) };
            let Some(element) = model.element(&ObjectId::from_string(id)) else {
                missing.push(json!(id));
                continue;
            };
            let piece = fragment(model, element, &options);
            let rev = self.revision(id, lod, &piece);
            if known.get(id).and_then(Value::as_u64) == Some(rev) {
                unchanged.push(json!(id));
                continue;
            }
            let mut entry = json!({ "id": id, "rev": rev, "svg": piece.svg, "box": rect_json(&piece.bounds), "frame": if piece.local { "local" } else { "world" } });
            if let Some(object) = entry.as_object_mut() {
                if let Some(slot) = piece.slot {
                    object.insert("slot".into(), rect_json(&slot));
                }
                if let Some((w, h)) = piece.size {
                    object.insert("size".into(), json!([w, h]));
                }
            }
            fragments.push(entry);
        }
        Ok(json!({ "fragments": fragments, "unchanged": unchanged, "missing": missing, "fontEpoch": kition_board_text::font_epoch() }))
    }
}
