//! Pictures as blobs (design note 1.2, 2.1): they travel in the `.kboard` file and between
//! replicas (`blobs`, `blobsSince`, `addBlobs`), and a picture whose bytes arrive after its change
//! (`blobs-late`) shows once they come.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use kition_board_kboard as kboard;
use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kit::kdm::Origin;
use kition_kit::session::KitSessionDocument;
use serde_json::json;

const PICTURE: &[u8] = b"\x89PNG\r\n\x1a\n not really a picture";

/// Adds an image element showing `bytes` to the first page, through the session.
fn add_picture(doc: &mut BoardDocument, bytes: &[u8]) -> String {
    let blob = doc.session_mut().blobs_mut().put(bytes, "image/png");
    let page = doc.model().pages()[0].id.clone();
    let hash = blob.hash.clone();
    doc.session_mut()
        .execute(&Origin::person("person:test"), |_, tx| {
            let mut fields = serde_json::Map::new();
            fields.insert("kind".into(), json!("image"));
            fields.insert("position".into(), json!({ "x": 10, "y": 10 }));
            fields.insert("size".into(), json!({ "w": 100, "h": 80 }));
            fields.insert("image".into(), serde_json::to_value(&blob).unwrap());
            fields.insert("alt".into(), json!("A chart"));
            let id = tx.create("Element", fields)?;
            let at = tx.list_items(&page, "elements")?.len();
            tx.list_insert(&page, "elements", at, &id)?;
            Ok(())
        })
        .unwrap();
    hash
}

#[test]
fn pictures_travel_in_the_file() {
    let mut doc = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    let hash = add_picture(&mut doc, PICTURE);
    let bytes = doc.save(None).unwrap();
    let parts = kboard::read(&bytes).unwrap();
    assert_eq!(parts.blobs.get(&hash).map(Vec::as_slice), Some(PICTURE));
    let back = BoardDocument::open("Plan.kboard", &bytes).unwrap();
    assert!(back.has_blob(&hash));
    assert!(back.missing_blobs().is_empty());
    assert_eq!(back.inspect().unwrap()["missingBlobs"], json!([]));
}

#[test]
fn a_picture_whose_bytes_come_late_shows_when_they_arrive() {
    let mut a = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    // A replica that follows `a` (the page's or a draft's): from its snapshot, on its line of changes.
    let mut b = BoardDocument::open_with_replica("Plan.kboard", &[], &kition_kit::session::__snapshot(&a).unwrap(), &[]).unwrap();
    let since = b.version().encode();
    let hash = add_picture(&mut a, PICTURE);
    // The changes arrive first.
    let changes = kition_kit::session::__export_changes(&a, &since).unwrap();
    KitSessionDocument::import_changes(&mut b, &changes).unwrap();
    assert_eq!(b.missing_blobs(), vec![hash.clone()]);
    assert_eq!(b.inspect().unwrap()["missingBlobs"], json!([hash.clone()]));
    let without = kboard::read(&b.save(None).unwrap()).unwrap();
    assert!(without.blobs.is_empty(), "a save never invents bytes it lacks");
    // Then the blobs: only what b lacks.
    let bundle = a.blobs_since(&since).unwrap();
    assert!(!bundle.is_empty());
    let revision = b.revision();
    assert!(b.add_blobs(&bundle).unwrap(), "a late blob changes the view");
    assert!(b.revision() > revision);
    assert!(b.missing_blobs().is_empty());
    assert!(!b.add_blobs(&bundle).unwrap(), "adding them again changes nothing");
    assert_eq!(kboard::read(&b.save(None).unwrap()).unwrap().blobs.get(&hash).map(Vec::as_slice), Some(PICTURE));
    // A peer at the current version needs nothing; one with nothing needs every referenced blob.
    assert!(b.blobs_since(&b.version().encode()).unwrap().is_empty());
    assert_eq!(a.blobs_since(&[]).unwrap(), a.blobs_bundle());
}

