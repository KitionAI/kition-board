//! The `.kboard` file (board design note sections 2.1–2.4): a ZIP whose entries are, in this order,
//!
//! | Entry | Compression | Content |
//! | --- | --- | --- |
//! | `mimetype` | STORE, first, no extra field | the ASCII bytes `application/vnd.kition.board+zip` |
//! | `meta.json` | DEFLATE | [`Meta`] |
//! | `snapshot.bin` | STORE | a KDM snapshot envelope of the document at the last compaction |
//! | `changes/<seq>.bin` | STORE | KDM change envelopes, in replay order (`000001`, `000002`, …) |
//! | `blobs/<sha256>` | STORE | one blob's bytes, named by the lower-case hex SHA-256 its KDM value carries |
//!
//! Any other entry with a safe name is kept byte for byte and written back ([`Container::extra`]).
//! This crate knows bytes only: what the snapshot and changes hold is the KDM's business
//! (`kition-board-kdm` opens them).
//!
//! The reader refuses, opening nothing and never panicking: a missing or wrong `mimetype` first
//! entry ([`ContainerError::NotABoard`]); a missing `meta.json` or `snapshot.bin`, a listed change
//! entry that is missing, an entry name with `..`, `\`, a leading `/` or a drive letter, duplicate
//! names, encrypted entries, ZIP64, more than [`MAX_ENTRIES`] entries, an entry over
//! [`MAX_ENTRY_BYTES`] or a whole archive over [`MAX_TOTAL_BYTES`] uncompressed, a `meta.json`
//! deflated more than [`MAX_META_RATIO`]:1, and a blob whose bytes do not hash to its name
//! ([`ContainerError::Damaged`]). A newer `formatVersion` is [`ContainerError::TooNew`].
//!
//! Copied from kition-layout's `.klayout` container (`crates/formats/format/src/container.rs`),
//! which implements the same layout under its own MIME. The two are meant to become one
//! parameterised container in kition-sdk.

use std::collections::{BTreeMap, HashSet};
use std::io::{Cursor, Read, Write};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The `.kboard` file's MIME (its stored `mimetype` entry).
pub const MIME: &str = "application/vnd.kition.board+zip";
/// `meta.json`'s `format`.
pub const FORMAT: &str = "kition.board";
/// The container version this build writes and reads.
pub const FORMAT_VERSION: u32 = 1;
/// More entries than this and the file is refused.
pub const MAX_ENTRIES: usize = 100_000;
/// The largest uncompressed entry.
pub const MAX_ENTRY_BYTES: u64 = 512 * 1024 * 1024;
/// The largest uncompressed archive.
pub const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// The largest deflate ratio `meta.json` may have.
pub const MAX_META_RATIO: u64 = 100;
/// A save writes a fresh snapshot instead of appending once there are more change entries than
/// this (board design note 2.3, a proposed threshold).
pub const COMPACT_ENTRIES: usize = 200;

const SNAPSHOT: &str = "snapshot.bin";
const META: &str = "meta.json";

/// Why bytes are not a readable `.kboard` file.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContainerError {
    /// Not a Kition Board file at all (no `mimetype` first entry naming the board MIME).
    #[error("not-a-board: {0}")]
    NotABoard(String),
    /// A Kition Board file that is broken or breaks a limit.
    #[error("damaged: {0}")]
    Damaged(String),
    /// Written by a newer Kition Board.
    #[error("newer-version: the board is from a newer Kition Board (format {0}); please update")]
    TooNew(u32),
}

/// `meta.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub format: String,
    pub format_version: u32,
    /// The KDM version, a copy for hosts that do not load the KDM.
    pub kdm: u32,
    /// The `kit.board` schema version, a copy.
    pub schema_version: u32,
    /// The KDM document id, a copy (so a host can index the file without the engine).
    pub doc_id: String,
    /// The board's title, a copy for file lists.
    #[serde(default)]
    pub title: String,
    pub snapshot: SnapshotRef,
    /// The change entries in replay order.
    #[serde(default)]
    pub changes: Vec<String>,
    #[serde(default)]
    pub app: App,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SnapshotRef {
    pub entry: String,
    /// Base64 of the KDM version the snapshot holds.
    #[serde(default)]
    pub version: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct App {
    pub kit: String,
    #[serde(default)]
    pub version: String,
}

/// The parts of a `.kboard` file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Container {
    pub meta: Meta,
    pub snapshot: Vec<u8>,
    /// The listed change entries' bytes, in replay order.
    pub changes: Vec<Vec<u8>>,
    /// Blob bytes by their SHA-256 (lower-case hex).
    pub blobs: BTreeMap<String, Vec<u8>>,
    /// Entries this build does not know, by name, kept byte for byte.
    pub extra: Vec<(String, Vec<u8>)>,
}

/// SHA-256 as lower-case hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The name of change entry `seq` (from 1).
pub fn change_entry(seq: usize) -> String {
    format!("changes/{seq:06}.bin")
}

