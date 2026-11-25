//! `kition-board` served headless keeps the editor's state (the page it shows) out of what an agent
//! reads of the board, and sets it for one call and puts it back through the Kit Process Protocol's
//! `context.get`, `context.set` and `context.restore` (section 7a), so nothing an agent does
//! depends on or leaves behind the person's page (task card C4/C5/C6-board).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_board_kit::BoardDocument;
use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit::KitDocument;
use kition_kit_server::protocol::Origin;
use kition_kit_server::{KitBackend, OpenRequest};
use serde_json::{Map, Value, json};

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// The headless kit as the `kition-board` executable makes it.
fn kit() -> Headless<BoardDocument> {
    Headless::<BoardDocument>::with_replica_port(
        KitInfo::from_manifest(include_str!("../../../kit.json"), kition_board_kit::SANDBOXED, kition_board_kit::EDITOR_STATE).unwrap(),
    )
}

/// A folder holding `Plan.kboard` with the pages `Plan` (a shape `Start`), `Ideas` and `Later`,
/// `Plan` shown; the kit opens it.
fn open(name: &str) -> (Headless<BoardDocument>, String, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("kition-board-context-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let mut board = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    let first = board.model().pages()[0].id.as_str().to_owned();
    board.execute("page.rename", json!({ "page": first, "name": "Plan" })).unwrap();
    board.execute("element.add", json!({ "elements": [{ "kind": "shape", "position": [0, 0], "text": "Start" }] })).unwrap();
    for name in ["Ideas", "Later"] {
        board.execute("page.add", json!({ "name": name })).unwrap();
    }
    std::fs::write(root.join("Plan.kboard"), board.save(None).unwrap()).unwrap();
    let mut kit = kit();
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let id = kit.open(OpenRequest { file: &folder.file("Plan.kboard").unwrap(), read_only: false, create: None }).unwrap().id;
    (kit, id, root)
}

fn run(kit: &mut Headless<BoardDocument>, id: &str, command: &str, value: Value, origin: Origin) -> Value {
    kit.execute(id, command, params(value), origin).unwrap_or_else(|error| panic!("{command}: {error:?}")).result.unwrap_or(Value::Null)
}

/// The pages as the agent reads them: `(id, name)` in board order.
fn pages(kit: &mut Headless<BoardDocument>, id: &str) -> Vec<(String, String)> {
    let state = kit.describe(id).unwrap().state;
    state["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|page| (page["id"].as_str().unwrap().to_owned(), page["name"].as_str().unwrap().to_owned()))
        .collect()
}

fn page_id(kit: &mut Headless<BoardDocument>, id: &str, name: &str) -> String {
    pages(kit, id).into_iter().find(|(_, n)| n == name).unwrap().0
}

/// How many elements each page holds, by name.
fn counts(kit: &mut Headless<BoardDocument>, id: &str) -> Vec<(String, u64)> {
    let state = kit.describe(id).unwrap().state;
    state["pages"].as_array().unwrap().iter().map(|page| (page["name"].as_str().unwrap().to_owned(), page["elements"].as_u64().unwrap())).collect()
}

fn shape() -> Value {
    json!({ "elements": [{ "kind": "shape", "position": [400, 100], "text": "New" }] })
}

#[test]
fn what_an_agent_reads_of_the_board_leaves_out_the_page_the_editor_shows() {
    let (mut kit, id, root) = open("reads");
    let before = kit.describe(&id).unwrap().state;
    let ideas = page_id(&mut kit, &id, "Ideas");
    // The person goes to another page.
    run(&mut kit, &id, "page.activate", json!({ "page": ideas }), Origin::User);
    let moved = kit.describe(&id).unwrap().state;
    assert_eq!(moved, before, "the shown page changed what the agent reads of the board");
    let shown = Value::Object(moved);
    for key in kition_board_kit::EDITOR_STATE {
        assert!(shown.pointer(key).is_none(), "{key} is the editor's, not the board's: {shown}");
    }
    // Every page is listed with its id, name, element count and its area (the default view).
    let listed = shown["pages"].as_array().unwrap();
    assert_eq!(listed.iter().map(|page| page["name"].clone()).collect::<Vec<_>>(), [json!("Plan"), json!("Ideas"), json!("Later")]);
    assert!(listed.iter().all(|page| page["area"]["width"].as_u64() == Some(1920)), "{shown}");
    let _ = std::fs::remove_dir_all(root);
}

/// A run as `kition-kits` makes it over the Kit Process Protocol (section 7a): `context.get` before
/// the first step, `context.set` with each step's `at.part`, `context.restore` after the last.
/// Steps land on the pages they name; the person's page is as it was.
#[test]
fn a_run_sets_each_steps_page_and_puts_the_editor_back() {
    let (mut kit, id, root) = open("run");
    let saved = kit.context_get(&id).unwrap();
    let (ideas, later) = (page_id(&mut kit, &id, "Ideas"), page_id(&mut kit, &id, "Later"));
    kit.context_set(&id, &params(json!({ "part": { "page": ideas } }))).unwrap();
    run(&mut kit, &id, "element.add", shape(), Origin::Agent);
    run(&mut kit, &id, "element.add", shape(), Origin::Agent);
    // By number, as the agent's tools pass a run's `target`: Later is page 3.
    assert_eq!(pages(&mut kit, &id)[2].0, later);
    kit.context_set(&id, &params(json!({ "part": { "page": 3 } }))).unwrap();
    run(&mut kit, &id, "frame.add", json!({ "box": [0, 0, 300, 200], "title": "Q4" }), Origin::Agent);
    kit.context_restore(&id, &saved).unwrap();
    assert_eq!(kit.context_get(&id).unwrap(), saved, "the editor shows the person's page again");
    assert_eq!(counts(&mut kit, &id), [("Plan".to_owned(), 1), ("Ideas".to_owned(), 2), ("Later".to_owned(), 1)]);
    // Without a part, the shown page (the person's): nothing new landed on Ideas or Later.
    run(&mut kit, &id, "element.add", shape(), Origin::Agent);
    assert_eq!(counts(&mut kit, &id)[0], ("Plan".to_owned(), 2));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_context_the_board_lacks_is_invalid_params_and_says_where_ids_come_from() {
    let (mut kit, id, root) = open("refused");
    let saved = kit.context_get(&id).unwrap();
    let ideas = page_id(&mut kit, &id, "Ideas");
    for (given, says) in [
        (json!({ "part": { "page": "nowhere" } }), "no page `nowhere`"),
        (json!({ "part": { "page": 4 } }), "the board has 3 pages"),
        (json!({ "part": { "page": 0 } }), "counted from 1"),
        (json!({ "part": { "slide": 1 } }), "names no part of a board"),
        (json!({ "part": { "page": ideas }, "on": ["nothing"] }), "no element `nothing`"),
        (json!({ "part": "Ideas" }), "is not `{"),
    ] {
        let error = kit.context_set(&id, &params(given.clone())).unwrap_err();
        assert_eq!(error.code, kition_kit_server::ErrorReason::InvalidParams.code(), "{given}: {}", error.message);
        assert!(error.message.contains(says) || error.data.as_ref().is_some_and(|data| data.to_string().contains(says)), "{given}: {error:?}");
        assert_eq!(kit.context_get(&id).unwrap(), saved, "{given} changed the editor");
    }
    // What a board cannot set: a time, and a selection (it keeps none; `on` fills a command's
    // element parameter instead).
    let start = kit.describe(&id).unwrap().state["pages"][0]["id"].clone();
    let element = run(&mut kit, &id, "element.add", shape(), Origin::Agent)["created"][0].clone();
    for given in [json!({ "time": 1.5 }), json!({ "part": { "page": start }, "on": [element] })] {
        let error = kit.context_set(&id, &params(given.clone())).unwrap_err();
        assert_eq!(error.code, kition_kit_server::ErrorReason::CommandDisabled.code(), "{given}: {error:?}");
        assert_eq!(kit.context_get(&id).unwrap(), saved, "{given} changed the editor");
    }
    let _ = std::fs::remove_dir_all(root);
}

/// An agent's command acts where its parameters say and leaves the person's page as it was, also
/// when it fails; a navigation command keeps its effect, and a page the agent makes and shows is
/// where its next steps land when they name none.
#[test]
fn an_agents_command_given_its_page_moves_nothing() {
    let (mut kit, id, root) = open("named");
    let saved = kit.context_get(&id).unwrap();
    let ideas = page_id(&mut kit, &id, "Ideas");
    run(&mut kit, &id, "element.add", json!({ "page": ideas, "elements": shape()["elements"] }), Origin::Agent);
    assert_eq!(kit.context_get(&id).unwrap(), saved, "an agent's command given its page moved the editor");
    let failed = kit.execute(&id, "element.add", params(json!({ "page": "nowhere", "elements": shape()["elements"] })), Origin::Agent);
    assert!(failed.is_err(), "a page the board lacks is refused");
    assert_eq!(kit.context_get(&id).unwrap(), saved, "a failed command moved the editor");
    // Navigation keeps its effect; a page made and shown takes the steps that name none.
    let made = run(&mut kit, &id, "page.add", json!({ "name": "Fresh", "activate": true }), Origin::Agent)["page"].clone();
    assert_ne!(kit.context_get(&id).unwrap(), saved, "page.add with activate is the agent's move");
    run(&mut kit, &id, "element.add", shape(), Origin::Agent);
    assert_eq!(counts(&mut kit, &id).last().unwrap(), &("Fresh".to_owned(), 1), "create a page, then fill it");
    run(&mut kit, &id, "page.activate", json!({ "page": ideas }), Origin::Agent);
    run(&mut kit, &id, "element.add", shape(), Origin::Agent);
    assert_eq!(counts(&mut kit, &id)[1], ("Ideas".to_owned(), 2), "page.activate is the agent's move");
    assert!(made.is_string());
    kit.context_restore(&id, &saved).unwrap();
    assert_eq!(kit.context_get(&id).unwrap(), saved);
    let _ = std::fs::remove_dir_all(root);
}

/// `kition_kit::testing::check_context` with a context that really moves the editor: another
/// page.
#[test]
fn the_editor_state_hooks_pass_the_sdks_contract_check() {
    let mut board = BoardDocument::create("Plan.kboard", 1920, 1080).unwrap();
    let other = board.execute("page.add", json!({ "name": "Other" })).unwrap()["page"].clone();
    let context = kition_kit::CallContext { on: Vec::new(), part: Some(json!({ "page": other })), time: None };
    kition_kit::testing::check_context(&mut board, &context, kition_board_kit::EDITOR_STATE).unwrap();
    let before = board.editor_state();
    board.set_context(&context).unwrap();
    assert_ne!(board.editor_state(), before, "the context moved the editor");
    board.restore_editor_state(&before).unwrap();
    assert_eq!(board.editor_state(), before);
}

/// What the kit publishes for the agent: the editor state each command reads (`x-khp-reads`), the
/// parameter `on` fills (`x-khp-role`), and the view the render command draws.
#[test]
fn the_agents_commands_say_what_they_read_and_the_view_the_render_takes() {
    let (mut kit, id, root) = open("published");
    let commands = kit.agent_commands(Some(&id)).unwrap();
    let params = |command: &str| commands.iter().find(|listed| listed.id == command).map(|listed| listed.params.clone()).unwrap();
    assert_eq!(params("element.add")["x-khp-reads"], json!(["part"]));
    assert_eq!(params("file.export")["x-khp-reads"], json!(["part"]));
    assert!(params("element.style").get("x-khp-reads").is_none());
    assert_eq!(params("element.style")["properties"]["ids"]["x-khp-role"], "targets");
    assert_eq!(params("element.setText")["properties"]["id"]["x-khp-role"], "targets");
    assert_eq!(params("arrange.ungroup")["properties"]["groupIds"]["x-khp-role"], "targets");
    assert_eq!(params("mindmap.addChild")["properties"]["node"]["x-khp-role"], "targets");
    assert_eq!(params("layout.mindmap")["properties"]["root"]["x-khp-role"], "targets");
    assert_eq!(params("frame.add")["properties"]["children"]["x-khp-role"], "object", "the children resolve live Element UUIDs");
    assert!(params("file.render").get("x-khp-reads").is_none());
    let mut view = params("file.render")["properties"]["target"]["properties"].as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    view.sort();
    assert_eq!(view, ["area", "frame", "objectId", "page"]);
    let _ = std::fs::remove_dir_all(root);
}
