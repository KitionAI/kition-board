//! Cards, pictures and links (design note 3, 5.1): `card.create`, `card.setTarget`,
//! `image.insert`, `image.setAlt`, `links.retarget`.
//!
//! A card points at a workspace file (`target.path`, relative to the board's folder or rooted with
//! `/`, plus the target's `docId` when known) or a web address. A picture is read once into a blob.
//! Files are read only through the host's lent files (`kition_kit::host`); a host that lends
//! nothing still gets its card, with the target state unknown (design note 3.3).

use std::collections::BTreeSet;

use kition_board_kboard as kboard;
use kition_board_kdm::{BoardModel, ELEMENT, Kind, Point, Rect, Size};
use kition_kit::kdm::{ObjectId, Tx};
use serde_json::{Map, Value, json};

use crate::Ctx;
use crate::edit;
use crate::params::{self, Result, invalid};

/// The largest picture `image.insert` takes.
pub const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;

/// A card's kind from its target's extension.
pub fn kind_for_path(path: &str) -> &'static str {
    let extension = path.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase()).unwrap_or_default();
    match extension.as_str() {
        "md" | "markdown" => "note",
        "ktable" => "table",
        "kboard" => "board",
        _ => "file",
    }
}

/// A target path as stored: relative or rooted, never absolute on the host or a URI.
fn check_path(path: &str) -> Result<String> {
    let path = path.trim();
    if path.is_empty() || path.ends_with('/') {
        return Err(invalid("`file` must name a file"));
    }
    if path.contains('\\') || path.contains('\0') || path.contains("://") || path.as_bytes().get(1) == Some(&b':') {
        return Err(invalid("`file` must be a workspace path, not a host path or a URI"));
    }
    Ok(path.to_owned())
}

fn check_url(url: &str) -> Result<String> {
    if ["http://", "https://", "mailto:"].iter().any(|scheme| url.starts_with(scheme)) && url.len() > 8 && !url.chars().any(char::is_whitespace) {
        Ok(url.to_owned())
    } else {
        Err(invalid("`url` must be an http, https or mailto address"))
    }
}

/// `view` as stored: an object or absent.
fn view(params: &Value) -> Result<Option<Value>> {
    Ok(params::object(params, "view")?.map(|view| Value::Object(view.clone())))
}

/// The `card` value for `file` or `url` (exactly one), with `kind` checked against the target.
fn card_value(ctx: &Ctx<'_>, params: &Value, title: Option<&str>) -> Result<Value> {
    let kind = params::opt_string(params, "kind")?;
    let view = view(params)?;
    match (params::opt_string(params, "file")?, params::opt_string(params, "url")?) {
        (Some(file), None) => {
            let path = check_path(file)?;
            let inferred = kind_for_path(&path);
            let kind = match kind {
                None => inferred,
                Some(kind) if kind == inferred || (kind == "file" && inferred != "link") => kind_static(kind),
                Some(kind) => return Err(invalid(format!("a {kind} card cannot point at {path}"))),
            };
            let mut target = Map::new();
            target.insert("path".into(), json!(path));
            if kind == "board" {
                // A board card never shows the board it is on; the target's docId comes from its file.
                if let Some(doc_id) = board_doc_id(&path) {
                    if doc_id == ctx.doc_id {
                        return Err(invalid("a board card cannot show the board it is on"));
                    }
                    target.insert("docId".into(), json!(doc_id));
                }
            }
            let mut card = json!({ "kind": kind, "target": Value::Object(target) });
            if let Some(view) = view {
                insert(&mut card, "view", view);
            }
            Ok(card)
        }
        (None, Some(url)) => {
            if kind.is_some_and(|kind| kind != "link") {
                return Err(invalid("a web address makes a link card"));
            }
            let mut card = json!({ "kind": "link", "url": check_url(url)? });
            if let Some(title) = title {
                insert(&mut card, "title", json!(title));
            }
            Ok(card)
        }
        _ => Err(invalid("give exactly one of `file` and `url`")),
    }
}

