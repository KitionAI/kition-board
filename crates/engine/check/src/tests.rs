#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kdm::blobs::SharedBlobs;
use kition_board_kdm::{BoardModel, create_replica};
use kition_kit::diagnostics::{Diagnostic, Severity};
use kition_kit::kdm::{ActorId, ObjectId, Origin};
use kition_kit::session::KitSession;
use serde_json::{Value, json};

use crate::check;

struct Board {
    session: KitSession<BoardModel>,
    blobs: SharedBlobs,
}

impl Board {
    fn new() -> Self {
        let replica = create_replica("Plan", None, ActorId::random(), &Origin::system("create")).unwrap();
        let blobs = SharedBlobs::default();
        let session = KitSession::from_replica(replica, Box::new(blobs.clone())).unwrap();
        Self { session, blobs }
    }

    fn run(&mut self, command: &str, params: Value) -> Value {
        let mut blobs = self.blobs.clone();
        let page = self.session.model().pages()[0].id.clone();
        self.session
            .execute(&Origin::person("person:test"), |model, tx| {
                let mut ctx = kition_board_model::Ctx { blobs: &mut blobs, doc_id: "doc", active_page: Some(&page) };
                kition_board_model::run(model, tx, &mut ctx, command, &params)
            })
            .unwrap()
            .0
    }

    fn shape(&mut self, x: f64, y: f64, extra: Value) -> String {
        let mut element = json!({ "kind": "shape", "position": [x, y], "size": [160, 80] });
        if let (Some(target), Some(extra)) = (element.as_object_mut(), extra.as_object()) {
            target.extend(extra.clone());
        }
        self.run("element.create", json!({ "elements": [element] }))["created"][0].as_str().unwrap().to_owned()
    }

    fn check(&self) -> Vec<Diagnostic> {
        check(self.session.model(), &self.blobs)
    }

    fn codes(&self) -> Vec<String> {
        self.check().into_iter().map(|found| found.code).collect()
    }
}

fn count(codes: &[String], code: &str) -> usize {
    codes.iter().filter(|found| *found == code).count()
}

#[test]
fn a_healthy_board_has_nothing_to_report() {
    let mut board = Board::new();
    let a = board.shape(0.0, 0.0, json!({ "shapeStyle": "flow-node", "text": "Start" }));
    let b = board.shape(300.0, 0.0, json!({ "shapeStyle": "flow-node", "text": "End" }));
    board.run("connector.create", json!({ "from": { "element": a }, "to": { "element": b } }));
    board.run("frame.create", json!({ "box": [-40, -40, 500, 200], "children": [a, b] }));
    assert_eq!(board.codes(), Vec::<String>::new());
}

#[test]
fn flow_chart_rules() {
    let mut board = Board::new();
    let a = board.shape(0.0, 0.0, json!({ "shapeStyle": "flow-node", "text": "Start" }));
    let b = board.shape(100.0, 20.0, json!({ "shapeStyle": "flow-node" }));
    let c = board.shape(600.0, 0.0, json!({ "shapeStyle": "mind-node" }));
    let found = board.check();
    let codes: Vec<String> = found.iter().map(|found| found.code.clone()).collect();
    assert_eq!(count(&codes, "board.missing-label"), 2, "{codes:?}");
    assert_eq!(count(&codes, "board.disconnected-node"), 2, "two flow nodes, neither connected");
    assert_eq!(count(&codes, "board.overlap"), 1, "the two flow nodes overlap");
    let missing = found.iter().find(|found| found.code == "board.missing-label" && found.target.object_id.as_deref() == Some(c.as_str())).unwrap();
    assert_eq!(missing.fix.as_ref().unwrap().command, "element.setText");
    assert_eq!(missing.target.page, Some(1));
    let _ = (a, b);
}

