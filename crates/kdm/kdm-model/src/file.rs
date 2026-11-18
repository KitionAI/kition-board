//! The `.kboard` file as a replica (design note 2.3): [`open_file`] reads the container
//! (`kition-board-kboard`), loads the snapshot, imports the listed changes in order, collects the
//! blobs and repairs once; it never writes. [`save_file`] re-emits the snapshot and the change
//! entries it read or wrote byte for byte, appends one entry with the changes since, and writes the
//! blobs the state references, or writes a fresh snapshot and no entries once the log has grown
//! past the compaction thresholds (`kition_board_kboard::wants_compaction`). Nothing a save writes
//! is re-encoded.
//!
//! Follows kition-layout's `crates/kdm/kdm-model/src/file.rs`, the same scheme for `.klayout`.

use std::collections::BTreeMap;
use std::sync::Arc;

use kition_board_kboard::{self as kboard, App, Container, Meta, SnapshotRef};
use kition_kit::kdm::{ActorId, KDM_VERSION, ObjectId, Origin, Replica, Version};
use kition_kit::session::{KitError, Result};

use crate::{BoardModel, DanglingEnd, KIT, schema};

/// What a document's file holds, so the next save re-emits it and appends what changed since.
#[derive(Clone, Debug)]
pub struct FileBase {
    snapshot: Arc<Vec<u8>>,
    snapshot_version: Version,
    changes: Vec<Arc<Vec<u8>>>,
    /// The replica version the file's snapshot and changes add up to.
    version: Version,
    doc_id: String,
    /// Entries this build does not know, written back as they were.
    extra: Vec<(String, Vec<u8>)>,
}

impl FileBase {
    /// The replica version the file holds.
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// How many change entries the file has after its snapshot.
    pub fn change_entries(&self) -> usize {
        self.changes.len()
    }

    /// The bytes of the file's snapshot entry.
    pub fn snapshot(&self) -> &[u8] {
        &self.snapshot
    }
}

/// A `.kboard` file, opened.
pub struct OpenedFile {
    /// The replica, repaired once when the file needed it.
    pub replica: Replica,
    /// What the file holds.
    pub base: FileBase,
    /// The blobs the file carries, by hash.
    pub blobs: BTreeMap<String, Vec<u8>>,
    /// The connector ends the file had bound to nothing they can be bound to, which the repair
    /// detached (rule V6), by page: `board_check` reports them while they stay detached.
    pub detached: Vec<(ObjectId, DanglingEnd)>,
}

/// The connector ends of `replica` that rule V6 would detach, with their pages.
pub fn dangling_ends(replica: &Replica) -> Result<Vec<(ObjectId, DanglingEnd)>> {
    let model = BoardModel::read(replica)?;
    Ok(model.pages().iter().flat_map(|page| model.dangling_ends(page).into_iter().map(|end| (page.id.clone(), end))).collect())
}

/// The `docId` a `.kboard` file's `meta.json` names (M7), without loading its replica.
pub fn doc_id_of(bytes: &[u8]) -> Option<String> {
    kboard::read(bytes).ok().map(|parts| parts.meta.doc_id).filter(|id| !id.is_empty())
}

fn damaged(error: impl std::fmt::Display) -> KitError {
    KitError::new(format!("damaged: {error}"))
}

/// Opens a `.kboard` file as a replica of `actor`. Answers `not-a-board: …`, `damaged: …` or
/// `newer-version: …` for files it does not open.
pub fn open_file(bytes: &[u8], actor: ActorId) -> Result<OpenedFile> {
    let parts = kboard::read(bytes).map_err(|error| KitError::new(error.to_string()))?;
    let mut replica = Replica::load(schema(), &parts.snapshot, actor).map_err(|error| match error {
        kition_kit::kdm::KdmError::NewerSchema { .. } => KitError::new(format!("newer-version: {error}")),
        kition_kit::kdm::KdmError::Schema(_) => KitError::new(format!("not-a-board: {error}")),
        other => damaged(other),
    })?;
    for change in &parts.changes {
        replica.import_changes(change).map_err(damaged)?;
    }
    let version = replica.version();
    let doc_id = replica.doc_id().to_string();
    if !parts.meta.doc_id.is_empty() && parts.meta.doc_id != doc_id {
        return Err(damaged(format!("meta.json names document {}, the snapshot {doc_id}", parts.meta.doc_id)));
    }
    let snapshot_version = Version::decode(&kboard::base64_decode(&parts.meta.snapshot.version)).unwrap_or_else(|_| Version::empty());
    let detached = dangling_ends(&replica)?;
    repair_once(&mut replica)?;
    replica.take_changed();
    let base = FileBase {
        snapshot: Arc::new(parts.snapshot),
        snapshot_version,
        changes: parts.changes.into_iter().map(Arc::new).collect(),
        version,
        doc_id,
        extra: parts.extra,
    };
    Ok(OpenedFile { replica, base, blobs: parts.blobs, detached })
}

/// Runs the repair (V1–V9) in a `system` transaction labelled `repair`, written only when there is
/// something to repair. Answers whether it wrote.
pub fn repair_once(replica: &mut Replica) -> Result<bool> {
    let mut failed = None;
    let result = replica.transact(&Origin::system("repair"), |tx| {
        crate::validate::repair(tx).map_err(|error| {
            let message = error.to_string();
            failed = Some(error);
            kition_kit::kdm::KdmError::Engine(message)
        })
    });
    match (result, failed) {
        (_, Some(error)) => Err(error),
        (result, None) => Ok(result?),
    }
}

/// The file `replica` saves to, on top of `base` (what it opened from or last saved, when it is the
/// same document), with `blobs` (what the state references): the bytes, and what the file then
/// holds. A save that has nothing new appends nothing.
pub fn save_file(
    replica: &Replica,
    title: &str,
    blobs: BTreeMap<String, Vec<u8>>,
    base: Option<&FileBase>,
    app_version: &str,
) -> Result<(Vec<u8>, FileBase)> {
    let doc_id = replica.doc_id().to_string();
    let version = replica.version();
    let base = base.filter(|base| base.doc_id == doc_id);
    let (mut snapshot, mut snapshot_version, mut changes, extra) = match base {
        Some(base) => (base.snapshot.clone(), base.snapshot_version.clone(), base.changes.clone(), base.extra.clone()),
        None => (Arc::new(replica.snapshot()?), version.clone(), Vec::new(), Vec::new()),
    };
    if let Some(base) = base
        && base.version != version
    {
        changes.push(Arc::new(replica.export_changes(&base.version)?));
    }
    let plain: Vec<Vec<u8>> = changes.iter().map(|change| (**change).clone()).collect();
    if base.is_some() && kboard::wants_compaction(snapshot.len(), &plain) {
        snapshot = Arc::new(replica.snapshot()?);
        snapshot_version = version.clone();
        changes = Vec::new();
    }
    let parts = Container {
        meta: Meta {
            kdm: KDM_VERSION,
            schema_version: replica.schema().schema_version,
            doc_id: doc_id.clone(),
            title: title.to_owned(),
            snapshot: SnapshotRef { entry: String::new(), version: kboard::base64_encode(&snapshot_version.encode()) },
            app: App { kit: KIT.to_owned(), version: app_version.to_owned() },
            ..Default::default()
        },
        snapshot: (*snapshot).clone(),
        changes: changes.iter().map(|change| (**change).clone()).collect(),
        blobs,
        extra: extra.clone(),
    };
    let bytes = kboard::write(&parts).map_err(|error| KitError::new(error.to_string()))?;
    Ok((bytes, FileBase { snapshot, snapshot_version, changes, version, doc_id, extra }))
}
