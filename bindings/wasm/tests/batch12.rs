//! C8-land-12-board: model batches invalidate document caches on both SDK paths.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
use kition_board_kit::BoardDocument;
use kition_kit::{
    KitDocument,
    session::model::{self, Exchange},
};
use serde_json::json;

#[test]
fn direct_and_exchange_batches_refresh_cached_area_and_pixels() {
    for direct in [true, false] {
        let mut doc = BoardDocument::create("Cache.kboard", 800, 600).unwrap();
        let id =
            doc.execute("element.create", json!({"elements":[{"kind":"shape","position":[0,0],"fillColor":"red"}]})).unwrap()["created"][0].clone();
        doc.mark_saved();
        let width = doc.width();
        let pixels = doc.render(0.0, 0.0, 1.0, 100, 100).unwrap();
        let request = json!({"ops":[{"op":"set","target":id,"field":"position","value":{"x":5000,"y":0}}]});
        if direct {
            model::session_batch(&mut doc, &request, false).unwrap();
        } else {
            Exchange::new().batch(&mut doc, &request, false).unwrap();
        }
        assert!(doc.width() > width, "cached area must follow a direct={direct} batch");
        assert_ne!(doc.render(0.0, 0.0, 1.0, 100, 100).unwrap(), pixels, "cached pixels must follow a direct={direct} batch");
        doc.prepare_save().unwrap();
        let back = BoardDocument::open("Cache.kboard", &doc.save(None).unwrap()).unwrap();
        assert_eq!(doc.model(), back.model());
        assert_eq!(doc.width(), back.width());
    }
}
