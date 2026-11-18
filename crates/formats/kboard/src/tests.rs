use super::*;

fn sample() -> Container {
    let blob = b"\x89PNG fake".to_vec();
    Container {
        meta: Meta {
            kdm: 1,
            schema_version: 2,
            doc_id: "doc-1".into(),
            title: "Plan".into(),
            app: App { kit: "kit.board".into(), version: "0.0.0".into() },
            ..Default::default()
        },
        snapshot: b"snapshot bytes".to_vec(),
        changes: vec![b"one".to_vec(), b"two".to_vec()],
        blobs: BTreeMap::from([(sha256_hex(&blob), blob)]),
        extra: vec![("later/thing.json".into(), b"{}".to_vec())],
    }
}

#[test]
fn round_trips_every_part_with_mimetype_first_and_stored() {
    let bytes = write(&sample()).unwrap();
    assert!(is_container(&bytes));
    assert_eq!(&bytes[30..38], b"mimetype");
    assert_eq!(&bytes[38..38 + MIME.len()], MIME.as_bytes());
    let back = read(&bytes).unwrap();
    assert_eq!(back.snapshot, b"snapshot bytes");
    assert_eq!(back.changes, vec![b"one".to_vec(), b"two".to_vec()]);
    assert_eq!(back.meta.changes, vec!["changes/000001.bin", "changes/000002.bin"]);
    assert_eq!(back.meta.format, FORMAT);
    assert_eq!(back.meta.format_version, FORMAT_VERSION);
    assert_eq!(back.blobs, sample().blobs);
    assert_eq!(back.extra, sample().extra, "an unknown entry survives");
    assert_eq!(write(&back).unwrap(), bytes, "writing what was read gives the same bytes");
}

/// A ZIP built entry by entry, for damaged files.
fn zip_of(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut out);
        for (name, bytes, deflate) in entries {
            let method = if *deflate { zip::CompressionMethod::Deflated } else { zip::CompressionMethod::Stored };
            zip.start_file(*name, zip::write::SimpleFileOptions::default().compression_method(method)).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    out.into_inner()
}

fn meta_json(changes: &[&str]) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "format": FORMAT, "formatVersion": 1, "kdm": 1, "schemaVersion": 2, "docId": "d",
        "snapshot": { "entry": "snapshot.bin", "version": "" }, "changes": changes
    }))
    .unwrap()
}

#[test]
fn refuses_foreign_files_as_not_a_board() {
    let meta = meta_json(&[]);
    let mime = MIME.as_bytes();
    let not = |bytes: &[u8]| matches!(read(bytes), Err(ContainerError::NotABoard(_)));
    assert!(not(b""));
    assert!(not(b"not a zip"));
    assert!(not(b"{\"format\":\"kition-board\",\"version\":2}"), "a JSON envelope is not a board");
    assert!(not(&zip_of(&[("meta.json", &meta, true), ("mimetype", mime, false)])), "mimetype not first");
    assert!(not(&zip_of(&[("mimetype", mime, true), ("meta.json", &meta, true)])), "mimetype compressed");
    assert!(not(&zip_of(&[("mimetype", b"application/vnd.kition.layout+zip", false)])), "another kit's MIME");
    let other = serde_json::to_vec(&serde_json::json!({ "format": "kition.design", "formatVersion": 1 })).unwrap();
    assert!(not(&zip_of(&[("mimetype", mime, false), ("meta.json", &other, true), ("snapshot.bin", b"s", false)])), "another format");
}

#[test]
fn refuses_damaged_files() {
    let meta = meta_json(&[]);
    let mime = MIME.as_bytes();
    let bad = |bytes: &[u8]| matches!(read(bytes), Err(ContainerError::Damaged(_)));
    assert!(bad(&zip_of(&[("mimetype", mime, false), ("snapshot.bin", b"s", false)])), "no meta.json");
    assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &meta, true)])), "no snapshot");
    assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", b"{ not json", true), ("snapshot.bin", b"s", false)])), "meta.json is not JSON");
    let listed = meta_json(&["changes/000001.bin"]);
    assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &listed, true), ("snapshot.bin", b"s", false)])), "listed change missing");
    for name in ["../evil", "/abs", "a\\b", "C:/x", "a/../b", "./x"] {
        assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &meta, true), ("snapshot.bin", b"s", false), (name, b"x", false)])), "{name}");
    }
    let wrong_hash = format!("blobs/{}", sha256_hex(b"other"));
    assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &meta, true), ("snapshot.bin", b"s", false), (&wrong_hash, b"x", false)])));
    let bomb = serde_json::to_vec(&serde_json::json!({
        "format": FORMAT, "formatVersion": 1, "kdm": 1, "schemaVersion": 2, "docId": "d",
        "snapshot": { "entry": "snapshot.bin" }, "pad": " ".repeat(200_000)
    }))
    .unwrap();
    assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &bomb, true), ("snapshot.bin", b"s", false)])), "deflate bomb");
    let other_snapshot = serde_json::to_vec(&serde_json::json!({
        "format": FORMAT, "formatVersion": 1, "kdm": 1, "schemaVersion": 2, "docId": "d", "snapshot": { "entry": "other.bin" }
    }))
    .unwrap();
    assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &other_snapshot, true), ("snapshot.bin", b"s", false)])));
    // Truncated archives never panic.
    let whole = write(&sample()).unwrap();
    for cut in [40, 80, whole.len() / 2, whole.len() - 1] {
        assert!(read(&whole[..cut]).is_err(), "cut at {cut}");
    }
}

#[test]
fn refuses_a_newer_format_with_its_version() {
    let mime = MIME.as_bytes();
    let newer = serde_json::to_vec(&serde_json::json!({ "format": FORMAT, "formatVersion": 9, "snapshot": { "entry": "snapshot.bin" } })).unwrap();
    assert_eq!(
        read(&zip_of(&[("mimetype", mime, false), ("meta.json", &newer, true), ("snapshot.bin", b"s", false)])),
        Err(ContainerError::TooNew(9))
    );
}

#[test]
fn ignores_change_entries_meta_json_does_not_list() {
    let meta = meta_json(&[]);
    let ok = read(&zip_of(&[
        ("mimetype", MIME.as_bytes(), false),
        ("meta.json", &meta, true),
        ("snapshot.bin", b"s", false),
        ("changes/000009.bin", b"c", false),
    ]))
    .unwrap();
    assert!(ok.changes.is_empty());
    assert!(ok.extra.is_empty());
}

#[test]
fn compaction_follows_the_thresholds() {
    assert!(!wants_compaction(1000, &[vec![0; 100]]));
    assert!(!wants_compaction(1000, &[vec![0; 250]]));
    assert!(wants_compaction(1000, &[vec![0; 251]]));
    assert!(!wants_compaction(1_000_000, &vec![vec![0; 1]; COMPACT_ENTRIES]));
    assert!(wants_compaction(1_000_000, &vec![vec![0; 1]; COMPACT_ENTRIES + 1]));
}

#[test]
fn base64_round_trips() {
    for bytes in [&b""[..], b"a", b"ab", b"abc", b"abcd", &[0, 255, 7, 128, 9]] {
        assert_eq!(base64_decode(&base64_encode(bytes)), bytes);
    }
    assert_eq!(base64_encode(b"abcd"), "YWJjZA==");
}
