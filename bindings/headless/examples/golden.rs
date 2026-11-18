//! Writes the expected preview of `conformance/basic.kboard`, `conformance/golden/basic.png`, that
//! `khp check` compares `board_inspect {preview: true}` with (KHP A39, khp
//! `spec/conformance.md`): drawn by the kit's own engine with the fonts the kit ships, through the
//! same `view.screenshot` (canvas, at most 1024 pixels) a kits server asks the headless kit for.
//!
//! `cargo run -p kition-board --example golden` (with kition-fonts beside the repository or
//! `KITION_FONTS_DIR` set, as for the executable). Review the picture before committing it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use kition_board_kit::BoardDocument;
use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit_server::protocol::ScreenshotScope;
use kition_kit_server::{KitBackend, OpenRequest, ScreenshotRequest};

/// The longest side a kits server asks a preview for when the call gives no `maxSize`.
const MAX_SIZE: u32 = 1024;

fn main() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert_eq!(kition_board::add_shipped_fonts(), 2, "the build found no kition-fonts checkout: the preview would have no text");
    let root = std::env::temp_dir().join(format!("kition-board-golden-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    std::fs::copy(repo.join("conformance/basic.kboard"), root.join("basic.kboard")).unwrap();
    let mut kit = Headless::<BoardDocument>::with_replica_port(KitInfo::from_manifest(include_str!("../../../kit.json"), &[], &[]).unwrap());
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let opened = kit.open(OpenRequest { file: &folder.file("basic.kboard").unwrap(), read_only: true, create: None }).unwrap();
    let shot =
        kit.screenshot(&opened.id, &ScreenshotRequest { scope: ScreenshotScope::Canvas, max_size: MAX_SIZE, scale: 1.0, target: None }).unwrap();
    let target = repo.join("conformance/golden/basic.png");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, &shot.png).unwrap();
    println!("conformance/golden/basic.png: {} x {}", shot.width, shot.height);
    let _ = std::fs::remove_dir_all(root);
}
