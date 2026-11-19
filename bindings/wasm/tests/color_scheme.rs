//! The board in the host's color scheme (task dark-surface-board-sheet, row H16 of Kition's host
//! matrix): the editor's minimap is drawn on the dark paper with the tokens' dark colours
//! (`renderScheme`), while `render`, `file.render` and `file.export` stay light and byte for byte
//! what they were; asking for the dark minimap changes nothing in the board, its revision or its
//! saved state.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use serde_json::json;

const W: u32 = 480;
const H: u32 = 270;

/// A board with shapes in the default colours, a sticky, a label, a connector and a text.
fn board() -> BoardDocument {
    let mut doc = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    let made = doc
        .execute(
            "element.create",
            json!({ "elements": [
                { "kind": "shape", "position": [100, 100], "size": [400, 240], "text": "Plan the launch" },
                { "kind": "shape", "position": [900, 100], "size": [300, 300], "shapeStyle": "sticky", "text": "Ask Ana" },
                { "kind": "text", "position": [100, 600], "text": "Notes for the week" },
                { "kind": "shape", "position": [900, 600], "size": [200, 200], "fillColor": "#ff0000" },
            ] }),
        )
        .unwrap();
    let ids: Vec<String> = made["created"].as_array().unwrap().iter().map(|id| id.as_str().unwrap().to_owned()).collect();
    doc.execute("connector.create", json!({ "from": { "element": ids[0] }, "to": { "element": ids[1] } })).unwrap();
    doc
}

/// The relative luminance of a straight RGBA pixel.
fn luminance(px: &[u8]) -> f64 {
    let channel = |v: u8| {
        let c = f64::from(v) / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel(px[0]) + 0.7152 * channel(px[1]) + 0.0722 * channel(px[2])
}

fn median_luminance(rgba: &[u8]) -> f64 {
    let mut values: Vec<f64> = rgba.chunks(4).map(luminance).collect();
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

#[test]
fn the_minimap_follows_the_scheme_and_everything_else_stays_light() {
    let doc = board();
    let scale = f64::from(W) / f64::from(doc.width());
    let light = doc.render(0.0, 0.0, scale, W, H).unwrap();
    let (revision, dirty) = (doc.revision(), doc.dirty());

    let dark = doc.render_scheme("dark", 0.0, 0.0, scale, W, H).unwrap();
    assert_eq!(dark.len(), light.len());
    assert!(median_luminance(&light) >= 0.9, "the light board is white: {}", median_luminance(&light));
    assert!(median_luminance(&dark) <= 0.02, "the dark minimap is on the dark paper: {}", median_luminance(&dark));
    // Light text and lines on it: the default ink is drawn light.
    assert!(dark.chunks(4).any(|px| luminance(px) > 0.6), "the dark minimap draws its ink light");
    // The file's own red stays red.
    assert!(dark.chunks(4).any(|px| px[0] > 240 && px[1] < 16 && px[2] < 16), "the file's #ff0000 is kept");

    // The light scheme is the contract's `render`, byte for byte, before and after the dark one.
    assert_eq!(doc.render_scheme("light", 0.0, 0.0, scale, W, H).unwrap(), light);
    assert_eq!(doc.render(0.0, 0.0, scale, W, H).unwrap(), light);
    // A view concern: nothing changed in the board.
    assert_eq!((doc.revision(), doc.dirty()), (revision, dirty));
    assert!(doc.render_scheme("sepia", 0.0, 0.0, scale, W, H).unwrap_err().starts_with("invalid-params"));
}

#[test]
fn the_dark_minimap_moves_nothing() {
    let doc = board();
    let scale = f64::from(W) / f64::from(doc.width());
    let light = doc.render(0.0, 0.0, scale, W, H).unwrap();
    let dark = doc.render_scheme("dark", 0.0, 0.0, scale, W, H).unwrap();
    // Every pixel that is not the paper in one scheme is not the paper in the other: the same
    // shapes, lines and letters in the same places (anti-aliased edges included, at a margin).
    let paper_light = [255, 255, 255];
    let paper_dark = [0x1e, 0x1e, 0x1e];
    let away = |px: &[u8], paper: [u8; 3]| px.iter().zip(paper).map(|(a, b)| a.abs_diff(b)).max().unwrap_or(0) > 24;
    let drawn_light: Vec<bool> = light.chunks(4).map(|px| away(px, paper_light)).collect();
    let drawn_dark: Vec<bool> = dark.chunks(4).map(|px| away(px, paper_dark)).collect();
    let differ = drawn_light.iter().zip(&drawn_dark).filter(|(a, b)| a != b).count();
    let drawn = drawn_light.iter().filter(|a| **a).count();
    assert!(drawn > 500, "the board draws something: {drawn}");
    assert!(differ * 50 < drawn, "the same pixels are drawn in both schemes: {differ} of {drawn} differ");
}

#[test]
fn previews_and_exports_are_drawn_light_after_the_dark_minimap() {
    let mut doc = board();
    let (revision, dirty) = (doc.revision(), doc.dirty());
    let preview = doc.execute("file.render", json!({})).unwrap();
    let export = doc.execute("file.export", json!({ "format": "svg" })).unwrap();
    let scale = f64::from(W) / f64::from(doc.width());
    doc.render_scheme("dark", 0.0, 0.0, scale, W, H).unwrap();
    assert_eq!(doc.execute("file.render", json!({})).unwrap(), preview);
    assert_eq!(doc.execute("file.export", json!({ "format": "svg" })).unwrap(), export);
    assert_eq!((doc.revision(), doc.dirty()), (revision, dirty));
}
