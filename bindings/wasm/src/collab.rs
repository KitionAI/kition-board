//! The members a host uses to keep a board's replica and its blobs (engine contract 2): open a
//! board from a replica the host stored, the replica's id, the blob exchange (`blobs`,
//! `blobsSince`, `addBlobs`; pictures are blobs, design note 1.2), and the object model served over
//! the change exchange (`ReplicaPort`) for `kition-board`.

use std::collections::BTreeSet;
use std::sync::Arc;

use kition_board_kdm::blobs::{SharedBlobs, decode_bundle, encode_bundle};
use kition_board_kdm::file;
use kition_board_kdm::{BoardModel, schema};
use kition_kit::kdm::{ActorId, Origin, Replica, Schema, Version};
use kition_kit::session::{BlobSource, Damage, KitSessionDocument};
use kition_kit::{KitCommand, KitDocument};
use serde_json::Value;

use crate::{BoardDocument, check_name};

/// Which blobs arrived with which version, so `blobsSince` answers what a peer at a version lacks.
#[derive(Clone, Debug, Default)]
pub struct BlobLog(Vec<(Version, Vec<String>)>);

impl BlobLog {
    /// Records `hashes` as new at `version`.
    pub fn record(&mut self, version: Version, hashes: Vec<String>) {
        if !hashes.is_empty() {
            self.0.push((version, hashes));
        }
    }

    /// Every hash logged.
    pub fn known(&self) -> BTreeSet<String> {
        self.0.iter().flat_map(|(_, hashes)| hashes.iter().cloned()).collect()
    }

    /// The hashes logged at versions `since` does not include.
    fn after(&self, since: &Version) -> BTreeSet<String> {
        self.0.iter().filter(|(version, _)| !since.includes(version)).flat_map(|(_, hashes)| hashes.iter().cloned()).collect()
    }
}

impl BoardDocument {
    /// The replica's document id (stable across saves; never the file's path).
    pub fn doc_id(&self) -> String {
        self.session.replica().doc_id().to_string()
    }

    /// Opens a board with the replica the host stored for it (`snapshot`: its state and version,
    /// the head of the host's history) and that replica's blobs (a blob bundle). Empty `bytes` open
    /// the replica alone. When both are given and are the same document, the file (which holds the
    /// content and no history) is taken into the stored replica (`kition_kit::session::take_file`):
    /// nothing when both hold the same state, otherwise (the file was saved elsewhere, or changed
    /// outside) the file's state written over the replica's as one `system` transaction labelled
    /// `import`, keyed by object id. Either way the board goes on on the stored replica's line of
    /// changes, with its object ids. A file of another document, or a replica that does not open,
    /// leaves the file as [`KitDocument::open`] opens it.
    pub fn open_with_replica(name: &str, bytes: &[u8], snapshot: &[u8], blobs: &[u8]) -> Result<Self, String> {
        check_name(name)?;
        let store = SharedBlobs::default();
        for (hash, bytes) in decode_bundle(blobs)? {
            store.insert(&hash, &bytes)?;
        }
        let stored = Replica::load(schema(), snapshot, ActorId::random());
        if bytes.is_empty() {
            let mut replica = stored.map_err(|error| format!("the stored replica does not open: {error}"))?;
            let detached = file::dangling_ends(&replica)?;
            file::repair_once(&mut replica)?;
            replica.take_changed();
            let mut document = Self::attach(replica, store, None)?;
            document.detached = detached;
            return Ok(document);
        }
        let opened = file::open_file(bytes, ActorId::random())?;
        for (hash, bytes) in &opened.blobs {
            store.insert(hash, bytes)?;
        }
        match stored {
            Ok(mut replica) if replica.doc_id() == opened.replica.doc_id() => {
                kition_kit::session::take_file(&mut replica, &opened.replica).map_err(|error| format!("damaged: {error}"))?;
                let detached = file::dangling_ends(&replica)?;
                file::repair_once(&mut replica)?;
                replica.take_changed();
                let mut document = Self::attach(replica, store, Some(opened.base))?;
                document.detached = detached;
                Ok(document)
            }
            _ => {
                let mut document = Self::attach(opened.replica, store, Some(opened.base))?;
                document.detached = opened.detached;
                Ok(document)
            }
        }
    }

    /// Every blob the board references that the store holds, as a blob bundle.
    pub fn blobs_bundle(&self) -> Vec<u8> {
        let hashes = self.session.model().image_hashes();
        encode_bundle(&self.blob_pairs(hashes))
    }

