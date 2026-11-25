//! Every committed `.kboard` sample (`conformance/`, `cases/`, `web/public/sample/`) loads and saves
//! to its own bytes (kition-sdk data model 7.4; feature `board.file.content-only`): its content is
//! the canonical `content.json` the kit writes, so the samples are the kit's own files. Loaded
//! without the repair on open, so a sample of a broken board (the `check-*` fixtures) stays broken.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::{Path, PathBuf};

use kition_board_kboard as kboard;
use kition_board_kdm::file::save_file;
use kition_board_kdm::schema;
use kition_kit::kdm::{ActorId, Replica};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// The committed samples, by their path from the repository root, sorted.
fn samples() -> Vec<String> {
    let root = repo();
    let mut found = Vec::new();
    let mut stack: Vec<PathBuf> = ["conformance", "cases", "web/public/sample"].iter().map(|dir| root.join(dir)).collect();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "kboard") {
                found.push(path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

/// The file the kit saves for `content` (a `content.json`), with `blobs`.
fn kit_save(content: &[u8], blobs: std::collections::BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let replica = Replica::load(schema(), content, ActorId::random()).unwrap();
    save_file(&replica, blobs, None).unwrap().bytes
}

#[test]
fn every_committed_sample_loads_and_saves_to_its_own_bytes() {
    let samples = samples();
    assert!(samples.len() >= 19, "the samples: {samples:?}");
    for sample in samples {
        let bytes = std::fs::read(repo().join(&sample)).unwrap();
        let parts = kboard::read(&bytes).unwrap_or_else(|error| panic!("{sample}: {error}"));
        assert!(parts.extra.is_empty(), "{sample}: a sample holds no entries the kit does not write");
        assert_eq!(kit_save(&parts.content, parts.blobs), bytes, "{sample} saves to its own bytes");
    }
}