/// Sets `key` on a JSON object (no-op on anything else).
fn insert(object: &mut Value, key: &str, value: Value) {
    if let Some(object) = object.as_object_mut() {
        object.insert(key.to_owned(), value);
    }
}

fn kind_static(kind: &str) -> &'static str {
    ["note", "table", "board", "file", "link"].into_iter().find(|known| *known == kind).unwrap_or("file")
}

/// The `docId` a `.kboard` file names, when the host lends it.
pub fn board_doc_id(path: &str) -> Option<String> {
    let size = kition_kit::host::size(path).ok()?;
    if size > 512 * 1024 * 1024 {
        return None;
    }
    let bytes = kition_kit::host::read(path).ok()?;
    kition_board_kdm::file::doc_id_of(&bytes)
}

/// Where a new card or picture goes without a `position`: right of the page's content.
fn default_position(model: &BoardModel, page: &kition_board_kdm::PageView) -> Point {
    model.content_bounds(page).map_or(Point::new(40.0, 40.0), |bounds: Rect| Point::new(bounds.x1 + 40.0, bounds.y0))
}

/// `card.create { page?, file? | url?, kind?, view?, position?, size?, caption?, title? }`.
pub fn create(model: &BoardModel, tx: &mut Tx<'_>, ctx: &Ctx<'_>, params: &Value) -> Result<Value> {
    let page = params::page(model, params, ctx.active_page)?;
    let title = params::opt_string(params, "title")?;
    let card = card_value(ctx, params, title)?;
    let position = params::opt_point(params, "position")?.unwrap_or_else(|| default_position(model, page));
    let size = match params.get("size").filter(|value| !value.is_null()) {
        Some(size) => params::size(size, "size")?,
        None => Size::new(320.0, 200.0),
    };
    let mut fields = Map::new();
    fields.insert("kind".into(), json!(Kind::Card.as_str()));
    fields.insert("position".into(), position.to_object());
    fields.insert("size".into(), size.to_json());
    fields.insert("card".into(), card.clone());
    let id = tx.create(ELEMENT, fields)?;
    if let Some(caption) = params::opt_string(params, "caption")?.filter(|caption| !caption.is_empty()) {
        tx.text_insert(&id, "text", 0, caption)?;
    }
    edit::append(tx, &page.id, &id)?;
    Ok(json!({ "created": [id.as_str()], "card": card }))
}

/// `card.setTarget { id, file? | url?, kind?, view?, title? }`: the caption stays.
pub fn set_target(model: &BoardModel, tx: &mut Tx<'_>, ctx: &Ctx<'_>, params: &Value) -> Result<Value> {
    let id = params::id(params, "id")?;
    let element = params::element(model, &id)?;
    if element.kind != Kind::Card {
        return Err(invalid(format!("{} is not a card", id.as_str())));
    }
    if element.locked {
        return Err(params::locked(&id));
    }
    let card = card_value(ctx, params, params::opt_string(params, "title")?)?;
    tx.set(&id, "card", card.clone())?;
    Ok(json!({ "updated": [id.as_str()], "card": card }))
}