#[test]
fn a_board_opens_from_a_stored_replica_and_its_blobs() {
    let mut a = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    let hash = add_picture(&mut a, PICTURE);
    let snapshot = kition_kit::session::__snapshot(&a).unwrap();
    let alone = BoardDocument::open_with_replica("Plan.kboard", &[], &snapshot, &a.blobs_bundle()).unwrap();
    assert_eq!(alone.doc_id(), a.doc_id());
    assert!(alone.has_blob(&hash));
    // With the file too, edits only the file has are kept.
    let file = a.save(None).unwrap();
    let mut later = BoardDocument::open("Plan.kboard", &file).unwrap();
    later
        .execute("element.add", json!({ "elements": [{ "kind": "shape", "position": { "x": 300, "y": 300 }, "size": { "w": 50, "h": 50 } }] }))
        .unwrap();
    let newer_file = later.save(None).unwrap();
    let both = BoardDocument::open_with_replica("Plan.kboard", &newer_file, &snapshot, &[]).unwrap();
    assert_eq!(both.doc_id(), a.doc_id());
    assert_eq!(both.model().element_count(), 2);
    assert!(both.has_blob(&hash), "the file's blobs come along");
    assert!(BoardDocument::open_with_replica("Plan.kboard", &[], b"not a replica", &[]).is_err());
    assert!(BoardDocument::open_with_replica("Plan.kboard", &[], &snapshot, b"short").is_err(), "a damaged blob bundle");
}

#[test]
fn saves_write_the_content_the_document_holds_now() {
    let mut doc = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    let shapes: Vec<_> = (0..40).map(|x| json!({ "kind": "shape", "position": { "x": x * 30, "y": 400 }, "size": { "w": 20, "h": 20 } })).collect();
    doc.execute("element.add", json!({ "elements": shapes })).unwrap();
    let first = doc.save(None).unwrap();
    doc.mark_saved();
    assert_eq!(doc.save(None).unwrap(), first, "a save of the version the file holds writes the same bytes");
    let mut doc = BoardDocument::open("Plan.kboard", &first).unwrap();
    doc.execute("element.add", json!({ "elements": [{ "kind": "shape", "position": { "x": 0, "y": 0 }, "size": { "w": 50, "h": 50 } }] })).unwrap();
    let second = doc.save(None).unwrap();
    assert!(doc.dirty(), "save leaves the document dirty until mark_saved");
    doc.mark_saved();
    assert!(!doc.dirty());
    let parts = kboard::read(&second).unwrap();
    assert_ne!(parts.content, kboard::read(&first).unwrap().content);
    assert_eq!(doc.save(None).unwrap(), second, "an unchanged session saves the same bytes");
    assert_eq!(BoardDocument::open("Plan.kboard", &second).unwrap().model().element_count(), 41);
}

/// Only the blobs the current state references travel in the file: a picture deleted before the
/// save leaves its bytes behind, though the history still references them.
#[test]
fn a_deleted_picture_leaves_its_bytes_out_of_the_file() {
    let mut doc = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    let kept = add_picture(&mut doc, PICTURE);
    let other: &[u8] = b"\x89PNG\r\n\x1a\n another picture";
    let gone = add_picture(&mut doc, other);
    let model = doc.model();
    let id = model.pages()[0]
        .elements
        .iter()
        .find(|id| model.element(id).and_then(|element| element.image.as_ref()).is_some_and(|image| image.hash == gone))
        .unwrap()
        .clone();
    doc.execute("element.delete", json!({ "ids": [id.as_str()] })).unwrap();
    let parts = kboard::read(&doc.save(None).unwrap()).unwrap();
    assert!(parts.blobs.contains_key(&kept));
    assert!(!parts.blobs.contains_key(&gone), "the deleted picture's bytes are not in the file");
    let back = BoardDocument::open("Plan.kboard", &kboard::write(&parts).unwrap()).unwrap();
    assert!(back.missing_blobs().is_empty(), "the board the file holds misses nothing");
}