    /// The blobs a peer at `since` (an encoded version; empty for none) needs to reach this
    /// document's state, as a blob bundle: those that arrived after `since`, and with an empty
    /// `since` every blob the board references.
    pub fn blobs_since(&self, since: &[u8]) -> Result<Vec<u8>, String> {
        if since.is_empty() {
            return Ok(self.blobs_bundle());
        }
        let since = Version::decode(since).map_err(|error| error.to_string())?;
        let mut hashes = self.blob_log.after(&since);
        // Blobs the store took outside a logged step (a bridge writing through the session) count
        // as new: a peer that has them already ignores them.
        let known = self.blob_log.known();
        hashes.extend(self.session.model().image_hashes().into_iter().filter(|hash| !known.contains(hash)));
        Ok(encode_bundle(&self.blob_pairs(hashes)))
    }

    fn blob_pairs(&self, hashes: BTreeSet<String>) -> Vec<(String, Arc<[u8]>)> {
        hashes.into_iter().filter_map(|hash| self.blobs.get(&hash).map(|bytes| (hash, bytes))).collect()
    }

    /// Takes a peer's blobs (before or after their changes). Answers whether the view changed:
    /// blobs the board already references that were missing (they came late) redraw the pictures
    /// that show them.
    pub fn add_blobs(&mut self, bundle: &[u8]) -> Result<bool, String> {
        let wanted = self.session.model().image_hashes();
        let mut added = Vec::new();
        let mut late = false;
        for (hash, bytes) in decode_bundle(bundle)? {
            if self.blobs.insert(&hash, &bytes)? {
                late |= wanted.contains(&hash);
                added.push(hash);
            }
        }
        self.blob_log.record(self.session.version(), added);
        if late {
            self.changed(&Damage::Full);
        }
        Ok(late)
    }

    /// The store holding `hash`.
    pub fn has_blob(&self, hash: &str) -> bool {
        self.blobs.contains(hash)
    }
}

/// The object model (`KitObjectModel 1.0`) headless, over the document's change exchange, and
/// `model.open`, `model.import` and `model.export` with the pictures the changes reference.
impl kition_kit::session::model::ReplicaPort for BoardDocument {
    fn model_schema(&self) -> Arc<Schema> {
        <BoardModel as kition_kit::session::KitModel>::schema()
    }

    fn model_export(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        kition_kit::session::__export_changes(self, since)
    }

    fn model_import(&mut self, bytes: &[u8]) -> Result<(), String> {
        KitSessionDocument::import_changes(self, bytes).map(|_| ())
    }

    fn model_command(&mut self, command: &str, params: Value, origin: &Origin) -> Result<Value, String> {
        self.execute_as(command, params, origin)
    }

    fn model_commands(&self) -> Vec<KitCommand> {
        self.commands()
    }

    fn model_add_blobs(&mut self, blobs: &[u8]) -> Result<(), String> {
        self.add_blobs(blobs).map(|_| ())
    }

    fn model_blobs_since(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        self.blobs_since(since)
    }

    /// A shared board with no file yet: the replica and its blobs alone, failing rather than
    /// falling back to an empty board.
    fn model_open_replica(name: &str, snapshot: &[u8], blobs: &[u8]) -> Result<Self, String> {
        Self::open_with_replica(name, &[], snapshot, blobs)
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use kition_kit::wasm_bindgen::prelude::*;

    use crate::__kition_kit_export::WebDocument;
    use crate::BoardDocument;

    fn error(message: String) -> JsError {
        JsError::new(&message)
    }

    #[wasm_bindgen(js_class = "BoardDocument", wasm_bindgen = kition_kit::wasm_bindgen)]
    impl WebDocument {
        #[wasm_bindgen(js_name = openWithReplica)]
        pub fn open_with_replica(name: &str, bytes: &[u8], snapshot: &[u8], blobs: &[u8]) -> Result<WebDocument, JsError> {
            BoardDocument::open_with_replica(name, bytes, snapshot, blobs).map(WebDocument).map_err(error)
        }

        #[wasm_bindgen(getter, js_name = hasReplica)]
        pub fn has_replica(&self) -> bool {
            true
        }

        #[wasm_bindgen(getter, js_name = docId)]
        pub fn doc_id(&self) -> String {
            self.0.doc_id()
        }

        pub fn blobs(&self) -> Vec<u8> {
            self.0.blobs_bundle()
        }

        #[wasm_bindgen(js_name = blobsSince)]
        pub fn blobs_since(&self, since: &[u8]) -> Result<Vec<u8>, JsError> {
            self.0.blobs_since(since).map_err(error)
        }

        #[wasm_bindgen(js_name = addBlobs)]
        pub fn add_blobs(&mut self, bytes: &[u8]) -> Result<bool, JsError> {
            self.0.add_blobs(bytes).map_err(error)
        }
    }
}
