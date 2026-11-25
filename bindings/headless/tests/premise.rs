//! The Board kit's call context, receipts, ids and the premise guard end to end (task card
//! C4/C5/C6-board): the headless `kition-board` executable under the kits' MCP server's tools
//! (`kition-kits`, the library `kition-kits-mcp` serves, at the pinned kition-sdk revision), on a
//! `.kboard` in a workspace.
//!
//! A board has one id space: elements and pages are named by their object ids everywhere (the
//! commands, `on`, `at.part`, `board_query`, `board_batch`, receipts, `others` and `conflicts`), and
//! the file keeps them. A second agent is a second caller on the same server: its accepted writes
//! are in the document's journal with what they changed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use serde_json::{Value, json};

const ME: &str = "agent-me";
const OTHER: &str = "agent-other";

fn kit() -> &'static KitDefinition {
    KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap()
}

/// Page `Plan`: shapes `Plan` and `Build` joined by a connector, a shape `Ship`, and a frame
/// `Sprint` holding a shape `Inside`; page `Ideas`: a shape `Idea`.
fn board() -> Vec<u8> {
    let mut doc = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    let first = doc.model().pages()[0].id.as_str().to_owned();
    doc.execute("page.rename", json!({ "page": first, "name": "Plan" })).unwrap();
    let made = doc
        .execute(
            "element.add",
            json!({ "elements": [
                { "kind": "shape", "position": [0, 0], "text": "Plan" },
                { "kind": "shape", "position": [300, 0], "text": "Build" },
                { "kind": "shape", "position": [600, 0], "text": "Ship" },
            ] }),
        )
        .unwrap();
    let (plan, build) = (made["created"][0].clone(), made["created"][1].clone());
    doc.execute("connector.add", json!({ "from": { "element": plan }, "to": { "element": build } })).unwrap();
    let inside = doc.execute("element.add", json!({ "elements": [{ "kind": "shape", "position": [40, 440], "text": "Inside" }] })).unwrap();
    doc.execute("frame.add", json!({ "box": [0, 400, 400, 640], "title": "Sprint", "children": inside["created"] })).unwrap();
    let ideas = doc.execute("page.add", json!({ "name": "Ideas" })).unwrap()["page"].clone();
    doc.execute("element.add", json!({ "page": ideas, "elements": [{ "kind": "shape", "position": [0, 0], "text": "Idea" }] })).unwrap();
    doc.save(None).unwrap()
}

