//! Helpers for the model's tests: boards built through transactions, replicas of one board, and
//! the change exchange between them.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_board_kdm::blobs::SharedBlobs;
use kition_board_kdm::{BoardModel, ELEMENT, PAGE, create_replica, root_id, schema};
use kition_kit::kdm::{ActorId, ObjectId, Origin, Replica, Tx};
use kition_kit::session::KitSession;
use serde_json::{Map, Value, json};

pub type Session = KitSession<BoardModel>;

pub fn person() -> Origin {
    Origin::person("person:test")
}

/// A new board titled `Plan` with one page.
pub fn board() -> Session {
    let replica = create_replica("Plan", None, ActorId::random(), &Origin::system("create")).unwrap();
    KitSession::from_replica(replica, Box::new(SharedBlobs::default())).unwrap()
}

/// Another replica of `session`'s document, from its snapshot, under a fresh actor.
pub fn fork(session: &Session) -> Session {
    let replica = Replica::load(schema(), &session.snapshot().unwrap(), ActorId::random()).unwrap();
    KitSession::from_replica(replica, Box::new(SharedBlobs::default())).unwrap()
}

/// Exchanges changes until both replicas have everything (repairs included).
pub fn sync(a: &mut Session, b: &mut Session) {
    for _ in 0..4 {
        let (va, vb) = (a.version(), b.version());
        let from_b = b.export_changes(&va).unwrap();
        let from_a = a.export_changes(&vb).unwrap();
        a.import_changes(&from_b).unwrap();
        b.import_changes(&from_a).unwrap();
        if a.version() == b.version() {
            return;
        }
    }
    panic!("the replicas did not settle");
}

/// Runs `f` as one person transaction.
pub fn write<R>(session: &mut Session, f: impl FnOnce(&mut Tx<'_>) -> kition_kit::session::Result<R>) -> R {
    session.execute(&person(), |_, tx| f(tx)).unwrap().0
}

pub fn first_page(session: &Session) -> ObjectId {
    session.model().pages()[0].id.clone()
}

pub fn fields(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// Appends an element with `fields` to `page`.
pub fn add(tx: &mut Tx<'_>, page: &ObjectId, element: Value) -> ObjectId {
    let id = tx.create(ELEMENT, fields(element)).unwrap();
    let at = tx.list_items(page, "elements").unwrap().len();
    tx.list_insert(page, "elements", at, &id).unwrap();
    id
}

pub fn shape(x: f64, y: f64, w: f64, h: f64) -> Value {
    json!({ "kind": "shape", "position": { "x": x, "y": y }, "size": { "w": w, "h": h } })
}

pub fn frame(x: f64, y: f64, w: f64, h: f64) -> Value {
    json!({ "kind": "frame", "position": { "x": x, "y": y }, "size": { "w": w, "h": h } })
}

/// A connector from `from` (bound at `anchor`) to a free point.
pub fn connector_from(from: &ObjectId, anchor: [f64; 2], to: [f64; 2]) -> Value {
    json!({
        "kind": "connector",
        "start": { "point": [0, 0], "binding": { "target": from.as_str(), "anchor": anchor } },
        "end": { "point": to },
    })
}

/// Adds a page after the others.
pub fn add_page(tx: &mut Tx<'_>, name: &str) -> ObjectId {
    let page = tx.create(PAGE, fields(json!({ "name": name }))).unwrap();
    let at = tx.list_items(&root_id(), "pages").unwrap().len();
    tx.list_insert(&root_id(), "pages", at, &page).unwrap();
    page
}

/// Removes `id` from `owner`'s list `field` and deletes it.
pub fn remove(tx: &mut Tx<'_>, owner: &ObjectId, field: &str, id: &ObjectId) {
    let index = tx.list_items(owner, field).unwrap().iter().position(|item| item == id).unwrap();
    tx.list_remove(owner, field, index).unwrap();
    tx.delete(id).unwrap();
}

pub fn state(session: &Session) -> Value {
    session.replica().state_json().unwrap()
}

/// The ids on a page's stored list.
pub fn listed(session: &Session, page: &ObjectId) -> Vec<ObjectId> {
    session.replica().list(page, "elements").unwrap()
}