/// The MIME of picture bytes, from their signature.
pub fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    let starts = |prefix: &[u8]| bytes.starts_with(prefix);
    if starts(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if starts(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Some("image/gif")
    } else if starts(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else {
        let head = String::from_utf8_lossy(bytes.get(..bytes.len().min(512)).unwrap_or_default()).to_ascii_lowercase();
        (head.trim_start().starts_with("<svg") || (head.contains("<?xml") && head.contains("<svg"))).then_some("image/svg+xml")
    }
}

/// A picture's pixel size from its header (PNG, GIF, JPEG); `None` when unknown.
pub fn pixel_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let four = |at: usize| bytes.get(at..at + 4).and_then(|b| <[u8; 4]>::try_from(b).ok());
    let two = |at: usize| bytes.get(at..at + 2).and_then(|b| <[u8; 2]>::try_from(b).ok());
    let be32 = |at: usize| four(at).map(u32::from_be_bytes);
    let le16 = |at: usize| two(at).map(|b| u32::from(u16::from_le_bytes(b)));
    let be16 = |at: usize| two(at).map(|b| u32::from(u16::from_be_bytes(b)));
    match sniff_mime(bytes)? {
        "image/png" => Some((be32(16)?, be32(20)?)),
        "image/gif" => Some((le16(6)?, le16(8)?)),
        "image/jpeg" => {
            let mut at = 2;
            while let (Some(0xff), Some(marker)) = (bytes.get(at).copied(), bytes.get(at + 1).copied()) {
                let length = usize::try_from(be16(at + 2)?).ok()?;
                if (0xc0..=0xcf).contains(&marker) && ![0xc4, 0xc8, 0xcc].contains(&marker) {
                    return Some((be16(at + 7)?, be16(at + 5)?));
                }
                at = at.checked_add(2 + length)?;
            }
            None
        }
        _ => None,
    }
    .filter(|(w, h)| *w > 0 && *h > 0)
}

/// `image.insert { page?, file? | data?, mime?, position?, size?, alt? }`: `file` is a workspace
/// picture read once through the host; `data` base64 bytes (the editor's paste).
pub fn image_insert(model: &BoardModel, tx: &mut Tx<'_>, ctx: &mut Ctx<'_>, params: &Value) -> Result<Value> {
    let page = params::page(model, params, ctx.active_page)?;
    let bytes = match (params::opt_string(params, "file")?, params::opt_string(params, "data")?) {
        (Some(file), None) => {
            let path = check_path(file)?;
            let size = kition_kit::host::size(&path).map_err(|error| invalid(format!("cannot read {path}: {error}")))?;
            if size > MAX_IMAGE_BYTES as u64 {
                return Err(invalid(format!("{path} is larger than {MAX_IMAGE_BYTES} bytes")));
            }
            kition_kit::host::read(&path).map_err(|error| invalid(format!("cannot read {path}: {error}")))?
        }
        (None, Some(data)) => {
            if data.len() > MAX_IMAGE_BYTES / 3 * 4 + 4 {
                return Err(invalid(format!("`data` is larger than {MAX_IMAGE_BYTES} bytes")));
            }
            if !data.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=' | b'\n' | b'\r')) {
                return Err(invalid("`data` must be base64"));
            }
            kboard::base64_decode(data)
        }
        _ => return Err(invalid("give exactly one of `file` and `data`")),
    };
    let sniffed = sniff_mime(&bytes).ok_or_else(|| invalid("the file is not a PNG, JPEG, GIF, WebP or SVG picture"))?;
    if let Some(mime) = params::opt_string(params, "mime")?
        && mime != sniffed
    {
        return Err(invalid(format!("the bytes are {sniffed}, not {mime}")));
    }
    let size = match params.get("size").filter(|value| !value.is_null()) {
        Some(size) => params::size(size, "size")?,
        None => match pixel_size(&bytes) {
            Some((w, h)) => {
                let scale = (800.0 / f64::from(w)).min(1.0);
                Size::new(f64::from(w) * scale, f64::from(h) * scale)
            }
            None => Size::new(320.0, 240.0),
        },
    };
    let position = params::opt_point(params, "position")?.unwrap_or_else(|| default_position(model, page));
    let alt = params::opt_string(params, "alt")?;
    // The bytes are stored once every parameter is checked: a refused call stores nothing.
    let blob = ctx.blobs.put(&bytes, sniffed);
    let mut fields = Map::new();
    fields.insert("kind".into(), json!(Kind::Image.as_str()));
    fields.insert("position".into(), position.to_object());
    fields.insert("size".into(), size.to_json());
    fields.insert("image".into(), json!({ "hash": blob.hash, "mime": blob.mime, "size": blob.size }));
    if let Some(alt) = alt {
        fields.insert("alt".into(), json!(alt));
    }
    let id = tx.create(ELEMENT, fields)?;
    edit::append(tx, &page.id, &id)?;
    Ok(json!({ "created": [id.as_str()], "blob": blob.hash }))
}

