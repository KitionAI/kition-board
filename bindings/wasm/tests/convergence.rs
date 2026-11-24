//! Kition Board against the Kition platform convergence suite (L6): two to five replicas of a
//! board, edited through the engine's own commands (contract 1 `execute`, the contract 2 change
//! exchange and blob exchange, `open_with_replica` to join), and its native and WebAssembly
//! builds against one change log.
//!
//! What the suite's steps are on a board:
//! - the probe is the first page's back-most shape; `SetField` slots 0 and 1 are its fill and
//!   stroke colours (`element.style`);
//! - `Create`, `Delete`, `Duplicate` are shapes (`element.create`, `element.delete`,
//!   `element.duplicate`); `Reorder` is the stacking order of the page (`element.reorder`);
//! - the text is the first text element's (`element.setText`, written as insertions and deletions
//!   of the replica's text); `InsertBlob` is a picture (`image.insert`, an SVG blob);
//! - `Reparent` is frame membership: `which` 0 puts the first frame in the second, 1 the second in
//!   the first (`element.update { parent }`), so the two at once make a cycle the repair breaks
//!   (rule V5);
//! - the random edits also draw connectors bound to two elements, rebind ends, make frames and
//!   move elements into and out of them, make and retarget cards, and type into shape labels.
//!
//! After the suite, the board's own co-editing scenarios (`board_*` tests below) run two to five
//! replicas through the cases a whiteboard meets: concurrent moves of one shape, a connector bound
//! to a shape someone else deletes, frame membership, groups, cards and z-order, and replicas
//! offline for a while then merged.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::collections::BTreeMap;
use std::path::Path;

use base64::Engine as _;
use kition_board_kdm::{BoardModel, Kind};
use kition_board_kit::BoardDocument;
use kition_convergence::wasm::{self, WasmEngine};
use kition_convergence::{Blobs, KitAdapter, Net, Options, Outcome, Result, Rng, Script, state};
use kition_kit::KitDocument;
use kition_kit::kdm::{ActorId, ObjectId, Replica};
use kition_kit::session::{self, KitModel, KitSessionDocument};
use serde_json::{Value, json};

const NAME: &str = "Shared.kboard";
/// The two frames of the starting board, as `[x0, y0, x1, y1]`.
const FRAME_ONE: [f64; 4] = [40.0, 500.0, 440.0, 800.0];
const FRAME_TWO: [f64; 4] = [600.0, 500.0, 1000.0, 800.0];
const SCHEMA: &str = include_str!("../../../crates/kdm/kdm-model/model/schema.json");

/// CJK ideographs ("middle", "text") and an emoji, by code point, for typed text.
const CJK_MIDDLE: char = '\u{4E2D}';
const CJK_TEXT: char = '\u{6587}';
const EMOJI: char = '\u{1F600}';

struct BoardKit;

fn exec(doc: &mut BoardDocument, command: &str, params: Value) -> Result<Value> {
    doc.execute(command, params).map_err(|error| format!("{command}: {error}"))
}

/// Runs a command; a refusal because of the board's state is no edit (`false`).
fn attempt(doc: &mut BoardDocument, command: &str, params: Value) -> bool {
    doc.execute(command, params).is_ok()
}

/// `(64-char hex SHA-256, u64 LE length, bytes) * N`, as `blobs_bundle`/`add_blobs` move them.
fn decode_blobs(bytes: &[u8]) -> Result<Blobs> {
    let mut out = Blobs::new();
    let mut at = 0;
    while at < bytes.len() {
        let hash = std::str::from_utf8(bytes.get(at..at + 64).ok_or("truncated hash")?).map_err(|e| e.to_string())?.to_owned();
        let len = u64::from_le_bytes(bytes.get(at + 64..at + 72).ok_or("truncated length")?.try_into().map_err(|_| "length")?) as usize;
        let data = bytes.get(at + 72..at + 72 + len).ok_or("truncated data")?;
        out.insert(hash, data.to_vec());
        at += 72 + len;
    }
    Ok(out)
}

/// The first page's unlocked elements of `kind` (any kind for `None`), back to front: the same on
/// every replica with the same state.
fn elements(doc: &BoardDocument, kind: Option<Kind>) -> Vec<String> {
    let model = doc.model();
    let Some(page) = model.pages().first() else { return Vec::new() };
    model
        .elements_of(page)
        .filter(|element| !element.locked && kind.is_none_or(|kind| element.kind == kind))
        .map(|element| element.id.as_str().to_owned())
        .collect()
}

/// The first page's elements a connector end can bind to, back to front.
fn bindable(doc: &BoardDocument) -> Vec<String> {
    let model = doc.model();
    let Some(page) = model.pages().first() else { return Vec::new() };
    model.elements_of(page).filter(|element| element.kind.bindable()).map(|element| element.id.as_str().to_owned()).collect()
}

