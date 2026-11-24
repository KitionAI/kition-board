//! The `.kboard` file as a replica (design note 2.3): the file holds the document's **content**, not
//! its history (kition-sdk data model 7.3, rule M16b). [`save_file`] writes the replica's content
//! snapshot (its state, and of its history only a `system` seal) and the blobs the state
//! references; [`open_file`] loads it as the same document (same docId, same object ids) and
//! repairs once; it never writes. The history lives in the host's history store (`snapshot()` of
//! the live replica), never in the file.
//!
//! A file opens on top of the replica the host stored for its document through kition-sdk's
//! `kition_kit::session::take_file` (data model 7.3).
//!
//! Follows kition-layout's `crates/kdm/kdm-model/src/file.rs`, the same scheme for `.klayout`.

use std::collections::BTreeMap;

use kition_board_kboard::{self as kboard, Container, Meta};
use kition_kit::kdm::{ActorId, KDM_VERSION, KdmError, ObjectId, Origin, Replica, Tx, Version};
use kition_kit::session::{KitError, Result};

use crate::{BoardModel, DanglingEnd, KIT, schema};

/// What a document's file holds besides its content.
#[derive(Clone, Debug)]
pub struct FileBase {
    /// The replica version the file's content holds (its seal included).
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

/// A save: the file's bytes and what the file then holds.
pub struct SavedFile {
    pub bytes: Vec<u8>,
    pub base: FileBase,
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

/// The container's parts with the replica its content loads as, checked against `meta.json`.
fn read(bytes: &[u8], actor: ActorId) -> Result<(Container, Replica)> {
    let parts = kboard::read(bytes).map_err(|error| KitError::new(error.to_string()))?;
    let replica = load(&parts.content, actor)?;
    let doc_id = replica.doc_id().to_string();
    if parts.meta.doc_id != doc_id {
        return Err(damaged(format!("meta.json names document {}, the content {doc_id}", parts.meta.doc_id)));
    }
    Ok((parts, replica))
}

/// Opens a `.kboard` file as a replica of `actor`: the document the file holds, with its docId and
/// object ids, and of its history only the seal. Answers `not-a-board: …`, `damaged: …` or
/// `newer-version: …` for files it does not open.
pub fn open_file(bytes: &[u8], actor: ActorId) -> Result<OpenedFile> {
    let (parts, mut replica) = read(bytes, actor)?;
    let version = replica.version();
    let doc_id = replica.doc_id().to_string();
    let detached = dangling_ends(&replica)?;
    repair_once(&mut replica)?;
    replica.take_changed();
    let base = FileBase { version, doc_id, extra: parts.extra };
    Ok(OpenedFile { replica, base, blobs: parts.blobs, detached })
}

/// The replica a `.kboard` file embeds (`document.identity`, KitModel M7a): its content snapshot,
/// or `None` for an empty file (one made for the kit before it saved a board there). Answers
/// `not-a-board: …`, `damaged: …` or `newer-version: …` for other bytes, as [`open_file`] does;
/// never repairs, never writes.
pub fn embedded_replica(bytes: &[u8]) -> Result<Option<Vec<u8>>> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let (parts, _) = read(bytes, ActorId::random())?;
    Ok(Some(parts.content))
}

/// `bytes` (a `.kboard` file) holding the document `snapshot` instead of its own (`document.fork`,
/// KitModel M7a): `content.bin` is the snapshot's content (a copy's birth is its seal, so a copy
/// adds no change), `meta.json`'s docId the snapshot's, and the blobs the ones its state
/// references, so no earlier identity stays in the file. Entries this build does not know are kept.
pub fn replace_replica(bytes: &[u8], snapshot: &[u8]) -> Result<Vec<u8>> {
    let mut parts = kboard::read(bytes).map_err(|error| KitError::new(error.to_string()))?;
    let mut replica = load(snapshot, ActorId::random())?;
    let referenced = BoardModel::read(&replica)?.image_hashes();
    parts.blobs.retain(|hash, _| referenced.contains(hash));
    parts.meta.doc_id = replica.doc_id().to_string();
    parts.content = replica.content_snapshot()?;
    kboard::write(&parts).map_err(|error| KitError::new(error.to_string()))
}

/// A board snapshot loaded as a replica of `actor`: `newer-version: …`, `not-a-board: …` or
/// `damaged: …` when it does not load.
fn load(snapshot: &[u8], actor: ActorId) -> Result<Replica> {
    Replica::load(schema(), snapshot, actor).map_err(|error| match error {
        KdmError::NewerSchema { .. } => KitError::new(format!("newer-version: {error}")),
        KdmError::Schema(_) => KitError::new(format!("not-a-board: {error}")),
        other => damaged(other),
    })
}

/// Runs `write` in one transaction of `replica` with `origin`, keeping the kit's error.
fn transact(replica: &mut Replica, origin: &Origin, write: impl FnOnce(&mut Tx<'_>) -> Result<bool>) -> Result<bool> {
    let mut failed = None;
    let result = replica.transact(origin, |tx| {
        write(tx).map_err(|error| {
            let message = error.to_string();
            failed = Some(error);
            KdmError::Engine(message)
        })
    });
    match (result, failed) {
        (_, Some(error)) => Err(error),
        (result, None) => Ok(result?),
    }
}

/// Runs the repair (V1–V9) in a `system` transaction labelled `repair`, written only when there is
/// something to repair. Answers whether it wrote.
pub fn repair_once(replica: &mut Replica) -> Result<bool> {
    transact(replica, &Origin::system("repair"), crate::validate::repair)
}

/// The file `replica` saves to, with `blobs` (what the state references), and `base` (what its file
/// holds, when it is the same document, for the entries this build does not know): the bytes and
/// what the file then holds. The replica must be sealed first (`KitDocument::prepare_save`,
/// `KitSession::seal`, which every host calls before a save): its own content snapshot is written,
/// the same bytes for the same version. An unsealed replica answers `not-sealed: …`.
pub fn save_file(replica: &Replica, blobs: BTreeMap<String, Vec<u8>>, base: Option<&FileBase>) -> Result<SavedFile> {
    let doc_id = replica.doc_id().to_string();
    let base = base.filter(|base| base.doc_id == doc_id);
    if !replica.is_sealed() {
        return Err(KitError::new("not-sealed: the replica is saved after its seal (prepareSave)"));
    }
    let version = replica.version();
    let content = replica.sealed_content_snapshot()?;
    let extra = base.map(|base| base.extra.clone()).unwrap_or_default();
    let parts = Container {
        meta: Meta {
            kit: KIT.to_owned(),
            kdm: KDM_VERSION,
            schema_version: replica.schema().schema_version,
            doc_id: doc_id.clone(),
            ..Default::default()
        },
        content,
        blobs,
        extra: extra.clone(),
    };
    let bytes = kboard::write(&parts).map_err(|error| KitError::new(error.to_string()))?;
    Ok(SavedFile { bytes, base: FileBase { version, doc_id, extra } })
}