/// `image.setAlt { id, alt }`.
pub fn image_set_alt(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let id = params::id(params, "id")?;
    let element = params::element(model, &id)?;
    if element.kind != Kind::Image {
        return Err(invalid(format!("{} is not a picture", id.as_str())));
    }
    if element.locked {
        return Err(params::locked(&id));
    }
    let alt = params::string(params, "alt")?;
    tx.set(&id, "alt", if alt.is_empty() { Value::Null } else { json!(alt) })?;
    Ok(json!({ "updated": [id.as_str()] }))
}

/// `links.retarget { from, to }` (KitLinks LK2): cards whose target is `from`, or inside the
/// folder `from`, point at `to` instead; `docId`, `view` and the caption stay. Idempotent.
pub fn retarget(model: &BoardModel, tx: &mut Tx<'_>, params: &Value) -> Result<Value> {
    let from = check_path(params::string(params, "from")?)?;
    let to = check_path(params::string(params, "to")?)?;
    let mut changed = BTreeSet::new();
    for page in model.pages() {
        for element in model.elements_of(page).filter(|element| element.kind == Kind::Card) {
            let Some(mut card) = element.card.clone() else { continue };
            let Some(path) = card.pointer("/target/path").and_then(Value::as_str).map(str::to_owned) else { continue };
            let next = if path == from {
                to.clone()
            } else if let Some(rest) = path.strip_prefix(&format!("{from}/")) {
                format!("{to}/{rest}")
            } else {
                continue;
            };
            if let Some(target) = card.pointer_mut("/target") {
                insert(target, "path", json!(next));
            }
            tx.set(&element.id, "card", card)?;
            changed.insert(element.id.clone());
        }
    }
    let ids: Vec<ObjectId> = changed.into_iter().collect();
    Ok(json!({ "retargeted": edit::id_list(&ids) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_kinds_follow_the_extension() {
        assert_eq!(kind_for_path("notes/Plan.md"), "note");
        assert_eq!(kind_for_path("Budget.KTABLE"), "table");
        assert_eq!(kind_for_path("Map.kboard"), "board");
        assert_eq!(kind_for_path("photo.png"), "file");
        assert_eq!(kind_for_path("README"), "file");
    }

    #[test]
    fn paths_are_workspace_paths() {
        assert!(check_path("../plans/Roadmap.md").is_ok(), "relative to the board's folder");
        assert!(check_path("/plans/Roadmap.md").is_ok());
        assert!(check_path("C:/x.md").is_err());
        assert!(check_path("file:///x.md").is_err());
        assert!(check_path("a\\b.md").is_err());
        assert!(check_path("").is_err());
    }

    #[test]
    fn pictures_are_recognised_by_their_bytes() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&640u32.to_be_bytes());
        png.extend_from_slice(&480u32.to_be_bytes());
        assert_eq!(sniff_mime(&png), Some("image/png"));
        assert_eq!(pixel_size(&png), Some((640, 480)));
        assert_eq!(pixel_size(b"GIF89a\x10\x00\x20\x00"), Some((16, 32)));
        assert_eq!(sniff_mime(b"<svg xmlns='http://www.w3.org/2000/svg'/>"), Some("image/svg+xml"));
        assert_eq!(sniff_mime(b"plain text"), None);
        assert_eq!(pixel_size(b"\xff\xd8\xff\xe0\xff\xff"), None, "a broken JPEG never panics");
    }
}