/// The middle of `frame`'s box less a 120 by 80 shape's half, where a drop lands it in the frame.
fn frame_middle(doc: &BoardDocument, frame: &str) -> Option<[f64; 2]> {
    let model = doc.model();
    let rect = model.rect(model.element(&ObjectId::from_string(frame))?)?;
    Some([(rect.x0 + rect.x1) / 2.0 - 60.0, (rect.y0 + rect.y1) / 2.0 - 40.0])
}

fn probe(doc: &BoardDocument) -> Result<String> {
    elements(doc, Some(Kind::Shape)).into_iter().next().ok_or_else(|| "the board has no shape".to_owned())
}

/// The first text element and its text.
fn text_element(doc: &BoardDocument) -> Option<(String, String)> {
    let model = doc.model();
    let page = model.pages().first()?;
    model
        .elements_of(page)
        .find(|element| element.kind == Kind::Text && !element.locked)
        .map(|element| (element.id.as_str().to_owned(), element.text.clone()))
}

fn color(value: u64) -> String {
    format!("#{:06x}", value.wrapping_mul(0x9E37_79B9) & 0xff_ffff)
}

/// A small SVG picture whose colour derives from `seed`, base64-encoded for `image.insert`.
fn picture(seed: u32) -> String {
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\"><rect width=\"8\" height=\"8\" fill=\"{}\"/></svg>",
        color(u64::from(seed) + 1)
    );
    base64::engine::general_purpose::STANDARD.encode(svg.as_bytes())
}

/// The first text element's text with `edit` applied (scalar indices), written as one setText.
fn edit_text(doc: &mut BoardDocument, edit: impl FnOnce(&mut Vec<char>)) -> Result<bool> {
    let Some((id, text)) = text_element(doc) else { return Ok(false) };
    let mut chars: Vec<char> = text.chars().collect();
    edit(&mut chars);
    exec(doc, "element.setText", json!({ "id": id, "text": chars.into_iter().collect::<String>() }))?;
    Ok(true)
}

fn shape(doc: &mut BoardDocument, x: u64, y: u64, fill: &str) -> Result<String> {
    let made = exec(
        doc,
        "element.create",
        json!({ "elements": [{ "kind": "shape", "position": [x % 1200, y % 800], "size": [120, 80], "fillColor": fill }] }),
    )?;
    made["created"][0].as_str().map(str::to_owned).ok_or_else(|| format!("element.create answered {made}"))
}

impl KitAdapter for BoardKit {
    type Doc = BoardDocument;

    fn name(&self) -> &str {
        "kit.board"
    }

    fn schema(&self) -> &str {
        SCHEMA
    }

    /// Three shapes (the last two grouped), a text element, a connector bound from the first shape
    /// to the third, two empty frames and a card, on the board's first page.
    fn create(&self) -> Result<BoardDocument> {
        let mut doc = BoardDocument::create(NAME, 1920, 1080)?;
        let a = shape(&mut doc, 40, 40, "#3366cc")?;
        let b = shape(&mut doc, 240, 40, "#cc6633")?;
        let c = shape(&mut doc, 440, 40, "#33cc66")?;
        exec(&mut doc, "element.group", json!({ "ids": [b, c] }))?;
        exec(&mut doc, "element.create", json!({ "elements": [{ "kind": "text", "position": [40, 240], "text": "Hello board, a shared text." }] }))?;
        exec(&mut doc, "connector.create", json!({ "from": { "element": a }, "to": { "element": c } }))?;
        exec(&mut doc, "frame.create", json!({ "box": FRAME_ONE, "title": "One" }))?;
        exec(&mut doc, "frame.create", json!({ "box": FRAME_TWO, "title": "Two" }))?;
        exec(&mut doc, "card.create", json!({ "file": "notes/plan.md", "position": [1200, 40] }))?;
        Ok(doc)
    }

    fn join(&self, snapshot: &[u8], blobs: &Blobs) -> Result<BoardDocument> {
        BoardDocument::open_with_replica(NAME, &[], snapshot, &wasm::encode_blobs(blobs))
    }

    fn version(&self, doc: &mut BoardDocument) -> Result<Vec<u8>> {
        Ok(session::__version(doc))
    }

    fn export_changes(&self, doc: &mut BoardDocument, since: &[u8]) -> Result<Vec<u8>> {
        session::__export_changes(doc, since)
    }

    fn import_changes(&self, doc: &mut BoardDocument, bytes: &[u8]) -> Result<()> {
        KitSessionDocument::import_changes(doc, bytes).map(|_| ())
    }

    fn snapshot(&self, doc: &mut BoardDocument) -> Result<Vec<u8>> {
        session::__snapshot(doc)
    }

    fn state(&self, doc: &mut BoardDocument) -> Result<Value> {
        let snapshot = session::__snapshot(doc)?;
        let replica = Replica::load(BoardModel::schema(), &snapshot, ActorId::random()).map_err(|error| error.to_string())?;
        replica.state_json().map_err(|error| error.to_string())
    }