/// Whether `bytes` start like a `.kboard` container: a ZIP whose first local entry is a stored
/// `mimetype` holding the board MIME (the ODF/EPUB check, without unzipping).
pub fn is_container(bytes: &[u8]) -> bool {
    first_entry_is_mimetype(bytes).is_ok()
}

fn first_entry_is_mimetype(bytes: &[u8]) -> Result<(), String> {
    let at = |range: std::ops::Range<usize>| bytes.get(range).ok_or_else(|| "too short".to_owned());
    if at(0..4)? != b"PK\x03\x04" {
        return Err("not a ZIP archive".into());
    }
    let u16_at = |i: usize| -> Result<u16, String> {
        at(i..i + 2).map(|b| u16::from_le_bytes([b.first().copied().unwrap_or(0), b.get(1).copied().unwrap_or(0)]))
    };
    let u32_at = |i: usize| -> Result<u32, String> {
        at(i..i + 4).map(|b| {
            let byte = |n: usize| b.get(n).copied().unwrap_or(0);
            u32::from_le_bytes([byte(0), byte(1), byte(2), byte(3)])
        })
    };
    if u16_at(8)? != 0 {
        return Err("the mimetype entry is compressed".into());
    }
    let (name_len, extra_len) = (usize::from(u16_at(26)?), usize::from(u16_at(28)?));
    if at(30..30 + name_len)? != b"mimetype" {
        return Err("the first entry is not mimetype".into());
    }
    if extra_len != 0 {
        return Err("the mimetype entry has an extra field".into());
    }
    let size = usize::try_from(u32_at(18)?).map_err(|_| "size".to_owned())?;
    let start = 30 + name_len;
    if at(start..start.saturating_add(size))? != MIME.as_bytes() {
        return Err("the mimetype is not a board's".into());
    }
    Ok(())
}

/// Whether `name` is a safe entry name: relative, no `..` component, no backslash, no drive.
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && !name.split('/').any(|part| part == ".." || part == ".")
        && name.as_bytes().get(1).is_none_or(|b| *b != b':')
        && !name.contains('\0')
}

fn damaged(message: impl Into<String>) -> ContainerError {
    ContainerError::Damaged(message.into())
}

/// Reads a `.kboard` file's parts. Never writes, never panics on bad input.
pub fn read(bytes: &[u8]) -> Result<Container, ContainerError> {
    first_entry_is_mimetype(bytes).map_err(ContainerError::NotABoard)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| damaged(error.to_string()))?;
    if zip.len() > MAX_ENTRIES {
        return Err(damaged(format!("{} entries (at most {MAX_ENTRIES})", zip.len())));
    }
    if zip.zip64_comment().is_some() {
        return Err(damaged("ZIP64 archives are not read"));
    }
    let mut names = HashSet::new();
    let mut total = 0u64;
    let mut entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(zip.len());
    for index in 0..zip.len() {
        let file = zip.by_index_raw(index).map_err(|error| damaged(error.to_string()))?;
        let name = file.name().to_owned();
        if !safe_name(&name) {
            return Err(damaged(format!("unsafe entry name {name:?}")));
        }
        if !names.insert(name.clone()) {
            return Err(damaged(format!("{name} appears twice")));
        }
        if file.encrypted() {
            return Err(damaged(format!("{name} is encrypted")));
        }
        let size = file.size();
        if size > MAX_ENTRY_BYTES || file.compressed_size() > MAX_ENTRY_BYTES || size > u64::from(u32::MAX) {
            return Err(damaged(format!("{name} is too large")));
        }
        total = total.saturating_add(size);
        if total > MAX_TOTAL_BYTES {
            return Err(damaged("the archive is too large"));
        }
        if name == META && file.compressed_size() > 0 && size / file.compressed_size().max(1) > MAX_META_RATIO {
            return Err(damaged("meta.json is compressed too much"));
        }
        if file.is_dir() {
            continue;
        }
        drop(file);
        let mut file = zip.by_index(index).map_err(|error| damaged(format!("{name}: {error}")))?;
        let mut data = Vec::with_capacity(usize::try_from(size).unwrap_or(0).min(1 << 24));
        // `take` guards against a header that lies about the size.
        (&mut file).take(MAX_ENTRY_BYTES + 1).read_to_end(&mut data).map_err(|error| damaged(format!("{name}: {error}")))?;
        if data.len() as u64 != size {
            return Err(damaged(format!("{name}: the size does not match its header")));
        }
        entries.push((name, data));
    }
    let mut meta_bytes = None;
    let mut snapshot = None;
    let mut by_name: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut blobs = BTreeMap::new();
    let mut extra = Vec::new();
    for (name, data) in entries {
        if name == "mimetype" {
            continue;
        }
        if name == META {
            meta_bytes = Some(data);
        } else if name == SNAPSHOT {
            snapshot = Some(data);
        } else if let Some(hash) = name.strip_prefix("blobs/") {
            if sha256_hex(&data) != hash {
                return Err(damaged(format!("{name}: the bytes do not match their hash")));
            }
            blobs.insert(hash.to_owned(), data);
        } else if name.starts_with("changes/") {
            by_name.insert(name, data);
        } else {
            extra.push((name, data));
        }
    }
    let meta_bytes = meta_bytes.ok_or_else(|| damaged("no meta.json"))?;
    let raw: serde_json::Value = serde_json::from_slice(&meta_bytes).map_err(|error| damaged(format!("meta.json: {error}")))?;
    if raw.get("format").and_then(serde_json::Value::as_str) != Some(FORMAT) {
        return Err(ContainerError::NotABoard("meta.json does not name kition.board".into()));
    }
    if let Some(version) = raw.get("formatVersion").and_then(serde_json::Value::as_u64)
        && version > u64::from(FORMAT_VERSION)
    {
        return Err(ContainerError::TooNew(u32::try_from(version).unwrap_or(u32::MAX)));
    }
    let meta: Meta = serde_json::from_value(raw).map_err(|error| damaged(format!("meta.json: {error}")))?;
    if meta.snapshot.entry != SNAPSHOT {
        return Err(damaged(format!("the snapshot entry is {}, not {SNAPSHOT}", meta.snapshot.entry)));
    }
    let snapshot = snapshot.ok_or_else(|| damaged("no snapshot.bin"))?;
    let mut changes = Vec::with_capacity(meta.changes.len());
    for name in &meta.changes {
        // An entry on disk but not listed is ignored (and dropped at the next save); a listed one
        // that is missing makes the file damaged.
        changes.push(by_name.remove(name).ok_or_else(|| damaged(format!("{name} is listed but missing")))?);
    }
    Ok(Container { meta, snapshot, changes, blobs, extra })
}

