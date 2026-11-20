//! The preview an agent gets of `conformance/basic.kboard` (KHP A39) is the expected one,
//! `conformance/golden/basic.png`, within the tolerance the kit states in
//! `conformance/golden/tolerance.json`, compared as `khp check` compares them: drawn by the kit's
//! own engine with the fonts it ships, through the `view.screenshot` a kits server asks for.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};

use kition_board_kit::BoardDocument;
use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit_server::protocol::ScreenshotScope;
use kition_kit_server::{KitBackend, OpenRequest, ScreenshotRequest};
use serde_json::Value;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn decode(png: &[u8]) -> image::RgbaImage {
    image::load_from_memory_with_format(png, image::ImageFormat::Png).unwrap().to_rgba8()
}

#[test]
fn the_preview_of_basic_is_the_golden_preview_within_the_stated_tolerance() {
    assert_eq!(kition_board::add_shipped_fonts(), 2, "the shipped fonts are built in");
    let root = std::env::temp_dir().join(format!("kition-board-preview-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    std::fs::copy(repo().join("conformance/basic.kboard"), root.join("basic.kboard")).unwrap();
    let mut kit = Headless::<BoardDocument>::with_replica_port(
        KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_board_kit::EDITOR_STATE).unwrap(),
    );
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let opened = kit.open(OpenRequest { file: &folder.file("basic.kboard").unwrap(), read_only: true, create: None }).unwrap();
    let preview = decode(
        &kit.screenshot(&opened.id, &ScreenshotRequest { scope: ScreenshotScope::Canvas, max_size: 1024, scale: 1.0, target: None }).unwrap().png,
    );
    let first = *preview.get_pixel(0, 0);
    assert!(preview.pixels().any(|pixel| *pixel != first), "the preview is not one colour");

    let golden = decode(&std::fs::read(repo().join("conformance/golden/basic.png")).unwrap());
    let tolerance: Value = serde_json::from_slice(&std::fs::read(repo().join("conformance/golden/tolerance.json")).unwrap()).unwrap();
    let delta = tolerance["maxChannelDelta"].as_u64().unwrap();
    let allowed = tolerance["maxDifferentPixels"].as_u64().unwrap();
    assert_eq!(preview.dimensions(), golden.dimensions());
    let different =
        preview.pixels().zip(golden.pixels()).filter(|(a, b)| a.0.iter().zip(b.0.iter()).any(|(a, b)| u64::from(a.abs_diff(*b)) > delta)).count()
            as u64;
    assert!(different <= allowed, "{different} pixels differ from golden/basic.png, the tolerance allows {allowed}");
    let _ = std::fs::remove_dir_all(root);
}