    fn undo(&self, doc: &mut BoardDocument) -> Result<bool> {
        KitSessionDocument::undo(doc).map(|(done, _)| done)
    }

    fn redo(&self, doc: &mut BoardDocument) -> Result<bool> {
        KitSessionDocument::redo(doc).map(|(done, _)| done)
    }

    fn random_edit(&self, doc: &mut BoardDocument, rng: &mut Rng) -> Result<Option<String>> {
        let all = elements(doc, None);
        let target = rng.pick(&all).cloned();
        let value = rng.next_u64() % 10_000;
        let (label, done) = match (rng.below(24), target) {
            (0 | 1, _) => ("shape", shape(doc, value, value / 7, &color(value)).is_ok()),
            (2, Some(id)) => ("fill", attempt(doc, "element.style", json!({ "ids": [id], "style": { "fillColor": color(value) } }))),
            (3, Some(id)) => ("stroke", attempt(doc, "element.style", json!({ "ids": [id], "style": { "strokeColor": color(value) } }))),
            (4, Some(id)) if all.len() > 2 => ("delete", attempt(doc, "element.delete", json!({ "ids": [id] }))),
            (5 | 6, Some(id)) => {
                let delta = [(value % 41) as i64 - 20, (value % 23) as i64 - 11];
                ("move", attempt(doc, "element.move", json!({ "ids": [id], "delta": delta })))
            }
            (7, Some(id)) => ("duplicate", attempt(doc, "element.duplicate", json!({ "ids": [id] }))),
            (8, Some(id)) => {
                let placement = ["front", "forward", "backward", "back"][rng.below(4)];
                ("reorder", attempt(doc, "element.reorder", json!({ "ids": [id], "placement": placement })))
            }
            (9, Some(id)) if all.len() > 1 => {
                let other = all[rng.below(all.len())].clone();
                if other == id {
                    return Ok(None);
                }
                ("group", attempt(doc, "element.group", json!({ "ids": [id, other] })))
            }
            (10, _) => {
                let groups = elements(doc, Some(Kind::Group));
                let Some(group) = rng.pick(&groups).cloned() else { return Ok(None) };
                ("ungroup", attempt(doc, "element.ungroup", json!({ "groupIds": [group] })))
            }
            (11, _) => {
                let word: String = [CJK_MIDDLE, CJK_TEXT, EMOJI].iter().collect();
                let text = format!("t{value}{word}");
                (
                    "text",
                    attempt(
                        doc,
                        "element.create",
                        json!({ "elements": [{ "kind": "text", "position": [value % 1200, 300 + value % 400], "text": text }] }),
                    ),
                )
            }
            (12, _) => {
                let Some((_, text)) = text_element(doc) else { return Ok(None) };
                let at = rng.below(text.chars().count() + 1);
                let insert = if rng.chance(20) { "\n".to_owned() } else { format!("{}", value % 10) };
                ("type", edit_text(doc, |chars| chars.splice(at..at, insert.chars()).for_each(drop))?)
            }
            (13, _) => {
                let Some((_, text)) = text_element(doc) else { return Ok(None) };
                let len = text.chars().count();
                if len == 0 {
                    return Ok(None);
                }
                let at = rng.below(len);
                let end = (at + 1 + rng.below(4)).min(len);
                ("erase", edit_text(doc, |chars| chars.drain(at..end).for_each(drop))?)
            }
            (14, _) => ("picture", self.script(doc, &Script::InsertBlob { seed: value as u32 })?),
            (15, Some(id)) => {
                let degrees = (value % 90) as f64;
                ("rotate", attempt(doc, "element.rotate", json!({ "ids": [id], "degrees": degrees })))
            }
            (16, _) => {
                let ends = bindable(doc);
                let (Some(from), Some(to)) = (rng.pick(&ends).cloned(), rng.pick(&ends).cloned()) else { return Ok(None) };
                let anchor = [(value % 11) as f64 / 10.0, (value % 7) as f64 / 6.0];
                ("connect", attempt(doc, "connector.create", json!({ "from": { "element": from }, "to": { "element": to, "anchor": anchor } })))
            }
            (17, _) => {
                let (connectors, ends) = (elements(doc, Some(Kind::Connector)), bindable(doc));
                let (Some(connector), Some(to)) = (rng.pick(&connectors).cloned(), rng.pick(&ends).cloned()) else { return Ok(None) };
                let terminal = if rng.chance(50) { "start" } else { "end" };
                let to = if rng.chance(25) { json!({ "point": [value % 900, value % 700] }) } else { json!({ "element": to }) };
                ("rebind", attempt(doc, "connector.bind", json!({ "id": connector, "terminal": terminal, "to": to })))
            }
            (18, _) => {
                let (x, y) = ((value % 1000) as f64, (value % 600) as f64);
                ("frame", attempt(doc, "frame.create", json!({ "box": [x, y, x + 300.0, y + 200.0] })))
            }
            (19, Some(id)) => {
                // Into a frame or out of one, as a drag's drop does.
                let frames = elements(doc, Some(Kind::Frame));
                let to = match rng.pick(&frames).cloned() {
                    Some(frame) if rng.chance(75) => frame_middle(doc, &frame).unwrap_or([value as f64 % 900.0, 900.0]),
                    _ => [(value % 900) as f64, 900.0],
                };
                ("drop", attempt(doc, "element.move", json!({ "ids": [id], "to": to, "reparent": true })))
            }
            (20, Some(id)) => {
                let frames = elements(doc, Some(Kind::Frame));
                let parent = rng.pick(&frames).cloned().map_or(Value::Null, Value::String);
                ("parent", attempt(doc, "element.update", json!({ "id": id, "set": { "parent": parent } })))
            }
            (21, _) => {
                let file = ["notes/plan.md", "data/table.ktable", "docs/brief.pdf", "boards/other.kboard"][rng.below(4)];
                ("card", attempt(doc, "card.create", json!({ "file": file, "position": [value % 1200, value % 800] })))
            }
            (22, _) => {
                let cards = elements(doc, Some(Kind::Card));
                let Some(card) = rng.pick(&cards).cloned() else { return Ok(None) };
                let target = if rng.chance(50) {
                    json!({ "id": card, "file": format!("notes/n{}.md", value % 5) })
                } else {
                    json!({ "id": card, "url": format!("https://example.com/{}", value % 5) })
                };
                ("retarget", attempt(doc, "card.setTarget", target))
            }
            (23, Some(id)) => {
                let Some(element) = doc.model().element(&ObjectId::from_string(&id)).cloned() else { return Ok(None) };
                if element.kind != Kind::Shape {
                    return Ok(None);
                }
                let mut chars: Vec<char> = element.text.chars().collect();
                let at = rng.below(chars.len() + 1);
                chars.splice(at..at, format!("{}", value % 100).chars());
                ("label", attempt(doc, "element.setText", json!({ "id": id, "text": chars.into_iter().collect::<String>() })))
            }
            _ => return Ok(None),
        };
        Ok(done.then(|| label.to_owned()))
    }

