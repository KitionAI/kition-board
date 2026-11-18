//! The Board kit's document on the Kit Data Model: schema `kit.board` v2 (`model/schema.json`,
//! `docs/design.md` section 1).
//!
//! A board is one `Board` object (fixed id [`root_id`]) with an ordered list of `Page`s, each with
//! an ordered list of `Element`s (back to front). Coupled values live in one `json` register each
//! (`position`, `size`, `start`, `end`, `points`, `waypoints`, `card`); pictures are `blob`s.
//!
//! - [`view`]: the materialised view ([`BoardModel`]), with bound connector ends derived from their
//!   targets (design note 1.4).
//! - [`validate`]: rules V1–V9, the deterministic repair after open, import, undo and redo (1.5).
//! - [`blobs`]: the blob store and the blob bundle hosts exchange.
//! - [`file`]: the `.kboard` file as a replica: open, and save by appending to the change log (2.3).
//!
//! Ported from Kition's `packages/board-editor/src/kdm` and `packages/board-kit/src/engine`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

use kition_kit::kdm::{ActorId, DocId, ObjectId, Origin, Replica, Schema, Tx};
use kition_kit::session::{BlobSink, BlobSource, Damage, KitModel, Result};
use serde_json::{Map, json};

pub mod blobs;
pub mod element;
pub mod file;
pub mod geometry;
pub mod mindmap;
pub mod validate;
pub mod view;

pub use element::{Binding, Element, End, Kind};
pub use geometry::{Point, Rect, Size};
pub use view::{BoardModel, DanglingEnd, PageView};

/// The kit id the schema names.
pub const KIT: &str = "kit.board";
/// The type names of the schema.
pub const BOARD: &str = "Board";
pub const PAGE: &str = "Page";
pub const ELEMENT: &str = "Element";
/// The name of a new board's first page.
pub const FIRST_PAGE: &str = "Page 1";

const SCHEMA: &str = include_str!("../model/schema.json");

/// The `Board` object's id: one per document, written by the replica that creates it, so every
/// reader finds it without an index.
pub fn root_id() -> ObjectId {
    ObjectId::from_string("board")
}

/// The kit's schema.
pub fn schema() -> Arc<Schema> {
    static PARSED: OnceLock<Arc<Schema>> = OnceLock::new();
    PARSED
        .get_or_init(|| {
            #[allow(clippy::expect_used)]
            Arc::new(Schema::parse(SCHEMA).expect("model/schema.json is a fixed, tested file"))
        })
        .clone()
}

/// The schema file's text, as the package ships it.
pub fn schema_json() -> &'static str {
    SCHEMA
}

/// A new board titled `title` with one empty page, written in the creating commit (not an undo
/// step). `size` is the document's size in board units from the origin (the area renders and
/// thumbnails show, [`BoardModel::size`]); none (or an empty or non-finite size) leaves the board
/// sized by its content.
pub fn create_replica(title: &str, size: Option<Size>, actor: ActorId, origin: &Origin) -> Result<Replica> {
    let size = size.filter(|size| size.w.is_finite() && size.h.is_finite() && size.w > 0.0 && size.h > 0.0);
    let (replica, ()) = Replica::create_with(schema(), DocId::new(), actor, origin, |tx| {
        let mut fields = Map::new();
        fields.insert("title".into(), json!(title));
        if let Some(size) = size {
            fields.insert("size".into(), size.to_json());
        }
        tx.create_with_id(BOARD, &root_id(), fields)?;
        let mut page = Map::new();
        page.insert("name".into(), json!(FIRST_PAGE));
        let page = tx.create(PAGE, page)?;
        tx.list_insert(&root_id(), "pages", 0, &page)?;
        Ok(())
    })?;
    Ok(replica)
}

/// The bytes of the blobs the view's images reference that `source` holds.
pub fn referenced_blobs(model: &BoardModel, source: &dyn BlobSource) -> BTreeMap<String, Vec<u8>> {
    model.image_hashes().into_iter().filter_map(|hash| source.get(&hash).map(|bytes| (hash, bytes.to_vec()))).collect()
}

impl KitModel for BoardModel {
    fn schema() -> Arc<Schema> {
        schema()
    }

    fn load(replica: &Replica, _blobs: &dyn BlobSource) -> Result<Self> {
        Self::read(replica)
    }

    fn apply(&mut self, replica: &Replica, changed: &BTreeSet<ObjectId>, _blobs: &dyn BlobSource) -> Result<Damage> {
        self.refresh(replica, changed)?;
        Ok(Damage::Full)
    }

    fn validate(tx: &mut Tx<'_>, _changed: &BTreeSet<ObjectId>) -> Result<bool> {
        validate::repair(tx)
    }

    /// Opens a `.kboard` file: the replica (repaired once) and its blobs into `blobs`.
    fn import(_name: &str, bytes: &[u8], blobs: &mut dyn BlobSink, actor: ActorId) -> Result<Replica> {
        let opened = file::open_file(bytes, actor)?;
        for bytes in opened.blobs.values() {
            blobs.put(bytes, "application/octet-stream");
        }
        Ok(opened.replica)
    }

    /// The board as a fresh `.kboard` file (a snapshot, no change entries).
    fn export(&self, replica: &Replica, _name: &str, blobs: &dyn BlobSource) -> Result<Vec<u8>> {
        file::save_file(replica, self.title(), referenced_blobs(self, blobs), None, env!("CARGO_PKG_VERSION")).map(|(bytes, _)| bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_file_parses_and_names_the_kit() {
        let schema = schema();
        assert_eq!(schema.kit, KIT);
        assert_eq!(schema.schema_version, 2);
        assert_eq!(schema.root, BOARD);
        assert_eq!(schema_json(), SCHEMA);
    }

    /// Rule K12: the manifest names this schema, its kit and its version.
    #[test]
    fn the_manifest_names_this_schema() {
        let manifest: serde_json::Value = serde_json::from_str(include_str!("../../../../kit.json")).unwrap();
        assert_eq!(manifest["id"], KIT);
        assert_eq!(manifest["model"]["schemaVersion"], schema().schema_version);
        assert_eq!(manifest["model"]["schema"], "crates/kdm/kdm-model/model/schema.json");
        assert_eq!(manifest["formats"][0]["mime"], kition_board_kboard::MIME);
    }

    #[test]
    fn a_new_board_has_one_empty_page() {
        let replica = create_replica("Plan", None, ActorId::random(), &Origin::system("create")).unwrap();
        let model = BoardModel::read(&replica).unwrap();
        assert_eq!(model.title(), "Plan");
        assert_eq!(model.pages().len(), 1);
        assert_eq!(model.pages()[0].name, FIRST_PAGE);
        assert_eq!(model.element_count(), 0);
    }
}