#[test]
fn overflowing_overlapping_and_outside_text() {
    let mut board = Board::new();
    board.shape(
        0.0,
        0.0,
        json!({ "text": "A label far too long to fit inside a small box at all, wrapping onto many many lines", "size": [80, 40] }),
    );
    board.run(
        "element.create",
        json!({ "elements": [
        { "kind": "text", "position": [0, 300], "text": "Notes" },
        { "kind": "text", "position": [10, 305], "text": "More notes" },
    ] }),
    );
    let frame = board.run("frame.create", json!({ "box": [500, 0, 700, 200] }))["frame"].as_str().unwrap().to_owned();
    let inside = board.shape(520.0, 20.0, json!({ "parent": frame, "text": "In" }));
    board.run("element.move", json!({ "ids": [inside], "delta": [150, 0] }));
    let codes = board.codes();
    assert_eq!(count(&codes, "board.label-overflow"), 1, "{codes:?}");
    assert_eq!(count(&codes, "board.overlapping-label"), 1, "{codes:?}");
    assert_eq!(count(&codes, "board.outside-frame"), 1, "{codes:?}");
    assert_eq!(count(&codes, "board.empty-frame"), 0, "the frame has a child");
    let empty = board.run("frame.create", json!({ "box": [900, 0, 1000, 100] }));
    assert!(empty["frame"].is_string());
    assert_eq!(count(&board.codes(), "board.empty-frame"), 1);
}

#[test]
fn connectors_through_nodes_and_dangling_bindings() {
    let mut board = Board::new();
    let a = board.shape(0.0, 0.0, json!({ "text": "A" }));
    let b = board.shape(600.0, 0.0, json!({ "text": "B" }));
    let middle = board.shape(300.0, 0.0, json!({ "text": "In the way" }));
    let link =
        board.run("connector.create", json!({ "from": { "element": a, "anchor": [1, 0.5] }, "to": { "element": b, "anchor": [0, 0.5] } }))["created"]
            [0]
        .as_str()
        .unwrap()
        .to_owned();
    let found = board.check();
    let crossing = found.iter().find(|found| found.code == "board.connector-crosses-node").unwrap();
    assert_eq!(crossing.severity, Severity::Info);
    assert_eq!(crossing.target.object_id.as_deref(), Some(link.as_str()));
    // A target deleted without the repair (an unrepaired import, a read-only reader).
    let target = ObjectId::from_string(&b);
    board.session.execute(&Origin::person("person:test"), |_, tx| Ok(tx.delete(&target)?)).unwrap();
    let found = board.check();
    let dangling: Vec<&Diagnostic> = found.iter().filter(|found| found.code == "board.dangling-binding").collect();
    assert_eq!(dangling.len(), 1, "{found:?}");
    assert_eq!(found[0].code, "board.dangling-binding", "errors come first");
    assert_eq!(dangling[0].fix.as_ref().unwrap().params.as_ref().unwrap()["terminal"], "end");
    let _ = middle;
}

#[test]
fn pictures_without_bytes_and_cards_without_targets() {
    let mut board = Board::new();
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEklEQVR4nGP4z8DAwMDAwMAAAA0EAf9n8uLhAAAAAElFTkSuQmCC";
    board.run("image.insert", json!({ "data": png, "mime": "image/png", "position": [0, 0], "size": [40, 40] }));
    assert_eq!(count(&board.codes(), "board.image-missing-blob"), 0, "the bytes are in the board");
    let without = check(board.session.model(), &SharedBlobs::default());
    assert_eq!(without.iter().filter(|found| found.code == "board.image-missing-blob").count(), 1);

    // A card's target is checked only when files are lent.
    board.run("card.create", json!({ "file": "plans/Missing.md", "position": [0, 200] }));
    board.run("card.create", json!({ "url": "https://kition.ai", "position": [400, 200] }));
    assert_eq!(count(&board.codes(), "board.card-target-missing"), 0, "nothing lent: unknown, not missing");
    let root = std::env::temp_dir().join(format!("kition-board-check-{}", std::process::id()));
    std::fs::create_dir_all(root.join("plans")).unwrap();
    kition_kit::host::set_root(Some(root.clone()));
    kition_kit::host::set_folder("");
    let missing = count(&board.codes(), "board.card-target-missing");
    std::fs::write(root.join("plans/Missing.md"), "# Here now").unwrap();
    let present = count(&board.codes(), "board.card-target-missing");
    kition_kit::host::set_root(None);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!((missing, present), (1, 0));
}
