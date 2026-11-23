use super::*;

fn sample() -> Container {
    let blob = b"\x89PNG fake".to_vec();
    Container {
        meta: Meta { kit: "kit.board".into(), kdm: 1, schema_version: 2, doc_id: "doc-1".into(), ..Default::default() },
        content: b"content bytes".to_vec(),
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
    assert_eq!(back.content, b"content bytes");
    assert_eq!(back.meta.format, FORMAT);
    assert_eq!(back.meta.format_version, FORMAT_VERSION);
    assert_eq!(back.blobs, sample().blobs);
    assert_eq!(back.extra, sample().extra, "an unknown entry survives");
    assert_eq!(write(&back).unwrap(), bytes, "writing what was read gives the same bytes");
    assert_eq!(write(&sample()).unwrap(), bytes, "the same parts give the same bytes: no entry carries a time");
}

/// `meta.json` holds what identifies and describes the content, and nothing a save changes.
#[test]
fn meta_json_names_the_document_and_nothing_a_save_changes() {
    let back = read(&write(&sample()).unwrap()).unwrap();
    let json = serde_json::to_value(&back.meta).unwrap();
    let mut keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["docId", "format", "formatVersion", "kdm", "kit", "schemaVersion"]);
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

fn meta_json() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "format": FORMAT, "formatVersion": 1, "kit": "kit.board", "kdm": 1, "schemaVersion": 2, "docId": "d" }))
        .unwrap()
}

#[test]
fn refuses_foreign_files_as_not_a_board() {
    let meta = meta_json();
    let mime = MIME.as_bytes();
    let not = |bytes: &[u8]| matches!(read(bytes), Err(ContainerError::NotABoard(_)));
    assert!(not(b""));
    assert!(not(b"not a zip"));
    assert!(not(b"{\"format\":\"kition-board\",\"version\":2}"), "a JSON envelope is not a board");
    assert!(not(&zip_of(&[("meta.json", &meta, true), ("mimetype", mime, false)])), "mimetype not first");
    assert!(not(&zip_of(&[("mimetype", mime, true), ("meta.json", &meta, true)])), "mimetype compressed");
    assert!(not(&zip_of(&[("mimetype", b"application/vnd.kition.layout+zip", false)])), "another kit's MIME");
    let other = serde_json::to_vec(&serde_json::json!({ "format": "kition.design", "formatVersion": 1 })).unwrap();
    assert!(not(&zip_of(&[("mimetype", mime, false), ("meta.json", &other, true), ("content.bin", b"s", false)])), "another format");
}

#[test]
fn refuses_damaged_files() {
    let meta = meta_json();
    let mime = MIME.as_bytes();
    let bad = |bytes: &[u8]| matches!(read(bytes), Err(ContainerError::Damaged(_)));
    assert!(bad(&zip_of(&[("mimetype", mime, false), ("content.bin", b"s", false)])), "no meta.json");
    assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &meta, true)])), "no content");
    assert!(
        bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &meta, true), ("snapshot.bin", b"s", false), ("changes/000001.bin", b"c", false)])),
        "a file with a snapshot and a change log, and no content, is not read"
    );
    assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", b"{ not json", true), ("content.bin", b"s", false)])), "meta.json is not JSON");
    for name in ["../evil", "/abs", "a\\b", "C:/x", "a/../b", "./x"] {
        assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &meta, true), ("content.bin", b"s", false), (name, b"x", false)])), "{name}");
    }
    let wrong_hash = format!("blobs/{}", sha256_hex(b"other"));
    assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &meta, true), ("content.bin", b"s", false), (&wrong_hash, b"x", false)])));
    let bomb = serde_json::to_vec(&serde_json::json!({
        "format": FORMAT, "formatVersion": 1, "kit": "kit.board", "kdm": 1, "schemaVersion": 2, "docId": "d", "pad": " ".repeat(200_000)
    }))
    .unwrap();
    assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &bomb, true), ("content.bin", b"s", false)])), "deflate bomb");
    let no_doc =
        serde_json::to_vec(&serde_json::json!({ "format": FORMAT, "formatVersion": 1, "kit": "kit.board", "kdm": 1, "schemaVersion": 2 })).unwrap();
    assert!(bad(&zip_of(&[("mimetype", mime, false), ("meta.json", &no_doc, true), ("content.bin", b"s", false)])), "meta.json without a docId");
    // Truncated archives never panic.
    let whole = write(&sample()).unwrap();
    for cut in [40, 80, whole.len() / 2, whole.len() - 1] {
        assert!(read(&whole[..cut]).is_err(), "cut at {cut}");
    }
}

#[test]
fn refuses_a_newer_format_with_its_version() {
    let mime = MIME.as_bytes();
    let newer = serde_json::to_vec(&serde_json::json!({ "format": FORMAT, "formatVersion": 9 })).unwrap();
    assert_eq!(
        read(&zip_of(&[("mimetype", mime, false), ("meta.json", &newer, true), ("content.bin", b"s", false)])),
        Err(ContainerError::TooNew(9))
    );
}

#[test]
fn base64_round_trips() {
    for bytes in [&b""[..], b"a", b"ab", b"abc", b"abcd", &[0, 255, 7, 128, 9]] {
        assert_eq!(base64_decode(&base64_encode(bytes)), bytes);
    }
    assert_eq!(base64_encode(b"abcd"), "YWJjZA==");
}