    fn script(&self, doc: &mut BoardDocument, step: &Script) -> Result<bool> {
        match step {
            Script::SetField { slot: 0, value } => {
                let id = probe(doc)?;
                exec(doc, "element.style", json!({ "ids": [id], "style": { "fillColor": color(u64::from(*value) + 1) } }))?;
            }
            Script::SetField { value, .. } => {
                let id = probe(doc)?;
                exec(doc, "element.style", json!({ "ids": [id], "style": { "strokeColor": color(u64::from(*value) + 7) } }))?;
            }
            Script::Create => {
                shape(doc, 640, 400, "#888888")?;
            }
            Script::Delete => {
                let id = probe(doc)?;
                exec(doc, "element.delete", json!({ "ids": [id] }))?;
            }
            Script::Duplicate => {
                let id = probe(doc)?;
                exec(doc, "element.duplicate", json!({ "ids": [id] }))?;
            }
            Script::Reorder { which } => {
                // The back-most and the next element, brought to the front.
                let all = elements(doc, None);
                let Some(id) = all.get(usize::from(*which)).cloned() else { return Ok(false) };
                exec(doc, "element.reorder", json!({ "ids": [id], "placement": "front" }))?;
            }
            Script::InsertText { at, text } => {
                let (at, text) = (*at, text.clone());
                return edit_text(doc, |chars| {
                    let at = at.min(chars.len());
                    chars.splice(at..at, text.chars()).for_each(drop);
                });
            }
            Script::DeleteText { at, len } => {
                let (at, len) = (*at, *len);
                return edit_text(doc, |chars| {
                    let end = (at + len).min(chars.len());
                    chars.drain(at.min(end)..end).for_each(drop);
                });
            }
            Script::SplitParagraph { at } => {
                let at = *at;
                return edit_text(doc, |chars| chars.insert(at.min(chars.len()), '\n'));
            }
            Script::MergeParagraph { at } => {
                let at = *at;
                return edit_text(doc, |chars| {
                    if chars.get(at) == Some(&'\n') {
                        chars.remove(at);
                    }
                });
            }
            Script::InsertBlob { seed } => {
                exec(doc, "image.insert", json!({ "data": picture(*seed), "position": [800, 600], "size": [40, 40] }))?;
            }
            Script::Reparent { which } => {
                // The first frame into the second (0) or the second into the first (1).
                let frames = elements(doc, Some(Kind::Frame));
                let (Some(one), Some(two)) = (frames.first().cloned(), frames.get(1).cloned()) else { return Ok(false) };
                let (child, parent) = if *which == 0 { (one, two) } else { (two, one) };
                exec(doc, "element.update", json!({ "id": child, "set": { "parent": parent } }))?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn text(&self, doc: &mut BoardDocument) -> Result<Option<String>> {
        Ok(text_element(doc).map(|(_, text)| text))
    }

    /// The view matches a fresh replica of the same state: the same pages and elements (ids and
    /// view state aside) and the same pixels.
    fn check_view(&self, doc: &mut BoardDocument) -> Result<()> {
        let snapshot = session::__snapshot(doc)?;
        let blobs = self.blobs(doc)?;
        let fresh = self.join(&snapshot, &blobs)?;
        let view = |doc: &BoardDocument| -> Result<Value> {
            let mut inspected = doc.inspect()?;
            if let Some(map) = inspected.as_object_mut() {
                for key in kition_board_kit::EDITOR_STATE.iter().map(|pointer| pointer.trim_start_matches('/')).chain(["canUndo", "canRedo"]) {
                    map.remove(key);
                }
            }
            Ok(inspected)
        };
        let (mine, theirs) = (view(doc)?, view(&fresh)?);
        if mine != theirs {
            return Err(format!(
                "the view differs from a fresh replica's: {}",
                kition_convergence::state::first_difference(&theirs, &mine).unwrap_or_default()
            ));
        }
        if doc.render_area(0, 0, 1280, 960, 320, 240)? != fresh.render_area(0, 0, 1280, 960, 320, 240)? {
            return Err("the view renders differently from a fresh replica".into());
        }
        Ok(())
    }

    fn blobs(&self, doc: &mut BoardDocument) -> Result<Blobs> {
        decode_blobs(&doc.blobs_bundle())
    }

    fn add_blobs(&self, doc: &mut BoardDocument, blobs: &Blobs) -> Result<()> {
        doc.add_blobs(&wasm::encode_blobs(blobs)).map(|_| ())
    }

    fn export_file(&self, doc: &mut BoardDocument) -> Result<Option<Vec<u8>>> {
        doc.prepare_save().unwrap();
        doc.save(None).map(Some)
    }

    /// A nudge of one element and the repaint of the visible area.
    fn keystroke(&self, doc: &mut BoardDocument, rng: &mut Rng) -> Result<()> {
        if let Some(id) = rng.pick(&elements(doc, None)).cloned() {
            exec(doc, "element.move", json!({ "ids": [id], "delta": [1, 0] }))?;
        }
        doc.render_area(0, 0, 1280, 720, 1280, 720).map(|_| ())
    }
}

#[test]
fn converges() {
    let report = kition_convergence::run(&BoardKit, &Options::from_env());
    println!("{}", report.render());
    report.save_if_requested().unwrap();
    let failures: BTreeMap<&str, &String> =
        report.checks.iter().filter_map(|check| if let Outcome::Failed(reason) = &check.outcome { Some((check.id, reason)) } else { None }).collect();
    assert!(failures.is_empty(), "failed: {failures:?}\n{}", report.render());
    for check in &report.checks {
        if let Outcome::Skipped(reason) = &check.outcome {
            assert!(!check.required, "{} is required for L6 but skipped: {reason}", check.id);
        }
    }
    assert!(report.level6(), "{}", report.render());
}

/// Native and WebAssembly engines reach one state from one change log. Set `KITION_WASM_ENGINE`
/// to a folder with `kition_board_kit.js` and its `.wasm` (a package's `engine/`, or a
/// `wasm-pack` output).
#[test]
fn wasm_parity() {
    let Some(engine) = WasmEngine::from_env("kition_board_kit", "BoardDocument", NAME) else {
        println!("SKIP wasm_parity: KITION_WASM_ENGINE is not set");
        return;
    };
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR")).join("board-convergence");
    for seed in [1, 2] {
        wasm::check_parity(&BoardKit, &engine, &scratch, seed).unwrap_or_else(|error| panic!("seed {seed}: {error}"));
    }
}

// The board's own co-editing scenarios, on the suite's replicas (`Net`): each runs on two to five
// replicas, every one edits before anyone hears of the others (offline), then all merge in seeded
// orders with duplicates (`Net::settle`), and the replicas must agree, keep the schema's
// references, and draw what a fresh replica of the merged state draws.

const REPLICAS: std::ops::RangeInclusive<usize> = 2..=5;

fn net(replicas: usize, seed: u64) -> Net<'static, BoardKit> {
    let mut net = Net::new(&BoardKit, replicas, seed).unwrap();
    net.settle().unwrap();
    net
}

/// The id of the first page's element of `kind` (back to front) at `index`.
fn nth(net: &mut Net<'_, BoardKit>, kind: Kind, index: usize) -> String {
    elements(net.doc(0).unwrap(), Some(kind)).get(index).cloned().unwrap_or_else(|| panic!("no {kind:?} {index}"))
}

fn run(net: &mut Net<'_, BoardKit>, replica: usize, command: &str, params: Value) -> Value {
    exec(net.doc(replica).unwrap(), command, params).unwrap()
}

/// `id`'s element on `replica`, if it is there.
fn element(net: &mut Net<'_, BoardKit>, replica: usize, id: &str) -> Option<kition_board_kdm::Element> {
    net.doc(replica).unwrap().model().element(&ObjectId::from_string(id)).cloned()
}

/// Every connector end bound to an element names a live element of the same page, and is drawn
/// on that element's box, on every replica.
fn bound_ends_hold(net: &mut Net<'_, BoardKit>) {
    for replica in 0..net.docs.len() {
        let model = net.doc(replica).unwrap().model();
        for page in model.pages() {
            for connector in model.elements_of(page).filter(|element| element.kind == Kind::Connector) {
                for end in [&connector.start, &connector.end].into_iter().flatten() {
                    let Some(binding) = &end.binding else { continue };
                    let target = model
                        .element(&binding.target)
                        .unwrap_or_else(|| panic!("{} is bound to a gone {}", connector.id.as_str(), binding.target.as_str()));
                    assert_eq!(model.page_of(&binding.target).map(|p| &p.id), Some(&page.id), "bound across pages");
                    let rect = model.rect(target).unwrap();
                    let at = model.end_point(&connector.id, end);
                    // An unrotated target holds its anchor point on its box.
                    if target.rotation == 0.0 {
                        assert!(
                            at.x >= rect.x0 - 1e-6 && at.x <= rect.x1 + 1e-6 && at.y >= rect.y0 - 1e-6 && at.y <= rect.y1 + 1e-6,
                            "an end off its target's box"
                        );
                    }
                }
            }
        }
    }
}

/// Every `parent` names a live frame or group of the same page, with no cycle, on every replica;
/// no group is empty.
fn membership_holds(net: &mut Net<'_, BoardKit>) {
    for replica in 0..net.docs.len() {
        let model = net.doc(replica).unwrap().model();
        for page in model.pages() {
            for element in model.elements_of(page) {
                let mut seen = vec![element.id.clone()];
                let mut at = element.parent.clone();
                while let Some(parent) = at {
                    let found = model.element(&parent).unwrap_or_else(|| panic!("{} has a gone parent", element.id.as_str()));
                    assert!(found.kind.can_contain(), "a {:?} contains {}", found.kind, element.id.as_str());
                    assert_eq!(model.page_of(&parent).map(|p| &p.id), Some(&page.id), "a parent on another page");
                    assert!(!seen.contains(&parent), "a parent cycle through {}", parent.as_str());
                    seen.push(parent.clone());
                    at = found.parent.clone();
                }
                if element.kind == Kind::Group {
                    assert!(!model.children(&element.id).is_empty(), "an empty group {}", element.id.as_str());
                }
            }
        }
    }
}

/// Merges, then checks agreement, references, views, bound ends and membership.
fn merge(net: &mut Net<'_, BoardKit>) -> Value {
    let state = net.settle().unwrap();
    bound_ends_hold(net);
    membership_holds(net);
    state
}

#[test]
fn board_concurrent_moves_of_one_shape_end_on_one_writers_move() {
    for replicas in REPLICAS {
        let mut net = net(replicas, 100 + replicas as u64);
        let shape = nth(&mut net, Kind::Shape, 0);
        let before = element(&mut net, 0, &shape).unwrap().position.unwrap();
        let deltas: Vec<[f64; 2]> = (0..replicas).map(|i| [17.0 + 31.0 * i as f64, -9.0 + 13.0 * i as f64]).collect();
        for (replica, delta) in deltas.iter().enumerate() {
            run(&mut net, replica, "element.move", json!({ "ids": [shape], "delta": delta }));
            // One of them also resizes it: a move and a resize at once both apply.
            if replica == 1 {
                let at = element(&mut net, 1, &shape).unwrap();
                let (p, size) = (at.position.unwrap(), at.size.unwrap());
                run(&mut net, 1, "element.update", json!({ "id": shape, "set": { "size": { "w": size.w + 40.0, "h": size.h + 20.0 } } }));
                assert_eq!(element(&mut net, 1, &shape).unwrap().position.unwrap(), p);
            }
        }
        merge(&mut net);
        let after = element(&mut net, 0, &shape).unwrap();
        let moved = after.position.unwrap();
        assert!(
            deltas.iter().any(|d| (moved.x - before.x - d[0]).abs() < 1e-9 && (moved.y - before.y - d[1]).abs() < 1e-9),
            "{replicas} replicas: the shape is at {moved:?}, nobody's move from {before:?}"
        );
        assert_eq!(after.size.unwrap().w, 160.0, "{replicas} replicas: the concurrent resize was lost");
    }
}

#[test]
fn board_connector_bound_to_a_shape_another_person_deletes_lets_go() {
    for replicas in REPLICAS {
        let mut net = net(replicas, 200 + replicas as u64);
        let target = nth(&mut net, Kind::Shape, 2);
        let connector = nth(&mut net, Kind::Connector, 0);
        let other = nth(&mut net, Kind::Shape, 0);
        run(&mut net, 0, "element.delete", json!({ "ids": [target] }));
        // At once: the target moved, a new connector bound to it, the existing connector's start
        // rebound to it, its end dragged to another anchor of it.
        let edits: [(&str, Value); 4] = [
            ("element.move", json!({ "ids": [target], "delta": [50, 30] })),
            ("connector.create", json!({ "from": { "element": other }, "to": { "element": target, "anchor": [0, 0.5] } })),
            ("connector.bind", json!({ "id": connector, "terminal": "end", "to": { "element": target, "anchor": [1, 1] } })),
            ("element.rotate", json!({ "ids": [target], "degrees": 30 })),
        ];
        for (replica, (command, params)) in (1..replicas).zip(edits) {
            run(&mut net, replica, command, params);
        }
        let state = merge(&mut net);
        assert!(!state::objects(&state).contains_key(&target), "{replicas} replicas: the deleted target came back");
        // The connectors stay, their ends where the target last was.
        let left = element(&mut net, 0, &connector).expect("the bound connector was deleted with its target");
        assert!(left.end.as_ref().unwrap().binding.is_none());
        assert!(left.start.as_ref().unwrap().binding.is_some(), "the start, bound to a shape nobody deleted, let go");
    }
}

#[test]
fn board_frame_membership_converges() {
    for replicas in REPLICAS {
        let mut net = net(replicas, 300 + replicas as u64);
        let (one, two) = (nth(&mut net, Kind::Frame, 0), nth(&mut net, Kind::Frame, 1));
        let shape = nth(&mut net, Kind::Shape, 0);
        // The same shape dropped into each frame at once.
        let middles = [frame_middle(net.doc(0).unwrap(), &one).unwrap(), frame_middle(net.doc(0).unwrap(), &two).unwrap()];
        for replica in 0..replicas {
            run(&mut net, replica, "element.move", json!({ "ids": [shape], "to": middles[replica % 2], "reparent": true }));
        }
        merge(&mut net);
        let dropped = element(&mut net, 0, &shape).unwrap();
        let parent = dropped.parent.as_ref().map(|id| id.as_str().to_owned());
        assert!(parent == Some(one.clone()) || parent == Some(two.clone()), "{replicas} replicas: the shape is in {parent:?}");
        // It sits in the frame it belongs to, in front of it: one person's drop, whole.
        let model = net.doc(0).unwrap().model();
        let frame = model.element(dropped.parent.as_ref().unwrap()).unwrap().clone();
        let (rect, center) = (model.rect(&frame).unwrap(), model.rect(&dropped).unwrap().center());
        assert!(
            center.x >= rect.x0 && center.x <= rect.x1 && center.y >= rect.y0 && center.y <= rect.y1,
            "{replicas} replicas: in one frame, drawn in another"
        );
        let order = elements(net.doc(0).unwrap(), None);
        let (at_shape, at_frame) = (order.iter().position(|id| *id == shape), order.iter().position(|id| id == frame.id.as_str()));
        assert!(at_shape > at_frame, "{replicas} replicas: the shape is behind its frame");

        // A frame deleted while someone drops another element into it: that element stays, out of
        // the frame; what was in the frame when it was deleted goes with it.
        let inside = dropped.id.as_str().to_owned();
        let frame_id = frame.id.as_str().to_owned();
        let other_frame = if frame_id == one { two.clone() } else { one.clone() };
        let text = nth(&mut net, Kind::Text, 0);
        let into = frame_middle(net.doc(0).unwrap(), &frame_id).unwrap();
        run(&mut net, 0, "element.delete", json!({ "ids": [frame_id] }));
        run(&mut net, 1, "element.move", json!({ "ids": [text], "to": into, "reparent": true }));
        if replicas > 2 {
            // And someone drags the frame's child out to the other frame at the same time.
            let out = frame_middle(net.doc(2).unwrap(), &other_frame).unwrap();
            run(&mut net, 2, "element.move", json!({ "ids": [inside], "to": out, "reparent": true }));
        }
        let state = merge(&mut net);
        let objects = state::objects(&state);
        assert!(!objects.contains_key(&frame_id) && !objects.contains_key(&inside), "{replicas} replicas: the frame's delete took its content");
        let kept = element(&mut net, 0, &text).expect("the element dropped into a deleted frame was lost");
        assert!(kept.parent.is_none(), "{replicas} replicas: the text is in a deleted frame");

        // Frames put inside each other at once: no cycle (rule V5).
        if replicas > 2 {
            let mut net = self::net(replicas, 350 + replicas as u64);
            net.must(0, &Script::Reparent { which: 0 }).unwrap();
            net.must(1, &Script::Reparent { which: 1 }).unwrap();
            merge(&mut net);
        }
    }
}

#[test]
fn board_groups_converge() {
    for replicas in REPLICAS {
        let mut net = net(replicas, 400 + replicas as u64);
        let group = nth(&mut net, Kind::Group, 0);
        let (a, b, c) = (nth(&mut net, Kind::Shape, 0), nth(&mut net, Kind::Shape, 1), nth(&mut net, Kind::Shape, 2));
        let text = nth(&mut net, Kind::Text, 0);
        let edits: [(&str, Value); 5] = [
            ("element.delete", json!({ "ids": [b] })),
            ("element.ungroup", json!({ "groupIds": [group] })),
            ("element.group", json!({ "ids": [a, text] })),
            ("element.delete", json!({ "ids": [c] })),
            ("element.move", json!({ "ids": [group], "delta": [10, 10] })),
        ];
        for (replica, (command, params)) in (0..replicas).zip(edits) {
            run(&mut net, replica, command, params);
        }
        merge(&mut net);
        // Both grouped elements' deletes win: a group left without children goes (rule V9).
        if replicas >= 4 {
            assert!(element(&mut net, 0, &group).is_none(), "{replicas} replicas: an emptied group stayed");
        }
    }
}

#[test]
fn board_cards_keep_one_whole_target() {
    for replicas in REPLICAS {
        let mut net = net(replicas, 500 + replicas as u64);
        let card = nth(&mut net, Kind::Card, 0);
        let targets = [
            json!({ "id": card, "file": "notes/a.md" }),
            json!({ "id": card, "url": "https://example.com/b", "title": "B" }),
            json!({ "id": card, "file": "data/c.ktable" }),
            json!({ "id": card, "file": "docs/d.pdf" }),
            json!({ "id": card, "url": "https://example.com/e" }),
        ];
        let mut written = Vec::new();
        for (replica, target) in targets.iter().take(replicas).enumerate() {
            let answer = run(&mut net, replica, "card.setTarget", target.clone());
            written.push(answer["card"].clone());
        }
        merge(&mut net);
        let value = element(&mut net, 0, &card).unwrap().card.unwrap();
        assert!(written.contains(&value), "{replicas} replicas: the card's target {value} is nobody's");

        // A card retargeted while someone deletes it: the delete wins.
        run(&mut net, 0, "element.delete", json!({ "ids": [card] }));
        run(&mut net, 1, "card.setTarget", json!({ "id": card, "file": "notes/z.md" }));
        merge(&mut net);
        assert!(element(&mut net, 0, &card).is_none());
    }
}

#[test]
fn board_z_order_keeps_every_element_once() {
    for replicas in REPLICAS {
        let mut net = net(replicas, 600 + replicas as u64);
        let before: std::collections::BTreeSet<String> = elements(net.doc(0).unwrap(), None).into_iter().collect();
        let placements = ["front", "back", "forward", "backward", "front"];
        for replica in 0..replicas {
            let all = elements(net.doc(replica).unwrap(), None);
            // Two replicas reorder the same element, the others different ones.
            let id = all[if replica < 2 { 0 } else { replica }].clone();
            run(&mut net, replica, "element.reorder", json!({ "ids": [id], "placement": placements[replica] }));
        }
        merge(&mut net);
        let order = elements(net.doc(0).unwrap(), None);
        let after: std::collections::BTreeSet<String> = order.iter().cloned().collect();
        assert_eq!(order.len(), after.len(), "{replicas} replicas: an element is listed twice");
        assert_eq!(before, after, "{replicas} replicas: the page's elements changed");
    }
}

#[test]
fn board_label_typed_by_everyone_at_once_keeps_every_word() {
    for replicas in REPLICAS {
        let mut net = net(replicas, 700 + replicas as u64);
        let shape = nth(&mut net, Kind::Shape, 0);
        run(&mut net, 0, "element.setText", json!({ "id": shape, "text": "Plan" }));
        net.settle().unwrap();
        let words: Vec<String> = (0..replicas).map(|i| format!(" w{i}{CJK_MIDDLE}")).collect();
        for (replica, word) in words.iter().enumerate() {
            run(&mut net, replica, "element.setText", json!({ "id": shape, "text": format!("Plan{word}") }));
        }
        merge(&mut net);
        let text = element(&mut net, 0, &shape).unwrap().text;
        for word in &words {
            assert!(text.contains(word.as_str()), "{replicas} replicas: {word:?} is not in {text:?}");
        }
    }
}

#[test]
fn board_replicas_offline_then_merged_agree() {
    for replicas in REPLICAS {
        for seed in [1, 2, 3] {
            let mut net = net(replicas, 800 + seed * 10 + replicas as u64);
            let mut rng = Rng::new(seed * 97 + replicas as u64);
            for replica in 0..replicas {
                for _ in 0..40 {
                    let doc = net.doc(replica).unwrap();
                    BoardKit.random_edit(doc, &mut rng).unwrap();
                    if rng.chance(8) {
                        BoardKit.undo(doc).unwrap();
                    }
                }
            }
            // Back online one by one, each hearing only from the first before everyone syncs.
            for replica in 1..replicas {
                net.deliver(replica, 0).unwrap();
                net.deliver(0, replica).unwrap();
            }
            merge(&mut net);
        }
    }
}
