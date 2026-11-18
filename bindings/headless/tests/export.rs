//! `kition-board` exports as its manifest declares (KHP A35, A41; Kition platform K5, K21): the
//! command `agent.export` names (what `board_export` runs) answers each `exports` extension as a
//! file in the field `agent.export.data` names, the agent may run it, `kit.initialize` reports the
//! formats, and `document.export` writes the same files. Each is valid in its own reader: a PNG that
//! decodes and is not one colour, an SVG that parses, a one-page PDF.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use base64::Engine as _;
use kition_board_kit::BoardDocument;
use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit_server::protocol::Origin;
use kition_kit_server::{KitBackend, OpenRequest};
use serde_json::{Map, Value, json};

const MANIFEST: &str = include_str!("../../../kit.json");

fn manifest() -> Value {
    serde_json::from_str(MANIFEST).unwrap()
}

/// The file is what its format says it is.
fn assert_valid(format: &str, bytes: &[u8]) {
    match format {
        "png" => {
            let picture = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).unwrap().to_rgba8();
            let first = picture.pixels().next().copied().unwrap();
            assert!(picture.pixels().any(|pixel| *pixel != first), "the PNG is one colour");
        }
        "svg" => {
            let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default())
                .unwrap_or_else(|error| panic!("the SVG does not parse: {error}"));
            assert!(tree.root().has_children(), "the SVG draws nothing");
        }
        "pdf" => {
            assert!(bytes.starts_with(b"%PDF-"), "no PDF header");
            assert!(bytes.trim_ascii_end().ends_with(b"%%EOF"), "no PDF trailer");
            // Page objects are `/Type/Page` (with or without a space), the page tree `/Type/Pages`.
            let text = String::from_utf8_lossy(bytes).replace("/Type /", "/Type/");
            let pages = text.matches("/Type/Page").count() - text.matches("/Type/Pages").count();
            assert_eq!(pages, 1, "a board page exports as one PDF page");
            assert!(text.contains("/Count 1"), "the page tree counts one page");
        }
        other => panic!("kit.json exports {other}, which this test does not read"),
    }
}

#[test]
fn issue_agent_export_every_declared_format_exports_through_the_agent_export_command() {
    let manifest = manifest();
    let export = &manifest["agent"]["export"];
    let command = export["command"].as_str().unwrap_or_else(|| panic!("kit.json has no agent.export.command (KHP A41, K21): {export}"));
    let data = export["data"].as_str().unwrap_or_else(|| panic!("kit.json has no agent.export.data (K21): {export}"));
    let formats: Vec<&str> = manifest["exports"].as_array().unwrap().iter().map(|entry| entry["extension"].as_str().unwrap()).collect();
    assert_eq!(formats, ["png", "svg", "pdf"]);

    let mut kit = Headless::<BoardDocument>::with_replica_port(KitInfo::from_manifest(MANIFEST, &[], &[]).unwrap());
    assert_eq!(kit.capabilities()["formats"]["export"], json!(formats), "kit.initialize reports the exports");
    let root = std::env::temp_dir().join(format!("kition-board-export-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/basic.kboard"), root.join("basic.kboard")).unwrap();
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let opened = kit.open(OpenRequest { file: &folder.file("basic.kboard").unwrap(), read_only: false, create: None }).unwrap();
    let listed = kit.commands(Some(&opened.id)).unwrap();
    assert!(listed.iter().any(|listed| listed.id == command), "the agent may run {command}");

    for format in &formats {
        let mut params: Map<String, Value> = export["params"].as_object().cloned().unwrap_or_default();
        params.insert("format".into(), json!(format));
        let executed =
            kit.execute(&opened.id, command, params, Origin::Agent).unwrap_or_else(|error| panic!("{command} {format}: {}", error.message));
        assert!(!executed.changed, "an export never edits the board");
        let answer = executed.result.unwrap();
        let encoded = answer[data].as_str().unwrap_or_else(|| panic!("{command} {format} answers no `{data}`: {answer}"));
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).unwrap();
        assert_valid(format, &bytes);

        let name = format!("basic.{format}");
        kit.export(&opened.id, &folder.export(&name).unwrap(), format, None)
            .unwrap_or_else(|error| panic!("document.export {format}: {}", error.message));
        assert_valid(format, &std::fs::read(root.join(&name)).unwrap());
    }
    assert!(!kit.document(&opened.id).unwrap().dirty);
    let _ = std::fs::remove_dir_all(root);
}