struct Fixture {
    kits: KitManager,
    workspace: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("Plan.kboard"), board()).unwrap();
        Self { kits: Self::server(workspace.path()), workspace }
    }

    /// A new server on `workspace` (a process of `kition-kits-mcp` is one of these).
    fn server(workspace: &Path) -> KitManager {
        KitManager::new(Some(workspace.to_path_buf()), vec![(kit(), Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-board"))))])
    }

    fn file(&self) -> Vec<u8> {
        std::fs::read(self.workspace.path().join("Plan.kboard")).unwrap()
    }

    async fn call_as(&self, agent: &str, action: &str, mut input: Value) -> Result<Value, Value> {
        input["path"] = json!("Plan.kboard");
        match tools::call_as(&self.kits, &format!("board_{action}"), &input, CallOptions::default(), agent).await {
            Some(Ok(output)) => Ok(output.output),
            Some(Err(error)) => Err(error.structured()),
            None => panic!("board_{action} is no tool"),
        }
    }

    async fn ok(&self, action: &str, input: Value) -> Value {
        self.by(ME, action, input).await
    }

    /// `agent`'s call, which must succeed.
    async fn by(&self, agent: &str, action: &str, input: Value) -> Value {
        self.call_as(agent, action, input).await.unwrap_or_else(|error| panic!("{agent} board_{action}: {error}"))
    }

    async fn refused(&self, action: &str, input: Value) -> Value {
        match self.call_as(ME, action, input).await {
            Err(error) => error,
            Ok(answer) => panic!("board_{action} was expected to be refused: {answer}"),
        }
    }

    /// The pages as `board_inspect` lists them, and the answer's `revision`.
    async fn pages(&self) -> (Vec<Value>, String) {
        let seen = self.ok("inspect", json!({})).await;
        (seen["details"]["pages"].as_array().unwrap_or_else(|| panic!("pages: {seen}")).clone(), revision(&seen))
    }

    /// The elements of page `n` (from 0) as `board_query` lists them.
    async fn elements(&self, n: usize) -> Vec<Value> {
        let read = self.ok("query", json!({ "follow": format!("pages.{n}.elements"), "fields": ["kind", "text", "parent", "start", "end"] })).await;
        read["objects"].as_array().unwrap_or_else(|| panic!("objects: {read}")).clone()
    }

    /// The id of the element of page `n` whose text is `text`.
    async fn id(&self, n: usize, text: &str) -> String {
        let elements = self.elements(n).await;
        let found = elements.iter().find(|element| text_of(element) == text).unwrap_or_else(|| panic!("no `{text}` in {elements:?}"));
        found["id"].as_str().unwrap().to_owned()
    }

    async fn shutdown(self) {
        self.kits.shutdown().await;
    }
}

/// An element's text as `board_query` answers it (a text field: its runs, `[{insert}]`).
fn text_of(element: &Value) -> String {
    element["fields"]["text"].as_array().into_iter().flatten().filter_map(|run| run["insert"].as_str()).collect()
}

fn revision(answer: &Value) -> String {
    answer["revision"].as_str().unwrap_or_else(|| panic!("a revision: {answer}")).to_owned()
}

fn ids(list: &Value) -> Vec<String> {
    let mut out: Vec<String> =
        list.as_array().map(|items| items.iter().filter_map(|item| item["id"].as_str().map(str::to_owned)).collect()).unwrap_or_default();
    out.sort();
    out
}

fn sorted(mut list: Vec<String>) -> Vec<String> {
    list.sort();
    list
}

/// A refusal of a write whose premises others changed, naming `id` with `reason`.
fn assert_conflict(refused: &Value, id: &str, reason: &str) {
    assert_eq!(refused["error"]["code"], "conflict", "{refused}");
    let found = refused["conflicts"].as_array().and_then(|conflicts| conflicts.iter().find(|conflict| conflict["id"] == id));
    let found = found.unwrap_or_else(|| panic!("no conflict on {id}: {refused}"));
    assert_eq!(found["reason"], reason, "{refused}");
}

fn style(id: &str, color: &str) -> Value {
    json!({ "command": "element.style", "on": [id], "params": { "style": { "fillColor": color } } })
}

// ---- Receipts name what a call changed by the ids board_query lists ----

#[tokio::test(flavor = "multi_thread")]
async fn a_receipt_names_exactly_the_changed_element_by_its_id_with_before_and_after() {
    let f = Fixture::new();
    let ship = f.id(0, "Ship").await;
    let ran = f.ok("run", json!({ "steps": [{ "command": "element.setText", "on": [ship], "params": { "text": "Launch" } }] })).await;
    assert_eq!(
        ran["changes"]["changed"],
        json!([{ "id": ship, "type": "Element", "fields": ["text"], "edits": [{ "field": "text", "before": "Ship", "after": "Launch" }] }]),
        "{ran}"
    );
    for list in ["created", "moved", "deleted"] {
        assert_eq!(ran["changes"][list], json!([]), "{ran}");
    }
    let ran = f.ok("run", json!({ "steps": [style(&ship, "red")] })).await;
    assert_eq!(
        ran["changes"]["changed"],
        json!([{ "id": ship, "type": "Element", "fields": ["fillColor"], "edits": [{ "field": "fillColor", "before": null, "after": "red" }] }]),
        "{ran}"
    );
    // A new element: `created` names it by the id the command answered and board_query lists.
    let ran = f.ok("run", json!({ "steps": [{ "command": "element.add", "params": { "elements": [{ "kind": "shape", "position": [900, 0], "text": "Review" }] } }] })).await;
    let made = ran["completed"][0]["result"]["created"][0].as_str().unwrap().to_owned();
    assert_eq!(ids(&ran["changes"]["created"]), std::slice::from_ref(&made), "{ran}");
    assert_eq!(f.id(0, "Review").await, made);
    f.shutdown().await;
}

/// Deleting a shape frees the ends of the connectors attached to it, and deleting a frame deletes
/// what it holds: the receipt tells all of it, by id, though the command's answer names only what
/// it deleted.
#[tokio::test(flavor = "multi_thread")]
async fn a_deletion_receipt_tells_the_cascade() {
    let f = Fixture::new();
    let elements = f.elements(0).await;
    let connector = elements.iter().find(|element| element["fields"]["kind"] == "connector").unwrap()["id"].as_str().unwrap().to_owned();
    let (plan, frame, inside) = (f.id(0, "Plan").await, f.id(0, "Sprint").await, f.id(0, "Inside").await);
    let page = f.pages().await.0[0]["id"].as_str().unwrap().to_owned();
    let ran = f.ok("run", json!({ "steps": [{ "command": "element.delete", "on": [plan] }] })).await;
    assert_eq!(ran["completed"][0]["result"]["deleted"], json!([plan]), "{ran}");
    assert_eq!(ids(&ran["changes"]["deleted"]), std::slice::from_ref(&plan), "{ran}");
    assert_eq!(ran["changes"]["deleted"][0]["at"], json!({ "parent": page, "field": "elements", "index": 0 }), "{ran}");
    let freed =
        ran["changes"]["changed"].as_array().unwrap().iter().find(|change| change["id"] == connector.as_str()).unwrap_or_else(|| panic!("{ran}"));
    assert_eq!(freed["fields"], json!(["start.binding"]), "{ran}");
    let edit = &freed["edits"][0];
    assert_eq!(edit["field"], "start.binding");
    assert_eq!((edit["before"]["target"].clone(), edit["after"].clone()), (json!(plan), Value::Null), "the end is free now: {ran}");
    let reopened = BoardDocument::open("Plan.kboard", &f.file()).unwrap();
    let saved_end = reopened.model().element(&kition_kit::kdm::ObjectId::from_string(&connector)).unwrap().start.as_ref().unwrap();
    assert!(saved_end.binding.is_none());
    let original = elements.iter().find(|element| element["id"] == connector).unwrap();
    assert_eq!(
        [saved_end.point.x, saved_end.point.y],
        [original["fields"]["start"]["point"][0].as_f64().unwrap(), original["fields"]["start"]["point"][1].as_f64().unwrap()],
        "the detached end stays where it was drawn"
    );
    let ran = f.ok("run", json!({ "steps": [{ "command": "element.delete", "on": [frame] }] })).await;
    assert_eq!(ids(&ran["changes"]["deleted"]), sorted(vec![frame, inside]), "the frame and what it held: {ran}");
    f.shutdown().await;
}

// ---- Steps on two pages; the person's page stays ----

#[tokio::test(flavor = "multi_thread")]
async fn a_run_acts_on_two_pages_and_leaves_the_shown_page_alone() {
    let f = Fixture::new();
    let idea = f.id(1, "Idea").await;
    let shape = |text: &str| json!({ "elements": [{ "kind": "shape", "position": [400, 400], "text": text }] });
    let ran = f
        .ok(
            "run",
            json!({ "target": { "page": 2 }, "steps": [
                { "command": "element.add", "params": shape("On Ideas") },
                { "command": "element.style", "on": [idea], "params": { "style": { "fillColor": "green" } } },
                { "command": "element.add", "at": { "part": { "page": 1 } }, "params": shape("On Plan") },
                { "command": "frame.add", "params": { "box": [0, 600, 300, 800], "title": "Also Ideas" } }
            ] }),
        )
        .await;
    assert_eq!(ran["completed"].as_array().unwrap().len(), 4, "{ran}");
    let texts = |elements: Vec<Value>| elements.iter().map(text_of).collect::<Vec<_>>();
    assert_eq!(texts(f.elements(1).await), ["Idea", "On Ideas", "Also Ideas"]);
    assert!(texts(f.elements(0).await).contains(&"On Plan".to_owned()));
    // Nothing carried over: a run naming no page acts on the page the editor shows (the first).
    f.ok("run", json!({ "steps": [{ "command": "element.add", "params": shape("Unnamed") }] })).await;
    assert!(texts(f.elements(0).await).contains(&"Unnamed".to_owned()), "the target of the last run did not stay");
    // A page the run makes and shows takes the steps after it that name none; the next run does not.
    f.ok(
        "run",
        json!({ "steps": [
            { "command": "page.add", "params": { "name": "Fresh", "activate": true } },
            { "command": "element.add", "params": shape("First on Fresh") }
        ] }),
    )
    .await;
    assert_eq!(texts(f.elements(2).await), ["First on Fresh"]);
    f.ok("run", json!({ "steps": [{ "command": "element.add", "params": shape("Back on Plan") }] })).await;
    assert!(texts(f.elements(0).await).contains(&"Back on Plan".to_owned()), "the new page is not left shown");
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_later_step_changes_nothing() {
    let f = Fixture::new();
    let ship = f.id(0, "Ship").await;
    let before = f.file();
    let refused = f
        .refused(
            "run",
            json!({ "steps": [
                { "command": "element.setText", "on": [ship], "params": { "text": "kept?" } },
                { "command": "element.add", "params": { "page": "nowhere", "elements": [{ "kind": "shape" }] } }
            ] }),
        )
        .await;
    assert_eq!(refused["error"]["index"], json!(1), "{refused}");
    assert_eq!(f.file(), before, "the run was rolled back: nothing changed in the file");
    assert!(f.elements(0).await.iter().any(|element| text_of(element) == "Ship"));
    f.shutdown().await;
}

// ---- What the board lacks or cannot mean is refused ----

#[tokio::test(flavor = "multi_thread")]
async fn a_page_the_board_lacks_a_time_or_a_stray_on_is_refused() {
    let f = Fixture::new();
    let ship = f.id(0, "Ship").await;
    let page = f.pages().await.0[0]["id"].clone();
    let before = f.file();
    let shape = json!({ "elements": [{ "kind": "shape" }] });
    for (input, says) in [
        (json!({ "target": { "page": 3 }, "steps": [{ "command": "element.add", "params": shape }] }), "the board has 2 pages"),
        (json!({ "target": { "page": "nowhere" }, "steps": [{ "command": "element.add", "params": shape }] }), "no page `nowhere`"),
        (json!({ "target": { "sheet": 1 }, "steps": [{ "command": "element.add", "params": shape }] }), "names no part"),
        (json!({ "steps": [{ "command": "element.add", "at": { "time": 2 }, "params": shape }] }), "no timeline"),
        (json!({ "steps": [{ "command": "page.rename", "on": [ship], "params": { "page": page, "name": "X" } }] }), "on"),
        (json!({ "steps": [{ "command": "element.style", "on": [ship], "params": { "ids": [ship], "style": { "fillColor": "red" } } }] }), "ids"),
        (json!({ "steps": [{ "command": "element.setText", "on": ["nothing"], "params": { "text": "x" } }] }), "nothing"),
    ] {
        let refused = f.refused("run", input.clone()).await;
        let code = refused["error"]["code"].as_str().unwrap().to_owned();
        assert!(["invalid-params", "unsupported", "command-failed"].contains(&code.as_str()), "{input}: {refused}");
        assert!(refused.to_string().contains(says), "{input}: {refused}");
    }
    assert_eq!(f.file(), before, "nothing changed in the file");
    f.shutdown().await;
}

// ---- Ids survive saving and a new server ----

#[tokio::test(flavor = "multi_thread")]
async fn ids_from_an_earlier_query_name_the_same_elements_and_pages_after_a_new_server_opens_the_saved_file() {
    let f = Fixture::new();
    let (pages, _) = f.pages().await;
    let before = f.elements(0).await;
    let ship = f.id(0, "Ship").await;
    // The agent renames and moves a page and edits; the file is saved.
    f.ok(
        "run",
        json!({ "steps": [
            { "command": "page.rename", "params": { "page": pages[1]["id"], "name": "Backlog" } },
            { "command": "page.move", "params": { "page": pages[1]["id"], "to": 0 } },
            { "command": "element.setText", "on": [ship], "params": { "text": "Saved" } }
        ] }),
    )
    .await;
    let Fixture { kits, workspace } = f;
    kits.shutdown().await;
    let f = Fixture { kits: Fixture::server(workspace.path()), workspace };
    let (after, _) = f.pages().await;
    assert_eq!((after[0]["id"].clone(), after[0]["name"].clone()), (pages[1]["id"].clone(), json!("Backlog")), "renamed and moved, the same id");
    assert_eq!(after[1]["id"], pages[0]["id"]);
    let now = f.elements(1).await;
    assert_eq!(ids(&json!(now)), ids(&json!(before)), "every element keeps its id");
    assert_eq!(f.id(1, "Saved").await, ship);
    // The object model and the commands take the earlier ids; the receipt names them.
    let read = f.ok("batch", json!({ "ops": [], "load": [{ "target": ship, "fields": ["kind"] }] })).await;
    assert_eq!(read["loaded"][0]["id"], ship.as_str(), "{read}");
    let ran = f.ok("run", json!({ "target": { "page": 2 }, "steps": [style(&ship, "blue")] })).await;
    assert_eq!(ids(&ran["changes"]["changed"]), [ship], "{ran}");
    f.shutdown().await;
}

// ---- The guard between two agents on one server ----

#[tokio::test(flavor = "multi_thread")]
async fn another_agents_edit_of_another_element_is_reported_and_my_write_applies() {
    let f = Fixture::new();
    let (_, basis) = f.pages().await;
    let (mine, theirs) = (f.id(0, "Build").await, f.id(0, "Ship").await);
    f.by(OTHER, "run", json!({ "steps": [style(&theirs, "blue")] })).await;
    let ran = f.ok("run", json!({ "basis": basis, "steps": [style(&mine, "green")] })).await;
    assert_eq!(ids(&ran["others"]["changed"]), [theirs], "{ran}");
    assert_eq!(ran["others"]["by"], json!([OTHER]), "{ran}");
    assert_eq!(ids(&ran["changes"]["changed"]), [mine], "{ran}");
    f.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn another_agents_edit_or_deletion_of_my_target_refuses_my_write() {
    let f = Fixture::new();
    let (_, basis) = f.pages().await;
    let target = f.id(0, "Ship").await;
    // Edited by the other agent: refused as changed, and nothing changes.
    f.by(OTHER, "run", json!({ "steps": [style(&target, "blue")] })).await;
    let before = f.file();
    let refused = f.refused("run", json!({ "basis": basis, "steps": [style(&target, "green")] })).await;
    assert_conflict(&refused, &target, "changed");
    assert_eq!(ids(&refused["others"]["changed"]), std::slice::from_ref(&target), "{refused}");
    assert_eq!(f.file(), before, "nothing changed in the file");
    // Deleted by the other agent: refused as deleted (by the targets parameter as by `on`).
    let basis = revision(&refused);
    f.by(OTHER, "run", json!({ "steps": [{ "command": "element.delete", "params": { "ids": [target] } }] })).await;
    let gone = f.file();
    let refused =
        f.refused("run", json!({ "basis": basis, "steps": [{ "command": "element.setText", "params": { "id": target, "text": "Mine" } }] })).await;
    assert_conflict(&refused, &target, "deleted");
    assert_eq!(f.file(), gone, "nothing changed in the file");
    f.shutdown().await;
}