/// Writes a `.kboard` file: the entries in the order of the crate docs. `meta.changes`,
/// `meta.format`, `meta.formatVersion` and `meta.snapshot.entry` are set here.
pub fn write(container: &Container) -> Result<Vec<u8>, ContainerError> {
    let io = |error: std::io::Error| damaged(error.to_string());
    let zip_io = |error: zip::result::ZipError| damaged(error.to_string());
    let mut meta = container.meta.clone();
    meta.format = FORMAT.to_owned();
    meta.format_version = FORMAT_VERSION;
    meta.snapshot.entry = SNAPSHOT.to_owned();
    meta.changes = (1..=container.changes.len()).map(change_entry).collect();
    let mut out = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut out);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflate = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("mimetype", stored).map_err(zip_io)?;
        zip.write_all(MIME.as_bytes()).map_err(io)?;
        zip.start_file(META, deflate).map_err(zip_io)?;
        let json = serde_json::to_vec_pretty(&meta).map_err(|error| damaged(error.to_string()))?;
        zip.write_all(&json).map_err(io)?;
        zip.start_file(SNAPSHOT, stored).map_err(zip_io)?;
        zip.write_all(&container.snapshot).map_err(io)?;
        for (name, bytes) in meta.changes.iter().zip(&container.changes) {
            zip.start_file(name.as_str(), stored).map_err(zip_io)?;
            zip.write_all(bytes).map_err(io)?;
        }
        for (hash, bytes) in &container.blobs {
            zip.start_file(format!("blobs/{hash}"), stored).map_err(zip_io)?;
            zip.write_all(bytes).map_err(io)?;
        }
        let known = |name: &str| name == "mimetype" || name == META || name == SNAPSHOT || name.starts_with("changes/") || name.starts_with("blobs/");
        for (name, bytes) in &container.extra {
            if !safe_name(name) || known(name) {
                continue;
            }
            zip.start_file(name.as_str(), deflate).map_err(zip_io)?;
            zip.write_all(bytes).map_err(io)?;
        }
        zip.finish().map_err(zip_io)?;
    }
    Ok(out.into_inner())
}

/// Whether a save should write a fresh snapshot instead of appending: the change entries together
/// exceed a quarter of the snapshot, or there are more than [`COMPACT_ENTRIES`] of them (the
/// board's proposed thresholds, design note 2.3).
pub fn wants_compaction(snapshot_len: usize, changes: &[Vec<u8>]) -> bool {
    let total: usize = changes.iter().map(Vec::len).sum();
    changes.len() > COMPACT_ENTRIES || total.saturating_mul(4) > snapshot_len
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding (`meta.json`'s `snapshot.version`).
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let byte = |n: usize| chunk.get(n).copied().unwrap_or(0);
        let n = u32::from(byte(0)) << 16 | u32::from(byte(1)) << 8 | u32::from(byte(2));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET.get(((n >> (18 - 6 * i)) & 63) as usize).copied().unwrap_or(b'A')));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decodes [`base64_encode`]'s text; characters outside the alphabet are skipped.
pub fn base64_decode(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut buf, mut bits) = (0u32, 0);
    for byte in text.bytes() {
        let Some(v) = ALPHABET.iter().position(|a| *a == byte) else { continue };
        buf = buf << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    out
}

#[cfg(test)]
mod tests;
